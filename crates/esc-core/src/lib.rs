//! ESC 内核（最小起步）：寄存器阵列与 AL 状态机。
//!
//! 依据 **ESC Sec.II（Registers）** 与 **ETG.1000.5/6（AL services/protocol）**。
//! 本文件只实现 B5 的第一块，保持可独立验证：
//! - 0x0000–0x0FFF 寄存器空间（4 KiB），带读/写权限表；
//! - AL Control (0x0120) / AL Status (0x0130) / AL Status Code (0x0134) 语义；
//! - ESM 合法迁移表、错误位与 Error ACK、ID Request。
//!
//! 尚未实现（后续子步骤）：SM/FMMU 寄存器块、watchdog、DC。
//!
//! 已实现（B2b 起）：[`sii`] 最小 SII 镜像、[`eeprom`] EEPROM 寄存器接口状态机。

pub mod eeprom;
pub mod sii;

/// ESC 标准寄存器空间大小（0x0000–0x0FFF）。
pub const REG_SPACE_LEN: usize = 0x1000;

/// 关键寄存器地址（按 ESC Sec.II）。
pub mod reg {
    pub const TYPE: u16 = 0x0000;
    pub const REVISION: u16 = 0x0001;
    pub const FMMU_COUNT: u16 = 0x0004;
    pub const SYNC_MANAGER_COUNT: u16 = 0x0005;
    pub const PORT_DESCRIPTOR: u16 = 0x0007;
    pub const FEATURES: u16 = 0x0008;
    pub const STATION_ADDRESS: u16 = 0x0010;
    pub const STATION_ALIAS: u16 = 0x0012;
    pub const DL_STATUS: u16 = 0x0110;
    pub const AL_CONTROL: u16 = 0x0120;
    pub const AL_STATUS: u16 = 0x0130;
    pub const AL_STATUS_CODE: u16 = 0x0134;
    pub const PDI_CONTROL: u16 = 0x0140;
    pub const EEPROM_CONFIG: u16 = 0x0500;
    pub const EEPROM_STATUS: u16 = 0x0502;
    pub const EEPROM_ADDRESS: u16 = 0x0504;
    pub const EEPROM_DATA: u16 = 0x0508;
    pub const FMMU_BASE: u16 = 0x0600;
    pub const SYNC_MANAGER_BASE: u16 = 0x0800;
}

/// AL 状态（应用层 ESM）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AlState {
    Init = 0x01,
    PreOp = 0x02,
    SafeOp = 0x04,
    Op = 0x08,
    Boot = 0x03,
}

impl AlState {
    /// 从 AL Status 低 4 位解析；非法值返回 [`None`]。
    #[must_use]
    pub fn from_bits(bits: u8) -> Option<Self> {
        Some(match bits & 0x0F {
            0x01 => Self::Init,
            0x02 => Self::PreOp,
            0x03 => Self::Boot,
            0x04 => Self::SafeOp,
            0x08 => Self::Op,
            _ => return None,
        })
    }

    /// 状态位值（写入 AL Status 低 4 位）。
    #[must_use]
    pub fn bits(self) -> u8 {
        self as u8
    }

    /// 是否允许从当前状态迁移到 `target`（简化 ESM：相邻状态双向 + 任意状态可回 INIT）。
    #[must_use]
    pub fn can_transition_to(self, target: Self) -> bool {
        use AlState::{Boot, Init, Op, PreOp, SafeOp};
        if self == target {
            return true;
        }
        matches!(
            (self, target),
            (Init, PreOp)
                | (PreOp, Init)
                | (PreOp, SafeOp)
                | (SafeOp, PreOp)
                | (SafeOp, Op)
                | (Op, SafeOp)
                | (_, Init)
                | (Init, Boot)
        )
    }
}

/// AL Control 寄存器解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlControl {
    /// 请求目标状态（低 4 位）。
    pub state_bits: u8,
    /// bit4：Error Acknowledge。
    pub error_ack: bool,
    /// bit5：ID Request。
    pub id_request: bool,
}

