//! 声明式插件 manifest（`plugin.yaml`，schema `ecsim.plugin/v1`）的解析与校验。
//!
//! 格式与红线见 `docs/DEVICE-PLUGINS.md` §4 与 `plugins/_template/plugin.yaml`。
//! 本模块**不引入第三方 YAML 依赖**：只实现 manifest 所需的 YAML 子集
//! （块映射 / 块序列 / 行内 `{...}` 与 `[...]` / 引号或裸标量 / 布尔 / 十进制与十六进制整数 /
//! `#` 注释）。不支持的语法返回明确错误，而不是静默误解析。

/// 解析后的 YAML 子集节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Yaml {
    /// 标量（原始文本；引号已去除）。
    Scalar(String),
    /// 映射（保持出现顺序）。
    Map(Vec<(String, Yaml)>),
    /// 序列。
    Seq(Vec<Yaml>),
}

impl Yaml {
    /// 取映射中的键值。
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Yaml> {
        match self {
            Yaml::Map(pairs) => pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// 取标量文本（非标量返回 `None`）。
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Yaml::Scalar(text) => Some(text),
            _ => None,
        }
    }
}

/// 去掉行内 `#` 注释（引号内的 `#` 不算注释），并返回去掉注释后的文本。
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(q) => {
                if byte == q {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'#' => return &line[..index],
                _ => {}
            },
        }
        index += 1;
    }
    line
}

/// 去掉外层引号（单/双引号）。
fn unquote(text: &str) -> &str {
    let text = text.trim();
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        if (bytes[0] == b'"' && bytes[text.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[text.len() - 1] == b'\'')
        {
            return &text[1..text.len() - 1];
        }
    }
    text
}

/// 解析行内 flow 值：`{k: v, ...}` 或 `[a, b]`；否则按标量处理。
fn parse_flow(text: &str) -> Result<Yaml, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Yaml::Scalar(String::new()));
    }
    if let Some(inner) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        let mut pairs = Vec::new();
        for item in split_top_level(inner, ',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let Some((key, value)) = split_key(item) else {
                return Err(format!("行内映射缺少 ':'：{item}"));
            };
            pairs.push((unquote(key).to_string(), parse_flow(value)?));
        }
        return Ok(Yaml::Map(pairs));
    }
    if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        let mut items = Vec::new();
        for item in split_top_level(inner, ',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            items.push(parse_flow(item)?);
        }
        return Ok(Yaml::Seq(items));
    }
    Ok(Yaml::Scalar(unquote(text).to_string()))
}

/// 按分隔符切分，忽略花括号/方括号/引号内部的分隔符。
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        let byte = ch as u8;
        match quote {
            Some(q) => {
                if byte == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(byte),
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                c if c == separator && depth == 0 => {
                    parts.push(&text[start..index]);
                    start = index + ch.len_utf8();
                }
                _ => {}
            },
        }
    }
    parts.push(&text[start..]);
    parts
}

/// 找到顶层的 `:`，返回键与值（`None` = 没有冒号）。
fn split_key(text: &str) -> Option<(&str, &str)> {
    let mut quote: Option<u8> = None;
    let mut depth = 0i32;
    for (index, ch) in text.char_indices() {
        let byte = ch as u8;
        match quote {
            Some(q) => {
                if byte == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(byte),
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                ':' if depth == 0 => {
                    let key = text[..index].trim();
                    let value = text[index + 1..].trim();
                    return Some((key, value));
                }
                _ => {}
            },
        }
    }
    None
}

/// 有效行：缩进（空格数）与去注释后的内容。
struct Line {
    indent: usize,
    text: String,
}

