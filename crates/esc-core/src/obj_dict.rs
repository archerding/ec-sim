//! 最小 CoE 对象字典（B7 子项 2）：只读、小整数对象，供 SDO **expedited upload**。
//!
//! 目的：让主站（IgH / SOEM）能读到配置 PDO 域所需的映射对象，从而不再只拿到 Abort；
//! 对象值与 SII 类别（[`crate::sii`]）保持**一致**：SM2 输出 PDO `0x1600` 映射
//! `0x7000:00`（8 位），SM3 输入 PDO `0x1A00` 映射 `0x6000:00`（8 位）。
//!
//! 依据（两家**未修改**主站读取的对象，只读参考、未复制代码）：
//! - SOEM `src/ec_config.c: ecx_readPDOmap()` 依次读 `0x1C00:00`（SM 数）、
//!   `0x1C00:0n`（SM 类型：1 = mailbox out、2 = mailbox in、3 = outputs、4 = inputs）、
//!   `0x1C12:00/0x1C12:0n`（RxPDO assign）、`0x1600:00/0x1600:0n`（RxPDO 映射）、
//!   `0x1C13:00/0x1C13:0n`、`0x1A00:…`（TxPDO 侧）；
//! - IgH `master/fsm_coe.c` 的 SDO 上传路径与 `ethercat upload` 工具都使用标准
//!   expedited upload 编码（见 [`crate::mailbox`]）。
//!
//! ⚠️ 未实现：SDO Download（写对象）、SDO Info 服务、Complete Access、分段（segmented）传输、
//! 以及 PDO 数据对象 `0x6000`/`0x7000` 的动态读写（PDO 走 FMMU 逻辑寻址，不走 SDO）。

/// 对象字典条目：索引 + 子索引 + 小端编码的值（长度 1..=4，适合 expedited upload）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectEntry {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// 值（小端字节序）。
    pub value: &'static [u8],
}

impl ObjectEntry {
    /// 编码为 8 字节 SDO 响应数据（CoE 头 2 + SDO 命令/索引/子索引 4 + 值 4 字节补齐）。
    ///
    /// 返回 `(SDO 命令字节, 8 字节数据)`；值长度 > 4 时返回 [`None`]（本字典不含大对象）。
    #[must_use]
    pub fn sdo_upload_payload(self) -> Option<(u8, [u8; 8])> {
        let size = self.value.len();
        if size == 0 || size > 4 {
            return None;
        }
        // CiA 301 expedited upload: 0x40 | exp(0x02) | (4-size)<<2 | size_indicated(0x01)。
        let command = 0x40u8 | 0x02 | (((4 - size) as u8) << 2) | 0x01;
        let mut data = [0u8; 8];
        data[0] = command;
        data[1..3].copy_from_slice(&self.index.to_le_bytes());
        data[3] = self.subindex;
        data[4..4 + size].copy_from_slice(self.value);
        Some((command, data))
    }
}

