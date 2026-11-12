//! 最小 ESC 从站节点（B2b）：把一条 EtherCAT 请求帧变成应答帧。
//!
//! 组成：
//! - [`esc_core::Registers`]：4 KiB ESC 寄存器空间（识别/站地址/DL/AL）；
//! - [`esc_core::EscAl`]：AL Control/Status 状态机；
//! - [`esc_core::eeprom::EepromInterface`]：0x0500–0x050F EEPROM 接口（SII）；
//! - [`wire`]：帧/datagram 解析与校验、WKC 增量、自增地址规则。
//!
//! 应答规则（ETG.1000.4）：
//! - `BRD/BWR/BRW`：广播，命中本从站；
//! - `APRD/APWR/APRW`：收到 ADP == 0 的从站命中（见 [`wire::auto_increment_step`]）；
//! - `FPRD/FPWR/FPRW`：配置站地址或站别名匹配才命中；
//! - `LRD/LWR/LRW`：按已激活的 FMMU 命中（[`esc_core::fmmu::Fmmu::translate`]），在
//!   逻辑地址与 DL 用户内存（0x1000–0xFFFF）之间搬运字节；WKC 按两家主站一致的规则累加
//!   （LRD 读 +1、LWR 写 +1、LRW 读 +1 / 写 +2，见 [`Responder::process_logical`] 与
//!   `handle_frame` 中的注释）。
//! - **邮箱（B7 子项 1）**：FPRD/FPWR 到 DL 用户内存（≥ 0x1000）按物理地址读写；主站写
//!   SM0 邮箱缓冲（0x1000）即解析邮箱头，对 CoE SDO 请求在 SM1 缓冲（0x1080）回
//!   SDO Abort（0x06020000），并用 SM1 Status（0x080D）bit3 = mailbox full 通知主站；
//!   主站读走 SM1 缓冲后清除该位，见 [`esc_core::mailbox`] 与 [`Responder::handle_mailbox_request`]。
//!
//! 命中后按命令的读/写属性回填数据并累加 WKC；未命中的 datagram 原样保留（WKC 不变）。

use std::collections::BTreeMap;

use esc_core::eeprom::EepromInterface;
use esc_core::fmmu::{Fmmu, FMMU_MAX};
use esc_core::mailbox::{self, MailboxHeader, SdoRequest};
use esc_core::memory::DlUserMemory;
use esc_core::obj_dict;
use esc_core::sii::SiiImage;
use esc_core::sm::SyncManager;
use esc_core::{reg, watchdog, AlControl, EscAl, Registers};
use wire::{
    auto_increment_step, matches_configured_address, Command, Frame, ECAT_HEADER_LEN,
    ETHERNET_HEADER_LEN,
};

/// 本从站应答时使用的默认 MAC（TAP 接口 MAC 由 CLI 从 `/sys/class/net` 读取后覆盖）。
pub const DEFAULT_SIM_MAC: [u8; 6] = [0x02, 0xec, 0x51, 0x4d, 0x00, 0x02];

/// 按 DL Status（0x0110）的位布局拼出寄存器值。
///
/// 位布局见 ETG.1000.4 table 34（与两家主站的解析一致）：
/// - bit0：DL user operational（PDI 已连接且工作）；bit1：DL user watchdog（未超时）；
///   bit2：extended link detection 已激活；bit3：保留；
/// - bit(4+i)：端口 i 的**物理链路**（ESC 扫描时主站读它填 `ports[i].link.link_up`）；
/// - bit(8+2i)：端口 i 的 `loop_back`（链路断开时 ESC 自动把环回闭合）；
/// - bit(9+2i)：端口 i 的 `signal_detection`（该端口 RX 上检测到信号）。
///
/// 证据：IgH 1.6.13 `master/fsm_slave_scan.c: ec_fsm_slave_scan_state_datalink()`：
/// `link_up = dl_status & (1 << (4 + i))`、`loop_closed = dl_status & (1 << (8 + i * 2))`、
/// `signal_detected = dl_status & (1 << (9 + i * 2))`；
/// SOEM `src/ec_config.c: ecx_config_init()` 用
/// `(topology & (0x3 << (8 + 2i))) == (0x2 << (8 + 2i))` 判"端口开放且有通信"，
/// 与"闭合端口的 signal_detection 必须为 0"一致。
#[must_use]
pub const fn dl_status_value(
    link_up: [bool; 4],
    loop_closed: [bool; 4],
    signal_detected: [bool; 4],
) -> u16 {
    let mut value = 0u16;
    let mut port = 0usize;
    while port < 4 {
        if link_up[port] {
            value |= 1 << (4 + port);
        }
        if loop_closed[port] {
            value |= 1 << (8 + 2 * port);
        }
        if signal_detected[port] {
            value |= 1 << (9 + 2 * port);
        }
        port += 1;
    }
    value
}

/// 单从站（只有端口 0 连到主站）的 DL Status 初值：端口 0 链路 up 且有信号，
/// 端口 1–3 未接、环回闭合且无信号；bit0–3 全 0（本阶段不模拟 PDI，无 DL user）。
///
/// IgH `master/master.c: ec_master_calc_topology_rec()` 按端口 **3 → 1 → 2** 顺序检查
/// `loop_closed`（见 `next_table[EC_MAX_PORTS] = {3, 2, 0, 1}`）：未接端口必须报"闭合"，
/// 否则它认为该端口后还有从站、单从站时拓扑计算失败
/// （现象：`Failed to calculate bus topology`）。旧值 `0x503F` 把端口 1 报成"环回开放"，
/// 正是该报错的原因。
pub const DL_STATUS_INITIAL: u16 = dl_status_value(
    [true, false, false, false],
    [false, true, true, true],
    [true, false, false, false],
);

/// 应答统计（便于现场取证）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// 收到的 EtherCAT 帧数。
    pub frames_in: u64,
    /// 发出的应答帧数。
    pub frames_out: u64,
    /// 被忽略的非 EtherCAT / 非法帧数。
    pub frames_dropped: u64,
    /// 命中的 datagram 数。
    pub datagrams_hit: u64,
    /// 收到的邮箱请求数（写入 SM0 缓冲的邮箱报文）。
    pub mailbox_requests: u64,
    /// 已回 SDO 响应的邮箱响应数（expedited upload 或 Abort）。
    pub mailbox_responses: u64,
}

/// 单从站最小 ESC 应答器。
#[derive(Debug, Clone)]
pub struct Responder {
    regs: Registers,
    al: EscAl,
    eeprom: EepromInterface,
    memory: DlUserMemory,
    /// SDO Download 写入的配置对象覆盖值（键 = (index, subindex)），Upload 时优先返回。
    object_writes: BTreeMap<(u16, u8), Vec<u8>>,
    /// 帧驱动时钟的当前时间（毫秒，由 [`Responder::tick`] 更新）。
    now_ms: u64,
    /// 最近一次收到过程数据的时刻（毫秒）；`None` 表示尚未收到过。
    last_process_data_ms: Option<u64>,
    stats: Stats,
}

impl Default for Responder {
    fn default() -> Self {
        Self::new(SiiImage::simulated_slave())
    }
}

