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
//! ⚠️ 未实现：SDO Info 服务、Complete Access、分段（segmented）传输；
//! PDO 数据对象 `0x6000`/`0x7000` 的动态读写（PDO 走 FMMU 逻辑寻址，不走 SDO）。
//! Download 只对 [`is_writable`] 列出的配置对象开放（值保存在模拟器内，不回写 SII）。

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

/// 是否允许 SDO **Download**（写对象）。
///
/// 只开放主站配置 PDO 域时会写的对象：PDO assign（`0x1C12/0x1C13`）、PDO 映射
/// （`0x1600/0x1A00`）与 SM 同步参数（`0x1C32/0x1C33`）。其余对象只读：主站写时
/// 回 Abort `0x06010002`。写入值由模拟器保存，供后续 Upload 读回（不回写 SII 类别）。
#[must_use]
pub fn is_writable(index: u16) -> bool {
    matches!(index, 0x1C12 | 0x1C13 | 0x1600 | 0x1A00 | 0x1C32 | 0x1C33)
}

/// 对象代码（ETG.1000.6 对象描述：Get OD 响应中的 ObjectCode 字段）。
pub mod object_code {
    /// 变量对象。
    pub const VAR: u8 = 0x07;
    /// 数组对象。
    pub const ARRAY: u8 = 0x08;
    /// 记录对象。
    pub const RECORD: u8 = 0x09;
}

/// 数据类型编码（ETG.1000.6 / `SOEM include/soem/ec_type.h: ec_datatype`）。
pub mod data_type {
    /// BOOLEAN。
    pub const BOOLEAN: u16 = 0x0001;
    /// INTEGER8。
    pub const INTEGER8: u16 = 0x0002;
    /// INTEGER16。
    pub const INTEGER16: u16 = 0x0003;
    /// INTEGER32。
    pub const INTEGER32: u16 = 0x0004;
    /// UNSIGNED8。
    pub const UNSIGNED8: u16 = 0x0005;
    /// UNSIGNED16。
    pub const UNSIGNED16: u16 = 0x0006;
    /// UNSIGNED32。
    pub const UNSIGNED32: u16 = 0x0007;
    /// VISIBLE_STRING。
    pub const VISIBLE_STRING: u16 = 0x0009;
    /// OCTET_STRING。
    pub const OCTET_STRING: u16 = 0x000A;
    /// 复合对象（ARRAY/RECORD）在对象描述中的 DataType 字段按惯例填 0。
    pub const NONE: u16 = 0x0000;
}

/// 对象条目访问权限位（ETG.1000.6；IgH `fsm_coe.c: ec_fsm_coe_dict_entry_response()`
/// 的位解析：bit0 R-preop、bit1 R-safeop、bit2 R-op、bit3 W-preop、bit4 W-safeop、bit5 W-op）。
pub mod access {
    /// 只读（三个状态都可读）。
    pub const READ_ONLY: u16 = 0x0007;
    /// PREOP 可读可写（PDO 配置对象）。
    pub const READ_WRITE_PREOP: u16 = 0x0009;
}

/// 对象描述（SDO Info **Get OD** 响应用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectDescription {
    /// 对象索引。
    pub index: u16,
    /// 最大子索引（0 表示只有子索引 0）。
    pub max_subindex: u8,
    /// 对象代码，见 [`object_code`]。
    pub object_code: u8,
    /// 对象数据类型，见 [`data_type`]。
    pub data_type: u16,
    /// 对象名（GET OD 响应中的名称字符串）。
    pub name: &'static str,
}

/// 对象条目描述（SDO Info **Get Entry Description** 响应用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryDescription {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// 数据类型，见 [`data_type`]。
    pub data_type: u16,
    /// 位长度。
    pub bit_length: u16,
    /// 访问权限位，见 [`access`]。
    pub access: u16,
    /// ValueInfo：0 = 普通值（ETG.1000.6 SDO Info）。
    pub value_info: u8,
    /// 条目名。
    pub name: &'static str,
}

/// 本镜像的对象描述表（顺序即 Get OD List 返回的索引顺序）。
pub const SIM_OBJECT_DESCRIPTIONS: &[ObjectDescription] = &[
    ObjectDescription {
        index: 0x1018,
        max_subindex: 4,
        object_code: object_code::RECORD,
        data_type: data_type::NONE,
        name: "Identity",
    },
    ObjectDescription {
        index: 0x1C00,
        max_subindex: 4,
        object_code: object_code::ARRAY,
        data_type: data_type::UNSIGNED8,
        name: "Sync manager communication type",
    },
    ObjectDescription {
        index: 0x1C12,
        max_subindex: 1,
        object_code: object_code::ARRAY,
        data_type: data_type::UNSIGNED16,
        name: "RxPDO assign",
    },
    ObjectDescription {
        index: 0x1C13,
        max_subindex: 1,
        object_code: object_code::ARRAY,
        data_type: data_type::UNSIGNED16,
        name: "TxPDO assign",
    },
    ObjectDescription {
        index: 0x1600,
        max_subindex: 1,
        object_code: object_code::ARRAY,
        data_type: data_type::UNSIGNED32,
        name: "RxPDO mapping",
    },
    ObjectDescription {
        index: 0x1A00,
        max_subindex: 1,
        object_code: object_code::ARRAY,
        data_type: data_type::UNSIGNED32,
        name: "TxPDO mapping",
    },
];