impl AlControl {
    /// 从 0x0120 的 1 字节值解析。
    #[must_use]
    pub fn from_byte(value: u8) -> Self {
        Self {
            state_bits: value & 0x0F,
            error_ack: value & 0x10 != 0,
            id_request: value & 0x20 != 0,
        }
    }

    /// 请求的目标状态；非法位返回 [`None`]。
    #[must_use]
    pub fn target(&self) -> Option<AlState> {
        AlState::from_bits(self.state_bits)
    }
}

/// AL Status 寄存器（1 字节）：状态位 + 错误位 + ID 位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlStatus {
    pub state: AlState,
    pub error: bool,
    pub id_request: bool,
}

impl AlStatus {
    /// 编码为写入 0x0130 的字节。
    #[must_use]
    pub fn to_byte(self) -> u8 {
        self.state.bits() | (u8::from(self.error) << 4) | (u8::from(self.id_request) << 5)
    }
}

/// AL 状态机 + 错误码持有者。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscAl {
    pub state: AlState,
    pub error: bool,
    pub status_code: u16,
}

impl Default for EscAl {
    fn default() -> Self {
        Self {
            state: AlState::Init,
            error: false,
            status_code: 0,
        }
    }
}

/// AL Status Code：非法状态迁移。
pub const AL_STATUS_INVALID_STATE_CHANGE: u16 = 0x0011;
/// AL Status Code：无错误。
pub const AL_STATUS_NO_ERROR: u16 = 0x0000;

impl EscAl {
    /// 当前 AL Status 字节（写 0x0130 用）。
    #[must_use]
    pub fn status_byte(&self) -> u8 {
        AlStatus {
            state: self.state,
            error: self.error,
            id_request: false,
        }
        .to_byte()
    }

    /// 处理一次 AL Control 写入。
    ///
    /// 语义（ETG.1000.5）：
    /// 1. 若 Error ACK 置位且当前有错误 → 清除错误位与状态码，状态保持不变；
    /// 2. 请求目标 == 当前状态 → 无操作；
    /// 3. 目标状态合法 → 迁移；
    /// 4. 否则置错误位并写 `0x0011`（非法状态迁移）。
    ///
    /// 返回是否发生了状态迁移或错误状态变化。
    pub fn handle_control(&mut self, control: AlControl) -> bool {
        if control.error_ack && self.error {
            self.error = false;
            self.status_code = AL_STATUS_NO_ERROR;
            return true;
        }
        let Some(target) = control.target() else {
            self.error = true;
            self.status_code = AL_STATUS_INVALID_STATE_CHANGE;
            return true;
        };
        if target == self.state {
            return false;
        }
        if self.state.can_transition_to(target) {
            self.state = target;
            self.error = false;
            self.status_code = AL_STATUS_NO_ERROR;
            true
        } else {
            self.error = true;
            self.status_code = AL_STATUS_INVALID_STATE_CHANGE;
            true
        }
    }
}

/// 寄存器访问权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// 主站只读（模拟器维护）。
    ReadOnly,
    /// 主站可读写。
    ReadWrite,
}

/// 4 KiB ESC 寄存器阵列。
#[derive(Debug, Clone)]
pub struct Registers {
    mem: Box<[u8; REG_SPACE_LEN]>,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    /// 新建全零寄存器阵列，并写入最小可识别标识（Type/Revision/FMMU/SM 数量/端口描述）。
    #[must_use]
    pub fn new() -> Self {
        let mut regs = Self {
            mem: Box::new([0u8; REG_SPACE_LEN]),
        };
        // 识别类寄存器对主站只读，初始化必须走内部写。
        regs.set_internal(reg::TYPE, &[0x01]); // ESC 类型（任意非 0）
        regs.set_internal(reg::REVISION, &[0x01]);
        regs.set_internal(reg::FMMU_COUNT, &[8]);
        regs.set_internal(reg::SYNC_MANAGER_COUNT, &[4]);
        regs.set_internal(reg::PORT_DESCRIPTOR, &[0x05]); // port0/1 为 MII
        regs
    }

