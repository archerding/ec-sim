# CNC E2E 固定场景 REF-CNC-E2E-3CSP（EC-Sim 侧）

> task-101 交付物。范围：EC-Sim 侧 3×CiA402 CSP 固定场景、启动/停止/复位命令、
> 参考状态（REF）定义与证据。CNC 侧（IgH + lcec + LinuxCNC + 桥 + HMI）的端到端
> 验收由 C5/CNC VM 与 `cnc-impl` 承担；本文不宣称认证/一致性测试。
>
> 边界：EC-Sim 侧没有 IgH/lcec，因此本文的 3/3 OP 是 **UDS 合成主站**对 EC-Sim
> 状态机的自证；TAP+IgH 下的 `0:0/0:1/0:2 OP` 仍需在 CNC VM 上复核（见 §6）。

## 1. 拓扑与 PDO 布局

| 项 | 值 |
| --- | --- |
| 场景 ID | `REF-CNC-E2E-3CSP` |
| 从站 | 3 × `plugins/cnc-csp-servo/plugin.yaml`（VID `0x00000EC5` / PID `0x0000E5E3`） |
| 行为 | `--behavior cia402`，Tx 布局 `Cia402TxLayout::LcecCsp` |
| SM0/SM1 | `0x1000`/128（mailbox in）、`0x1080`/128（mailbox out） |
| SM2 RxPDO | `0x1100`，**7 B**：`0x6040` U16、`0x6060` S8、`0x607A` S32 |
| SM3 TxPDO | `0x1110`，**11 B**：`0x6041` U16、`0x6064` S32、`0x606C` S32、`0x6061` S8（GAP-14 顺序） |
| DC | 不配置（V1/V2 不启用） |
| 0x6502 | `0x01C1`（CSP 支持位） |
| lcec XML 样例 | [cnc-e2e-3csp.xml](cnc-e2e-3csp.xml)（`basic_cia402`，`enablePV=false`/`enableCSP=true`） |

PDO 逐字节布局见 [plan/CNC-V1-integration.md](../plan/CNC-V1-integration.md) §2；此处不复制，避免漂移。

## 2. REF 状态（SIM 侧可断言部分）

CNC 侧 REF（demo-state）由 `cnc-impl` 的用例清单定稿后落到 `REF.json`（本目录，当前 **PENDING**，
不得用本文件替代）。SIM 侧 REF 断言：

| 断言 | 位置 | 期望 |
| --- | --- | --- |
| run_id | `/state.run_id` | 等于 harness 传入的 `--run-id` |
| 主站态 | `/state.master.al_state` | `0x08`（全 OP） |
| OP 数 | `/state.master.op_slaves` | `3` |
| 逐站态 | `/state.slaves[i].state` / `.op` | `0x08` / `true` |
| AL 码 | `/state.slaves[i].al_code` | `0x0000` |
| PDO 布局 | `/state.master.pdo.tx` / `.rx` | `7` / `11` |
| 看门狗 | `/state.master.watchdog.trips` | `0`（保持期内不应触发） |
| 丢帧/WKC | `/state.master.pdo.miss` / `.wkc_errors` | `0` / `0`（占位口径，见 §7） |

CNC 侧 REF（程序为空、DRO=0.000、无刀具、G54、overrides=100%、非 ESTOP）由
`cnc-impl` 的 HMI/桥证据提供；SIM 不产生此类状态。

## 3. 一键命令

### 3.1 EC-Sim VM（自证，UDS，无 root）

```bash
# 3 轮 × 20 s 保持期；每轮独立进程、独立 run_id
EC_SIM_BIN=target/release/ec-sim-esc \
LOG_DIR=.scratch/e2e-fixed-scene \
bash scripts/spikes/cnc-e2e-fixed-scene.sh --rounds 3 --hold-seconds 20 --port 18210
# 期望：round-1..3 PASS + RESULT: PASS（3/3 轮 3/3 OP）
```

单轮手工：

