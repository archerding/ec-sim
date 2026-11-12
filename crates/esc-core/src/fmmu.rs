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
//! | 0x0B | Type（bit0 read、bit1 write） | 读写（主站配置） |
//! | 0x0C | Activate（bit0） | 读写 |
//! | 0x0D–0x0F | 保留（0） | **只读** |
//!
//! **Type 是主站写入的配置字节**（不是 ESC 自述信息）：IgH 1.6.13 `master/fmmu_config.c:
//! ec_fmmu_config_page()` 写 `data + 11 = 0x01/0x02`（输入/输出方向）；SOEM
//! `src/ec_config.c: ecx_config_map_group()` 整块写 `ec_fmmut` 结构（含 `FMMUtype`）。
//! 若把 Type 当只读丢弃，方向位恒为 0，逻辑寻址永远不会命中——2026-11-12 19:19 班修正
//! 了上一班次"Type 只读"的错误假设。
//!
//! 本模块同时承担**逻辑→物理翻译**（[`Fmmu::translate`]）：给定 datagram 的逻辑地址区间
//! `[address, address + len)`，返回它与本 FMMU 逻辑区间 `[logical_start_address,
//! logical_start_address + length)` 的**字节对齐**交集，以及交集的物理侧起始地址。
//! 数据搬运本身由 [`crate::memory::DlUserMemory`] 完成（物理侧 = DL 用户内存 0x1000–0xFFFF）。
//!
//! 字节对齐依据（只读参考，未复制代码）：
//! - IgH 1.6.13 `master/fmmu_config.c: ec_fmmu_config_page()` 恒写逻辑起始位 `0x00`、
//!   逻辑结束位 `0x07`、物理起始位 `0x00`；
//! - SOEM `src/ec_config.c: ecx_config_map_group()` 的默认（字节）映射同样用 `BitPos 0/7`。
//!
//! ⚠️ 尚未实现：位级映射（任意 logical/physical bit 偏移，需整帧位移搬运）、FMMU 数量上限
//! 校验（主站配置超过 8 个实体时的行为）、FMMU 物理地址指向寄存器区（0x0000–0x0FFF）
//! 的语义（真实 ESC 允许，本模拟器当前只在 DL 用户内存内搬运）。

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

/// 该寄存器地址是否是 FMMU 区的只读字节（保留字节 0x0D–0x0F）。
///
/// Type（0x0B）由主站配置，可写（见模块文档）。
#[must_use]
pub fn is_read_only_byte(address: u16) -> bool {
    if !(FMMU_ADDRESS..FMMU_ADDRESS + u16::from(FMMU_MAX) * FMMU_SIZE).contains(&address) {
        return false;
    }
    let offset = (address - FMMU_ADDRESS) % FMMU_SIZE;
    offset >= OFFSET_TYPE + 2
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
    /// 0x0B：Type（bit0 read、bit1 write，主站配置，可写）。
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

    /// 逻辑区间终点（不含）。长度为 0 或 32 位地址溢出返回 [`None`]。
    #[must_use]
    pub fn logical_end(self) -> Option<u32> {
        if self.length == 0 {
            return None;
        }
        self.logical_start_address
            .checked_add(u32::from(self.length))
    }

    /// 是否字节对齐映射：逻辑起始位 `0`、逻辑结束位 `7`、物理起始位 `0`。
    ///
    /// 两家主站的默认映射都满足（见模块文档）。非字节对齐的位级映射尚未实现。
    #[must_use]
    pub fn is_byte_aligned(self) -> bool {
        self.logical_start_bit == 0 && self.logical_end_bit == 7 && self.physical_start_bit == 0
    }

    /// 计算逻辑地址区间 `[logical_address, logical_address + len)` 与本 FMMU 映射区间的
    /// **字节对齐**交集。
    ///
    /// 返回 [`None`] 表示本 datagram 未命中此 FMMU（未激活、非字节对齐、长度为 0、
    /// 区间不相交、32 位地址溢出，或物理地址越过 16 位地址空间）。
    #[must_use]
    pub fn translate(self, logical_address: u32, len: usize) -> Option<Translation> {
        if !self.is_enabled() || !self.is_byte_aligned() || len == 0 {
            return None;
        }
        let fmmu_end = self.logical_end()?;
        let datagram_len = u32::try_from(len).ok()?;
        let datagram_end = logical_address.checked_add(datagram_len)?;
        let start = logical_address.max(self.logical_start_address);
        let end = datagram_end.min(fmmu_end);
        if start >= end {
            return None;
        }
        let datagram_offset = usize::try_from(start.checked_sub(logical_address)?).ok()?;
        let mapped_len = usize::try_from(end.checked_sub(start)?).ok()?;
        let physical_start = u32::from(self.physical_start_address)
            .checked_add(start - self.logical_start_address)?;
        Some(Translation {
            datagram_offset,
            physical_address: u16::try_from(physical_start).ok()?,
            len: mapped_len,
        })
    }
}

