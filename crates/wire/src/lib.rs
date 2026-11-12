//! 线缆层：EtherCAT 以太网帧与 datagram 编解码、严格 WKC 语义、地址匹配。
//!
//! 依据：
//! - EtherType `0x88A4`（IANA 分配）——Spike 1 已在 Linux TAP 上双向验证
//!   （[docs/spikes/spike-1-tap-bidirectional.md]）。
//! - 帧/datagram 字段以 **ETG.1000.4 / ESC Sec.II** 为准。实现在此只覆盖本 crate 需要的部分：
//!   帧头、datagram 头、一帧多 datagram（M 位）、寻址模式与 Working Counter。
//!
//! 设计边界：
//! - 本 crate **不**持有状态（不模拟从站）；只做无状态的字节 <-> 结构转换与判定规则。
//! - 所有长度/越界检查都返回错误，不 panic（除显式的 `set_ethertype` 夹具有）。
//! - WKC 的"每个匹配 +N"由调用方按从站/FMMU 匹配次数累加；这里只给出每条命令的单次增量。

/// EtherCAT 帧的 EtherType（IANA 分配 0x88A4）。
pub const ETHERCAT_ETHERTYPE: u16 = 0x88A4;

/// 以太网帧头长度（不含 FCS）。
pub const ETHERNET_HEADER_LEN: usize = 14;

/// EtherCAT 帧头长度（2 字节：Length 11 bit + Reserved + Type 4 bit）。
pub const ECAT_HEADER_LEN: usize = 2;

/// datagram 头长度（10 字节）+ 2 字节 WKC。
pub const DATAGRAM_HEADER_LEN: usize = 10;

/// 单条 datagram 的数据长度上限（Length 字段 11 bit）。
pub const MAX_DATAGRAM_DATA_LEN: usize = 2047;

/// 承载 EtherCAT 的最小以太网帧长度：14 字节 MAC 头 + 2 字节 EtherCAT 帧头。
pub const MIN_ETHERCAT_FRAME_LEN: usize = ETHERNET_HEADER_LEN + ECAT_HEADER_LEN;

/// 帧头 Type 字段中表示 "EtherCAT 命令帧" 的取值。
pub const FRAME_TYPE_ECAT_COMMAND: u8 = 1;

/// 读取以太网帧的 EtherType；帧过短时返回 [`None`]。
#[must_use]
pub fn ethertype(frame: &[u8]) -> Option<u16> {
    if frame.len() < ETHERNET_HEADER_LEN {
        return None;
    }
    Some(u16::from_be_bytes([frame[12], frame[13]]))
}

/// 判断以太网帧是否承载 EtherCAT（EtherType == 0x88A4 且长度足够）。
#[must_use]
pub fn is_ethercat_frame(frame: &[u8]) -> bool {
    frame.len() >= MIN_ETHERCAT_FRAME_LEN && ethertype(frame) == Some(ETHERCAT_ETHERTYPE)
}

/// 把 EtherType 写入以太网帧头（测试夹具/构造用）。
///
/// # Panics
/// 帧短于 [`ETHERNET_HEADER_LEN`] 时 panic（夹具路径，调用方保证长度）。
pub fn set_ethertype(frame: &mut [u8], ethertype: u16) {
    assert!(
        frame.len() >= ETHERNET_HEADER_LEN,
        "ethernet frame too short"
    );
    frame[12..14].copy_from_slice(&ethertype.to_be_bytes());
}

