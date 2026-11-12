//! 最小 **ESI XML → SII** 生成器（B6 子项）。
//!
//! 解析 ESI（EtherCAT Slave Information）XML 中本仓库需要的子集，生成
//! [`esc_core::sii::SiiSpec`]（再由 `SiiImage::from_spec()` 编码为 SII 镜像）：
//! - `EtherCATInfo/Vendor/Id`、`Device/Type`（ProductCode/RevisionNo）、`Device/Name`；
//! - `Device/Sm`（StartAddress/ControlByte/Enable + 文本方向 MBoxOut/MBoxIn/Outputs/Inputs）；
//! - `Device/RxPdo|TxPdo`（Index/Sm + `Entry` 的 Index/SubIndex/BitLen）；
//! - `Device/Mailbox/CoE`（可选属性 SdoInfo/PdoAssign/PdoConfig/CompleteAccess）。
//!
//! **只解析、不内置任何厂商 ESI 文件**（R1）：测试夹具是本仓库自写的样例 XML。
//! 未支持：ObjectDictionary、DC 参数、多设备文件的逐个选择（取第一个 `Device`）、
//! 字符串类 PDO 映射（BitLen > 16 的映射项照常写入，但本模拟器的 PDO 运行时不产生数据）。
//!
//! 数值格式按 ESI 约定：`#x` 十六进制、`#` 八进制、其余十进制。
//! 邮箱长度：ESI 的 `Sm` 元素通常不声明长度（⚠️ 其真实语义以 ETG.2000 为准）；
//! 本实现优先读可选的 `Length` 属性，缺省按 128 字节（与常见 Beckhoff 端子一致）。

use esc_core::sii::{SiiMailbox, SiiPdoEntry, SiiPdoSpec, SiiSpec, SiiSyncManager};

/// 解析后的 XML 节点（最小树：名称 + 属性 + 文本 + 子节点）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlNode {
    /// 元素名。
    pub name: String,
    /// 属性（保持出现顺序）。
    pub attrs: Vec<(String, String)>,
    /// 直接文本内容（已拼接、未 trim）。
    pub text: String,
    /// 子元素。
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    /// 取属性值。
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// 取第一个同名子元素。
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|node| node.name == name)
    }

    /// 全部同名子元素。
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter(move |node| node.name == name)
    }

    /// 文本内容（trim 后）。
    #[must_use]
    pub fn text_trimmed(&self) -> &str {
        self.text.trim()
    }
}

