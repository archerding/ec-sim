//! MCP JSON-RPC 2.0 处理（stdio 传输由 bin 负责）：`initialize` / `ping` /
//! `tools/list` / `tools/call`。只读工具经 [`Upstream`] 发 GET；没有写路径。

use crate::http::{HttpResponse, Upstream};
use crate::json::Json;
use crate::tools::{self, Catalog, Tool};
use crate::SERVER_NAME;

/// 默认 MCP 协议版本（`initialize` 未带版本时使用）。
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// 处理一条 JSON-RPC 请求；返回 `None` 表示无需响应（通知/响应消息）。
pub fn handle(request: &Json, catalog: &Catalog, upstream: &mut dyn Upstream) -> Option<Json> {
    // JSON-RPC 通知（无 id）：不响应（`notifications/initialized` 等）。
    let id = request
        .get("id")
        .filter(|value| !matches!(value, Json::Null))
        .cloned()?;
    let method = request.get("method").and_then(Json::as_str);
    let Some(method) = method else {
        return Some(error_response(Some(&id), -32600, "缺少 method"));
    };
    let response = match method {
        "initialize" => ok_response(Some(&id), initialize(request)),
        "ping" => ok_response(Some(&id), Json::Object(Vec::new())),
        "tools/list" => ok_response(Some(&id), catalog.tools_list()),
        "tools/call" => tools_call(Some(&id), request, catalog, upstream),
        other => error_response(Some(&id), -32601, &format!("未知方法 {other}")),
    };
    Some(response)
}

fn initialize(request: &Json) -> Json {
    let version = request
        .get("params")
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Json::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    Json::object(vec![
        ("protocolVersion", Json::String(version.to_string())),
        (
            "capabilities",
            Json::object(vec![("tools", Json::Object(Vec::new()))]),
        ),
        (
            "serverInfo",
            Json::object(vec![
                ("name", Json::String(SERVER_NAME.to_string())),
                (
                    "version",
                    Json::String(env!("CARGO_PKG_VERSION").to_string()),
                ),
            ]),
        ),
    ])
}

fn tools_call(
    id: Option<&Json>,
    request: &Json,
    catalog: &Catalog,
    upstream: &mut dyn Upstream,
) -> Json {
    let Some(params) = request.get("params").and_then(Json::as_object) else {
        return error_response(id, -32602, "tools/call 缺少 params 对象");
    };
    let find = |key: &str| {
        params
            .iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    };
    let Some(name) = find("name").and_then(Json::as_str) else {
        return error_response(id, -32602, "tools/call 缺少 tool name");
    };
    let Some(tool) = catalog.get(name) else {
        return ok_response(
            id,
            tool_error("TOOL_NOT_FOUND", &format!("未知工具 {name}"), None),
        );
    };
    let arguments = find("arguments")
        .cloned()
        .unwrap_or(Json::Object(Vec::new()));
    if let Err(message) = tools::validate_arguments(tool, &arguments) {
        return ok_response(id, tool_error("BAD_ARGUMENT", &message, None));
    }
    let path = match tools::build_path(tool, &arguments) {
        Ok(path) => path,
        Err(message) => return ok_response(id, tool_error("BAD_ARGUMENT", &message, None)),
    };
    match upstream.get(&path) {
        Ok(response) => ok_response(id, success_response(tool, &response)),
        Err(error) => ok_response(id, tool_error(error.code(), &error.message(), None)),
    }
}

fn success_response(tool: &Tool, response: &HttpResponse) -> Json {
    if response.status == 200 {
        return tool_result(&response.body, false);
    }
    let code = match response.status {
        400 => "BAD_ARGUMENT",
        401 => "AUTH_REQUIRED",
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        405 => "MODE_UNAVAILABLE",
        _ => "HTTP_ERROR",
    };
    let hint = match tool.mode.as_str() {
        "live" if response.status == 404 => {
            "（该工具仅在实时模式 `--serve` + UDS/TAP 同进程时可用）"
        }
        _ => "",
    };
    let mut text = Json::object(vec![
        ("code", Json::String(code.to_string())),
        (
            "message",
            Json::String(format!("上游返回 HTTP {}{hint}", response.status)),
        ),
        ("http_status", Json::Number(f64::from(response.status))),
    ]);
    const BODY_LIMIT: usize = 200;
    if !response.body.is_empty() {
        let snippet: String = response.body.chars().take(BODY_LIMIT).collect();
        text = match text {
            Json::Object(mut entries) => {
                entries.push(("body_snippet".to_string(), Json::String(snippet)));
                Json::Object(entries)
            }
            other => other,
        };
    }
    tool_result(&text.to_json(), true)
}

fn tool_result(text: &str, is_error: bool) -> Json {
    Json::object(vec![
        (
            "content",
            Json::Array(vec![Json::object(vec![
                ("type", Json::String("text".to_string())),
                ("text", Json::String(text.to_string())),
            ])]),
        ),
        ("isError", Json::Bool(is_error)),
    ])
}

fn tool_error(code: &str, message: &str, http_status: Option<u16>) -> Json {
    let mut entries = vec![
        ("code".to_string(), Json::String(code.to_string())),
        ("message".to_string(), Json::String(message.to_string())),
    ];
    if let Some(status) = http_status {
        entries.push(("http_status".to_string(), Json::Number(f64::from(status))));
    }
    tool_result(&Json::Object(entries).to_json(), true)
}

fn ok_response(id: Option<&Json>, result: Json) -> Json {
    Json::object(vec![
        ("jsonrpc", Json::String("2.0".to_string())),
        ("id", id.cloned().unwrap_or(Json::Null)),
        ("result", result),
    ])
}