/// 线缆层错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    /// 数据短于一个完整的帧头/datagram 头。
    TooShort { need: usize, got: usize },
    /// 以太网帧的 EtherType 不是 0x88A4。
    ForeignEtherType(u16),
    /// 帧头 Type 不是 EtherCAT 命令帧。
    UnsupportedFrameType(u8),
    /// 帧头声明的长度与实际剩余字节数不一致。
    LengthMismatch { declared: usize, actual: usize },
    /// datagram 头声明的数据长度越过了帧尾。
    DatagramOverrun {
        offset: usize,
        declared: usize,
        remaining: usize,
    },
    /// 单条 datagram 超过 11 bit 长度上限。
    DatagramTooLong(usize),
    /// 未知命令码。
    UnknownCommand(u8),
    /// 一帧的 datagram 总长度超过 11 bit 帧长度上限。
    FrameTooLong(usize),
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort { need, got } => write!(f, "frame too short: need {need}, got {got}"),
            Self::ForeignEtherType(t) => write!(f, "not an EtherCAT frame: EtherType 0x{t:04X}"),
            Self::UnsupportedFrameType(t) => write!(f, "unsupported EtherCAT frame type {t}"),
            Self::LengthMismatch { declared, actual } => {
                write!(
                    f,
                    "frame length mismatch: declared {declared}, actual {actual}"
                )
            }
            Self::DatagramOverrun {
                offset,
                declared,
                remaining,
            } => write!(
                f,
                "datagram at {offset} declares {declared} bytes but only {remaining} remain"
            ),
            Self::DatagramTooLong(n) => write!(
                f,
                "datagram data too long: {n} bytes (max {MAX_DATAGRAM_DATA_LEN})"
            ),
            Self::UnknownCommand(c) => write!(f, "unknown EtherCAT command 0x{c:02X}"),
            Self::FrameTooLong(n) => write!(f, "ethercat payload too long: {n} bytes"),
        }
    }
}

impl std::error::Error for WireError {}

/// EtherCAT 命令码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Nop = 0x00,
    Aprd = 0x01,
    Apwr = 0x02,
    Aprw = 0x03,
    Fprd = 0x04,
    Fpwr = 0x05,
    Fprw = 0x06,
    Brd = 0x07,
    Bwr = 0x08,
    Brw = 0x09,
    Lrd = 0x0A,
    Lwr = 0x0B,
    Lrw = 0x0C,
    Armw = 0x0D,
    Frmw = 0x0E,
}

impl Command {
    /// 从命令码解析；未知码返回 [`None`]。
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x00 => Self::Nop,
            0x01 => Self::Aprd,
            0x02 => Self::Apwr,
            0x03 => Self::Aprw,
            0x04 => Self::Fprd,
            0x05 => Self::Fpwr,
            0x06 => Self::Fprw,
            0x07 => Self::Brd,
            0x08 => Self::Bwr,
            0x09 => Self::Brw,
            0x0A => Self::Lrd,
            0x0B => Self::Lwr,
            0x0C => Self::Lrw,
            0x0D => Self::Armw,
            0x0E => Self::Frmw,
            _ => return None,
        })
    }

    /// 命令码的数值。
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// 是否读取从站数据（帧 -> 主站）。
    #[must_use]
    pub fn is_read(self) -> bool {
        matches!(
            self,
            Self::Aprd
                | Self::Fprd
                | Self::Brd
                | Self::Lrd
                | Self::Aprw
                | Self::Fprw
                | Self::Brw
                | Self::Lrw
                | Self::Armw
                | Self::Frmw
        )
    }

    /// 是否写入从站数据（主站 -> 帧）。
    #[must_use]
    pub fn is_write(self) -> bool {
        matches!(
            self,
            Self::Apwr
                | Self::Fpwr
                | Self::Bwr
                | Self::Lwr
                | Self::Aprw
                | Self::Fprw
                | Self::Brw
                | Self::Lrw
                | Self::Armw
                | Self::Frmw
        )
    }

    /// 每条命中该从站的 datagram 应给 Working Counter 增加的数值。
    ///
    /// 规则（ETG.1000.4）：读命中 +1、写命中 +1、读+写命中 +2。
    ///
    /// ⚠️ **逻辑寻址的 LRW 是例外**：两家主站的期望是"写 FMMU 命中 +2、读 FMMU 命中 +1"
    /// （IgH `master/datagram_pair.c`：`expected_working_counter = used[OUTPUT] * 2 +
    /// used[INPUT]`；SOEM `src/ec_main.c` 对 LWR 也乘 2 以模拟 LRW），因此 LRW 不能直接用
    /// 本函数的结果，由 `esc-node` 按 FMMU 命中方向分别累加。
    #[must_use]
    pub fn wkc_increment(self) -> u16 {
        u16::from(self.is_read()) + u16::from(self.is_write())
    }

    /// 该命令使用的寻址方式。
    #[must_use]
    pub fn addressing(self) -> Addressing {
        match self {
            Self::Nop => Addressing::None,
            Self::Aprd | Self::Apwr | Self::Aprw | Self::Armw => Addressing::AutoIncrement,
            Self::Fprd | Self::Fpwr | Self::Fprw | Self::Frmw => Addressing::Configured,
            Self::Brd | Self::Bwr | Self::Brw => Addressing::Broadcast,
            Self::Lrd | Self::Lwr | Self::Lrw => Addressing::Logical,
        }
    }
}

