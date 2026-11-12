//! ESC 内核错误类型。
//!
//! B1 只保留最小集合；B5 起按寄存器/FMMU/SM 的规范校验细化（并映射到 AL 错误位）。

use core::fmt;

/// ESC 内核错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscError {
    /// 状态转换未满足规范前置条件（如未经 PRE-OP 直接请求 OP）。
    IllegalTransition,
}

impl fmt::Display for EscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IllegalTransition => f.write_str("illegal AL state transition"),
        }
    }
}

impl std::error::Error for EscError {}
