# CNC E2E 支撑：EC-Sim 供给清单与可观察量接口约定（v0.1）

> 状态：**草案（2026-11-15，CNC E2E 支撑期）** ｜ 作者：PM（sim-pm / DeepSeek Harness）
> 定位：本文件回答"**CNC 端到端闭环测试要 EC-Sim 提供什么、EC-Sim 现在能提供什么、缺什么**"，并给出 **AC-E-926（CNC HMI 逐页功能一致性）所需的可观察量接口约定**。它是**供给侧清单 + 接口契约**，不是验收结论，也不改变 CNC 侧任何判据。
> 背景：用户 2026-11-15 指令（经 Lead 转达）：**SIM 暂停其他任务，转去支撑 CNC E2E；由 CNC E2E 需求驱动 SIM 开发**。供给项映射 task-101 / task-102 / task-103。
> 配套判据（**权威**）：`docs/verification/2026-11-15-cnc-e2e-support-audit.md` **v0.2**（sim-expert，task-101 A1–A7 / task-102 B1–B6 / task-103 C1–C5 + Lead 裁决）；本文件只做**供给侧映射**，不重复判据、不替代该文件。
> 技术基线：EC-Sim HEAD `acba541`（B62 遥测 `73b5151` + E2E 判据两笔 docs；写作时 sim-impl 正在实现 task-101/102/103，工作区有未提交改动，以下"现状"以 HEAD 已落地能力为准）。C5 栈固定资产：`ec-sim-esc` sha256 `75fa8a1a…`、`cnc-csp-servo` manifest sha256 `5f96adc1…`（以 `cnc-station/scripts/vm-verify/c5-stack-up.sh` 内置值为准）。
> 事实依据：`docs/plan/CNC-V1-integration.md`（V1/V2 PDO 布局）、`docs/CONTROL-PLANE.md`（控制面协议）、`docs/STATUS.md`、`docs/VERIFICATION.md`、`docs/verification/2026-11-14-b62-al-telemetry.md`。
> 真实性红线：不宣称 EtherCAT 一致性 / CTT / 认证 / 与厂商设备兼容 / 实时 SLA；未验证项标 `⚠️`；`0x18` 等未知 AL 码一律 NOT-JUDGED（`G-17`）；PENDING 不得写成 PASS。

---

## 0. 一句话结论

- **已能提供（P0）**：固定场景骨架（3×`cnc-csp-servo` 克隆，TAP）、PDO 双向可见（`/pdo` raw+decoded）、启动期故障/错误注入、B62 运行期 AL 遥测（`/state.live.stats` + `last_al_event` + `AL_EVENT` + `/metrics`）、只读控制面（`/state`、`/topology`、`/events`、`/health`）、可重复脚本与二进制/manifest 指纹。
- **尚缺（P0，sim-impl 实现中）**：`docs/CNC-E2E-PROFILE.md` + 启动/复位脚本 + lcec XML（task-101）、`POST /fault` 与审计/恢复（task-102，P0 仅 F-LINK/F-PROC/F-SDO）、可观测性补齐与证据矩阵（task-103：WKC/抖动来源、`/events?since=`、harness 侧 run_id）。
- **P1**：运行期单站 AL 注入（与 BL-20 冲突，Lead 裁决降 P1/可选）、混合画像拓扑（多轴 + IO 同链）、DC 真实同步（不承诺）、控制面 run_id 字段。
- **AC-E-926 的接口原则**：CNC 侧主证据 = **LinuxCNC 可观察量**（HAL pin / `stat` / NML·日志 / 运动·DRO·报警码）；EC-Sim 提供**第二来源对账**（同一次动作在从站侧的 PDO/SDO/AL 变化），用于证明"不是 UI 动画/本地缓存/假值"，**不能替代** LCNC 证据。

---

## 1. 当前阶段与暂停边界

**当前阶段 = CNC E2E 支撑**（2026-11-15 起）。由 CNC E2E 需求驱动 SIM 开发；P0/P1 以本文件 §2 + 判据文件 §3–§5 为准。

