# CNC 端到端闭环测试 · EC-Sim 支撑验收判据与审计

> 版本：**v0.3（判据冻结 + Lead 裁决落地；task-102 故障矩阵已独立复核）** ｜ 日期：**2026-11-15** ｜ 作者：行业专家（sim-expert）
> 任务：**task-101 / task-102 / task-103**（CNC E2E 支撑）｜ 性质：**支撑性验收判据 + 独立审计记录**
> 用户指令（2026-11-15）：EC-Sim 暂停其他任务，转去支撑 CNC 端到端闭环测试。
> 边界：本文件**不替代** Gate U DoD / CNC-ACCEPTANCE / EC-Sim 产品验收；EC-Sim 侧为仿真从站，**不构成**实时性/一致性/安全/认证保证；不得以本文件写 v1.1/认证/PASS。

---

## 0. 当前状态（2026-11-15，如实记录）

| 项 | 状态 | 依据/说明 |
|---|---|---|
| task-101 固定场景（3×CiA402 CSP + IO、一键起停/重置） | **PENDING（sim-impl 实现中）** | owner=sim-impl（已 claim/in_progress）；计划 `docs/CNC-E2E-PROFILE.md`+启动/复位脚本+lcec XML；当前尚无产物；C5 `c5-stack-up.sh` 可作基线参考 |
| task-102 故障注入（AL/链路/DC/SDO/掉线 + 恢复） | **部分 PASS（SIM 侧独立复核）** | `30ce3e9` 故障矩阵已独立复跑：B1/B5/B6 PASS；B2 中 F-LINK/F-SDO PASS、**F-PROC 未交付**、CNC HMI 联动 PENDING；运行期 AL 注入降 P1/可选（见 §11 裁决）；证据 `assets/cnc-e2e/audit-2026-11-15/` |
| task-103 可观测性与证据矩阵 | **PENDING（sim-impl 实现中）** | owner=sim-impl；计划 `/state` run_id/master/slaves、`/events?since=` JSONL、metrics 扩展；当前 `run_id`/WKC/jitter 未落地 |
| C4 浏览器 4h / DOD-07 | **PAUSED（用户指令）** | 保留历史 NOT-JUDGED 记录，不重判 FAIL、不写 PASS；**已回写 task-78 rev6**（标题前缀 [PAUSED]），恢复后按 task-78 重试 |
| DOD-12 | **PASS（保留）** | 既有判定 `7487186` 不变 |
| 资源 P0 | **PAUSED** | 用户指令暂停 |
| EC-Sim HMI 设计线 | **HOLD** | task-89；G2 `9dd0194`、G5 判据 `f313f61` 保留不删 |
| C5 24h 长稳 | **运行中** | ETA 2026-11-15 18:25；E2E 执行与取证**必须避开 C5**、不抢 VM、不 reboot |

> 本文件所有未实施项一律 **PENDING**，不写 PASS；已实施但不符合判据才写 FAIL；PAUSED 不计入 FAIL。

---

## 1. 范围与边界

**EC-Sim 支撑范围**：固定可复现场景、确定性 REF 状态、故障注入与恢复、可观测性与 run_id 关联、证据包。

**不在范围**：CNC HMI 实现与 Gate U DoD 判定；物理急停/安全门/真实伺服；EtherCAT 一致性/CTT/LinuxCNC 认证；实时性 SLA；v1.1 发布。

**前提**：CNC E2E 以「GUI → 桥(NML) → LinuxCNC/HAL → IgH → TAP → EC-Sim」链路为准（见 CNC 仓 `docs/plan-virtual-loop.md`）；EC-Sim 只对**从站侧可观察行为**负责。

---

## 2. 术语与判定规则

