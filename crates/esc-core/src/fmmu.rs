//! FMMU 寄存器块（0x0600 起，每个实体 16 字节）。
//!
//! 依据 **ETG.1000.4 table 56 / ESC Sec.II（Registers）**。寄存器布局（实体内偏移）：
//!
//! | 偏移 | 字段 | 主站访问 |
//! |---|---|---|
//! | 0x00–0x03 | Logical Start Address | 读写 |
//! | 0x04–0x05 | Length（字节） | 读写 |
//! | 0x06 | Logical Start Bit（低 3 位） | 读写 |
//! | 0x07 | Logical End Bit（低 3 位） | 读写 |
//! | 0x08–0x09 | Physical Start Address | 读写 |
//! | 0x0A | Physical Start Bit（低 3 位） | 读写 |
//! | 0x0B | Type（bit0 read、bit1 write） | **只读** |
//! | 0x0C | Activate（bit0） | 读写 |
//! | 0x0D–0x0F | 保留（0） | **只读** |
//!
//! 主站在 `PREOP → SAFEOP` 之前写入 FMMU0/1（逻辑地址 → 物理地址映射，本模拟器的物理侧
//! 就是 4 KiB 寄存器/DL-user 内存）；本模块只实现寄存器语义与位域解码，**不执行**实际
//! 逻辑寻址读写（Lrd/Lwr/Lrw 需要按 FMMU 查表搬运，属 B5 后续子步骤 / B8）。
//!
//! ⚠️ 尚未实现：FMMU 命中判定与逻辑→物理搬运、位级映射（bit 偏移）、FMMU 数量上限校验。

use crate::Registers;

/// 第一个 FMMU 实体的寄存器地址（0x0600）。
pub const FMMU_ADDRESS: u16 = 0x0600;

/// 单个 FMMU 实体的寄存器大小（字节）。
pub const FMMU_SIZE: u16 = 16;

/// 寄存器实现的 FMMU 实体数上限（0x0600–0x067F）。
pub const FMMU_MAX: u8 = 8;

/// 实体内偏移：Logical Start Address。
pub const OFFSET_LOGICAL_START_ADDRESS: u16 = 0x00;
/// 实体内偏移：Length（字节）。
pub const OFFSET_LENGTH: u16 = 0x04;
/// 实体内偏移：Logical Start Bit。
pub const OFFSET_LOGICAL_START_BIT: u16 = 0x06;
/// 实体内偏移：Logical End Bit。
pub const OFFSET_LOGICAL_END_BIT: u16 = 0x07;
/// 实体内偏移：Physical Start Address。
pub const OFFSET_PHYSICAL_START_ADDRESS: u16 = 0x08;
/// 实体内偏移：Physical Start Bit。
pub const OFFSET_PHYSICAL_START_BIT: u16 = 0x0A;
/// 实体内偏移：Type（只读）。
pub const OFFSET_TYPE: u16 = 0x0B;
/// 实体内偏移：Activate。
pub const OFFSET_ACTIVATE: u16 = 0x0C;

/// 第 `index` 个 FMMU 实体的首地址；越界返回 [`None`]。
#[must_use]
pub fn fmmu_address(index: u8) -> Option<u16> {
    if index >= FMMU_MAX {
        return None;
    }
    Some(FMMU_ADDRESS + u16::from(index) * FMMU_SIZE)
}

/// 该寄存器地址是否是 FMMU 区的只读字节（Type 与保留字节）。
#[must_use]
pub fn is_read_only_byte(address: u16) -> bool {
    if !(FMMU_ADDRESS..FMMU_ADDRESS + u16::from(FMMU_MAX) * FMMU_SIZE).contains(&address) {
        return false;
    }
    let offset = (address - FMMU_ADDRESS) % FMMU_SIZE;
    offset == OFFSET_TYPE || offset >= OFFSET_TYPE + 2
}

/// 一个 FMMU 实体的寄存器值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fmmu {
    /// 0x00–0x03：逻辑起始地址。
    pub logical_start_address: u32,
    /// 0x04–0x05：映射长度（字节）。
    pub length: u16,
    /// 0x06：逻辑起始位（低 3 位有效）。
    pub logical_start_bit: u8,
    /// 0x07：逻辑结束位（低 3 位有效）。
    pub logical_end_bit: u8,
    /// 0x08–0x09：物理起始地址。
    pub physical_start_address: u16,
    /// 0x0A：物理起始位（低 3 位有效）。
    pub physical_start_bit: u8,
    /// 0x0B：Type（bit0 read、bit1 write，只读）。
    pub fmmu_type: u8,
    /// 0x0C：Activate（bit0）。
    pub activate: u8,
}

impl Fmmu {
    /// 从实体的 16 字节寄存器内容解码。
    #[must_use]
    pub fn from_bytes(bytes: &[u8; FMMU_SIZE as usize]) -> Self {
        Self {
            logical_start_address: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            length: u16::from_le_bytes([bytes[4], bytes[5]]),
            logical_start_bit: bytes[6],
            logical_end_bit: bytes[7],
            physical_start_address: u16::from_le_bytes([bytes[8], bytes[9]]),
            physical_start_bit: bytes[10],
            fmmu_type: bytes[11],
            activate: bytes[12],
        }
    }

