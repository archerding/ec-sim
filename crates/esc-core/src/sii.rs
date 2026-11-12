//! 最小 SII（Slave Information Interface / EEPROM）镜像。
//!
//! 仅供 B2b 最小 ESC 应答器使用：固定区字段 + 字符串类别 + 通用类别 + `0xFFFF` 终止。
//! 字段位置以**未修改主站实际读取的 word 偏移**为准（可用源码复核）：
//! - IgH `master/fsm_slave_scan.c`：`sii_words + 0x0004`（alias）、`+0x0008`（Vendor）、
//!   `+0x000A`（Product）、`+0x000C`（Revision）、`+0x000E`（Serial）、
//!   `+0x0014..0x001C`（邮箱）、类别链从 `EC_FIRST_SII_CATEGORY_OFFSET = 0x40` 开始；
//! - SOEM `src/ec_config.c` / `include/soem/ec_type.h`：`ECT_SII_MANUF = 0x0008`、
//!   `ECT_SII_ID = 0x000A`、`ECT_SII_REV = 0x000C`、`ECT_SII_SER = 0x000E`、
//!   `ECT_SII_START = 0x0040`（`ecx_siifind` 用 `ECT_SII_START << 1` 转字节地址）。
//!
//! 尚未实现（B6）：FMMU/SM/TxPDO/RxPDO 类别、DC 类别、ESI XML → SII 生成器。
//!
//! 固定区校验（ETG.1000.6 §5.4 table 16，已按两家未修改主站复核）：
//! - **word 0x0007：SII CRC-8**，覆盖 word 0x0000–0x0006（14 字节），多项式
//!   `x^8 + x^2 + x + 1`（0x07）、初值 0xFF、MSB-first（无反转、无 xorout）；
//! - **word 0x003E：EEPROM 大小**，单位 128 字节（值 0 = 128 字节）；
//! - **word 0x003F：SII 版本**，值为 1。
//!
//! 复核证据：SOEM `test/linux/eepromtool/eepromtool.c` 的 `SIIcrc()` 对前 14 字节求 CRC
//! 并写入 word 0x0007（`eeprom_writealias()`），`-i` 输出同时打印 "Checksum"（word 0x0007）
//! 与 "calculated"；IgH `tool/sii_crc.cpp` 的多项式/初值与此一致
//! （`pycrc --width=8 --poly=0x07 --reflect-in=0 --xor-in=0xff`）。
//! ⚠️ 早期文档把 CRC 记成 word 0x003F，已按上述来源更正；0x003F 是 SII 版本。

/// SII 类别链起始 **word** 偏移（= 字节 0x80）。
pub const SII_FIRST_CATEGORY_WORD: u16 = 0x40;

/// SII 固定区 CRC-8 所在 word（低字节存放校验值，高字节为 0）。
pub const SII_CHECKSUM_WORD: u16 = 0x0007;
/// CRC-8 覆盖的字节数：word 0x0000–0x0006 共 14 字节。
pub const SII_CHECKSUM_BYTES: usize = 14;
/// EEPROM 大小 word（单位 128 字节；值 0 = 128 字节）。
pub const SII_EEPROM_SIZE_WORD: u16 = 0x003E;
/// SII 版本 word。
pub const SII_VERSION_WORD: u16 = 0x003F;
/// 本镜像声明的 EEPROM 大小：值 1 = 2 KiBit = 256 字节（覆盖当前约 202 字节镜像）。
pub const SIM_EEPROM_SIZE: u16 = 1;
/// 本镜像声明的 SII 版本（ETG.1000.6：固定为 1）。
pub const SIM_SII_VERSION: u16 = 1;

/// SII CRC-8：多项式 `x^8 + x^2 + x + 1`（0x07）、初值 0xFF、MSB-first。
///
/// 覆盖范围由调用方给定：固定区校验必须传 word 0x0000–0x0006 的 14 个字节
/// （见 [`SII_CHECKSUM_BYTES`]）。
#[must_use]
pub fn sii_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0xFF;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// 读取超出镜像范围的 word 时返回的值（SII 未使用区的填充值）。
pub const SII_WORD_OUT_OF_RANGE: u16 = 0xFFFF;

