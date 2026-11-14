# CNC E2E 故障矩阵验证记录（task-102，EC-Sim 侧）

> 日期：2026-11-15（团队班）。范围：P0 故障的 **触发 → 恢复 → `/state`/`/events` 时间线**。
> 环境：EC-Sim 自己的验证机（openEuler 24.03 SP4，Linux）。**未触碰 CNC 验证机**。
> 本文只给用例映射/预回归证据，**不构成**一致性测试或认证结论。

## 1. 交付物

| 项 | 内容 |
| --- | --- |
| 产品改动 | `crates/esc-node/src/lib.rs`：`Responder.sdo_reject` + Abort 注入；静默从站（muted）改环网旁路语义；`crates/esc-node/src/bin/ec_sim_esc.rs`：`/fault` 增 `sdo_reject` |
| 探针 | `scripts/spikes/fault_matrix_probe.py`、`cnc-e2e-fault-matrix.sh`、`cnc-e2e-tap-fault.sh`、`uds_keepalive.py`；B-1 注册表 **28 项** |
| 证据目录 | `docs/verification/assets/cnc-e2e/fault-matrix/`、`docs/verification/assets/cnc-e2e/tap-fault/`、`b1-28.log` |
| 二进制 | sha256 `912564b43977a9038e865140b35c044c2324f2c8e23db541a58d8bf63c665852`（1,153,736 B） |
| 设备画像 | `plugins/cnc-csp-servo/plugin.yaml`，sha256 `5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9` |

## 2. 语义修复（本轮发现）

首轮 `slave_drop` 注入后，**所有**从站都不再收帧：`handle_frame_chain` 用
`?` 把 muted 从站的 `None` 当作整链失败。这不符合静默从站/环网旁路的物理语义。

修复：muted 从站本帧不应答、不置 WKC，但返回原帧继续传给下游；`frames_dropped`
仍计数。单测 `muted_slave_passes_frame_through_without_answering` 覆盖
（WKC=0、`frames_in=1`、`frames_dropped=1`；恢复后 WKC=1）。

由此得出对外口径：`slave_drop` 判定看目标站 **`frames_dropped` 增长**，不是
`frames_in` 停滞（静默站仍物理收帧）。

## 3. 故障矩阵（UDS 合成主站，3×CSP）

命令（验证机本地，无 root）：

```bash
cd /root/ecsim-faults
EC_SIM_BIN=target/release/ec-sim-esc \
  bash scripts/spikes/cnc-e2e-fault-matrix.sh
```

run_id：`fault-matrix-20261115072346`。`probe.out` 全部 OK：

- 初始 3/3 OP；
- `slave_drop` target=1 duration=1500 ms：注入 200、目标站帧计为丢弃、其他站仍收帧、
  `fault_injected`、到期清理、恢复后帧增长、`fault_cleared`；
- `sdo_reject` target=0 duration=1500 ms：注入前 SDO 正常 `0x43`、注入 200、
  注入后 Abort **`0x06010000`**、到期清理、恢复后正常应答、事件成对；
- 矩阵结束仍 3/3 OP。

`RESULT: PASS（0 个失败）`；timeline：`fault-matrix/slave-drop-timeline.json`、
`fault-matrix/sdo-reject-timeline.json`。

## 4. TAP down/up（SOEM 主站）

命令（root，需要 TAP 权限）：

```bash
bash scripts/spikes/cnc-e2e-tap-fault.sh --tap ecsimfault0
```

时间线（`tap-fault/timeline.log`）：

1. `23:23:56` TAP up，主站 #1 建链；`23:23:57` 三站 OP，`frames_in=487/486/486`；
2. `23:23:57` `ip link set ecsimfault0 down` → down 期间各站 `frames_in` 停止
   （`491/490/490` 不变），`/state` 仍返回 200，主站 #1 存活并记录 167 条故障相关行；
3. `23:24:00` `ip link set up`；`23:24:01` 主站 #2 重建至
   `all slaves are now operational`，3/3 OP；恢复后 `frames_in` 由 `1003/996/996`
   增至 `1094/1087/1087`。

`RESULT: PASS`。状态快照：`tap-fault/state-before-down.json`、
`state-during-down-1/2.json`、`state-after-up*.json`。

## 5. 整轮回归

EC-Sim 验证机上 `bash scripts/spikes/run-all-probes.sh`：

```text
== 汇总：28 PASS / 0 FAIL / 0 SKIP（共 28）==
RESULT: PASS
```

完整日志见 `assets/cnc-e2e/b1-28.log`。本机 `cargo test --workspace` **270 passed**
（269 单测 + 1 doc-test）；`cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` 0。

## 6. 边界（勿误读）

- 上述 OP/帧计数来自 EC-Sim 验证机（UDS/TAP 自证），**不是** CNC VM 上
  IgH+lcec+LinuxCNC 的结果；CNC 侧 STALE/LOST、HMI 联动仍待 G1 判定后的联调窗口。
- TAP 故障用未修改的 SOEM 2.0.0 `simple_ng` 主站，不是 IgH ecrt。
- DC 丢失：**P1/NOT-JUDGED**（无硬件 DC；当前架构实现代价高，本轮不做）。
- 8 IO 混合拓扑：P1/NOT-JUDGED（`--slaves` 复制同一 manifest）。
- AL 单站错误 / watchdog 运行期注入：P1（仅 `--serve-write` + `confirm:true` + 审计）。
- `/state.wkc_errors`、`ecsim_wkc_errors_total` 为占位恒 0；WKC 由内核/IgH 派生。
- `--al-error` 仍是启动期**全站**注入，不是单站语义。
- 不宣称 EtherCAT 一致性 / CTT / LinuxCNC 认证；未验证项不写 PASS。
