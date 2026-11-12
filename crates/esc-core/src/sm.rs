//! Sync Manager 寄存器块（0x0800 起，每个通道 8 字节）。
//!
//! 依据 **ETG.1000.4 table 58 / ESC Sec.II（Registers）**。寄存器布局（通道内偏移）：
//!
//! | 偏移 | 字段 | 主站访问 |
//! |---|---|---|
//! | 0x00–0x01 | Physical Start Address | 读写 |
//! | 0x02–0x03 | Length | 读写 |
//! | 0x04 | Control | 读写 |
//! | 0x05 | Status | **只读** |
//! | 0x06 | Activate | 读写 |
//! | 0x07 | PDI Control | **只读**（本模拟器暂无 PDI） |
//!
//! Control 字节（ETG.1000.4 table 58）：
//! - bit0–1：operation mode（0 = buffered，2 = mailbox；1/3 保留）；
//! - bit2–3：direction（0 = read，1 = write，2 = read/write）；
//! - bit4：ECAT event enable；bit5：DLS-user event enable；bit6：watchdog enable；bit7 保留。
//!
//! Status 字节：bit0 write event、bit1 read event、bit2 保留、bit3 mailbox full、
//! bit4–5 buffer state、bit6 read buffer open、bit7 write buffer open。
//!
//! 主站实际使用的通道约定（IgH/SOEM）：SM0 = mailbox out（主站写）、SM1 = mailbox in，
//! SM2 = process data out、SM3 = process data in；本模块只存寄存器语义，通道用途由 SII
//! 的 SM 类别（0x0029）声明，留到 B6/B7。
//!
//! ⚠️ 尚未实现：SM 触发的 mailbox 握手/中断、watchdog 计时；这些属 B5 后续子步骤与 B7。

use crate::Registers;

/// 第一个 Sync Manager 通道的寄存器地址（0x0800）。
pub const SYNC_MANAGER_ADDRESS: u16 = 0x0800;

/// 单个通道的寄存器大小（字节）。
pub const SYNC_MANAGER_SIZE: u16 = 8;

/// 寄存器实现的通道数上限（0x0800–0x083F）。
pub const SYNC_MANAGER_MAX: u8 = 8;

/// 通道内偏移：Physical Start Address。
pub const OFFSET_START_ADDRESS: u16 = 0x00;
/// 通道内偏移：Length。
pub const OFFSET_LENGTH: u16 = 0x02;
/// 通道内偏移：Control。
pub const OFFSET_CONTROL: u16 = 0x04;
/// 通道内偏移：Status（只读）。
pub const OFFSET_STATUS: u16 = 0x05;
/// 通道内偏移：Activate。
pub const OFFSET_ACTIVATE: u16 = 0x06;
/// 通道内偏移：PDI Control（只读）。
pub const OFFSET_PDI_CONTROL: u16 = 0x07;

/// 第 `index` 个通道的首地址；越界返回 [`None`]。
#[must_use]
pub fn channel_address(index: u8) -> Option<u16> {
    if index >= SYNC_MANAGER_MAX {
        return None;
    }
    Some(SYNC_MANAGER_ADDRESS + u16::from(index) * SYNC_MANAGER_SIZE)
}

/// 该寄存器地址是否是 Sync Manager 区的只读字节（Status / PDI Control）。
#[must_use]
pub fn is_read_only_byte(address: u16) -> bool {
    if !(SYNC_MANAGER_ADDRESS
        ..SYNC_MANAGER_ADDRESS + u16::from(SYNC_MANAGER_MAX) * SYNC_MANAGER_SIZE)
        .contains(&address)
    {
        return false;
    }
    let offset = (address - SYNC_MANAGER_ADDRESS) % SYNC_MANAGER_SIZE;
    offset == OFFSET_STATUS || offset == OFFSET_PDI_CONTROL
}

/// 一个 Sync Manager 通道的寄存器值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncManager {
    /// 0x00–0x01：物理起始地址（DL-user 内存中的偏移）。
    pub start_address: u16,
    /// 0x02–0x03：长度（字节）。
    pub length: u16,
    /// 0x04：Control（模式/方向/事件/watchdog）。
    pub control: u8,
    /// 0x05：Status（只读）。
    pub status: u8,
    /// 0x06：Activate。
    pub activate: u8,
    /// 0x07：PDI Control（只读）。
    pub pdi_control: u8,
}

impl SyncManager {
    /// 从通道的 8 字节寄存器内容解码。
    #[must_use]
    pub fn from_bytes(bytes: &[u8; SYNC_MANAGER_SIZE as usize]) -> Self {
        Self {
            start_address: u16::from_le_bytes([bytes[0], bytes[1]]),
            length: u16::from_le_bytes([bytes[2], bytes[3]]),
            control: bytes[4],
            status: bytes[5],
            activate: bytes[6],
            pdi_control: bytes[7],
        }
    }

