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
//! 尚未实现（B6）：SII CRC-8（word 0x3F）、FMMU/SM/TxPDO/RxPDO 类别、DC 类别。
//! IgH 与 SOEM 的扫描路径均不校验 CRC-8（IgH 仅做类别解析，SOEM 只在写 EEPROM 时生成），
//! 因此本镜像把 CRC 字留 0；接 TwinCAT 等会校验 CRC 的主站前必须补 B6。

/// SII 类别链起始 **word** 偏移（= 字节 0x80）。
pub const SII_FIRST_CATEGORY_WORD: u16 = 0x40;

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
}