| 项 | 状态（2026-11-15） | 处置 |
| --- | --- | --- |
| CNC E2E 支撑（task-101/102/103） | **P0，进行中** | owner=sim-impl（claim/in_progress）；sim-expert 维护判据并独立审计 |
| C5 24h 长稳 | **运行中**（ETA 18:25） | E2E 执行/取证避开 C5，不抢 CNC VM、不 reboot |
| C4 浏览器 4h / DOD-07/08 | **PAUSED** | 历史 NOT-JUDGED 保留；不重判 FAIL、不写 PASS；已回写 task-78 rev6 |
| 资源 P0（SOEM 逐画像 / `source_accessed`） | **PAUSED** | 不撤已有证据、不判 ✅ |
| EC-Sim HMI 设计线（V3/G3/G4） | **HOLD**（task-89） | 不启动、不提交新设计文档；G1 `d376675` / G2 `9dd0194` / G5 判据 `f313f61` 保留 |
| DOD-12 | **PASS（保留）** | `7487186` 既有判定不变，不在本阶段重判 |
| HMI 解冻 / v1.1 实施放行 | **不变**：仍待用户/GO | 发布冻结、GO-B Gate U DoD 13/13 不变 |

> 恢复条件：Lead/用户显式指令。本文件不据此宣称任何暂停项 PASS 或 FAIL。

---

## 2. 供给清单（S1–S9）

| ID | 供给项 | 优先级 | 承接 task | 现状（HEAD `acba541`） | 缺口 / 证据 |
| --- | --- | --- | --- | --- | --- |
| **S1** | 固定场景：TAP + 3×CiA402 CSP（+8 路 IO 或裁剪）+ REF 状态 | **P0** | task-101 | 🟡 场景骨架可复现（`--slaves 3` 克隆同画像；C5 栈已按此起跑） | `docs/CNC-E2E-PROFILE.md` + `REF.json` 待 sim-impl 产出；混合画像不支持 → S6/P1；C5 栈可作基线参考 |
| **S2** | 一键起停 / 健康 / 重置（EC-Sim 侧） | **P0** | task-101 | 🟡 部分：进程级起停 + `GET /health`；CNC 侧 `scripts/vm-verify/c5-stack-up.sh`/`c5-stack-down.sh` 可作参照 | 无 EC-Sim 侧 up/down/reset 脚本；"健康"未定义深度判据 → §6 |
| **S3** | REF 状态定义与判据（可机读） | **P0** | task-101 | ✅ 本文 §3 定稿（观测全部来自现有端点）；权威算法见判据文件 A2/A3 | 需与 cnc-impl demo-state 对齐后写入 `REF.json`（Lead 裁决 #6） |
| **S4** | 故障注入（F-LINK / F-PROC / F-SDO 为 P0；AL/DC 为 P1/可选） | **P0**（三类） | task-102 | 🟡 仅**启动期**：`--fault <hex>`（CiA402）、`--al-error <hex>`（AL error+code，已知码白名单） | 运行期 `/fault` **不存在**；无审计/恢复；AL 运行期注入与 BL-20 冲突 → P1/可选 |
| **S5** | 可观测性（遥测 / 事件 / 指标 / SSE） | **P0** | task-103 | 🟡 B62 遥测已落地；`/events` SSE 已有 | 缺 WKC/周期抖动来源（C1）、`/events?since=`（C2）、harness 侧 run_id（C3，P0）、证据矩阵（C4） |
| **S6** | 多从站与拓扑（含混合画像） | **P1** | task-101 后续 | 🟡 `--slaves N` 克隆同画像（TAP/UDS 8 从站已验证；`tap-cia402-multi-slave-probe.sh` N=2 断言 WKC `3N`、`/topology` 全 OP） | **混合画像不支持**（3 伺服 + IO 需"每从站画像列表"）⚠️ |
| **S7** | 控制面契约（只读、站号、错误语义） | **P0** | 跨 task | ✅ 只读 GET；`?slave=N` 0-based、非法/越界统一 404（B55 `validate_slave_param`）；写路径默认 405（`--serve-write` 才开） | 虚拟闭环不需要写路径；保持默认关闭 |
| **S8** | 证据与可重复性（脚本 / 指纹 / 注册表） | **P0** | task-103 | ✅ 脚本自算注册表：B-1 **24 项**（`run-all-probes.sh`）、B-2 **37 项**（`run-acceptance-probes.sh`；EC-Sim VM 2026-11-14 轮：30 PASS/0 FAIL/7 SKIP，7 SKIP=`igh-*` NOT-JUDGED） | EC-Sim VM 无 IgH 应用 → `igh-*` 类不判；CNC VM 侧由 C5 栈承接 |
| **S9** | 边界与真实性（不宣称、不造假） | **P0** | 全部 | ✅ 文档/代码口径一致：仿真器侧计数 ≠ 链路质量；`0x18` NOT-JUDGED；无认证声明 | 新增供给项须逐条过本文件 §9 |

> 计数纪律：B-1/B-2 项数一律"以脚本 `--list` 自算"；本文所记 24/37 为 HEAD `73b5151` 口径（见 `docs/STATUS.md`），后续以脚本输出为准。

---

## 3. 固定场景与 REF 状态（S1 / S3）

