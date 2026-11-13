//! 工具目录：编译期嵌入 `docs/plan/m3-w8-mcp-tools-v1.json`（单一事实来源），
//! 负责参数校验与只读端点 URL 构造。目录里没有写工具，`method` 必须全是 GET。

use crate::json::{self, Json};

/// 机器可读工具目录（与 `scripts/spikes/mcp-tools-check.py` 校验的是同一文件）。
pub const CATALOG_JSON: &str = include_str!("../../../docs/plan/m3-w8-mcp-tools-v1.json");

/// 一个只读工具。
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub method: String,
    pub endpoint: String,
    /// 允许的查询参数（与输入 schema 的字段对应）。
    pub queries: Vec<String>,
    /// 固定查询参数（如 `/pdo?history=1` 的 `history=1`）。
    pub fixed_query: Vec<(String, String)>,
    pub mode: String,
    pub input_schema: Json,
}

/// 工具目录。
#[derive(Debug, Clone)]
pub struct Catalog {
    pub tools: Vec<Tool>,
}

impl Catalog {
    /// 解析编译期嵌入的目录；结构不合法（含写方法）直接报错。
    pub fn load() -> Result<Self, String> {
        let root = json::parse(CATALOG_JSON)?;
        if root.get("read_only") != Some(&Json::Bool(true)) {
            return Err("工具目录必须 read_only=true".to_string());
        }
        let items = root
            .get("tools")
            .and_then(Json::as_array)
            .ok_or_else(|| "工具目录缺少 tools 数组".to_string())?;
        let mut tools = Vec::with_capacity(items.len());
        for item in items {
            let name = item
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| "工具缺少 name".to_string())?
                .to_string();
            let description = item
                .get("description")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            let method = item
                .get("http")
                .and_then(|http| http.get("method"))
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            if method != "GET" {
                return Err(format!("工具 {name} 的 method 必须是 GET（当前 {method}）"));
            }
            let endpoint = item
                .get("http")
                .and_then(|http| http.get("endpoint"))
                .and_then(Json::as_str)
                .ok_or_else(|| format!("工具 {name} 缺少 http.endpoint"))?
                .to_string();
            let queries = item
                .get("queries")
                .and_then(Json::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let mut fixed_query = Vec::new();
            if let Some(fixed) = item.get("fixed_query").and_then(Json::as_object) {
                for (key, value) in fixed {
                    let text = value
                        .as_str()
                        .ok_or_else(|| format!("工具 {name} 的 fixed_query.{key} 必须是字符串"))?;
                    fixed_query.push((key.clone(), text.to_string()));
                }
            }
            let mode = item
                .get("mode")
                .and_then(Json::as_str)
                .unwrap_or("any")
                .to_string();
            let input_schema = item
                .get("input_schema")
                .cloned()
                .ok_or_else(|| format!("工具 {name} 缺少 input_schema"))?;
            tools.push(Tool {
                name,
                description,
                method,
                endpoint,
                queries,
                fixed_query,
                mode,
                input_schema,
            });
        }
        Ok(Self { tools })
    }

    /// 按名字查找工具。
    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    /// `tools/list` 的 result（MCP 只暴露 name/description/inputSchema）。
    pub fn tools_list(&self) -> Json {
        let items: Vec<Json> = self
            .tools
            .iter()
            .map(|tool| {
                Json::object(vec![
                    ("name", Json::String(tool.name.clone())),
                    ("description", Json::String(tool.description.clone())),
                    ("inputSchema", tool.input_schema.clone()),
                ])
            })
            .collect();
        Json::object(vec![("tools", Json::Array(items))])
    }
}

/// 校验工具参数：`additionalProperties:false`、必填、整数范围/类型。
pub fn validate_arguments(tool: &Tool, arguments: &Json) -> Result<(), String> {
    let entries = match arguments {
        Json::Object(entries) => entries,
        Json::Null => return validate_arguments(tool, &Json::Object(Vec::new())),
        _ => return Err("arguments 必须是对象".to_string()),
    };
    let schema = tool
        .input_schema
        .as_object()
        .ok_or_else(|| "input_schema 必须是对象".to_string())?;
    let find = |key: &str| {
        schema
            .iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    };
    let properties = find("properties")
        .and_then(Json::as_object)
        .ok_or_else(|| "input_schema.properties 缺失".to_string())?;
    if find("additionalProperties") != Some(&Json::Bool(false)) {
        return Err("input_schema 必须 additionalProperties=false".to_string());
    }
    for (key, _) in entries {
        if !properties.iter().any(|(name, _)| name == key) {
            return Err(format!("未知参数 {key}"));
        }
    }
    if let Some(required) = find("required").and_then(Json::as_array) {
        for name in required.iter().filter_map(Json::as_str) {
            if !entries.iter().any(|(key, _)| key == name) {
                return Err(format!("缺少必填参数 {name}"));
            }
        }
    }
    for (key, value) in entries {
        let Some((_, property)) = properties.iter().find(|(name, _)| name == key) else {
            continue;
        };
        let kind = property.get("type").and_then(Json::as_str).unwrap_or("");
        match kind {
            "integer" => {
                let Some(number) = value.as_i64() else {
                    return Err(format!("参数 {key} 必须是整数"));
                };
                if let Some(minimum) = property.get("minimum").and_then(Json::as_i64) {
                    if number < minimum {
                        return Err(format!("参数 {key} 小于最小值 {minimum}"));
                    }
                }
                if let Some(maximum) = property.get("maximum").and_then(Json::as_i64) {
                    if number > maximum {
                        return Err(format!("参数 {key} 大于最大值 {maximum}"));
                    }
                }
            }
            "string" => {
                if value.as_str().is_none() {
                    return Err(format!("参数 {key} 必须是字符串"));
                }
            }
            "boolean" => {
                if !matches!(value, Json::Bool(_)) {
                    return Err(format!("参数 {key} 必须是布尔值"));
                }
            }
            other => return Err(format!("参数 {key} 的类型 {other} 不受支持")),
        }
    }
    Ok(())
}

