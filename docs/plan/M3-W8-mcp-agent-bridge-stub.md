# M3-W8：MCP/Agent 桥接 stub 设计（只读端点映射，**不含写**）

> 对应 backlog B45 的 **M3-W8**；关联 [M3 范围](M3-scope.md)、[CONTROL-PLANE](../CONTROL-PLANE.md)、
> [ADR 0002](../adr/0002-m3-plugin-host-and-write-paths.md)、[M3-W7 瓶颈调研](M3-W7-32slave-1khz-bottleneck.md)。
> 本文是**设计/stub 文档**，不是 MCP server 实现；"将实现"均为计划，未落地项标 ⚠️。
> 机器可读的工具清单在 [m3-w8-mcp-tools-v1.json](m3-w8-mcp-tools-v1.json)，
> 一致性由 `python3 scripts/spikes/mcp-tools-check.py` 静态校验（本班已跑通）。
> MCP 协议细节按公开规范（JSON-RPC 2.0 + tools 子集）整理，**落地前需按规范原文复核**，
> 本文不宣称通过任何 MCP/一致性认证（AGENTS §4）。

## 1. 问题与范围

- 目标：让 Agent（MCP client：IDE/Agent 框架等）用**标准工具调用**读取 EC-Sim 的观测面，
  而不是让每个 Agent 自己拼 HTTP/curl；同时把"只能看、不能写"作为桥接层的硬边界。
- 本班交付（可独立验证）：
  1. 本设计文档（架构决策、工具清单、错误映射、安全边界、实现计划）；
  2. 机器可读目录 `docs/plan/m3-w8-mcp-tools-v1.json`（9 个只读工具 + 排除项）；
  3. 静态一致性检查 `scripts/spikes/mcp-tools-check.py`（目录 ↔ `CONTROL-PLANE.md` 路由表逐项核对）。
- 非目标（本 stub 明确不做）：
  - **不实现** MCP server（下一实现项；见 §8 阶段 S1/S2）；
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
- **D4 零第三方依赖**：目标与主仓一致（默认构建无第三方依赖）；JSON 复用 `control-plane` 的
  手写助手（实现时确认 `pub` 可见性），HTTP 用 `std::net::TcpStream` 的最小请求即可（仅 http）。
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

## 8. 实现计划（后续班次，本班只做 stub）

- **S0（本班）**：本文 + 工具目录 + `mcp-tools-check.py` 静态校验；M3 清单/验证索引同步。✅
- **S1（建议下一实现项，预估 1–1.5 班）**：新增 `ec-sim-mcp` 二进制（放 `crates/cli` 或
  `crates/control-plane` 的模块 + bin）：JSON-RPC 2.0 stdio 最小实现（`initialize`、`ping`、
  `tools/list`、`tools/call`），HTTP GET 客户端用 `std::net`，零第三方依赖；只读 allowlist 与目录同源
  （可用 `include_str!` 或代码常量，目录文件作为单一事实源）。
- **S2（0.5 班）**：探针 `scripts/spikes/mcp-stdio-probe.sh`（python 合成 MCP client）：
  断言 `tools/list` 恰好是目录里的 9 个工具、全是 GET、无写工具、未知工具报错；
  对静态 `--serve` 跑 `/state`、`/dict`、`/sdo`；对 UDS 实时跑 `/topology`、`/pdo`、`/metrics`；
  再用 `--serve-write` 起服务确认工具表**不变**（写路径不可达）。
- **S3**：加入 `run-all-probes.sh`（Linux 项）与 VERIFICATION/STATUS 行；人工/CI 回归。

## 9. 验收（stub 本班）

- `python3 scripts/spikes/mcp-tools-check.py` → **OK**：9 个只读工具、文档全部 GET 路由有归宿、
  POST 只出现在排除项、设计文档与清单/验证索引互链。
- 无新增第三方依赖、无代码路径改动（本班是纯文档 + 静态检查）。
- 明确未做：MCP server 实现、stdio 探针、`run-all-probes.sh` 的 Linux 实跑（⚠️ 待验证，S2/S3）。

## 10. 复现

```bash
python3 scripts/spikes/mcp-tools-check.py
# 验证机（Linux）下一轮：
#   bash scripts/spikes/run-all-probes.sh   # 新增 mcp-tools 一项（静态检查，无需 root）
```