- **E2E run**：一次 `stack-up → 场景执行 → 证据采集 → stack-down`；`run_id` = UUID v4，由测试 harness 生成并写入 `runs/<run_id>/run.json`。
- **REF 状态**（固定基线，须在 task-101 profile 中逐字段定义）：
  - SIM 侧：3/3 从站 `AL=OP(0x08)`、`al_status_code=0`、无 watchdog 触发、PDO 初值 = profile 文档值、站地址固定、WKC 符合 3 字节 LRW 期望；
  - CNC 侧：`DRO=0`、无报警、无程序、G54、overrides=100%、MODE=MAN（与 CNC 团队确认后写入 `REF.json`）。
- **判定**：
  - **PASS**：可复现命令 + 原始证据（JSON/日志/截图）+ 环境（HEAD、二进制/manifest sha256、run_id、时间）+ 达到阈值；3 次运行证据。
  - **PENDING**：未实施/无证据/能力缺失；**不得**写成 PASS 或 FAIL。
  - **FAIL**：已实施，但未达到判据（可复现失败）。
  - **PAUSED**：用户/Lead 明确暂停；不改变历史判定，不计 FAIL。
- **证据规则**：摘要/自述不算证据；以 `/state`、`/metrics`、`/events` 原始响应、SIM stderr `AL_EVENT`、CNC 侧日志、截图为准；大文件给 sha256；时间戳须单调、可对齐。

---

## 3. task-101 判据：固定场景（A1–A7）

| ID | 判据 | 阈值/方法 | 状态 |
|---|---|---|---|
| A1 | profile 固定且文档化：3×CiA402 CSP + 8 路 IO（或裁剪），站地址/VID/PID/PDO/SM/WKC 期望、manifest sha256 写入 `docs/CNC-E2E-PROFILE.md` | 文档 + `manifest sha256` + `/topology`、`/state` 实测一致 | PENDING |
| A2 | 连续 3 次一键 stack-up，每次 **3/3 从站 OP**；同一组站地址；无 AL error | 3/3 次；OP 时间 ≤120s/次；每次留存 `state.json`/日志 | PENDING |
| A3 | **REF 一致性**：run1/2/3 的 `REF.json` 在定义字段上完全相等（SIM+CNC 两侧，OP 后 5s 内采集） | 字段级 diff = 0；任一差异 FAIL | PENDING |
| A4 | 重置/stack-down 幂等：reset 回到 REF ≤30s；结束后无残留 `ec-sim-esc`、TAP 已删、无 lock/shm | 3/3 次；进程/TAP/lock 检查命令输出 | PENDING |
| A5 | 10 分钟 OP 保持：`frames_in` 单调增、`al_transitions` 仅启动期、`watchdog_triggers=0`、结束仍 3/3 OP | 0/5/10min 抓 `/metrics`；3 次中至少 1 次完整 10min | PENDING |
| A6 | 可复制 runbook：TAP/MAC/promisc/IgH modprobe/lcec XML 片段、端口/socket、日志位置；无真实地址/账号；离线可用 | 文档可照做；0 外部网络依赖 | PENDING |
| A7 | 证据包：`runs/<run_id>/` 含 run.json、manifest sha256、state/metrics/topology、stack 日志、清理证据 | 目录完整+hash；缺项=PENDING | PENDING |

---

## 4. task-102 判据：故障注入与恢复（B1–B6）

