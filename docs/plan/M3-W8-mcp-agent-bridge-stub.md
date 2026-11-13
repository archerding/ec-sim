# M3-W8：MCP/Agent 桥接 stub 设计（只读端点映射，**不含写**）

> 对应 backlog **B45 / B47** 的 **M3-W8**；关联 [M3 范围](M3-scope.md)、[CONTROL-PLANE](../CONTROL-PLANE.md)、
> [ADR 0002](../adr/0002-m3-plugin-host-and-write-paths.md)、[M3-W7 瓶颈调研](M3-W7-32slave-1khz-bottleneck.md)。
> **实现状态（2026-11-13，B47）**：S0（设计 + 工具目录 + 静态校验）与 S1/S2（`crates/mcp-server`
> 的 `ec-sim-mcp` 二进制 + `mcp-stdio-probe.sh`）已完成；本文兼作实现说明。
> **仍然只读**：工具目录里没有写工具，桥接代码只发 HTTP GET。
> 机器可读的工具清单在 [m3-w8-mcp-tools-v1.json](m3-w8-mcp-tools-v1.json)，
> 一致性由 `python3 scripts/spikes/mcp-tools-check.py` 静态校验（S0/S1 均跑通）。
> MCP 协议按公开规范（JSON-RPC 2.0 + tools 子集）实现，**未做**规范对照测试或一致性认证（AGENTS §4）。

## 1. 问题与范围

- 目标：让 Agent（MCP client：IDE/Agent 框架等）用**标准工具调用**读取 EC-Sim 的观测面，
  而不是让每个 Agent 自己拼 HTTP/curl；同时把"只能看、不能写"作为桥接层的硬边界。
- 本轮交付（可独立验证，已完成）：
  1. 本设计文档（架构决策、工具清单、错误映射、安全边界、实现状态）；
  2. 机器可读目录 `docs/plan/m3-w8-mcp-tools-v1.json`（9 个只读工具 + 6 个排除项）；
  3. 静态一致性检查 `scripts/spikes/mcp-tools-check.py`（目录 ↔ `CONTROL-PLANE.md` 路由表逐项核对）；
  4. 实现 `crates/mcp-server`（`ec-sim-mcp` 二进制，JSON-RPC 2.0 over stdio，零第三方依赖）
     + 端到端探针 `scripts/spikes/mcp-stdio-probe.sh`（B47）。
- 非目标（本桥接明确不做）：
  - **不含任何写**：`POST /sdo`、`POST /pdo` 永不映射，即使控制面开了 `--serve-write`；
  - 不映射 `/events`（SSE 长连接）、静态资源（`/`、`/index.html`、`/app.js`）；
  - 不做鉴权体系/TLS/多租户；不改变 ESC 实时环任何行为。

## 2. 设计决策（stub 级）

- **D1 传输用 stdio，不新开网络端口**：MCP 走 JSON-RPC 2.0 over stdio；桥接进程由 Agent 拉起，
  没有监听 socket，减少攻击面，也避免与 `--serve` 的 HTTP 端口语义混在一起。
- **D2 独立进程 `ec-sim-mcp`，经 `--api http://127.0.0.1:<port>` 只发 GET**：不嵌入实时进程
  （与 ADR 0002"插件/工具不进实时环"同向）；可对接静态 `--serve` 与 UDS/TAP 实时 `--serve`。
- **D3 只读 allowlist 写死在桥接层**：工具表在编译期常量/目录中，代码路径只允许 `GET`；
  这是**第二道**边界——控制面本身的写路由默认关闭，桥接层连工具都不提供。
- **D4 零第三方依赖**：MCP 侧自带最小 JSON 解析/序列化（`crate::json`）与 HTTP GET 客户端
  （`crate::http`，仅 `std::net`）；不复用控制面内部 JSON 助手（未公开为稳定 API），避免与实时侧耦合。
- **D5 原样透传端点 JSON**：工具返回端点原始响应（`content[0].text` 为 JSON 字符串），
  不重新解释字段、不改 schema 版本；`/metrics` 返回 Prometheus 文本原样。
- **D6 一工具一端点，命名 `ecsim_*`**：不做隐藏的聚合/缓存，避免"工具语义 ≠ 端点语义"。
- **D7 快照语义如实告知**：工具描述写明观测值/实时近似，不承诺实时性或一致性（同 CONTROL-PLANE §7）。

## 3. 工具清单与只读端点映射