> 权威验收：判据文件 **A1–A7**（连续 3 次 stack-up、3/3 OP ≤120s/次、REF 字段级 diff=0、reset 幂等 ≤30s、10min OP 保持、runbook、证据包）。本节只给 SIM 侧 REF 摘要，供 task-101 写入 `docs/CNC-E2E-PROFILE.md` 与 `REF.json`。

### 3.1 场景 REF-CNC-E2E-3CSP（C5 采用）

| 项 | 值（固定） |
| --- | --- |
| 线缆层 | Linux TAP `tapcnc0`，MAC `02:ec:51:4d:00:01` |
| 从站 | `--slaves 3`，画像 `plugins/cnc-csp-servo/plugin.yaml`（**克隆同画像**） |
| 行为 | `--behavior cia402`（内置 Tier0 CSP 状态机，不依赖 WASM feature） |
| 控制面 | `--serve 127.0.0.1:18210`（只回环；写路径默认关闭） |
| PDO 布局 | Rx/SM2 **7B**：`0x6040` U16 + `0x6060` S8 + `0x607A` S32；Tx/SM3 **11B**：`0x6041` U16 + `0x6064` S32 + `0x606C` S32 + `0x6061` S8（GAP-14 顺序） |
| DC | 不配置（lcec XML 无 `<dcConf>`；仅 Tier0 帧驱动时钟） |
| 资产指纹 | `ec-sim-esc` sha256 `75fa8a1a…`；manifest sha256 `5f96adc1…`（以 `c5-stack-up.sh` 内置值为准） |
| IO | 3×CSP + 8 路 IO **或裁剪**（判据 A1 允许裁剪）；混合画像当前不支持 → S6/P1 |

> 布局依据与逐字节核对见 `docs/plan/CNC-V1-integration.md` §2；`--esm-lax` **不需要**（画像 SM/PDO 自洽）。

### 3.2 SIM 侧 REF 判据（启动完成、无故障、主站在跑）

| # | 观测面 | 期望 | 判定方式 |
| --- | --- | --- | --- |
| REF-1 | EC-Sim `/topology` | `count=3`，`slaves[0..2]` 的 AL 低 4 位 = `0x08`（OP） | `GET /topology` |
| REF-2 | EC-Sim `/state?slave=N` | `live.al_status` 低 4 位 `0x08`、bit4=0（无 error）、`al_status_code=0x0000` | 逐站 N=0,1,2 |
| REF-3 | EC-Sim `/pdo?slave=N` | `sm2` 7B / `sm3` 11B；`decoded` 含 `0x6040/0x6060/0x607A`（rx）与 `0x6041/0x6064/0x606C/0x6061`（tx），顺序与 §3.1 一致；初值 = profile 文档值 | 逐站比对字节长度、索引顺序、初值 |
| REF-4 | EC-Sim `/health` | HTTP 200（存活级） | `GET /health` |
| REF-5 | EC-Sim `/metrics?slave=N` | 启动后 `sm_watchdog_triggers=0`；`al_transitions` 随启动迁移增长后稳定 | 0/5/10 min 采样（A5） |
| REF-6 | IgH（**CNC 侧**） | `ethercat slaves` = 3 行、全 `OP` | CNC 脚本；SIM 不替证 |
| REF-7 | lcec（**CNC 侧**） | WKC = `3N` = 9（3 站，判据按 3 字节 LRW 期望）；lcec HAL pin 出现 | CNC 脚本；SIM 不替证 |
| REF-8 | 桥/GUI（**CNC 侧**） | 10 Hz snapshot、命令两段 ACK；CNC REF：`DRO=0`、无报警/程序、G54、overrides=100%、MODE=MAN | CNC 脚本；SIM 不替证 |

> REF 状态是后续所有 E2E 用例的**前置**；REF 不满足时不判任何功能用例（记录 BLOCKED/PENDING）。REF 字段以判据文件 A1/A3 为准，`REF.json` 由 task-101 产出。

### 3.3 场景变体

- **V1 单站 PP**：`--slaves 1` + `plugins/generic-cia402-servo/plugin.yaml`（Rx 7B / Tx 7B，`0x6502=0x01C1`）——历史联合验收场景，保留作回归。
- **3 站 CSP**：本文件 §3.1（C5 / AC-E-926 主场景）。
- **3 伺服 + IO**：判据 A1 允许"或裁剪"；混合画像当前不支持 → **P1**（S6）。不得在文档/验收中假装支持。

---

## 4. 可观察量清单（S5 / S7）

### 4.1 只读端点（虚拟闭环用）