/// 逻辑→物理翻译结果：datagram 数据区里的一段连续字节映射到物理侧的一段连续地址。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Translation {
    /// 该段在 datagram 数据区中的起始偏移。
    pub datagram_offset: usize,
    /// 该段在物理侧（DL 用户内存）的起始地址。
    pub physical_address: u16,
    /// 该段字节数（保证 > 0）。
    pub len: usize,
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
    fn read_only_bytes_are_reserved_only_type_is_configurable() {
        for index in 0..FMMU_MAX {
            let base = fmmu_address(index).expect("in range");
            assert!(!is_read_only_byte(base + OFFSET_LOGICAL_START_ADDRESS));
            assert!(!is_read_only_byte(base + OFFSET_ACTIVATE));
            assert!(
                !is_read_only_byte(base + OFFSET_TYPE),
                "Type 由主站写入方向位，必须可写"
            );
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
    fn translate_maps_full_datagram_into_physical_memory() {
        // IgH 为 8 字节输出 PDO 写入的典型实体：逻辑 0x00001000 → 物理 0x1000。
        let fmmu = Fmmu {
            logical_start_address: 0x0000_1000,
            length: 8,
            logical_start_bit: 0,
            logical_end_bit: 7,
            physical_start_address: 0x1000,
            physical_start_bit: 0,
            fmmu_type: 0x02,
            activate: 0x01,
        };
        assert_eq!(fmmu.logical_end(), Some(0x0000_1008));
        assert_eq!(
            fmmu.translate(0x0000_1000, 8),
            Some(Translation {
                datagram_offset: 0,
                physical_address: 0x1000,
                len: 8,
            })
        );
    }

    #[test]
    fn translate_returns_only_the_intersecting_bytes() {
        let fmmu = Fmmu {
            logical_start_address: 0x0000_1000,
            length: 8,
            logical_end_bit: 7,
            physical_start_address: 0x1000,
            fmmu_type: 0x03,
            activate: 0x01,
            ..Fmmu::default()
        };
        // datagram 从 FMMU 区间前 4 字节开始、长 16 字节：只映射后 4 字节。
        assert_eq!(
            fmmu.translate(0x0000_0FFC, 16),
            Some(Translation {
                datagram_offset: 4,
                physical_address: 0x1000,
                len: 8,
            })
        );
        // datagram 落在区间尾部：偏移 4 起 8 字节中只命中 4 字节。
        assert_eq!(
            fmmu.translate(0x0000_1004, 8),
            Some(Translation {
                datagram_offset: 0,
                physical_address: 0x1004,
                len: 4,
            })
        );
    }

    #[test]
    fn translate_rejects_inactive_misaligned_or_non_intersecting() {
        let base = Fmmu {
            logical_start_address: 0x0000_1000,
            length: 8,
            logical_end_bit: 7,
            physical_start_address: 0x1000,
            activate: 0x01,
            fmmu_type: 0x03,
            ..Fmmu::default()
        };
        // 未激活。
        let inactive = Fmmu {
            activate: 0,
            ..base
        };
        assert_eq!(inactive.translate(0x1000, 8), None);
        // 位级映射（逻辑起止位不是 0/7）：本阶段未实现，按未命中处理。
        let misaligned = Fmmu {
            logical_start_bit: 3,
            logical_end_bit: 6,
            ..base
        };
        assert!(!misaligned.is_byte_aligned());
        assert_eq!(misaligned.translate(0x1000, 8), None);
        // 长度为 0 / datagram 长度为 0 / 区间不相交。
        assert_eq!(Fmmu { length: 0, ..base }.translate(0x1000, 8), None);
        assert_eq!(base.translate(0x1000, 0), None);
        assert_eq!(base.translate(0x0000_2000, 8), None);
        // 32 位逻辑地址溢出。
        let overflow = Fmmu {
            logical_start_address: 0xFFFF_FFF0,
            length: 0x20,
            ..base
        };
        assert_eq!(overflow.logical_end(), None);
        assert_eq!(overflow.translate(0xFFFF_FFF0, 0x20), None);
    }

    #[test]
    fn translate_rejects_physical_address_overflow() {
        // 物理起始 0xFFF8 + 逻辑偏移 8 → 0x10000，越过 16 位物理地址空间。
        let fmmu = Fmmu {
            logical_start_address: 0x0000_1000,
            length: 16,
            logical_end_bit: 7,
            physical_start_address: 0xFFF8,
            fmmu_type: 0x03,
            activate: 0x01,
            ..Fmmu::default()
        };
        assert_eq!(
            fmmu.translate(0x1000, 8),
            Some(Translation {
                datagram_offset: 0,
                physical_address: 0xFFF8,
                len: 8,
            })
        );
        assert_eq!(fmmu.translate(0x1008, 8), None);
    }

    #[test]
    fn registers_store_master_fmmu_page_writes_except_reserved_bytes() {
        let mut registers = Registers::new();
        let base = fmmu_address(0).expect("in range");
        let master = Fmmu {
            logical_start_address: 0x1000,
            length: 0x0008,
            logical_start_bit: 0,
            logical_end_bit: 7,
            physical_start_address: 0x1200,
            physical_start_bit: 0,
            fmmu_type: 0x02, // 主站写的输出方向位
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
        assert_eq!(stored.fmmu_type, 0x02, "Type 由主站配置");
        assert!(stored.can_write());
        assert!(!stored.can_read());
        for offset in 0x0Du16..=0x0F {
            assert_eq!(registers.read_u8(base + offset), Some(0), "保留字节只读");
        }
    }
}