fn prepare_lines(source: &str) -> Result<Vec<Line>, String> {
    let mut lines = Vec::new();
    for (number, raw) in source.lines().enumerate() {
        let stripped = strip_comment(raw).trim_end();
        if stripped.trim().is_empty() {
            continue;
        }
        let indent = stripped.len() - stripped.trim_start().len();
        let content = stripped.trim_start();
        if content.starts_with('\t') || stripped[..indent].contains('\t') {
            return Err(format!("第 {} 行：不支持 tab 缩进", number + 1));
        }
        if indent > 0 && indent % 2 != 0 {
            return Err(format!(
                "第 {} 行：缩进 {} 不是 2 的倍数（manifest 约定 2 空格）",
                number + 1,
                indent
            ));
        }
        lines.push(Line {
            indent,
            text: content.to_string(),
        });
    }
    Ok(lines)
}

/// 解析一段 YAML（manifest 子集）为 [`Yaml`] 树。
pub fn parse_yaml(source: &str) -> Result<Yaml, String> {
    let lines = prepare_lines(source)?;
    if lines.is_empty() {
        return Ok(Yaml::Map(Vec::new()));
    }
    let mut cursor = 0usize;
    let node = parse_block(&lines, &mut cursor, lines[0].indent)?;
    if cursor != lines.len() {
        return Err(format!("第 {} 行之后出现无法归属的缩进", cursor));
    }
    Ok(node)
}

fn parse_block(lines: &[Line], cursor: &mut usize, indent: usize) -> Result<Yaml, String> {
    if lines[*cursor].text.starts_with("- ") || lines[*cursor].text == "-" {
        parse_sequence(lines, cursor, indent)
    } else {
        parse_mapping(lines, cursor, indent)
    }
}

fn parse_mapping(lines: &[Line], cursor: &mut usize, indent: usize) -> Result<Yaml, String> {
    let mut pairs = Vec::new();
    while *cursor < lines.len() {
        let line = &lines[*cursor];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(format!("缩进不一致：'{}'", line.text));
        }
        if line.text.starts_with("- ") {
            break;
        }
        let Some((key, raw_value)) = split_key(&line.text) else {
            return Err(format!("缺少 ':'：'{}'", line.text));
        };
        let key = unquote(key).to_string();
        *cursor += 1;
        let value = if raw_value.is_empty() {
            if *cursor < lines.len() && lines[*cursor].indent > indent {
                parse_block(lines, cursor, lines[*cursor].indent)?
            } else {
                Yaml::Scalar(String::new())
            }
        } else {
            parse_flow(raw_value)?
        };
        pairs.push((key, value));
    }
    Ok(Yaml::Map(pairs))
}

fn parse_sequence(lines: &[Line], cursor: &mut usize, indent: usize) -> Result<Yaml, String> {
    let mut items = Vec::new();
    while *cursor < lines.len() {
        let line = &lines[*cursor];
        if line.indent < indent {
            break;
        }
        if line.indent != indent {
            return Err(format!("序列缩进不一致：'{}'", line.text));
        }
        let Some(rest) = line.text.strip_prefix('-') else {
            break;
        };
        let rest = rest.trim_start();
        if rest.is_empty() {
            *cursor += 1;
            if *cursor < lines.len() && lines[*cursor].indent > indent {
                items.push(parse_block(lines, cursor, lines[*cursor].indent)?);
            } else {
                items.push(Yaml::Scalar(String::new()));
            }
            continue;
        }
        if let Some((key, value)) = split_key(rest) {
            // `- key: value`：序列项是一个映射，后续同层缩进（dash + 2）的行补充其键值。
            let item_indent = indent + 2;
            let mut pairs = vec![(unquote(key).to_string(), parse_flow(value)?)];
            *cursor += 1;
            while *cursor < lines.len() && lines[*cursor].indent == item_indent {
                let more_line = &lines[*cursor];
                let Some((more_key, more_value)) = split_key(&more_line.text) else {
                    return Err(format!("映射项缺少 ':'：'{}'", more_line.text));
                };
                *cursor += 1;
                let parsed = if more_value.is_empty() {
                    if *cursor < lines.len() && lines[*cursor].indent > item_indent {
                        parse_block(lines, cursor, lines[*cursor].indent)?
                    } else {
                        Yaml::Scalar(String::new())
                    }
                } else {
                    parse_flow(more_value)?
                };
                pairs.push((unquote(more_key).to_string(), parsed));
            }
            items.push(Yaml::Map(pairs));
        } else {
            items.push(parse_flow(rest)?);
            *cursor += 1;
        }
    }
    Ok(Yaml::Seq(items))
}