| 端点 | 语义 | 用于 E2E 的用途 |
| --- | --- | --- |
| `GET /state?slave=N` | `ec-sim/state-v1`；实时模式含 `live` | 从站身份/画像、AL 状态、`live.stats`、`last_al_event` |
| `GET /pdo?slave=N` | `ec-sim/pdo-v1`：`sm2/sm3` raw hex + `decoded` | 动作前后 PDO 对账（0x6040/0x6041/0x6060/0x6061/0x6064/0x606C/0x607A） |
| `GET /pdo?slave=N&history=1` | `ec-sim/pdo-history-v1`（按变化去重，每站 256 条） | 变化时序（谁先变、间隔） |
| `GET /topology` | `ec-sim/topology-v1`：全局，无 `?slave` | 3 站全 OP / 逐站 `frames_in` 等摘要（WKC 无独立计数，见 §4.4） |
| `GET /metrics?slave=N` | Prometheus 文本，`ecsim_*`（AL 迁移/错误/ACK/watchdog/SII/帧/邮箱/DC 计数） | 长跑计数器增量；ALARM-DIAG 对账 |
| `GET /events?slave=N&interval_ms=` | SSE 推 `state-v1`（默认 500 ms，100..5000 可调） | 实时观察；断流/降级判据由 CNC 侧定义；`?since=` 待 task-103（C2） |
| `GET /health` | 内联 JSON 存活级 | 起停后"已就绪"的快速判据（REF-4） |
| `GET /sdo?index=&sub=&slave=N` | 运行时对象字典只读浏览（不经 mailbox） | 参数类功能的**配置侧**对账（非动作证据） |
| stderr `AL_EVENT` | `AL_EVENT slave=N from=0xNN to=0xNN from_code=0xNNNN code=0xNNNN watchdog=<bool> at_ms=N` | 长跑日志定位"哪个从站、何时、因何离开 OP" |

> 写路径（`POST /pdo`、`POST /sdo`、`/sdo-audit`）仅在 `--serve-write` 下开启；**虚拟闭环不需要**，保持关闭作为默认安全边界。task-102 的 `POST /fault` 计划复用同一安全模型（默认关闭 + 二次确认 + 审计）。

### 4.2 AL 语义（`D-PM-12`，CNC 侧口径一致）

- `al_status` = **低 4 位状态** + **bit4 错误标志**；OP = `0x08`；OP+error = `0x18`。
- `al_status_code` 对齐 ETG（如 `0x001B` = SM watchdog）。
- `last_al_event`：`{from,to,from_code,code,watchdog,at_ms}`，`at_ms` 为帧驱动时钟毫秒（**非墙钟**，与 LCNC 日志对齐时须换算/同源打点）。
- `AL_EVENT` 与 `/events` SSE 是"事件存在性"证据；`/metrics` 的 `ecsim_al_errors_total`/`ecsim_al_error_acks_total` 是"错误出现→主站 ACK 清除"的时序证据。

### 4.3 站号与错误语义（S7）

- `?slave=N` **0-based**，合法 `0..=N-1`；缺省 = 站 0。
- 非数字/负数/空串/越界 → **HTTP 404 JSON**（不静默回退到他站，B55/`ACC-27②`）。
- 全局路由（`/topology`、`/health`）**不接受** `?slave`，带了 → 404。
- 以上是 E2E 证据可信度的前提：任何"跨站数据串台"都会让 AC-E-926 对账失真。

### 4.4 指标口径与已知缺口

- `/metrics` 的 `frames_in/out/dropped`、`datagrams_hit` 等是**模拟器侧计数器**，**不是链路质量/丢包率**；不得据此写"链路无损/实时性"结论。
- **WKC 与周期抖动**：`/metrics` 当前**无** WKC/抖动指标；WKC 仅在 `/topology` 的 `sm2/sm3` hex 与内核侧可核对 → 判据 C1 要求"须有来源（新增指标或 IgH 侧派生并文档化）"（task-103）。
- **`ack_latency_ms`**：SIM 侧无真实值（计划 `null`），由桥侧测量（判据 C1/C3 对齐）。
- DC 仅 Tier0 帧驱动时钟，**不承诺**真实同步。

---

## 5. 故障注入供给（S4，task-102）

> Lead 裁决（2026-11-15，判据文件 §11）：v1.0 E2E **P0 只保 F-LINK / F-PROC / F-SDO 三类**；**AL 运行期注入降 P1/可选**（与 BL-20 暂停冲突，不得擅自实现），AL 显示测试优先用启动参数 `--al-error` 或故障模板。P0 三类不因 AL 未做而阻塞。

### 5.1 已有（启动期，默认关闭）