```bash
target/release/ec-sim-esc --uds /tmp/ref3.sock --slaves 3 \
  --manifest plugins/cnc-csp-servo/plugin.yaml --behavior cia402 \
  --serve 127.0.0.1:18210 --serve-write --run-id ref-3csp
python3 scripts/spikes/uds_op_hold.py /tmp/ref3.sock \
  --http-base http://127.0.0.1:18210 --expect-slaves 3 --hold-seconds 20
```

### 3.2 CNC VM（TAP + IgH + lcec，由 `cnc-impl` 执行）

```bash
# 使用 cnc-impl 的 C5 栈脚本；EC-Sim 侧参数等价于：
#   --tap tapcnc0 --slaves 3 --manifest plugins/cnc-csp-servo/plugin.yaml --behavior cia402 \
#   --serve 127.0.0.1:18210 --run-id <harness run_id>
ethercat slaves          # 期望 0/1/2 均 OP（flag 列 '+'）
curl -fsS http://127.0.0.1:18210/state   # master.op_slaves==3
```

停止/复位（CNC 侧）：`c5-stack-down.sh` → `c5-stack-up.sh`（或等价的 LinuxCNC 停止 →
`ethercatctl stop/start` → EC-Sim 重启）；本仓库不直接操作 CNC VM。

## 4. 证据（本仓）

`docs/verification/assets/cnc-e2e/fixed-scene/`：

- `summary.txt`：二进制 sha256 `d2241dec…`、manifest sha256 `5f96adc1…`、3 轮结果；
- `round-N/hold.out`：`HOLD_OK samples≈970 slaves=3 run_id=… al_state=0x08`；
- `round-N/state.json`：`master.op_slaves=3`、`pdo.tx=7/rx=11`、逐站 `al_code=0x0000`；
- `round-N/server.log`：启动日志（manifest/behavior/tx_layout/SERVE）。

判定：**EC-Sim 侧 3/3 轮 3/3 OP PASS**（2026-11-15，EC-Sim 自己的验证机；见
[verification/2026-11-15-cnc-e2e-control-plane.md](../verification/2026-11-15-cnc-e2e-control-plane.md)）。
CNC VM 的 TAP+IgH+lcec 结果不在此文件内，未验证前不得引用为 PASS。

## 5. 复现步骤（含观测）

```bash
# 1) 起栈
target/release/ec-sim-esc --uds /tmp/ref3.sock --slaves 3 \
  --manifest plugins/cnc-csp-servo/plugin.yaml --behavior cia402 \
  --serve 127.0.0.1:18210 --serve-write --run-id ref-3csp
# 2) 拉 OP 并保持（另一终端）
python3 scripts/spikes/uds_op_hold.py /tmp/ref3.sock \
  --http-base http://127.0.0.1:18210 --expect-slaves 3 --hold-seconds 60
# 3) 观测采集（另一终端）
python3 scripts/spikes/e2e_observability_collect.py \
  --base http://127.0.0.1:18210 --run-id ref-3csp \
  --seconds 60 --interval 1 --out .scratch/observability/ref-3csp
# 4) 失败排查：/state、/events?since=0、/metrics 三者同 run_id 对齐
```

## 6. 与 CNC 侧的分工

| 项 | EC-Sim 侧 | CNC 侧 |
| --- | --- | --- |
| 3/3 OP | UDS 合成主站自证（本文） | TAP + IgH + lcec 复核 |
| PDO 值 | 7B/11B 布局与 Tx 顺序 | lcec HAL pin 值/方向 |
| REF 状态 | run_id/AL/PDO 计数 | 程序/DRO/G54/overrides/ESTOP |
| 8 IO | **P1/NOT-JUDGED**（当前无混合拓扑 manifest） | 后续用例 |

## 7. 已知边界

- 8 IO 混合拓扑不在本轮固定场景内（P1/NOT-JUDGED；`--slaves` 复制同一 manifest）。
- EC-Sim 侧不使用 IgH/lcec；本文的 OP 不代表 IgH 主站视角。
- `/state.wkc_errors` 与 `ecsim_wkc_errors_total` 是占位（恒 0）；WKC 由内核/IgH 侧派生。
- 无 DC；`--al-error` 是启动期全站注入，运行期单站注入为 P1。
- 不宣称 EtherCAT 一致性/CTT/LinuxCNC 认证。
