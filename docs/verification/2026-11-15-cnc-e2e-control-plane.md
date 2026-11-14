# CNC E2E 控制面与固定场景验收（EC-Sim）

- 日期：2026-11-15
- 范围：task-101（固定场景）、task-102（故障注入）、task-103（可观测性）的 EC-Sim 侧实现与验证
- 结论：**实现完成、EC-Sim 验证机证据齐备**；8 IO 混合拓扑与 `REF.json` 仍 PENDING；不构成一致性/认证结论

## 1. 变更

| 文件 | 内容 |
| --- | --- |
| `crates/esc-node/src/lib.rs` | `Stats.wkc_errors`（占位）；`Responder` 增加 `muted`/`pdo_frozen`/`probe_tripped` 及 setter/getter、`force_watchdog()`、`clear_al_error()`；`tick()` 在 `pdo_frozen` 时冻结行为；`handle_frame()` 在 muted 时计 `frames_dropped` 并丢弃 |
| `crates/control-plane/src/lib.rs` | `e2e_state_json`（顶层 `run_id`/`master`/`slaves`）、`event_json`/`events_json`（`/events?since`）、`FaultView`/`fault_status_json`；`MetricsSlave.wkc_errors`、`ecsim_wkc_errors_total`；+3 单测 |
| `crates/esc-node/src/bin/ec_sim_esc.rs` | `E2eRuntime`（事件环 4096、faults、probe）、`log_al_events`、`handle_e2e_control`（`/fault`、`/fault/status`、`/probe`）、`--run-id`、`FpsTracker`、`/state`/`/events` 接线、UDS/TAP 帧循环 fault 到期与 link_loss 丢帧 |
| `scripts/spikes/uds_op_hold.py` | UDS 广播写 SM0/SM1、配置 SM2/SM3、PREOP→SAFEOP→OP、20 ms 周期 SM2、5 s 读 `/state` 自检 |
| `scripts/spikes/cnc-e2e-fixed-scene.sh` | 3×`cnc-csp-servo` 固定场景；每轮独立 run_id/进程/证据目录；`RESULT: PASS` |
| `scripts/spikes/e2e_control_probe.py` / `e2e-control-probe.sh` | 控制面契约探针（门禁/幂等/事件/到期恢复/405） |
| `scripts/spikes/e2e_observability_collect.py` | task-103 只读采集（state/metrics/events + summary） |
| `scripts/spikes/run-all-probes.sh` | 注册 `e2e-control`、`cnc-e2e-fixed-scene`（B-1 26 项） |
| `docs/CONTROL-PLANE.md` §10、`docs/CNC-E2E-PROFILE.md`、`docs/CNC-E2E-OBSERVABILITY.md`、`docs/cnc-e2e/cnc-e2e-3csp.xml` | 端点/字段/场景/矩阵/lcec 样例 |

## 2. 本机验证

```
cargo fmt --all -- --check        # 0
cargo clippy --workspace --all-targets -- -D warnings   # 0
cargo test --workspace            # 268 passed / 0 failed（control-plane 25）
cargo check --target x86_64-unknown-linux-gnu -p esc-node --bins --features wasm-runtime  # 0
bash -n scripts/spikes/{cnc-e2e-fixed-scene,e2e-control-probe}.sh  # 0
python -m py_compile scripts/spikes/{uds_op_hold,e2e_control_probe,e2e_observability_collect}.py  # 0
```

## 3. EC-Sim 验证机

环境：EC-Sim 自有验证机（openEuler 24.03 SP4），源码 `/root/ecsim-e2e`（`git archive` 的未提交工作树），
`cargo build --release -p esc-node --offline` 成功。

- 二进制 sha256：`d2241dec1cc6fff839ccee781cd5768ad1099d0ea8fdb83e7e957607f21904db`
- manifest sha256：`5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`

### 3.1 控制面契约探针（`e2e-control-probe.sh`）

`RESULT: PASS`，覆盖：