| 开关 | 行为 | 观测 | 边界 |
| --- | --- | --- | --- |
| `--fault <hex>` | 需 `--behavior cia402`：注入 CiA402 Fault（`0x6041` bit3、`0x603F`=码）；单测覆盖 fault→reset 恢复 | `/pdo` 的 `0x6041` bit3、`/sdo` 的 `0x603F`、`/state` | 仅启动期一次性注入；不能运行中触发 |
| `--al-error <hex>` | 启动即置 AL Status error 位 + code（写 `0x0130`/`0x0134`），**不改变** AL 状态；主站 Error ACK 可清；只接受已知码（`0x0011/12/13/15/16/17/18/19/1B/24/25/2B`），未知码/0 拒启动（exit 2） | `/state.live.al_status`（如 `0x11`）、`al_status_code`；`/metrics` `ecsim_al_status*`；`AL_EVENT`（`watchdog=false`） | 作用于本进程**全部**模拟从站；`0x18` 等未知码 NOT-JUDGED；可作"重启后单站错误"场景 |

### 5.2 P0 故障类与规格（task-102）

| 故障类 | 触发方式（可实现） | SIM 侧可观察 | 验收要点（判据 B2） |
| --- | --- | --- | --- |
| **F-LINK** | TAP `down`/`up`（CNC 侧 root 操作，不新增 SIM API） | `frames_in` 停增；AL 迁移/`AL_EVENT`；恢复后 REF-1..3 | HMI 显示 STALE/LOST/ALARM、运动命令禁用；恢复后 3/3 OP 可继续 |
| **F-PROC** | kill/重启 `ec-sim-esc` | 进程消失→`/health` 失败；重启后 REF 复现 | 同上；恢复 ≤60s（B5） |
| **F-SDO** | 未开 `--serve-write` 的 `POST /sdo` → 405；非 `writable: true` 对象 → 403 | 响应码/JSON；`/sdo-audit`（开启写路径时） | 故障接口默认关闭、显式开启、审计（B1） |

### 5.3 P1 / 可选（不阻塞 E2E）

| 项 | 裁决/状态 | 说明 |
| --- | --- | --- |
| 单站运行期 AL 注入（`0x11`/`0x24`） | **P1/可选**（Lead 2026-11-15） | 与 BL-20（task-42 暂停）冲突；若实现须 enabled 门禁 + 二次确认 + 审计，**不得据此写 v1.0 PASS**；AL 显示测试用 `--al-error` 启动参数 |
| `POST /fault` 通用端点 | P0 三类可复用该端点；AL 类为可选参数 | 默认 405；显式开启；审计 who/时间/类型/目标/持续/前值→后值/run_id（B1） |
| DC 丢失 / 主站重启 | P1/可选（B4） | 控制面无运行期 DC 故障 API；DC 本身不承诺真实同步 |

> 约束：故障注入属"写/注入"操作 → opt-in + 确认 + 审计（HMI 侧同口径）；测试模式隔离（B6），C5 期间禁止执行。

---

## 6. 一键起停 / 健康 / 重置（S2，task-101）

- **现状**：EC-Sim 侧只有进程级起停；`GET /health` 为存活级。CNC 侧 `scripts/vm-verify/c5-stack-up.sh`（幂等：预清理→资产指纹→TAP→EC-Sim→IgH→lcec→LinuxCNC→桥→GUI，退出码 0/4）与 `c5-stack-down.sh` 是**完整栈**参照，但**不属于 EC-Sim 仓库**。
- **P0 需求（task-101）**：EC-Sim 提供 up/down/reset 脚本（只负责 **SIM 段**）：
  1. 参数：TAP、`--slaves`、manifest、`--serve` 端口；默认值 = §3.1；
  2. 起：建 TAP（root 前置检查）→ 起 `ec-sim-esc` → 轮询 `/health` + `/topology` 直到 REF-1..5 满足或超时（A2 ≤120s）；
  3. 停：幂等终止 + 清理 TAP/临时日志（A4 无残留）；
  4. 重置：停→起（≤30s 回 REF），不 reboot、不动 kiosk、不抢 VM；
  5. 输出：日志目录 + REF 断言 JSON（供 task-103 汇总）。
- **不做**：EC-Sim 侧不拉 IgH/LinuxCNC/桥/GUI（保持进程与 GPL 边界）；完整栈仍由 CNC 侧脚本编排。
- **健康定义**：`/health` 200 + REF-1..5 全过 = "SIM 段健康"；不新增深层自检字段（避免未验证语义）。

---

## 7. AC-E-926 功能一致性接口约定（核心）