/// 本镜像的条目描述表（顺序与 [`SIM_OBJECTS`] 一致）。
pub const SIM_ENTRY_DESCRIPTIONS: &[EntryDescription] = &[
    EntryDescription {
        index: 0x1018,
        subindex: 0,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_ONLY,
        value_info: 0,
        name: "Number of entries",
    },
    EntryDescription {
        index: 0x1018,
        subindex: 1,
        data_type: data_type::UNSIGNED32,
        bit_length: 32,
        access: access::READ_ONLY,
        value_info: 0,
        name: "Vendor ID",
    },
    EntryDescription {
        index: 0x1018,
        subindex: 2,
        data_type: data_type::UNSIGNED32,
        bit_length: 32,
        access: access::READ_ONLY,
        value_info: 0,
        name: "Product code",
    },
    EntryDescription {
        index: 0x1018,
        subindex: 3,
        data_type: data_type::UNSIGNED32,
        bit_length: 32,
        access: access::READ_ONLY,
        value_info: 0,
        name: "Revision number",
    },
    EntryDescription {
        index: 0x1018,
        subindex: 4,
        data_type: data_type::UNSIGNED32,
        bit_length: 32,
        access: access::READ_ONLY,
        value_info: 0,
        name: "Serial number",
    },
    EntryDescription {
        index: 0x1C00,
        subindex: 0,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_ONLY,
        value_info: 0,
        name: "Number of sync managers",
    },
    EntryDescription {
        index: 0x1C00,
        subindex: 1,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_ONLY,
        value_info: 0,
        name: "SM0 type",
    },
    EntryDescription {
        index: 0x1C00,
        subindex: 2,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_ONLY,
        value_info: 0,
        name: "SM1 type",
    },
    EntryDescription {
        index: 0x1C00,
        subindex: 3,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_ONLY,
        value_info: 0,
        name: "SM2 type",
    },
    EntryDescription {
        index: 0x1C00,
        subindex: 4,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_ONLY,
        value_info: 0,
        name: "SM3 type",
    },
    EntryDescription {
        index: 0x1C12,
        subindex: 0,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "Number of assigned RxPDOs",
    },
    EntryDescription {
        index: 0x1C12,
        subindex: 1,
        data_type: data_type::UNSIGNED16,
        bit_length: 16,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "RxPDO assign",
    },
    EntryDescription {
        index: 0x1C13,
        subindex: 0,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "Number of assigned TxPDOs",
    },
    EntryDescription {
        index: 0x1C13,
        subindex: 1,
        data_type: data_type::UNSIGNED16,
        bit_length: 16,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "TxPDO assign",
    },
    EntryDescription {
        index: 0x1600,
        subindex: 0,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "Number of mapped objects",
    },
    EntryDescription {
        index: 0x1600,
        subindex: 1,
        data_type: data_type::UNSIGNED32,
        bit_length: 32,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "RxPDO mapping 0x7000:00",
    },
    EntryDescription {
        index: 0x1A00,
        subindex: 0,
        data_type: data_type::UNSIGNED8,
        bit_length: 8,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "Number of mapped objects",
    },
    EntryDescription {
        index: 0x1A00,
        subindex: 1,
        data_type: data_type::UNSIGNED32,
        bit_length: 32,
        access: access::READ_WRITE_PREOP,
        value_info: 0,
        name: "TxPDO mapping 0x6000:00",
    },
];

/// 对象字典的索引列表（去重、按 [`SIM_OBJECT_DESCRIPTIONS`] 顺序）。
#[must_use]
pub fn object_index_list() -> Vec<u16> {
    SIM_OBJECT_DESCRIPTIONS
        .iter()
        .map(|description| description.index)
        .collect()
}

/// 按索引查对象描述（SDO Info Get OD 用）。
#[must_use]
pub fn object_description(index: u16) -> Option<ObjectDescription> {
    SIM_OBJECT_DESCRIPTIONS
        .iter()
        .find(|description| description.index == index)
        .copied()
}

/// 按索引+子索引查条目描述（SDO Info Get Entry Description 用）。
#[must_use]
pub fn entry_description(index: u16, subindex: u8) -> Option<EntryDescription> {
    SIM_ENTRY_DESCRIPTIONS
        .iter()
        .find(|description| description.index == index && description.subindex == subindex)
        .copied()
}

/// 运行时对象条目：值 + 描述（内置画像与声明式 manifest 生成共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEntry {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// 当前值（小端字节序）。
    pub value: Vec<u8>,
    /// 数据类型，见 [`data_type`]。
    pub data_type: u16,
    /// 位长度。
    pub bit_length: u16,
    /// 访问权限位，见 [`access`]。
    pub access: u16,
    /// ValueInfo（0 = 普通值）。
    pub value_info: u8,
    /// 名称（SDO Info 响应中的字符串）。
    pub name: String,
    /// 是否允许 SDO Download。
    pub writable: bool,
}

