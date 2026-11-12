//! AL（Application Layer）状态机骨架。
//!
//! 取值与转换规则以 **ETG.1000.6 / ESC Sec.II** 为准；B5 实现 INIT→PREOP→SAFEOP→OP
//! 的请求/应答握手与错误位 ACK。

use core::fmt;

/// EtherCAT 从站 AL 状态。
///
/// 状态编码与 ESC 的 AL Status 寄存器表示一致（B5 落地寄存器读写时复核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlState {
    /// INIT：初始态，仅邮箱可用（无过程数据）。
    Init,
    /// PRE-OP：邮箱可用，过程数据未映射。
    PreOp,
    /// SAFE-OP：过程数据输入有效，输出保持安全值。
    SafeOp,
    /// BOOT：固件升级态。
    Boot,
    /// OP：正常操作态。
    Op,
}

impl AlState {
    /// AL 状态码（1/2/4/3/8，对应 INIT/PRE-OP/SAFE-OP/BOOT/OP）。
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Init => 1,
            Self::PreOp => 2,
            Self::SafeOp => 4,
            Self::Boot => 3,
            Self::Op => 8,
        }
    }

    /// 由 AL 状态码反解；未知编码返回 [`None`]（调用方按规范置错误位）。
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Init),
            2 => Some(Self::PreOp),
            3 => Some(Self::Boot),
            4 => Some(Self::SafeOp),
            8 => Some(Self::Op),
            _ => None,
        }
    }
}

impl fmt::Display for AlState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Init => "INIT",
            Self::PreOp => "PRE-OP",
            Self::SafeOp => "SAFE-OP",
            Self::Boot => "BOOT",
            Self::Op => "OP",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::AlState;

    #[test]
    fn state_codes_round_trip() {
        for state in [
            AlState::Init,
            AlState::PreOp,
            AlState::SafeOp,
            AlState::Boot,
            AlState::Op,
        ] {
            assert_eq!(AlState::from_code(state.code()), Some(state));
        }
    }

    #[test]
    fn unknown_code_is_none() {
        for code in [0u8, 5, 6, 7, 9, 0xFF] {
            assert_eq!(AlState::from_code(code), None);
        }
    }

    #[test]
    fn codes_are_distinct() {
        let mut codes: Vec<u8> = [
            AlState::Init,
            AlState::PreOp,
            AlState::SafeOp,
            AlState::Boot,
            AlState::Op,
        ]
        .iter()
        .map(|s| s.code())
        .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 5);
    }
}