impl Responder {
    /// 用给定 SII 镜像构造，并把识别/DL/AL 寄存器置为可被主站扫描到的初值。
    #[must_use]
    pub fn new(sii: SiiImage) -> Self {
        let mut regs = Registers::new();
        regs.set_internal(reg::DL_STATUS, &DL_STATUS_INITIAL.to_le_bytes());
        let al = EscAl::default();
        regs.set_internal(reg::AL_STATUS, &[al.status_byte()]);
        regs.set_internal(reg::AL_STATUS_CODE, &al.status_code.to_le_bytes());
        Self {
            regs,
            al,
            eeprom: EepromInterface::new(sii),
            memory: DlUserMemory::new(),
            object_writes: BTreeMap::new(),
            now_ms: 0,
            last_process_data_ms: None,
            stats: Stats::default(),
        }
    }

    /// 统计快照。
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// 当前 AL 状态字节。
    #[must_use]
    pub fn al_status(&self) -> u8 {
        self.al.status_byte()
    }

    /// 当前配置站地址。
    #[must_use]
    pub fn station_address(&self) -> u16 {
        self.regs.read_u16(reg::STATION_ADDRESS).unwrap_or(0)
    }

    /// 帧驱动时钟 tick：由线缆层每收到一帧调用一次（TAP/UDS 均如此）。
    ///
    /// 检查 SM 过程数据看门狗：任一过程数据 SM（SM2/SM3）已激活、control bit6 = 1 且
    /// length > 0 时视为使能；超时（`0x0400 × 0x0420 × 40 ns`）则置 AL Status error 位、
    /// AL Status Code = `0x001B`，并把状态从 OP 退回 SAFEOP（等待主站 Error ACK）。
    ///
    /// ⚠️ 本模拟器没有独立硬件定时器：没有帧到达时时钟不前进（停止收帧不会主动报超时）。
    pub fn tick(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        if !self.watchdog_enabled() {
            self.last_process_data_ms = None;
            return;
        }
        let Some(last) = self.last_process_data_ms else {
            // 首次启用：从现在开始计时，避免启动即误报。
            self.last_process_data_ms = Some(now_ms);
            return;
        };
        let divider = self.regs.read_u16(watchdog::WATCHDOG_DIVIDER).unwrap_or(0);
        let intervals = self
            .regs
            .read_u16(watchdog::WATCHDOG_TIME_PROCESS_DATA)
            .unwrap_or(0);
        let timeout_ms = watchdog::timeout_ms(divider, intervals);
        if timeout_ms == 0 {
            // 0 值配置：真实 ESC 的语义待核实（⚠️），模拟器按"未配置超时"处理。
            return;
        }
        if now_ms.saturating_sub(last) >= timeout_ms {
            self.trigger_watchdog(now_ms);
        }
    }

    /// SM 过程数据看门狗是否使能：SM2/SM3 中任一"已激活 + control.bit6 + length > 0"。
    fn watchdog_enabled(&self) -> bool {
        (2..=3).any(|index| {
            SyncManager::read(&self.regs, index).is_some_and(|sm| {
                sm.activate & 0x01 != 0
                    && sm.control & watchdog::SM_CONTROL_WATCHDOG != 0
                    && sm.length > 0
            })
        })
    }

    /// 记录一次过程数据写入（FMMU 逻辑写或物理写命中 SM2/SM3 缓冲区），重置看门狗计时。
    fn note_process_data_write(&mut self, physical_address: u16, len: usize) {
        if len == 0 {
            return;
        }
        let write_end = u32::from(physical_address) + len as u32;
        for index in 2..=3u8 {
            let Some(sm) = SyncManager::read(&self.regs, index) else {
                continue;
            };
            if sm.length == 0 {
                continue;
            }
            let start = u32::from(sm.start_address);
            let end = start + u32::from(sm.length);
            if u32::from(physical_address) < end && start < write_end {
                self.last_process_data_ms = Some(self.now_ms);
                return;
            }
        }
    }

    /// 看门狗触发：AL Status error 位 + code 0x001B，OP → SAFEOP（等待 Error ACK）。
    fn trigger_watchdog(&mut self, now_ms: u64) {
        self.al.error = true;
        self.al.status_code = watchdog::AL_STATUS_SM_WATCHDOG;
        if self.al.state == esc_core::AlState::Op {
            self.al.state = esc_core::AlState::SafeOp;
        }
        self.regs
            .set_internal(reg::AL_STATUS, &[self.al.status_byte()]);
        self.regs
            .set_internal(reg::AL_STATUS_CODE, &self.al.status_code.to_le_bytes());
        // 避免每个 tick 重复触发；主站 ACK 后会重新开始计时。
        self.last_process_data_ms = Some(now_ms);
    }

    /// 处理一帧以太网请求，返回应发回的应答帧；非法/非 EtherCAT 帧返回 [`None`]。
    ///
    /// `own_mac` 用作应答帧源 MAC（真实主站不校验，但保持线缆语义正确）。
    pub fn handle_frame(&mut self, frame: &[u8], own_mac: [u8; 6]) -> Option<Vec<u8>> {
        self.stats.frames_in = self.stats.frames_in.saturating_add(1);
        if Frame::parse_ethernet(frame).is_err() && !Self::is_padded_ethercat_frame(frame) {
            self.stats.frames_dropped = self.stats.frames_dropped.saturating_add(1);
            return None;
        }

        let mut reply = frame.to_vec();
        // 交换 MAC：请求的源成为应答的目的，本从站 MAC 成为应答的源。
        reply[0..6].copy_from_slice(&frame[6..12]);
        reply[6..12].copy_from_slice(&own_mac);

        let mut offset = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN;
        loop {
            if offset + 12 > reply.len() {
                break;
            }
            let command = Command::from_u8(reply[offset]).expect("Frame::parse validated command");
            let adp = u16::from_le_bytes([reply[offset + 2], reply[offset + 3]]);
            let register_offset = u16::from_le_bytes([reply[offset + 4], reply[offset + 5]]);
            let len_flags = u16::from_le_bytes([reply[offset + 6], reply[offset + 7]]);
            let data_len = usize::from(len_flags & 0x07FF);
            let more = len_flags & 0x8000 != 0;
            let data_start = offset + 10;
            let wkc_offset = data_start + data_len;
            if wkc_offset + 2 > reply.len() {
                break;
            }

            let increment = if Self::is_logical(command) {
                // 逻辑寻址的 4 字节地址是 32 位逻辑地址（小端，跨 ADP/ADO 两个字段）。
                let logical_address = u32::from_le_bytes([
                    reply[offset + 2],
                    reply[offset + 3],
                    reply[offset + 4],
                    reply[offset + 5],
                ]);
                let (read_hit, write_hit) = self.process_logical(
                    command,
                    logical_address,
                    &mut reply[data_start..wkc_offset],
                );
                // 逻辑寻址的 WKC 规则（两家未修改主站的实现一致）：
                // LRW 的**写** FMMU 命中 +2、**读** FMMU 命中 +1，二者都命中 = 3；
                // 纯 LRD 读 +1、纯 LWR 写 +1。
                // 证据：IgH `master/datagram_pair.c` 注释 "If LRW is used, output FMMUs
                // increment the working counter by 2, while input FMMUs increment it by 1"，
                // 期望值 `used[OUTPUT] * 2 + used[INPUT]`；SOEM `src/ec_main.c` 对 LWR
                // 也乘 2（"output WKC counts 2 times when using LRW, emulate the same for LWR"）。
                let multiplier = if command == Command::Lrw { 2 } else { 1 };
                u16::from(read_hit) + u16::from(write_hit) * multiplier
            } else if self.matches(command, adp) {
                if command.is_read() {
                    let bytes = self.read_registers(register_offset, data_len);
                    reply[data_start..wkc_offset].copy_from_slice(&bytes);
                }
                if command.is_write() {
                    let data = reply[data_start..wkc_offset].to_vec();
                    self.write_registers(register_offset, &data);
                }
                command.wkc_increment()
            } else {
                0
            };

            if increment > 0 {
                self.stats.datagrams_hit = self.stats.datagrams_hit.saturating_add(1);
                let wkc = u16::from_le_bytes([reply[wkc_offset], reply[wkc_offset + 1]]);
                let updated = wkc.wrapping_add(increment);
                reply[wkc_offset..wkc_offset + 2].copy_from_slice(&updated.to_le_bytes());
            }

            offset = wkc_offset + 2;
            if !more {
                break;
            }
        }

        self.stats.frames_out = self.stats.frames_out.saturating_add(1);
        Some(reply)
    }