| 工具 | HTTP | 端点 | 响应 schema | 模式 | 参数 | 说明 |
| --- | --- | --- | --- | --- | --- | --- |
| `ecsim_health` | GET | `/health` | 内联 JSON | 全部 | — | 健康检查（无鉴权豁免） |
| `ecsim_state` | GET | `/state`（`?slave=N`） | `ec-sim/state-v1` | 全部 | `slave` 可选 | 状态快照；实时模式含 `live` |
| `ecsim_dict` | GET | `/dict` | `ec-sim/dict-v1` | 全部 | — | 运行时字典；无 manifest 时服务端 404 |
| `ecsim_sdo_read` | GET | `/sdo?index=..&sub=..` | `ec-sim/sdo-v1` | 全部 | `index` 必填、`sub` 缺省 0 | 只读浏览，不经过 mailbox |
| `ecsim_topology` | GET | `/topology` | `ec-sim/topology-v1` | 实时 | — | 链上从站摘要 |
| `ecsim_pdo` | GET | `/pdo`（`?slave=N`） | `ec-sim/pdo-v1` | 实时 | `slave` 可选 | SM2/SM3 原始字节 + decoded |
| `ecsim_pdo_history` | GET | `/pdo?history=1`（可叠加 `slave`） | `ec-sim/pdo-history-v1` | 实时 | `slave` 可选 | 变化去重时间序列（每站 256 条） |
| `ecsim_metrics` | GET | `/metrics`（`?slave=N`） | Prometheus 文本（`version=0.0.4`） | 实时 | `slave` 可选 | 每站计数器/状态 |
| `ecsim_sdo_audit` | GET | `/sdo-audit` | `ec-sim/sdo-audit-v1` | 实时 | — | 最近 64 条 SDO 写审计（只读观测） |

排除项（见 JSON `excluded`，每条都有理由）：

- `GET /`、`GET /index.html`、`GET /app.js`：静态 HTML/前端脚本，不是结构化观测数据；
- `GET /events`：SSE 长连接，不适合单次请求/响应，后续可作为订阅类能力单独设计；
- `POST /sdo`、`POST /pdo`：写路径，**永不映射**（即使 `--serve-write` 已开启）。

## 4. 工具输入 schema（JSON Schema 草案）

公共约定：所有工具输入是 `type: object`、`additionalProperties: false`；未知参数直接报错
（不静默忽略）。完整定义以目录文件为准，下面给两个代表例。

```json
// ecsim_sdo_read
{
  "type": "object",
  "properties": {
    "index": {"type": "string", "pattern": "^(0[xX][0-9a-fA-F]{1,4}|[0-9]{1,5})$"},
    "sub":   {"type": "integer", "minimum": 0, "maximum": 255}
  },
  "required": ["index"],
  "additionalProperties": false
}
```

```json
// ecsim_state
{
  "type": "object",
  "properties": {"slave": {"type": "integer", "minimum": 0}},
  "required": [],
  "additionalProperties": false
}
```

- `slave` 是**从站索引**（不是站地址）；上界由服务端拓扑决定，越界由服务端返回 404。
- `index` 与 `/sdo` 一致：支持 `0x` 十六进制（如 `0x6040`）或十进制 u16；`sub` 为 u8。
- 参数校验失败 → 桥接层直接返回工具错误（`BAD_ARGUMENT`），**不发出 HTTP 请求**。

## 5. 错误映射（HTTP → 工具错误）

工具结果用 MCP 的 `isError` 语义 + 稳定 `code`（文本内容里带 JSON；⚠️ 字段名待按 MCP 规范复核）：

| 场景 | 建议 code | 说明 |
| --- | --- | --- |
| HTTP 200 | — | 原样返回响应体（`content[0].text`） |
| 400 参数错（服务端） | `BAD_ARGUMENT` | 例如 `/sdo` 长度/编码错误 |
| 404 | `NOT_FOUND` | 未知对象/子索引；或静态模式下 `/dict` 无 manifest |
| 404（仅实时路由在静态模式） | `MODE_UNAVAILABLE` | 提示"需要 `--serve` 与 UDS/TAP 同进程" |
| 401 | `AUTH_REQUIRED` | 提示通过 `--token-env` 配 Bearer token |
| 403 | `FORBIDDEN` | token 错误 / 非白名单对象（写路径，本 stub 不应出现） |
| 405 | `MODE_UNAVAILABLE` | 不应出现（工具只发 GET）；出现即上游路由变化，需修目录 |
| 5xx / 连接失败 | `UPSTREAM_ERROR` | 附服务端错误文本（不泄漏 token） |
| 连接/读取超时（默认 5 s） | `TIMEOUT` | 可重试一次；仍失败报错 |
| 响应 > 1 MiB | `RESPONSE_TOO_LARGE` | 不截断返回（避免 Agent 误判完整），提示用 `?slave=N` 缩小 |

## 6. 安全边界

- **只读双保险**：桥接层只有 GET 工具（D3）+ 控制面写路由默认关闭；`POST` 不进入工具表，
  也不接受"透传任意 URL/方法"的调试口。
- **token 只从环境变量读**：`--token-env VAR`（与 `--serve-token-env` 同名约定），值为空则不带头；
  `Authorization: Bearer <token>` 只发给上游，**绝不写入日志/工具输出/错误文本**。
- **默认只允许回环地址**：`--api` 必须是 `127.0.0.1`/`[::1]`；非回环需显式 `--allow-remote` 并告警，
  防止 Agent 把本机观测面代理到外部。
