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
//!   （LRD 读 +1、LWR 写 +1、LRW 读 +1 / 写 +2，见 `Responder::process_logical` 与
//!   `handle_frame` 中的注释）。
//! - **邮箱（B7 子项 1）**：FPRD/FPWR 到 DL 用户内存（≥ 0x1000）按物理地址读写；主站写
//!   SM0 邮箱缓冲（0x1000）即解析邮箱头，对 CoE SDO 请求在 SM1 缓冲（0x1080）回
//!   SDO Abort（0x06020000），并用 SM1 Status（0x080D）bit3 = mailbox full 通知主站；
//!   主站读走 SM1 缓冲后清除该位，见 [`esc_core::mailbox`] 与 `Responder::handle_mailbox_request`。
//!
//! 命中后按命令的读/写属性回填数据并累加 WKC；未命中的 datagram 原样保留（WKC 不变）。

use std::collections::BTreeMap;

pub mod cia402;

use esc_core::eeprom::EepromInterface;
use esc_core::fmmu::{Fmmu, FMMU_MAX};
use esc_core::mailbox::{self, MailboxHeader, SdoInfoRequest, SdoRequest};
use esc_core::memory::DlUserMemory;
use esc_core::obj_dict::ObjectDictionary;
use esc_core::sii::SiiImage;
use esc_core::sm::SyncManager;
use esc_core::{reg, watchdog, AlControl, EscAl, EsmPreconditions, Registers};
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
    /// 读取 DC 寄存器块（0x0900–0x09AF）的次数。
    pub dc_reg_reads: u64,
    /// 写入 DC 寄存器块（0x0900–0x09AF）的次数。
    pub dc_reg_writes: u64,
}

/// 进行中的分段 SDO 上传（值 > 4 字节时使用，CiA 301 segmented upload）。
#[derive(Debug, Clone)]
struct SegmentedUpload {
    /// 对象索引。
    index: u16,
    /// 子索引。
    subindex: u8,
    /// 完整数据。
    data: Vec<u8>,
    /// 已经发到主站的字节数（首帧之后的偏移）。
    offset: usize,
    /// 下一段请求期望的 toggle 位（0/1；CiA 301 要求主站每段交替）。
    toggle: u8,
    /// 最近一次发出的 segment 响应（toggle + 服务数据），用于主站重发上一段时幂等重发。
    last_response: Option<(u8, Vec<u8>)>,
}

/// 进行中的分段 SDO 下载（值 > 4 字节时使用，CiA 301 segmented download）。
#[derive(Debug, Clone)]
struct SegmentedDownload {
    /// 对象索引。
    index: u16,
    /// 子索引。
    subindex: u8,
    /// 数据总长度。
    total_size: u32,
    /// 下一段请求期望的 toggle 位（0/1；CiA 301 要求主站每段交替）。
    toggle: u8,
    /// 已收到的数据。
    data: Vec<u8>,
    /// 是否为 Complete Access 下载（完成时按子索引长度拆分写入）。
    complete_access: bool,
    /// 最近一次已处理的 segment toggle（用于识别主站重发上一段并幂等应答）。
    last_toggle: Option<u8>,
}

/// CiA 402 状态机行为：每个帧周期读 SM2 的控制字，写 SM3 的状态字（Tier0 骨架）。
#[derive(Debug, Clone)]
struct Cia402Behavior {
    /// 状态机。
    machine: crate::cia402::Cia402StateMachine,
    /// SM2（RxPDO）在 DL 用户内存中的起始地址；前 2 字节为控制字 `0x6040`。
    out_offset: u16,
    /// SM2 长度（>=7 时按完整 RxPDO 解析：控制字 + 模式 + 目标位置）。
    out_len: u16,
    /// SM3（TxPDO）在 DL 用户内存中的起始地址；前 2 字节为状态字 `0x6041`。
    in_offset: u16,
    /// SM3 长度（>=7 时按完整 TxPDO 写入：状态字 + 模式显示 + 实际位置）。
    in_len: u16,
    /// Tier0 简化：Operation enabled 时实际位置直接跟随目标位置（无运动环）。
    position_actual: i32,
    /// 模式显示 `0x6061`（镜像最近一次 RxPDO 中的模式 `0x6060`）。
    modes_display: i8,
    /// 速度实际值 `0x606C`（CSV 模式且 enabled 时镜像目标速度，否则 0）。
    velocity_actual: i32,
    /// 转矩实际值 `0x6077`（CST 模式且 enabled 时镜像目标转矩，否则 0）。
    torque_actual: i16,
}

