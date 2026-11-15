# CNC E2E 缺口补齐（C1/C3/C4/A3/A4/A7）

- 日期：2026-11-15
- 触发：lead m04142（sim-expert 复核 commit `9ca34c2` 判 task-101 🟡、task-103 🟡，
  C4 仅 4/10 类、C1/C3/A3/A4/A7 PENDING）
- 环境：EC-Sim 自己的验证机（openEuler 24.03 SP4，root；地址只在 `.local/ops.md`），
  单占锁 `vm-ecsim.lock`；**未触碰 CNC 验证机**。
- 二进制：`ec-sim-esc` sha256 `66587fa0094a94190eb82fb7ec42bd7377a3a0ab61369fc191c1ce7ba2f30c1f`
  （1,157,928 B）；manifest `plugins/cnc-csp-servo/plugin.yaml` sha256
  `5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`。
- B-1：**32 PASS / 0 FAIL / 0 SKIP（共 32）**，日志
  [assets/cnc-e2e/b1-32.log](assets/cnc-e2e/b1-32.log)。

## 1. C4：10 类 SIM 可观测等价路径

脚本：`scripts/spikes/c4_matrix_probe.py` + `scripts/spikes/cnc-e2e-c4-matrix.sh`。

| 类 | 等价路径（SIM 侧） | 断言 | 证据 |
| --- | --- | --- | --- |
| program | 目标轨迹点 [100,200,300,400,500]，每 80ms | TxPDO position 逐点跟随 + Target reached(bit10) | `assets/cnc-e2e/c4-matrix/program/` |
| jog | 目标 +100×5 | 每次增量 100 | `.../c4-matrix/jog/` |
| MDI | 单目标 5000 | 到位 + bit10 | `.../c4-matrix/mdi/` |
| home | CiA402 HM：mode 6 + cw bit4=1（`0x001F`） | position=0 + Homing attained(bit12) + mode=6 | `.../c4-matrix/home/` |
| override | +200×10 vs +100×10 | 增量 2000/1000、比率 0.4–0.6 | `.../c4-matrix/override/` |
| tool | SDO 写 0x60FF=750 → 读回；`/sdo-audit` | 写/读一致、审计 200 | `.../c4-matrix/tool/` |
| offset | 1000→3500(+2500)→3000(-500) | 实际位置同步平移 | `.../c4-matrix/offset/` |
| probe | `POST /probe` tripped=true→false | 200、读回、`probe_state` 事件成对 | `.../c4-matrix/probe/` |
| alarm | `POST /fault al_error target=0 900ms` | `al_code=0x0011`、`state` error bit、到期恢复 `0x0000`、事件 | `.../c4-matrix/alarm/` |
| link-loss | `POST /fault link_loss 600ms` | 期间 frames 停更、到期清理、恢复增长、link down/up 事件 | `.../c4-matrix/link-loss/` |

结果：**10/10 通过**（`c4-matrix-summary.json`；每类 `events.jsonl`/`result.json`）。
说明：这是 SIM 侧可观测等价路径，不是 CNC HMI/桥/LinuxCNC 的真实操作证据；
CNC 侧 D1–D3 证据仍 PENDING。

## 2. C1：抖动来源与量化

脚本：`scripts/spikes/c1_jitter_probe.py` + `cnc-e2e-jitter-probe.sh`（200 样本，目标 20ms）。

| 指标 | mean | std | p95 | p99 | max |
| --- | --- | --- | --- | --- | --- |
| 主站发送间隔（ms） | 20.4192 | 0.1453 | 20.6501 | 20.7006 | 20.8309 |
| `GET /state` 往返（ms） | 0.54 | 0.0646 | 0.6423 | 0.7219 | 0.7219 |

- 帧率：600 帧 / 4.084 s = 146.91 fps（3 站合计）= 48.97 fps/站；WKC 恒 3。
- 来源分层：①合成主站 Python sleep + 主机调度 + socket 唤醒；②serve 线程调度 +
  JSON 序列化；③`/state.master.fps` 是粗估（两次 serve 调用间帧差，可能瞬时 0/跳变），
  不作为抖动依据。
- WKC 来源：SIM 帧回复；**内核/IgH 侧 WKC 不由 SIM 提供或声称**。
- 证据：`assets/cnc-e2e/jitter/jitter.json`。