/// 去掉 XML 注释、处理指令与 DOCTYPE，便于最小解析器逐字符扫描。
fn strip_non_elements(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    let bytes = source.as_bytes();
    while index < bytes.len() {
        if source[index..].starts_with("<!--") {
            match source[index..].find("-->") {
                Some(end) => index += end + 3,
                None => break,
            }
            continue;
        }
        if source[index..].starts_with("<?") {
            match source[index..].find("?>") {
                Some(end) => index += end + 2,
                None => break,
            }
            continue;
        }
        if source[index..].starts_with("<!") {
            // DOCTYPE 等：跳过第一个 '>'（ESI 不使用内部子集）。
            match source[index..].find('>') {
                Some(end) => index += end + 1,
                None => break,
            }
            continue;
        }
        let ch = source[index..].chars().next().expect("in bounds");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// XML 属性列表。
type XmlAttrs = Vec<(String, String)>;

/// 开始标签解析结果：(元素名, 属性, 是否自闭合, 标签结束后的字节偏移)。
type ParsedTag = Result<(String, XmlAttrs, bool, usize), String>;

/// 解析 XML 文本为树；只支持元素/属性/文本，非法结构返回错误。
pub fn parse_xml(source: &str) -> Result<XmlNode, String> {
    let cleaned = strip_non_elements(source);
    let bytes = cleaned.as_bytes();
    let mut index = 0usize;
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;

    fn parse_tag(bytes: &[u8], mut index: usize) -> ParsedTag {
        // 跳过 '<' 后的空白；返回 (name, attrs, self_closing, new_index_after_tag)
        index += 1;
        let start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'/'
            && bytes[index] != b'>'
        {
            index += 1;
        }
        if start == index {
            return Err("空元素名".to_string());
        }
        let name = std::str::from_utf8(&bytes[start..index])
            .map_err(|_| "元素名不是 UTF-8".to_string())?
            .to_string();
        let mut attrs = Vec::new();
        loop {
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() {
                return Err(format!("元素 <{name}> 未闭合"));
            }
            if bytes[index] == b'/' {
                index += 1;
                if index < bytes.len() && bytes[index] == b'>' {
                    return Ok((name, attrs, true, index + 1));
                }
                return Err(format!("元素 <{name}> 的 '/>' 不完整"));
            }
            if bytes[index] == b'>' {
                return Ok((name, attrs, false, index + 1));
            }
            let attr_start = index;
            while index < bytes.len()
                && !bytes[index].is_ascii_whitespace()
                && bytes[index] != b'='
                && bytes[index] != b'>'
            {
                index += 1;
            }
            let attr_name = std::str::from_utf8(&bytes[attr_start..index])
                .map_err(|_| "属性名不是 UTF-8".to_string())?
                .to_string();
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() || bytes[index] != b'=' {
                return Err(format!("属性 {attr_name} 缺少 '='"));
            }
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() || (bytes[index] != b'"' && bytes[index] != b'\'') {
                return Err(format!("属性 {attr_name} 的值缺少引号"));
            }
            let quote = bytes[index];
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            if index >= bytes.len() {
                return Err(format!("属性 {attr_name} 的值未闭合"));
            }
            let value = std::str::from_utf8(&bytes[value_start..index])
                .map_err(|_| "属性值不是 UTF-8".to_string())?
                .to_string();
            attrs.push((attr_name, decode_entities(&value)));
            index += 1;
        }
    }

    while index < bytes.len() {
        if bytes[index] != b'<' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'<' {
                index += 1;
            }
            let text = std::str::from_utf8(&bytes[start..index])
                .map_err(|_| "文本不是 UTF-8".to_string())?;
            if let Some(node) = stack.last_mut() {
                node.text.push_str(&decode_entities(text));
            }
            continue;
        }
        if bytes[index + 1..].starts_with(b"/") {
            // 关闭标签
            let end = cleaned[index..]
                .find('>')
                .ok_or_else(|| "关闭标签缺少 '>'".to_string())?;
            let name = cleaned[index + 2..index + end].trim();
            let node = stack
                .pop()
                .ok_or_else(|| format!("多余的关闭标签 </{name}>"))?;
            if node.name != name {
                return Err(format!("关闭标签 </{name}> 与 <{}> 不匹配", node.name));
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => {
                    if root.is_some() {
                        return Err("XML 有多个根元素".to_string());
                    }
                    root = Some(node);
                }
            }
            index += end + 1;
            continue;
        }
        // 开始标签/自闭合标签
        let (name, attrs, self_closing, next) = parse_tag(bytes, index)?;
        index = next;
        let node = XmlNode {
            name,
            attrs,
            text: String::new(),
            children: Vec::new(),
        };
        if self_closing {
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => {
                    if root.is_some() {
                        return Err("XML 有多个根元素".to_string());
                    }
                    root = Some(node);
                }
            }
        } else {
            stack.push(node);
        }
    }
    if !stack.is_empty() {
        return Err(format!(
            "元素 <{}> 未闭合",
            stack.last().expect("non-empty").name
        ));
    }
    root.ok_or_else(|| "XML 没有根元素".to_string())
}

/// 解析 ESI 数值：`#x` 十六进制、`#` 八进制、其余十进制。
pub fn esi_number(value: &str) -> Result<u32, String> {
    let text = value.trim();
    if let Some(hex) = text.strip_prefix("#x").or_else(|| text.strip_prefix("#X")) {
        u32::from_str_radix(hex, 16).map_err(|_| format!("非法十六进制：{value}"))
    } else if let Some(octal) = text.strip_prefix('#') {
        u32::from_str_radix(octal, 8).map_err(|_| format!("非法八进制：{value}"))
    } else {
        text.parse::<u32>()
            .map_err(|_| format!("非法数值：{value}"))
    }
}

/// ESI 的 Sync Manager 声明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EsiSm {
    /// 方向/用途文本（`MBoxOut` / `MBoxIn` / `Outputs` / `Inputs` 等）。
    pub name: String,
    /// 物理起始地址。
    pub start_address: u16,
    /// Control 字节。
    pub control: u8,
    /// 是否使能。
    pub enable: bool,
    /// 可选长度（ESI 通常不声明；缺省由生成器按方向补齐）。
    pub length: Option<u16>,
}