/// 运行时对象描述（SDO Info Get OD 用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObject {
    /// 对象索引。
    pub index: u16,
    /// 最大子索引。
    pub max_subindex: u8,
    /// 对象代码，见 [`object_code`]。
    pub object_code: u8,
    /// 对象数据类型，见 [`data_type`]。
    pub data_type: u16,
    /// 对象名。
    pub name: String,
}

/// 运行时对象字典。
///
/// 内置画像由 [`ObjectDictionary::simulated`] 从 [`SIM_OBJECTS`] / [`SIM_ENTRY_DESCRIPTIONS`]
/// / [`SIM_OBJECT_DESCRIPTIONS`] 构造；声明式插件由 `plugin_host::dict_gen` 从 manifest 生成。
/// `esc-node::Responder` 持有它，SDO 上传/下载与 SDO Info 均按运行时字典回复。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectDictionary {
    objects: Vec<RuntimeObject>,
    entries: Vec<RuntimeEntry>,
}

impl ObjectDictionary {
    /// 用给定的对象/条目构造字典。
    #[must_use]
    pub fn new(objects: Vec<RuntimeObject>, entries: Vec<RuntimeEntry>) -> Self {
        Self { objects, entries }
    }

    /// 内置仿真从站画像（与静态 [`SIM_OBJECTS`] / `SIM_*_DESCRIPTIONS` 一致）。
    #[must_use]
    pub fn simulated() -> Self {
        let objects = SIM_OBJECT_DESCRIPTIONS
            .iter()
            .map(|description| RuntimeObject {
                index: description.index,
                max_subindex: description.max_subindex,
                object_code: description.object_code,
                data_type: description.data_type,
                name: description.name.to_string(),
            })
            .collect();
        let entries = SIM_ENTRY_DESCRIPTIONS
            .iter()
            .map(|description| {
                let value = read_object(description.index, description.subindex)
                    .map(|object| object.value.to_vec())
                    .unwrap_or_default();
                RuntimeEntry {
                    index: description.index,
                    subindex: description.subindex,
                    value,
                    data_type: description.data_type,
                    bit_length: description.bit_length,
                    access: description.access,
                    value_info: description.value_info,
                    name: description.name.to_string(),
                    writable: description.access & 0x0038 != 0,
                }
            })
            .collect();
        Self { objects, entries }
    }

    /// 对象描述表。
    #[must_use]
    pub fn objects(&self) -> &[RuntimeObject] {
        &self.objects
    }

    /// 条目表。
    #[must_use]
    pub fn entries(&self) -> &[RuntimeEntry] {
        &self.entries
    }

    /// 按索引查对象描述。
    #[must_use]
    pub fn object(&self, index: u16) -> Option<&RuntimeObject> {
        self.objects.iter().find(|object| object.index == index)
    }

    /// 按索引+子索引查条目。
    #[must_use]
    pub fn entry(&self, index: u16, subindex: u8) -> Option<&RuntimeEntry> {
        self.entries
            .iter()
            .find(|entry| entry.index == index && entry.subindex == subindex)
    }

    /// 读条目当前值。
    #[must_use]
    pub fn value(&self, index: u16, subindex: u8) -> Option<&[u8]> {
        self.entry(index, subindex)
            .map(|entry| entry.value.as_slice())
    }

    /// 对象索引列表（SDO Info Get OD List 顺序）。
    #[must_use]
    pub fn object_indices(&self) -> Vec<u16> {
        self.objects.iter().map(|object| object.index).collect()
    }

    /// 该索引下是否有可写条目。
    #[must_use]
    pub fn is_writable(&self, index: u16) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.index == index && entry.writable)
    }
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

    #[test]
    fn descriptions_cover_every_simulated_object_entry() {
        for entry in SIM_OBJECTS {
            let description = entry_description(entry.index, entry.subindex).unwrap_or_else(|| {
                panic!("缺少条目描述 0x{:04X}:{:02X}", entry.index, entry.subindex)
            });
            assert_eq!(
                usize::from(description.bit_length),
                entry.value.len() * 8,
                "0x{:04X}:{:02X} 位长度与值长度不一致",
                entry.index,
                entry.subindex
            );
        }
        assert_eq!(
            object_index_list(),
            vec![0x1018, 0x1C00, 0x1C12, 0x1C13, 0x1600, 0x1A00],
            "Get OD List 的索引顺序"
        );
        for description in SIM_OBJECT_DESCRIPTIONS {
            let max_subindex = SIM_ENTRY_DESCRIPTIONS
                .iter()
                .filter(|entry| entry.index == description.index)
                .map(|entry| entry.subindex)
                .max()
                .unwrap_or(0);
            assert_eq!(
                description.max_subindex, max_subindex,
                "0x{:04X} 的 max_subindex 与条目表不一致",
                description.index
            );
            assert!(object_description(description.index).is_some());
        }
        assert!(entry_description(0x1018, 5).is_none());
        assert!(object_description(0x2000).is_none());
    }
}
