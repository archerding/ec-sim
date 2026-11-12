//! ESC EEPROM 接口（0x0500–0x050F）：供主站通过寄存器读 SII。
//!
//! 协议依据 **ESC Sec.II（EEPROM Interface）**，并以下面两条未修改主站的实现复核：
//! - IgH `master/fsm_sii.c`：向 0x0502 写 4 字节 `[0x80, 0x01, addr_lo, addr_hi]`
//!   （`0x80` = 双字节地址模式，`0x01` = 读命令），再读 0x0502 起 10 字节，
//!   取偏移 +6 起的 4 字节作为数据；轮询状态字节的 busy/error 位（`& 0x81` / `& 0x20`）。
//! - SOEM `src/ec_main.c`：`ecx_readeeprom*` 向 0x0502 写 6 字节记录
//!   `comm(2) + addr(2) + d2(2)`，`EC_ECMD_READ = 0x0100`；状态按 u16 读 0x0502，
//!   `EC_ESTAT_BUSY = 0x8000`、`EC_ESTAT_R64 = 0x0040`（支持一次读 8 字节）。
//!
//! 因此本实现把 0x0500–0x050F 当作 16 字节窗口：命令写入即执行并把状态/数据回填到窗口，
//! 读操作只是读窗口。命令是**即时完成**的（不置 busy），主站轮询一次即可拿到数据。

use crate::sii::{SiiImage, SII_WORD_OUT_OF_RANGE};

/// EEPROM 接口寄存器窗口起始地址。
pub const EEPROM_WINDOW_START: u16 = 0x0500;
/// EEPROM 接口寄存器窗口长度。
pub const EEPROM_WINDOW_LEN: usize = 0x10;

/// 命令：读（SOEM `EC_ECMD_READ`）。
pub const EEPROM_CMD_READ: u16 = 0x0100;
/// 命令：写（SOEM `EC_ECMD_WRITE`）。
pub const EEPROM_CMD_WRITE: u16 = 0x0201;
/// 状态位：支持一次读 8 字节（R64）。
pub const EEPROM_STATUS_R64: u16 = 0x0040;
/// 状态位：写命令成功（IgH `fsm_sii.c` 检查的 high byte `0x40`）。
pub const EEPROM_STATUS_WRITE_ACK: u16 = 0x4000;

/// EEPROM 寄存器接口状态机。
#[derive(Debug, Clone)]
pub struct EepromInterface {
    image: SiiImage,
    window: [u8; EEPROM_WINDOW_LEN],
    read_commands: u32,
    write_commands: u32,
}

impl EepromInterface {
    /// 用给定 SII 镜像初始化（窗口全 0，即无 busy、无错误；状态字预先置 R64 能力位）。
    #[must_use]
    pub fn new(image: SiiImage) -> Self {
        let mut window = [0u8; EEPROM_WINDOW_LEN];
        window[2..4].copy_from_slice(&EEPROM_STATUS_R64.to_le_bytes());
        Self {
            image,
            window,
            read_commands: 0,
            write_commands: 0,
        }
    }

    /// 已执行的读命令数。
    #[must_use]
    pub fn read_commands(&self) -> u32 {
        self.read_commands
    }

    /// 只读访问本接口承载的 SII 镜像（如 ESM 前置条件按 SII 声明推导能力）。
    #[must_use]
    pub fn image(&self) -> &SiiImage {
        &self.image
    }

    /// 已执行的写命令数。
    #[must_use]
    pub fn write_commands(&self) -> u32 {
        self.write_commands
    }

    /// 该地址是否落在 EEPROM 接口窗口内。
    #[must_use]
    pub fn covers(address: u16, len: usize) -> bool {
        let start = u32::from(address);
        let end = start + len as u32;
        start >= u32::from(EEPROM_WINDOW_START)
            && end <= u32::from(EEPROM_WINDOW_START) + EEPROM_WINDOW_LEN as u32
    }

    /// 读窗口内字节（调用方保证 [`Self::covers`]）。
    #[must_use]
    pub fn read(&self, address: u16, len: usize) -> Vec<u8> {
        let start = usize::from(address - EEPROM_WINDOW_START);
        let mut out = vec![0u8; len];
        for (i, byte) in out.iter_mut().enumerate() {
            let idx = start + i;
            if idx < EEPROM_WINDOW_LEN {
                *byte = self.window[idx];
            }
        }
        out
    }

    /// 写窗口内字节（调用方保证 [`Self::covers`]）。命中命令寄存器时立即执行命令。
    pub fn write(&mut self, address: u16, data: &[u8]) {
        let start = usize::from(address - EEPROM_WINDOW_START);
        for (i, byte) in data.iter().enumerate() {
            let idx = start + i;
            if idx < EEPROM_WINDOW_LEN {
                self.window[idx] = *byte;
            }
        }
        let end = start + data.len();
        // 命令字（0x0502）被完整写入即执行；主站总是把命令与地址一起写。
        if start <= 2 && end >= 4 {
            self.execute_command();
        }
    }

