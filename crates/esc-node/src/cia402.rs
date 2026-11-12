//! CiA 402 伺服驱动状态机骨架（B11 Tier0 行为插件的第一步）。
//!
//! 只实现**状态机本身**（控制字 `0x6040` → 状态字 `0x6041` 的最小状态转换），
//! 不绑定 PDO 数据通路（下一班：把状态机接到运行时过程数据）。
//!
//! 事实来源（公开规范）：CiA 402 伺服驱动 profile 的控制字/状态字位定义与状态编码，
//! 见 [`docs/plugins/cia402-state-machine.md`](../../../docs/plugins/cia402-state-machine.md)。
//!
//! ⚠️ 已知简化（Tier0 骨架）：
//! - 未实现 Ti=0/Ti=1 的精确时序（如 "enable operation" 需要多个周期置位）；
//! - 未实现厂商扩展位（bit10 target reached/bit11 internal limit 由上层决定）；
//! - 未实现错误码 `0x603F` 的触发条件（只提供 [`Cia402StateMachine::inject_fault`] 供测试/上层注入）；
//! - 未实现模式相关位（0x6040 bit4–6：new set-point/change set immediately/halt）。

/// 控制字位（`0x6040`，CiA 402）。
pub mod controlword {
    /// Switch on（bit0）。
    pub const SWITCH_ON: u16 = 0x0001;
    /// Enable voltage（bit1）。
    pub const ENABLE_VOLTAGE: u16 = 0x0002;
    /// Quick stop（低有效，bit2）。
    pub const QUICK_STOP: u16 = 0x0004;
    /// Enable operation（bit3）。
    pub const ENABLE_OPERATION: u16 = 0x0008;
    /// Fault reset（bit7，上升沿有效）。
    pub const FAULT_RESET: u16 = 0x0080;
    /// 模式相关 bit4：CSP 的 new set-point / HM 的 start homing。
    pub const BIT4: u16 = 0x0010;
}

/// 状态字位（`0x6041`，CiA 402）。
pub mod statusword {
    /// Ready to switch on（bit0）。
    pub const READY_TO_SWITCH_ON: u16 = 0x0001;
    /// Switched on（bit1）。
    pub const SWITCHED_ON: u16 = 0x0002;
    /// Operation enabled（bit2）。
    pub const OPERATION_ENABLED: u16 = 0x0004;
    /// Fault（bit3）。
    pub const FAULT: u16 = 0x0008;
    /// Voltage enabled（bit4）。
    pub const VOLTAGE_ENABLED: u16 = 0x0010;
    /// Homing attained（bit12，HM 模式）。
    pub const HOMING_ATTAINED: u16 = 0x1000;
    /// Quick stop（低有效，bit5）。
    pub const QUICK_STOP: u16 = 0x0020;
    /// Switch on disabled（bit6）。
    pub const SWITCH_ON_DISABLED: u16 = 0x0040;
    /// Warning（bit7）。
    pub const WARNING: u16 = 0x0080;
    /// Remote（bit9）。
    pub const REMOTE: u16 = 0x0200;
    /// Target reached（bit10）。
    pub const TARGET_REACHED: u16 = 0x0400;
    /// Internal limit active（bit11）。
    pub const INTERNAL_LIMIT_ACTIVE: u16 = 0x0800;
}

/// CiA 402 状态机状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cia402State {
    /// Not ready to switch on（上电/自检阶段）。
    NotReadyToSwitchOn,
    /// Switch on disabled（初始状态，状态字 bit6）。
    SwitchOnDisabled,
    /// Ready to switch on（bit0 + bit5）。
    ReadyToSwitchOn,
    /// Switched on（bit0 + bit1 + bit5）。
    SwitchedOn,
    /// Operation enabled（bit0 + bit1 + bit2 + bit5）。
    OperationEnabled,
    /// Quick stop active（bit0 + bit1 + bit2 + bit5，bit5 清零）。
    QuickStopActive,
    /// Fault reaction active（正在执行故障反应）。
    FaultReactionActive,
    /// Fault（bit3）。
    Fault,
}