/// datagram 的寻址方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addressing {
    None,
    /// ADP 为自增位置。
    AutoIncrement,
    /// ADP 为配置站地址（含别名匹配）。
    Configured,
    /// 广播（地址字段为 0）。
    Broadcast,
    /// ADP+ADO 组成 32 位逻辑地址。
    Logical,
}

/// 解析后的一条 datagram（零拷贝借用帧缓冲区）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Datagram<'a> {
    pub command: Command,
    /// 主站用于匹配请求/响应的序号。
    pub index: u8,
    /// 地址低 16 位（自增位置 / 配置站地址 / 逻辑地址低半）。
    pub address_low: u16,
    /// 地址高 16 位（寄存器偏移 / 逻辑地址高半）。
    pub address_high: u16,
    /// datagram 头的 M（More）位：后面还有 datagram。
    pub more: bool,
    /// IRQ 字段（从站在事件命中时回写；模拟器通常置 0）。
    pub interrupt: u16,
    /// 数据区。
    pub data: &'a [u8],
    /// 工作计数器。
    pub working_counter: u16,
}

impl Datagram<'_> {
    /// 逻辑地址（`address_low` + `address_high` 组成 32 位小端地址）。
    #[must_use]
    pub fn logical_address(&self) -> u32 {
        u32::from(self.address_low) | (u32::from(self.address_high) << 16)
    }

    /// 寄存器偏移（ADR 高 16 位）。
    #[must_use]
    pub fn register_offset(&self) -> u16 {
        self.address_high
    }

    /// 该 datagram 数据区长度。
    #[must_use]
    pub fn data_len(&self) -> usize {
        self.data.len()
    }
}

/// 解析后的 EtherCAT 负载（帧头 + 一条或多条 datagram）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame<'a> {
    /// 帧头 Length 字段声明的值（不含 2 字节帧头）。
    pub declared_len: u16,
    /// 帧头 Type 字段。
    pub frame_type: u8,
    /// datagram 列表（至少一条；一帧多 datagram 由 M 位串联）。
    pub datagrams: Vec<Datagram<'a>>,
}

impl<'a> Frame<'a> {
    /// 解析以太网帧（跳过 14 字节以太网头，校验 EtherType）。
    pub fn parse_ethernet(frame: &'a [u8]) -> Result<Self, WireError> {
        if frame.len() < ETHERNET_HEADER_LEN {
            return Err(WireError::TooShort {
                need: ETHERNET_HEADER_LEN,
                got: frame.len(),
            });
        }
        let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
        if ethertype != ETHERCAT_ETHERTYPE {
            return Err(WireError::ForeignEtherType(ethertype));
        }
        Self::parse(&frame[ETHERNET_HEADER_LEN..])
    }