## 3. C3/A7：按 run 归档与校验

工具：`scripts/spikes/e2e_run_archive.py`。

```bash
python3 scripts/spikes/e2e_run_archive.py create \
  --out-root docs/verification/assets/cnc-e2e --run-id <run_id> \
  --meta result=PASS --meta commit=<sha> --meta binary_sha256=<sha> \
  --meta manifest_sha256=<sha> --artifact <evidence-dir>
python3 scripts/spikes/e2e_run_archive.py verify \
  --dir docs/verification/assets/cnc-e2e/runs/<run_id>
```

本轮 c4-matrix / reset-idempotency / jitter 均自动归档并校验通过
（`runs/<run_id>/{run.json,README.md,sha256sums.txt}`，`run.json` 含
result/command/commit/binary/manifest/files+sha256；目录型 artifact 保留子目录结构，
如 `runs/c4-matrix-*/c4-matrix/program/...`）；run_id 与 `/state.run_id`、`/events`、
`/fault` 审计同值。归档里的 `commit` 记产品代码提交 `7a919cf`（本轮 gap 脚本/文档
不改变二进制，二进制 sha256 与 F-PROC 版一致），gap 脚本/文档本身随后单独提交。

## 4. A3：REF schema 与 CNC demo-state 字段映射

- `docs/cnc-e2e/REF.schema.json`（schema `ec-sim/cnc-e2e-ref-v1`）+
  `docs/cnc-e2e/REF.example.json`。
- CNC 侧 `ref_state` 字段（program/dro/tool/wcs/overrides/machine/estop）已映射到
  `cnc-station` 的 `docs/product/assets/cnc-hmi-g1/demo-state.json`；`cnc.status=PENDING`。
- `REF.json` 仍 **PENDING**（等 cnc-impl 实测值；届时只替换 `cnc.values`、
  `status=CAPTURED`，schema 不变）。

## 5. A4：reset/stack-down 幂等

脚本：`scripts/spikes/cnc-e2e-reset-idempotency.sh` + `e2e_reset_probe.py`。

| 轮次 | 启动→3/3 OP | 停止后残留 | REF |
| --- | --- | --- | --- |
| r1 | 43 ms | 无进程/socket/端口 | 一致 |
| r2 | 42 ms | 无 | 一致 |
| r3 | 42 ms | 无 | 一致 |

- 上限 30s，3/3 轮远低于上限；`ref-compare.json` 的 `equal=true`（6 字段级一致：
  slave_count/op_slaves/al_state/pdo/watchdog_trips/slaves）。
- 停止流程：kill 进程 → 清理运行期 UDS socket → 断言无进程/socket/端口。
- CNC 侧 stack-down（IgH/lcec/LinuxCNC）不在本脚本内，仍待 CNC 窗口。
- 证据：`assets/cnc-e2e/reset-idempotency/`、`runs/reset-idem-*-r{1,2,3}/`。

## 6. 边界

- 部署口径：验证机源码应以某个 commit 的 `git archive` 为准并记录 commit；本轮现场先用了
  `2bb8fa1` 树，归档工具其后在 `f016506`/`33bf2fb` 修正（保留子目录、字节稳定），
  仓内 runs 证据已用修正版工具重新生成并校验；后续复跑按 commit 部署，避免脚本/证据漂移。

- 上述均为 EC-Sim 验证机自证（UDS 合成主站/TAP），不代表 CNC VM 上 lcec/HMI 行为；
  CNC VM 未触碰（G1 截图占用中），联调窗口待 G1 判定。
- C4 的 program/jog/MDI/home 等是“SIM 可观测等价路径”，不是 CNC 程序解释器验证。
- `home` 为 Tier0 HM 语义（mode 6 + bit4 归零 + Homing attained），不模拟寻零开关/轨迹。
- WKC、`ack_latency_ms`、到期清理依赖数据面帧等边界不变（见
  [CNC-E2E-OBSERVABILITY.md](../CNC-E2E-OBSERVABILITY.md) §6）。
- 8 路 IO 混合拓扑、DC 丢失仍 P1/NOT-JUDGED。
- 不宣称 EtherCAT 一致性/CTT/LinuxCNC 认证；未验证项不写 PASS。