/// 构造只读 GET 路径（含固定查询参数与用户参数；值做百分号编码）。
pub fn build_path(tool: &Tool, arguments: &Json) -> Result<String, String> {
    let empty = Json::Object(Vec::new());
    let arguments = if matches!(arguments, Json::Null) {
        &empty
    } else {
        arguments
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(properties) = tool
        .input_schema
        .get("properties")
        .and_then(Json::as_object)
    {
        for (name, _) in properties {
            let Some(value) = arguments.get(name) else {
                continue;
            };
            let text = match value {
                Json::String(text) => text.clone(),
                Json::Number(_) => value
                    .as_i64()
                    .ok_or_else(|| format!("参数 {name} 不是整数"))?
                    .to_string(),
                Json::Bool(flag) => flag.to_string(),
                _ => return Err(format!("参数 {name} 不是标量")),
            };
            pairs.push((name.clone(), text));
        }
    }
    pairs.extend(tool.fixed_query.iter().cloned());
    if pairs.is_empty() {
        return Ok(tool.endpoint.clone());
    }
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!("{}?{query}", tool.endpoint))
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let unreserved = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~' | b':' | b'[' | b']');
        if unreserved {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::load().expect("目录应合法")
    }

    #[test]
    fn catalog_has_nine_read_only_tools() {
        let catalog = catalog();
        assert_eq!(catalog.tools.len(), 9);
        assert!(catalog.tools.iter().all(|tool| tool.method == "GET"));
        for name in [
            "ecsim_health",
            "ecsim_state",
            "ecsim_dict",
            "ecsim_sdo_read",
            "ecsim_topology",
            "ecsim_pdo",
            "ecsim_pdo_history",
            "ecsim_metrics",
            "ecsim_sdo_audit",
        ] {
            assert!(catalog.get(name).is_some(), "缺工具 {name}");
        }
    }

    #[test]
    fn builds_paths_with_queries_and_fixed_query() {
        let catalog = catalog();
        let state = catalog.get("ecsim_state").expect("state");
        assert_eq!(
            build_path(state, &Json::Object(vec![])).expect("p"),
            "/state"
        );
        assert_eq!(
            build_path(state, &crate::json::parse(r#"{"slave":2}"#).expect("json")).expect("p"),
            "/state?slave=2"
        );

        let history = catalog.get("ecsim_pdo_history").expect("history");
        assert_eq!(
            build_path(
                history,
                &crate::json::parse(r#"{"slave":1}"#).expect("json")
            )
            .expect("p"),
            "/pdo?slave=1&history=1"
        );

        let sdo = catalog.get("ecsim_sdo_read").expect("sdo");
        assert_eq!(
            build_path(
                sdo,
                &crate::json::parse(r#"{"index":"0x6040","sub":0}"#).expect("json")
            )
            .expect("p"),
            "/sdo?index=0x6040&sub=0"
        );
    }

    #[test]
    fn validates_arguments_strictly() {
        let catalog = catalog();
        let state = catalog.get("ecsim_state").expect("state");
        assert!(validate_arguments(state, &Json::Object(vec![])).is_ok());
        assert!(
            validate_arguments(state, &crate::json::parse(r#"{"slave":0}"#).expect("json")).is_ok()
        );
        assert!(
            validate_arguments(state, &crate::json::parse(r#"{"slave":-1}"#).expect("json"))
                .is_err()
        );
        assert!(
            validate_arguments(state, &crate::json::parse(r#"{"other":1}"#).expect("json"))
                .is_err()
        );

        let sdo = catalog.get("ecsim_sdo_read").expect("sdo");
        assert!(
            validate_arguments(sdo, &crate::json::parse(r#"{"sub":1}"#).expect("json")).is_err()
        );
        assert!(
            validate_arguments(sdo, &crate::json::parse(r#"{"index":123}"#).expect("json"))
                .is_err()
        );
        assert!(
            validate_arguments(sdo, &crate::json::parse(r#"{"index":"x"}"#).expect("json")).is_ok()
        );
    }
}