/// 模拟 DC 系统时间每帧前进的步长（ns）。Tier0：帧驱动、无真实硬件时钟/SYNC0 脉冲。
pub const DC_TICK_NS: u64 = 1_000;
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
    /// 模拟 DC 系统时间（ns，每 tick 前进 [`DC_TICK_NS`]；由 0x0900–0x0917 读出）。
    dc_time_ns: u64,
    /// 最近一次收到过程数据的时刻（毫秒）；`None` 表示尚未收到过。
    last_process_data_ms: Option<u64>,
    /// 是否启用 ESM 状态迁移前置条件检查（默认**开启**，见
    /// [`Responder::with_esm_preconditions`]）。
    esm_preconditions: bool,
    /// 运行时对象字典（内置画像或声明式 manifest 生成，见 [`Responder::with_dictionary`]）。
    dict: ObjectDictionary,
    /// 进行中的分段 SDO 上传（值 > 4 字节的对象）。
    upload: Option<SegmentedUpload>,
    /// 进行中的分段 SDO 下载（值 > 4 字节的对象）。
    download: Option<SegmentedDownload>,
    /// 可选的 CiA 402 状态机行为（SM2 控制字 → SM3 状态字），见 [Responder::with_cia402_behavior]。
    cia402: Option<Cia402Behavior>,
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
            dc_time_ns: 0,
            last_process_data_ms: None,
            esm_preconditions: true,
            dict: ObjectDictionary::simulated(),
            upload: None,
            download: None,
            cia402: None,
            stats: Stats::default(),
        }
    }

    /// 替换运行时对象字典（声明式 manifest 生成的画像用），返回自身以便链式构造。
    #[must_use]
    pub fn with_dictionary(mut self, dict: ObjectDictionary) -> Self {
        self.dict = dict;
        self
    }

    /// 当前运行时对象字典。
    #[must_use]
    pub fn dictionary(&self) -> &ObjectDictionary {
        &self.dict
    }

    /// 进行中的分段 SDO 上传状态（无传输时为 `None`）。
    #[must_use]
    pub fn segmented_upload_in_progress(&self) -> bool {
        self.upload.is_some()
    }

    /// 设置是否启用 ESM 状态迁移前置条件检查，返回自身以便链式构造。
    ///
    /// 启用后（`enabled = true`，**默认**），从站在进入 **PREOP** 前要求邮箱 SM
    /// （SM0/SM1）已配置、进入 **SAFEOP** 前要求过程数据 SM（SM2/SM3）已配置，否则置
    /// AL Status error 位并给出 AL Status Code **0x0015 / 0x0017**（ETG.1000.6 Table 11）。
    /// "已配置" = Activate bit0 置位且 Length > 0；是否需要检查由 SII 声明决定
    /// （未声明邮箱/过程数据 SM 的从站不检查）。
    ///
    /// `enabled = false`（CLI `--esm-lax`）关闭检查，供需要"来者不拒"的兼容场景
    /// 或负向对照测试使用。默认开启的依据：两家未修改主站（IgH/SOEM）都在请求状态前
    /// 配置好对应 SM，2026-11-12 班在 Linux 验证机上以严格模式跑过 6/6 双主站回归全绿
    /// （见 WORKLOG 2026-11-12 20:44 班）。
    #[must_use]
    pub fn with_esm_preconditions(mut self, enabled: bool) -> Self {
        self.esm_preconditions = enabled;
        self
    }

    /// 当前是否启用 ESM 前置条件检查。
    #[must_use]
    pub fn esm_preconditions_enabled(&self) -> bool {
        self.esm_preconditions
    }

    /// 启用 CiA 402 状态机行为（Tier0）：每帧从 out_offset 读控制字 `0x6040`，
    /// 推进状态机，并把状态字 `0x6041` 写到 in_offset（各 2 字节，小端）。
    ///
    /// 偏移应取 manifest 的 SM2（dir out）/SM3（dir in）起始地址；行为在
    /// [Responder::tick] 中执行（每收到一帧前），因此主站读到的是**上一帧**写入
    /// 控制字对应的状态字（一帧延迟，Tier0 简化）。
    #[must_use]
    pub fn with_cia402_behavior(
        mut self,
        out_offset: u16,
        out_len: u16,
        in_offset: u16,
        in_len: u16,
    ) -> Self {
        self.cia402 = Some(Cia402Behavior {
            machine: crate::cia402::Cia402StateMachine::new(),
            out_offset,
            out_len,
            in_offset,
            in_len,
            position_actual: 0,
            modes_display: 0,
            velocity_actual: 0,
            torque_actual: 0,
        });
        self
    }

    /// 注入 CiA 402 故障（Tier0：测试/上层用）：状态机进入 Fault，并设置
    /// `0x603F` 错误码（通过 SDO 可读回）；故障复位（控制字 bit7）只清状态机，
    /// 错误码保留到上层再次写入（⚠️ 与真机的错误码清除时序不同）。
    pub fn inject_cia402_fault(&mut self, error_code: u16) {
        if let Some(behavior) = self.cia402.as_mut() {
            behavior.machine.inject_fault();
            self.object_writes
                .insert((0x603F, 0u8), error_code.to_le_bytes().to_vec());
        }
    }

    /// 当前 CiA 402 状态（未启用行为时返回 None）。
    #[must_use]
    pub fn cia402_state(&self) -> Option<crate::cia402::Cia402State> {
        self.cia402
            .as_ref()
            .map(|behavior| behavior.machine.state())
    }

    /// 执行 CiA 402 行为（如果启用）：读 SM2（控制字/模式/目标位置）→ 状态机 →
    /// 写 SM3（状态字/模式显示/实际位置）。
    ///
    /// Tier0 简化：Operation enabled 时实际位置直接跟随目标位置（无运动环/限幅），
    /// 并置状态字 bit10 Target reached；SM 长度不足 7 字节时只处理/写状态字 2 字节。
    fn step_cia402_behavior(&mut self) {
        use crate::cia402::{statusword::TARGET_REACHED, Cia402State};

        let Responder { cia402, memory, .. } = self;
        let Some(behavior) = cia402.as_mut() else {
            return;
        };
        let out_len = usize::from(behavior.out_len.max(2));
        let Some(out_bytes) = memory.read(behavior.out_offset, out_len) else {
            return;
        };
        let controlword = u16::from_le_bytes([out_bytes[0], out_bytes[1]]);
        let mut modes = behavior.modes_display;
        let mut target = behavior.position_actual;
        let mut target_velocity = behavior.velocity_actual;
        let mut target_torque = behavior.torque_actual;
        if out_bytes.len() == 5 {
            // 5 字节布局（CST）：控制字(2) + 模式(1) + 目标转矩 i16(2)。
            modes = out_bytes[2] as i8;
            target_torque = i16::from_le_bytes([out_bytes[3], out_bytes[4]]);
        } else if out_bytes.len() >= 7 {
            modes = out_bytes[2] as i8;
            target = i32::from_le_bytes([out_bytes[3], out_bytes[4], out_bytes[5], out_bytes[6]]);
        }
        if out_bytes.len() >= 11 {
            target_velocity =
                i32::from_le_bytes([out_bytes[7], out_bytes[8], out_bytes[9], out_bytes[10]]);
        }
        let mut statusword = behavior.machine.update(controlword);
        let enabled = behavior.machine.state() == Cia402State::OperationEnabled;
        // Tier0：
        // - CSP（模式 8）：Operation enabled 时实际位置跟随目标位置；
        // - CSV（模式 9，需 11 字节 PDO）：Operation enabled 时位置每个周期累加目标速度
        //   （模拟器把"速度目标"简化成每 tick 的增量，非真实单位/轨迹）；
        // - 其它模式：位置冻结。
        let csp = modes == 8;
        let csv = modes == 9 && out_bytes.len() >= 11;
        let cst = modes == 10 && out_bytes.len() == 5;
        if enabled && csp {
            behavior.position_actual = target;
        } else if enabled && csv {
            behavior.position_actual = behavior.position_actual.saturating_add(target_velocity);
        }
        let velocity_actual = if enabled && csv { target_velocity } else { 0 };
        behavior.velocity_actual = velocity_actual;
        // CST（模式 10，5 字节 PDO）：Tier0 只镜像目标转矩为实际转矩（不模拟运动）。
        let torque_actual = if enabled && cst { target_torque } else { 0 };
        behavior.torque_actual = torque_actual;
        if out_bytes.len() >= 7
            && usize::from(behavior.in_len) >= 7
            && enabled
            && csp
            && behavior.position_actual == target
        {
            statusword |= TARGET_REACHED;
        }
        behavior.modes_display = modes;
        if usize::from(behavior.in_len) == 5 {
            let mut tx = [0u8; 5];
            tx[0..2].copy_from_slice(&statusword.to_le_bytes());
            tx[2] = modes as u8;
            tx[3..5].copy_from_slice(&torque_actual.to_le_bytes());
            memory.write(behavior.in_offset, &tx);
        } else if usize::from(behavior.in_len) >= 11 {
            let mut tx = [0u8; 11];
            tx[0..2].copy_from_slice(&statusword.to_le_bytes());
            tx[2] = modes as u8;
            tx[3..7].copy_from_slice(&behavior.position_actual.to_le_bytes());
            tx[7..11].copy_from_slice(&velocity_actual.to_le_bytes());
            memory.write(behavior.in_offset, &tx);
        } else if usize::from(behavior.in_len) >= 7 {
            let mut tx = [0u8; 7];
            tx[0..2].copy_from_slice(&statusword.to_le_bytes());
            tx[2] = modes as u8;
            tx[3..7].copy_from_slice(&behavior.position_actual.to_le_bytes());
            memory.write(behavior.in_offset, &tx);
        } else {
            memory.write(behavior.in_offset, &statusword.to_le_bytes());
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
        // 模拟 DC 系统时间：每帧前进固定步长（Tier0：无真实时钟/SYNC0 脉冲）。
        self.dc_time_ns = self.dc_time_ns.wrapping_add(DC_TICK_NS);
        // Tier0 行为：每帧推进一次 CiA 402 状态机（SM2 控制字 → SM3 状态字）。
        self.step_cia402_behavior();
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
        // DC 时间寄存器（0x0900–0x0917）由模拟时钟动态回填：
        // 0x0900–0x090F 为各端口接收时间、0x0910–0x0917 为系统时间（Tier0 简化：同值）。
        if (0x0900..=0x09AF).contains(&address)
            || (0x0900..=0x09AF).contains(&address.wrapping_add(len as u16))
        {
            self.stats.dc_reg_reads += 1;
        }
        let dc_bytes = self.dc_time_ns.to_le_bytes();
        for (i, byte) in out.iter_mut().enumerate() {
            let addr = address.wrapping_add(i as u16);
            if (0x0900..=0x0917).contains(&addr) {
                *byte = dc_bytes[usize::from(addr - 0x0900) % 8];
            }
        }
        out
    }

    /// 按物理地址写：EEPROM 窗口 / DL 用户内存（≥ 0x1000）/ 寄存器区。
    ///
    /// 写 SM0 邮箱缓冲（0x1000）视为一条邮箱报文，交给 [`Self::handle_mailbox_request`]。
    fn write_registers(&mut self, address: u16, data: &[u8]) {
        if (0x0900..=0x09AF).contains(&address) {
            self.stats.dc_reg_writes += 1;
        }
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
        let response_service = if self.download.is_some() {
            // 进行中的分段下载：本帧的 SDO 请求按 download segment request 解析。
            match mailbox::parse_segment_download(service) {
                Some(segment) => self.handle_download_segment(segment),
                None => {
                    mailbox::sdo_abort_payload(0x0000, 0x00, mailbox::SDO_ABORT_PROTOCOL_TIMEOUT)
                }
            }
        } else if let Some(initiate) = mailbox::parse_initiate_download(service) {
            self.handle_download_initiate(initiate)
        } else if let Some(segment_request) = mailbox::parse_upload_segment_request(service) {
            // SDO 分段上传的后续段请求（值 > 4 字节的对象，CiA 301）。
            self.upload_segment_response(segment_request)
                .unwrap_or_else(|| {
                    mailbox::sdo_abort_payload(
                        segment_request.index,
                        segment_request.subindex,
                        mailbox::SDO_ABORT_PROTOCOL_TIMEOUT,
                    )
                })
        } else if let Some(ca_index) = mailbox::parse_complete_access_upload(service) {
            // Complete Access 上传（合成主站；IgH 1.6.13 的公共 API 不下发 CA 位）。
            match self.build_complete_access_upload(ca_index) {
                Some(service) => service,
                None => {
                    mailbox::sdo_abort_payload(ca_index, 0x00, mailbox::SDO_ABORT_OBJECT_NOT_EXIST)
                }
            }
        } else if let Some((ca_index, ca_size, ca_chunk)) =
            mailbox::parse_complete_access_download_initiate(service)
        {
            // Complete Access 下载（首帧/分段；完成时按子索引长度拆分后逐项写）。
            self.handle_download_complete_access(ca_index, ca_size, ca_chunk)
        } else if let Some(info_request) = mailbox::parse_sdo_info_request(service) {
            // CoE SDO Information 服务（`ethercat sdos` / SOEM `slaveinfo -sdo`）。
            match info_request {
                SdoInfoRequest::OdList { .. } => {
                    mailbox::build_sdo_info_od_list(&self.dict.object_indices())
                }
                SdoInfoRequest::ObjectDescription { index } => match self.dict.object(index) {
                    Some(description) => mailbox::build_sdo_info_object_description(
                        description.index,
                        description.data_type,
                        description.max_subindex,
                        description.object_code,
                        &description.name,
                    ),
                    None => mailbox::build_sdo_info_error(mailbox::SDO_ABORT_OBJECT_NOT_EXIST),
                },
                SdoInfoRequest::EntryDescription {
                    index, subindex, ..
                } => match self.dict.entry(index, subindex) {
                    Some(entry) => mailbox::build_sdo_info_entry_description(
                        entry.index,
                        entry.subindex,
                        entry.value_info,
                        entry.data_type,
                        entry.bit_length,
                        entry.access,
                        &entry.name,
                    ),
                    None => mailbox::build_sdo_info_error(mailbox::SDO_ABORT_OBJECT_NOT_EXIST),
                },
            }
        } else {
            match mailbox::parse_sdo_request(service) {
                Some(SdoRequest::Upload { index, subindex }) => {
                    match self.build_upload_response(index, subindex) {
                        Some(service) => service,
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
                    if self
                        .dict
                        .entry(index, subindex)
                        .is_some_and(|entry| entry.writable)
                    {
                        self.object_writes.insert((index, subindex), data.to_vec());
                        mailbox::sdo_download_ok_service(index, subindex)
                    } else {
                        let abort_code = if self.dict.entry(index, subindex).is_some() {
                            mailbox::SDO_ABORT_READ_ONLY
                        } else {
                            mailbox::SDO_ABORT_OBJECT_NOT_EXIST
                        };
                        mailbox::sdo_abort_payload(index, subindex, abort_code)
                    }
                }
                // master 发起的 Abort / 分段传输 / SDO Info：暂不应答。
                Some(SdoRequest::Other) | None => return,
            }
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

    /// 处理分段下载的 initiate 请求（normal、size indicated）。
    fn handle_download_initiate(&mut self, initiate: mailbox::InitiateDownload) -> Vec<u8> {
        if !self
            .dict
            .entry(initiate.index, initiate.subindex)
            .is_some_and(|entry| entry.writable)
        {
            let abort_code = if self.dict.entry(initiate.index, initiate.subindex).is_some() {
                mailbox::SDO_ABORT_READ_ONLY
            } else {
                mailbox::SDO_ABORT_OBJECT_NOT_EXIST
            };
            return mailbox::sdo_abort_payload(initiate.index, initiate.subindex, abort_code);
        }
        let total = usize::try_from(initiate.total_size).unwrap_or(usize::MAX);
        let mut data = initiate.data;
        if data.len() > total {
            return mailbox::sdo_abort_payload(
                initiate.index,
                initiate.subindex,
                mailbox::SDO_ABORT_LENGTH_MISMATCH,
            );
        }
        data.truncate(total);
        if data.len() >= total {
            self.object_writes
                .insert((initiate.index, initiate.subindex), data);
            self.download = None;
        } else {
            self.download = Some(SegmentedDownload {
                index: initiate.index,
                subindex: initiate.subindex,
                total_size: initiate.total_size,
                complete_access: false,
                data,
                toggle: 0,
                last_toggle: None,
            });
        }
        mailbox::sdo_download_ok_service(initiate.index, initiate.subindex)
    }

    /// 处理分段下载的 segment 请求（响应 `0x20|toggle<<4`；末段把数据写入对象）。
    fn handle_download_segment(&mut self, segment: mailbox::SegmentDownload) -> Vec<u8> {
        let Some(mut state) = self.download.take() else {
            return mailbox::sdo_abort_payload(0x0000, 0x00, mailbox::SDO_ABORT_PROTOCOL_TIMEOUT);
        };
        if segment.toggle != state.toggle {
            // 主站重发上一段（响应丢失）：幂等应答，不重复追加数据。
            if state.last_toggle == Some(segment.toggle) {
                self.download = Some(state);
                return mailbox::sdo_download_segment_ok_service(segment.toggle);
            }
            return mailbox::sdo_abort_payload(
                state.index,
                state.subindex,
                mailbox::SDO_ABORT_TOGGLE,
            );
        }
        let total = usize::try_from(state.total_size).unwrap_or(usize::MAX);
        if state.data.len() + segment.data.len() > total {
            return mailbox::sdo_abort_payload(
                state.index,
                state.subindex,
                mailbox::SDO_ABORT_LENGTH_MISMATCH,
            );
        }
        state.data.extend_from_slice(&segment.data);
        if segment.last || state.data.len() >= total {
            let response = if state.complete_access {
                // CA：先按子索引拆分写入；成功仍回 **segment response**（CiA 301 对末段
                // 的应答格式与中间段一致），失败则回 Abort。
                let abort = self.write_complete_access(state.index, &state.data);
                if abort.get(2).copied() == Some(mailbox::SDO_COMMAND_ABORT) {
                    abort
                } else {
                    mailbox::sdo_download_segment_ok_service(segment.toggle)
                }
            } else {
                self.object_writes
                    .insert((state.index, state.subindex), state.data);
                mailbox::sdo_download_segment_ok_service(segment.toggle)
            };
            self.download = None;
            return response;
        } else {
            state.toggle ^= 1;
            state.last_toggle = Some(segment.toggle);
            self.download = Some(state);
        }
        mailbox::sdo_download_segment_ok_service(segment.toggle)
    }

    /// 查对象当前值：优先 Download 覆盖值，其次运行时对象字典。
    fn object_value(&self, index: u16, subindex: u8) -> Option<&[u8]> {
        if let Some(value) = self.object_writes.get(&(index, subindex)) {
            return Some(value.as_slice());
        }
        self.dict.value(index, subindex)
    }

    /// 构造 SDO 上传响应：≤4 字节用 expedited，否则用 segmented upload 首帧。
    fn build_upload_response(&mut self, index: u16, subindex: u8) -> Option<Vec<u8>> {
        let value = self.object_value(index, subindex)?.to_vec();
        self.upload = None;
        if value.len() <= 4 {
            return Some(mailbox::sdo_upload_service(index, subindex, &value));
        }
        // 首帧容量 = 邮箱容量 - 邮箱头 - CoE/命令/索引/子索引/长度字段。
        let first_capacity = mailbox::MAILBOX_SIZE - mailbox::MAILBOX_HEADER_LEN - 10;
        let first = value.len().min(first_capacity);
        let service =
            mailbox::sdo_upload_initiate_service(index, subindex, value.len(), &value[..first]);
        if first < value.len() {
            self.upload = Some(SegmentedUpload {
                index,
                subindex,
                data: value,
                offset: first,
                toggle: 0,
                last_response: None,
            });
        }
        Some(service)
    }

    /// 处理 **Complete Access 下载**：把单帧数据按各子索引的字节长度拆分，逐项写入
    /// object_writes（要求每一项都可写；未知对象/只读对象回对应 Abort；数据长度不符回
    /// 0x05040000）。首帧未携带完整数据时转入分段下载；完成后再拆分写入。
    fn handle_download_complete_access(
        &mut self,
        index: u16,
        total_size: u32,
        chunk: &[u8],
    ) -> Vec<u8> {
        let total = usize::try_from(total_size).unwrap_or(usize::MAX);
        let mut data = chunk.to_vec();
        data.truncate(total);
        if data.len() >= total {
            return self.write_complete_access(index, &data);
        }
        self.download = Some(SegmentedDownload {
            index,
            subindex: 0,
            total_size,
            complete_access: true,
            data,
            toggle: 0,
            last_toggle: None,
        });
        mailbox::sdo_download_ok_service(index, 0)
    }

    /// 把完整的 CA 数据按对象子索引长度拆分写入 `object_writes`。
    fn write_complete_access(&mut self, index: u16, data: &[u8]) -> Vec<u8> {
        let Some(object) = self.dict.object(index) else {
            return mailbox::sdo_abort_payload(index, 0x00, mailbox::SDO_ABORT_OBJECT_NOT_EXIST);
        };
        // 第一遍：只校验（所有子索引存在、可写、长度和与数据一致），避免"部分写后 Abort"。
        let mut total = 0usize;
        for sub in 0..=object.max_subindex {
            let Some(entry) = self.dict.entry(index, sub) else {
                return mailbox::sdo_abort_payload(
                    index,
                    0x00,
                    mailbox::SDO_ABORT_OBJECT_NOT_EXIST,
                );
            };
            let len = usize::from(entry.bit_length.saturating_add(7)) / 8;
            if len == 0 || total + len > data.len() {
                return mailbox::sdo_abort_payload(
                    index,
                    0x00,
                    mailbox::SDO_ABORT_PROTOCOL_TIMEOUT,
                );
            }
            if !entry.writable {
                return mailbox::sdo_abort_payload(index, sub, mailbox::SDO_ABORT_READ_ONLY);
            }
            total += len;
        }
        if total != data.len() {
            return mailbox::sdo_abort_payload(index, 0x00, mailbox::SDO_ABORT_PROTOCOL_TIMEOUT);
        }
        // 第二遍：全部校验通过后才写入。
        let mut offset = 0usize;
        for sub in 0..=object.max_subindex {
            let Some(entry) = self.dict.entry(index, sub) else {
                return mailbox::sdo_abort_payload(
                    index,
                    0x00,
                    mailbox::SDO_ABORT_OBJECT_NOT_EXIST,
                );
            };
            let len = usize::from(entry.bit_length.saturating_add(7)) / 8;
            self.object_writes
                .insert((index, sub), data[offset..offset + len].to_vec());
            offset += len;
        }
        mailbox::sdo_download_ok_service(index, 0)
    }

    /// 构造 **Complete Access 上传**响应：把对象从子索引 0 到最大子索引的值按顺序拼接
    /// （每个子索引按其自身长度，**不做对齐填充**）；> 首段容量时走分段上传。
    fn build_complete_access_upload(&mut self, index: u16) -> Option<Vec<u8>> {
        let object = self.dict.object(index)?;
        let max_subindex = object.max_subindex;
        let mut data = Vec::new();
        for sub in 0..=max_subindex {
            // 用 object_value 以便读到 Download/CA 写入的覆盖值。
            let value = self.object_value(index, sub)?.to_vec();
            data.extend_from_slice(&value);
        }
        self.upload = None;
        let first_capacity = mailbox::MAILBOX_SIZE - mailbox::MAILBOX_HEADER_LEN - 10;
        let first = data.len().min(first_capacity);
        let service = mailbox::sdo_upload_initiate_service(index, 0, data.len(), &data[..first]);
        if first < data.len() {
            self.upload = Some(SegmentedUpload {
                index,
                subindex: 0,
                data,
                offset: first,
                toggle: 0,
                last_response: None,
            });
        }
        Some(service)
    }

    /// 处理 SDO 分段上传的段请求（`Command = 0x60 | toggle<<4`）。
    fn upload_segment_response(
        &mut self,
        request: mailbox::UploadSegmentRequest,
    ) -> Option<Vec<u8>> {
        let mut state = self.upload.take()?;
        if state.index != request.index || state.subindex != request.subindex {
            return None;
        }
        if request.toggle != state.toggle {
            // 主站重发上一段（响应丢失）：幂等重发相同响应，不推进状态。
            if let Some((last_toggle, last_service)) = &state.last_response {
                if *last_toggle == request.toggle {
                    let response = last_service.clone();
                    self.upload = Some(state);
                    return Some(response);
                }
            }
            return Some(mailbox::sdo_abort_payload(
                state.index,
                state.subindex,
                mailbox::SDO_ABORT_TOGGLE,
            ));
        }
        let take = (state.data.len() - state.offset).min(mailbox::SDO_SEGMENT_DATA_LEN);
        let chunk = state.data[state.offset..state.offset + take].to_vec();
        let last = state.offset + take >= state.data.len();
        let service = mailbox::sdo_upload_segment_service(request.toggle, &chunk, last);
        if !last {
            state.offset += take;
            state.toggle ^= 1;
            state.last_response = Some((request.toggle, service.clone()));
            self.upload = Some(state);
        }
        Some(service)
    }

    fn sync_al_state(&mut self) {
        let control = self.regs.read_u8(reg::AL_CONTROL).unwrap_or(0);
        if control & 0x0F == 0 && control & 0x30 == 0 {
            // AL Control 全 0：主站没有请求状态变化（部分主站初始化时会写 0），忽略。
            return;
        }
        let parsed = AlControl::from_byte(control);
        if self.esm_preconditions {
            let preconditions = self.esm_preconditions();
            self.al.handle_control_checked(parsed, &preconditions);
        } else {
            self.al.handle_control(parsed);
        }
        self.regs
            .set_internal(reg::AL_STATUS, &[self.al.status_byte()]);
        self.regs
            .set_internal(reg::AL_STATUS_CODE, &self.al.status_code.to_le_bytes());
    }

    /// 由当前 SII 声明与 SM 寄存器推导 ESM 前置条件（见 [`Responder::with_esm_preconditions`]）。
    fn esm_preconditions(&self) -> EsmPreconditions {
        let sii = self.eeprom.image();
        let outputs_declared = sii
            .category_bytes(esc_core::sii::category::RX_PDO)
            .is_some();
        let outputs_configured = Self::sync_manager_configured(&self.regs, 2);
        let inputs_declared = sii
            .category_bytes(esc_core::sii::category::TX_PDO)
            .is_some();
        let inputs_configured = Self::sync_manager_configured(&self.regs, 3);
        // 映射一致性只在"SM 长度与 SII 声明一致"时检查：合成主站测试可能写入不同的
        // SM 长度（非真实主站行为），此时无法判断映射是否有效，按"不检查"处理。
        let sm2_declared = Self::sii_sync_manager_length(sii, 2);
        let sm3_declared = Self::sii_sync_manager_length(sii, 3);
        let sm2_length = SyncManager::read(&self.regs, 2).map(|sm| sm.length);
        let sm3_length = SyncManager::read(&self.regs, 3).map(|sm| sm.length);
        let check_outputs =
            outputs_configured && sm2_declared.is_some() && sm2_length == sm2_declared;
        let check_inputs =
            inputs_configured && sm3_declared.is_some() && sm3_length == sm3_declared;
        let outputs_mapping_valid = !outputs_declared
            || !outputs_configured
            || !check_outputs
            || self.process_data_mapping_valid(0x1C12, 2);
        let inputs_mapping_valid = !inputs_declared
            || !inputs_configured
            || !check_inputs
            || self.process_data_mapping_valid(0x1C13, 3);
        EsmPreconditions {
            mailbox_declared: sii.mailbox_protocols() != 0,
            mailbox_out_configured: Self::sync_manager_configured(&self.regs, 0),
            mailbox_in_configured: Self::sync_manager_configured(&self.regs, 1),
            sync_managers_valid: Self::active_sm_layout_valid(&self.regs),
            process_data_declared: sii.declares_process_data_sync_managers(),
            process_data_configured: (2..=3)
                .any(|index| Self::sync_manager_configured(&self.regs, index)),
            outputs_declared,
            outputs_configured,
            inputs_declared,
            inputs_configured,
            outputs_mapping_valid,
            inputs_mapping_valid,
        }
    }

    /// 已激活的 SM 布局是否合法：地址落在 DL 内存范围（0x1000–0xFFFF）内、区间互不重叠。
    ///
    /// 只有 Activate bit0 置位且 Length > 0 的 SM 参与检查（未配置的 SM 不参与）。
    fn active_sm_layout_valid(regs: &Registers) -> bool {
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for index in 0..=3u8 {
            let Some(sm) = SyncManager::read(regs, index) else {
                return false;
            };
            if sm.activate & 0x01 == 0 || sm.length == 0 {
                continue;
            }
            let start = u32::from(sm.start_address);
            let end = start + u32::from(sm.length);
            if start < 0x1000 || end > 0x1_0000 {
                return false;
            }
            ranges.push((start, end));
        }
        ranges.sort_unstable();
        ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0)
    }

    /// 从 SII 的 SM 类别读取第 `index` 个 SM 的声明长度（用于映射一致性检查的有效性判断）。
    fn sii_sync_manager_length(sii: &SiiImage, index: usize) -> Option<u16> {
        let bytes = sii.category_bytes(esc_core::sii::category::SYNC_MANAGER)?;
        let offset = index * 8;
        if bytes.len() < offset + 4 {
            return None;
        }
        Some(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]))
    }

    /// 判断过程数据的分配 + 映射是否**有效**（比只比总位长更细的检查）。
    ///
    /// 规则（Tier0，基于 `0x1C12`/`0x1C13` 分配与 PDO 映射对象内容）：
    /// - 分配对象存在、条目数 > 0；每个分配的 PDO 索引非 0 且**不重复**；
    /// - 每个 PDO 映射对象存在且条目数 > 0；每个映射字的对象索引非 0、位长非 0；
    /// - 所有映射位长之和 = 对应 SM 的 `length × 8`。
    fn process_data_mapping_valid(&self, assign_index: u16, sm_index: u8) -> bool {
        let Some(sm) = SyncManager::read(&self.regs, sm_index) else {
            return false;
        };
        let Some(count) = self
            .dict
            .value(assign_index, 0)
            .and_then(|bytes| bytes.first())
        else {
            return false;
        };
        let mut total_bits = 0u32;
        let mut seen: Vec<u16> = Vec::new();
        for sub in 1..=*count {
            let Some(pdo_bytes) = self.dict.value(assign_index, sub) else {
                return false;
            };
            if pdo_bytes.len() < 2 {
                return false;
            }
            let pdo = u16::from_le_bytes([pdo_bytes[0], pdo_bytes[1]]);
            if pdo == 0 || seen.contains(&pdo) {
                return false;
            }
            seen.push(pdo);
            let Some(pdo_count) = self.dict.value(pdo, 0).and_then(|bytes| bytes.first()) else {
                return false;
            };
            if *pdo_count == 0 {
                return false;
            }
            for entry_sub in 1..=*pdo_count {
                let Some(word) = self.dict.value(pdo, entry_sub) else {
                    return false;
                };
                if word.len() < 4 {
                    return false;
                }
                let packed = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
                let index = (packed >> 16) as u16;
                let bits = packed & 0x00FF;
                if index == 0 || bits == 0 {
                    return false;
                }
                total_bits += bits;
            }
        }
        total_bits == u32::from(sm.length) * 8
    }

    /// SM 是否已配置：Activate bit0 置位且 Length > 0（与 IgH/SOEM 写入的 SM 页一致）。
    fn sync_manager_configured(regs: &Registers, index: u8) -> bool {
        SyncManager::read(regs, index).is_some_and(|sm| sm.activate & 0x01 != 0 && sm.length > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use esc_core::sii::{SIM_PRODUCT_CODE, SIM_SYNC_MANAGERS, SIM_VENDOR_ID};
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
    fn cia402_behavior_drives_statusword_from_controlword() {
        use crate::cia402::Cia402State;

        // 用合成主站同样的方式：APWR 写 SM2 控制字、APRD 读 SM3 状态字。
        fn write_controlword(responder: &mut Responder, value: u16) {
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0,
                0x1100,
                value.to_le_bytes().to_vec(),
            )]);
            responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APWR reply");
        }
        fn read_statusword(responder: &mut Responder) -> u16 {
            let frame = request(&[DatagramData::auto_increment(
                Command::Aprd,
                1,
                0,
                0x1110,
                vec![0u8; 2],
            )]);
            let reply = responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APRD reply");
            let (wkc, data) = reply_datagram(&reply, ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10, 2);
            assert_eq!(wkc, 1);
            u16::from_le_bytes([data[0], data[1]])
        }

        let mut responder = Responder::default().with_cia402_behavior(0x1100, 2, 0x1110, 2);
        assert_eq!(
            responder.cia402_state(),
            Some(Cia402State::SwitchOnDisabled)
        );

        write_controlword(&mut responder, 0x0000);
        responder.tick(1);
        assert_eq!(read_statusword(&mut responder), 0x0240);

        write_controlword(&mut responder, 0x0006);
        responder.tick(2);
        assert_eq!(read_statusword(&mut responder), 0x0221);
        assert_eq!(responder.cia402_state(), Some(Cia402State::ReadyToSwitchOn));

        write_controlword(&mut responder, 0x0007);
        responder.tick(3);
        assert_eq!(read_statusword(&mut responder), 0x0223);

        write_controlword(&mut responder, 0x000F);
        responder.tick(4);
        assert_eq!(read_statusword(&mut responder), 0x0237);
        assert_eq!(
            responder.cia402_state(),
            Some(Cia402State::OperationEnabled)
        );

        write_controlword(&mut responder, 0x0000);
        responder.tick(5);
        assert_eq!(read_statusword(&mut responder), 0x0240);
    }

    #[test]
    fn cia402_behavior_drives_full_txpdo_when_sms_are_long_enough() {
        fn write_rx(responder: &mut Responder, controlword: u16, mode: i8, target: i32) {
            let mut data = Vec::with_capacity(7);
            data.extend_from_slice(&controlword.to_le_bytes());
            data.push(mode as u8);
            data.extend_from_slice(&target.to_le_bytes());
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0,
                0x1100,
                data,
            )]);
            responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APWR reply");
        }
        fn read_tx(responder: &mut Responder) -> (u16, i8, i32) {
            let frame = request(&[DatagramData::auto_increment(
                Command::Aprd,
                1,
                0,
                0x1110,
                vec![0u8; 7],
            )]);
            let reply = responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APRD reply");
            let start = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10;
            let statusword = u16::from_le_bytes([reply[start], reply[start + 1]]);
            let modes_display = reply[start + 2] as i8;
            let position = i32::from_le_bytes([
                reply[start + 3],
                reply[start + 4],
                reply[start + 5],
                reply[start + 6],
            ]);
            (statusword, modes_display, position)
        }

        let mut responder = Responder::default().with_cia402_behavior(0x1100, 7, 0x1110, 7);
        write_rx(&mut responder, 0x0000, 8, 1000);
        responder.tick(1);
        assert_eq!(read_tx(&mut responder), (0x0240, 8, 0), "初始位置为 0");

        write_rx(&mut responder, 0x0006, 8, 1000);
        responder.tick(2);
        assert_eq!(read_tx(&mut responder), (0x0221, 8, 0));

        write_rx(&mut responder, 0x0007, 8, 1000);
        responder.tick(3);
        assert_eq!(read_tx(&mut responder), (0x0223, 8, 0));

        // Operation enabled + CSP 模式 8 + 目标 1000：位置跟随并置 Target reached(bit10)。
        write_rx(&mut responder, 0x000F, 8, 1000);
        responder.tick(4);
        assert_eq!(
            read_tx(&mut responder),
            (0x0237 | 0x0400, 8, 1000),
            "完整 TxPDO：状态字 + 模式显示 + 实际位置"
        );
        assert_eq!(
            responder.cia402_state(),
            Some(crate::cia402::Cia402State::OperationEnabled)
        );

        // Disable voltage：位置保持最后值。
        write_rx(&mut responder, 0x0000, 8, 1000);
        responder.tick(5);
        assert_eq!(read_tx(&mut responder), (0x0240, 8, 1000));
    }

    #[test]
    fn cia402_behavior_tracks_position_only_in_csp_mode() {
        fn write_rx(responder: &mut Responder, controlword: u16, mode: i8, target: i32) {
            let mut data = vec![0u8; 7];
            data[0..2].copy_from_slice(&controlword.to_le_bytes());
            data[2] = mode as u8;
            data[3..7].copy_from_slice(&target.to_le_bytes());
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0,
                0x1100,
                data,
            )]);
            responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APWR reply");
        }
        fn read_tx(responder: &mut Responder) -> (u16, i8, i32) {
            let frame = request(&[DatagramData::auto_increment(
                Command::Aprd,
                1,
                0,
                0x1110,
                vec![0u8; 7],
            )]);
            let reply = responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APRD reply");
            let start = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10;
            (
                u16::from_le_bytes([reply[start], reply[start + 1]]),
                reply[start + 2] as i8,
                i32::from_le_bytes([
                    reply[start + 3],
                    reply[start + 4],
                    reply[start + 5],
                    reply[start + 6],
                ]),
            )
        }

        let mut responder = Responder::default().with_cia402_behavior(0x1100, 7, 0x1110, 7);
        write_rx(&mut responder, 0x0006, 8, 1000);
        responder.tick(1);
        write_rx(&mut responder, 0x0007, 8, 1000);
        responder.tick(2);
        write_rx(&mut responder, 0x000F, 8, 1000);
        responder.tick(3);
        assert_eq!(read_tx(&mut responder), (0x0237 | 0x0400, 8, 1000));

        // 切换到 CSV（模式 9）并给新目标：位置保持，不置 Target reached。
        write_rx(&mut responder, 0x000F, 9, 2000);
        responder.tick(4);
        assert_eq!(
            read_tx(&mut responder),
            (0x0237, 9, 1000),
            "非 CSP 模式位置冻结、无 Target reached"
        );
    }

    #[test]
    fn cia402_fault_injection_sets_error_code_and_recovers_on_reset() {
        fn write_rx(responder: &mut Responder, controlword: u16, mode: i8, target: i32) {
            let mut data = vec![0u8; 7];
            data[0..2].copy_from_slice(&controlword.to_le_bytes());
            data[2] = mode as u8;
            data[3..7].copy_from_slice(&target.to_le_bytes());
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0,
                0x1100,
                data,
            )]);
            responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APWR reply");
        }
        fn read_statusword(responder: &mut Responder) -> u16 {
            let frame = request(&[DatagramData::auto_increment(
                Command::Aprd,
                1,
                0,
                0x1110,
                vec![0u8; 7],
            )]);
            let reply = responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APRD reply");
            let start = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10;
            u16::from_le_bytes([reply[start], reply[start + 1]])
        }

        let mut responder = Responder::default().with_cia402_behavior(0x1100, 7, 0x1110, 7);
        write_rx(&mut responder, 0x0006, 8, 1000);
        responder.tick(1);
        write_rx(&mut responder, 0x0007, 8, 1000);
        responder.tick(2);
        write_rx(&mut responder, 0x000F, 8, 1000);
        responder.tick(3);
        assert_eq!(read_statusword(&mut responder), 0x0237 | 0x0400);

        responder.inject_cia402_fault(0x7500);
        responder.tick(4);
        assert_eq!(read_statusword(&mut responder), 0x0208, "Fault 状态字");
        assert_eq!(
            responder.object_value(0x603F, 0),
            Some(0x7500u16.to_le_bytes().as_slice()),
            "错误码可经 SDO 读回"
        );
        assert_eq!(
            responder.cia402_state(),
            Some(crate::cia402::Cia402State::Fault)
        );

        // Fault reset（控制字 bit7）→ Switch on disabled，错误码保留（Tier0 行为）。
        write_rx(&mut responder, 0x0080, 8, 1000);
        responder.tick(5);
        assert_eq!(read_statusword(&mut responder), 0x0240);
        assert_eq!(
            responder.cia402_state(),
            Some(crate::cia402::Cia402State::SwitchOnDisabled)
        );
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
    fn segment_retransmission_is_idempotent() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        fn write_mailbox(responder: &mut Responder, service: &[u8]) {
            let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
            payload[0..2].copy_from_slice(&(service.len() as u16).to_le_bytes());
            payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
            payload[5] = mailbox::MAILBOX_TYPE_COE;
            payload[6..6 + service.len()].copy_from_slice(service);
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
        fn coe_request(command: u8, index: u16, subindex: u8, tail: &[u8]) -> Vec<u8> {
            let mut service = vec![0u8; 6];
            service[0..2].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
            service[2] = command;
            service[3..5].copy_from_slice(&index.to_le_bytes());
            service[5] = subindex;
            service.extend_from_slice(tail);
            service
        }

        let value = vec![b'Q'; 130];
        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x2000,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                name: "Blob".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x2000,
                subindex: 0,
                value: value.clone(),
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                bit_length: 130 * 8,
                access: esc_core::obj_dict::access::READ_WRITE_PREOP,
                value_info: 0,
                name: "Blob".to_string(),
                writable: true,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // 上传：首段请求重发（同一 toggle）应返回**相同**响应且不推进 offset。
        write_mailbox(&mut responder, &coe_request(0x40, 0x2000, 0, &[]));
        let first_request = coe_request(0x60, 0x2000, 0, &[]);
        write_mailbox(&mut responder, &first_request);
        let first_response = read_tx_mailbox(&mut responder);
        let offset_after_first = responder.upload.as_ref().expect("upload state").offset;
        write_mailbox(&mut responder, &first_request);
        let retransmitted = read_tx_mailbox(&mut responder);
        assert_eq!(first_response, retransmitted, "重发段请求应得到相同响应");
        assert_eq!(
            responder.upload.as_ref().expect("upload state").offset,
            offset_after_first,
            "重发不应推进上传 offset"
        );

        // 下载：首段重发（同一 toggle）应幂等应答且不重复追加数据。
        let payload: Vec<u8> = (0u8..20).collect();
        let mut initiate = coe_request(0x21, 0x2000, 0, &[]);
        initiate.extend_from_slice(&20u32.to_le_bytes());
        initiate.extend_from_slice(&payload[..8]);
        write_mailbox(&mut responder, &initiate);
        assert!(responder.download.is_some(), "应进入分段下载");
        // 第一段：toggle=0，7 字节。
        let mut segment0 = coe_request(0x20, 0x2000, 0, &[]);
        segment0.truncate(3);
        segment0.extend_from_slice(&payload[8..15]);
        write_mailbox(&mut responder, &segment0);
        let data_after_first = responder.download.as_ref().expect("download").data.len();
        assert_eq!(data_after_first, 15);
        // 重发同一段：toggle 仍是 0（期望现在是 1）→ 幂等应答，数据不重复。
        write_mailbox(&mut responder, &segment0);
        assert_eq!(
            responder.download.as_ref().expect("download").data.len(),
            data_after_first,
            "重发段不应重复追加数据"
        );
        // 正确下一段（toggle=1，剩余 5 字节，last）。
        let mut segment1 = coe_request(0x20, 0x2000, 0, &[]);
        segment1.truncate(3);
        segment1[2] = 0x20 | (1 << 4) | ((7 - 5) << 1) | 1;
        segment1.extend_from_slice(&payload[15..20]);
        write_mailbox(&mut responder, &segment1);
        assert!(responder.download.is_none());
        assert_eq!(
            responder.object_writes.get(&(0x2000, 0)),
            Some(&payload),
            "重发容错后仍应写入完整数据"
        );
    }

    #[test]
    fn complete_access_download_is_all_or_nothing() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        fn entry(index: u16, subindex: u8, value: &[u8], writable: bool) -> RuntimeEntry {
            RuntimeEntry {
                index,
                subindex,
                value: value.to_vec(),
                data_type: esc_core::obj_dict::data_type::UNSIGNED16,
                bit_length: (value.len() * 8) as u16,
                access: if writable {
                    esc_core::obj_dict::access::READ_WRITE_PREOP
                } else {
                    esc_core::obj_dict::access::READ_ONLY
                },
                value_info: 0,
                name: String::new(),
                writable,
            }
        }

        // 0x3000 数组：子 0（1B 可写）、子 1（2B 可写）、子 2（2B 只读）。
        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x3000,
                max_subindex: 2,
                object_code: esc_core::obj_dict::object_code::ARRAY,
                data_type: esc_core::obj_dict::data_type::UNSIGNED16,
                name: "CA array".to_string(),
            }],
            vec![
                entry(0x3000, 0, &[0x02], true),
                entry(0x3000, 1, &[0x11, 0x11], true),
                entry(0x3000, 2, &[0x22, 0x22], false),
            ],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // CA 下载 5 字节（1+2+2）：子 2 只读 → Abort 0x06010002，且**不得**写入子 0/1。
        let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
        payload[0..2].copy_from_slice(&15u16.to_le_bytes());
        payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
        payload[5] = mailbox::MAILBOX_TYPE_COE;
        payload[6..8].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
        payload[8] = 0x31;
        payload[9..11].copy_from_slice(&0x3000u16.to_le_bytes());
        payload[11] = 0x00;
        payload[12..16].copy_from_slice(&5u32.to_le_bytes());
        payload[16..21].copy_from_slice(&[0x02, 0xAA, 0xAA, 0xBB, 0xBB]);
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
        assert_eq!(response[8], 0x80, "只读子索引应回 Abort");
        assert_eq!(response[11], 0x02, "Abort 的子索引应为 2");
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            mailbox::SDO_ABORT_READ_ONLY
        );
        assert!(
            responder.object_writes.is_empty(),
            "校验失败时不得产生部分写：{:?}",
            responder.object_writes
        );
    }

    #[test]
    fn out_of_order_segment_toggle_is_aborted() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        fn write_mailbox(responder: &mut Responder, service: &[u8]) {
            let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
            payload[0..2].copy_from_slice(&(service.len() as u16).to_le_bytes());
            payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
            payload[5] = mailbox::MAILBOX_TYPE_COE;
            payload[6..6 + service.len()].copy_from_slice(service);
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
        fn coe_request(command: u8, index: u16, subindex: u8, tail: &[u8]) -> Vec<u8> {
            let mut service = vec![0u8; 6];
            service[0..2].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
            service[2] = command;
            service[3..5].copy_from_slice(&index.to_le_bytes());
            service[5] = subindex;
            service.extend_from_slice(tail);
            service
        }

        let value = vec![b'Z'; 130];
        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x2000,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                name: "Blob".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x2000,
                subindex: 0,
                value: value.clone(),
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                bit_length: 130 * 8,
                access: esc_core::obj_dict::access::READ_WRITE_PREOP,
                value_info: 0,
                name: "Blob".to_string(),
                writable: true,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // 上传：initiate 后第一段请求用 toggle=1（期望 0）→ Abort 0x05030000。
        write_mailbox(&mut responder, &coe_request(0x40, 0x2000, 0, &[]));
        assert!(responder.upload.is_some(), "130B 对象应进入分段上传");
        write_mailbox(&mut responder, &coe_request(0x70, 0x2000, 0, &[]));
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], 0x80, "toggle 错误应回 Abort");
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            mailbox::SDO_ABORT_TOGGLE
        );
        assert!(responder.upload.is_none(), "Abort 后清空分段上传状态");

        // 下载：initiate 后第一段请求用 toggle=1（期望 0）→ Abort 0x05030000。
        let mut initiate = coe_request(0x21, 0x2000, 0, &[0u8; 6]);
        initiate[6..10].copy_from_slice(&130u32.to_le_bytes());
        write_mailbox(&mut responder, &initiate);
        assert!(responder.download.is_some(), "未收完数据应进入分段下载");
        let mut segment = coe_request(0x31, 0x2000, 0, &[b'x'; 7]);
        segment[2] = 0x31; // 0x20 | toggle=1 <<4 | unused=0 | last=0
        write_mailbox(&mut responder, &segment);
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], 0x80, "下载 toggle 错误应回 Abort");
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            mailbox::SDO_ABORT_TOGGLE
        );
        assert!(responder.download.is_none(), "Abort 后清空分段下载状态");
    }

    #[test]
    fn mailbox_complete_access_download_handles_segmented_transfer() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        // 130 字节可写字符串对象：CA 首帧最多 112 字节，剩余 18 字节走 3 个 segment。
        let value = vec![b'A'; 130];
        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x2000,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                name: "Config blob".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x2000,
                subindex: 0,
                value: value.clone(),
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                bit_length: 130 * 8,
                access: esc_core::obj_dict::access::READ_WRITE_PREOP,
                value_info: 0,
                name: "Config blob".to_string(),
                writable: true,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // CA 下载 initiate：命令 0x31 + size=130 + 首帧 112 字节。
        let new_value: Vec<u8> = (0..130).map(|i| b'a' + (i % 26) as u8).collect();
        let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
        let service_len = 10 + 112;
        payload[0..2].copy_from_slice(&(service_len as u16).to_le_bytes());
        payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
        payload[5] = mailbox::MAILBOX_TYPE_COE;
        payload[6..8].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
        payload[8] = 0x31;
        payload[9..11].copy_from_slice(&0x2000u16.to_le_bytes());
        payload[11] = 0x00;
        payload[12..16].copy_from_slice(&130u32.to_le_bytes());
        payload[16..16 + 112].copy_from_slice(&new_value[..112]);
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
        assert_eq!(response[8], 0x60, "CA 分段下载 initiate 响应 0x60");
        assert!(responder.download.is_some(), "首帧未完成 → 进入分段状态");

        // 3 个 segment：7 / 7 / 4 字节，toggle 0/1/0，末段 last=1。
        let chunks: [&[u8]; 3] = [
            &new_value[112..119],
            &new_value[119..126],
            &new_value[126..130],
        ];
        for (i, chunk) in chunks.iter().enumerate() {
            let toggle = (i % 2) as u8;
            let unused = 7u8 - chunk.len() as u8;
            let last = u8::from(i == 2);
            let command = 0x20u8 | (toggle << 4) | (unused << 1) | last;
            let mut segment = vec![0u8; 3 + chunk.len()];
            segment[0..2].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
            segment[2] = command;
            segment[3..].copy_from_slice(chunk);
            let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
            payload[0..2].copy_from_slice(&(segment.len() as u16).to_le_bytes());
            payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
            payload[5] = mailbox::MAILBOX_TYPE_COE;
            payload[6..6 + segment.len()].copy_from_slice(&segment);
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
        assert!(responder.download.is_none(), "末段完成后清零分段状态");
        assert_eq!(
            responder.object_writes.get(&(0x2000, 0)),
            Some(&new_value),
            "CA 分段下载数据应完整写入对象"
        );
    }

    #[test]
    fn mailbox_complete_access_download_writes_all_subindices() {
        // CA 下载（命令 0x31，单帧）：0x1600 = 计数 1 + 映射字 0x70000008（5 字节）。
        let mut responder = Responder::default();
        let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
        payload[0..2].copy_from_slice(&15u16.to_le_bytes()); // 服务数据长度 = 10 + 5
        payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
        payload[5] = mailbox::MAILBOX_TYPE_COE;
        payload[6..8].copy_from_slice(&(mailbox::COE_COMMAND_SDO_REQUEST << 12).to_le_bytes());
        payload[8] = 0x31; // CA download（normal、size indicated）
        payload[9..11].copy_from_slice(&0x1600u16.to_le_bytes());
        payload[11] = 0x00;
        payload[12..16].copy_from_slice(&5u32.to_le_bytes());
        payload[16..21].copy_from_slice(&[0x01, 0x08, 0x00, 0x00, 0x70]);
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
        let header = MailboxHeader::parse(&response).expect("mailbox header");
        assert_eq!(header.length, 6, "CA 下载响应为 6 字节（无数据）");
        assert_eq!(response[8], 0x60, "initiate download response 0x60");
        assert_eq!(u16::from_le_bytes([response[9], response[10]]), 0x1600);
        assert_eq!(response[11], 0x00);

        // 回读：CA 上传 0x1600 应返回刚写入的 5 字节。
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            mailbox::COE_COMMAND_SDO_REQUEST,
            0x50,
            0x1600,
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
        assert_eq!(response[8], 0x41);
        assert_eq!(&response[16..21], &[0x01, 0x08, 0x00, 0x00, 0x70]);
    }

    #[test]
    fn segmented_download_overflow_is_aborted_with_length_mismatch() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x2100,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                name: "Writable string".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x2100,
                subindex: 0,
                value: vec![0u8; 20],
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                bit_length: 160,
                access: esc_core::obj_dict::access::READ_WRITE_PREOP,
                value_info: 0,
                name: "Writable string".to_string(),
                writable: true,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // initiate：声明 20 字节，首段 8 字节。
        let mut service = vec![0x00, 0x20, 0x21];
        service.extend_from_slice(&0x2100u16.to_le_bytes());
        service.push(0x00);
        service.extend_from_slice(&20u32.to_le_bytes());
        service.extend_from_slice(&[0xAA; 8]);
        write_mailbox(&mut responder, mailbox_service_payload(&service));
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_COMMAND_DOWNLOAD_RESPONSE);
        assert!(responder.download.is_some());

        // 第一段 7 字节（累计 15）。
        let mut segment = vec![0x00, 0x20, 0x20];
        segment.extend_from_slice(&[0xBB; 7]);
        write_mailbox(&mut responder, mailbox_service_payload(&segment));
        let response = read_tx_service(&mut responder);
        assert_eq!(
            response[2] & 0xE0,
            0x20,
            "segment download response 0x20|toggle<<4"
        );

        // 第二段 7 字节：15+7=22 > 20 → Abort 0x06070010，状态清空、无写入。
        let mut segment = vec![0x00, 0x20, 0x30]; // toggle=1
        segment.extend_from_slice(&[0xCC; 7]);
        write_mailbox(&mut responder, mailbox_service_payload(&segment));
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_COMMAND_ABORT);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            mailbox::SDO_ABORT_LENGTH_MISMATCH
        );
        assert!(responder.download.is_none(), "Abort 后清空分段状态");
        assert!(responder.object_writes.is_empty(), "Abort 时不得写入对象");

        // initiate 自身数据超过声明长度 → 同样 Abort 0x06070010。
        let mut service = vec![0x00, 0x20, 0x21];
        service.extend_from_slice(&0x2100u16.to_le_bytes());
        service.push(0x00);
        service.extend_from_slice(&4u32.to_le_bytes());
        service.extend_from_slice(&[0xDD; 8]);
        write_mailbox(&mut responder, mailbox_service_payload(&service));
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_COMMAND_ABORT);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            mailbox::SDO_ABORT_LENGTH_MISMATCH
        );
    }

    #[test]
    fn mailbox_complete_access_upload_returns_all_subindices() {
        // Complete Access 上传（命令 0x50）：一次返回 0x1C12 全部子索引（子 0 计数 + 子 1
        // 分配值），按各子索引自身长度拼接、不做填充。
        let mut responder = Responder::default();
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            mailbox::COE_COMMAND_SDO_REQUEST,
            0x50,
            0x1C12,
            0x00,
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
        assert!(
            mailbox::mailbox_full(read_sm1_status(&mut responder)),
            "CA 响应就绪后 SM1 status bit3 必须置位"
        );
        let response = read_tx_mailbox(&mut responder);
        let header = MailboxHeader::parse(&response).expect("mailbox header");
        assert_eq!(header.length, 13, "10 字节头 + 3 字节 CA 数据");
        assert_eq!(response[8], 0x41, "CA 上传用 normal response 0x41");
        assert_eq!(u16::from_le_bytes([response[9], response[10]]), 0x1C12);
        assert_eq!(response[11], 0x00, "CA 响应子索引固定为 0");
        assert_eq!(
            u32::from_le_bytes([response[12], response[13], response[14], response[15]]),
            3,
            "CA 数据总长度 = 1（子 0）+ 2（子 1）"
        );
        assert_eq!(&response[16..19], &[0x01, 0x00, 0x16]);

        // 0x1600（子 0 计数 1 + 子 1 映射字 0x70000008）→ 5 字节 CA 数据。
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            mailbox::COE_COMMAND_SDO_REQUEST,
            0x50,
            0x1600,
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
        assert_eq!(&response[16..21], &[0x01, 0x08, 0x00, 0x00, 0x70]);
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

        // 未知 SDO Info 操作码（0x7F）：不应答（SDO Info 已实现，见
        // `sdo_info_od_list_and_descriptions_are_answered`）。
        let payload = mailbox_request_payload(
            mailbox::MAILBOX_TYPE_COE,
            0x8, // SDO information
            0x7F,
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
        // ESM 前置条件默认开启：按主站（IgH/SOEM）的顺序先配置邮箱 SM0/SM1 与过程数据
        // SM2/SM3，再请求 INIT→PREOP→SAFEOP→OP。
        configure_sync_managers(responder, 4);
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

    /// 通过 APWR 写 SM0..SM(count-1) 的寄存器块（与主站配置邮箱/过程数据 SM 的方式一致）。
    fn configure_sync_managers(responder: &mut Responder, count: usize) {
        let bytes: Vec<u8> = SIM_SYNC_MANAGERS
            .iter()
            .take(count)
            .flat_map(|sm| sm.to_bytes())
            .collect();
        write_regs(responder, esc_core::sm::SYNC_MANAGER_ADDRESS, bytes);
    }

    #[test]
    fn dc_system_time_advances_with_ticks() {
        let mut responder = Responder::default();
        let read_time = |r: &mut Responder, address: u16| {
            u64::from_le_bytes(
                r.read_registers(address, 8)
                    .try_into()
                    .expect("8 bytes of DC time"),
            )
        };
        assert_eq!(read_time(&mut responder, 0x0910), 0);
        responder.tick(1);
        let t2 = read_time(&mut responder, 0x0910);
        assert_eq!(t2, crate::DC_TICK_NS);
        responder.tick(2);
        let t3 = read_time(&mut responder, 0x0910);
        assert_eq!(t3, t2 + crate::DC_TICK_NS);
        // 0x0900 端口接收时间由同一模拟时钟回填（Tier0 简化）。
        assert_eq!(read_time(&mut responder, 0x0900), t3);
    }

    #[test]
    fn handle_frame_never_panics_on_random_bytes() {
        // 确定性 LCG：任意字节/长度/以太网类型的帧都不能让应答器 panic。
        let mut state = 0x1234_5678_9ABC_DEF0u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state
        };
        for iteration in 0..500 {
            let len = 14 + usize::try_from(next() % 120).expect("len") + (iteration % 7);
            let mut frame = vec![0u8; len];
            for byte in &mut frame {
                *byte = (next() >> 33) as u8;
            }
            // 一半随机帧设成 EtherCAT 以太网类型，覆盖解析/分发路径。
            if iteration % 2 == 0 && len >= 14 {
                frame[12] = 0x88;
                frame[13] = 0xA4;
            }
            let _ = responder_hint(&mut Responder::default(), &frame);
        }
    }

    fn responder_hint(responder: &mut Responder, frame: &[u8]) -> Option<Vec<u8>> {
        responder.handle_frame(frame, DEFAULT_SIM_MAC)
    }

    #[test]
    fn dc_register_access_is_counted() {
        let mut responder = Responder::default();
        assert_eq!(responder.stats().dc_reg_reads, 0);
        assert_eq!(responder.stats().dc_reg_writes, 0);
        write_regs(&mut responder, 0x0980, vec![0x01, 0x00]);
        responder.read_registers(0x0910, 8);
        responder.read_registers(0x0130, 2); // 非 DC 区不计
        assert_eq!(responder.stats().dc_reg_writes, 1);
        assert_eq!(responder.stats().dc_reg_reads, 1);
    }

    #[test]
    fn pdi_watchdog_register_is_read_write_without_timeout() {
        // 0x0410（Watchdog Time PDI）：可读写保存；模拟器无 PDI，不模拟超时（不置 AL error）。
        let mut responder = Responder::default();
        let address = esc_core::watchdog::WATCHDOG_TIME_PDI;
        write_regs(&mut responder, address, 0x00C8u16.to_le_bytes().to_vec());
        assert_eq!(
            responder.read_registers(address, 2),
            0x00C8u16.to_le_bytes().to_vec()
        );
        responder.tick(1000);
        assert_eq!(
            responder.read_registers(address, 2),
            0x00C8u16.to_le_bytes().to_vec(),
            "无 PDI 时 0x0410 只保存值、不递减/超时"
        );
        assert_eq!(
            al_status_byte(&mut responder) & 0x10,
            0,
            "不应产生 AL error"
        );
    }

    #[test]
    fn esm_preconditions_are_on_by_default() {
        // 默认开启：不配置 SM 时拒绝 PREOP（AL Status Code 0x0015、状态保持 INIT）。
        let mut responder = Responder::default();
        assert!(responder.esm_preconditions_enabled());
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]);
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x01);
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x10);
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::INVALID_MAILBOX_CONFIGURATION
        );

        // 显式关闭（CLI `--esm-lax`）才放行未配置 SM 的状态迁移。
        let mut lax = Responder::default().with_esm_preconditions(false);
        assert!(!lax.esm_preconditions_enabled());
        write_regs(&mut lax, reg::AL_CONTROL, vec![0x02]);
        assert_eq!(al_status_byte(&mut lax) & 0x0F, 0x02);
        assert_eq!(al_status_byte(&mut lax) & 0x10, 0x00);
        assert_eq!(al_status_code(&mut lax), 0);
    }

    #[test]
    fn esm_strict_rejects_preop_until_mailbox_sync_managers_configured() {
        let mut responder = Responder::default().with_esm_preconditions(true);
        // SM0/SM1 未配置 → 拒绝 PREOP，AL Status Code = 0x0015，状态保持 INIT。
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]);
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x01, "前置条件失败时状态保持 INIT");
        assert_eq!(status & 0x10, 0x10, "error 位必须置位");
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::INVALID_MAILBOX_CONFIGURATION
        );
        // 主站 ACK（写当前状态 + ACK 位）清错；配置邮箱 SM0/SM1 后 PREOP 被接受。
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x11]);
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x00);
        configure_sync_managers(&mut responder, 2);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]);
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x02);
        assert_eq!(status & 0x10, 0x00);
        assert_eq!(al_status_code(&mut responder), 0);
    }

    #[test]
    fn esm_strict_rejects_safeop_with_overlapping_sync_managers() {
        let mut responder = Responder::default().with_esm_preconditions(true);
        configure_sync_managers(&mut responder, 2); // 邮箱 SM0/SM1（0x1000/0x1080 各 128B）
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]); // PREOP 正常
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x02);
        // 把 SM2 地址改到与 SM1 重叠（0x1080，4 字节）并激活 → 布局非法。
        let overlapping = SyncManager {
            start_address: 0x1080,
            length: 4,
            control: 0,
            status: 0,
            activate: 1,
            pdi_control: 0,
        };
        write_regs(
            &mut responder,
            esc_core::sm::SYNC_MANAGER_ADDRESS + 2 * esc_core::sm::SYNC_MANAGER_SIZE,
            overlapping.to_bytes().to_vec(),
        );
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x04]); // SAFEOP
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x02, "布局非法时状态保持 PREOP");
        assert_eq!(status & 0x10, 0x10, "error 位必须置位");
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::INVALID_SYNC_MANAGER_CONFIGURATION
        );
    }

    #[test]
    fn esm_strict_rejects_safeop_until_process_data_sync_managers_configured() {
        let mut responder = Responder::default().with_esm_preconditions(true);
        configure_sync_managers(&mut responder, 2); // 只有邮箱 SM
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]);
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x02);
        // SM2/SM3 未配置 → 拒绝 SAFEOP，AL Status Code = 0x0017。
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x04]);
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x02, "状态保持 PREOP");
        assert_eq!(status & 0x10, 0x10);
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::INVALID_SYNC_MANAGER_CONFIGURATION
        );
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x14]); // PREOP|ACK
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x00);
        // 配置 SM2/SM3 后 SAFEOP → OP 成功。
        configure_sync_managers(&mut responder, 4);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x04]);
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x04);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x08]);
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x08);
        assert_eq!(status & 0x10, 0x00);
        assert_eq!(al_status_code(&mut responder), 0);
    }

    #[test]
    fn esm_reports_unknown_requested_state_code() {
        // 0x0F 不是合法状态编码 → AL Status Code 0x0012（而非 0x0011）。
        let mut responder = Responder::default().with_esm_preconditions(true);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x0F]);
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x10);
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::UNKNOWN_REQUESTED_STATE
        );
    }

    #[test]
    fn esm_strict_rejects_op_until_inputs_configured() {
        let mut responder = Responder::default().with_esm_preconditions(true);
        configure_sync_managers(&mut responder, 3); // SM0/SM1/SM2，缺 SM3
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x04]);
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x04);
        // SII 声明了 TxPDO 但 SM3 未配置 → OP 被拒绝，AL Status Code 0x0018。
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x08]);
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x04, "前置条件失败时状态保持 SAFEOP");
        assert_eq!(status & 0x10, 0x10, "error 位必须置位");
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::NO_VALID_INPUTS
        );
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x14]); // SAFEOP|ACK
        assert_eq!(al_status_byte(&mut responder) & 0x10, 0x00);
        configure_sync_managers(&mut responder, 4);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x08]);
        assert_eq!(
            al_status_byte(&mut responder) & 0x0F,
            0x08,
            "SM3 配置后 OP 通过"
        );
        assert_eq!(al_status_code(&mut responder), 0);
    }

    #[test]
    fn esm_strict_rejects_op_with_invalid_process_data_mapping() {
        // 运行字典缺少 PDO 映射对象 → 映射一致性检查失败，OP 被拒绝（0x0024）。
        let dict = ObjectDictionary::new(Vec::new(), Vec::new());
        let mut responder = Responder::default()
            .with_dictionary(dict)
            .with_esm_preconditions(true);
        configure_sync_managers(&mut responder, 4);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]); // PREOP
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x04]); // SAFEOP
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x04);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x08]); // OP
        let status = al_status_byte(&mut responder);
        assert_eq!(status & 0x0F, 0x04, "映射无效时状态保持 SAFEOP");
        assert_eq!(status & 0x10, 0x10, "error 位必须置位");
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::NO_VALID_OUTPUT_CONFIGURATION
        );
    }

    #[test]
    fn esm_strict_rejects_op_with_duplicate_pdo_assignment() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        // 0x1C12 把同一个 PDO（0x1600）分配两次 → 映射无效（0x0024）。
        let objects = vec![
            RuntimeObject {
                index: 0x1C12,
                max_subindex: 2,
                object_code: esc_core::obj_dict::object_code::ARRAY,
                data_type: esc_core::obj_dict::data_type::UNSIGNED16,
                name: "RxPDO assign".to_string(),
            },
            RuntimeObject {
                index: 0x1600,
                max_subindex: 1,
                object_code: esc_core::obj_dict::object_code::ARRAY,
                data_type: esc_core::obj_dict::data_type::UNSIGNED32,
                name: "RxPDO mapping".to_string(),
            },
        ];
        let entry =
            |index: u16, subindex: u8, value: Vec<u8>, data_type: u16, bits: u16| RuntimeEntry {
                index,
                subindex,
                value,
                data_type,
                bit_length: bits,
                access: esc_core::obj_dict::access::READ_ONLY,
                value_info: 0,
                name: String::new(),
                writable: false,
            };
        let entries = vec![
            entry(
                0x1C12,
                0,
                vec![2],
                esc_core::obj_dict::data_type::UNSIGNED8,
                8,
            ),
            entry(
                0x1C12,
                1,
                0x1600u16.to_le_bytes().to_vec(),
                esc_core::obj_dict::data_type::UNSIGNED16,
                16,
            ),
            entry(
                0x1C12,
                2,
                0x1600u16.to_le_bytes().to_vec(),
                esc_core::obj_dict::data_type::UNSIGNED16,
                16,
            ),
            entry(
                0x1600,
                0,
                vec![1],
                esc_core::obj_dict::data_type::UNSIGNED8,
                8,
            ),
            entry(
                0x1600,
                1,
                0x7000_0008u32.to_le_bytes().to_vec(),
                esc_core::obj_dict::data_type::UNSIGNED32,
                32,
            ),
        ];
        let dict = ObjectDictionary::new(objects, entries);
        let mut responder = Responder::default()
            .with_dictionary(dict)
            .with_esm_preconditions(true);
        configure_sync_managers(&mut responder, 4);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x02]);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x04]);
        assert_eq!(al_status_byte(&mut responder) & 0x0F, 0x04);
        write_regs(&mut responder, reg::AL_CONTROL, vec![0x08]);
        assert_eq!(
            al_status_code(&mut responder),
            esc_core::al_status::NO_VALID_OUTPUT_CONFIGURATION
        );
        assert_eq!(
            al_status_byte(&mut responder) & 0x0F,
            0x04,
            "状态保持 SAFEOP"
        );
    }

    #[test]
    fn cia402_cst_mode_mirrors_target_torque() {
        fn write_rx(responder: &mut Responder, controlword: u16, mode: i8, torque: i16) {
            let mut data = vec![0u8; 5];
            data[0..2].copy_from_slice(&controlword.to_le_bytes());
            data[2] = mode as u8;
            data[3..5].copy_from_slice(&torque.to_le_bytes());
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0,
                0x1100,
                data,
            )]);
            responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APWR reply");
        }
        fn read_tx(responder: &mut Responder) -> (u16, i8, i16) {
            let frame = request(&[DatagramData::auto_increment(
                Command::Aprd,
                1,
                0,
                0x1110,
                vec![0u8; 5],
            )]);
            let reply = responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APRD reply");
            let start = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10;
            (
                u16::from_le_bytes([reply[start], reply[start + 1]]),
                reply[start + 2] as i8,
                i16::from_le_bytes([reply[start + 3], reply[start + 4]]),
            )
        }

        let mut responder = Responder::default().with_cia402_behavior(0x1100, 5, 0x1110, 5);
        write_rx(&mut responder, 0x0006, 10, 300); // Shutdown
        responder.tick(1);
        assert_eq!(read_tx(&mut responder), (0x0221, 10, 0));
        write_rx(&mut responder, 0x0007, 10, 300); // Switch on
        responder.tick(2);
        assert_eq!(read_tx(&mut responder), (0x0223, 10, 0));
        write_rx(&mut responder, 0x000F, 10, 300); // Enable operation
        responder.tick(3);
        assert_eq!(
            read_tx(&mut responder),
            (0x0237, 10, 300),
            "CST：实际转矩=目标转矩"
        );
        write_rx(&mut responder, 0x0000, 10, 300); // Disable voltage
        responder.tick(4);
        assert_eq!(
            read_tx(&mut responder),
            (0x0240, 10, 0),
            "禁用后实际转矩归零"
        );
    }

    #[test]
    fn cia402_csv_mode_accumulates_velocity() {
        fn write_rx(
            responder: &mut Responder,
            controlword: u16,
            mode: i8,
            target: i32,
            velocity: i32,
        ) {
            let mut data = vec![0u8; 11];
            data[0..2].copy_from_slice(&controlword.to_le_bytes());
            data[2] = mode as u8;
            data[3..7].copy_from_slice(&target.to_le_bytes());
            data[7..11].copy_from_slice(&velocity.to_le_bytes());
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0,
                0x1100,
                data,
            )]);
            responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APWR reply");
        }
        fn read_tx(responder: &mut Responder) -> (u16, i8, i32, i32) {
            let frame = request(&[DatagramData::auto_increment(
                Command::Aprd,
                1,
                0,
                0x1110,
                vec![0u8; 11],
            )]);
            let reply = responder
                .handle_frame(&frame, DEFAULT_SIM_MAC)
                .expect("APRD reply");
            let start = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN + 10;
            (
                u16::from_le_bytes([reply[start], reply[start + 1]]),
                reply[start + 2] as i8,
                i32::from_le_bytes([
                    reply[start + 3],
                    reply[start + 4],
                    reply[start + 5],
                    reply[start + 6],
                ]),
                i32::from_le_bytes([
                    reply[start + 7],
                    reply[start + 8],
                    reply[start + 9],
                    reply[start + 10],
                ]),
            )
        }

        let mut responder = Responder::default().with_cia402_behavior(0x1100, 11, 0x1110, 11);
        write_rx(&mut responder, 0x0006, 9, 0, 5); // Shutdown → Ready
        responder.tick(1);
        assert_eq!(read_tx(&mut responder), (0x0221, 9, 0, 0));
        write_rx(&mut responder, 0x0007, 9, 0, 5); // Switch on
        responder.tick(2);
        assert_eq!(read_tx(&mut responder), (0x0223, 9, 0, 0));
        write_rx(&mut responder, 0x000F, 9, 0, 5); // Enable operation
        responder.tick(3);
        assert_eq!(
            read_tx(&mut responder),
            (0x0237, 9, 5, 5),
            "CSV：位置 +5/周期"
        );
        write_rx(&mut responder, 0x000F, 9, 0, 5);
        responder.tick(4);
        assert_eq!(read_tx(&mut responder), (0x0237, 9, 10, 5));
        // 禁用后速度显示归零、位置保持。
        write_rx(&mut responder, 0x0000, 9, 0, 5);
        responder.tick(5);
        assert_eq!(read_tx(&mut responder), (0x0240, 9, 10, 0));
    }

    #[test]
    fn custom_dictionary_drives_sdo_upload_and_info() {
        // 运行时字典可被整体替换（声明式 manifest 画像用）：SDO 上传与 SDO Info 都应以新字典为准。
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x2000,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::UNSIGNED16,
                name: "Custom counter".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x2000,
                subindex: 0,
                value: 0x1234u16.to_le_bytes().to_vec(),
                data_type: esc_core::obj_dict::data_type::UNSIGNED16,
                bit_length: 16,
                access: esc_core::obj_dict::access::READ_ONLY,
                value_info: 0,
                name: "Custom counter".to_string(),
                writable: false,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);
        assert_eq!(responder.dictionary().object_indices(), vec![0x2000]);

        // SDO 上传 0x2000:00 → 0x1234。
        write_mailbox(
            &mut responder,
            mailbox_request_payload(
                mailbox::MAILBOX_TYPE_COE,
                mailbox::COE_COMMAND_SDO_REQUEST,
                0x40,
                0x2000,
                0x00,
            ),
        );
        let response = read_tx_mailbox(&mut responder);
        assert_eq!(response[8], 0x4B, "2 字节 expedited upload");
        assert_eq!(u16::from_le_bytes([response[12], response[13]]), 0x1234);

        // SDO Info：Get OD List 只返回新字典的索引。
        let od_list = [0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00];
        write_mailbox(&mut responder, mailbox_sdo_info_payload(&od_list));
        let response = read_tx_service(&mut responder);
        assert_eq!(u16::from_le_bytes([response[8], response[9]]), 0x2000);
        assert_eq!(response.len(), 8 + 2, "只有 1 个对象");
    }

    /// 把完整 CoE 服务数据（含 CoE 头与操作码）写成一条 SM0 邮箱报文。
    fn mailbox_service_payload(service: &[u8]) -> Vec<u8> {
        let mut payload = vec![0u8; mailbox::MAILBOX_SIZE];
        payload[0..2].copy_from_slice(&(service.len() as u16).to_le_bytes());
        payload[2..4].copy_from_slice(&0x1001u16.to_le_bytes());
        payload[4] = 0;
        payload[5] = mailbox::MAILBOX_TYPE_COE;
        payload[6..6 + service.len()].copy_from_slice(service);
        payload
    }

    /// 把 CoE 服务数据（含 CoE 头）写成一条 SM0 邮箱报文（长度 = 服务数据长度）。
    fn mailbox_sdo_info_payload(service: &[u8]) -> Vec<u8> {
        mailbox_service_payload(service)
    }

    /// 从 SM1 邮箱读回 CoE 服务数据（含 CoE 头）。
    fn read_tx_service(responder: &mut Responder) -> Vec<u8> {
        let response = read_tx_mailbox(responder);
        let header = MailboxHeader::parse(&response).expect("mailbox header");
        let len = usize::from(header.length);
        response[6..6 + len].to_vec()
    }

    #[test]
    fn sdo_info_od_list_and_descriptions_are_answered() {
        let mut responder = Responder::default();

        // 1) Get OD List：opcode 0x01，deliver all = 1；响应索引表从 +8 开始。
        let od_list_request = [0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00];
        write_mailbox(&mut responder, mailbox_sdo_info_payload(&od_list_request));
        assert!(mailbox::mailbox_full(read_sm1_status(&mut responder)));
        let response = read_tx_service(&mut responder);
        assert_eq!(&response[..2], &[0x00, 0x80], "CoE 服务 = SDO Info");
        assert_eq!(response[2], mailbox::SDO_INFO_GET_OD_LIST_RESPONSE);
        assert_eq!(&response[4..6], &[0, 0], "单帧：fragments left = 0");
        let indices: Vec<u16> = response[8..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        assert_eq!(
            indices,
            vec![0x1018, 0x1C00, 0x1C12, 0x1C13, 0x1600, 0x1A00]
        );

        // 2) Get OD（0x1C12）：opcode 0x04，index +6、名称 +12。
        let od_request = [0x00, 0x80, 0x03, 0x00, 0x00, 0x00, 0x12, 0x1C];
        write_mailbox(&mut responder, mailbox_sdo_info_payload(&od_request));
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_INFO_GET_OD_RESPONSE);
        assert_eq!(u16::from_le_bytes([response[6], response[7]]), 0x1C12);
        assert_eq!(response[10], 1, "max_subindex");
        assert_eq!(
            response[11],
            esc_core::obj_dict::object_code::ARRAY,
            "object_code"
        );
        assert_eq!(&response[12..], b"RxPDO assign".as_slice());

        // 3) Get Entry Description（0x1C12:01）：opcode 0x06，名称 +16。
        let entry_request = [0x00, 0x80, 0x05, 0x00, 0x00, 0x00, 0x12, 0x1C, 0x01, 0x07];
        write_mailbox(&mut responder, mailbox_sdo_info_payload(&entry_request));
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_INFO_GET_ENTRY_RESPONSE);
        assert_eq!(u16::from_le_bytes([response[6], response[7]]), 0x1C12);
        assert_eq!(response[8], 0x01);
        assert_eq!(
            u16::from_le_bytes([response[10], response[11]]),
            esc_core::obj_dict::data_type::UNSIGNED16
        );
        assert_eq!(u16::from_le_bytes([response[12], response[13]]), 16);
        assert_eq!(
            u16::from_le_bytes([response[14], response[15]]),
            esc_core::obj_dict::access::READ_WRITE_PREOP
        );
        assert_eq!(&response[16..], b"RxPDO assign".as_slice());

        // 4) 未知对象 → SDO Info 错误响应（opcode 0x07，abort code 从 +6 开始）。
        let unknown_request = [0x00, 0x80, 0x03, 0x00, 0x00, 0x00, 0x00, 0x20];
        write_mailbox(&mut responder, mailbox_sdo_info_payload(&unknown_request));
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_INFO_ERROR_RESPONSE);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            mailbox::SDO_ABORT_OBJECT_NOT_EXIST
        );
    }

    #[test]
    fn segmented_download_writes_long_object() {
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        let payload: Vec<u8> = (0u8..20).collect();
        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x2100,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                name: "Writable string".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x2100,
                subindex: 0,
                value: vec![0u8; 20],
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                bit_length: 160,
                access: esc_core::obj_dict::access::READ_WRITE_PREOP,
                value_info: 0,
                name: "Writable string".to_string(),
                writable: true,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // initiate download（normal，size indicated）：首段 8 字节。
        let first = &payload[..8];
        let mut service = Vec::new();
        service.extend_from_slice(&[0x00, 0x20, 0x21]);
        service.extend_from_slice(&0x2100u16.to_le_bytes());
        service.push(0x00);
        service.extend_from_slice(&u32::try_from(payload.len()).expect("size").to_le_bytes());
        service.extend_from_slice(first);
        write_mailbox(&mut responder, mailbox_service_payload(&service));
        let response = read_tx_service(&mut responder);
        assert_eq!(
            response[2],
            mailbox::SDO_COMMAND_DOWNLOAD_RESPONSE,
            "initiate download 响应"
        );
        assert!(responder.download.is_some(), "首段不完整时应保持下载状态");

        // segment 请求：剩余 12 字节，分 7 + 5 两段（toggle 0 → 1）。
        let mut offset = first.len();
        let mut toggle = 0u8;
        while offset < payload.len() {
            let remaining = payload.len() - offset;
            let take = remaining.min(7);
            let last = offset + take >= payload.len();
            let unused = 7 - take;
            let mut chunk = payload[offset..offset + take].to_vec();
            chunk.resize(7, 0);
            let command = 0x20
                | (toggle << 4)
                | (u8::try_from(unused).expect("unused") << 1)
                | u8::from(last);
            let mut service = vec![0x00, 0x20, command];
            service.extend_from_slice(&chunk);
            write_mailbox(&mut responder, mailbox_service_payload(&service));
            let response = read_tx_service(&mut responder);
            assert_eq!(response[2] & 0xE0, 0x20, "segment download 响应命令");
            assert_eq!((response[2] >> 4) & 0x01, toggle, "toggle 回显");
            offset += take;
            toggle ^= 1;
        }
        assert!(!responder.download.is_some(), "末段后清除下载状态");
        assert_eq!(
            responder.object_value(0x2100, 0),
            Some(payload.as_slice()),
            "分段下载的数据必须写入对象并可回读"
        );
    }

    #[test]
    fn segmented_upload_transfers_long_object() {
        // 130 字节的字符串对象（> 首帧 112 字节）→ initiate 响应 + 3 个段响应。
        use esc_core::obj_dict::{RuntimeEntry, RuntimeObject};

        let payload: Vec<u8> = (0u8..130).collect();
        let dict = ObjectDictionary::new(
            vec![RuntimeObject {
                index: 0x1008,
                max_subindex: 0,
                object_code: esc_core::obj_dict::object_code::VAR,
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                name: "Device name".to_string(),
            }],
            vec![RuntimeEntry {
                index: 0x1008,
                subindex: 0,
                value: payload.clone(),
                data_type: esc_core::obj_dict::data_type::VISIBLE_STRING,
                bit_length: u16::try_from(payload.len() * 8).expect("bits"),
                access: esc_core::obj_dict::access::READ_ONLY,
                value_info: 0,
                name: "Device name".to_string(),
                writable: false,
            }],
        );
        let mut responder = Responder::default().with_dictionary(dict);

        // initiate upload → 非 expedited 响应（0x41），首帧 112 字节。
        write_mailbox(
            &mut responder,
            mailbox_request_payload(
                mailbox::MAILBOX_TYPE_COE,
                mailbox::COE_COMMAND_SDO_REQUEST,
                0x40,
                0x1008,
                0x00,
            ),
        );
        let response = read_tx_service(&mut responder);
        assert_eq!(response[2], mailbox::SDO_COMMAND_UPLOAD_NORMAL_RESPONSE);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            130,
            "长度字段 = 完整对象字节数"
        );
        let mut received = response[10..].to_vec();
        assert_eq!(received.len(), 112, "首帧容量 = 128 - 6 - 10");
        assert!(responder.segmented_upload_in_progress());

        // 依次发段请求（toggle 交替），直到末段。
        let mut toggle = 0u8;
        for _ in 0..8 {
            let request = [
                0x00,
                0x20,
                mailbox::SDO_COMMAND_UPLOAD_SEGMENT_REQUEST | (toggle << 4),
                0x08,
                0x10,
                0x00,
                0x00,
            ];
            write_mailbox(&mut responder, mailbox_service_payload(&request));
            let response = read_tx_service(&mut responder);
            let unused = (response[2] & 0x0E) >> 1;
            let last = response[2] & 0x01;
            assert_eq!((response[2] >> 4) & 0x01, toggle, "段响应 echo toggle");
            let take = mailbox::SDO_SEGMENT_DATA_LEN - usize::from(unused);
            received.extend_from_slice(&response[3..3 + take]);
            toggle ^= 1;
            if last == 1 {
                break;
            }
        }
        assert_eq!(received, payload, "分段上传内容必须与对象值一致");
        assert!(
            !responder.segmented_upload_in_progress(),
            "末段后清除传输状态"
        );
    }
}