const fn u8_le(value: u8) -> [u8; 1] {
    [value]
}
const fn u16_le(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}
const fn u32_le(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

/// 对象 0x1C00:00 的值：Sync Manager 数（4）。
const SM_COUNT: [u8; 1] = u8_le(4);
/// SM 类型：1 = mailbox out、2 = mailbox in、3 = outputs、4 = inputs（与 SOEM `SMtype` 约定一致）。
const SM_TYPE_MAILBOX_OUT: [u8; 1] = u8_le(1);
const SM_TYPE_MAILBOX_IN: [u8; 1] = u8_le(2);
const SM_TYPE_OUTPUTS: [u8; 1] = u8_le(3);
const SM_TYPE_INPUTS: [u8; 1] = u8_le(4);
/// 0x1C12/0x1C13 子索引 0：分配的 PDO 数量（各 1 个）。
const PDO_ASSIGN_COUNT: [u8; 1] = u8_le(1);
/// 输出 PDO 索引 0x1600（RxPDO assign 0x1C12:01）。
const RX_PDO_INDEX: [u8; 2] = u16_le(0x1600);
/// 输入 PDO 索引 0x1A00（TxPDO assign 0x1C13:01）。
const TX_PDO_INDEX: [u8; 2] = u16_le(0x1A00);
/// PDO 映射条目数量（各 1 条）。
const PDO_MAPPING_COUNT: [u8; 1] = u8_le(1);
/// 输出映射：0x7000:00，8 位。
const RX_PDO_MAPPING: [u8; 4] = u32_le(0x7000_0008);
/// 输入映射：0x6000:00，8 位。
const TX_PDO_MAPPING: [u8; 4] = u32_le(0x6000_0008);
/// 0x1018:00：Identity 子索引数（4）。
const IDENTITY_SUBINDEX_COUNT: [u8; 1] = u8_le(4);
/// 0x1018:01 Vendor ID（与 SII 固定区一致；我方占位值）。
const IDENTITY_VENDOR_ID: [u8; 4] = u32_le(crate::sii::SIM_VENDOR_ID);
/// 0x1018:02 Product Code。
const IDENTITY_PRODUCT_CODE: [u8; 4] = u32_le(crate::sii::SIM_PRODUCT_CODE);
/// 0x1018:03 Revision Number。
const IDENTITY_REVISION: [u8; 4] = u32_le(crate::sii::SIM_REVISION);
/// 0x1018:04 Serial Number。
const IDENTITY_SERIAL: [u8; 4] = u32_le(crate::sii::SIM_SERIAL);

/// 仿真从站的最小对象字典（只读）。
pub const SIM_OBJECTS: &[ObjectEntry] = &[
    // Identity（0x1018）——主站/工具查询设备身份用。
    ObjectEntry {
        index: 0x1018,
        subindex: 0,
        value: &IDENTITY_SUBINDEX_COUNT,
    },
    ObjectEntry {
        index: 0x1018,
        subindex: 1,
        value: &IDENTITY_VENDOR_ID,
    },
    ObjectEntry {
        index: 0x1018,
        subindex: 2,
        value: &IDENTITY_PRODUCT_CODE,
    },
    ObjectEntry {
        index: 0x1018,
        subindex: 3,
        value: &IDENTITY_REVISION,
    },
    ObjectEntry {
        index: 0x1018,
        subindex: 4,
        value: &IDENTITY_SERIAL,
    },
    // Sync Manager 通信类型（0x1C00）。
    ObjectEntry {
        index: 0x1C00,
        subindex: 0,
        value: &SM_COUNT,
    },
    ObjectEntry {
        index: 0x1C00,
        subindex: 1,
        value: &SM_TYPE_MAILBOX_OUT,
    },
    ObjectEntry {
        index: 0x1C00,
        subindex: 2,
        value: &SM_TYPE_MAILBOX_IN,
    },
    ObjectEntry {
        index: 0x1C00,
        subindex: 3,
        value: &SM_TYPE_OUTPUTS,
    },
    ObjectEntry {
        index: 0x1C00,
        subindex: 4,
        value: &SM_TYPE_INPUTS,
    },
    // RxPDO assign（0x1C12）→ 0x1600。
    ObjectEntry {
        index: 0x1C12,
        subindex: 0,
        value: &PDO_ASSIGN_COUNT,
    },
    ObjectEntry {
        index: 0x1C12,
        subindex: 1,
        value: &RX_PDO_INDEX,
    },
    // TxPDO assign（0x1C13）→ 0x1A00。
    ObjectEntry {
        index: 0x1C13,
        subindex: 0,
        value: &PDO_ASSIGN_COUNT,
    },
    ObjectEntry {
        index: 0x1C13,
        subindex: 1,
        value: &TX_PDO_INDEX,
    },
    // RxPDO 映射（0x1600）。
    ObjectEntry {
        index: 0x1600,
        subindex: 0,
        value: &PDO_MAPPING_COUNT,
    },
    ObjectEntry {
        index: 0x1600,
        subindex: 1,
        value: &RX_PDO_MAPPING,
    },
    // TxPDO 映射（0x1A00）。
    ObjectEntry {
        index: 0x1A00,
        subindex: 0,
        value: &PDO_MAPPING_COUNT,
    },
    ObjectEntry {
        index: 0x1A00,
        subindex: 1,
        value: &TX_PDO_MAPPING,
    },
];

/// 按索引+子索引查字典；不存在返回 [`None`]（调用方回 SDO Abort 0x06020000）。
#[must_use]
pub fn read_object(index: u16, subindex: u8) -> Option<ObjectEntry> {
    SIM_OBJECTS
        .iter()
        .find(|entry| entry.index == index && entry.subindex == subindex)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sm_communication_type_matches_soem_expectations() {
        assert_eq!(read_object(0x1C00, 0).map(|e| e.value), Some(&SM_COUNT[..]));
        let types: Vec<u8> = (1..=4)
            .map(|sub| read_object(0x1C00, sub).expect("SM type").value[0])
            .collect();
        assert_eq!(types, vec![1, 2, 3, 4], "mailbox out/in + outputs/inputs");
    }

    #[test]
    fn pdo_assign_and_mapping_objects_are_consistent() {
        assert_eq!(
            read_object(0x1C12, 1).map(|e| e.value),
            Some(&RX_PDO_INDEX[..])
        );
        assert_eq!(
            read_object(0x1C13, 1).map(|e| e.value),
            Some(&TX_PDO_INDEX[..])
        );
        assert_eq!(
            read_object(0x1600, 1).map(|e| e.value),
            Some(&RX_PDO_MAPPING[..])
        );
        assert_eq!(
            read_object(0x1A00, 1).map(|e| e.value),
            Some(&TX_PDO_MAPPING[..])
        );
        // 0x7000:00 8 位 = 0x7000_0008；0x6000:00 8 位 = 0x6000_0008。
        assert_eq!(
            u32::from_le_bytes(RX_PDO_MAPPING),
            (u32::from(0x7000u16) << 16) | 0x08
        );
        assert_eq!(
            u32::from_le_bytes(TX_PDO_MAPPING),
            (u32::from(0x6000u16) << 16) | 0x08
        );
    }

    #[test]
    fn identity_matches_sii() {
        assert_eq!(
            u32::from_le_bytes(IDENTITY_VENDOR_ID),
            crate::sii::SIM_VENDOR_ID
        );
        assert_eq!(
            u32::from_le_bytes(IDENTITY_PRODUCT_CODE),
            crate::sii::SIM_PRODUCT_CODE
        );
        assert_eq!(
            u32::from_le_bytes(IDENTITY_REVISION),
            crate::sii::SIM_REVISION
        );
        assert_eq!(u32::from_le_bytes(IDENTITY_SERIAL), crate::sii::SIM_SERIAL);
    }

    #[test]
    fn expedited_upload_payload_sizes() {
        // 4 字节对象（0x1600:01 PDO 映射）：命令 0x43（expedited，4 字节）。
        let (command, data) = read_object(0x1600, 1)
            .expect("object")
            .sdo_upload_payload()
            .expect("payload");
        assert_eq!(command, 0x43);
        assert_eq!(&data[..3], &[0x43, 0x00, 0x16]);
        assert_eq!(data[3], 0x01);
        assert_eq!(
            u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            0x7000_0008
        );

        // 2 字节对象（0x1C13:01 PDO assign）：命令 0x4B。
        let (command, data) = read_object(0x1C13, 1)
            .expect("object")
            .sdo_upload_payload()
            .expect("payload");
        assert_eq!(command, 0x4B);
        assert_eq!(&data[4..6], &TX_PDO_INDEX);

        // 1 字节对象：命令 0x4F。
        let (command, data) = read_object(0x1C00, 0)
            .expect("object")
            .sdo_upload_payload()
            .expect("payload");
        assert_eq!(command, 0x4F);
        assert_eq!(data[4], 4);
        assert_eq!(&data[5..8], &[0, 0, 0], "不足 4 字节补零");
    }

    #[test]
    fn unknown_objects_return_none() {
        assert!(read_object(0x2000, 0).is_none());
        assert!(read_object(0x1C12, 2).is_none());
    }
}