/// 仿真从站 Vendor ID（我方占位值，非 ETG 注册号；R2：不使用厂商 VID/PID）。
pub const SIM_VENDOR_ID: u32 = 0x0000_0EC5;
/// 仿真从站 Product Code（我方占位值）。
pub const SIM_PRODUCT_CODE: u32 = 0x0000_E5C1;
/// 仿真从站 Revision Number。
pub const SIM_REVISION: u32 = 0x0000_0001;
/// 仿真从站 Serial Number。
pub const SIM_SERIAL: u32 = 0x0000_0001;

/// 字符串类别（0x000A）中的设备名（索引 0）与厂商名（索引 1）。
pub const SIM_NAMES: [&str; 2] = ["EC-Sim SIMULATED slave", "SIMULATED"];

/// 最小 SII 镜像（word 数组，1 word = 2 字节小端）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiiImage {
    words: Vec<u16>,
}

impl SiiImage {
    /// 构造"1 个仿真从站"的最小 SII：固定区 + 字符串类别 + 通用类别 + 终止字。
    ///
    /// 邮箱配置全为 0（不提供 mailbox），因此主站不会协商 CoE/FoE/EoE/SoE，
    /// 扫描停在 INIT —— 这正是 B2b"看到 1 个从站、不要求进 OP"的目标。
    #[must_use]
    pub fn simulated_slave() -> Self {
        let mut words = vec![0u16; usize::from(SII_FIRST_CATEGORY_WORD)];
        put_u32(&mut words, 0x0008, SIM_VENDOR_ID);
        put_u32(&mut words, 0x000A, SIM_PRODUCT_CODE);
        put_u32(&mut words, 0x000C, SIM_REVISION);
        put_u32(&mut words, 0x000E, SIM_SERIAL);
        // 0x0014..=0x001C 保持 0：无 mailbox、无 mailbox protocol。
        words[usize::from(SII_EEPROM_SIZE_WORD)] = SIM_EEPROM_SIZE;
        words[usize::from(SII_VERSION_WORD)] = SIM_SII_VERSION;
        // 固定区 CRC-8 必须在 0x0000–0x0006 全部确定后计算，写入 word 0x0007。
        words[usize::from(SII_CHECKSUM_WORD)] = u16::from(Self::fixed_area_crc8(&words));

        push_category(&mut words, 0x000A, &strings_category(&SIM_NAMES));
        // 通用类别固定 32 字节（IgH ec_slave_fetch_sii_general 要求 data_size == 32）。
        // data[0..3] 为 group/image/order/name 的字符串索引，全 0 → 名称字符串索引 0。
        push_category(&mut words, 0x001E, &[0u8; 32]);
        words.push(0xFFFF);
        Self { words }
    }

    /// 完整 word 数组。
    #[must_use]
    pub fn words(&self) -> &[u16] {
        &self.words
    }

    /// 读取一个 word；越界返回 [`SII_WORD_OUT_OF_RANGE`]。
    #[must_use]
    pub fn read_word(&self, offset: u16) -> u16 {
        self.words
            .get(usize::from(offset))
            .copied()
            .unwrap_or(SII_WORD_OUT_OF_RANGE)
    }

    /// 镜像长度（word 数）。
    #[must_use]
    pub fn len_words(&self) -> usize {
        self.words.len()
    }

    /// word 0x0007 中当前存储的 CRC-8（低字节）。
    #[must_use]
    pub fn checksum(&self) -> u8 {
        (self.read_word(SII_CHECKSUM_WORD) & 0x00FF) as u8
    }

