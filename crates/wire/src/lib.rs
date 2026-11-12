//! 线缆层：EtherCAT 以太网帧与 datagram 编解码。
//!
//! 依据：
//! - EtherType `0x88A4`（IANA 分配）——Spike 1 已在 Linux TAP 上双向验证
//!   （[docs/spikes/spike-1-tap-bidirectional.md]）。
//! - 帧/datagram 字段以 **ETG.1000.4 / ESC Sec.II** 为准；B4 实现 datagram 头解析、
//!   一帧多 datagram（M 位）、ADP/FPRD/BRD/LRD 地址匹配与严格 WKC 语义。
//!
//! B1 先落地"帧级过滤"这一最小可测原语，供线缆层接收路径按 EtherType 丢包
//! （Spike 1 踩坑 #2：TAP 上会混入内核的 IPv6 MLD 等帧，必须按 EtherType 过滤）。

/// EtherCAT 帧的 EtherType（IANA 分配，0x88A4）。
pub const ETHERCAT_ETHERTYPE: u16 = 0x88A4;

/// 承载 EtherCAT 的最小以太网帧长度：14 字节 MAC 头 + 2 字节 EtherCAT frame header。
pub const MIN_ETHERCAT_FRAME_LEN: usize = 16;

/// 以太网帧头长度（不含 FCS）。
pub const ETHERNET_HEADER_LEN: usize = 14;

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

/// 把 EtherType 写入以太网帧头（调试/夹具用，B4 接入真实编解码后移除）。
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
        // Spike 1 踩坑 #2：TAP 上会混入 0x86DD（IPv6）等内核帧。
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
}