    /// 编码为通道的 8 字节寄存器内容。
    #[must_use]
    pub fn to_bytes(self) -> [u8; SYNC_MANAGER_SIZE as usize] {
        let mut bytes = [0u8; SYNC_MANAGER_SIZE as usize];
        bytes[0..2].copy_from_slice(&self.start_address.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.length.to_le_bytes());
        bytes[4] = self.control;
        bytes[5] = self.status;
        bytes[6] = self.activate;
        bytes[7] = self.pdi_control;
        bytes
    }

    /// 读寄存器阵列中的第 `index` 个通道；越界返回 [`None`]。
    #[must_use]
    pub fn read(registers: &Registers, index: u8) -> Option<Self> {
        let base = channel_address(index)?;
        let mut bytes = [0u8; SYNC_MANAGER_SIZE as usize];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = registers.read_u8(base + i as u16)?;
        }
        Some(Self::from_bytes(&bytes))
    }

    /// 通道是否已激活（Activate.bit0）。
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self.activate & 0x01 != 0
    }

    /// 通道是否配置为 mailbox 模式（Control.bit0–1 == 2）。
    #[must_use]
    pub fn is_mailbox(self) -> bool {
        self.control & 0x03 == 0x02
    }

    /// 方向字段（Control.bit2–3）：0 = read、1 = write、2 = read/write。
    #[must_use]
    pub fn direction(self) -> u8 {
        (self.control >> 2) & 0x03
    }

    /// 是否使能 watchdog（Control.bit6）。
    #[must_use]
    pub fn is_watchdog_enabled(self) -> bool {
        self.control & 0x40 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_addresses_are_eight_bytes_apart() {
        assert_eq!(channel_address(0), Some(0x0800));
        assert_eq!(channel_address(1), Some(0x0808));
        assert_eq!(channel_address(3), Some(0x0818));
        assert_eq!(channel_address(SYNC_MANAGER_MAX), None);
    }

    #[test]
    fn read_only_bytes_are_status_and_pdi_control() {
        for index in 0..SYNC_MANAGER_MAX {
            let base = channel_address(index).expect("in range");
            assert!(!is_read_only_byte(base + OFFSET_START_ADDRESS));
            assert!(!is_read_only_byte(base + OFFSET_LENGTH));
            assert!(!is_read_only_byte(base + OFFSET_CONTROL));
            assert!(is_read_only_byte(base + OFFSET_STATUS));
            assert!(!is_read_only_byte(base + OFFSET_ACTIVATE));
            assert!(is_read_only_byte(base + OFFSET_PDI_CONTROL));
        }
        assert!(!is_read_only_byte(0x07FF));
        assert!(!is_read_only_byte(
            SYNC_MANAGER_ADDRESS + 8 * SYNC_MANAGER_SIZE
        ));
    }

    #[test]
    fn roundtrip_and_field_decoding() {
        // SOEM 的 mailbox SM 默认 flags：control=0x26（mailbox + write + PDI event）、activate=1。
        let sm = SyncManager {
            start_address: 0x1000,
            length: 0x0080,
            control: 0x26,
            status: 0,
            activate: 0x01,
            pdi_control: 0,
        };
        let bytes = sm.to_bytes();
        assert_eq!(bytes, [0x00, 0x10, 0x80, 0x00, 0x26, 0x00, 0x01, 0x00]);
        assert_eq!(SyncManager::from_bytes(&bytes), sm);
        assert!(sm.is_enabled());
        assert!(sm.is_mailbox());
        assert_eq!(sm.direction(), 1, "write direction");
        assert!(!sm.is_watchdog_enabled());

        // SM1（mailbox in）用 control=0x22（mailbox + read + PDI event）。
        let sm1 = SyncManager::from_bytes(&[0x80, 0x10, 0x80, 0x00, 0x22, 0x00, 0x01, 0x00]);
        assert!(sm1.is_mailbox());
        assert_eq!(sm1.direction(), 0, "read direction");
        assert_eq!(sm1.start_address, 0x1080);

        // buffered + watchdog 的 process data 通道：control=0x40（bit6）不影响模式判定。
        let buffered = SyncManager {
            control: 0x40,
            ..SyncManager::default()
        };
        assert!(!buffered.is_mailbox());
        assert!(buffered.is_watchdog_enabled());
        assert!(!buffered.is_enabled());
    }

    #[test]
    fn registers_reject_master_writes_to_status_and_pdi_control() {
        let mut registers = Registers::new();
        let base = channel_address(0).expect("in range");
        // 主站按 8 字节块写：start/length/control/activate 生效，status/PDI control 保持只读。
        let master = SyncManager {
            start_address: 0x1000,
            length: 0x0080,
            control: 0x26,
            status: 0xAA,
            activate: 0x01,
            pdi_control: 0xBB,
        };
        for (i, byte) in master.to_bytes().iter().enumerate() {
            registers.write_u8(base + i as u16, *byte);
        }
        let stored = SyncManager::read(&registers, 0).expect("in range");
        assert_eq!(stored.start_address, 0x1000);
        assert_eq!(stored.length, 0x0080);
        assert_eq!(stored.control, 0x26);
        assert_eq!(stored.activate, 0x01);
        assert_eq!(stored.status, 0, "Status 只读");
        assert_eq!(stored.pdi_control, 0, "PDI Control 只读");
    }
}