- `/state` 200、`run_id`/`master`/3 个 `slaves` 字段齐全；
- 缺 `confirm`→400、run_id 不匹配→409、未知 fault→400、target 越界→400；
- `link_loss` 注入 200；重复注入幂等；`/fault/status` active/remaining_ms/run_id 一致；
- 事件含 `fault_injected`（审计 `who=http`、before/after、confirm、run_id）与 `link` down；
- `seq` 严格单调、`/events?since=` 只返回更大 seq；到期 `fault_cleared`+`link` up；
- `al_error` target=1 → `slaves[1].al_code="0x0011"`、`op=false`、state error 位；恢复后 `0x0000`；
- `/probe` 置位/清除 + `probe_state` 事件；
- 无 `--serve-write` 实例 POST `/fault`→405。

证据：`docs/verification/assets/cnc-e2e/e2e-control-probe.out`。

### 3.2 固定场景（`cnc-e2e-fixed-scene.sh --rounds 3 --hold-seconds 20`）

`RESULT: PASS（3/3 轮 3/3 OP）`。每轮 20 s 保持，采样约 970 次；`/state` 终态：

- `run_id` 与启动参数一致；`master.op_slaves=3`、`pdo.tx=7`、`rx=11`、`watchdog.trips=0`；
- 逐站 `al_code="0x0000"`、`state="0x08"`、`op=true`、`frames_in=975`。

证据目录：`docs/verification/assets/cnc-e2e/fixed-scene/`（`summary.txt` + `round-1..3/{hold.out,server.log,state.json}`）。

### 3.3 B-1 整轮

`LOG_DIR=/tmp/e2e-b1b bash scripts/spikes/run-all-probes.sh` → **26 PASS / 0 FAIL / 0 SKIP（共 26）**，
`RESULT: PASS`；新增两项 `e2e-control`、`cnc-e2e-fixed-scene` 均 PASS。日志：`docs/verification/assets/cnc-e2e/b1-26.log`。

## 4. 复现命令

```bash
# 固定场景（EC-Sim 验证机，UDS 合成主站；不依赖 CNC VM）
EC_SIM_BIN=target/release/ec-sim-esc \
  bash scripts/spikes/cnc-e2e-fixed-scene.sh --rounds 3 --hold-seconds 20

# 控制面契约
EC_SIM_BIN=target/release/ec-sim-esc bash scripts/spikes/e2e-control-probe.sh

# 整轮 B-1
LOG_DIR=/tmp/e2e-b1b bash scripts/spikes/run-all-probes.sh
```

CNC VM 侧（TAP）起停与 lcec 配置见 `docs/CNC-E2E-PROFILE.md` §3、`docs/cnc-e2e/cnc-e2e-3csp.xml`；
TAP 场景需 `--tap tapcnc0 --slaves 3 --manifest plugins/cnc-csp-servo/plugin.yaml --behavior cia402 --serve 127.0.0.1:18210`。

## 5. 边界（不得越过）

- `run_id` 为 P1：SIM 字段只增不改；C3 验收以 harness 侧关联为权威，缺省 `ecsim-<秒>-<pid>` 仅兜底。
- `--al-error` 是**启动期全站**注入；运行期单站注入（`POST /fault` al_error）为 P1，且仅
  `--serve-write`+每请求 `confirm:true`+审计可用。
- `/topology` 的 `sm2_hex`/`sm3_hex` 是 SM2/SM3 原始字节，不是 WKC；`wkc_errors` 恒 0 占位。
- `ack_latency_ms` 当前 `null`；`fps` 取站 0 帧增量，合成主站空闲时可为 0。
- 8 IO 混合拓扑 P1/NOT-JUDGED（现有 `--slaves` 复制同一 manifest）；`REF.json` 待 cnc-impl demo-state 定稿后生成并对齐。
- 未在 CNC VM 本轮运行（C5 提前结束、G1 截图占用）；不宣称 EtherCAT 一致性/CTT/LinuxCNC 认证。