    /// 按当前 word 0x0000–0x0006 重新计算固定区 CRC-8。
    #[must_use]
    pub fn calculate_checksum(&self) -> u8 {
        Self::fixed_area_crc8(&self.words)
    }

    /// word 0x0007 是否与重新计算值一致。
    #[must_use]
    pub fn verify_checksum(&self) -> bool {
        self.checksum() == self.calculate_checksum()
    }

    /// 设置站别名（word 0x0004）并重算 CRC-8。
    ///
    /// 对应 SOEM `eepromtool -walias` 的行为：改别名后必须重写 word 0x0007，
    /// 否则校验不一致（真实 EEPROM 的写入工具即如此处理）。
    pub fn set_alias(&mut self, alias: u16) {
        let Some(word) = self.words.get_mut(0x0004) else {
            return;
        };
        *word = alias;
        self.recompute_checksum();
    }

    /// 由 word 0x0000–0x0006 的 14 字节计算固定区 CRC-8（word 不足时按 0 补齐）。
    fn fixed_area_crc8(words: &[u16]) -> u8 {
        let mut bytes = [0u8; SII_CHECKSUM_BYTES];
        for (i, slot) in bytes.chunks_mut(2).enumerate() {
            if let Some(word) = words.get(i) {
                slot.copy_from_slice(&word.to_le_bytes());
            }
        }
        sii_crc8(&bytes)
    }

    fn recompute_checksum(&mut self) {
        let crc = Self::fixed_area_crc8(&self.words);
        if let Some(word) = self.words.get_mut(usize::from(SII_CHECKSUM_WORD)) {
            *word = u16::from(crc);
        }
    }
}

fn put_u32(words: &mut [u16], word_offset: usize, value: u32) {
    words[word_offset] = (value & 0xFFFF) as u16;
    words[word_offset + 1] = (value >> 16) as u16;
}

/// 按 ETG.1000.6 字符串类别格式编码：u8 条数 + 每条 (u8 长度 + 字节)，补齐偶数字节。
fn strings_category(strings: &[&str]) -> Vec<u8> {
    let mut data = Vec::new();
    data.push(strings.len() as u8);
    for s in strings {
        let bytes = s.as_bytes();
        data.push(bytes.len() as u8);
        data.extend_from_slice(bytes);
    }
    if data.len() % 2 == 1 {
        data.push(0);
    }
    data
}

