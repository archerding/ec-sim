# M2 退出标准核对清单（2026-11-13）

> 目的：把 **M2「控制面 + WebUI（只读观测面，写操作显式开关）」** 的退出条件逐条对照当前证据，
> 给出"已达成 / 未达成 / 下一步"。**不是认证或承诺**；C++/IgH/SOEM 探针只作零补丁主站夹具。
> 相关：[STATUS.md](../STATUS.md)、[VERIFICATION.md](../VERIFICATION.md)、[CONTROL-PLANE.md](../CONTROL-PLANE.md)、
> [M1 退出清单](M1-exit-checklist.md)。

## 1. 范围（M2）

- **控制面**（`crates/control-plane` + `ec-sim-esc --serve`）：本机只读 HTTP JSON API + SSE 推送 +
  Prometheus 文本指标；显式开关下的最小写操作。
- **WebUI**（`web/`）：零依赖只读页面，可离线载入 dump JSON 或直连控制面 API。
- **非目标**（留给 M3+）：SDO 在线编辑、ESI 导入、插件热加载/WASM、MCP、鉴权/TLS、多用户、
  32 从站 @1 kHz、一致性/认证结论。

## 2. 退出条件

| # | 退出条件 | 状态 | 证据 | 备注 |
| --- | --- | --- | --- | --- |
| 1 | `control-plane` 只读快照序列化（state/dict）+ 路由骨架 | ✅ | `control-plane` 13 个单测；`state_json`/`dict_json` | B20 |
| 2 | 只读 HTTP JSON API（std `TcpListener`，零外部依赖） | ✅ | `http-state-probe.sh` **EXIT=0**（`/state`、`/dict`、404、CORS/缓存头） | B21 |
| 3 | 实时模式：UDS/TAP 与 HTTP 共享 `Responder` 状态 | ✅ | `live-state-probe.sh`（AL INIT→PREOP）、`tap-live-state-probe.sh`（SOEM 推入 OP）**EXIT=0** | B22 / B24 |
| 4 | PDO 快照（当前值 + 历史 + 具名解码 + 逐站） | ✅ | `pdo-live-probe.sh`、`pdo-history-probe.sh`、`pdo-decode-probe.sh`、`slave-select-probe.sh` **EXIT=0** | B23 / B25 / B26 / B28 |
| 5 | 多从站拓扑摘要（`/topology`）与 Web 链式 SVG | ✅ | `topology-probe.sh` **EXIT=0**；`web/` 人工验收 | B27 / B32 |
| 6 | Prometheus 文本指标（每站标签 + `?slave=N` 筛选） | ✅ | `metrics-probe.sh`、`metrics-filter-probe.sh` **EXIT=0** | B29 / B30 |
| 7 | WebUI 只读页面：state/dict/pdo/波形/拓扑/指标/实时/SSE | ✅（人工验收） | `node --check web/app.js`；⚠️ 无自动化 DOM 测试 | B18/B19/B32/B34 |
| 8 | 控制面文档 + 文档/代码一致性检查 | ✅ | `docs/CONTROL-PLANE.md`；`control-plane-doc-check.py` **EXIT=0**（12 路由 / 13 探针） | B31 |
| 9 | `--serve --web-root web` 同源托管（静态 + 实时） | ✅ | `serve-web-probe.sh` **EXIT=0** | B33 |
| 10 | 写操作显式开关 `--serve-write` + `POST /pdo`（默认 405） | ✅ | `write-pdo-probe.sh` **EXIT=0**（默认拒绝 / 开启生效） | B35 |
| 11 | 安全边界文档化（只读默认、无鉴权、仅回环/受信环境、写开关警告） | ✅ | `docs/CONTROL-PLANE.md` §6/§7 | B31 / B35 |

## 3. 未完成 / 明确范围外（M3+）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| SDO 在线编辑（读写对象字典） | ❌ 未做 | 需 CoE SDO upload/download 写路径 + 权限模型 |
| ESI 导入 / 插件热加载 / WASM 行为插件 | ❌ 未做 | M3 插件宿主 |
| MCP server / Agent Skills 接入 | ❌ 未做 | 路线图 L5 |
| 鉴权 / TLS / 多用户 | ❌ 未做 | 当前仅回环/受信环境 |
| Web 自动化 DOM 测试 | ⚠️ 缺 | 目前 `node --check` + 人工浏览器验收 |
| 32 从站 @1 kHz | ❌ 未做 | M1 清单已记录 8 从站能力；32 属后续 |
| 验证机 clippy | ⚠️ 未装 | VM 以 `cargo build --release` 0 warning + 本机 clippy 覆盖 |

## 4. 验证命令（本机 + Linux 验证机）

```bash
# 一键预回归（按平台/root 自动选择探针；验证机 14 PASS）
bash scripts/spikes/run-all-probes.sh

# 单元/集成 + 静态检查
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 文档/代码一致性（任意平台）
python3 scripts/spikes/control-plane-doc-check.py

# 只读控制面（Linux/UDS；root 的 TAP 项在验证机）
BIN=target/release/ec-sim-esc bash scripts/spikes/http-state-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/live-state-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/pdo-live-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/pdo-history-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/pdo-decode-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/topology-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/slave-select-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/metrics-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/metrics-filter-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/serve-web-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/events-probe.sh
BIN=target/release/ec-sim-esc bash scripts/spikes/write-pdo-probe.sh

# Web 前端语法 + 人工浏览器验收（⚠️ 无自动化 DOM 测试）
node --check web/app.js
```

## 5. 结论

M3（插件宿主 / SDO 写路径 / 鉴权）范围与退出条件见 [M3 范围](M3-scope.md) 与
[ADR 0002](../adr/0002-m3-plugin-host-and-write-paths.md)。

**M2 的"只读观测面 + 显式写开关"已达到退出候选状态**：控制面 API/SSE/指标/文档/测试齐全，
WebUI 可同源或离线使用，写操作默认关闭且文档化警告。未完成项（SDO 编辑、插件热加载、
ESI 导入、鉴权/TLS、MCP、32 从站）明确划入 M3+，不阻塞 M2 收尾。
