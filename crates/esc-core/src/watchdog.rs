//! SM 过程数据看门狗（Watchdog）寄存器与超时语义。
//!
//! 依据 **ESC Sec.II（Watchdog）** 与两家主站的实现复核（只读参考、未复制代码）：
//! - IgH 1.6.13 `include/ecrt.h: ecrt_slave_config_watchdog()`：`watchdog_divider`
//!   是"40 ns 间隔的个数"（寄存器 **0x0400**），作为所有从站看门狗的基数；
//!   `watchdog_intervals` 是同步管理器看门狗的基础间隔数（寄存器 **0x0420**）；
//! - IgH `master/fsm_slave_config.c`：仅在应用显式配置非 0 值时写 0x0400 / 0x0420；
//! - IgH `master/sync.c`：SM **control bit6** = 看门狗使能（`EC_WD_ENABLE`）。
//!
//! 因此**过程数据看门狗超时** = `0x0400 × 0x0420 × 40 ns`；超时后 ESC 置 AL Status
//! error 位、AL Status Code = **0x001B（Sync manager watchdog）**，并从 OP 退回
//! SAFEOP+ERROR，等待主站写 Error ACK（ETG.1000.6 AL Status Codes）。
//!
//! - **0x0410（Watchdog Time PDI）**：寄存器可读写保存（透传），但模拟器没有 PDI，因此
//!   **不模拟 PDI 看门狗超时**（不递减、不置 AL error）——需要 PDI 行为时应由插件/上层显式模拟。
//! - ⚠️ 尚未实现/待验证：divider/intervals 为 0 时真实 ESC 的行为、独立硬件定时器
//!   （本模拟器用"帧驱动时钟"——每收到一帧调用一次 `esc-node` 的 tick；停止收帧时
//!   不会主动报超时）。

/// Watchdog Divider 寄存器地址（40 ns 基数）。
pub const WATCHDOG_DIVIDER: u16 = 0x0400;
/// Watchdog Time PDI 寄存器地址（PDI 看门狗，暂只保留读写语义）。
pub const WATCHDOG_TIME_PDI: u16 = 0x0410;
/// Watchdog Time Process Data 寄存器地址（以 [`WATCHDOG_DIVIDER`] 为单位）。
pub const WATCHDOG_TIME_PROCESS_DATA: u16 = 0x0420;

/// 一个基数单位的纳秒数（40 ns）。
pub const WATCHDOG_UNIT_NS: u64 = 40;

/// AL Status Code：同步管理器看门狗触发。
pub const AL_STATUS_SM_WATCHDOG: u16 = crate::al_status::SYNC_MANAGER_WATCHDOG;

/// SM control 字节中的看门狗使能位（bit6）。
pub const SM_CONTROL_WATCHDOG: u8 = 0x40;

/// 由 divider × intervals 计算看门狗超时（纳秒）；任一为 0 返回 0。
#[must_use]
pub fn timeout_ns(divider: u16, intervals: u16) -> u64 {
    u64::from(divider)
        .saturating_mul(u64::from(intervals))
        .saturating_mul(WATCHDOG_UNIT_NS)
}

/// 由 divider × intervals 计算看门狗超时（毫秒，向上取整）；任一为 0 返回 0。
#[must_use]
pub fn timeout_ms(divider: u16, intervals: u16) -> u64 {
    let ns = timeout_ns(divider, intervals);
    ns.div_ceil(1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_matches_ig_h_documentation() {
        // IgH ecrt.h：divider = 40 ns 间隔数，intervals = 基础间隔数。
        assert_eq!(timeout_ns(2500, 100), 10_000_000, "10 ms");
        assert_eq!(timeout_ms(2500, 100), 10);
        // 向上取整：1 × 1 × 40 ns = 40 ns → 1 ms（帧驱动时钟的最小分辨率）。
        assert_eq!(timeout_ms(1, 1), 1);
        // 任一为 0 → 0（模拟器视为"未配置超时"，不触发）。
        assert_eq!(timeout_ms(0, 100), 0);
        assert_eq!(timeout_ms(100, 0), 0);
    }

    #[test]
    fn constants_match_esc_register_map() {
        assert_eq!(WATCHDOG_DIVIDER, 0x0400);
        assert_eq!(WATCHDOG_TIME_PDI, 0x0410);
        assert_eq!(WATCHDOG_TIME_PROCESS_DATA, 0x0420);
        assert_eq!(SM_CONTROL_WATCHDOG, 0x40);
        assert_eq!(AL_STATUS_SM_WATCHDOG, 0x001B);
    }
}
