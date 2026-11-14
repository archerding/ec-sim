# task-102 故障矩阵独立复跑证据（2026-11-15）

审计者：行业专家（sim-expert，独立于 sim-impl）。
环境：**EC-Sim 自己的验证机**（openEuler 24.03 SP4；未触碰 CNC 验证机，C5 未受干扰）。
被测提交：EC-Sim `30ce3e9`（`feat(e2e): fault matrix P0`）。

## 环境与身份

| 项 | 值 |
|---|---|
| HEAD | `30ce3e9` |
| 二进制 | `ec-sim-esc` sha256 `912564b43977a9038e865140b35c044c2324f2c8e23db541a58d8bf63c665852`（1,153,736 B） |
| manifest | `plugins/cnc-csp-servo/plugin.yaml` sha256 `5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9` |
| 现场目录 | `/root/ecsim-faults`（4 个脚本与本地 `30ce3e9` 逐字节一致） |
| 时间（UTC+8） | 2026-11-15 07:31–07:33 |

脚本一致性：`cnc-e2e-fault-matrix.sh` `0053b482…`、`cnc-e2e-tap-fault.sh` `9e417f9f…`、
`fault_matrix_probe.py` `8321137a…`、`run-all-probes.sh` `85f2faef…`（本地独立复算，与 VM 现场一致）。

## 复跑结果

### 1. UDS 故障矩阵（`fault-matrix/`）

```bash
cd /root/ecsim-faults
EC_SIM_BIN=target/release/ec-sim-esc OUT_DIR=/tmp/audit-fm \
  RUN_ID=audit-fm-20261115073119 bash scripts/spikes/cnc-e2e-fault-matrix.sh
```

- `EXIT=0`、`RESULT: PASS（0 个失败）`；15 项断言全部 OK（3/3 OP、slave_drop frames_dropped 增长、
  其他站继续收帧、fault_injected/fault_cleared、SDO Abort `0x06010000`、恢复后 0x43、结束 3/3 OP）。
- 原始：`fault-matrix/run.log`、`probe.out`、`timeline.log`、`slave-drop-timeline.json`、
  `sdo-reject-timeline.json`、`server.log`。

### 2. TAP down/up（`tap-fault/`）

```bash
cd /root/ecsim-faults
EC_SIM_BIN=target/release/ec-sim-esc OUT_DIR=/tmp/audit-tap \
  RUN_ID=audit-tap-20261115073204 TAP=ecsimfault0 PORT=18135 \
  bash scripts/spikes/cnc-e2e-tap-fault.sh
```

- `EXIT=0`、`RESULT: PASS`：down 前三站 `frames_in=487/486/486`；down 后两次采样
  `492/491/491` 不变、控制面仍 200；恢复后 `frames_in` 由 `1003/996/996` 增至
  `1094/1087/1087`；主站 #2 `all slaves are now operational`、3/3 OP。
- 原始：`tap-fault/run.log`、`timeline.log`、`ip-link-{down,up}.txt`、`state-*.json`、
  `master1.log`、`master1-fault-lines.log`、`master2.log`、`server.log`。

### 3. B-1 整轮（`b1/`）

```bash
cd /root/ecsim-faults
BIN=target/release/ec-sim-esc PY=python3 LOG_DIR=/tmp/audit-b1 \
  bash scripts/spikes/run-all-probes.sh
```

- `== 汇总：28 PASS / 0 FAIL / 0 SKIP（共 28）==`、`EXIT=0`。
- 关键项：`fault-matrix` PASS、`tap-fault` PASS、`cnc-e2e-fixed-scene` PASS、`e2e-control` PASS。
- 原始：`b1/run.log` 及关键探针日志。

### 4. `/fault` 门禁与审计（`gating.md`、`g1/g2/g3.out`）

- 无 `--serve-write` → **405**；有 `--serve-write` 缺 `confirm` → **400**；
  `confirm:true` → **200**；`/events` 的 `fault_injected` details 含
  `confirm:true/who:http/run_id/before/after`；`/fault/status` 可读。

### 5. 清理

复跑后检查：无 `ec-sim-esc`/`simple_ng` 残留进程、无 1813x 监听、无 `ecsim*` TAP；未触发 reboot。

## 边界（不构成 PASS 的部分）

- **F-PROC（进程 kill/重启）**：sim-impl 交付物未包含该 P0 类用例/证据；本次未替代判定。
- **CNC 侧 HMI 联动**（STALE/LOST/ALARM 显示、运动命令禁用）：需 CNC VM 联调窗口，PENDING。
- 8 IO 混合拓扑、DC 丢失、AL 运行期单站注入：P1/NOT-JUDGED（不作为本轮 PASS）。
- 本证据只证明 EC-Sim 侧故障触发/恢复语义；不称 EtherCAT 一致性/CTT/LinuxCNC 认证。