### 7.1 AC-E-926 判据摘要（CNC 侧口径，原文见 `cnc-station/docs/product/CNC-ACCEPTANCE.md` §3.14）

- 范围：CNC HMI **10 页**每个 **P0 控件/功能点**都有映射行；P0 不得 UNKNOWN；未接功能必须显式 `not_available` 或禁用+原因。
- 每行五元组：**动作 → bridge 帧（`op`/字段/两段 ACK）→ LinuxCNC 可观察结果（HAL pin / `stat` / NML·日志 / 运动·DRO·报警码）→ 证据（截图 + 桥帧 log + LCNC log + 时间戳/generation）→ 结论（PASS/FAIL/BLOCKED/N-A）**。
- 硬判据：结果必须来自 **LinuxCNC 可观察量**，不得以 UI 动画/本地缓存/假值冒充；同一动作用两段 ACK 记录。
- 阶段：task-98 功能映射（CNC 侧已有首版工作件 `docs/product/assets/cnc-hmi-g1/interaction-map.json`，708 交互 / wired 462 / todo 246，**未审计**，仅作填充入口）→ task-99 接线+模拟（EC-Sim 可用，不上真机）→ task-100 机器功能验证（C5 后，按 Lead 窗口）。

### 7.2 新增第 6 列：SIM 对账量（本文件约定）

> CNC 五元组不变；本文件只在**证据**侧补"从站侧第二来源"，并明确其**从属地位**。

| 列 | 内容 | 由谁提供 |
| --- | --- | --- |
| ① 动作 | 控件/命令与参数 | CNC（task-98） |
| ② bridge 帧 | `op`/字段/两段 ACK（`SENT→DONE`） | CNC 桥日志 |
| ③ LCNC 可观察结果 | HAL pin / `stat` / NML·日志 / DRO / 报警码 | CNC |
| ④ 证据 | 截图 + 桥帧 log + LCNC log + 时间戳 | CNC |
| **⑤ SIM 对账量（新）** | 同一动作前后 `GET /pdo?slave=N` 的 `decoded` 值、`/state.live.al_status`、`/events` 或 `AL_EVENT` 行、`/metrics` 增量 | **EC-Sim** |
| ⑥ 结论 | PASS/FAIL/BLOCKED/N-A | CNC（审计） |

**对账规则（必须遵守）**：

1. **③ 是主证据，⑤ 是辅助**：⑤ 只用于证明"从站侧确实发生了对应变化"，不能替代 ③。
2. **缺 ⑤ 不自动 FAIL**（若该功能域无对应 PDO/SDO 字段，标 `N/A(SIM)` 并说明原因），但 **③ 缺失必 FAIL/BLOCKED**。
3. **⑤ 与 ③ 矛盾时以 ③ 为准**，同时记 WARN 并排查桥/时序问题（不得反过来改 LCNC 结论）。
4. ⑤ 的时间戳必须与 ②/③ 同源可对齐（`AL_EVENT.at_ms` 为帧驱动时钟；`run_id` 由 harness 关联，见 §7.5）。

### 7.3 功能域 × SIM 可观察量（10 页 P0，抽象映射）

> 与判据 C4 的 10 类（jog / MDI / 程序 / 回零 / override / tool / offset / probe / 报警 / 断链）对应；逐控件细表由 task-98 产出，本表给"能否对账"的诚实边界。