/// 插件身份（manifest `identity:`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Identity {
    /// 我方自有 Vendor ID（R2：不得用厂商 VID）。
    pub vendor_id: u32,
    /// 自有 Product Code。
    pub product_code: u32,
    /// Revision。
    pub revision: u32,
    /// 是否声明为仿真（必须为 true）。
    pub simulated: bool,
}

/// Sync Manager 声明（manifest `sm:`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmConfig {
    /// 通道号。
    pub index: u8,
    /// 方向：`mbox-in` / `mbox-out` / `out` / `in`。
    pub dir: String,
    /// 物理起始地址。
    pub start: u16,
    /// 长度（字节）。
    pub len: u16,
}

/// PDO 映射项（manifest `pdo.*[].entries[]`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdoEntryConfig {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub sub: u8,
    /// 位长度。
    pub bits: u8,
}

/// 一个 PDO 的声明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdoConfig {
    /// PDO 索引（如 0x1600 / 0x1A00）。
    pub index: u16,
    /// 映射项。
    pub entries: Vec<PdoEntryConfig>,
}

/// 对象声明（manifest `objects:`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectConfig {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub sub: u8,
    /// 名称。
    pub name: String,
    /// 数据类型（如 `UINT8`）。
    pub data_type: String,
    /// 访问权限（`ro` / `rw`）。
    pub access: String,
    /// 是否属于 PDO 映射（`rx` / `tx`）。
    pub pdo: Option<String>,
}

/// 证据链条目（manifest `sources:`，R5 必填）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// 文档标题。
    pub title: String,
    /// URL。
    pub url: String,
    /// 访问日期（YYYY-MM-DD）。
    pub accessed: String,
}

/// 声明式插件 manifest（`ecsim.plugin/v1`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    /// schema 标识（必须是 `ecsim.plugin/v1`）。
    pub schema: String,
    /// 插件 id（`com.ecsim.compat.<brand>-<model>` 形式）。
    pub id: String,
    /// 设备名（必须带 SIMULATED 标识）。
    pub name: String,
    /// 版本。
    pub version: String,
    /// 类型：device / app-layer / pdo-extension。
    pub kind: String,
    /// 身份。
    pub identity: Identity,
    /// CoE 邮箱能力（`mailbox.coe`）；`None` = 未声明。
    pub coe: Option<CoeCapabilities>,
    /// Sync Manager 列表。
    pub sm: Vec<SmConfig>,
    /// RxPDO（主站 → 从站）列表。
    pub rx_pdos: Vec<PdoConfig>,
    /// TxPDO（从站 → 主站）列表。
    pub tx_pdos: Vec<PdoConfig>,
    /// 对象声明。
    pub objects: Vec<ObjectConfig>,
    /// 是否声明支持 DC。
    pub dc_supported: bool,
    /// 证据链（R5）。
    pub sources: Vec<Source>,
}

/// CoE 邮箱能力位（manifest `mailbox.coe`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoeCapabilities {
    /// SDO 上传/下载。
    pub sdo: bool,
    /// SDO Info 服务。
    pub sdo_info: bool,
    /// Complete Access。
    pub complete_access: bool,
}