- **参数无路径拼接**：工具参数只有整数/枚举/受限字符串（`index` 有正则），不拼 URL 路径，
  无目录遍历/SSRF 面（`--api` 固定，工具不能改 host）。
- **资源上限**：单次超时 5 s、响应 ≤1 MiB、工具调用串行（stdio 单线程）；不缓存、不落盘。
- **提示注入边界**：工具描述只描述数据，不包含指令；端点返回的是**数据**，Agent 侧仍应按不可信输入处理。
- **审计**：桥接层把每次工具调用的 `name/endpoint/耗时/结果码/响应字节数` 写 stderr（JSON Lines），
  便于排查；不含 token、不含完整响应体。

## 7. 与实时环/一致性的关系

- 桥接进程**不在** `ec-sim-esc` 进程内：经 HTTP 读快照，不触碰共享内存/PDO arena/帧循环。
- `/state.live`、`/pdo`、`/metrics` 都是**读取时刻的近似值**，不构成实时性/一致性保证
  （与控制面文档一致）；需要严格因果的测试应走现有 UDS/TAP 探针，而不是 MCP 工具。
- 实时模式下 HTTP 请求只加锁读快照；MCP 侧并发上限 1，可避免观测流量放大。

## 8. 实现状态（原计划的 S0–S3）

- **S0（B45）✅**：本文 + 工具目录 + `mcp-tools-check.py` 静态校验；M3 清单/验证索引同步。
- **S1（B47）✅**：新增 crate `crates/mcp-server`（bin `ec-sim-mcp`，零第三方依赖）：
  `initialize`/`ping`/`tools/list`/`tools/call`；工具表 `include_str!` 嵌入
  `m3-w8-mcp-tools-v1.json`（单一事实来源）；`input_schema` 严格校验（未知参数/必填/整数范围）；
  HTTP 仅 GET、默认只连回环、`--token-env` 读 Bearer token、5 s 超时、1 MiB 响应上限；
  错误码映射 §5；16 个单测（JSON/HTTP/目录/参数/JSON-RPC）。**B50 增 2 个审计单测 → 18。**
- **S2（B47/B50）✅**：`scripts/spikes/mcp-stdio-probe.sh` + `mcp-stdio-probe.py`（python 合成 MCP client）：
  起真实静态 `--serve`，断言 9 工具、`state/dict/sdo` 读取、未知工具/缺参/静态模式 live-only 工具/
  未知方法/坏 JSON 错误路径，再以 `--serve-write` 起服务确认工具表**不变**；
  **B50 扩展**：token 矩阵（缺/错/对 → AUTH_REQUIRED/FORBIDDEN/200）、stderr 审计 **JSON Lines**、
  Linux 实时（`--uds`+`--serve`）live-only 工具端到端（topology/pdo/history/metrics/sdo_audit，
  含受控写产生审计），共 **31 项断言**。本机（Windows）与 Linux 验证机均 **EXIT=0**。
- **S3（已闭环）**：`mcp-stdio` 已登记进 `run-all-probes.sh`（类别 `any`，无需 root）；
  Linux 验证机整轮 `run-all-probes.sh` **20 PASS / 0 FAIL / 0 SKIP**（2026-11-13）。

## 9. 验收（当前实现）

- `python3 scripts/spikes/mcp-tools-check.py` → **OK**（9 个只读工具、文档全部 GET 路由有归宿、
  POST 只出现在排除项、设计文档与清单/验证索引互链）。
- `bash scripts/spikes/mcp-stdio-probe.sh` → **PASS**（本机与验证机）：9 个只读工具、
  `tools/list` 与目录一致、无写工具、`--serve-write` 下表不变、错误码符合 §5；B50 增 token 矩阵、
  审计 JSON Lines、Linux 实时 live-only 工具覆盖（共 31 项断言）。
- `cargo test --workspace` 含 `mcp-server` 18 个单测；`fmt`/`clippy -D warnings` 全绿。
- **B50 已闭环**：实时模式（UDS）下 live-only 工具（`/topology`、`/pdo`、`/pdo?history=1`、`/metrics`、
  `/sdo-audit`）端到端；token 模式矩阵；调用审计（stderr JSON Lines：ts_ms/event/tool/duration_ms/ok）。
  仍未做：`/events` 订阅类能力（非工具）、写工具（**永不映射**）。

## 10. 复现

```bash
# 静态工具目录与端点一致性（任意平台，无 root）
python3 scripts/spikes/mcp-tools-check.py

# 端到端：真实控制面（静态）+ MCP stdio 客户端
cargo build --release -p esc-node -p mcp-server
bash scripts/spikes/mcp-stdio-probe.sh

# 手动试用（两个终端）
cargo run -q --release -p esc-node --bin ec-sim-esc -- --serve 127.0.0.1:8091 \
  --manifest plugins/generic-cia402-cst-servo/plugin.yaml
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ecsim_sdo_read","arguments":{"index":"0x6040"}}}' \
  | target/release/ec-sim-mcp --api http://127.0.0.1:8091
```