    /// 以太网最小帧为 60 字节（不含 FCS），IgH 主站会把短 EtherCAT 帧补零到 60 字节。
    /// 这类帧的帧头声明的长度小于实际帧长，但 datagram 结构本身完整有效，必须接受。
    fn is_padded_ethercat_frame(frame: &[u8]) -> bool {
        if frame.len() < ETHERNET_HEADER_LEN + ECAT_HEADER_LEN
            || wire::ethertype(frame) != Some(wire::ETHERCAT_ETHERTYPE)
        {
            return false;
        }
        let payload = &frame[ETHERNET_HEADER_LEN..];
        let declared = usize::from(u16::from_le_bytes([payload[0], payload[1]]) & 0x07FF);
        let Some(body) = payload.get(..ECAT_HEADER_LEN + declared) else {
            return false;
        };
        frame.len() > ETHERNET_HEADER_LEN + body.len() && Frame::parse(body).is_ok()
    }

    fn matches(&self, command: Command, adp: u16) -> bool {
        match command {
            Command::Nop => false,
            Command::Brd | Command::Bwr | Command::Brw => true,
            Command::Aprd | Command::Apwr | Command::Aprw | Command::Armw => {
                auto_increment_step(adp).process
            }
            Command::Fprd | Command::Fpwr | Command::Fprw | Command::Frmw => {
                let alias = self.regs.read_u16(reg::STATION_ALIAS).unwrap_or(0);
                matches_configured_address(adp, self.station_address(), alias)
            }
            // 逻辑寻址由 [`Self::process_logical`] 单独处理（按 FMMU 命中）。
            Command::Lrd | Command::Lwr | Command::Lrw => false,
        }
    }

    /// 是否为逻辑寻址命令（LRD/LWR/LRW）。
    const fn is_logical(command: Command) -> bool {
        matches!(command, Command::Lrd | Command::Lwr | Command::Lrw)
    }

    /// 处理一条逻辑寻址 datagram：按 FMMU 命中把数据搬进/搬出 DL 用户内存。
    ///
    /// 命中条件（[`Fmmu::translate`]）：FMMU 已激活、字节对齐、datagram 逻辑区间与
    /// FMMU 逻辑区间有交集，且物理侧落在 DL 用户内存内；读/写方向还必须分别满足
    /// FMMU 的 Type 位（bit0 = 可读、bit1 = 可写）。
    ///
    /// 返回 `(读命中, 写命中)`，WKC 由调用方累加：读命中 +1、写命中 +1；
    /// 对 **LRW**，写命中额外再 +1（写 +2、读 +1，见 `handle_frame` 中的证据注释）。
    fn process_logical(
        &mut self,
        command: Command,
        logical_address: u32,
        data: &mut [u8],
    ) -> (bool, bool) {
        let (mut read_hit, mut write_hit) = (false, false);
        if data.is_empty() {
            return (read_hit, write_hit);
        }
        for index in 0..FMMU_MAX {
            let Some(fmmu) = Fmmu::read(&self.regs, index) else {
                continue;
            };
            let Some(translation) = fmmu.translate(logical_address, data.len()) else {
                continue;
            };
            let start = translation.datagram_offset;
            let end = start + translation.len;
            if command.is_read() && fmmu.can_read() {
                if let Some(bytes) = self
                    .memory
                    .read(translation.physical_address, translation.len)
                {
                    data[start..end].copy_from_slice(&bytes);
                    read_hit = true;
                }
            }
            if command.is_write()
                && fmmu.can_write()
                && self
                    .memory
                    .write(translation.physical_address, &data[start..end])
            {
                write_hit = true;
                // 逻辑写命中过程数据 SM 缓冲区 → 重置看门狗。
                self.note_process_data_write(translation.physical_address, translation.len);
            }
        }
        (read_hit, write_hit)
    }

    /// 按物理地址读：EEPROM 窗口 / DL 用户内存（≥ 0x1000）/ 寄存器区。
    ///
    /// 读 SM1 邮箱缓冲（0x1080）表示主站取走响应，此时清 SM1 Status 的 mailbox full 位
    /// （模拟从站邮箱已被消费；真实 ESC 由 PDI/协议栈处理）。
    fn read_registers(&mut self, address: u16, len: usize) -> Vec<u8> {
        if EepromInterface::covers(address, len) {
            return self.eeprom.read(address, len);
        }
        if DlUserMemory::covers(address, len) {
            let bytes = self.memory.read(address, len).unwrap_or_default();
            if address == mailbox::MAILBOX_TX_OFFSET {
                mailbox::clear_mailbox_full(&mut self.regs);
            }
            return bytes;
        }
        let mut out = vec![0u8; len];
        for (i, byte) in out.iter_mut().enumerate() {
            let addr = address.wrapping_add(i as u16);
            if let Some(value) = self.regs.read_u8(addr) {
                *byte = value;
            }
        }
        out
    }

    /// 按物理地址写：EEPROM 窗口 / DL 用户内存（≥ 0x1000）/ 寄存器区。
    ///
    /// 写 SM0 邮箱缓冲（0x1000）视为一条邮箱报文，交给 [`Self::handle_mailbox_request`]。
    fn write_registers(&mut self, address: u16, data: &[u8]) {
        if EepromInterface::covers(address, data.len()) {
            self.eeprom.write(address, data);
            return;
        }
        if DlUserMemory::covers(address, data.len()) {
            self.memory.write(address, data);
            // 物理写命中 SM2/SM3 过程数据缓冲区 → 重置看门狗。
            self.note_process_data_write(address, data.len());
            if address == mailbox::MAILBOX_RX_OFFSET {
                self.handle_mailbox_request(data);
            }
            return;
        }
        let mut touched_al_control = false;
        for (i, byte) in data.iter().enumerate() {
            let addr = address.wrapping_add(i as u16);
            if self.regs.write_u8(addr, *byte) && addr == reg::AL_CONTROL {
                touched_al_control = true;
            }
        }
        if touched_al_control {
            self.sync_al_state();
        }
    }

