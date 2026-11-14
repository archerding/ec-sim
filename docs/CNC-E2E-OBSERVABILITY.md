# CNC E2E 可观测性约定（EC-Sim 侧）

> task-103 交付物。定义 SIM 暴露给 CNC E2E harness/桥/HMI 的观测面、`run_id` 关联方式、
> 采集脚本与验收矩阵。只增不改：不替换既有 `/state` 字段，不改变 24h 判据。
>
> 口径：`run_id` 为 P1 可选；C3 验收以 **harness 侧关联**为权威，SIM 字段仅辅助。
> WKC 由内核/IgH 侧派生，SIM 的 `wkc_errors` 恒 0，见 §6。

## 1. 端点与字段

### 1.1 `GET /state`

在既有 state-v1 基础上追加（顶层）：

```json
{
  "run_id": "harness-run-uuid",
  "master": {
    "al_state": "0x08",
    "op_slaves": 3,
    "slave_count": 3,
    "fps": 49.9,
    "ack_latency_ms": null,
    "watchdog": {"trips": 0},
    "pdo": {"tx": 7, "rx": 11, "miss": 0, "wkc_errors": 0}
  },
  "slaves": [
    {"index": 0, "station_address": "0x0000", "al_code": "0x0000", "state": "0x08",
     "op": true, "frames_in": 975, "frames_out": 975, "frames_dropped": 0,
     "datagrams_hit": 975, "wkc_errors": 0}
  ]
}
```

- `al_state`：全从站 OP → `0x08`，否则 `0x00`（聚合值，不是 AL 寄存器原值）。
- `state`：逐站 AL Status 原字节（低 4 位状态 + bit4 error + bit5 ID）。
- `fps`：从站 0 `frames_in` 的差值与墙钟；合成主站下可能为 0（无持续帧时）。
- `ack_latency_ms`：当前 `null`（桥/HMI 侧测量）。
- `pdo.tx/rx`：SM2/SM3 配置长度（cnc-csp-servo：7/11）。
- `pdo.miss`：逐站 `frames_dropped` 求和；`pdo.wkc_errors` 恒 0（占位）。

### 1.2 `GET /metrics`

新增计数器（Prometheus 文本，`help/type/labels` 风格与既有指标一致）：

```
ecsim_al_transitions_total
ecsim_al_errors_total
ecsim_al_error_acks_total
ecsim_sm_watchdog_triggers_total
ecsim_sii_read_commands_total
ecsim_sii_write_commands_total
ecsim_wkc_errors_total        # 恒 0（占位）
```

既有 `ecsim_al_status`、`ecsim_al_status_code`、`ecsim_frames_in_total` 等保持不变。

### 1.3 `GET /events?since=<seq>`

```json
{"schema": "ec-sim/events-v1", "since": 0, "count": 1, "dropped": 0,
 "events": [{"seq": 1, "ts_ms": 179469..., "run_id": "...",
             "type": "fault_injected", "slave": null, "details": {...}}]}
```

- `seq` 单调递增；`details` 为 JSON 对象。
- 无 `since` 参数时仍为原有 SSE 流（`/events`）。
- 事件类型：`state_change`、`al_transition`、`watchdog`、`fault_injected`、
  `fault_cleared`、`link`、`pdo_loss`、`probe_state`。

### 1.4 `GET /fault/status`、`GET /probe`

```json
{"schema":"ec-sim/fault-v1","run_id":"...","active":[{"fault":"link_loss","target":null,
  "remaining_ms":400,"injected_at_ms":...,"applied_count":1}],"applied_count":1}
{"schema":"ec-sim/probe-v1","run_id":"...","probe":[{"target":2,"tripped":true}]}
```

## 2. `run_id` 关联

- 启动：`ec-sim-esc ... --run-id <id>`（1..128 字节、无空白）；缺省 `ecsim-<unix秒>-<pid>` 仅兜底。
- 出现位置（四处同值）：`/state.run_id`、每条 `/events` 的 `run_id`、`/fault` 请求校验与
  审计 `details.run_id`、`/probe` 响应。