/// ESI 的 PDO 映射项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EsiPdoEntry {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// 位长度。
    pub bit_length: u16,
    /// 映射项名称（可选）。
    pub name: Option<String>,
}

/// ESI 的一个 PDO。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EsiPdo {
    /// PDO 索引。
    pub index: u16,
    /// 绑定的 SM 序号（`Sm` 属性；缺省 rx=2 / tx=3）。
    pub sync_manager: u8,
    /// PDO 名称。
    pub name: Option<String>,
    /// 映射项。
    pub entries: Vec<EsiPdoEntry>,
}

/// 一个 ESI 设备（取文件中的第一个）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EsiDevice {
    /// Vendor ID（ESI 中厂商声明；**不用于**本仓库设备身份，R2）。
    pub vendor_id: u32,
    /// Product Code（同上，仅作兼容性事实）。
    pub product_code: u32,
    /// Revision No。
    pub revision: u32,
    /// 设备名（ESI `Device/Name`，缺失时用 `Type` 文本）。
    pub name: String,
    /// SM 列表（顺序即 SM 序号）。
    pub sm: Vec<EsiSm>,
    /// RxPDO（主站 → 从站）。
    pub rx_pdos: Vec<EsiPdo>,
    /// TxPDO（从站 → 主站）。
    pub tx_pdos: Vec<EsiPdo>,
    /// 是否声明了 CoE 邮箱。
    pub coe: bool,
    /// 是否声明 Complete Access。
    pub coe_complete_access: bool,
}

fn required_child<'a>(node: &'a XmlNode, name: &str) -> Result<&'a XmlNode, String> {
    node.child(name)
        .ok_or_else(|| format!("缺少元素 {name}（在 <{}> 下）", node.name))
}

fn parse_sm(node: &XmlNode) -> Result<EsiSm, String> {
    Ok(EsiSm {
        name: node.text_trimmed().to_string(),
        start_address: u16::try_from(esi_number(
            node.attr("StartAddress").ok_or("Sm 缺少 StartAddress")?,
        )?)
        .map_err(|_| "Sm StartAddress 超出 u16".to_string())?,
        control: u8::try_from(esi_number(
            node.attr("ControlByte").ok_or("Sm 缺少 ControlByte")?,
        )?)
        .map_err(|_| "Sm ControlByte 超出 u8".to_string())?,
        enable: match node.attr("Enable") {
            Some(value) => value.trim() != "0",
            None => true,
        },
        length: match node.attr("Length") {
            Some(value) => Some(
                u16::try_from(esi_number(value)?).map_err(|_| "Sm Length 超出 u16".to_string())?,
            ),
            None => None,
        },
    })
}

