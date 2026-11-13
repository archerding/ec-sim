//! EC-Sim MCP/Agent 桥接（B47 / M3-W8）：把控制面只读观测端点映射为 MCP 工具
//! （JSON-RPC 2.0 over stdio）。**只读**——目录里没有任何写工具，代码只发 HTTP GET。
//!
//! 设计见 `docs/plan/M3-W8-mcp-agent-bridge-stub.md`，工具清单为
//! `docs/plan/m3-w8-mcp-tools-v1.json`（编译期 `include_str!`，单一事实来源）。
//! 零第三方依赖（默认构建）；协议细节按公开 MCP/JSON-RPC 规范实现，未做规范一致性认证。

pub mod http;
pub mod json;
pub mod rpc;
pub mod tools;

pub use http::{HttpClient, HttpResponse, Upstream, UpstreamError};
pub use json::Json;

/// MCP server 名称（`initialize` 的 `serverInfo.name`）。
pub const SERVER_NAME: &str = "ec-sim-mcp";
