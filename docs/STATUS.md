# EC-Sim 当前状态快照（2026-11-13 团队班 13:00）

> 本页是**项目状态索引**，不是规范或认证结论。所有"已验证"都指仓库内脚本/单测的
> 可复现结果，**不构成**一致性测试或认证（见 `AGENTS.md` §3/§4）。

## 一句话

核心 ESC（寄存器/SM/FMMU/AL/EEPROM-SII/邮箱/watchdog）+ CoE（SDO 加速/分段/SDO Info/
Complete Access）+ CiA 402（常用模式全覆盖：PP/VL/PV/TQ/HM/IP/CSP/CSV/CST，Tier0 简化）
+ 声明式设备画像已实现；**未修改的 SOEM 2.0.0 与 IgH 1.6.13** 可发现从站并进入 OP
（IgH 无应用时到 PREOP）。测试基线 **261 passed**（另 `wasm-runtime` feature 下 plugin-host **56 单测 + 5 集成** / esc-node **77 单测 + 1 集成**：WASM 宿主 + arena + worker 线程 + 行为桥 + 仓库内真实行为画像 + B51 重启/配额 + B54 多实例接线）。**M3 = 退出候选**：Linux 验证机整轮 `run-all-probes.sh` **20 PASS / 0 FAIL / 1 SKIP**（`tap-live` 单独补跑 PASS）、本机 261 passed、许可核对完成、ADR 0002 → Accepted，见 [M3 退出清单](plan/M3-exit-checklist.md)（W6 真人浏览器点击验收待补）。**B54（2026-11-13 团队班）**：新增 `plugins/cnc-csp-servo/`（与 lcec `basic_cia402` 的 PP/CSP PDO 布局一致）+ 多实例行为插件接线 + TAP 多 CiA402 从站探针，作为 CNC 虚拟闭环 V1/V2 的 EC-Sim 侧对接包（见 [CNC-V1-integration](plan/CNC-V1-integration.md)）。**B55（同班）**：修复 ACC-27② P1——控制面 `?slave=` 统一校验（0-based；非法/越界 → 404 JSON，不再静默回退站 0），静态/UDS/TAP 三路径探针 `invalid-slave-probe.sh`。

## 代码与测试

| crate | 职责 | 测试数 |
| --- | --- | --- |
| `esc-core` | 寄存器、SM/FMMU、AL 状态机、EEPROM-SII 生成、邮箱解析、watchdog | 86 |
| `esc-node` | 从站应答器、PDI 内存、邮箱处理、CiA 402 行为、`ec-sim-esc` 二进制；WASM 行为桥（可选 feature）；链处理/1 kHz 基准示例（B46） | 73（另 `wasm-runtime` 下 73 单测 + 1 行为画像集成） |
| `plugin-host` | plugin.yaml 解析/校验、SII/字典/ESI 生成；PDO arena 同步（W2）；WASM 宿主 + 行为运行时（可选 feature） | 33 + 5 集成（另 `wasm-runtime` 下 53 + 5 集成） |
| `wire` | EtherCAT 帧/datagram 编解码 + WKC 语义 | 23 |
| `control-plane` | 只读快照 API：state-v1 / dict-v1 JSON + 路由表（`/state`、`/dict`）+ 站号校验（`validate_slave_param`，0-based，非法/越界 404） | 22 |
| `mcp-server` | MCP/Agent 桥接（B47）：JSON-RPC 2.0 stdio + 9 个只读工具（只发 GET），零第三方依赖；B50 增调用审计 JSON Lines | 18 |

`cargo fmt --check`、`cargo clippy --workspace --all-targets -D warnings`、`cargo test --workspace`
均为 0 失败；`cargo doc --workspace --no-deps` 0 警告。

## 能力清单（已实现）

