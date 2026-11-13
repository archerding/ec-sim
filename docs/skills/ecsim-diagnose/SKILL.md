---
name: ecsim-diagnose
description: 只读诊断 EC-Sim 虚拟 EtherCAT 段：检查服务健康、从站身份/AL、拓扑、PDO、SDO 与指标，并输出可归档摘要。
version: 0.1
mode: read-only
tools:
  - ecsim_health
  - ecsim_state
  - ecsim_dict
  - ecsim_topology
  - ecsim_pdo
  - ecsim_pdo_history
  - ecsim_metrics
  - ecsim_sdo_read
  - ecsim_sdo_audit
---

# ecsim-diagnose

用于回答："EC-Sim 现在是否在跑？总线有几个从站、谁不在 OP？PDO/SDO 值是什么？指标有没有异常？"
本技能**只读**，不发起任何写操作（`POST /sdo` / `POST /pdo` 不在工具表中）。

## 前置

- 已启动 `ec-sim-esc --serve <addr>`（静态模式）或 `--serve` + `--uds/--tap`（实时模式）；
- 已启动 `ec-sim-mcp --api http://127.0.0.1:<port>`（默认只允许回环）；
- token 模式需 `--token-env VAR`（技能不保存 token）。

## 诊断步骤

1. **健康**：`ecsim_health` → 非 200 或连接失败 → 先排查服务/端口（不要继续猜测数据）。
2. **版本/模式**：`ecsim_state`（`slave` 可选，默认 0）→ 看 `schema`、`identity`、`profile`；
   实时模式含 `live.al_status`/`live.stats`；**无 `live` 即静态模式**（拓扑/PDO/指标/审计不可用）。
3. **总线拓扑**（实时）：`ecsim_topology` → 站数、每站 `al_status`/`al_status_code`；非 OP 的站是下一步目标。
4. **单站状态**：`ecsim_state {slave:N}` → 身份四元组、SM、PDO 映射、DC 声明；与 manifest/SII 期望比对。
5. **PDO**（实时）：`ecsim_pdo {slave:N}` → SM2/SM3 原始 hex + `decoded` 具名值；
   需要趋势时 `ecsim_pdo_history {slave:N}`（变化去重，最多 256 条/站）。
6. **对象/参数**：`ecsim_dict` 查对象；`ecsim_sdo_read {index:"0x6040", sub:0}` 读运行时值
   （**不经过** CoE mailbox 往返，属运行时字典视图）。
7. **指标/审计**（实时）：`ecsim_metrics`（Prometheus 文本，每站标签）、`ecsim_sdo_audit`（最近 64 条写审计，只读观测）。
8. **输出摘要**：按下方模板归档；结论必须带"观测时刻 + 模式（静态/实时）+ 数据来源"。

## 成功判据

- 能回答：服务是否健康、站数、每站 AL/AL code、PDO 字节与解码值、关键 SDO 值、指标是否单调异常；
- 结论引用具体工具返回（字段/计数），不臆造未返回的数据；
- 未覆盖项（如 DC 同步精度、停帧墙钟超时、物理层）明确写"仿真不判，需真机/HIL"。

## 错误码（工具错误 `code`）

| code | 含义 | 下一步 |
|---|---|---|
| `AUTH_REQUIRED` | 控制面开启了 token，未带 Bearer | 用 `--token-env` 注入环境变量；或经反向代理 |
| `FORBIDDEN` | token 错误（或写路径被拒，本技能不应出现） | 检查 token 值；确认只读工具 |
| `NOT_FOUND` | 对象/子索引/从站不存在；静态模式无 manifest 时 `/dict` 404 | 收窄 `slave`/`index`；确认 manifest |
| `MODE_UNAVAILABLE` | 该工具需要实时模式（`--serve` 与 UDS/TAP 同进程） | 以实时模式重启控制面；或只用静态可用工具 |
| `BAD_ARGUMENT` | 参数不合法（index 格式/slave 越界/sub 范围） | 按工具 schema 修正参数 |
| `TIMEOUT` / `UPSTREAM_ERROR` | 上游超时/不可达 | 检查服务进程与端口；重试一次 |
| `RESPONSE_TOO_LARGE` | 响应 >1 MiB | 用 `slave` 参数收窄范围 |

## 边界（必须随结论声明）

- **只读快照**：`/state.live`、`/pdo`、`/metrics` 是读取时刻近似值，**不构成**实时性/一致性保证；
- **不宣称**一致性测试/CTT/认证；"仿真通过 ≠ 设备可用"；
- DC 同步精度、停帧墙钟超时、物理层/拓扑错误必须真机/HIL（见 `docs/plan/M3-exit-checklist.md` §7）；
- 不发起写操作；如需写 SDO，转人工确认流程（控制面 `--serve-write` + 白名单 + 审计）。

## 示例

- 静态模式请求序列：[`examples/static-session.jsonl`](examples/static-session.jsonl)（health/state/dict/sdo_read）
- 实时模式请求序列：[`examples/live-session.jsonl`](examples/live-session.jsonl)（topology/pdo/history/metrics/sdo_audit）

```bash
# 用法（两个终端；示例见 examples/）
target/release/ec-sim-mcp --api http://127.0.0.1:8091 < examples/static-session.jsonl
```

## 归档摘要模板

```text
[EC-Sim 诊断摘要]
时间：<ISO8601>    模式：STATIC | LIVE(SSE/POLL)    工具版本：<ec-sim-mcp --version>
站数：N    OP：N-1    异常站：<index: AL 0xXX / code 0xXXXX>
关键对象：0x6040=0x....  0x6041=0x....
PDO SM2：<hex>   SM3：<hex>
指标：frames_in=… frames_dropped=… datagrams_hit=…
未覆盖/需真机：<DC 同步 / 停帧超时 / 物理层>
边界：只读快照，不构成实时/一致性/认证结论。
```