| 功能域（P0） | 典型动作 | SIM 侧对账量（⑤） | 判据 / 备注 |
| --- | --- | --- | --- |
| 状态显示（DRO/模式/状态条） | 上电、使能、模式切换 | `/pdo` 的 `0x6041` statusword、`0x6061` mode display；`/state.live.al_status` | 状态字/模式显示与 LCNC 侧一致；不得只比 UI 文本 |
| 倍率（feed/spindle override） | 倍率 ±10% | **无对应 PDO 字段** | `N/A(SIM)`；不得在 SIM 上造"倍率值"；链路存活 + `al_transitions` 增量 0 可作旁证 |
| 主轴/冷却/润滑（M3/M4/M5） | 启停主轴/冷却 | **当前场景无 IO 站（或裁剪）** | `N/A(SIM)`；需 S6 混合拓扑（P1）⚠️ 不得假装有 IO 对账 |
| 程序 AUTO | Cycle Start / Pause / Stop | `al_transitions`/`al_errors` 增量 0；PDO 周期连续；`/pdo?history=1` 变化序 | 运动类以 0x607A→0x6064 变化为主；程序语义仍由 LCNC 判 |
| MDI | MDI 行执行 | 同 AUTO（PDO 变化 + AL 稳定） | 同上 |
| EDIT/程序编辑 | 打开/保存/校验 | 无实时动作 | `N/A(SIM)`；不得注入假 PDO |
| OFFSETS | 设置/应用偏置 | 无 PDO 字段（位置影响在 LCNC 侧） | `N/A(SIM)` |
| TOOL | 换刀（虚拟轴无 ATC） | 无 | `not_available` / P1（依赖 IO/ATC 画像） |
| PROBE | 探测动作 | 若触发报警：`0x6041` bit3 + `0x603F`（`--fault`）；否则位置变化 | 报警码对账；探测逻辑本身 LCNC 判 |
| SETTINGS | 修改配置→重启栈 | 重启后 REF-1..5 复现；指纹（二进制/manifest sha256）不变 | 配置生效判据在 LCNC；SIM 只证"重启后仍 REF" |
| ALARM-DIAG | 触发 AL 错误/看门狗 | `--al-error` / SM watchdog：`last_al_event`、`AL_EVENT`、`ecsim_al_errors_total`/`ecsim_al_error_acks_total`（ACK 清零）、`sm_watchdog_triggers_total` | 报警码/时序对账；`0x18` NOT-JUDGED |
| 全局 JOG + Home | Jog 点动 / 回零 | Jog：`0x607A` 目标→`0x6064` 实际；`0x6041` bit10（target reached）；Home：`0x6060=6`（HM）、`0x6061=6`、位置变化 | 方向/符号/比例按 CSP 语义；回零结果以 LCNC 为准 |
| 断链/恢复 | 拔线/停主站/停进程 | `frames_in` 停增、AL 迁移事件、恢复后 REF | 断链翻红/恢复判据在 GUI/桥（AC-E-104/106/107/110）；故障类见 §5 |

> 表中"SIM 无对应字段"的域**明确 `N/A(SIM)`**，这是诚实清单的一部分；不得为了凑"全覆盖"而伪造对账。

### 7.4 证据格式（与判据 A7/C3 对齐）

- 每个 E2E run 一个目录：`runs/<run_id>/`，含 `run.json`（HEAD、二进制/manifest sha256、环境、命令）、`bridge.log`、`lcnc.log`、`state-<slave>.json`、`pdo-<slave>.json`、`metrics.txt`、`screenshot.png`、`result.json`（五元组 + 对账结论）。
- `state-*.json`/`pdo-*.json` 至少含：`run_id`、`slave`、`at_ms`/墙钟、`al_status`、`al_status_code`、`sm2.hex`、`sm3.hex`、`decoded`、`live.stats` 快照。
- 大文件给 sha256；证据矩阵与汇总归 task-103（EC-Sim 侧只提供原始可观察量，不写 CNC 结论）。

### 7.5 run_id（Lead 裁决：harness 侧先做）

- **定义**：`run_id` = **UUID v4**，由测试 harness 生成，导出为 `E2E_RUN_ID`，写入 `runs/<run_id>/run.json`；`run_id` 同时进入桥日志、LCNC 日志、SIM 采样 JSON、截图文件名。
- **控制面**：**不新增字段**（`state-v1` 等协议不变）；`/events?since=` 与 `run_id` 回显列为 **P1**（task-103，只增不改）。
- **对齐**：SIM 事件（`AL_EVENT`/`/events`）按时间戳与 harness 的 run 边界对齐；三处（CNC 脚本 / 证据 manifest / SIM 证据）`run_id` 必须一致（判据 C3）。

---

## 8. 与 task-101 / 102 / 103 的映射

| task | 范围 | 优先级 | 交付（EC-Sim 侧，owner=sim-impl） | 验收要点（判据） |
| --- | --- | --- | --- | --- |
| **task-101** | 固定场景 / REF / 一键起停·健康·重置 / lcec XML / runbook | **P0** | `docs/CNC-E2E-PROFILE.md` + `REF.json` + up/down/reset 脚本 + lcec XML 片段 | A1–A7（3 次 3/3 OP、REF diff=0、reset≤30s、10min soak） |
| **task-102** | 故障注入 + 审计 + 恢复（P0：F-LINK/F-PROC/F-SDO） | **P0** | `POST /fault`、`/fault/status` + 审计 + 恢复路径；默认关闭 | B1/B2/B5/B6；AL 运行期注入 P1/可选（B3 冲突） |
| **task-103** | 可观测性遥测 / 证据矩阵 / run_id | **P0** | `docs/CNC-E2E-OBSERVABILITY.md`；metrics 扩展（WKC/抖动来源）、`/events?since=`、harness 侧 run_id 关联、用例×SIM 期望矩阵 | C1–C5（10/10 类矩阵、10min SSE 无断档、三处 run_id 一致、不伪造 PASS） |
| 后续（P1） | S6 混合画像（每从站画像列表）、DC 故障扩展、控制面 run_id 字段 | P1 | 设计+实现 | 单独任务，不阻塞 P0 |

