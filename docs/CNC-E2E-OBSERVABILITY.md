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
| program（程序/轨迹） | TxPDO `position` 逐点跟随 [100,200,300,400,500]；`statusword` Target reached(bit10) | 每点到位、位 10 置位（`c4-matrix/program`） |
| jog（增量点动） | `position` +100×5 | 每次增量 100（`c4-matrix/jog`） |
| MDI（单条绝对移动） | `position` → 5000，Target reached | 到位 + bit10（`c4-matrix/mdi`） |
| home（回零） | CiA402 HM：mode 6 + 控制字 bit4 → `position=0`、`statusword` Homing attained(bit12)、mode=6 | 归零 + bit12（`c4-matrix/home`；SIM Tier0 不模拟寻零开关/轨迹） |
| override（进给倍率） | 目标增量速率 100% vs 50%：+200×10 vs +100×10 | 增量 2000/1000、比率 0.4–0.6（`c4-matrix/override`） |
| tool（参数装载） | SDO 写/读 `0x60FF=750`、`/sdo-audit` 200 | 写成功、读回 750、审计可关联（`c4-matrix/tool`） |
| offset（坐标偏置） | 目标 1000→3500(+2500)→3000(-500)，`position` 同步平移 | 位置随目标同步（`c4-matrix/offset`） |
| AL 单站故障（P1） | `fault_injected` + `slaves[i].al_code` + `state` error bit | 注入码、恢复后 `0x0000` |
| link_loss / TAP down/up | `fault_injected`/`link` down/up、`/state` 帧计数停滞；TAP 层由 `ip link` 验证 | 事件成对、恢复；down 期间 `/state` 仍 200 |
| slave_drop / 重连 | `fault_injected`、目标站 `frames_dropped` 增长、其他站继续收帧、到期 `fault_cleared` | 帧计数与事件同时成立 |
| sdo_reject | 目标站 SDO Upload/Download 返回 Abort `0x06010000`，到期恢复；`sdo-audit` 可关联 | 正常 `0x43` → abort → 恢复 `0x43` |
| pdo_corrupt（F-PROC 过程数据） | `fault_injected` + `audit.pdo.before_hex/xor`；注入期间 TxPDO 逐字节 XOR `0xA5`（确定、不累积）；到期恢复 | 审计前值、字节变化、恢复原值 |
| 进程 kill/重启（F-PROC 进程） | run_id 变化；kill 后 `/state` 不可达；重启后 3/3 OP、`HOLD_OK` | 控制面不可达 → 3/3 OP 恢复 |
| pdo_loss | `pdo_loss` 事件、`frames_dropped` 增长 | 冻结/恢复 |
| watchdog | `sm_watchdog_triggers_total`、`watchdog` 事件、`al_code=0x001B` | 触发/ACK |
| STALE/LOST | 无直接信号（桥/GUI 侧）；`/metrics` 帧率可辅助 | HMI 证据 |
| 探针 tripped | `/probe`、`probe_state` 事件 | 置位/清除成对 |

覆盖说明：`c4_matrix_probe.py` 的 10 类为 **SIM 可观测等价路径**（合成主站 + PDO/状态 + SDO/事件），
可执行证据在 `docs/verification/assets/cnc-e2e/c4-matrix/<class>/`，汇总在
`c4-matrix-summary.json`；CNC 侧 HMI/桥的真实操作证据仍由 CNC E2E harness 提供（PENDING）。
`c4-matrix` 已注册 B-1（`scripts/spikes/cnc-e2e-c4-matrix.sh`），在本轮 EC-Sim 验证机上 10/10 通过。

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
- 故障**到期清理由数据面帧推进**：无帧流量时 `tick` 不前进，`/fault/status` 可能
  `remaining_ms=0` 但仍 active；需要 keepalive/主站帧才会清 `fault_cleared`。
- `ack_latency_ms` 当前 `null`；`fps` 依赖主站帧，合成主站空闲时可能为 0。
- 8 IO 混合拓扑 P1/NOT-JUDGED（`--slaves` 复制同一 manifest）。
- 不宣称 EtherCAT 一致性/CTT/LinuxCNC 认证。