    /// 解析 EtherCAT 负载（不含以太网头）。
    ///
    /// 遵循 M 位串联 datagram；帧头 Length 必须与实际数据完全一致。
    pub fn parse(payload: &'a [u8]) -> Result<Self, WireError> {
        if payload.len() < ECAT_HEADER_LEN {
            return Err(WireError::TooShort {
                need: ECAT_HEADER_LEN,
                got: payload.len(),
            });
        }
        let header = u16::from_le_bytes([payload[0], payload[1]]);
        let declared_len = (header & 0x07FF) as usize;
        let frame_type = ((header >> 12) & 0x000F) as u8;
        if frame_type != FRAME_TYPE_ECAT_COMMAND {
            return Err(WireError::UnsupportedFrameType(frame_type));
        }
        let actual = payload.len() - ECAT_HEADER_LEN;
        if declared_len != actual {
            return Err(WireError::LengthMismatch {
                declared: declared_len,
                actual,
            });
        }

        let mut datagrams = Vec::new();
        let mut offset = ECAT_HEADER_LEN;
        loop {
            if payload.len() - offset < DATAGRAM_HEADER_LEN + 2 {
                return Err(WireError::TooShort {
                    need: DATAGRAM_HEADER_LEN + 2,
                    got: payload.len() - offset,
                });
            }
            let command = Command::from_u8(payload[offset])
                .ok_or(WireError::UnknownCommand(payload[offset]))?;
            let index = payload[offset + 1];
            let address_low = u16::from_le_bytes([payload[offset + 2], payload[offset + 3]]);
            let address_high = u16::from_le_bytes([payload[offset + 4], payload[offset + 5]]);
            let len_flags = u16::from_le_bytes([payload[offset + 6], payload[offset + 7]]);
            let data_len = (len_flags & 0x07FF) as usize;
            let more = (len_flags & 0x8000) != 0;
            let interrupt = u16::from_le_bytes([payload[offset + 8], payload[offset + 9]]);

            let data_start = offset + DATAGRAM_HEADER_LEN;
            let wkc_start = data_start + data_len;
            if data_len > MAX_DATAGRAM_DATA_LEN || payload.len() < wkc_start + 2 {
                return Err(WireError::DatagramOverrun {
                    offset,
                    declared: data_len,
                    remaining: payload.len().saturating_sub(data_start),
                });
            }
            let data = &payload[data_start..wkc_start];
            let working_counter = u16::from_le_bytes([payload[wkc_start], payload[wkc_start + 1]]);

            datagrams.push(Datagram {
                command,
                index,
                address_low,
                address_high,
                more,
                interrupt,
                data,
                working_counter,
            });

            offset = wkc_start + 2;
            if !more {
                break;
            }
            if offset >= payload.len() {
                return Err(WireError::TooShort {
                    need: DATAGRAM_HEADER_LEN,
                    got: 0,
                });
            }
        }

        if offset != payload.len() {
            return Err(WireError::LengthMismatch {
                declared: offset,
                actual: payload.len(),
            });
        }

        Ok(Self {
            declared_len: declared_len as u16,
            frame_type,
            datagrams,
        })
    }

    /// 帧内 datagram 条数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.datagrams.len()
    }

    /// 是否为空帧（合法帧至少一条 datagram，因此正常解析结果恒为 `false`）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.datagrams.is_empty()
    }
}

/// 构造用的一条 datagram（拥有数据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatagramData {
    pub command: Command,
    pub index: u8,
    pub address_low: u16,
    pub address_high: u16,
    pub interrupt: u16,
    pub data: Vec<u8>,
    pub working_counter: u16,
}

impl DatagramData {
    /// 便捷构造：自动自增寻址、无中断、WKC=0。
    #[must_use]
    pub fn auto_increment(command: Command, index: u8, adp: u16, ado: u16, data: Vec<u8>) -> Self {
        Self {
            command,
            index,
            address_low: adp,
            address_high: ado,
            interrupt: 0,
            data,
            working_counter: 0,
        }
    }

    /// 便捷构造：逻辑寻址。
    #[must_use]
    pub fn logical(command: Command, index: u8, address: u32, data: Vec<u8>) -> Self {
        Self {
            command,
            index,
            address_low: (address & 0xFFFF) as u16,
            address_high: (address >> 16) as u16,
            interrupt: 0,
            data,
            working_counter: 0,
        }
    }
}