| ID | 判据 | 阈值/方法 | 状态 |
|---|---|---|---|
| B1 | 故障接口默认关闭、显式开启、二次确认、**审计**（who/时间/类型/目标/持续时间/前值→后值/run_id）；不启用时 404/405 | 审计记录可读；未开启时接口拒绝 | ✅ **PASS**（独立复跑：405/400/200；`fault_injected` 含 confirm/who/run_id/before/after） |
| B2 | **v1.0 必做故障类**（外部可实现）：F-LINK（TAP down/up）、F-PROC（EC-Sim 进程 kill/重启）、F-SDO（未开 `--serve-write` 405 / 非 writable 403）；每类触发后 HMI 显示 STALE/LOST/ALARM 且运动命令禁用，恢复后 3/3 OP 且可继续 | 每类 ≥1 次完整触发→恢复；证据 `/state`、`/events`、CNC 日志、审计 | 🟡 **部分**：F-LINK ✅（TAP down/up）、F-SDO ✅（Abort `0x06010000`）；**F-PROC 未交付**；CNC HMI 联动 PENDING（§4.1） |
| B3 | **单从站 AL error 运行期注入（0x11/0x24）** | **与 BL-20 暂停冲突**：task-42 已冻结、v1.x 候选；不得擅自实现。可用 startup `--al-error` 做“重启后单站错误”场景；运行期注入待 Lead 裁决 → PENDING/待裁决 | PENDING（P1，不阻塞；Lead 已裁决） |
| B4 | DC 丢失 / 主站重启 | 控制面无运行期 DC 故障 API；标记可选 v1.x；不阻塞 B2 | PENDING（P1/可选） |
| B5 | 恢复路径：故障清除后自动/一键回 OP、无手动 reboot、≤60s；审计含恢复时间 | 计时 + 状态快照；失败可复现 | ✅ **PASS（SIM 侧）**：矩阵到期清理/恢复 <5s、TAP 重建 ~2s、无 reboot；HMI 侧恢复待 CNC 联调 |
| B6 | 测试模式隔离：故障仅在测试档；不干扰 C5/生产；清理彻底 | 检查命令；C5 期间禁止执行 | ✅ **PASS（SIM 侧）**：复跑后无残留进程/端口/TAP；C5 在 CNC VM 未受影响 |

> **裁决（Lead，2026-11-15）**：AL 运行期注入 **v1.0 E2E 不作为必验（降 P1/可选）**；P0 只保 **F-LINK / F-PROC / F-SDO** 三类。AL 显示测试优先用启动参数 `--al-error` 或故障模板；若运行期注入与 BL-20（task-42 暂停）冲突则记 P1 待做，**不阻塞 E2E**。sim-impl 若实现须 enabled 门禁+二次确认+审计，但不得据此写 v1.0 PASS。

### 4.1 独立复跑记录（2026-11-15，EC-Sim 验证机）

- **被测**：EC-Sim `30ce3e9`；二进制 sha256 `912564b4…`（1,153,736 B）、manifest `5f96adc1…`；脚本与本地 commit 逐字节一致；现场 `/root/ecsim-faults`。
- **fault-matrix**：`EXIT=0 / RESULT: PASS（0 失败）`，15 项 OK（3/3 OP、slave_drop 看 `frames_dropped`、SDO Abort `0x06010000`、事件成对、结束 3/3 OP）；run_id `audit-fm-20261115073119`。
- **tap-fault**：`EXIT=0 / RESULT: PASS`，down 后 `frames_in` 停止（492/491/491）、控制面仍 200；恢复后 `1003/996/996 → 1094/1087/1087`、主站 #2 `all slaves operational`、3/3 OP；run_id `audit-tap-20261115073204`。
- **B-1 整轮**：`28 PASS / 0 FAIL / 0 SKIP`（含 `fault-matrix`/`tap-fault`/`cnc-e2e-fixed-scene`/`e2e-control`）。
- **门禁/审计**：无 `--serve-write` → 405；缺 confirm → 400；`confirm:true` → 200；`/events` 含 confirm/who/run_id/before/after；`/fault/status` 可读。
- **证据**：`docs/verification/assets/cnc-e2e/audit-2026-11-15/`（README、gating.md、fault-matrix/、tap-fault/、b1/，含 sha256 清单）。
- **未过项（不写 PASS）**：F-PROC（进程 kill/重启）无用例/证据；CNC 侧 STALE/LOST/ALARM 显示与运动禁用需联调窗口；8 IO/DC/运行期 AL 注入为 P1/NOT-JUDGED。
- **口径**：只证明 EC-Sim 侧故障触发/恢复语义；不称 EtherCAT 一致性/CTT/LinuxCNC 认证。