fn parse_pdo(node: &XmlNode, default_sm: u8) -> Result<EsiPdo, String> {
    let index = u16::try_from(esi_number(required_child(node, "Index")?.text_trimmed())?)
        .map_err(|_| "PDO Index 超出 u16".to_string())?;
    let entries = node
        .children_named("Entry")
        .map(|entry| {
            Ok(EsiPdoEntry {
                index: u16::try_from(esi_number(required_child(entry, "Index")?.text_trimmed())?)
                    .map_err(|_| "Entry Index 超出 u16".to_string())?,
                subindex: u8::try_from(
                    entry
                        .child("SubIndex")
                        .map_or(Ok(0), |node| esi_number(node.text_trimmed()))?,
                )
                .map_err(|_| "Entry SubIndex 超出 u8".to_string())?,
                bit_length: u16::try_from(esi_number(
                    required_child(entry, "BitLen")?.text_trimmed(),
                )?)
                .map_err(|_| "Entry BitLen 超出 u16".to_string())?,
                name: entry
                    .child("Name")
                    .map(|node| node.text_trimmed().to_string()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(EsiPdo {
        index,
        sync_manager: match node.attr("Sm") {
            Some(value) => {
                u8::try_from(esi_number(value)?).map_err(|_| "PDO Sm 超出 u8".to_string())?
            }
            None => default_sm,
        },
        name: node
            .child("Name")
            .map(|node| node.text_trimmed().to_string()),
        entries,
    })
}

/// 解析 ESI XML（取第一个 `Device`）。
pub fn parse_esi(xml: &str) -> Result<EsiDevice, String> {
    let root = parse_xml(xml)?;
    if root.name != "EtherCATInfo" {
        return Err(format!("根元素是 <{}>，期望 <EtherCATInfo>", root.name));
    }
    let vendor_id =
        esi_number(required_child(required_child(&root, "Vendor")?, "Id")?.text_trimmed())?;
    let devices = required_child(required_child(&root, "Descriptions")?, "Devices")?;
    let device = devices
        .children_named("Device")
        .next()
        .ok_or_else(|| "ESI 中没有任何 <Device>".to_string())?;
    let type_node = required_child(device, "Type")?;
    let product_code = esi_number(
        type_node
            .attr("ProductCode")
            .ok_or("Device/Type 缺少 ProductCode")?,
    )?;
    let revision = esi_number(
        type_node
            .attr("RevisionNo")
            .ok_or("Device/Type 缺少 RevisionNo")?,
    )?;
    let name = device
        .child("Name")
        .map(|node| node.text_trimmed().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| type_node.text_trimmed().to_string());

    let sm = device
        .children_named("Sm")
        .map(parse_sm)
        .collect::<Result<Vec<_>, String>>()?;
    let rx_pdos = device
        .children_named("RxPdo")
        .map(|node| parse_pdo(node, 2))
        .collect::<Result<Vec<_>, String>>()?;
    let tx_pdos = device
        .children_named("TxPdo")
        .map(|node| parse_pdo(node, 3))
        .collect::<Result<Vec<_>, String>>()?;
    let coe_node = device
        .child("Mailbox")
        .and_then(|mailbox| mailbox.child("CoE"));
    let coe = coe_node.is_some();
    let coe_complete_access = coe_node
        .and_then(|node| node.attr("CompleteAccess"))
        .is_some_and(|value| value.trim() == "1");

    Ok(EsiDevice {
        vendor_id,
        product_code,
        revision,
        name,
        sm,
        rx_pdos,
        tx_pdos,
        coe,
        coe_complete_access,
    })
}

fn direction(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower.contains("mboxout") {
        Some("rx")
    } else if lower.contains("mboxin") {
        Some("tx")
    } else if lower.contains("output") {
        Some("out")
    } else if lower.contains("input") {
        Some("in")
    } else {
        None
    }
}

/// 把 ESI 设备描述转换为 [`SiiSpec`]（供 `SiiImage::from_spec()` 编码）。
///
/// 邮箱长度：`Sm` 未声明时按 128 字节；过程数据 SM 长度 = 其绑定 PDO 的位长之和向上取整。
pub fn sii_spec_from_esi(device: &EsiDevice) -> Result<SiiSpec, String> {
    let mut lengths: Vec<u16> = vec![0; device.sm.len()];
    for pdo in device.rx_pdos.iter().chain(device.tx_pdos.iter()) {
        let index = usize::from(pdo.sync_manager);
        if index >= lengths.len() {
            return Err(format!(
                "PDO {:#06X} 的 Sm={} 超过 ESI 声明的 SM 数量 {}",
                pdo.index,
                pdo.sync_manager,
                lengths.len()
            ));
        }
        let bits: u32 = pdo
            .entries
            .iter()
            .map(|entry| u32::from(entry.bit_length))
            .sum();
        let bytes = u16::try_from(bits.div_ceil(8)).map_err(|_| "PDO 长度过大".to_string())?;
        lengths[index] = lengths[index].saturating_add(bytes);
    }

    let mut mailbox = None;
    for (index, sm) in device.sm.iter().enumerate() {
        let Some(kind) = direction(&sm.name) else {
            continue;
        };
        if !sm.enable {
            continue;
        }
        let length = sm.length.unwrap_or_else(|| {
            if sm.control & 0x03 == 0x02 {
                128
            } else {
                lengths.get(index).copied().unwrap_or(0)
            }
        });
        if let Some(existing) = mailbox.as_mut() {
            let mailbox: &mut SiiMailbox = existing;
            match kind {
                "rx" => {
                    mailbox.rx_offset = sm.start_address;
                    mailbox.rx_size = length;
                }
                "tx" => {
                    mailbox.tx_offset = sm.start_address;
                    mailbox.tx_size = length;
                }
                _ => {}
            }
        } else if kind == "rx" || kind == "tx" {
            let mut candidate = SiiMailbox {
                rx_offset: 0,
                rx_size: 0,
                tx_offset: 0,
                tx_size: 0,
                protocols: 0x0004, // CoE
            };
            match kind {
                "rx" => {
                    candidate.rx_offset = sm.start_address;
                    candidate.rx_size = length;
                }
                _ => {
                    candidate.tx_offset = sm.start_address;
                    candidate.tx_size = length;
                }
            }
            mailbox = Some(candidate);
        }
    }
    let mailbox = if device.coe {
        Some(mailbox.ok_or_else(|| "声明了 CoE 但 ESI 没有 MBoxOut/MBoxIn SM".to_string())?)
    } else {
        None
    };

    let sync_managers = device
        .sm
        .iter()
        .enumerate()
        .map(|(index, sm)| {
            let is_mailbox = sm.control & 0x03 == 0x02;
            SiiSyncManager {
                physical_start_address: sm.start_address,
                length: sm.length.unwrap_or_else(|| {
                    if is_mailbox {
                        128
                    } else {
                        lengths.get(index).copied().unwrap_or(0)
                    }
                }),
                control: sm.control,
                status: 0,
                enable: u8::from(sm.enable),
                pdi_control: 0,
            }
        })
        .collect();

    let pdo_spec = |pdo: &EsiPdo| -> Result<SiiPdoSpec, String> {
        Ok(SiiPdoSpec {
            index: pdo.index,
            sync_manager: pdo.sync_manager,
            entries: pdo
                .entries
                .iter()
                .map(|entry| {
                    Ok(SiiPdoEntry {
                        index: entry.index,
                        subindex: entry.subindex,
                        bit_length: u8::try_from(entry.bit_length).map_err(|_| {
                            format!(
                                "PDO {:#06X} 映射项位长 {} 超过 SII 单字节上限 255",
                                pdo.index, entry.bit_length
                            )
                        })?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        })
    };

    let mut coe_details = 0u8;
    if device.coe {
        coe_details |= 0x01; // SDO
        coe_details |= 0x02; // SDO Info
        if !device.rx_pdos.is_empty() || !device.tx_pdos.is_empty() {
            coe_details |= 0x04; // PDO assign
            coe_details |= 0x08; // PDO configuration
        }
        if device.coe_complete_access {
            coe_details |= 0x20;
        }
    }

    Ok(SiiSpec {
        vendor_id: device.vendor_id,
        product_code: device.product_code,
        revision: device.revision,
        serial: 0,
        // R2：ESI 的厂商名/标识不写入本仓库设备的 SII 名称；设备名沿用 ESI 设备名
        //（调用方应确保是"兼容仿真"命名），厂商名固定为我方。
        names: vec![device.name.clone(), "EC-Sim".to_string()],
        mailbox,
        sync_managers,
        tx_pdos: device
            .tx_pdos
            .iter()
            .map(pdo_spec)
            .collect::<Result<Vec<_>, String>>()?,
        rx_pdos: device
            .rx_pdos
            .iter()
            .map(pdo_spec)
            .collect::<Result<Vec<_>, String>>()?,
        coe_details,
        general_flags: 0,
        ebus_current: 0,
        // DC 类别：ESI 的 DC 参数尚未映射（模拟器未实现 DC 同步），保持不声明。
        dc: None,
    })
}

/// 由 ESI 生成**最小运行时对象字典**：只有身份对象 `0x1018`（Vendor/Product/Revision/Serial）。
///
/// ESI 不携带完整对象字典（那在从站固件/插件行为里），因此其余对象（SM 类型、PDO 分配/
/// 映射、用户对象）需要来自声明式 manifest 或行为插件；本函数只保证扫描阶段的身份一致。
pub fn dictionary_from_esi(device: &EsiDevice) -> esc_core::obj_dict::ObjectDictionary {
    use esc_core::obj_dict::{self, ObjectDictionary, RuntimeEntry, RuntimeObject};

    let object = |index: u16, max_subindex: u8, data_type: u16, name: &str| RuntimeObject {
        index,
        max_subindex,
        object_code: obj_dict::object_code::ARRAY,
        data_type,
        name: name.to_string(),
    };
    let entry =
        |subindex: u8, value: Vec<u8>, data_type: u16, bit_length: u16, name: &str| RuntimeEntry {
            index: 0x1018,
            subindex,
            value,
            data_type,
            bit_length,
            access: obj_dict::access::READ_ONLY,
            value_info: 0,
            name: name.to_string(),
            writable: false,
        };
    let mut count = entry(
        0,
        vec![4],
        obj_dict::data_type::UNSIGNED8,
        8,
        "Number of entries",
    );
    count.index = 0x1018;
    let entries = vec![
        count,
        entry(
            1,
            device.vendor_id.to_le_bytes().to_vec(),
            obj_dict::data_type::UNSIGNED32,
            32,
            "Vendor ID",
        ),
        entry(
            2,
            device.product_code.to_le_bytes().to_vec(),
            obj_dict::data_type::UNSIGNED32,
            32,
            "Product code",
        ),
        entry(
            3,
            device.revision.to_le_bytes().to_vec(),
            obj_dict::data_type::UNSIGNED32,
            32,
            "Revision number",
        ),
        entry(
            4,
            vec![0u8; 4],
            obj_dict::data_type::UNSIGNED32,
            32,
            "Serial number",
        ),
    ];
    ObjectDictionary::new(
        vec![object(
            0x1018,
            4,
            obj_dict::data_type::UNSIGNED32,
            "Identity",
        )],
        entries,
    )
}

/// 组合 **ESI（SII 描述）+ manifest（身份与对象字典）**，返回最终 SII 规格（B6 子项）。
///
/// - `Sm`/`RxPdo`/`TxPdo`/邮箱/CoE 能力来自 ESI（设备真实描述）；
/// - `vendor_id`/`product_code`/`revision` 与 SII 名称以 **manifest 为准**：本仓库设备必须
///   使用自有 VID/PID 且名称带 `SIMULATED`（R2），ESI 的身份只作为兼容性参考事实；
/// - 运行时对象字典仍由 [`crate::dict_gen::dictionary_from_manifest`] 生成（ESI 不含 OD）。
pub fn sii_spec_from_esi_and_manifest(
    device: &EsiDevice,
    manifest: &crate::manifest::PluginManifest,
) -> Result<SiiSpec, String> {
    let issues = manifest.validate();
    if !issues.is_empty() {
        return Err(format!(
            "manifest {} 校验未通过，不能与 ESI 组合：{}",
            manifest.id,
            issues.join("；")
        ));
    }
    let mut spec = sii_spec_from_esi(device)?;
    spec.vendor_id = manifest.identity.vendor_id;
    spec.product_code = manifest.identity.product_code;
    spec.revision = manifest.identity.revision;
    spec.names = vec![manifest.name.clone(), "EC-Sim".to_string()];
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use esc_core::sii::{category, SiiImage};
    const SAMPLE: &str = include_str!("../tests/fixtures/sample.esi.xml");

    #[test]
    fn parses_esi_sample_device() {
        let device = parse_esi(SAMPLE).expect("parse ESI");
        assert_eq!(device.vendor_id, 0x0000_0EC5);
        assert_eq!(device.product_code, 0x0000_E5D3);
        assert_eq!(device.revision, 0x0001_0000);
        assert_eq!(device.name, "Sample 8DO (SIMULATED)");
        assert_eq!(device.sm.len(), 4);
        assert_eq!(device.rx_pdos.len(), 1);
        assert_eq!(device.tx_pdos.len(), 1);
        assert!(device.coe);
        assert!(!device.coe_complete_access);
        assert_eq!(device.rx_pdos[0].entries.len(), 1);
        assert_eq!(device.rx_pdos[0].entries[0].index, 0x7000);
        assert_eq!(device.rx_pdos[0].entries[0].bit_length, 8);
    }

    #[test]
    fn generates_sii_from_esi() {
        let device = parse_esi(SAMPLE).expect("parse ESI");
        let spec = sii_spec_from_esi(&device).expect("sii spec");
        assert_eq!(spec.vendor_id, 0x0000_0EC5);
        assert_eq!(spec.product_code, 0x0000_E5D3);
        assert_eq!(spec.names[0], "Sample 8DO (SIMULATED)");
        assert_eq!(spec.sync_managers.len(), 4);
        // SM0/SM1 邮箱 128 字节、SM2 输出 1 字节、SM3 输入 1 字节。
        assert_eq!(spec.sync_managers[0].physical_start_address, 0x1000);
        assert_eq!(spec.sync_managers[0].length, 128);
        assert_eq!(spec.sync_managers[2].length, 1);
        assert_eq!(spec.sync_managers[3].length, 1);
        assert_eq!(spec.mailbox.expect("mailbox").rx_offset, 0x1000);
        assert_eq!(spec.mailbox.expect("mailbox").tx_offset, 0x1080);

        let sii = SiiImage::from_spec(&spec);
        assert!(sii.verify_checksum());
        assert_eq!(sii.sync_manager_count(), 4);
        assert_eq!(sii.mailbox_protocols(), 0x0004);
        let rx = sii
            .category_bytes(category::RX_PDO)
            .expect("RxPDO category");
        assert_eq!(u16::from_le_bytes([rx[0], rx[1]]), 0x1600);
        assert_eq!(rx[3], 2, "SM2");
        let tx = sii
            .category_bytes(category::TX_PDO)
            .expect("TxPDO category");
        assert_eq!(u16::from_le_bytes([tx[0], tx[1]]), 0x1A00);
        assert_eq!(tx[3], 3, "SM3");
    }

    #[test]
    fn esi_plus_manifest_uses_manifest_identity_and_esi_layout() {
        let device = parse_esi(SAMPLE).expect("parse ESI");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/sample.esi.plugin.yaml"
        );
        let manifest = crate::manifest::PluginManifest::load(path).expect("load paired manifest");
        let spec = sii_spec_from_esi_and_manifest(&device, &manifest).expect("combined spec");
        assert_eq!(spec.vendor_id, manifest.identity.vendor_id);
        assert_eq!(spec.product_code, manifest.identity.product_code);
        assert_eq!(spec.revision, manifest.identity.revision);
        assert_eq!(spec.names[0], manifest.name);
        // SM/PDO/邮箱布局仍来自 ESI。
        assert_eq!(spec.sync_managers.len(), 4);
        assert_eq!(spec.sync_managers[0].length, 128);
        assert_eq!(spec.sync_managers[2].length, 1);
        assert_eq!(spec.rx_pdos[0].index, 0x1600);
        assert_eq!(spec.tx_pdos[0].index, 0x1A00);
        let sii = SiiImage::from_spec(&spec);
        assert!(sii.verify_checksum());
        assert_eq!(sii.mailbox_protocols(), 0x0004);
    }

    #[test]
    fn dictionary_from_esi_has_identity_object() {
        let device = parse_esi(SAMPLE).expect("parse ESI");
        let dict = dictionary_from_esi(&device);
        assert_eq!(dict.object_indices(), vec![0x1018]);
        assert_eq!(dict.value(0x1018, 0), Some([4u8].as_slice()));
        assert_eq!(
            dict.value(0x1018, 1),
            Some(0x0000_0EC5u32.to_le_bytes().as_slice()),
            "0x1018:01 = ESI Vendor Id"
        );
        assert_eq!(
            dict.value(0x1018, 2),
            Some(0x0000_E5D3u32.to_le_bytes().as_slice()),
            "0x1018:02 = ESI ProductCode"
        );
        assert_eq!(
            dict.value(0x1018, 3),
            Some(0x0001_0000u32.to_le_bytes().as_slice()),
            "0x1018:03 = ESI RevisionNo"
        );
    }

    #[test]
    fn rejects_malformed_xml_and_missing_fields() {
        assert!(parse_esi("<EtherCATInfo>").is_err(), "未闭合");
        assert!(parse_esi("<NotEsi/>").is_err(), "根元素不符");
        assert!(
            parse_esi("<EtherCATInfo><Vendor><Id>1</Id></Vendor></EtherCATInfo>").is_err(),
            "缺 Descriptions"
        );
        let missing_product = SAMPLE.replace("ProductCode=\"#x0000e5d3\"", "");
        assert!(parse_esi(&missing_product).is_err(), "缺 ProductCode");
    }

    #[test]
    fn esi_numbers_use_esi_conventions() {
        assert_eq!(esi_number("#x0000e5d3"), Ok(0x0000_E5D3));
        assert_eq!(esi_number("#330"), Ok(0o330));
        assert_eq!(esi_number("128"), Ok(128));
        assert!(esi_number("bogus").is_err());
    }
}