    /// 该寄存器地址的访问权限（未列出的按可读写处理）。
    #[must_use]
    pub fn access(address: u16) -> Access {
        match address {
            reg::TYPE
            | reg::REVISION
            | reg::FMMU_COUNT
            | reg::SYNC_MANAGER_COUNT
            | reg::PORT_DESCRIPTOR
            | reg::FEATURES
            | reg::AL_STATUS
            | reg::AL_STATUS_CODE
            | reg::DL_STATUS
            | reg::PDI_CONTROL => Access::ReadOnly,
            _ => Access::ReadWrite,
        }
    }

    fn in_range(address: u16, len: usize) -> bool {
        usize::from(address) + len <= REG_SPACE_LEN
    }

    /// 读 1 字节；越界返回 [`None`]。
    #[must_use]
    pub fn read_u8(&self, address: u16) -> Option<u8> {
        if !Self::in_range(address, 1) {
            return None;
        }
        Some(self.mem[usize::from(address)])
    }

    /// 读 2 字节（小端）。
    #[must_use]
    pub fn read_u16(&self, address: u16) -> Option<u16> {
        if !Self::in_range(address, 2) {
            return None;
        }
        let i = usize::from(address);
        Some(u16::from_le_bytes([self.mem[i], self.mem[i + 1]]))
    }

    /// 读 4 字节（小端）。
    #[must_use]
    pub fn read_u32(&self, address: u16) -> Option<u32> {
        if !Self::in_range(address, 4) {
            return None;
        }
        let i = usize::from(address);
        Some(u32::from_le_bytes([
            self.mem[i],
            self.mem[i + 1],
            self.mem[i + 2],
            self.mem[i + 3],
        ]))
    }

    /// 主站写 1 字节；只读寄存器或越界返回 `false`。
    pub fn write_u8(&mut self, address: u16, value: u8) -> bool {
        if !Self::in_range(address, 1) || Self::access(address) == Access::ReadOnly {
            return false;
        }
        self.mem[usize::from(address)] = value;
        true
    }

    /// 主站写 2 字节（小端）；只读或越界返回 `false`。
    pub fn write_u16(&mut self, address: u16, value: u16) -> bool {
        if !Self::in_range(address, 2) || Self::access(address) == Access::ReadOnly {
            return false;
        }
        let i = usize::from(address);
        self.mem[i..i + 2].copy_from_slice(&value.to_le_bytes());
        true
    }