---

## 5. task-103 判据：可观测性与证据矩阵（C1–C5）

| ID | 判据 | 阈值/方法 | 状态 |
|---|---|---|---|
| C1 | 遥测完整：逐从站 AL 码/迁移、`al_errors`/watchdog、`frames_in/out/dropped`、邮箱、SII、PDO；**WKC 与周期抖动**须有来源（新增指标或 IgH 侧派生并文档化） | `/metrics` 解析；字段清单逐项核对；缺项=PENDING | PENDING |
| C2 | `/events` SSE：interval 可配、时间戳单调、含 `AL_EVENT`；10min 采样无断档 | 采样行数/间隔/单调性检查 | PENDING |
| C3 | **run_id 关联**：CNC 测试脚本、SIM 事件/指标、证据 manifest 使用同一 `run_id` | **先做 harness 侧关联**（测试脚本/证据 manifest 带 run_id，SIM `/events` 按时间戳对齐）；控制面加字段列 **P1**、不改现有协议；`runs/<run_id>/` 三处一致 | PENDING |
| C4 | 用例×SIM 期望矩阵：jog/MDI/程序/回零/override/tool/offset/probe/报警/断链 10 类，各列 SIM 可观察量（AL/PDO/events/metrics）与采集方法 | 10/10 覆盖；矩阵可执行 | PENDING |
| C5 | 不伪造 PASS：原始证据+hash；失败可复现；结论逐项 PASS/PENDING/FAIL | 抽查证据；无源数据不得 PASS | PENDING |

---

## 6. CNC 侧前置（E2E 门，不属 EC-Sim 验收）

| ID | 内容 | 状态 |
|---|---|---|
| D1 | CNC REF 快照：DRO=0、G54、overrides=100%、无报警/程序、MAN | PENDING（CNC 团队） |
| D2 | HMI 对 STALE/LOST/ALARM 的显示与运动禁用符合 Gate U；本文件不重判 Gate U | PENDING（引用 Gate U） |
| D3 | 桥/测试脚本记录 run_id 与命令日志，供 C3 关联 | PENDING（CNC 团队） |

---

## 7. 审计执行程序（待 C5 结束后）

1. 前置检查：C5 已结束/让出 VM；记录 EC-Sim HEAD、二进制 sha256、manifest sha256、CNC HEAD。
2. 生成 `run_id`（`uuidgen`），导出 `E2E_RUN_ID`；创建 `runs/<run_id>/`。
3. 连续 3 次 stack-up：抓 `/state`、`/metrics`、`/topology`；比对 A2/A3/A4。
4. 10min soak：0/5/10min 抓 `/metrics`；保存 SIM stderr（含 `AL_EVENT`）。
5. 故障矩阵：B2 逐类触发→恢复；保存 `/events` 片段、CNC 日志、审计。
6. 观测矩阵：C1/C2/C4 逐项核对；C3 三处 run_id 一致性。
7. 输出/更新本文件状态表；FAIL 附复现命令；PENDING 给缺失原因与责任项。
8. 任何二进制/manifest 变更 → 旧证据失效，需重跑。

---

## 8. 待 Lead 裁决 / 对齐

- **Q1（阻塞项）**：task-102 的“单从站 AL error 运行期注入”与 BL-20 暂停（task-42）冲突。建议 v1.0 仅验收 F-LINK/F-PROC/F-SDO，AL 运行期注入标 PENDING/待解冻；是否同意？
- **Q2**：run_id 关联采用哪层实现：仅 harness 侧日志/manifest 关联（零产品改动），还是 EC-Sim 控制面增加 run_id 回显/字段（task-103 范围内的小改动）？
- **Q3**：task-101/102/103 当前无 owner；由 sim-impl 实施、我独立审计，还是另派？请确认。
- **Q4**：VM 窗口：C5 ETA 18:25；之后是否允许占用 EC-Sim/CNC 验证机执行 3 次 + 10min soak？不 reboot。
- **Q5**：C4 浏览器/DOD-07 的 PAUSED 状态是否需回写 task-78 描述/原验证文档（我只在本文件记录，未改他人文档）。

