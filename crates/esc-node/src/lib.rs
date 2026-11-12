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
//! - `LRD/LWR/LRW`：需要 FMMU，本阶段未实现 → 不命中（WKC 不增）。
//!
//! 命中后按命令的读/写属性回填数据并累加 WKC；未命中的 datagram 原样保留（WKC 不变）。

use esc_core::eeprom::EepromInterface;
use esc_core::sii::SiiImage;
use esc_core::{reg, AlControl, EscAl, Registers};
use wire::{
    auto_increment_step, matches_configured_address, Command, Frame, ECAT_HEADER_LEN,
    ETHERNET_HEADER_LEN,
};

/// 本从站应答时使用的默认 MAC（TAP 接口 MAC 由 CLI 从 `/sys/class/net` 读取后覆盖）。
pub const DEFAULT_SIM_MAC: [u8; 6] = [0x02, 0xec, 0x51, 0x4d, 0x00, 0x02];

/// DL Status 初值：
/// - bit0–3：端口 0–3 通信状态；
/// - bit4/bit5：端口 0/1 链路建立；
/// - bit12/bit14（`8 + 2*i`，i=2/3）：端口 2/3 环回闭合。
///
/// IgH `master.c: ec_master_calc_topology_rec()` 只对端口 3、2 检查 `loop_closed`：
/// 未接端口必须报"闭合"，否则它认为该端口后还有从站，单从站时拓扑计算会失败
/// （现象：`Failed to calculate bus topology`）。端口 0/1 由链路位表示。
pub const DL_STATUS_INITIAL: u16 = 0x503F;

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
}

/// 单从站最小 ESC 应答器。
#[derive(Debug, Clone)]
pub struct Responder {
    regs: Registers,
    al: EscAl,
    eeprom: EepromInterface,
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

            if self.matches(command, adp) {
                self.stats.datagrams_hit = self.stats.datagrams_hit.saturating_add(1);
                if command.is_read() {
                    let bytes = self.read_registers(register_offset, data_len);
                    reply[data_start..wkc_offset].copy_from_slice(&bytes);
                }
                if command.is_write() {
                    let data = reply[data_start..wkc_offset].to_vec();
                    self.write_registers(register_offset, &data);
                }
                let wkc = u16::from_le_bytes([reply[wkc_offset], reply[wkc_offset + 1]]);
                let updated = wkc.wrapping_add(command.wkc_increment());
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
            // 逻辑寻址需要 FMMU，本阶段不实现。
            Command::Lrd | Command::Lwr | Command::Lrw => false,
        }
    }

    fn read_registers(&self, address: u16, len: usize) -> Vec<u8> {
        if EepromInterface::covers(address, len) {
            return self.eeprom.read(address, len);
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

    fn write_registers(&mut self, address: u16, data: &[u8]) {
        if EepromInterface::covers(address, data.len()) {
            self.eeprom.write(address, data);
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
}
