//! DL 用户内存（ESC 物理侧数据区）：0x1000–0xFFFF。
//!
//! ESC 的 64 KiB 地址空间里，0x0000–0x0FFF 是寄存器（见 [`crate::Registers`]），
//! 0x1000–0xFFFF 是 DL 用户内存：SM 缓冲区、PDO 影子内存、PDI 可见数据都放这里。
//! 两家主站的 FMMU 物理地址取自 SII/SM 的 start address（IgH 1.6.13
//! `master/fmmu_config.c` 用 `sync->physical_start_address`；SOEM `src/ec_config.c`
//! 用 `SM[SMc].StartAddr`），实际都落在这一段，因此逻辑寻址（Lrd/Lwr/Lrw）的物理侧
//! 就是本模块。
//!
//! ⚠️ 尚未实现：物理地址指向寄存器区（< 0x1000）或越界时的 ESC 行为（真实 ESC 的
//! 物理地址覆盖整个 64 KiB 地址空间）；PDI/SM 与这块内存的联动（属 B5 后续 / B7）。

/// DL 用户内存起始地址（紧随 4 KiB 寄存器区之后）。
pub const DL_USER_MEMORY_START: u16 = 0x1000;

/// ESC 64 KiB 地址空间的总字节数。
pub const ESC_ADDRESS_SPACE_LEN: usize = 0x1_0000;

/// DL 用户内存字节数（0x1000–0xFFFF，48 KiB）。
pub const DL_USER_MEMORY_LEN: usize = ESC_ADDRESS_SPACE_LEN - DL_USER_MEMORY_START as usize;

/// DL 用户内存镜像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlUserMemory {
    mem: Box<[u8; DL_USER_MEMORY_LEN]>,
}

impl Default for DlUserMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl DlUserMemory {
    /// 新建全零 DL 用户内存（相当于上电复位后、PDI 未写入的初值）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            mem: Box::new([0u8; DL_USER_MEMORY_LEN]),
        }
    }

    /// 访问区间 `[address, address + len)` 是否完整落在 DL 用户内存内。
    #[must_use]
    pub fn covers(address: u16, len: usize) -> bool {
        Self::offset(address, len).is_some()
    }

    /// 区间在内部数组中的起始下标；越界（含低于 0x1000、跨 0xFFFF、空区间）返回 [`None`]。
    fn offset(address: u16, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        let start = usize::from(address);
        let end = start.checked_add(len)?;
        let dl_start = usize::from(DL_USER_MEMORY_START);
        if start < dl_start || end > ESC_ADDRESS_SPACE_LEN {
            return None;
        }
        Some(start - dl_start)
    }

    /// 读一段字节；越界返回 [`None`]。
    #[must_use]
    pub fn read(&self, address: u16, len: usize) -> Option<Vec<u8>> {
        let offset = Self::offset(address, len)?;
        Some(self.mem[offset..offset + len].to_vec())
    }

    /// 读一段字节到调用方缓冲（**无堆分配**，供实时环使用）；越界返回 [`None`]。
    pub fn read_into(&self, address: u16, dst: &mut [u8]) -> Option<usize> {
        let offset = Self::offset(address, dst.len())?;
        dst.copy_from_slice(&self.mem[offset..offset + dst.len()]);
        Some(dst.len())
    }

    /// 写一段字节；越界返回 `false`（数据不写入）。
    pub fn write(&mut self, address: u16, data: &[u8]) -> bool {
        let Some(offset) = Self::offset(address, data.len()) else {
            return false;
        };
        self.mem[offset..offset + data.len()].copy_from_slice(data);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dl_user_memory_starts_right_after_registers() {
        assert_eq!(DL_USER_MEMORY_START, 0x1000);
        assert_eq!(DL_USER_MEMORY_LEN, 0xF000);
        assert_eq!(ESC_ADDRESS_SPACE_LEN, 0x1_0000);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let mut memory = DlUserMemory::new();
        assert!(memory.write(0x1000, &[1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(memory.read(0x1000, 8), Some(vec![1, 2, 3, 4, 5, 6, 7, 8]));
        // 相邻区间不受影响。
        assert_eq!(memory.read(0x1008, 8), Some(vec![0u8; 8]));
    }

    #[test]
    fn last_bytes_are_accessible() {
        let mut memory = DlUserMemory::new();
        assert!(memory.write(0xFFFE, &[0xAA, 0xBB]));
        assert_eq!(memory.read(0xFFFE, 2), Some(vec![0xAA, 0xBB]));
    }

    #[test]
    fn register_area_and_out_of_range_are_rejected() {
        let mut memory = DlUserMemory::new();
        // 寄存器区（< 0x1000）不属于 DL 用户内存。
        assert!(!memory.write(0x0800, &[1]));
        assert_eq!(memory.read(0x0FFF, 1), None);
        // 跨过 64 KiB 末端。
        assert!(!memory.write(0xFFFF, &[1, 2]));
        assert_eq!(memory.read(0xFFFF, 2), None);
        // 空区间视为非法访问。
        assert!(!DlUserMemory::covers(0x1000, 0));
        assert_eq!(memory.read(0x1000, 0), None);
        assert!(!memory.write(0x1000, &[]));
        // covers 与读写一致。
        assert!(DlUserMemory::covers(0x1000, DL_USER_MEMORY_LEN));
        assert!(!DlUserMemory::covers(0x0FFF, 2));
    }
}