---

## 9. 非目标

- 不替代 Gate U DoD / CNC-ACCEPTANCE / EC-Sim 产品验收；
- 不声称 EtherCAT 一致性、CTT、LinuxCNC 或安全认证；
- 不把仿真通过当作真实设备/物理安全验证；
- 不在 C5 运行期抢 VM、不 reboot、不改产品代码（本阶段仅判据/审计）。

---

## 10. 对齐记录（sim-impl，2026-11-15）

- sim-impl 持有 task-101/102/103（claim/in_progress），正在 EC-Sim 仓实现；计划产物：`docs/CNC-E2E-PROFILE.md`+启动/复位脚本+lcec XML（101）；`POST /fault`、`/fault/status`+运行期故障应用与审计（102）；`docs/CNC-E2E-OBSERVABILITY.md`+`/state` run_id/master/slaves、`/events?since=`、metrics 扩展（103）。
- 现状确认：`/metrics` 无 WKC/jitter（WKC 仅 `/topology` sm2/sm3 hex 与内核侧；EC-Sim 无 master WKC 计数）；无 `/fault`；无 `run_id`；`ack_latency_ms` SIM 侧无真实值（计划 null，由桥侧测）。
- 可复用资产：`plugins/cnc-csp-servo/plugin.yaml`（manifest sha256 `5f96adc1…`）、`scripts/spikes/tap-cia402-multi-slave-probe.sh`、`scripts/spikes/tap-live-state-probe.sh`、CNC 仓 `scripts/vm-verify/c5-stack-up.sh`（3×CiA402 `--slaves 3`）、`docs/verification/2026-11-14-b62-al-telemetry.md`。
- 约束：C5 结束（11-15 18:25）前不碰 CNC VM、不 reboot；EC-Sim VM 做 3×CSP 连续 3 次 3/3 OP；C4 浏览器/DOD-07 记 PAUSED；DOD-12 PASS 保留。
- 审计规则：所有产物先按 **PENDING**，我独立复跑/取证后才可改 PASS/FAIL。

## 11. Lead 裁决落地（2026-11-15，m01525）

1. **AL 运行期注入**：v1.0 E2E **不作为必验**（降 P1/可选）；P0 只保 F-LINK/F-PROC/F-SDO；AL 显示测试优先 `--al-error` 启动参数或故障模板；与 BL-20 冲突则记 P1 待做、不阻塞 E2E。
2. **run_id**：先做 **harness 侧关联**（脚本/manifest 带 run_id，SIM `/events` 按时间戳对齐）；控制面加字段列 **P1**、不改现有协议。
3. **owner**：task-101/102/103 = **sim-impl**；我负责审计与判据维护；sim-pm 写 `CNC-E2E-SUPPORT.md`。
4. **VM 窗口**：C5 18:25 结束 → 18:25–18:45 收尾归档；18:45–19:30 Lead 独占 CNC VM 做 G1 组件像素/10 页重截；19:30–24:00 CNC E2E 功能验证（SIM 固定场景+故障注入）；SIM 任务在 EC-Sim VM 随时进行、不占 CNC VM。
5. **C4/DOD-07 PAUSED** 已回写 task-78 rev6（标题前缀 [PAUSED]，证据保留、不重判）。
6. **REF 对齐**：task-101 的 REF 状态需与 cnc-impl 的 demo-state 对齐后写入 `REF.json`（D1/D3）。

> 裁决不改变本文件判定规则：PENDING 不得写成 PASS；FAIL 须可复现；PAUSED 不计 FAIL。