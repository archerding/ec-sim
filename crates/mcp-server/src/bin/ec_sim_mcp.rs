//! `ec-sim-mcp`：MCP stdio 桥接二进制（只读）。
//!
//! 用法：
//!   `ec-sim-mcp [--api http://127.0.0.1:8091] [--token-env VAR] [--timeout-ms 5000]
//!               [--max-response-bytes 1048576] [--allow-remote]`
//!
//! 从 stdin 逐行读 JSON-RPC 2.0 请求，向 stdout 写响应（每行一条 JSON）；EOF 退出。
//! 只读工具表见 `docs/plan/m3-w8-mcp-tools-v1.json`（编译期嵌入）。

use std::io::{BufRead, Write};

use mcp_server::http::HttpClient;
use mcp_server::json::{self, Json};
use mcp_server::tools::Catalog;

const DEFAULT_API: &str = "http://127.0.0.1:8091";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

struct Options {
    api: String,
    token_env: Option<String>,
    timeout_ms: u64,
    max_response_bytes: usize,
    allow_remote: bool,
}

fn main() {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("ec-sim-mcp: {message}");
            std::process::exit(2);
        }
    };
    let token = options
        .token_env
        .as_deref()
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty());
    let catalog = match Catalog::load() {
        Ok(catalog) => catalog,
        Err(message) => {
            eprintln!("ec-sim-mcp: 工具目录非法：{message}");
            std::process::exit(2);
        }
    };
    let mut upstream = match HttpClient::new(
        &options.api,
        token,
        options.timeout_ms,
        options.max_response_bytes,
        options.allow_remote,
    ) {
        Ok(client) => client,
        Err(message) => {
            eprintln!("ec-sim-mcp: {message}");
            std::process::exit(2);
        }
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("ec-sim-mcp: 读取 stdin 失败：{error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let request = match json::parse(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = json::parse(
                    r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}"#,
                )
                .expect("静态 JSON");
                let _ = writeln!(stdout, "{}", response.to_json());
                let _ = stdout.flush();
                eprintln!("ec-sim-mcp: JSON 解析失败：{error}");
                continue;
            }
        };
        let started_ms = now_ms();
        let response = mcp_server::rpc::handle(&request, &catalog, &mut upstream);
        let duration_ms = now_ms().saturating_sub(started_ms);
        if let Some(response) = &response {
            log_tool_call(&request, response, duration_ms);
        }
        let Some(response) = response else {
            continue;
        };
        if writeln!(stdout, "{}", response.to_json()).is_err() {
            break;
        }
        let _ = stdout.flush();
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// 调用审计：每次 `tools/call` 向 stderr 写一行 **JSON Lines**（B50）。
///
/// 字段：`ts_ms` / `event` / `tool` / `duration_ms` / `ok` / `error_code`（仅失败时）。
/// 这是**只读调用**审计，不含参数值/响应体，避免把 PDO/SDO 值写入日志。
fn log_tool_call(request: &Json, response: &Json, duration_ms: u64) {
    let method = request.get("method").and_then(Json::as_str).unwrap_or("?");
    if method != "tools/call" {
        return;
    }
    let name = request
        .get("params")
        .and_then(|params| params.get("name"))
        .and_then(Json::as_str)
        .unwrap_or("?");
    let result = response.get("result");
    let is_error = result
        .and_then(|result| result.get("isError"))
        .and_then(|value| match value {
            Json::Bool(value) => Some(*value),
            _ => None,
        })
        .unwrap_or(false);
    let error_code = if is_error {
        tool_error_code(result)
    } else {
        None
    };
    eprintln!(
        "{}",
        audit_line(
            name,
            duration_ms,
            !is_error,
            error_code.as_deref(),
            now_ms()
        )
    );
}

fn tool_error_code(result: Option<&Json>) -> Option<String> {
    let text = result?
        .get("content")?
        .as_array()?
        .first()?
        .get("text")?
        .as_str()?;
    let parsed = json::parse(text).ok()?;
    parsed
        .get("code")
        .and_then(Json::as_str)
        .map(str::to_string)
}

fn audit_line(
    tool: &str,
    duration_ms: u64,
    ok: bool,
    error_code: Option<&str>,
    ts_ms: u64,
) -> String {
    let mut entries: Vec<(&str, Json)> = vec![
        ("ts_ms", Json::Number(ts_ms as f64)),
        ("event", Json::String("tools/call".to_string())),
        ("tool", Json::String(tool.to_string())),
        ("duration_ms", Json::Number(duration_ms as f64)),
        ("ok", Json::Bool(ok)),
    ];
    if let Some(code) = error_code {
        entries.push(("error_code", Json::String(code.to_string())));
    }
    Json::object(entries).to_json()
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        api: DEFAULT_API.to_string(),
        token_env: None,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        allow_remote: false,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--api" => {
                options.api = arguments.next().ok_or_else(|| "--api 缺少值".to_string())?;
            }
            "--token-env" => {
                options.token_env = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--token-env 缺少环境变量名".to_string())?,
                );
            }
            "--timeout-ms" => {
                options.timeout_ms = arguments
                    .next()
                    .ok_or_else(|| "--timeout-ms 缺少值".to_string())?
                    .parse()
                    .map_err(|_| "--timeout-ms 必须是正整数".to_string())?;
            }
            "--max-response-bytes" => {
                options.max_response_bytes = arguments
                    .next()
                    .ok_or_else(|| "--max-response-bytes 缺少值".to_string())?
                    .parse()
                    .map_err(|_| "--max-response-bytes 必须是正整数".to_string())?;
            }
            "--allow-remote" => options.allow_remote = true,
            "--version" | "-V" => {
                println!("ec-sim-mcp {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!(
                    "ec-sim-mcp {}：EC-Sim MCP 桥接（只读工具，JSON-RPC 2.0 over stdio）\n\
                     用法：ec-sim-mcp [--api http://127.0.0.1:8091] [--token-env VAR]\n\
                     \x20                [--timeout-ms 5000] [--max-response-bytes 1048576] [--allow-remote]\n\
                     工具见 docs/plan/m3-w8-mcp-tools-v1.json；不含任何写工具。",
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(0);
            }
            other => return Err(format!("未知参数 {other}（--help 查看用法）")),
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_line_is_valid_json_lines() {
        let line = audit_line("ecsim_state", 12, true, None, 1_700_000_000_000);
        let parsed = json::parse(&line).expect("audit line 必须是合法 JSON");
        assert_eq!(
            parsed.get("event").and_then(Json::as_str),
            Some("tools/call")
        );
        assert_eq!(
            parsed.get("tool").and_then(Json::as_str),
            Some("ecsim_state")
        );
        assert_eq!(parsed.get("ok").and_then(Json::as_str), None);
        assert_eq!(parsed.get("duration_ms").and_then(Json::as_i64), Some(12));
    }

    #[test]
    fn audit_line_escapes_tool_name_and_marks_error_code() {
        let line = audit_line("bad\"tool\\name", 3, false, Some("TOOL_NOT_FOUND"), 1);
        let parsed = json::parse(&line).expect("audit line 必须是合法 JSON");
        assert_eq!(
            parsed.get("tool").and_then(Json::as_str),
            Some("bad\"tool\\name")
        );
        assert_eq!(
            parsed.get("error_code").and_then(Json::as_str),
            Some("TOOL_NOT_FOUND")
        );
        assert_eq!(parsed.get("ok").and_then(Json::as_str), None);
    }
}