    /// 处理主站写入 SM0 缓冲的一条邮箱报文（[`esc_core::mailbox`]）。
    ///
    /// **CoE SDO**：initiate upload 从对象字典（含 Download 覆盖值）回 expedited upload；
    /// initiate download 只接受 [`esc_core::obj_dict::is_writable`] 列出的配置对象
    /// （PDO assign/映射、SM 同步参数），保存后回成功响应；其余下载回 Abort
    /// （只读 0x06010002 / 对象不存在 0x06020000）。SDO Info 与分段传输留到后续。
    fn handle_mailbox_request(&mut self, message: &[u8]) {
        let Some(header) = MailboxHeader::parse(message) else {
            return;
        };
        self.stats.mailbox_requests = self.stats.mailbox_requests.saturating_add(1);
        if header.protocol() != mailbox::MAILBOX_TYPE_COE {
            return;
        }
        let available = message.len() - mailbox::MAILBOX_HEADER_LEN;
        let service_len = usize::from(header.length).min(available);
        let service =
            &message[mailbox::MAILBOX_HEADER_LEN..mailbox::MAILBOX_HEADER_LEN + service_len];
        let response_service = match mailbox::parse_sdo_request(service) {
            Some(SdoRequest::Upload { index, subindex }) => {
                match self.object_value(index, subindex) {
                    Some(value) => mailbox::sdo_upload_service(index, subindex, value),
                    None => mailbox::sdo_abort_payload(
                        index,
                        subindex,
                        mailbox::SDO_ABORT_OBJECT_NOT_EXIST,
                    ),
                }
            }
            Some(SdoRequest::Download {
                index,
                subindex,
                data,
            }) => {
                if obj_dict::is_writable(index) {
                    self.object_writes.insert((index, subindex), data.to_vec());
                    mailbox::sdo_download_ok_service(index, subindex)
                } else {
                    let abort_code = if obj_dict::read_object(index, subindex).is_some() {
                        mailbox::SDO_ABORT_READ_ONLY
                    } else {
                        mailbox::SDO_ABORT_OBJECT_NOT_EXIST
                    };
                    mailbox::sdo_abort_payload(index, subindex, abort_code)
                }
            }
            // master 发起的 Abort / 分段传输 / SDO Info：暂不应答。
            Some(SdoRequest::Other) | None => return,
        };
        let response_header = MailboxHeader {
            length: 0,
            address: header.address,
            priority: header.priority,
            mbox_type: mailbox::MAILBOX_TYPE_COE,
        };
        let Some(response) = mailbox::build_mailbox(response_header, &response_service) else {
            return;
        };
        // 与真实 ESC 一致：响应放在 SM1 缓冲，置 mailbox full 等主站取走。
        let _ = self.memory.write(mailbox::MAILBOX_TX_OFFSET, &response);
        mailbox::set_mailbox_full(&mut self.regs);
        self.stats.mailbox_responses = self.stats.mailbox_responses.saturating_add(1);
    }

    /// 查对象当前值：优先 Download 覆盖值，其次静态对象字典。
    fn object_value(&self, index: u16, subindex: u8) -> Option<&[u8]> {
        if let Some(value) = self.object_writes.get(&(index, subindex)) {
            return Some(value.as_slice());
        }
        obj_dict::read_object(index, subindex).map(|entry| entry.value)
    }