/// 把 datagram 列表编码成 EtherCAT 负载（帧头 + datagrams），并正确设置 M 位。
pub fn encode_frame(datagrams: &[DatagramData]) -> Result<Vec<u8>, WireError> {
    let mut total = ECAT_HEADER_LEN;
    for d in datagrams {
        if d.data.len() > MAX_DATAGRAM_DATA_LEN {
            return Err(WireError::DatagramTooLong(d.data.len()));
        }
        total += DATAGRAM_HEADER_LEN + d.data.len() + 2;
    }
    if total - ECAT_HEADER_LEN > MAX_DATAGRAM_DATA_LEN {
        return Err(WireError::FrameTooLong(total));
    }

    let mut out = Vec::with_capacity(total);
    let header = ((FRAME_TYPE_ECAT_COMMAND as u16) << 12) | ((total - ECAT_HEADER_LEN) as u16);
    out.extend_from_slice(&header.to_le_bytes());

    let last = datagrams.len().saturating_sub(1);
    for (i, d) in datagrams.iter().enumerate() {
        let more = u16::from(i != last) << 15;
        out.push(d.command.as_u8());
        out.push(d.index);
        out.extend_from_slice(&d.address_low.to_le_bytes());
        out.extend_from_slice(&d.address_high.to_le_bytes());
        out.extend_from_slice(&(more | d.data.len() as u16).to_le_bytes());
        out.extend_from_slice(&d.interrupt.to_le_bytes());
        out.extend_from_slice(&d.data);
        out.extend_from_slice(&d.working_counter.to_le_bytes());
    }
    Ok(out)
}

/// 自增寻址的处理结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoIncrementStep {
    /// 本从站是否应当处理该 datagram。
    pub process: bool,
    /// 转发给下一个从站时 ADP 应取的值。
    pub forward_adp: u16,
}

/// 按 EtherCAT 自增寻址规则处理一次 ADP。
///
/// 规则（ESC Sec.II / ETG.1000.4，2026-11-12 用两份未修改主站源码复核）：
/// 主站把 ADP 写成**环位置的相反数**（16 位二补数），每个从站在转发前把 ADP 加 1；
/// 入站 ADP 为 0 的从站执行命令。一帧沿环每经过一个从站加 1，因此
/// ADP = -(N) 命中环上第 N 个从站（0 起算）。
///
/// 证据：
/// - IgH `master/datagram.c`：`ec_datagram_aprd/apwr` 写入 `(int16_t) ring_position * (-1)`；
/// - SOEM `src/ec_config.c`：`ADPh = (uint16)(1 - slave)`（1 起算的从站号）。
///
/// 早先实现（"ADP==1 执行、转发时减 1"）在真实主站上会整体偏移一个从站，已修正。
#[must_use]
pub fn auto_increment_step(incoming_adp: u16) -> AutoIncrementStep {
    AutoIncrementStep {
        process: incoming_adp == 0,
        forward_adp: incoming_adp.wrapping_add(1),
    }
}

/// 配置地址寻址：配置站地址或站别名任一匹配即命中。
#[must_use]
pub fn matches_configured_address(
    incoming_adp: u16,
    station_address: u16,
    station_alias: u16,
) -> bool {
    incoming_adp != 0 && (incoming_adp == station_address || incoming_adp == station_alias)
}