依赖顺序：S1/S3（场景+REF）→ CNC task-98 映射 → S4/S5 → CNC task-99（模拟）→ CNC task-100（C5 后上机）。P0 不依赖 HMI 设计线（暂停中）。

---

## 9. 已知缺口、不判项与边界

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 运行期 `/fault` | 🟡 待 sim-impl（task-102） | P0 三类可先由外部动作实现（TAP/进程/写路径），通用端点按需 |
| AL 运行期注入 | **P1/可选** | 与 BL-20 冲突；Lead 裁决不阻塞 E2E；`0x18` NOT-JUDGED |
| 混合画像拓扑 | ❌ 不支持 | `--slaves N` 克隆同画像；3 伺服+IO 需 P1；当前 IO 可"裁剪" |
| WKC / 周期抖动指标 | ❌ 无 | `/metrics` 无；仅在 `/topology` hex 与内核侧核对；C1 要求补来源（task-103） |
| `ack_latency_ms` | ⚠️ SIM 侧 null | 由桥侧测量；SIM 不造值 |
| DC 真实同步 | ⚠️ 不承诺 | 仅 Tier0 帧驱动时钟；不得写"DC 同步通过" |
| IgH ecrt SDO（EC-Sim VM） | ⚠️ 阻塞 | PV-01/PV-14：EC-Sim VM 上不判；CNC VM 侧由 C5 承接 |
| 证明力边界 | 事实 | SIM 只提供从站侧可观察量；**不产生** LCNC HAL pin、不替代 `stat`/NML；bridge 帧不是 LCNC 证据 |
| 认证/实时 | 禁止声称 | 不宣称一致性/CTT/认证/实时 SLA；`/metrics` 帧计数 ≠ 链路质量 |
| HMI 设计线 | HOLD | G3/G4 不启动；V2 设计层 PASS 保留，不涉本阶段验收 |

---

## 10. 协作与变更流程

- **分工（Lead 裁决 2026-11-15）**：task-101/102/103 = **sim-impl** 实现；**sim-expert** 维护判据（`docs/verification/2026-11-15-cnc-e2e-support-audit.md` v0.2）并独立审计；**sim-pm**（本文件）维护供给侧清单与 AC-E-926 接口约定；CNC 侧由 cnc-pm/cnc-impl/cnc-expert 对接。
- **需求入口**：cnc-pm / cnc-expert 提出"场景 / 可观察量 / 故障类型"需求 → 本文件给 **S 编号**（必要时 S10+）→ 映射 task → sim-impl 实现 → sim-expert 审计 → 本文更新"现状"列。
- **接口稳定性**：`state-v1`/`pdo-v1`/`topology-v1`/`metrics` 等 schema 名不变；新增字段**只增不改**（B62 先例）；破坏性变更必须升版本并在本文登记。
- **证据纪律**：PASS = `HEAD + env + date + command` + 可复现证据 + 3 次运行（判据 §2）；PENDING/PAUSED 不写成 PASS/FAIL；不写未验证 PASS；CNC 侧判据由 CNC 文档维护，本文不复制为判据。
- **VM/窗口**：C5 至 18:25；18:25–18:45 收尾归档；18:45–19:30 Lead 独占 CNC VM（G1 像素/截图）；19:30–24:00 CNC E2E 功能验证；SIM 任务在 EC-Sim VM 随时进行、不占 CNC VM；C5 运行期禁止执行故障注入（B6）。
- **与 cnc-pm 对齐（Lead 要求）**：本文 §7（AC-E-926 第 6 列 + 对账规则 + 功能域映射）即接口约定；请 cnc-pm 确认后，CNC 侧 task-98 映射表可直接引用本文件 §7.2/§7.3，避免两边各写一套。

---

## 11. 变更记录

| 日期 | 版本 | 变更 | 作者 |
| --- | --- | --- | --- |
| 2026-11-15 | v0.1 | 首版：CNC E2E 支撑期声明与暂停边界 + 供给清单 S1–S9 + REF 状态（对齐判据 A1–A7）+ 可观察量（含 WKC/抖动/`ack_latency_ms` 缺口）+ 故障注入现状/缺口（P0=F-LINK/F-PROC/F-SDO，AL 运行期 P1）+ 一键起停需求 + AC-E-926 第 6 列接口约定 + task-101/102/103 映射 + 协作/VM 窗口 | PM（sim-pm / DeepSeek Harness） |