impl PluginManifest {
    /// 从文件读取并解析 `plugins/<id>/plugin.yaml`。
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("读取 manifest {} 失败：{error}", path.display()))?;
        Self::parse(&text)
    }

    /// 解析 manifest 文本（YAML 子集）。
    pub fn parse(source: &str) -> Result<Self, String> {
        let root = parse_yaml(source)?;
        let schema = required_scalar(&root, "schema")?;
        let id = required_scalar(&root, "id")?;
        let name = required_scalar(&root, "name")?;
        let version = required_scalar(&root, "version")?;
        let kind = required_scalar(&root, "kind")?;

        let identity_node = root
            .get("identity")
            .ok_or_else(|| "缺少 identity 段（R2：必须声明自有 VID/PID）".to_string())?;
        let identity = Identity {
            vendor_id: integer(identity_node.get("vendor_id"))?,
            product_code: integer(identity_node.get("product_code"))?,
            revision: integer_or(identity_node.get("revision"), 0)?,
            simulated: boolean(identity_node.get("simulated"))?,
        };

        let coe = match root
            .get("mailbox")
            .and_then(|node| node.get("coe"))
            .filter(|node| matches!(node, Yaml::Map(_)))
        {
            Some(node) => Some(CoeCapabilities {
                sdo: optional_boolean(node.get("sdo"))?,
                sdo_info: optional_boolean(node.get("sdo_info"))?,
                complete_access: optional_boolean(node.get("complete_access"))?,
            }),
            None => None,
        };

        let sm = sequence(&root, "sm")?
            .iter()
            .map(|entry| {
                Ok(SmConfig {
                    index: u8::try_from(integer(entry.get("index"))?)
                        .map_err(|_| "sm.index 超出 u8".to_string())?,
                    dir: required_scalar(entry, "dir")?,
                    start: integer(entry.get("start"))? as u16,
                    len: integer(entry.get("len"))? as u16,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let pdo_node = root.get("pdo");
        let rx_pdos = parse_pdo_list(pdo_node.and_then(|node| node.get("rx")))?;
        let tx_pdos = parse_pdo_list(pdo_node.and_then(|node| node.get("tx")))?;

        let objects = root
            .get("objects")
            .map(|node| match node {
                Yaml::Seq(items) => items
                    .iter()
                    .map(|entry| {
                        Ok(ObjectConfig {
                            index: integer(entry.get("index"))? as u16,
                            sub: integer_or(entry.get("sub"), 0)? as u8,
                            name: required_scalar(entry, "name")?,
                            data_type: required_scalar(entry, "dtype")?,
                            access: required_scalar(entry, "access")?,
                            pdo: entry.get("pdo").and_then(Yaml::as_str).map(str::to_string),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>(),
                _ => Err("objects 必须是序列".to_string()),
            })
            .transpose()?
            .unwrap_or_default();

        let dc_supported = match root.get("dc").and_then(|node| node.get("supported")) {
            Some(_) => boolean(root.get("dc").and_then(|node| node.get("supported")))?,
            None => false,
        };

        let sources = root
            .get("sources")
            .map(|node| match node {
                Yaml::Seq(items) => items
                    .iter()
                    .map(|entry| {
                        Ok(Source {
                            title: required_scalar(entry, "title")?,
                            url: required_scalar(entry, "url")?,
                            accessed: required_scalar(entry, "accessed")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>(),
                _ => Err("sources 必须是序列".to_string()),
            })
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            schema,
            id,
            name,
            version,
            kind,
            identity,
            coe,
            sm,
            rx_pdos,
            tx_pdos,
            objects,
            dc_supported,
            sources,
        })
    }

    /// 校验红线与基本一致性；返回违规说明列表（空 = 通过）。
    ///
    /// 覆盖 R2（自有 VID/PID、名称带 SIMULATED）、R5（sources 必填），以及 manifest 基本结构。
    /// R1/R3/R4 是内容/法务约束，需人工按 `docs/DEVICE-PLUGINS.md` 审核，不在本函数自动放行。
    #[must_use]
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.schema != "ecsim.plugin/v1" {
            problems.push(format!("schema 不是 ecsim.plugin/v1：{}", self.schema));
        }
        if !self.id.starts_with("com.ecsim.") {
            problems.push(format!("id 必须使用自有域名前缀 com.ecsim.*：{}", self.id));
        }
        if !self.name.to_ascii_lowercase().contains("simulated") {
            problems.push("name 必须带 SIMULATED 标识（R2）".to_string());
        }
        if self.identity.vendor_id == 0 {
            problems.push("identity.vendor_id 为 0：必须填我方自有 Vendor ID（R2）".to_string());
        }
        if self.identity.product_code == 0 {
            problems.push("identity.product_code 为 0：必须填自有 Product Code（R2）".to_string());
        }
        if !self.identity.simulated {
            problems.push("identity.simulated 必须为 true（R2）".to_string());
        }
        if self.sources.is_empty() {
            problems.push("sources 为空：必须提供证据链（R5）".to_string());
        }
        for (index, source) in self.sources.iter().enumerate() {
            if source.url.trim().is_empty() || source.accessed.trim().is_empty() {
                problems.push(format!("sources[{index}] 缺少 URL 或访问日期（R5）"));
            }
        }
        if self.sm.is_empty() {
            problems.push("sm 为空：必须声明 Sync Manager 布局".to_string());
        }
        let mut seen = std::collections::BTreeSet::new();
        for sm in &self.sm {
            if !seen.insert(sm.index) {
                problems.push(format!("sm.index {} 重复", sm.index));
            }
            if !matches!(sm.dir.as_str(), "mbox-in" | "mbox-out" | "out" | "in") {
                problems.push(format!("sm[{}].dir 非法：{}", sm.index, sm.dir));
            }
        }
        for pdo in self.rx_pdos.iter().chain(self.tx_pdos.iter()) {
            if pdo.entries.is_empty() {
                problems.push(format!("PDO {:#06x} 没有任何映射项", pdo.index));
            }
        }
        problems
    }
}

fn required_scalar(node: &Yaml, key: &str) -> Result<String, String> {
    node.get(key)
        .and_then(Yaml::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("缺少标量字段 {key}"))
}

fn sequence<'a>(node: &'a Yaml, key: &str) -> Result<&'a [Yaml], String> {
    match node.get(key) {
        Some(Yaml::Seq(items)) => Ok(items),
        Some(_) => Err(format!("{key} 必须是序列")),
        None => Ok(&[]),
    }
}

fn parse_pdo_list(node: Option<&Yaml>) -> Result<Vec<PdoConfig>, String> {
    let Some(node) = node else {
        return Ok(Vec::new());
    };
    let items = match node {
        Yaml::Seq(items) => items,
        _ => return Err("pdo.rx / pdo.tx 必须是序列".to_string()),
    };
    items
        .iter()
        .map(|pdo| {
            let entries = sequence(pdo, "entries")?
                .iter()
                .map(|entry| {
                    Ok(PdoEntryConfig {
                        index: integer(entry.get("index"))? as u16,
                        sub: integer_or(entry.get("sub"), 0)? as u8,
                        bits: integer(entry.get("bits"))? as u8,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(PdoConfig {
                index: integer(pdo.get("index"))? as u16,
                entries,
            })
        })
        .collect()
}

/// 解析整数：支持十进制与 `0x` 十六进制，允许下划线分隔与正负号。
fn integer(node: Option<&Yaml>) -> Result<u32, String> {
    let Some(node) = node else {
        return Err("缺少数值字段".to_string());
    };
    let Some(text) = node.as_str() else {
        return Err("数值字段不是标量".to_string());
    };
    let text = text.trim().replace('_', "");
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(&text)),
    };
    let value = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => digits.parse::<u32>(),
    }
    .map_err(|_| format!("非法整数：{text}"))?;
    if negative {
        return Err(format!("此处不接受负数：{text}"));
    }
    Ok(value)
}

fn integer_or(node: Option<&Yaml>, default: u32) -> Result<u32, String> {
    match node {
        Some(_) => integer(node),
        None => Ok(default),
    }
}

fn boolean(node: Option<&Yaml>) -> Result<bool, String> {
    let Some(node) = node else {
        return Err("缺少布尔字段".to_string());
    };
    let Some(text) = node.as_str() else {
        return Err("布尔字段不是标量".to_string());
    };
    match text {
        "true" | "True" | "TRUE" => Ok(true),
        "false" | "False" | "FALSE" => Ok(false),
        other => Err(format!("非法布尔值：{other}")),
    }
}

fn optional_boolean(node: Option<&Yaml>) -> Result<bool, String> {
    match node {
        Some(_) => boolean(node),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
schema: ecsim.plugin/v1
id: com.ecsim.compat.sample
name: "Sample 8DO (SIMULATED)"
version: 0.1.0
kind: device
identity:
  vendor_id: 0x00000EC5
  product_code: 1
  simulated: true
sm:
  - {index: 0, dir: mbox-in, start: 0x1000, len: 128}
  - {index: 1, dir: mbox-out, start: 0x1080, len: 128}
sources:
  - {title: "Public manual", url: "https://example.invalid/x.pdf", accessed: "2026-11-12"}
"#;

    #[test]
    fn parses_blocks_sequences_and_flow_maps() {
        let root = parse_yaml(
            "top: 1\nlist:\n  - {a: 1, b: \"two\"}\n  - plain\nnested:\n  child: true\n",
        )
        .expect("parse");
        assert_eq!(root.get("top").and_then(Yaml::as_str), Some("1"));
        let list = match root.get("list") {
            Some(Yaml::Seq(items)) => items,
            other => panic!("expected seq, got {other:?}"),
        };
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].get("a").and_then(Yaml::as_str), Some("1"));
        assert_eq!(list[0].get("b").and_then(Yaml::as_str), Some("two"));
        assert_eq!(list[1].as_str(), Some("plain"));
        assert_eq!(
            root.get("nested")
                .and_then(|node| node.get("child"))
                .and_then(Yaml::as_str),
            Some("true")
        );
    }

    #[test]
    fn comments_are_stripped_outside_quotes() {
        let root = parse_yaml("a: 1 # trailing\nb: \"# not a comment\"\n").expect("parse");
        assert_eq!(root.get("a").and_then(Yaml::as_str), Some("1"));
        assert_eq!(
            root.get("b").and_then(Yaml::as_str),
            Some("# not a comment")
        );
    }

    #[test]
    fn parses_hex_and_decimal_integers() {
        let root = parse_yaml("hex: 0x00FF\ndec: 42\nneg: -1\n").expect("parse");
        assert_eq!(integer(root.get("hex")), Ok(0x00FF));
        assert_eq!(integer(root.get("dec")), Ok(42));
        assert!(integer(root.get("neg")).is_err(), "负数应被拒绝");
    }

    #[test]
    fn loads_manifest_from_real_repository_template() {
        let path = format!(
            "{}/../../plugins/_template/plugin.yaml",
            env!("CARGO_MANIFEST_DIR")
        );
        let manifest = PluginManifest::load(&path).expect("模板 manifest 必须可读可解析");
        assert_eq!(manifest.schema, "ecsim.plugin/v1");
        assert_eq!(manifest.id, "com.ecsim.compat.brand-model");
        assert!(manifest.coe.expect("mailbox.coe").sdo_info);
        assert!(manifest.coe.expect("mailbox.coe").sdo);
        assert!(
            !manifest.validate().is_empty(),
            "模板含 TODO 占位，应报违规"
        );
    }

    #[test]
    fn valid_manifest_has_no_violations() {
        let manifest = PluginManifest::parse(VALID).expect("parse");
        assert!(manifest.validate().is_empty(), "{:?}", manifest.validate());
    }

    #[test]
    fn rejects_missing_identity_and_sources() {
        // 缺 identity：解析阶段直接报错（R2 必须声明自有 VID/PID）。
        let no_identity =
            "schema: ecsim.plugin/v1\nid: com.ecsim.x\nname: x\nversion: 1\nkind: device\n";
        assert!(PluginManifest::parse(no_identity).is_err());

        // 有 identity 但缺 sources / 名称不带 SIMULATED → validate 报违规（R2/R5）。
        let no_sources = "schema: ecsim.plugin/v1\nid: com.ecsim.x\nname: x\nversion: 1\nkind: device\nidentity:\n  vendor_id: 1\n  product_code: 1\n  simulated: true\n";
        let manifest = PluginManifest::parse(no_sources).expect("parse");
        let problems = manifest.validate();
        assert!(
            problems.iter().any(|problem| problem.contains("sources")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|problem| problem.contains("SIMULATED")),
            "{problems:?}"
        );
    }
}