fn error_response(id: Option<&Json>, code: i64, message: &str) -> Json {
    Json::object(vec![
        ("jsonrpc", Json::String("2.0".to_string())),
        ("id", id.cloned().unwrap_or(Json::Null)),
        (
            "error",
            Json::object(vec![
                ("code", Json::Number(code as f64)),
                ("message", Json::String(message.to_string())),
            ]),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::UpstreamError;
    use crate::json;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeUpstream {
        replies: HashMap<String, Result<HttpResponse, UpstreamError>>,
        seen: Vec<String>,
    }

    impl FakeUpstream {
        fn with(path: &str, status: u16, body: &str) -> Self {
            let mut replies = HashMap::new();
            replies.insert(
                path.to_string(),
                Ok(HttpResponse {
                    status,
                    body: body.to_string(),
                }),
            );
            Self {
                replies,
                seen: Vec::new(),
            }
        }
    }

    impl Upstream for FakeUpstream {
        fn get(&mut self, path: &str) -> Result<HttpResponse, UpstreamError> {
            self.seen.push(path.to_string());
            self.replies
                .get(path)
                .cloned()
                .unwrap_or_else(|| Err(UpstreamError::Io(format!("未配置的上游路径 {path}"))))
        }
    }

    fn catalog() -> Catalog {
        Catalog::load().expect("目录")
    }

    fn call(request: &str, upstream: &mut dyn Upstream) -> Json {
        let value = json::parse(request).expect("请求 JSON");
        handle(&value, &catalog(), upstream).expect("请求需要响应")
    }

    fn result_text(response: &Json) -> String {
        response
            .get("result")
            .and_then(|result| result.get("content"))
            .and_then(Json::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn initialize_reports_protocol_and_server_info() {
        let mut upstream = FakeUpstream::default();
        let response = call(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
            &mut upstream,
        );
        assert_eq!(response.get("jsonrpc").and_then(Json::as_str), Some("2.0"));
        assert_eq!(response.get("id").and_then(Json::as_i64), Some(1));
        let result = response.get("result").expect("result");
        assert_eq!(
            result.get("protocolVersion").and_then(Json::as_str),
            Some("2025-03-26")
        );
        assert_eq!(
            result
                .get("serverInfo")
                .and_then(|info| info.get("name"))
                .and_then(Json::as_str),
            Some(SERVER_NAME)
        );
    }

    #[test]
    fn tools_list_exposes_exactly_the_catalog_and_no_writes() {
        let mut upstream = FakeUpstream::default();
        let response = call(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            &mut upstream,
        );
        let tools = response
            .get("result")
            .and_then(|result| result.get("tools"))
            .and_then(Json::as_array)
            .expect("tools");
        assert_eq!(tools.len(), 9);
        for tool in tools {
            let name = tool.get("name").and_then(Json::as_str).expect("name");
            assert!(name.starts_with("ecsim_"), "工具名 {name}");
            assert!(!name.contains("write"), "不应有写工具：{name}");
            assert!(tool.get("inputSchema").is_some());
        }
        assert!(upstream.seen.is_empty(), "tools/list 不应访问上游");
    }

    #[test]
    fn tools_call_state_returns_json_text() {
        let mut upstream = FakeUpstream::with("/state", 200, r#"{"schema":"ec-sim/state-v1"}"#);
        let response = call(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ecsim_state","arguments":{}}}"#,
            &mut upstream,
        );
        assert_eq!(upstream.seen, vec!["/state"]);
        assert_eq!(result_text(&response), r#"{"schema":"ec-sim/state-v1"}"#);
        let is_error = response
            .get("result")
            .and_then(|result| result.get("isError"))
            .cloned();
        assert_eq!(is_error, Some(Json::Bool(false)));
    }

    #[test]
    fn tools_call_pdo_history_uses_fixed_query() {
        let mut upstream = FakeUpstream::with("/pdo?slave=1&history=1", 200, "{}");
        call(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ecsim_pdo_history","arguments":{"slave":1}}}"#,
            &mut upstream,
        );
        assert_eq!(upstream.seen, vec!["/pdo?slave=1&history=1"]);
    }

    #[test]
    fn unknown_tool_and_bad_arguments_are_tool_errors_without_upstream_calls() {
        let mut upstream = FakeUpstream::default();
        let unknown = call(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ecsim_nope","arguments":{}}}"#,
            &mut upstream,
        );
        assert!(result_text(&unknown).contains("TOOL_NOT_FOUND"));
        let bad = call(
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"ecsim_sdo_read","arguments":{"sub":0}}}"#,
            &mut upstream,
        );
        assert!(result_text(&bad).contains("BAD_ARGUMENT"));
        assert!(upstream.seen.is_empty());
    }

    #[test]
    fn http_errors_map_to_stable_codes() {
        let mut upstream = FakeUpstream::with("/topology", 405, "method not allowed");
        let response = call(
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"ecsim_topology","arguments":{}}}"#,
            &mut upstream,
        );
        assert!(result_text(&response).contains("MODE_UNAVAILABLE"));

        let mut upstream = FakeUpstream::with("/sdo?index=0x6040", 401, "no token");
        let response = call(
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"ecsim_sdo_read","arguments":{"index":"0x6040"}}}"#,
            &mut upstream,
        );
        assert!(result_text(&response).contains("AUTH_REQUIRED"));
    }

    #[test]
    fn unknown_method_and_notifications() {
        let mut upstream = FakeUpstream::default();
        let response = call(r#"{"jsonrpc":"2.0","id":9,"method":"nope"}"#, &mut upstream);
        assert_eq!(
            response
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Json::as_i64),
            Some(-32601)
        );
        let notification =
            json::parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).expect("json");
        assert!(handle(&notification, &catalog(), &mut upstream).is_none());
    }
}