impl Cia402State {
    /// 该状态对应的状态字（不含厂商/模式扩展位；bit9 Remote 恒置 1）。
    #[must_use]
    pub fn statusword(self) -> u16 {
        let remote = statusword::REMOTE;
        match self {
            Cia402State::NotReadyToSwitchOn => remote,
            Cia402State::SwitchOnDisabled => remote | statusword::SWITCH_ON_DISABLED,
            Cia402State::ReadyToSwitchOn => {
                remote | statusword::READY_TO_SWITCH_ON | statusword::QUICK_STOP
            }
            Cia402State::SwitchedOn => {
                remote
                    | statusword::READY_TO_SWITCH_ON
                    | statusword::SWITCHED_ON
                    | statusword::QUICK_STOP
            }
            Cia402State::OperationEnabled => {
                remote
                    | statusword::READY_TO_SWITCH_ON
                    | statusword::SWITCHED_ON
                    | statusword::OPERATION_ENABLED
                    | statusword::QUICK_STOP
                    | statusword::VOLTAGE_ENABLED
            }
            Cia402State::QuickStopActive => {
                remote
                    | statusword::READY_TO_SWITCH_ON
                    | statusword::SWITCHED_ON
                    | statusword::OPERATION_ENABLED
            }
            Cia402State::FaultReactionActive => remote | statusword::FAULT | 0x0007,
            Cia402State::Fault => remote | statusword::FAULT,
        }
    }
}

/// CiA 402 状态机骨架。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cia402StateMachine {
    state: Cia402State,
}

