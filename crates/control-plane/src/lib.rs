//! 控制面。
//!
//! 规划：对外提供 REST/gRPC 与 MCP 操作面，承载"虚拟总线 + 插件宿主 + AI 可操作"
//! 中的可操作性；Web 前端（`web/`）从 M2 起接这里，B1 仅占位模块边界。
//!
//! 依赖方向：`control-plane` → `esc-core`/`plugin-host`/`wire`；不得被这些 crate 反向依赖
//! （避免控制面把 web/网络依赖带进实时路径）。