- harness 在证据目录命名与 HMI/桥 JSON 中复用同一 `run_id`（UUID v4 由 harness 生成）。
- 不匹配：`/fault`、`/probe` 返回 409；不提供 `run_id` 视为兜底模式。

## 3. 采集脚本

`scripts/spikes/e2e_observability_collect.py`（只读 GET，不改状态）：

```bash
python3 scripts/spikes/e2e_observability_collect.py \
  --base http://127.0.0.1:18210 --run-id <run_id> \
  --seconds 60 --interval 1 \
  --out .scratch/observability/<run_id>
```

输出：`state.jsonl`、`metrics.jsonl`、`events.jsonl`、`summary.json`
（op 值集合、AL 码集合、watchdog/frames 增量、事件数、最后 seq）。

## 4. 验收矩阵（cnc-impl 用例域 → SIM 观测量）

| CNC E2E 用例域 | SIM 观测量 | 判定 |
| --- | --- | --- |
| 栈启动/3×CSP OP | `/state.master.op_slaves`、`slaves[].op`、`al_code` | `3/3`、`al_code=0x0000` |
| PDO 布局/方向 | `master.pdo.tx/rx`；`/pdo`（逐站原始字节，P1） | `7/11`；lcec HAL 侧另验 |
| 程序/循环/暂停 | 无直接信号（HMI/桥/LinuxCNC 侧） | SIM 仅给 run_id 关联 |
| AL 单站故障（P1） | `fault_injected` + `slaves[i].al_code` + `op=false` | 注入码、恢复后 `0x0000` |
| link_loss / TAP down/up | `fault_injected`/`link` down/up、`/state` 帧计数停滞；TAP 层由 `ip link` 验证 | 事件成对、恢复；down 期间 `/state` 仍 200 |
| slave_drop / 重连 | `fault_injected`、目标站 `frames_dropped` 增长、其他站继续收帧、到期 `fault_cleared` | 帧计数与事件同时成立 |
| sdo_reject | 目标站 SDO Upload/Download 返回 Abort `0x06010000`，到期恢复；`sdo-audit` 可关联 | 正常 `0x43` → abort → 恢复 `0x43` |
| pdo_loss | `pdo_loss` 事件、`frames_dropped` 增长 | 冻结/恢复 |
| watchdog | `sm_watchdog_triggers_total`、`watchdog` 事件、`al_code=0x001B` | 触发/ACK |
| STALE/LOST | 无直接信号（桥/GUI 侧）；`/metrics` 帧率可辅助 | HMI 证据 |
| 探针 tripped | `/probe`、`probe_state` 事件 | 置位/清除成对 |

## 5. 与 C5 24h 的关系

C5 的硬判据仍是 `ethercat slaves` 全 OP + 采样；SIM 的 `/state`、`/metrics`、`/events`
是**补充证据**（C5 下轮 dump 已要求抓这三者与逐站 AL 码）。SIM 不改变 C5 PASS/FAIL 判据。

## 6. 边界与易误读点

- `--al-error` 是**启动期全站**注入；运行期单站注入是 P1（需 `--serve-write`+`confirm`+审计）。
- `/topology` 的 `sm2_hex`/`sm3_hex` 是 SM2/SM3 原始字节，不是 WKC。
- `wkc_errors` 与 `ecsim_wkc_errors_total` 恒 0；WKC 由内核/IgH 侧派生，SIM 不声称提供。
- `slave_drop` 是“静默 + 环网旁路”：帧继续下传，目标站 `frames_in` 仍增长、
  `frames_dropped` 增长；判定看 `frames_dropped`，不要期望 `frames_in` 停滞。
- DC 丢失为 P1/NOT-JUDGED（无硬件 DC，本轮不做 DC 丢失注入）。
- `ack_latency_ms` 当前 `null`；`fps` 依赖主站帧，合成主站空闲时可能为 0。
- 8 IO 混合拓扑 P1/NOT-JUDGED（`--slaves` 复制同一 manifest）。
- 不宣称 EtherCAT 一致性/CTT/LinuxCNC 认证。