impl Default for Cia402StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Cia402StateMachine {
    /// 上电后进入 `SwitchOnDisabled`（CiA 402 标准初始状态；此处跳过 Not ready 阶段）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Cia402State::SwitchOnDisabled,
        }
    }

    /// 当前状态。
    #[must_use]
    pub fn state(&self) -> Cia402State {
        self.state
    }

    /// 当前状态字（`0x6041`）。
    #[must_use]
    pub fn statusword(&self) -> u16 {
        self.state.statusword()
    }

    /// 注入故障：进入故障反应并立即稳定到 `Fault`（Tier0 简化；真机有反应延时）。
    pub fn inject_fault(&mut self) {
        self.state = Cia402State::FaultReactionActive;
        // Tier0 模型不做反应延时，同一"周期"内进入 Fault。
        self.state = Cia402State::Fault;
    }

    /// 用一个控制字周期推进状态机；返回状态字。
    ///
    /// 转换表按 CiA 402 状态机的 shutdown / switch on / enable operation / disable voltage /
    /// quick stop / fault reset 命令；未实现的命令位（bit4–6）被忽略。
    pub fn update(&mut self, cw_value: u16) -> u16 {
        let shutdown = cw_value & 0x0007 == (controlword::QUICK_STOP | controlword::ENABLE_VOLTAGE);
        let switch_on = cw_value & 0x0007
            == (controlword::QUICK_STOP | controlword::ENABLE_VOLTAGE | controlword::SWITCH_ON);
        let enable_operation = cw_value & 0x000F
            == (controlword::QUICK_STOP
                | controlword::ENABLE_VOLTAGE
                | controlword::SWITCH_ON
                | controlword::ENABLE_OPERATION);
        let disable_voltage = cw_value & controlword::ENABLE_VOLTAGE == 0;
        let quick_stop = cw_value & controlword::QUICK_STOP == 0;
        let fault_reset = cw_value & controlword::FAULT_RESET != 0;

        self.state = match self.state {
            Cia402State::NotReadyToSwitchOn => Cia402State::NotReadyToSwitchOn,
            Cia402State::SwitchOnDisabled => {
                if fault_reset {
                    // 故障复位在非 Fault 状态无副作用（保持）。
                    Cia402State::SwitchOnDisabled
                } else if shutdown {
                    Cia402State::ReadyToSwitchOn
                } else {
                    Cia402State::SwitchOnDisabled
                }
            }
            Cia402State::ReadyToSwitchOn => {
                if disable_voltage {
                    Cia402State::SwitchOnDisabled
                } else if switch_on {
                    Cia402State::SwitchedOn
                } else {
                    Cia402State::ReadyToSwitchOn
                }
            }
            Cia402State::SwitchedOn => {
                if disable_voltage {
                    Cia402State::SwitchOnDisabled
                } else if enable_operation {
                    Cia402State::OperationEnabled
                } else {
                    Cia402State::SwitchedOn
                }
            }
            Cia402State::OperationEnabled => {
                if disable_voltage {
                    Cia402State::SwitchOnDisabled
                } else if quick_stop {
                    Cia402State::QuickStopActive
                } else if shutdown {
                    Cia402State::ReadyToSwitchOn
                } else {
                    Cia402State::OperationEnabled
                }
            }
            Cia402State::QuickStopActive => {
                if disable_voltage || fault_reset {
                    Cia402State::SwitchOnDisabled
                } else {
                    Cia402State::QuickStopActive
                }
            }
            Cia402State::FaultReactionActive => Cia402State::Fault,
            Cia402State::Fault => {
                if fault_reset {
                    Cia402State::SwitchOnDisabled
                } else {
                    Cia402State::Fault
                }
            }
        };
        self.statusword()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_switch_on_disabled() {
        let machine = Cia402StateMachine::new();
        assert_eq!(machine.state(), Cia402State::SwitchOnDisabled);
        assert_eq!(machine.statusword(), 0x0240, "Remote + Switch on disabled");
    }

    #[test]
    fn follows_canonical_enable_sequence() {
        let mut machine = Cia402StateMachine::new();
        // Shutdown (0x06) → Ready to switch on。
        assert_eq!(machine.update(0x0006), 0x0221);
        assert_eq!(machine.state(), Cia402State::ReadyToSwitchOn);
        // Switch on (0x07) → Switched on。
        assert_eq!(machine.update(0x0007), 0x0223);
        assert_eq!(machine.state(), Cia402State::SwitchedOn);
        // Enable operation (0x0F) → Operation enabled。
        assert_eq!(machine.update(0x000F), 0x0237);
        assert_eq!(machine.state(), Cia402State::OperationEnabled);
    }

    #[test]
    fn operation_enabled_shutdown_returns_to_ready() {
        let mut machine = Cia402StateMachine::new();
        machine.update(0x0006);
        machine.update(0x0007);
        machine.update(0x000F);
        assert_eq!(machine.state(), Cia402State::OperationEnabled);
        assert_eq!(machine.update(0x0006), 0x0221);
        assert_eq!(machine.state(), Cia402State::ReadyToSwitchOn);
    }

    #[test]
    fn quick_stop_and_disable_voltage_paths() {
        let mut machine = Cia402StateMachine::new();
        machine.update(0x0006);
        machine.update(0x0007);
        machine.update(0x000F);
        // 清 bit2（quick stop 低有效）→ Quick stop active（状态字 bit2 保留、bit5 清零）。
        assert_eq!(machine.update(0x000B), 0x0207);
        assert_eq!(machine.state(), Cia402State::QuickStopActive);
        // Disable voltage（bit1=0）→ Switch on disabled。
        assert_eq!(machine.update(0x0000), 0x0240);
        assert_eq!(machine.state(), Cia402State::SwitchOnDisabled);
    }

    #[test]
    fn fault_and_reset_recovery() {
        let mut machine = Cia402StateMachine::new();
        machine.update(0x0006);
        machine.update(0x0007);
        machine.update(0x000F);
        machine.inject_fault();
        assert_eq!(machine.state(), Cia402State::Fault);
        assert_eq!(machine.statusword(), 0x0208, "Remote + Fault");
        // 无 fault reset 时保持 Fault。
        assert_eq!(machine.update(0x000F), 0x0208);
        // fault reset（bit7）→ Switch on disabled。
        assert_eq!(machine.update(0x0080), 0x0240);
        assert_eq!(machine.state(), Cia402State::SwitchOnDisabled);
    }

    #[test]
    fn invalid_controlword_keeps_state() {
        let mut machine = Cia402StateMachine::new();
        // 只有 enable voltage：不满足 shutdown（缺 quick stop 释放），保持初始状态。
        assert_eq!(machine.update(0x0002), 0x0240);
        assert_eq!(machine.state(), Cia402State::SwitchOnDisabled);
    }
}