## 7. 缺口补齐（2026-11-15，lead m04142）

### 7.1 C1 抖动来源与量化

来源分三层，均由 SIM 侧可观测面测量：

1. **主站发送间隔**（合成 UDS master）：Python 睡眠粒度 + 主机调度 + socket 唤醒；
2. **采样/遥测**：`GET /state` 往返（serve 线程调度 + JSON 序列化）；
3. **帧率**：`/state.master.fps` 是 serve 线程两次调用间的粗估（可能瞬时为 0 或跳变，
   不作为抖动依据）；本探针另给 `frame_rate`（窗口内 `frames_in` 增量/墙钟，逐站/总数）。

测量（EC-Sim 验证机，`c1_jitter_probe.py`，300 周期，目标 20ms）：

| 指标 | mean | std | p95 | p99 | max |
| --- | --- | --- | --- | --- | --- |
| master 发送间隔（ms） | 20.424 | 0.149 | 20.636 | 20.714 | 21.052 |
| `GET /state` 延迟（ms） | 0.546 | 0.082 | 0.696 | 0.709 | 0.803 |

帧率：900 帧/6.13s = 146.88 fps（3 站合计）= **48.96 fps/站**；WKC 恒 3（3 站）。
WKC 来源：SIM 帧回复（每帧 3 站）；**内核/IgH 侧 WKC 不由 SIM 提供或声称**。
证据：`docs/verification/assets/cnc-e2e/jitter/jitter.json`。

### 7.2 C3/A7 按 run 归档

`scripts/spikes/e2e_run_archive.py`：

- `create --out-root <root> --run-id <id> --meta k=v --artifact <path>...` →
  `<root>/runs/<run_id>/`，含 `run.json`（schema `ec-sim/e2e-run-v1`、result/command/
  commit/binary_sha256/manifest_sha256/meta/files[{file,bytes,sha256}]）、`README.md`、
  `sha256sums.txt`；
- `verify --dir <root>/runs/<id>` 重算全部 sha256 并检查未登记/缺文件；退出码 0/1。

本轮 `c4-matrix`、`reset-idempotency`、`jitter` 均自动归档并 `verify` 通过；
`run_id` 与 `/state`、`/events`、`/fault` 审计同值（harness 传入为权威）。

### 7.3 C4 10 类矩阵

`scripts/spikes/c4_matrix_probe.py` + `cnc-e2e-c4-matrix.sh`：program/jog/MDI/home/
override/tool/offset/probe/alarm/link-loss 各写
`<class>/{stimulus,samples,state-before,state-after,events.jsonl,result.json}`，
汇总 `c4-matrix-summary.json`；本轮 **10/10 通过**（见 §4 各行证据路径）。

### 7.4 A3 REF schema 与字段映射

`docs/cnc-e2e/REF.schema.json`（`ec-sim/cnc-e2e-ref-v1`）+ `REF.example.json`：
SIM 侧字段已在 schema 固化；CNC 侧 `ref_state` 与 `cnc-station` 的
`docs/product/assets/cnc-hmi-g1/demo-state.json` 字段映射（program/dro/tool/wcs/
overrides/machine/estop）已写入 `field_map`。`REF.json` 仍 **PENDING**（等 cnc-impl
实测值；届时替换 `cnc.values`、`status=CAPTURED`，schema 不变）。

### 7.5 A4 reset 幂等

`cnc-e2e-reset-idempotency.sh` + `e2e_reset_probe.py`：3 次独立启动→3/3 OP→停止，
reset/teardown 结果：

| 轮次 | 到 REF 用时 | 停止后残留 | REF 字段 |
| --- | --- | --- | --- |
| 1 | 43 ms | 无进程/socket/端口 | 一致 |
| 2 | 42 ms | 无 | 一致 |
| 3 | 42 ms | 无 | 一致 |

3 次均 ≤30s 上限；`ref-compare.json` 显示 6 个 REF 字段级一致（`equal=true`）。
证据：`docs/verification/assets/cnc-e2e/reset-idempotency/`。