- **ESC 核心**：寄存器阵列、SM 配置/状态、FMMU 逻辑寻址（LRW WKC 写+2/读+1）、
  AL 状态机（INIT↔PREOP↔SAFEOP↔OP，BOOT 拒绝、错误 ACK）、EEPROM 接口状态机、
  SII 生成（CRC-8、字符串/通用/SM/**FMMU**/TxPDO/RxPDO/**DC** 类别链、0xFFFF 终止）、
  SM watchdog（0x001B）、ESM 前置条件错误码（0x0015/0x0017/0x0018/0x0019/0x0024/
  0x0025/0x002B）、0x0410 读写语义（无 PDI 超时）。
- **线缆层**：Linux TAP（旗舰，AF_PACKET 双向）与 ESC-over-UDS shim（全平台/CI）；`--slaves N` 支持 **多从站链**（UDS shim 与 TAP 均验证 8 从站：未修改 SOEM `slaveinfo` 报 `8 slaves found`、`simple_ng` 映射 8O+8I 且 WKC 24 稳定；TAP 实测循环 ≈425 Hz，未隔离 CPU）。
- **CoE**：expedited + segmented SDO 上传/下载、SDO Info、Complete Access（上传/单帧+
  分段下载/重发幂等/超长 Abort 0x06070010）、Abort 码、PDO assign/map 对象
  （0x1C12/0x1C13/0x1600/0x1A00）、CA 下载 all-or-nothing。
- **DC Tier0**：ESC Support 声明 DC（0x0008 bit2）、0x0900–0x0917 帧驱动模拟时钟、
  SV660N 画像声明 SII DC 类别（assignActivate 0x300）。
- **CiA 402 Tier0**：常用模式全覆盖（PP/VL/PV/TQ/HM/IP/CSP/CSV/CST，均简化）、
  `--fault` 故障注入与复位；`0x6502` 按各画像布局声明（7B=0x01C1、11B=0x03CD、
  5B=0x0410、SV660N=0x03AD 公开实测）。详见 [cia402-modes-matrix.md](plugins/cia402-modes-matrix.md)。
- **设备画像**：`generic-io`、`beckhoff-el2008`、`generic-cia402-servo`、
  `generic-cia402-csv-servo`、`generic-cia402-cst-servo`、`cnc-csp-servo`（CNC 虚拟闭环 V2 CSP 布局：
  Rx 7B + Tx 11B 含 `0x606C`；均自有 VID/PID + SIMULATED），
  `inovance-sv660n` 兼容画像（默认可变 PDO 已按公开手册核对：RPDO 8B / TPDO 22B + DC）。
- **多实例行为插件（B51 后续 / B54）**：`esc-node::behavior::BehaviorBridges` 按 `--slaves N` 为每个从站
  各启动一个 WASM 行为实例（独立 arena/worker/ticker），共享一个 `BehaviorQuota`（默认上限 = N，
  `--behavior-max-instances` 可收紧，超限显式失败）；帧循环仍只 publish/poll，不把插件耗时带进帧处理。
- **插件宿主（M3-W1 spike）**：`plugin-host` 的 `wasm-runtime` feature（**默认关闭**，默认构建仍零第三方
  依赖）提供 wasmi 0.31 最小宿主——`env.read_pdo`/`env.write_pdo`/`env.log` + `tick` 导出、PDO arena
  快照、日志上限 64KiB；WAT 夹具 + `wasm-plugin-probe.sh`；release 实测实例化 605µs / tick 179.5ns。
  插件**不进入实时环**；W2 子项 1（arena 双缓冲/seqlock + generation）已完成——`plugin-host::arena`
  零撕裂并发压测 + 发布路径零分配守卫，子项 2（`plugin-host::wasm_worker`：插件独占 worker 线程 + fuel 预算 + `recv_timeout` 有界等待 + `StoreLimits` 内存上限，宿主永不 join 卡住的插件）已完成；子项 3 已完成——`plugin-host::behavior` 非实时运行时（publish/poll + ticker 线程/超时/统计）与 `esc_node::behavior::BehaviorBridge` 已接 `ec-sim-esc` 的 UDS/TAP 帧循环，manifest 支持 `behavior:`（code/tick_hz/fuel/memory/timeout）；子项 4 已完成——仓库内真实行为画像 **`plugins/behavior-counter/`**（manifest `behavior:` + 自写 `behavior.wat`）经 `wasm-behavior-probe.sh` 端到端验证：合成主站帧写 SM2（APWR 0x1100）→ worker 线程 tick → SM3 读到 SM2 + 1（`crates/esc-node/tests/behavior_counter_plugin.rs`）。**B44 原定范围已全部完成**；剩余增量项：panic 重启/多实例配额、Linux CLI UDS/TAP 端到端（验证机需 crates.io）。见 [spike-6](spikes/spike-6-wasm-host.md)。

- **性能基准（B46）**：原生 32 站 @1 kHz `chain_1khz` + `chain_latency` 帧形状（`single`/`lrw`/`fprd`）——
  32 站 PDO 式 LRW 链处理 **4.27 µs（0.43% 预算，WKC 96）**、最坏多 datagram 13.96 µs；
  Windows/验证机忙轮询 5000 轮均 0 超限；RT 忙轮询受 `sched_rt_runtime_us=950000` 节流
  （设 -1 后 0 超限，已恢复）；**不承诺 SLA**。见 [B46 文档](plan/B46-native-32slave-1khz.md)。

## 控制面与 WebUI

- 控制面（`--serve`）：只读 API（`/state`、`/dict`、`/pdo`、`/topology`、`/metrics`，支持 `?slave=N`）+ M3 写路径（`--serve-write` 下的 `POST /sdo`/`POST /pdo`、SDO 只读浏览、`/sdo-audit`）+ 可选静态 Bearer token（`--serve-token-env`，默认关闭）；零依赖 Web 页含 **SDO 面板**（`GET /sdo` 浏览 + 勾选/`confirm` 二次确认的受控写；⚠️ 无 DOM 自动化，静态回归 + 人工验收）：见 **[CONTROL-PLANE.md](CONTROL-PLANE.md)**、[web/README.md](../web/README.md)；M2 退出条件核对见 **[M2 退出清单](plan/M2-exit-checklist.md)**；M3 范围见 **[M3 范围](plan/M3-scope.md)** 与 **[ADR 0002](adr/0002-m3-plugin-host-and-write-paths.md)**；MCP/Agent 桥接（B47）**已实现只读** `ec-sim-mcp`（JSON-RPC 2.0 stdio，9 个只读工具映射，只发 GET、不含写，默认只连回环；探针 `mcp-stdio-probe.sh` 18 项断言本机/验证机 EXIT=0），设计与实现说明见 **[M3-W8 文档](plan/M3-W8-mcp-agent-bridge-stub.md)**。

## 验证证据（详见 [docs/VERIFICATION.md](VERIFICATION.md)）

- **SOEM**：`slaveinfo` 扫描身份/PDO 位宽、`simple_ng` 进 OP（WKC 3 稳定）、
  `-map` 读到 PDO 映射、SDO Info / 分段上传 / ESI→SII / UDS shim。
- **IgH 1.6.13**：`ethercat slaves` 识别身份；ecrt 应用（验证机本地夹具，不入库）
  把 8DO / CiA402 7B / CSV 11B / CST 5B 画像推入 OP，并验证状态字/模式/位置/速度/转矩。
- **合成 UDS 主站**：datagram/WKC、SM watchdog、ESM 前置条件、SDO（expedited/segmented/
  CA/Info）、CiA402 行为/CSV/CST/故障、分段下载超长 Abort。

## 已知差距（诚实清单）

| 差距 | 影响 | 现状 |
| --- | --- | --- |
| **DC 无真实同步** | SV660N 画像已声明 DC（`Has DC: 1`）；SOEM `ecx_configdc()` 与 IgH `ecrt_slave_config_dc()` 路径均已验证（`dc_rd/dc_wr>0`、仍进 OP），但只有 Tier0 帧驱动模拟时钟；无 SYNC0 脉冲/漂移补偿/传播延迟测量 | DC 寄存器+模拟时钟已实现；真实同步待硬件级仿真 |
| SV660N 固定映射位长待逐项核对 | 默认可变映射（RPDO 8B / TPDO 22B）与**全部 9 个固定映射**（0x1701..0x1705 / 0x1B01..0x1B04）均已生成映射对象；每组的条目数/位长总和与手册字节数一致（守卫测试覆盖） | 位长按 CiA 402 标准对象推算；厂商扩展对象/故障码表未收录 |
| CA 部分写/子索引顺序语义 | 主站异常 CA 序列的细节行为未覆盖 | 已做 all-or-nothing；顺序语义未实现 |
| 无独立硬件定时器 | watchdog/DC 由帧驱动，停帧不主动超时 | 设计选择，已文档化 |
| IgH SDO 上传/下载在验证机阻塞 | IgH 侧 SDO 行为只能用 SOEM/合成主站验证 | 环境问题（ecrt ioctl），⚠️ 待验证 |
| SOEM `slaveinfo -sdo` 偶发段错误 | 探针需重试规避 | 已知 SOEM 2.0.0 数组读缺陷 |
| fuzz 仅确定性 LCG | 无覆盖率引导模糊测试 | `wire`/`esc-core` 解析器与 `esc-node::handle_frame` 均有无 panic 随机测试（确定性 LCG） |

## 快速开始（本机，无 root）

```bash
cargo build --release -p esc-node
cargo run -q -p esc-node --bin ec-sim-esc -- --version          # 版本
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-sii --manifest plugins/generic-cia402-cst-servo/plugin.yaml
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-dict --manifest plugins/generic-cia402-cst-servo/plugin.yaml
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-state --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/state.json
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-dict --json --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/dict.json
python3 -m http.server 8080 --directory web   # 只读状态页（M2 起点）
cargo run -q -p esc-node --bin ec-sim-esc -- --serve 127.0.0.1:8090 --manifest plugins/generic-cia402-cst-servo/plugin.yaml   # 只读 HTTP API：/state,/dict
cargo run -q -p esc-node --bin ec-sim-esc -- --uds /tmp/ecsim.sock --serve 127.0.0.1:8091   # 实时模式：/state 含 live（AL/统计）、/pdo 过程数据、/pdo?history=1 时间序列（Web 波形）、/pdo 含 decoded 具名值、/topology 多从站摘要、/state?slave=N 与 /pdo?slave=N 逐站查看、/metrics Prometheus 文本指标（支持 ?slave=N 筛选；Web 指标卡片）、/events SSE 推送、`GET /sdo` 只读浏览、`--serve-write` 显式开启的 `POST /pdo`/`POST /sdo` 受控写（默认关闭）
EC_SIM_TOKEN='<token>' cargo run -q -p esc-node --bin ec-sim-esc -- --serve 127.0.0.1:8091 --serve-token-env EC_SIM_TOKEN   # 可选静态 token：API 需 Authorization: Bearer（空/未设置 = 现状）
cargo run -q -p esc-node --bin ec-sim-esc -- --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8091 --web-root web   # 同源托管 WebUI + API
bash scripts/spikes/uds-loopback.sh        # UDS 合成主站基础闭环
bash scripts/spikes/cia402-cst-probe.sh    # CST Tier0 行为
cargo test --workspace
```

真实主站验证（Linux 验证机，root）：见 `docs/VERIFICATION.md` 的 TAP/root 表。

## 下一步（按优先级）

1. ~~**B48（backlog 顶部）**：M3 退出条件核对与收尾~~ ✅ 2026-11-13 完成——验证机整轮 **20 PASS**、
   本机 258 passed、许可/依赖核对（默认 0 第三方；可选 25 个宽松许可）、ADR 0002 → Accepted；
   见 [M3 退出清单](plan/M3-exit-checklist.md)。**M3 = 退出候选，W6 真人浏览器点击验收待补**。
2. ~~**B49（backlog 顶部）M4 测试报告与 CI 接入**~~ ✅ 2026-11-13 完成——新增
   `scripts/spikes/gen-test-report.py`（JSON/Markdown 报告：环境/HEAD/检查/探针/证据链接，零第三方依赖）；
   CI 新增 `docs-and-probes` job（文档一致性 + 非 root UDS 探针 + 报告 artifact）；`run-all-probes.sh`
   的 Python 探测修正为 `python3 || python`（Windows/Git Bash）。CI 首跑已 **全绿**（4/4 jobs）。
3. ~~**B50（backlog 顶部）MCP 覆盖矩阵**~~ ✅ 2026-11-13 完成——`mcp-stdio-probe` 扩展至 **31 项断言**：
   token 矩阵（缺 token→AUTH_REQUIRED / 错 token→FORBIDDEN / 正确→200）、调用审计 **JSON Lines**、
   Linux 实时模式（`--uds`+`--serve`）live-only 工具端到端（topology/pdo/history/metrics/sdo_audit，
   含受控写产生审计）；本机与验证机 PASS，验证机整轮 20 PASS；`mcp-server` 单测 16→18。
4. ~~**B51（backlog 顶部）B44 增量：插件 panic 重启 + 多实例配额**~~ ✅ 2026-11-13 完成——
   `BehaviorRuntime` 在 worker 死亡（panic/Stopped）或连续失败（trap/fuel/timeout）达阈值时按模块字节
   自动重启（`max_restarts`，默认 3），预算耗尽**隔离**；新增 `BehaviorQuota` 实例配额（显式传入）；
   统计加 `restarts/quarantined`；feature 单测 **56**（+3）覆盖重启恢复/隔离/配额。
   `esc-node` 仍单实例（配额未接线，⚠️ 多实例接线属后续）。
5. ~~**B52（backlog 顶部）M4 Agent Skills 只读技能文档与示例**~~ ✅ 2026-11-13 完成——新增
   `docs/skills/`（README + `ecsim-diagnose/SKILL.md` + 静态/实时示例 `.jsonl`），把 9 个只读 MCP 工具
   映射为诊断技能（诊断步骤、错误码、只读/快照边界、归档摘要模板）；新增
   `scripts/spikes/agent-skills-check.py` 并登记进 `run-all-probes.sh`（`agent-skills`，any）——
   校验 frontmatter/只读边界/工具目录一致/示例 JSON-RPC 合法性/无 token。本机 **EXIT=0**。
6. ~~**B54（团队班）CNC 虚拟闭环对接包**~~ ✅ 2026-11-13 完成——`plugins/cnc-csp-servo/`（Rx 7B `0x6040/0x6060/0x607A`，
   Tx 11B `0x6041/0x6061/0x6064/0x606C`，`0x6502=0x01C1`，与 lcec `basic_cia402` PP/CSP 期望一致）；
   `esc-node` 多实例行为插件接线（`BehaviorBridges` 每站一实例 + 共享 `BehaviorQuota`，
   `--behavior-max-instances` 收紧）；`scripts/spikes/tap-cia402-multi-slave-probe.sh`（TAP 2 站 **EXIT=0**）；
   对接文档 [plan/CNC-V1-integration.md](plan/CNC-V1-integration.md)；release 二进制交付给 CNC 侧
   （`.local/artifacts/`，sha256 `878712fe…88b8ee`，⚠️ 不入库）。⚠️ 验证机上 csp 画像的 IgH ecrt 夹具在
   `ecrt_master_sdo_download` 内核 D 状态卡死（PV-01/PV-14 类环境问题）→ 该项仅以 SOEM/TAP 与本侧 TAP 探针为证据。
7. ~~**B55（团队班）ACC-27② P1：控制面 `?slave=` 统一校验**~~ ✅ 2026-11-13 完成——`control-plane::validate_slave_param`
   （0-based；非数字/负数/空串/越界 → 404 JSON，不再静默回退站 0）；`/state` `/pdo` `/metrics` `/events`
   `/sdo` `/dict` `/sdo-audit` + `POST /sdo`/`POST /pdo` 全部接线（静态/UDS/TAP），全局路由带 `?slave` 显式 404；
   `/sdo-audit?slave=N` 按站过滤；新增探针 `scripts/spikes/invalid-slave-probe.sh`（证书矩阵，
   登记 run-all → 注册表 **23 项**）与 2 条 control-plane 单测；CONTROL-PLANE §2.1 写明站号语义。
8. **B53（建议）DC 仿真 Tier1**：寄存器 + SYNC0/tick（帧驱动）；实现后在测试画像上验证再声明 SII DC。
   **不承诺**真实同步精度/抖动（需真机/HIL）。
7. 验证机补跑 `wasm-plugin-probe.sh` / `wasm-behavior-probe.sh` 与 Linux CLI UDS/TAP 行为插件端到端（需 crates.io，或先 vendor wasmi/wat）。
8. CA 部分写/子索引顺序语义完善；fuzz 升级。
9. 性能：TAP/UDS 闭环 32 站 @1 kHz 与 DC 下的长稳测试（B46 未覆盖部分，⚠️ 待验证）。