    /// 编码为实体的 16 字节寄存器内容（保留字节写 0）。
    #[must_use]
    pub fn to_bytes(self) -> [u8; FMMU_SIZE as usize] {
        let mut bytes = [0u8; FMMU_SIZE as usize];
        bytes[0..4].copy_from_slice(&self.logical_start_address.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.length.to_le_bytes());
        bytes[6] = self.logical_start_bit;
        bytes[7] = self.logical_end_bit;
        bytes[8..10].copy_from_slice(&self.physical_start_address.to_le_bytes());
        bytes[10] = self.physical_start_bit;
        bytes[11] = self.fmmu_type;
        bytes[12] = self.activate;
        bytes
    }

    /// 读寄存器阵列中的第 `index` 个实体；越界返回 [`None`]。
    #[must_use]
    pub fn read(registers: &Registers, index: u8) -> Option<Self> {
        let base = fmmu_address(index)?;
        let mut bytes = [0u8; FMMU_SIZE as usize];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = registers.read_u8(base + i as u16)?;
        }
        Some(Self::from_bytes(&bytes))
    }

    /// 实体是否已激活（Activate.bit0）。
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self.activate & 0x01 != 0
    }

    /// 是否允许读服务（Type.bit0）。
    #[must_use]
    pub fn can_read(self) -> bool {
        self.fmmu_type & 0x01 != 0
    }

    /// 是否允许写服务（Type.bit1）。
    #[must_use]
    pub fn can_write(self) -> bool {
        self.fmmu_type & 0x02 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmmu_addresses_are_sixteen_bytes_apart() {
        assert_eq!(fmmu_address(0), Some(0x0600));
        assert_eq!(fmmu_address(1), Some(0x0610));
        assert_eq!(fmmu_address(7), Some(0x0670));
        assert_eq!(fmmu_address(FMMU_MAX), None);
    }

    #[test]
    fn read_only_bytes_are_type_and_reserved() {
        for index in 0..FMMU_MAX {
            let base = fmmu_address(index).expect("in range");
            assert!(!is_read_only_byte(base + OFFSET_LOGICAL_START_ADDRESS));
            assert!(!is_read_only_byte(base + OFFSET_ACTIVATE));
            assert!(is_read_only_byte(base + OFFSET_TYPE));
            assert!(
                !is_read_only_byte(base + OFFSET_TYPE + 1),
                "0x0C 是 Activate，可写"
            );
            assert!(is_read_only_byte(base + OFFSET_TYPE + 2), "0x0D 保留");
            assert!(is_read_only_byte(base + OFFSET_TYPE + 4), "0x0F 保留");
        }
        assert!(!is_read_only_byte(0x05FF));
        assert!(!is_read_only_byte(FMMU_ADDRESS + 8 * FMMU_SIZE));
    }

    #[test]
    fn roundtrip_and_field_decoding() {
        // IgH 为 PDO 映射写入的典型实体：逻辑地址 0x00001000，8 字节，物理地址 0x1200。
        let fmmu = Fmmu {
            logical_start_address: 0x0000_1000,
            length: 8,
            logical_start_bit: 0,
            logical_end_bit: 7,
            physical_start_address: 0x1200,
            physical_start_bit: 0,
            fmmu_type: 0x03,
            activate: 0x01,
        };
        let bytes = fmmu.to_bytes();
        assert_eq!(&bytes[0..4], &[0x00, 0x10, 0x00, 0x00]);
        assert_eq!(&bytes[4..6], &[0x08, 0x00]);
        assert_eq!(bytes[11], 0x03);
        assert_eq!(Fmmu::from_bytes(&bytes), fmmu);
        assert!(fmmu.is_enabled());
        assert!(fmmu.can_read());
        assert!(fmmu.can_write());

        // 只写方向的实体：Type=0x02。
        let write_only = Fmmu {
            fmmu_type: 0x02,
            ..Fmmu::default()
        };
        assert!(!write_only.can_read());
        assert!(write_only.can_write());
        assert!(!write_only.is_enabled());
    }

    #[test]
    fn registers_reject_master_writes_to_type_and_reserved_bytes() {
        let mut registers = Registers::new();
        let base = fmmu_address(0).expect("in range");
        let master = Fmmu {
            logical_start_address: 0x1000,
            length: 0x0008,
            logical_start_bit: 0,
            logical_end_bit: 7,
            physical_start_address: 0x1200,
            physical_start_bit: 0,
            fmmu_type: 0xFF,
            activate: 0x01,
        };
        for (i, byte) in master.to_bytes().iter().enumerate() {
            registers.write_u8(base + i as u16, *byte);
        }
        let stored = Fmmu::read(&registers, 0).expect("in range");
        assert_eq!(stored.logical_start_address, 0x1000);
        assert_eq!(stored.length, 8);
        assert_eq!(stored.physical_start_address, 0x1200);
        assert_eq!(stored.activate, 0x01);
        assert_eq!(stored.fmmu_type, 0, "Type 只读");
        for offset in 0x0Du16..=0x0F {
            assert_eq!(registers.read_u8(base + offset), Some(0), "保留字节只读");
        }
    }
}