    fn execute_command(&mut self) {
        let command = u16::from_le_bytes([self.window[2], self.window[3]]);
        let word_address = u16::from_le_bytes([self.window[4], self.window[5]]);
        let operation = command & 0x0F00;
        let status = if operation == EEPROM_CMD_READ {
            self.read_commands = self.read_commands.saturating_add(1);
            for i in 0..4u16 {
                let value = self.image.read_word(word_address.wrapping_add(i));
                let offset = 8 + 2 * usize::from(i);
                self.window[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            }
            EEPROM_STATUS_R64
        } else if operation == (EEPROM_CMD_WRITE & 0x0F00) {
            // SII 在本阶段只读；记录次数以证明主站写路径被正确解析。
            self.write_commands = self.write_commands.saturating_add(1);
            EEPROM_STATUS_R64 | EEPROM_STATUS_WRITE_ACK
        } else {
            // NOP / reload：清状态与数据。
            self.window[8..].fill(0);
            EEPROM_STATUS_R64
        };
        self.window[2..4].copy_from_slice(&status.to_le_bytes());
        self.window[4..6].copy_from_slice(&word_address.to_le_bytes());
        let _ = SII_WORD_OUT_OF_RANGE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sii::{SIM_PRODUCT_CODE, SIM_VENDOR_ID};

    fn interface() -> EepromInterface {
        EepromInterface::new(SiiImage::simulated_slave())
    }

    #[test]
    fn idle_status_is_not_busy_and_advertises_r64() {
        let eeprom = interface();
        let status = u16::from_le_bytes([eeprom.read(0x0502, 2)[0], eeprom.read(0x0502, 2)[1]]);
        assert_eq!(status & 0x8000, 0, "busy must be clear");
        assert_ne!(status & EEPROM_STATUS_R64, 0, "R64 must be advertised");
    }

    #[test]
    fn igh_read_sequence_returns_vendor_word() {
        let mut eeprom = interface();
        // IgH: 写 4 字节 [0x80, 0x01, word_lo, word_hi] 到 0x0502（读 word 0x0008）
        eeprom.write(0x0502, &[0x80, 0x01, 0x08, 0x00]);
        // 读 0x0502 起 10 字节；data = 偏移 6..10
        let reply = eeprom.read(0x0502, 10);
        let status = u16::from_le_bytes([reply[0], reply[1]]);
        assert_eq!(status & 0x8000, 0);
        assert_eq!(status & 0x2000, 0, "no error bits");
        let data = u32::from_le_bytes([reply[6], reply[7], reply[8], reply[9]]);
        assert_eq!(data, SIM_VENDOR_ID);
        assert_eq!(eeprom.read_commands(), 1);
    }

    #[test]
    fn soem_read_sequence_returns_eight_bytes() {
        let mut eeprom = interface();
        // SOEM: 写 6 字节记录 comm + addr + d2 到 0x0502（读 word 0x000A）
        let mut record = Vec::new();
        record.extend_from_slice(&EEPROM_CMD_READ.to_le_bytes());
        record.extend_from_slice(&0x000Au16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        eeprom.write(0x0502, &record);
        // SOEM 读 8 字节数据（0x0508）
        let data = eeprom.read(0x0508, 8);
        let product = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        assert_eq!(product, SIM_PRODUCT_CODE);
        // 8 字节读覆盖 word 0x0A..0x0D：紧跟 Product 的是 Revision（word 0x0C）。
        assert_eq!(
            u16::from_le_bytes([data[4], data[5]]),
            (crate::sii::SIM_REVISION & 0xFFFF) as u16
        );
    }

    #[test]
    fn out_of_range_words_return_sentinel() {
        let mut eeprom = interface();
        eeprom.write(0x0502, &[0x80, 0x01, 0x00, 0x70]);
        let reply = eeprom.read(0x0502, 10);
        assert_eq!(
            u32::from_le_bytes([reply[6], reply[7], reply[8], reply[9]]),
            0xFFFF_FFFF
        );
    }

    #[test]
    fn write_command_is_counted_and_acked() {
        let mut eeprom = interface();
        eeprom.write(0x0508, &[0x34, 0x12]);
        let mut record = Vec::new();
        record.extend_from_slice(&EEPROM_CMD_WRITE.to_le_bytes());
        record.extend_from_slice(&0x0010u16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        eeprom.write(0x0502, &record);
        let status = u16::from_le_bytes([eeprom.read(0x0502, 2)[0], eeprom.read(0x0502, 2)[1]]);
        assert_ne!(status & EEPROM_STATUS_WRITE_ACK, 0);
        assert_eq!(eeprom.write_commands(), 1);
    }

    #[test]
    fn covers_only_the_register_window() {
        assert!(EepromInterface::covers(0x0502, 10));
        assert!(EepromInterface::covers(0x0500, 1));
        assert!(!EepromInterface::covers(0x0502, 15));
        assert!(!EepromInterface::covers(0x04FE, 4));
        assert!(!EepromInterface::covers(0x0400, 4));
    }

    #[test]
    fn alias_register_word_is_zero() {
        let mut eeprom = interface();
        eeprom.write(0x0502, &[0x80, 0x01, 0x04, 0x00]);
        let reply = eeprom.read(0x0502, 10);
        assert_eq!(
            u32::from_le_bytes([reply[6], reply[7], reply[8], reply[9]]),
            0
        );
    }
}