/// 逻辑寻址：FMMU 的 `[start, start+len)` 与 datagram 的 `[address, address+len)` 是否相交。
#[must_use]
pub fn logical_range_intersects(fmmu_start: u32, fmmu_len: u32, address: u32, len: u32) -> bool {
    if fmmu_len == 0 || len == 0 {
        return false;
    }
    let a_start = fmmu_start;
    let a_end = fmmu_start.saturating_add(fmmu_len);
    let b_start = address;
    let b_end = address.saturating_add(len);
    b_start < a_end && a_start < b_end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ethernet_frame(ethertype: u16) -> Vec<u8> {
        let mut frame = vec![0u8; MIN_ETHERCAT_FRAME_LEN];
        set_ethertype(&mut frame, ethertype);
        frame
    }

    #[test]
    fn accepts_ethercat_frame() {
        let frame = ethernet_frame(ETHERCAT_ETHERTYPE);
        assert_eq!(ethertype(&frame), Some(ETHERCAT_ETHERTYPE));
        assert!(is_ethercat_frame(&frame));
    }

    #[test]
    fn rejects_foreign_ethertype() {
        // Spike 1 踩坑：TAP 上会混入 0x86DD（IPv6）等内核帧。
        assert!(!is_ethercat_frame(&ethernet_frame(0x86DD)));
        assert!(!is_ethercat_frame(&ethernet_frame(0x0800)));
    }

    #[test]
    fn rejects_short_frames() {
        assert_eq!(ethertype(&[]), None);
        assert_eq!(ethertype(&[0u8; 13]), None);
        let mut short = ethernet_frame(ETHERCAT_ETHERTYPE);
        short.truncate(15);
        assert!(!is_ethercat_frame(&short));
    }

    #[test]
    fn roundtrip_single_datagram() {
        let d = DatagramData::auto_increment(Command::Aprd, 7, 1, 0x0130, vec![0u8; 4]);
        let payload = encode_frame(&[d]).expect("encode");
        let frame = Frame::parse(&payload).expect("parse");
        assert_eq!(frame.len(), 1);
        let dg = &frame.datagrams[0];
        assert_eq!(dg.command, Command::Aprd);
        assert_eq!(dg.index, 7);
        assert_eq!(dg.address_low, 1);
        assert_eq!(dg.register_offset(), 0x0130);
        assert!(!dg.more);
        assert_eq!(dg.data_len(), 4);
    }

    #[test]
    fn roundtrip_multi_datagram_sets_more_bits() {
        let a = DatagramData::auto_increment(Command::Apwr, 1, 1, 0x0120, vec![0x02, 0x00]);
        let b = DatagramData::auto_increment(Command::Aprd, 2, 1, 0x0130, vec![0x00; 2]);
        let payload = encode_frame(&[a, b]).expect("encode");
        // 第一条的 M 位应为 1，第二条为 0。
        let first_len_flags =
            u16::from_le_bytes([payload[ECAT_HEADER_LEN + 6], payload[ECAT_HEADER_LEN + 7]]);
        assert_ne!(
            first_len_flags & 0x8000,
            0,
            "first datagram must set More bit"
        );
        let frame = Frame::parse(&payload).expect("parse");
        assert_eq!(frame.len(), 2);
        assert!(frame.datagrams[0].more);
        assert!(!frame.datagrams[1].more);
    }

    #[test]
    fn rejects_length_mismatch() {
        let d = DatagramData::auto_increment(Command::Brd, 1, 0, 0x0000, vec![0u8; 2]);
        let mut payload = encode_frame(&[d]).expect("encode");
        payload.push(0xAA); // 尾部多一字节，但帧头长度未改
        match Frame::parse(&payload) {
            Err(WireError::LengthMismatch { .. }) => {}
            other => panic!("expected LengthMismatch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_datagram_overrun() {
        let d = DatagramData::auto_increment(Command::Brd, 1, 0, 0x0000, vec![0u8; 8]);
        let mut payload = encode_frame(&[d]).expect("encode");
        // 把帧头长度改小，使 datagram 数据区越过帧尾
        let declared = 6u16;
        let header = ((FRAME_TYPE_ECAT_COMMAND as u16) << 12) | declared;
        payload[0..2].copy_from_slice(&header.to_le_bytes());
        payload.truncate(ECAT_HEADER_LEN + declared as usize);
        match Frame::parse(&payload) {
            Err(WireError::DatagramOverrun { .. }) | Err(WireError::TooShort { .. }) => {}
            other => panic!("expected overrun/short error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_command() {
        let d = DatagramData::auto_increment(Command::Brd, 1, 0, 0, vec![0u8; 2]);
        let mut payload = encode_frame(&[d]).expect("encode");
        payload[ECAT_HEADER_LEN] = 0x7F;
        match Frame::parse(&payload) {
            Err(WireError::UnknownCommand(0x7F)) => {}
            other => panic!("expected UnknownCommand, got {other:?}"),
        }
    }

    #[test]
    fn rejects_foreign_ethertype_on_parse() {
        let d = DatagramData::auto_increment(Command::Brd, 1, 0, 0, vec![0u8; 2]);
        let payload = encode_frame(&[d]).expect("encode");
        let mut eth = vec![0u8; ETHERNET_HEADER_LEN];
        set_ethertype(&mut eth, 0x0800);
        eth.extend_from_slice(&payload);
        match Frame::parse_ethernet(&eth) {
            Err(WireError::ForeignEtherType(0x0800)) => {}
            other => panic!("expected ForeignEtherType, got {other:?}"),
        }
    }

    #[test]
    fn parses_ethernet_frame_roundtrip() {
        let d = DatagramData::logical(Command::Lrw, 3, 0x0001_0000, vec![1, 2, 3, 4]);
        let payload = encode_frame(&[d]).expect("encode");
        let mut eth = vec![0u8; ETHERNET_HEADER_LEN];
        set_ethertype(&mut eth, ETHERCAT_ETHERTYPE);
        eth.extend_from_slice(&payload);
        let frame = Frame::parse_ethernet(&eth).expect("parse ethernet");
        assert_eq!(frame.datagrams[0].command, Command::Lrw);
        assert_eq!(frame.datagrams[0].logical_address(), 0x0001_0000);
        assert_eq!(frame.datagrams[0].data, &[1, 2, 3, 4]);
    }

    #[test]
    fn wkc_increment_rules() {
        assert_eq!(Command::Nop.wkc_increment(), 0);
        assert_eq!(Command::Aprd.wkc_increment(), 1);
        assert_eq!(Command::Apwr.wkc_increment(), 1);
        assert_eq!(Command::Aprw.wkc_increment(), 2);
        assert_eq!(Command::Brd.wkc_increment(), 1);
        // 这里是"命令自身读/写位之和"；逻辑寻址 LRW 的实际增量由 FMMU 方向决定
        //（写命中 +2、读命中 +1），见 `wkc_increment` 文档，由 esc-node 处理。
        assert_eq!(Command::Lrw.wkc_increment(), 2);
        assert_eq!(Command::Armw.wkc_increment(), 2);
        // 所有命令的读/写属性与增量一致
        for code in 0u8..=0x0E {
            let cmd = Command::from_u8(code).expect("known command");
            assert_eq!(
                cmd.wkc_increment(),
                u16::from(cmd.is_read()) + u16::from(cmd.is_write())
            );
        }
    }

    #[test]
    fn addressing_classification() {
        assert_eq!(Command::Aprd.addressing(), Addressing::AutoIncrement);
        assert_eq!(Command::Fprd.addressing(), Addressing::Configured);
        assert_eq!(Command::Brd.addressing(), Addressing::Broadcast);
        assert_eq!(Command::Lrd.addressing(), Addressing::Logical);
        assert_eq!(Command::Nop.addressing(), Addressing::None);
    }

    #[test]
    fn auto_increment_increments_and_processes_at_zero() {
        // 环上第一个从站：主站 ADP = -(0) = 0。
        let step = auto_increment_step(0);
        assert!(step.process);
        assert_eq!(step.forward_adp, 1);
        // 环上第二个从站：主站 ADP = -(1) = 0xFFFF，第一个从站转发后变为 0。
        let step = auto_increment_step(0xFFFF);
        assert!(!step.process);
        assert_eq!(step.forward_adp, 0);
        let step = auto_increment_step(0xFFFE);
        assert!(!step.process);
        assert_eq!(step.forward_adp, 0xFFFF);
        // 转发是 16 位回绕（不能饱和）。
        assert_eq!(auto_increment_step(0xFFFF).forward_adp, 0);
    }

    #[test]
    fn configured_and_alias_matching() {
        assert!(matches_configured_address(0x1001, 0x1001, 0));
        assert!(matches_configured_address(0x0042, 0x1001, 0x0042));
        assert!(!matches_configured_address(0x1001, 0x1002, 0));
        assert!(!matches_configured_address(0, 0, 0));
    }

    #[test]
    fn logical_range_intersections() {
        assert!(logical_range_intersects(0x1000, 16, 0x1008, 4));
        assert!(logical_range_intersects(0x1000, 16, 0x0FF0, 32));
        assert!(!logical_range_intersects(0x1000, 16, 0x2000, 4));
        assert!(!logical_range_intersects(0x1000, 0, 0x1000, 4));
        assert!(!logical_range_intersects(0x1000, 16, 0x1010, 0));
    }

    #[test]
    fn frame_too_long_is_rejected() {
        // 两条各 2047 字节的 datagram 会超过 11 bit 帧长度上限。
        let a =
            DatagramData::auto_increment(Command::Brd, 1, 0, 0, vec![0u8; MAX_DATAGRAM_DATA_LEN]);
        let b =
            DatagramData::auto_increment(Command::Brd, 2, 0, 0, vec![0u8; MAX_DATAGRAM_DATA_LEN]);
        assert!(matches!(
            encode_frame(&[a, b]),
            Err(WireError::FrameTooLong(_))
        ));
    }
}