    fn sync_al_state(&mut self) {
        let control = self.regs.read_u8(reg::AL_CONTROL).unwrap_or(0);
        if control & 0x0F == 0 && control & 0x30 == 0 {
            // AL Control 全 0：主站没有请求状态变化（部分主站初始化时会写 0），忽略。
            return;
        }
        self.al.handle_control(AlControl::from_byte(control));
        self.regs
            .set_internal(reg::AL_STATUS, &[self.al.status_byte()]);
        self.regs
            .set_internal(reg::AL_STATUS_CODE, &self.al.status_code.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use esc_core::sii::{SIM_PRODUCT_CODE, SIM_VENDOR_ID};
    use wire::{encode_frame, DatagramData};

    fn request(datagrams: &[DatagramData]) -> Vec<u8> {
        let payload = encode_frame(datagrams).expect("encode");
        let mut frame = vec![0u8; ETHERNET_HEADER_LEN];
        frame[0..6].copy_from_slice(&[0xFF; 6]); // 广播目的
        frame[6..12].copy_from_slice(&[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        wire::set_ethertype(&mut frame, wire::ETHERCAT_ETHERTYPE);
        frame.extend_from_slice(&payload);
        frame
    }

    fn reply_datagram(reply: &[u8], data_start: usize, data_len: usize) -> (u16, Vec<u8>) {
        let wkc = u16::from_le_bytes([
            reply[data_start + data_len],
            reply[data_start + data_len + 1],
        ]);
        (wkc, reply[data_start..data_start + data_len].to_vec())
    }

    #[test]
    fn broadcast_read_of_al_status_is_answered_with_wkc_one() {
        let mut responder = Responder::default();
        let frame = request(&[DatagramData::auto_increment(
            Command::Brd,
            1,
            0,
            0x0130,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
        assert_eq!(&reply[0..6], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        assert_eq!(&reply[6..12], &DEFAULT_SIM_MAC);
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 1);
        assert_eq!(data[0] & 0x0F, 0x01, "slave starts in INIT");
        assert_eq!(responder.stats().frames_in, 1);
        assert_eq!(responder.stats().frames_out, 1);
    }

    #[test]
    fn auto_increment_address_zero_is_processed_but_one_is_not() {
        let mut responder = Responder::default();
        let hit = request(&[DatagramData::auto_increment(
            Command::Aprd,
            1,
            0,
            0x0000,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&hit, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 1);

        let miss = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0xFFFF,
            0x0000,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&miss, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 0, "ADP=0xFFFF targets the second slave");
    }

    #[test]
    fn station_address_write_then_configured_read_matches() {
        let mut responder = Responder::default();
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            0x0010,
            vec![0x01, 0x10],
        )]);
        let reply = responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 1);
        assert_eq!(responder.station_address(), 0x1001);

        let read = request(&[DatagramData::auto_increment(
            Command::Fprd,
            3,
            0x1001,
            0x0130,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 1);
        assert_eq!(data[0] & 0x0F, 0x01);
    }

    #[test]
    fn unknown_configured_address_is_ignored() {
        let mut responder = Responder::default();
        let read = request(&[DatagramData::auto_increment(
            Command::Fprd,
            1,
            0x1002,
            0x0130,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 0);
    }

    #[test]
    fn two_datagrams_in_one_frame_are_both_processed() {
        let mut responder = Responder::default();
        let frame = request(&[
            DatagramData::auto_increment(Command::Brd, 1, 0, 0x0000, vec![0u8; 2]),
            DatagramData::auto_increment(Command::Brd, 2, 0, 0x0130, vec![0u8; 2]),
        ]);
        let reply = responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
        let first = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN;
        let (wkc1, data1) = reply_datagram(&reply, first + 10, 2);
        assert_eq!(wkc1, 1);
        assert_eq!(data1[0], 0x01, "ESC type");
        let second = first + 12 + 2;
        let (wkc2, _) = reply_datagram(&reply, second + 10, 2);
        assert_eq!(wkc2, 1);
        assert_eq!(responder.stats().datagrams_hit, 2);
    }

    #[test]
    fn eeprom_read_through_register_interface_returns_sii() {
        let mut responder = Responder::default();
        // IgH 的 SII 读：FPRD 0x0502 起 10 字节前，先 FPWR 0x0502 4 字节命令。
        let mut responder_setup = Responder::default();
        let command = request(&[DatagramData::auto_increment(
            Command::Fpwr,
            1,
            0,
            0x0500,
            vec![0x00],
        )]);
        responder_setup.handle_frame(&command, DEFAULT_SIM_MAC);
        // 写站地址以便配置站地址寻址（这里直接走广播，简化）
        assert_eq!(responder_setup.station_address(), 0);
        // 用 ADP=0 的自增写命令（广播语义等价于对唯一从站下发）
        let write_cmd = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            0x0502,
            vec![0x80, 0x01, 0x08, 0x00],
        )]);
        responder.handle_frame(&write_cmd, DEFAULT_SIM_MAC);
        let read_cmd = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0,
            0x0502,
            vec![0u8; 10],
        )]);
        let reply = responder
            .handle_frame(&read_cmd, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 10);
        assert_eq!(wkc, 1);
        assert_eq!(u16::from_le_bytes([data[0], data[1]]) & 0x8000, 0);
        assert_eq!(
            u32::from_le_bytes([data[6], data[7], data[8], data[9]]),
            SIM_VENDOR_ID
        );
    }

    #[test]
    fn product_code_is_readable_from_eeprom() {
        let mut responder = Responder::default();
        let write_cmd = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            0x0502,
            vec![0x80, 0x01, 0x0A, 0x00],
        )]);
        responder.handle_frame(&write_cmd, DEFAULT_SIM_MAC);
        let read_cmd = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0,
            0x0502,
            vec![0u8; 10],
        )]);
        let reply = responder
            .handle_frame(&read_cmd, DEFAULT_SIM_MAC)
            .expect("reply");
        let (_, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 10);
        assert_eq!(
            u32::from_le_bytes([data[6], data[7], data[8], data[9]]),
            SIM_PRODUCT_CODE
        );
    }

    #[test]
    fn al_status_present_after_broadcast_read() {
        let mut responder = Responder::default();
        let frame = request(&[DatagramData::auto_increment(
            Command::Brd,
            1,
            0,
            0x0130,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
        let (_, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(data[0], responder.al_status());
    }

    #[test]
    fn ethernet_padded_frame_is_accepted() {
        // IgH 主站把短 EtherCAT 帧补零到以太网最小帧 60 字节，帧头长度小于实际长度。
        let mut responder = Responder::default();
        let mut frame = request(&[DatagramData::auto_increment(
            Command::Brd,
            1,
            0,
            0x0130,
            vec![0u8; 2],
        )]);
        frame.resize(60, 0x00);
        let reply = responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
        assert_eq!(reply.len(), 60);
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 1);
        assert_eq!(data[0] & 0x0F, 0x01);
        assert_eq!(responder.stats().frames_dropped, 0);
    }

    #[test]
    fn non_ethercat_frames_are_dropped() {
        let mut responder = Responder::default();
        let mut frame = vec![0u8; 64];
        wire::set_ethertype(&mut frame, 0x0800);
        assert!(responder.handle_frame(&frame, DEFAULT_SIM_MAC).is_none());
        assert_eq!(responder.stats().frames_dropped, 1);
    }

    #[test]
    fn dl_status_layout_matches_esc_sec_ii_fields() {
        // ETG.1000.4 table 34：bit0–3 DL user/保留，bit4–7 物理链路，
        // bit8–15 每端口两个位（loop_back, signal_detection）。
        assert_eq!(DL_STATUS_INITIAL, 0x5610);
        assert_eq!(
            DL_STATUS_INITIAL & 0x000F,
            0,
            "本阶段不模拟 PDI，DL user/watchdog/extended link 位应为 0"
        );
        for port in 0..4usize {
            assert_eq!(
                DL_STATUS_INITIAL >> (4 + port) & 1,
                u16::from(port == 0),
                "端口 {port} 链路位"
            );
            assert_eq!(
                DL_STATUS_INITIAL >> (8 + 2 * port) & 1,
                u16::from(port != 0),
                "端口 {port} loop_back 位"
            );
            assert_eq!(
                DL_STATUS_INITIAL >> (9 + 2 * port) & 1,
                u16::from(port == 0),
                "端口 {port} signal_detection 位"
            );
        }
    }

    #[test]
    fn ig_h_topology_rec_accepts_single_slave_dl_status() {
        // 复刻 IgH 1.6.13 `ec_master_calc_topology_rec()` 的单从站分支：
        // 按端口 3 → 1 → 2 检查，`loop_closed == 0` 表示该端口后还有从站；
        // 若此时环位置已到尾部则返回 -1，即内核日志 `Failed to calculate bus topology`。
        let dl = DL_STATUS_INITIAL;
        for port in [3usize, 1, 2] {
            let loop_closed = dl & (1 << (8 + 2 * port)) != 0;
            assert!(
                loop_closed,
                "单从站时端口 {port} 必须报环路闭合，否则 IgH 拓扑计算失败"
            );
        }
    }

    #[test]
    fn soem_topology_extraction_sees_end_of_line_single_slave() {
        // 复刻 SOEM `ec_config.c` 的拓扑提取：端口两位为 0b10 表示"端口开放且有通信"。
        let dl = DL_STATUS_INITIAL;
        let (mut topology, mut active_ports) = (0u32, 0u8);
        for port in 0..4usize {
            if (dl >> (8 + 2 * port)) & 0x3 == 0x2 {
                topology += 1;
                active_ports |= 1 << port;
            }
        }
        assert_eq!(topology, 1, "单链路 = end of line");
        assert_eq!(active_ports, 0x01, "只有端口 0 链路建立");
    }

    #[test]
    fn dl_status_register_is_readable_over_aprd() {
        let mut responder = Responder::default();
        let frame = request(&[DatagramData::auto_increment(
            Command::Aprd,
            1,
            0,
            0x0110,
            vec![0u8; 2],
        )]);
        let reply = responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
        assert_eq!(wkc, 1);
        assert_eq!(u16::from_le_bytes([data[0], data[1]]), DL_STATUS_INITIAL);
    }

    #[test]
    fn master_sync_manager_config_is_stored_but_status_is_read_only() {
        use esc_core::sm::SyncManager;

        // 主站按 8 字节块整块写 SM0（mailbox out）配置；Status/PDI Control 是只读字节。
        let mut responder = Responder::default();
        let master = SyncManager {
            start_address: 0x1000,
            length: 0x0080,
            control: 0x26, // mailbox + write + DLS-user event
            status: 0xFF,
            activate: 0x01,
            pdi_control: 0xFF,
        };
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            0x0800,
            master.to_bytes().to_vec(),
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");

        let read = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0,
            0x0800,
            vec![0u8; 8],
        )]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 1);
        let bytes: [u8; 8] = data[..].try_into().expect("8 bytes");
        let stored = SyncManager::from_bytes(&bytes);
        assert_eq!(stored.start_address, 0x1000);
        assert_eq!(stored.length, 0x0080);
        assert_eq!(stored.control, 0x26);
        assert_eq!(stored.activate, 0x01);
        assert_eq!(stored.status, 0, "Status 只读");
        assert_eq!(stored.pdi_control, 0, "PDI Control 只读");
        assert!(stored.is_mailbox());
    }

    #[test]
    fn master_fmmu_config_is_stored_including_type() {
        use esc_core::fmmu::Fmmu;

        // 主站按 16 字节块整块写 FMMU0（IgH fmmu_config.c 的字段顺序）；Type 是方向位，可写。
        let mut responder = Responder::default();
        let master = Fmmu {
            logical_start_address: 0x0000_1000,
            length: 0x0008,
            logical_start_bit: 0,
            logical_end_bit: 7,
            physical_start_address: 0x1200,
            physical_start_bit: 0,
            fmmu_type: 0x02,
            activate: 0x01,
        };
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            0x0600,
            master.to_bytes().to_vec(),
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");

        let read = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0,
            0x0600,
            vec![0u8; 16],
        )]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 16);
        assert_eq!(wkc, 1);
        let bytes: [u8; 16] = data[..].try_into().expect("16 bytes");
        let stored = Fmmu::from_bytes(&bytes);
        assert_eq!(stored.logical_start_address, 0x0000_1000);
        assert_eq!(stored.length, 0x0008);
        assert_eq!(stored.logical_end_bit, 7);
        assert_eq!(stored.physical_start_address, 0x1200);
        assert_eq!(stored.activate, 0x01);
        assert_eq!(stored.fmmu_type, 0x02, "Type 由主站配置");
        assert!(stored.can_write() && !stored.can_read());
        assert_eq!(&bytes[0x0D..0x10], &[0, 0, 0], "保留字节只读");
    }

    /// 构造一页 FMMU 寄存器内容（与 IgH `ec_fmmu_config_page()` 相同的字段值：
    /// 逻辑起始位 0、逻辑结束位 7、物理起始位 0）。
    fn fmmu_page(
        logical_start: u32,
        len: u16,
        physical_start: u16,
        fmmu_type: u8,
        activate: u8,
    ) -> Vec<u8> {
        esc_core::fmmu::Fmmu {
            logical_start_address: logical_start,
            length: len,
            logical_start_bit: 0,
            logical_end_bit: 7,
            physical_start_address: physical_start,
            physical_start_bit: 0,
            fmmu_type,
            activate,
        }
        .to_bytes()
        .to_vec()
    }

    /// 用 APWR 整块写第 `index` 个 FMMU（主站配置 FMMU 的真实做法）。
    fn configure_fmmu(responder: &mut Responder, index: u8, page: Vec<u8>) {
        let address = esc_core::fmmu::fmmu_address(index).expect("fmmu in range");
        let frame = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            address,
            page,
        )]);
        responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
    }

    fn logical_frame(command: Command, address: u32, data: Vec<u8>) -> Vec<u8> {
        request(&[DatagramData::logical(command, 1, address, data)])
    }

    #[test]
    fn logical_write_then_read_roundtrip_through_fmmus() {
        // 输出（写）FMMU0 与输入（读）FMMU1 映射同一物理区，模拟 IgH 的 PDO 方向配置。
        let mut responder = Responder::default();
        configure_fmmu(
            &mut responder,
            0,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x02, 0x01),
        );
        configure_fmmu(
            &mut responder,
            1,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x01, 0x01),
        );

        let payload = vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
        let write = logical_frame(Command::Lwr, 0x0000_1000, payload.clone());
        let reply = responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 1, "写命中 +1");

        let read = logical_frame(Command::Lrd, 0x0000_1000, vec![0u8; 8]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 1, "读命中 +1");
        assert_eq!(data, payload, "逻辑写进了 DL 用户内存，逻辑读同一物理区");
    }

    #[test]
    fn logical_datagram_misses_without_active_fmmu() {
        let mut responder = Responder::default();
        // 配置了 FMMU 但未激活：不得命中，WKC 不增。
        configure_fmmu(
            &mut responder,
            0,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x03, 0x00),
        );
        let write = logical_frame(Command::Lwr, 0x0000_1000, vec![1u8; 8]);
        let reply = responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 0, "未激活的 FMMU 不命中");

        // 逻辑地址不在任何 FMMU 区间内：同样不命中。
        let read = logical_frame(Command::Lrd, 0x0000_2000, vec![0u8; 8]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 0);
        // 只有配置 FMMU 的 APWR 命中过一次，两条逻辑 datagram 都没有命中。
        assert_eq!(responder.stats().datagrams_hit, 1);
    }

    #[test]
    fn lrd_requires_read_direction_bit() {
        // 只配置写方向 FMMU（IgH 的 output FMMU 写 Type=0x02）：LRD 不命中。
        let mut responder = Responder::default();
        configure_fmmu(
            &mut responder,
            0,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x02, 0x01),
        );
        let read = logical_frame(Command::Lrd, 0x0000_1000, vec![0u8; 8]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 0, "写方向 FMMU 不满足读命中");
    }

    #[test]
    fn lrw_increments_wkc_per_processed_direction() {
        let mut responder = Responder::default();
        // 只有写方向 FMMU：LRW 的写部分命中 → WKC 2（LRW 的写命中 +2）。
        configure_fmmu(
            &mut responder,
            0,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x02, 0x01),
        );
        let lrw = logical_frame(Command::Lrw, 0x0000_1000, vec![0xA5u8; 8]);
        let reply = responder
            .handle_frame(&lrw, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 2, "LRW 写方向命中 +2");

        // 再加读方向 FMMU：LRW 读写都命中 → WKC 3（写 +2、读 +1），且读到同一物理区。
        configure_fmmu(
            &mut responder,
            1,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x01, 0x01),
        );
        let lrw = logical_frame(Command::Lrw, 0x0000_1000, vec![0x5Au8; 8]);
        let reply = responder
            .handle_frame(&lrw, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 3, "LRW 读写都命中 = 写 2 + 读 1");
        assert_eq!(data, vec![0x5Au8; 8]);
    }

    #[test]
    fn logical_write_maps_only_the_intersection_with_fmmu_range() {
        // FMMU：逻辑 0x1000..0x1008 → 物理 0x1000。datagram 逻辑 0x0FFC 起 12 字节，
        // 只有后 8 字节落在 FMMU 区间内 → 写到物理 0x1000..0x1008。
        let mut responder = Responder::default();
        configure_fmmu(
            &mut responder,
            0,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x02, 0x01),
        );
        configure_fmmu(
            &mut responder,
            1,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x01, 0x01),
        );

        let payload: Vec<u8> = (0u8..12).map(|i| 0xA0 + i).collect();
        let write = logical_frame(Command::Lwr, 0x0000_0FFC, payload.clone());
        let reply = responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 12);
        assert_eq!(wkc, 1, "部分重叠仍算一次命中");

        let read = logical_frame(Command::Lrd, 0x0000_1000, vec![0u8; 8]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 8);
        assert_eq!(wkc, 1);
        assert_eq!(data, payload[4..].to_vec(), "只搬运交集的 8 字节");
    }

    #[test]
    fn logical_read_leaves_unmapped_bytes_untouched() {
        // datagram 逻辑 0x0FFC 起 12 字节，FMMU 只覆盖偏移 4..12；前 4 字节保持请求里的值。
        let mut responder = Responder::default();
        configure_fmmu(
            &mut responder,
            0,
            fmmu_page(0x0000_1000, 8, 0x1000, 0x01, 0x01),
        );
        let mut requested = vec![0xFFu8; 12];
        requested[4..].fill(0);
        let read = logical_frame(Command::Lrd, 0x0000_0FFC, requested);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 12);
        assert_eq!(wkc, 1);
        assert_eq!(&data[..4], &[0xFF; 4], "未映射字节不被覆盖");
        assert_eq!(&data[4..], &[0u8; 8], "DL 用户内存初值为 0");
    }

    /// 构造主站写入 SM0 邮箱缓冲的一条 128 字节报文（与 SOEM `ecx_mbxsend` 的
    /// `FPWR(mbx_wo, mbxl, ...)` 相同：整块 128 字节，头 6 字节 + CoE 服务数据）。
    fn mailbox_request_payload(
        mbox_type: u8,
        coe_command: u16,
        sdo_command: u8,
        index: u16,
        subindex: u8,
    ) -> Vec<u8> {
        let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
        payload[0..2].copy_from_slice(&10u16.to_le_bytes()); // 服务数据长度
        payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes()); // 站地址（IgH 风格）
        payload[4] = 0; // channel/priority
        payload[5] = mbox_type;
        payload[6..8].copy_from_slice(&(coe_command << 12).to_le_bytes());
        payload[8] = sdo_command;
        payload[9..11].copy_from_slice(&index.to_le_bytes());
        payload[11] = subindex;
        payload
    }

    fn read_tx_mailbox(responder: &mut Responder) -> Vec<u8> {
        let read = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0,
            mailbox::MAILBOX_TX_OFFSET,
            vec![0u8; mailbox::MAILBOX_SIZE],
        )]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(
            &reply,
            ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10,
            mailbox::MAILBOX_SIZE,
        );
        assert_eq!(wkc, 1);
        data
    }

    fn read_sm1_status(responder: &mut Responder) -> u8 {
        let read = request(&[DatagramData::auto_increment(
            Command::Aprd,
            3,
            0,
            mailbox::SM1_STATUS_ADDRESS,
            vec![0u8; 1],
        )]);
        let reply = responder
            .handle_frame(&read, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 1);
        assert_eq!(wkc, 1);
        data[0]
    }

    #[test]
    fn mailbox_sdo_upload_returns_dictionary_value() {
        // SOEM/IgH 发起 SDO 上传（0x1C12:01 = RxPDO assign）：应答 0x1600（2 字节 expedited）。
        let mut responder = Responder::default();
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            mailbox::COE_COMMAND_SDO_REQUEST,
            0x40, // initiate upload request
            0x1C12,
            0x01,
        );
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            mailbox::MAILBOX_RX_OFFSET,
            payload,
        )]);
        let reply = responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, _) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 128);
        assert_eq!(wkc, 1);

        // 主站轮询 SM1 Status：bit3 = mailbox full。
        assert!(
            mailbox::mailbox_full(read_sm1_status(&mut responder)),
            "响应就绪后 SM1 status bit3 必须置位"
        );

        // 读 SM1 缓冲：邮箱头 + CoE SDO upload response。
        let response = read_tx_mailbox(&mut responder);
        let header = MailboxHeader::parse(&response).expect("mailbox header");
        assert_eq!(header.protocol(), mailbox::MAILBOX_TYPE_COE);
        assert_eq!(header.length, 10);
        assert_eq!(
            u16::from_le_bytes([response[6], response[7]]),
            mailbox::COE_COMMAND_SDO_RESPONSE << 12,
            "CoE 命令 = SDO response"
        );
        assert_eq!(response[8], 0x4B, "2 字节对象 → expedited upload 0x4B");
        assert_eq!(u16::from_le_bytes([response[9], response[10]]), 0x1C12);
        assert_eq!(response[11], 0x01);
        assert_eq!(u16::from_le_bytes([response[12], response[13]]), 0x1600);

        // 主站取走数据后 mailbox full 清除。
        assert!(!mailbox::mailbox_full(read_sm1_status(&mut responder)));
        assert_eq!(responder.stats().mailbox_requests, 1);
        assert_eq!(responder.stats().mailbox_responses, 1);
    }

    #[test]
    fn mailbox_unknown_sdo_object_gets_abort() {
        // 0x2000:00 不在字典中：回 Abort 0x06020000。
        let mut responder = Responder::default();
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            mailbox::COE_COMMAND_SDO_REQUEST,
            0x40,
            0x2000,
            0x00,
        );
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            mailbox::MAILBOX_RX_OFFSET,
            payload,
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], mailbox::SDO_COMMAND_ABORT);
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            mailbox::SDO_ABORT_OBJECT_NOT_EXIST
        );
        assert_eq!(responder.stats().mailbox_responses, 1);
    }

    #[test]
    fn mailbox_non_coe_requests_are_not_answered() {
        let mut responder = Responder::default();
        // 非 CoE 协议类型（0x04 = FoE）：只记录收到，不应答。
        let payload = mailbox_request_payload(0x04, 0, 0, 0, 0);
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            mailbox::MAILBOX_RX_OFFSET,
            payload,
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        assert_eq!(responder.stats().mailbox_requests, 1);
        assert_eq!(responder.stats().mailbox_responses, 0);
        assert!(!mailbox::mailbox_full(read_sm1_status(&mut responder)));

        // CoE SDO Info（0x8）暂不实现：也不应答。
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            0x8, // SDO information
            0x01,
            0,
            0,
        );
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            mailbox::MAILBOX_RX_OFFSET,
            payload,
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        assert_eq!(responder.stats().mailbox_requests, 2);
        assert_eq!(responder.stats().mailbox_responses, 0);
        assert!(!mailbox::mailbox_full(read_sm1_status(&mut responder)));
    }

    #[test]
    fn too_short_mailbox_write_is_ignored() {
        let mut responder = Responder::default();
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            mailbox::MAILBOX_RX_OFFSET,
            vec![0u8; 4],
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
        assert_eq!(responder.stats().mailbox_requests, 0);
        assert_eq!(responder.stats().mailbox_responses, 0);
    }

    /// 构造主站执行 expedited SDO Download 的 128 字节邮箱报文（CI 配置时 IgH 会写映射对象）。
    fn mailbox_download_payload(index: u16, subindex: u8, data: &[u8]) -> Vec<u8> {
        let sdo_command = match data.len() {
            1 => 0x2F,
            2 => 0x2B,
            3 => 0x27,
            4 => 0x23,
            other => panic!("expedited download 只支持 1..=4 字节，得到 {other}"),
        };
        let service_len = 6 + data.len();
        let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
        payload[0..2].copy_from_slice(&(service_len as u16).to_le_bytes());
        payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
        payload[5] = mailbox::MAILBOX_TYPE_COE;
        payload[6..8].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
        payload[8] = sdo_command;
        payload[9..11].copy_from_slice(&index.to_le_bytes());
        payload[11] = subindex;
        payload[12..12 + data.len()].copy_from_slice(data);
        payload
    }

    fn write_mailbox(responder: &mut Responder, payload: Vec<u8>) {
        let write = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            mailbox::MAILBOX_RX_OFFSET,
            payload,
        )]);
        responder
            .handle_frame(&write, DEFAULT_SIM_MAC)
            .expect("reply");
    }

    #[test]
    fn mailbox_sdo_download_accepts_writable_object_and_upload_reads_it_back() {
        let mut responder = Responder::default();
        // 下载 0x1C12:01 = 0x1600（2 字节，命令 0x2B）→ 成功响应 0x60。
        write_mailbox(
            &mut responder,
            mailbox_download_payload(0x1C12, 0x01, &[0x00, 0x16]),
        );
        let response = read_tx_mailbox(&mut responder);
        let header = MailboxHeader::parse(&response).expect("mailbox header");
        assert_eq!(header.length, 6, "download 成功响应 = CoE 2 + 4 字节");
        assert_eq!(response[8], 0x60, "SDO download initiate response");
        assert_eq!(u16::from_le_bytes([response[9], response[10]]), 0x1C12);
        assert_eq!(response[11], 0x01);
        assert!(!mailbox::mailbox_full(read_sm1_status(&mut responder)));

        // 再上传同一对象：读到刚写入的值。
        write_mailbox(
            &mut responder,
            mailbox_request_payload(
                mailbox::MAILBOX_TYPE_COE,
                mailbox::COE_COMMAND_SDO_REQUEST,
                0x40,
                0x1C12,
                0x01,
            ),
        );
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], 0x4B, "2 字节 expedited upload");
        assert_eq!(u16::from_le_bytes([response[12], response[13]]), 0x1600);
    }

    #[test]
    fn mailbox_sdo_download_to_read_only_or_unknown_object_aborts() {
        let mut responder = Responder::default();
        // 0x1018:01 只读 → Abort 0x06010002。
        write_mailbox(
            &mut responder,
            mailbox_download_payload(0x1018, 0x01, &[0xC5, 0x0E, 0x00, 0x00]),
        );
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], mailbox::SDO_COMMAND_ABORT);
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            mailbox::SDO_ABORT_READ_ONLY
        );

        // 0x2000:00 不存在 → Abort 0x06020000。
        write_mailbox(
            &mut responder,
            mailbox_download_payload(0x2000, 0x00, &[0x00, 0x00]),
        );
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], mailbox::SDO_COMMAND_ABORT);
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            mailbox::SDO_ABORT_OBJECT_NOT_EXIST
        );
    }

    /// 通过 APWR 写寄存器块（测试辅助）。
    fn write_regs(responder: &mut Responder, address: u16, data: Vec<u8>) {
        let frame = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0,
            address,
            data,
        )]);
        responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
    }

    /// 通过 APRd 读寄存器块（测试辅助）。
    fn read_regs(responder: &mut Responder, address: u16, len: usize) -> Vec<u8> {
        let frame = request(&[DatagramData::auto_increment(
            Command::Aprd,
            2,
            0,
            address,
            vec![0u8; len],
        )]);
        let reply = responder
            .handle_frame(&frame, DEFAULT_SIM_MAC)
            .expect("reply");
        let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, len);
        assert_eq!(wkc, 1);
        data
    }

    fn al_status_byte(responder: &mut Responder) -> u8 {
        read_regs(responder, reg::AL_STATUS, 1)[0]
    }

    fn al_status_code(responder: &mut Responder) -> u16 {
        let bytes = read_regs(responder, reg::AL_STATUS_CODE, 2);
        u16::from_le_bytes([bytes[0], bytes[1]])
    }

    /// 配置 SM2（过程数据输出）与看门狗寄存器/使能位，并把 AL 推进到 OP。
    fn setup_watchdog_environment(responder: &mut Responder, watchdog_control: u8) {
        for control in [0x02u8, 0x04, 0x08] {
            write_regs(responder, reg::AL_CONTROL, vec![control]);
        }
        assert_eq!(al_status_byte(responder) & 0x0F, 0x08, "应进入 OP");
        // divider = 2500（40ns 单位）、intervals = 100 → 10 ms。
        write_regs(
            responder,
            watchdog::WATCHDOG_DIVIDER,
            2500u16.to_le_bytes().to_vec(),
        );
        write_regs(
            responder,
            watchdog::WATCHDOG_TIME_PROCESS_DATA,
            100u16.to_le_bytes().to_vec(),
        );
        let sync_manager = esc_core::sm::SyncManager {
            start_address: 0x1100,
            length: 8,
            control: watchdog_control, // bit6 = 看门狗使能；bit2 = 写方向
            status: 0,
            activate: 0x01,
            pdi_control: 0,
        };
        write_regs(responder, 0x0810, sync_manager.to_bytes().to_vec());
        configure_fmmu(responder, 0, fmmu_page(0x0000_1000, 8, 0x1100, 0x02, 0x01));
    }

    #[test]
    fn sm_watchdog_triggers_after_timeout_and_error_ack_clears() {
        let mut responder = Responder::default();
        setup_watchdog_environment(&mut responder, 0x44); // buffered + 写方向 + watchdog
        responder.tick(1_000); // 首次启用：开始计时
                               // 过程数据写（逻辑地址 0x1000 → 物理 0x1100）重置看门狗。
        let lwr = logical_frame(Command::Lwr, 0x0000_1000, vec![0xA5u8; 8]);
        responder
            .handle_frame(&lwr, DEFAULT_SIM_MAC)
            .expect("reply");
        responder.tick(1_005);
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x00, "5 ms < 10 ms");
        // 10 ms 未收到过程数据 → SAFEOP + ERROR，AL Status Code = 0x001B。
        responder.tick(1_010);
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x10, 0x10, "error 位必须置位");
        assert_eq!(status & 0x0F, 0x04, "OP 退回 SAFEOP");
        assert_eq!(
            al_status_code(&mut responder),
            watchdog::AL_STATUS_SM_WATCHDOG
        );
        // Error ACK 清错，状态保持 SAFEOP。
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x12]); // SAFEOP|ACK
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x10, 0x00);
        assert_eq!(status & 0x0F, 0x04, "ACK 后状态保持 SAFEOP");
    }

    #[test]
    fn sm_watchdog_disabled_without_control_bit6() {
        let mut responder = Responder::default();
        setup_watchdog_environment(&mut responder, 0x04); // 无 watchdog 位
        responder.tick(1_000);
        responder.tick(10_000); // 远超 10 ms
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x10, 0x00, "未使能看门狗不得置 error 位");
        assert_eq!(al_status_code(&mut responder), 0);
    }

    #[test]
    fn sm_watchdog_is_reset_by_process_data() {
        let mut responder = Responder::default();
        setup_watchdog_environment(&mut responder, 0x44);
        responder.tick(1_000);
        // t=1008 写过程数据 → 重新计时。
        responder.tick(1_008);
        let lwr = logical_frame(Command::Lwr, 0x0000_1000, vec![0x5Au8; 8]);
        responder
            .handle_frame(&lwr, DEFAULT_SIM_MAC)
            .expect("reply");
        responder.tick(1_015); // 距上次写 7 ms < 10 ms
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x00);
        responder.tick(1_019); // 11 ms → 触发
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x10);
    }
}
