//! AL Status Code（AL Status 寄存器 0x0134 的取值）。
//!
//! 码值与文本按 **ETG.1000.6 Table 11**；本模块只落地 ESM 前置条件检查与既有看门狗
//! 用到的子集，`name()` 供日志/诊断使用。交叉核对来源（只读参考，未复制代码）：
//! - IgH 1.6.13 `master/fsm_change.c: al_status_messages[]`（GPLv2 主站源码，只读参考）；
//! - ethercrab `al_status_code.rs`（MIT OR Apache-2.0 公开 crate 文档）；
//! - ABB AC500 EtherCAT 诊断文档「AL Status → Error codes」公开表格。
//!
//! ⚠️ `0x0015` 与 `0x0016` 文本同为 "Invalid mailbox configuration"，两者在规范中的区分
//! 细节（邮箱输入/输出方向）未从公开资料核实：本模拟器统一回 `0x0015`，保留
//! [`INVALID_MAILBOX_CONFIGURATION_OUT`] 常量但暂不使用。

/// 无错误。
pub const NO_ERROR: u16 = 0x0000;
/// 未指定错误。
pub const UNSPECIFIED_ERROR: u16 = 0x0001;
/// 非法请求的状态迁移（状态位合法但迁移不被允许）。
pub const INVALID_STATE_CHANGE: u16 = 0x0011;
/// 未知请求状态（AL Control 状态位不是任何合法状态编码）。
pub const UNKNOWN_REQUESTED_STATE: u16 = 0x0012;
/// 不支持 Bootstrap（BOOT）状态。
pub const BOOTSTRAP_NOT_SUPPORTED: u16 = 0x0013;
/// 邮箱配置无效（INIT → PREOP 前置条件失败）。
pub const INVALID_MAILBOX_CONFIGURATION: u16 = 0x0015;
/// 邮箱配置无效（规范中的第二个同文本码；⚠️ 与 `0x0015` 的方向区分待核实，暂不使用）。
pub const INVALID_MAILBOX_CONFIGURATION_OUT: u16 = 0x0016;
/// Sync Manager 配置无效（PREOP → SAFEOP 前置条件失败）。
pub const INVALID_SYNC_MANAGER_CONFIGURATION: u16 = 0x0017;
/// 无有效输入数据。
pub const NO_VALID_INPUTS: u16 = 0x0018;
/// 无有效输出数据。
pub const NO_VALID_OUTPUTS: u16 = 0x0019;
/// SM 过程数据看门狗超时（见 [`crate::watchdog`]）。
pub const SYNC_MANAGER_WATCHDOG: u16 = 0x001B;
/// 无有效的输出配置（OP 前置条件；⚠️ 文本按 ETG.1000.6 Table 11 常见解释，待原文复核）。
pub const NO_VALID_OUTPUT_CONFIGURATION: u16 = 0x0024;
/// 无有效的输入配置（OP 前置条件；⚠️ 同上）。
pub const NO_VALID_INPUT_CONFIGURATION: u16 = 0x0025;
/// 无有效输入与输出。
pub const NO_VALID_INPUTS_AND_OUTPUTS: u16 = 0x002B;

/// AL Status Code 的可读名称（未知码返回 `"unknown"`）。
///
/// 只覆盖本模拟器会写入的码；其余码按 ETG.1000.6 Table 11 由主站自行解释。
#[must_use]
pub const fn name(code: u16) -> &'static str {
    match code {
        NO_ERROR => "No error",
        UNSPECIFIED_ERROR => "Unspecified error",
        INVALID_STATE_CHANGE => "Invalid requested state change",
        UNKNOWN_REQUESTED_STATE => "Unknown requested state",
        BOOTSTRAP_NOT_SUPPORTED => "Bootstrap not supported",
        INVALID_MAILBOX_CONFIGURATION => "Invalid mailbox configuration",
        INVALID_MAILBOX_CONFIGURATION_OUT => "Invalid mailbox configuration",
        INVALID_SYNC_MANAGER_CONFIGURATION => "Invalid sync manager configuration",
        NO_VALID_INPUTS => "No valid inputs available",
        NO_VALID_OUTPUTS => "No valid outputs",
        NO_VALID_OUTPUT_CONFIGURATION => "No valid output configuration",
        NO_VALID_INPUT_CONFIGURATION => "No valid input configuration",
        SYNC_MANAGER_WATCHDOG => "Sync manager watchdog",
        NO_VALID_INPUTS_AND_OUTPUTS => "No valid inputs and outputs",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_etg_table_11() {
        assert_eq!(NO_ERROR, 0x0000);
        assert_eq!(INVALID_STATE_CHANGE, 0x0011);
        assert_eq!(UNKNOWN_REQUESTED_STATE, 0x0012);
        assert_eq!(BOOTSTRAP_NOT_SUPPORTED, 0x0013);
        assert_eq!(INVALID_MAILBOX_CONFIGURATION, 0x0015);
        assert_eq!(INVALID_MAILBOX_CONFIGURATION_OUT, 0x0016);
        assert_eq!(INVALID_SYNC_MANAGER_CONFIGURATION, 0x0017);
        assert_eq!(NO_VALID_INPUTS, 0x0018);
        assert_eq!(NO_VALID_OUTPUTS, 0x0019);
        assert_eq!(NO_VALID_OUTPUT_CONFIGURATION, 0x0024);
        assert_eq!(NO_VALID_INPUT_CONFIGURATION, 0x0025);
        assert_eq!(SYNC_MANAGER_WATCHDOG, 0x001B);
        assert_eq!(NO_VALID_INPUTS_AND_OUTPUTS, 0x002B);
    }

    #[test]
    fn names_are_known_for_all_used_codes() {
        for code in [
            NO_ERROR,
            INVALID_STATE_CHANGE,
            UNKNOWN_REQUESTED_STATE,
            BOOTSTRAP_NOT_SUPPORTED,
            INVALID_MAILBOX_CONFIGURATION,
            INVALID_SYNC_MANAGER_CONFIGURATION,
            NO_VALID_INPUTS,
            NO_VALID_OUTPUTS,
            SYNC_MANAGER_WATCHDOG,
        ] {
            assert_ne!(name(code), "unknown", "code {code:#06x} 缺少名称");
        }
        assert_eq!(name(0x7FFF), "unknown");
    }
}
