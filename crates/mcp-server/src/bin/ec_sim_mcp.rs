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
        log_call(&request);
        let Some(response) = mcp_server::rpc::handle(&request, &catalog, &mut upstream) else {
            continue;
        };
        if writeln!(stdout, "{}", response.to_json()).is_err() {
            break;
        }
        let _ = stdout.flush();
    }
}

fn log_call(request: &Json) {
    let method = request.get("method").and_then(Json::as_str).unwrap_or("?");
    if method == "tools/call" {
        let name = request
            .get("params")
            .and_then(|params| params.get("name"))
            .and_then(Json::as_str)
            .unwrap_or("?");
        eprintln!("ec-sim-mcp: tools/call {name}");
    }
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