    /// 模拟器内部写（绕过读权限，用于维护只读寄存器）。
    pub fn set_internal(&mut self, address: u16, bytes: &[u8]) -> bool {
        if !Self::in_range(address, bytes.len()) {
            return false;
        }
        let i = usize::from(address);
        self.mem[i..i + bytes.len()].copy_from_slice(bytes);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registers_have_identifying_fields() {
        let regs = Registers::new();
        assert_eq!(regs.read_u8(reg::TYPE), Some(0x01));
        assert_eq!(regs.read_u8(reg::FMMU_COUNT), Some(8));
        assert_eq!(regs.read_u8(reg::SYNC_MANAGER_COUNT), Some(4));
        assert_eq!(regs.read_u8(reg::PORT_DESCRIPTOR), Some(0x05));
    }

    #[test]
    fn read_only_registers_reject_master_writes() {
        let mut regs = Registers::new();
        assert!(!regs.write_u8(reg::TYPE, 0xFF));
        assert_eq!(regs.read_u8(reg::TYPE), Some(0x01));
        assert!(!regs.write_u8(reg::AL_STATUS, 0x08));
        // 主站可写站地址
        assert!(regs.write_u16(reg::STATION_ADDRESS, 0x1001));
        assert_eq!(regs.read_u16(reg::STATION_ADDRESS), Some(0x1001));
    }

    #[test]
    fn internal_write_bypasses_read_only() {
        let mut regs = Registers::new();
        assert!(regs.set_internal(reg::AL_STATUS, &[0x04]));
        assert_eq!(regs.read_u8(reg::AL_STATUS), Some(0x04));
    }

    #[test]
    fn out_of_range_access_is_rejected() {
        let mut regs = Registers::new();
        assert_eq!(regs.read_u8(REG_SPACE_LEN as u16), None);
        assert_eq!(regs.read_u16(0x0FFF), None);
        assert!(!regs.write_u8(0x1000, 1));
        assert_eq!(regs.read_u32(0x0FFE), None);
    }

    #[test]
    fn al_control_parsing() {
        let c = AlControl::from_byte(0x12); // 0x02 + ACK
        assert_eq!(c.state_bits, 0x02);
        assert_eq!(c.target(), Some(AlState::PreOp));
        assert!(c.error_ack);
        assert!(!c.id_request);
        let c = AlControl::from_byte(0x21); // ID request + INIT
        assert!(c.id_request);
        assert_eq!(c.target(), Some(AlState::Init));
        assert!(AlControl::from_byte(0x0F).target().is_none());
    }

    #[test]
    fn al_transitions_init_to_op_and_back() {
        let mut al = EscAl::default();
        assert_eq!(al.state, AlState::Init);
        assert!(al.handle_control(AlControl::from_byte(0x02)));
        assert_eq!(al.state, AlState::PreOp);
        assert!(al.handle_control(AlControl::from_byte(0x04)));
        assert_eq!(al.state, AlState::SafeOp);
        assert!(al.handle_control(AlControl::from_byte(0x08)));
        assert_eq!(al.state, AlState::Op);
        // OP 不能直接回 PREOP
        assert!(al.handle_control(AlControl::from_byte(0x02)));
        assert!(al.error);
        assert_eq!(al.status_code, AL_STATUS_INVALID_STATE_CHANGE);
        // 写 ACK 清错
        assert!(al.handle_control(AlControl::from_byte(0x10)));
        assert!(!al.error);
        assert_eq!(al.status_code, AL_STATUS_NO_ERROR);
        assert_eq!(al.state, AlState::Op);
    }

    #[test]
    fn al_invalid_transition_sets_error_and_status_code() {
        let mut al = EscAl {
            state: AlState::SafeOp,
            error: false,
            status_code: 0,
        };
        // SAFEOP 不能直接到 INIT 以外的 Boot
        assert!(al.handle_control(AlControl::from_byte(0x03)));
        assert!(al.error);
        assert_eq!(al.status_code, AL_STATUS_INVALID_STATE_CHANGE);
        // 任意状态可回 INIT
        assert!(al.handle_control(AlControl::from_byte(0x01)));
        assert_eq!(al.state, AlState::Init);
        assert!(!al.error);
    }

    #[test]
    fn al_status_byte_encoding() {
        let al = EscAl {
            state: AlState::SafeOp,
            error: true,
            status_code: 0x0011,
        };
        assert_eq!(al.status_byte(), 0x14); // 0x04 | 0x10
    }

    #[test]
    fn transition_table_adjacency() {
        assert!(AlState::Init.can_transition_to(AlState::PreOp));
        assert!(AlState::PreOp.can_transition_to(AlState::Init));
        assert!(AlState::PreOp.can_transition_to(AlState::SafeOp));
        assert!(AlState::SafeOp.can_transition_to(AlState::Op));
        assert!(AlState::Op.can_transition_to(AlState::SafeOp));
        assert!(AlState::Op.can_transition_to(AlState::Init));
        assert!(!AlState::Op.can_transition_to(AlState::PreOp));
        assert!(!AlState::Init.can_transition_to(AlState::Op));
        assert!(!AlState::Init.can_transition_to(AlState::SafeOp));
    }
}