fn push_category(words: &mut Vec<u16>, cat_type: u16, data: &[u8]) {
    let mut data = data.to_vec();
    if data.len() % 2 == 1 {
        data.push(0);
    }
    words.push(cat_type);
    words.push((data.len() / 2) as u16);
    for chunk in data.chunks(2) {
        words.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_area_has_identity_fields() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.read_word(0x0008), (SIM_VENDOR_ID & 0xFFFF) as u16);
        assert_eq!(sii.read_word(0x0009), (SIM_VENDOR_ID >> 16) as u16);
        assert_eq!(sii.read_word(0x000A), (SIM_PRODUCT_CODE & 0xFFFF) as u16);
        assert_eq!(sii.read_word(0x000C), (SIM_REVISION & 0xFFFF) as u16);
        assert_eq!(sii.read_word(0x000E), (SIM_SERIAL & 0xFFFF) as u16);
        // 无 mailbox：标准 mailbox protocol word 为 0
        assert_eq!(sii.read_word(0x001C), 0);
        // alias 为 0（word 0x0004，IgH 读取位置）
        assert_eq!(sii.read_word(0x0004), 0);
    }

    #[test]
    fn category_chain_is_walkable_and_terminated() {
        let sii = SiiImage::simulated_slave();
        let mut word = usize::from(SII_FIRST_CATEGORY_WORD);
        let mut types = Vec::new();
        loop {
            let cat_type = sii.words()[word];
            if cat_type == 0xFFFF {
                break;
            }
            let cat_size = usize::from(sii.words()[word + 1]);
            types.push(cat_type);
            word += 2 + cat_size;
            assert!(word < sii.len_words(), "category chain must stay in range");
        }
        assert_eq!(types, vec![0x000A, 0x001E]);
        // 终止字存在，且之后无多余 word
        assert_eq!(word + 1, sii.len_words());
    }

    #[test]
    fn strings_category_parses_back() {
        let sii = SiiImage::simulated_slave();
        let start = usize::from(SII_FIRST_CATEGORY_WORD) + 2;
        let data_word_len = usize::from(sii.words()[usize::from(SII_FIRST_CATEGORY_WORD) + 1]);
        let mut bytes = Vec::new();
        for w in &sii.words()[start..start + data_word_len] {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        assert_eq!(usize::from(bytes[0]), SIM_NAMES.len());
        let mut offset = 1;
        for name in SIM_NAMES {
            let len = usize::from(bytes[offset]);
            offset += 1;
            assert_eq!(&bytes[offset..offset + len], name.as_bytes());
            offset += len;
        }
    }

    #[test]
    fn out_of_range_reads_yield_sentinel() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.read_word(sii.len_words() as u16), SII_WORD_OUT_OF_RANGE);
    }

    #[test]
    fn crc8_matches_cross_checked_vectors() {
        // 向量由两个独立实现交叉校验：SOEM eepromtool 的逐位形式与
        // IgH tool/sii_crc.cpp 的 pycrc bit-feeding 形式（poly 0x07、init 0xFF、MSB-first）。
        assert_eq!(sii_crc8(&[]), 0xFF);
        assert_eq!(sii_crc8(&[0x00]), 0xF3);
        assert_eq!(sii_crc8(&[0, 1, 2, 3, 4, 5, 6]), 0x6B);
        assert_eq!(sii_crc8(&[0u8; SII_CHECKSUM_BYTES]), 0x30);
        let ascending: Vec<u8> = (0u8..SII_CHECKSUM_BYTES as u8).collect();
        assert_eq!(sii_crc8(&ascending), 0xE0);
    }

    #[test]
    fn checksum_word_matches_first_14_bytes() {
        let sii = SiiImage::simulated_slave();
        let mut bytes = Vec::new();
        for word in &sii.words()[..SII_CHECKSUM_BYTES / 2] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(bytes.len(), SII_CHECKSUM_BYTES);
        assert_eq!(sii.checksum(), sii_crc8(&bytes));
        assert!(sii.verify_checksum());
        // CRC-8 只占低字节，高字节必须为 0。
        assert_eq!(sii.read_word(SII_CHECKSUM_WORD) & 0xFF00, 0);
    }

    #[test]
    fn fixed_area_declares_eeprom_size_and_sii_version() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.read_word(SII_EEPROM_SIZE_WORD), SIM_EEPROM_SIZE);
        assert_eq!(sii.read_word(SII_VERSION_WORD), SIM_SII_VERSION);
        // 声明容量（(size + 1) * 128 字节）必须覆盖镜像本身。
        assert!(usize::from(SIM_EEPROM_SIZE + 1) * 128 >= sii.len_words() * 2);
    }

    #[test]
    fn alias_change_recomputes_checksum() {
        let mut sii = SiiImage::simulated_slave();
        let before = sii.checksum();
        sii.set_alias(0x1234);
        assert_eq!(sii.read_word(0x0004), 0x1234);
        assert_ne!(sii.checksum(), before);
        assert!(sii.verify_checksum());
    }

    #[test]
    fn checksum_detects_fixed_area_corruption() {
        let mut sii = SiiImage::simulated_slave();
        sii.words[0x0004] ^= 0x0001; // 破坏站别名（CRC 覆盖 word 0x0000–0x0006）
        assert!(!sii.verify_checksum());
        // 覆盖范围之外（Vendor ID 在 word 0x0008）的改变不影响校验。
        let mut sii = SiiImage::simulated_slave();
        sii.words[0x0008] ^= 0x0001;
        assert!(sii.verify_checksum());
    }
}
