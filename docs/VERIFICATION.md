# 验证脚本索引（可复现证据）

本页汇总 `scripts/spikes/` 下的验证脚本：**每个脚本验证什么、需要什么环境、最近一次结果**。
所有结论都是"用例映射 / 预回归"证据，**不构成**一致性测试或认证结论（见 `AGENTS.md` §4）。

> 环境基线：Linux 验证机（见 `docs/LINUX-SERVER.md`）上的**未修改** SOEM 2.0.0 与 IgH 1.6.13；
> IgH 的 ecrt 应用夹具只在验证机本地构建、**不入库**（GPL 边界，见 `AGENTS.md` §3）。
> 最近的完整验证状态：**2026-11-13 团队班（B54/B55 CNC 对接包 + ACC-27② P1 修复）**（详见 `WORKLOG.md` 同日条目、
> [STATUS.md](STATUS.md) 与 [M3 退出清单](plan/M3-exit-checklist.md)）；
> 探针注册表 **23 项**（B55 新增 `invalid-slave`；B54 新增 `tap-cia402`；B52 新增 `agent-skills`）：
> 本机 any 类 7 PASS / 16 SKIP；验证机整轮最近一次 **20 PASS / 0 FAIL / 1 SKIP**（B-1 复跑，
> `tap-live` 因 TAP 被占用单独补跑 PASS，见 `docs/verification/2026-11-13-ecsim-probe-registry-reconcile.md`）；
> 本机 `cargo test --workspace` **263 passed / 0 failed**（B55 +2：`validate_slave_param` 合法/非法矩阵；
> B57 +2：`al_status::is_known` 与 `force_al_error` 注入/ACK 清除）。
> `mcp-tools`/`mcp-stdio`/`agent-skills`/`tap-cia402`/`invalid-slave` 分见 B45/B47/B52/B54/B55（2026-11-13）。

## 无特权（UDS 合成主站，任意平台可跑）

| 脚本 | 验证内容 | 最近结果 |
|---|---|---|
| `uds-loopback.sh` | ESC-over-UDS 基础：BRD/APRD/APWR/FPRD/M 位、SM watchdog（10 ms → SAFEOP+ERROR 0x001B → ACK）、ESM 前置条件（0x0015/0x0017）、segmented SDO 上传；三轮（内置/`--esm-lax`/manifest 画像） | **EXIT=0** |
| `sdo-ca-probe.sh` | Complete Access：CA 上传（0x1C12/0x1600）、CA 单帧下载、CA 分段下载（140B 对象多段）、分段上传重发容错（首段重复请求幂等） | **EXIT=0**（四模式） |
| `cia402-behavior-probe.sh` | `--behavior cia402` 7 字节 PDO：状态机序列、模式门控、位置跟随/冻结 | **EXIT=0** |
| `cia402-csv-probe.sh` | 11 字节 PDO CSV：速度模式位置累加、速度显示、禁用后归零 | **EXIT=0** |
| `igh-cia402-cst-op-probe.sh` | IgH ecrt 5B CST：OP、`al_states=0x08`、`sw=0x0237`、模式 10、`0x6077=300` | **EXIT=0** |
| `cia402-cst-op-probe.sh` | CST 画像双主站：SOEM 40/40 bits、`0x6071`/`0x6077` 映射、`5O+5I` 进 OP；IgH PREOP+身份 `0xe5e2` | **EXIT=0** |
| `cia402-cst-probe.sh` | 5 字节 PDO CST：状态机序列 + 实际转矩镜像 `0x6071`→`0x6077`、禁用后归零 | **EXIT=0** |
| `cia402-ip-probe.sh` | 7B PDO IP（模式 7）：位置跟随 + Target reached | **EXIT=0** |
| `cia402-tq-probe.sh` | 5B PDO TQ（模式 4）：实际转矩镜像目标转矩，禁用后归零 | **EXIT=0** |
| `cia402-pv-probe.sh` | 11B PDO PV/VL（模式 3/2）：速度累加/速度显示，禁用后归零 | **EXIT=0** |
| `cia402-pp-probe.sh` | 7B PDO PP（模式 1）：new set-point 位置跳变 + Target reached + Set-point acknowledge | **EXIT=0** |
| `cia402-homing-probe.sh` | 7B PDO HM（模式 6）：位置归零 + Homing attained（bit12），bit4 清除后不再置位 | **EXIT=0** |
| `cia402-fault-probe.sh` | `--fault 0x7500`：Fault bit、控制字 bit7 复位、`0x603F` 回读 | **EXIT=0** |
| `cia402-modes-probe.sh` | 三个 CiA 402 画像经 SDO 上传的 `0x6502` 与布局一致（0x01C1/0x03CD/0x0410） | **EXIT=0** |
| `dc-clock-probe.sh` | 模拟 DC 时钟：0x0910 系统时间随帧前进（Tier0 帧驱动） | **EXIT=0** |
| `cia402-sdo-download-probe.sh` | 20 字节可写配置字符串 `0x2100` 的分段 SDO 下载 + 回读一致 | **EXIT=0** |

## 需要 Linux TAP + root（真实主站/内核模块）

| 脚本 | 验证内容 | 最近结果 |
|---|---|---|
| `minimal-esc-responder.sh` | SOEM 扫描到 1 个从站；IgH `ethercat slaves` 可见；单从站拓扑无报错 | **EXIT=0** |
| `soem-process-data.sh` | 未修改 SOEM `simple_ng`：进 OP、1055 轮 **WKC 3**、无 WKC 不符 | **EXIT=0** |
| `soem-mailbox-probe.sh` | SII 声明 128B/CoE 邮箱，`slaveinfo -map` 显示 mbox>0、8 位 I/O 不回归 | **EXIT=0** |
| `sdo-upload-probe.sh` | SOEM CoE 映射 `0x7000/0x6000`；IgH SDO 上传/下载（配置对象）与只读/未知拒绝 | **EXIT=0** |
| `sdo-segmented-upload-probe.sh` | 未修改 SOEM 分段上传 52 字节设备名（`0x1008`） | **EXIT=0** |
| `sdo-info-probe.sh` | SDO Info：OD list/对象描述/条目描述 | **EXIT=0** |
| `esi-sii-scan-probe.sh` | `--esi` ESI→SII：SOEM 扫描身份/PDO 正确 | **EXIT=0** |
| `manifest-slave-probe.sh` | `--manifest` 画像：SOEM 扫描身份、8 位 I/O、OP、WKC 3、`slaveinfo -sdo` | **EXIT=0** |
| `esi-manifest-op-probe.sh` | `--esi`+`--manifest` 组合：SOEM/IgH 双双正常（SOEM mapped 1O+1I、OP、WKC 3；IgH PREOP+身份） | **EXIT=0** |
| `beckhoff-el2008-probe.sh` | EL2008 兼容画像：SOEM 扫描/OP、IgH 身份 | **EXIT=0** |
| `igh-dc-op-probe.sh` | IgH `ecrt_slave_config_dc`（0x0300/1ms）+ 5B CST：仍进 OP、`al_states=0x08`、`dc_wr=8` | **EXIT=0** |
| `soem-dc-probe.sh` | 未修改 SOEM `ecx_configdc()` 路径：`Configuring distributed clock... done`、OP、WKC 3；应答器 `dc_rd/dc_wr > 0` | **EXIT=0** |
| `igh-sv660n-op-probe.sh` | IgH ecrt SV660N 画像（手册默认映射 8B/22B + DC）：OP、`al_states=0x08`、`dc_wr=8` | **EXIT=0** |
| `sv660n-capability-probe.sh` | SV660N 画像能力位复核：SOEM 64/176 bits（手册默认映射）、`CoE details=0x0d`（无 SDO Info）、`Has DC: 1`；IgH PREOP+身份 `0xe5f1` | **EXIT=0** |
| `generic-cia402-probe.sh` | 通用 CiA 402 画像：SOEM 56/56 bits、OP、WKC 3；IgH PREOP+身份 | **EXIT=0** |
| `igh-op-probe.sh` / `igh-el2008-op-probe.sh` | IgH ecrt 夹具（本地构建）配置 domain → OP（`al_states=0x08`、WKC） | **EXIT=0** |
| `igh-cia402-op-probe.sh` | IgH ecrt 7B CiA402：OP + 状态字/模式/位置（`EXPECT_PID` 可覆盖画像身份） | **EXIT=0**（V1 PP 画像，B54 复跑） |
| `igh-cia402-op-probe.sh`（`MANIFEST=plugins/cnc-csp-servo/plugin.yaml`） | IgH ecrt 在 **11B TxPDO** 的 CSP 画像上仍进 OP（身份 `0xe5e3`）；⚠️ 该 ecrt 夹具的 SDO 段在验证机阻塞（已知 ecrt ioctl 环境问题，PV-01/PV-14），完整 RESULT 断言未取到 | **OP 达成**（B54，⚠️ SDO 段阻塞） |
| `igh-cia402-csv-op-probe.sh` | IgH ecrt 11B CSV：OP + 速度模式位置累加/速度显示 | **EXIT=0** |
| `uds-multi-slave-probe.sh` | `--slaves 8` 多从站链：广播 WKC=8、逐站地址分配、配置地址寻址 | **EXIT=0** |
| `sdo-write-probe.sh` | `POST /sdo` 受控写：静态/未开开关 405；`--serve-write` 写 0x6040=0x000F 成功、`/sdo-audit` 有记录、`ecsim_sdo_writes_total=1`；0x1018→403、未知→404、长度错→400 | **EXIT=0** |
| `serve-token-probe.sh` | 可选静态 Bearer token（`--serve-token-env`）：默认/空变量维持不鉴权；非空时缺头→401（`WWW-Authenticate`）、错误 token→403、正确→200；静态资源与 `/health` 豁免；实时 `POST /sdo`、`/sdo-audit`、`/metrics`、`/events` 同保护 | **EXIT=0** |
| `invalid-slave-probe.sh` | **ACC-27②（P1）**：`?slave=` 统一校验（0-based）——静态 / 实时 UDS（`--slaves 3`，含 `POST /sdo`/`/pdo`）/ 实时 TAP（root，2 站）三路径；按站路由（`/state` `/pdo` `/metrics` `/events` `/sdo` `/dict` `/sdo-audit`）对 非数字/负数/空串/越界 一律 404 JSON，合法 0 与 N-1 一律 200，且写路由按站隔离；全局路由带 `?slave` 显式 404 | **EXIT=0**（本机静态段 PASS；UDS/TAP 段验证机 2026-11-13 团队班） |
| `al-error-probe.sh` | **B57 / task-34（AC-E-203 支持）**：`--al-error <hex>` 注入 AL Status error+码——非法码拒启动（exit 2）；Linux/UDS 实时段断言 `/state.live.al_status=0x11`（INIT|error）、`al_status_code=0x0024`、`/metrics` 的 `ecsim_al_status=17`/`ecsim_al_status_code=36`；主站 Error ACK 清错由单测覆盖 | **EXIT=0**（本机参数校验段 PASS；实时段验证机 2026-11-13 团队班） |
| `web-sdo-panel-check.py` | Web SDO 面板静态资源回归：`index.html` 控件 id（索引/子索引/读取/结果/写开关/写值/写按钮）+ token 提示；`app.js` 含 `readSdo`/`writeSdo`/`window.confirm`/405/401/403/404 提示与 `POST` 小端字节体；`node --check` 通过（无 node 时 SKIP）；⚠️ 无 DOM 自动化点击 | **EXIT=0** |
| `sdo-read-probe.sh` | `GET /sdo`：静态/实时均返回 sdo-v1（0x6040 Controlword/0x0006/16/rw），缺 index/非法/未知对象/未知子索引 → 404 | **EXIT=0** |
| `run-all-probes.sh` | 一键聚合：按平台/root 选择探针并汇总；本机 any 类 **7 PASS / 14 SKIP**（注册表 21 项），验证机 **20 PASS / 0 FAIL / 0 SKIP**（2026-11-13 B50 整轮；B52 新增 `agent-skills` 后待 CI/下一轮覆盖） | **EXIT=0**（本机/验证机） |
| `write-pdo-probe.sh` | `POST /pdo` 写操作：默认 405 且 PDO 不变；`--serve-write` 后写 slave0=beef、slave1=1234；`POST /state` 404 | **EXIT=0** |
| `events-probe.sh` | SSE `/events`：Content-Type、state-v1 + live、两事件间 frames_in 递增（0→2）、断开后 `/state` 仍 200 | **EXIT=0** |
| `serve-web-probe.sh` | `--serve --web-root web`：`/` HTML（含 B42 SDO 面板控件）、`/app.js` JS（含 `readSdo`/`writeSdo`）、`/state` JSON、未知 404；实时模式同源+live；无效 web-root 回退 API | **EXIT=0** |
| `control-plane-doc-check.py` | 文档 `docs/CONTROL-PLANE.md` 路由表/探针/源码引用与代码逐条一致（14 条路径、17 个探针） | **EXIT=0** |
| `mcp-tools-check.py` | MCP/Agent 桥接 stub 工具目录（`m3-w8-mcp-tools-v1.json`）与 `CONTROL-PLANE.md` 只读路由逐项一致：9 个 `ecsim_*` 工具全为 GET、无写工具、查询参数变体有声明、文档 GET 路由全覆盖或显式排除、设计文档/M3 清单/`run-all-probes.sh` 已互链 | **EXIT=0**（本机，2026-11-13） |
| `agent-skills-check.py` | B52 Agent Skills 静态校验：`docs/skills/` frontmatter（name/description/version/mode=read-only/tools）、工具 ⊆ MCP 目录且无写工具、正文声明只读/快照边界与稳定错误码、`examples/*.jsonl` 为合法 JSON-RPC 2.0 且无 token、相对链接可解析；已登记 `run-all-probes.sh`（`agent-skills`，any） | **EXIT=0**（本机，2026-11-13） |
| `mcp-stdio-probe.sh` | MCP/Agent 桥接端到端：真实控制面 + python 合成 MCP client（stdio JSON-RPC）——`initialize`/`tools/list` 恰好 9 个只读工具、`state`/`dict`/`sdo` 读取、错误路径、`--serve-write` 下工具表不变；**B50 扩展**：token 矩阵（缺/错/对 → 401/403/200）、调用审计 JSON Lines、Linux 实时（`--uds`）live-only 工具端到端（topology/pdo/history/metrics/sdo_audit，含受控写审计），共 **31 项断言** | **EXIT=0**（本机 + 验证机，2026-11-13） |
| `metrics-filter-probe.sh` | `/metrics?slave=1` 仅导出 1 站（全量 3 站），越界/非法 slave → 404 | **EXIT=0** |
| `metrics-probe.sh` | `/metrics` Prometheus 文本：Content-Type、`# TYPE ecsim_frames_in_total counter`、两站 label 行、发帧后计数单调递增 | **EXIT=0** |
| `slave-select-probe.sh` | 逐站 `/state?slave=N`、`/pdo?slave=N`：N=1（站地址 0x0002/SM2=beef）、N=0（SM2=0000，默认同旧行为）、越界/非法 slave → 404 | **EXIT=0** |
| `topology-probe.sh` | `/topology`：`--slaves 3` 时 count=3、初始 AL=0x01/地址 0；链上按位置配置地址 1/2/3 并写第 2 站 SM2=0xBEEF 后摘要正确（其余 0000） | **EXIT=0** |
| `pdo-decode-probe.sh` | `/pdo` 具名解码：CST 5B 布局写入后 decoded 6 项（0x6040=15、0x6060=8、0x6071=300、0x6041=0x237、0x6061=8、0x6077=-100），名称来自字典 | **EXIT=0** |
| `pdo-history-probe.sh` | `/pdo?history=1`：去重采样（无变化不增长）、写 0xBEEF→0x1234 后 count 递增且最后一条匹配、上限 256 | **EXIT=0** |
| `tap-live-state-probe.sh` | TAP 实时 `--serve`：SOEM `simple_ng` 推入 OP 后 `/state.live` AL 低 4 位=0x08、`frames_in>0`，`/pdo` 返回 pdo-v1（root，验证机） | **EXIT=0** |
| `pdo-live-probe.sh` | `--serve` 实时 `/pdo`：初始 SM2/SM3 全 0、AL=0x01；UDS APWR 写 0x1100=0xBEEF/0x1110=0x1234 后 `/pdo` 实时反映 `beef`/`1234` | **EXIT=0** |
| `live-state-probe.sh` | `--serve` 实时模式（UDS+HTTP 同进程）：`/state.live.al_status` INIT→PREOP（0x01→0x02）、`stats.frames_in>=1` | **EXIT=0** |
| `http-state-probe.sh` | `--serve` 只读 HTTP API：`/state`/`/dict`/`/health` 200（CORS、Content-Type、no-store）、未知路径 404；无 manifest 时 `/dict` 404 | **EXIT=0** |
| `dump-dict-probe.sh` | `--dump-dict --json` 输出合法 dict-v1（2 个画像）：0x6502 值（0x0410/0x3AD 小端）、0x6040=rw/16；纯文本 dump 向后兼容 | **EXIT=0** |
| `dump-state-probe.sh` | `--dump-state`（2 个 manifest 画像 + 内置画像）输出合法 state-v1 JSON：身份/SM/PDO/DC/字典对象数；`web/` 资源存在 | **EXIT=0** |
| `podman-unprivileged-probe.sh` | 无特权容器（podman，无 `--privileged/--network=host`）内跑 `--slaves 8` UDS shim：容器内启动、广播 WKC/地址分配、UDS 全链路通过 | **EXIT=0** |
| `tap-multi-slave-probe.sh` | TAP 上 N 从站链（默认 8；B45 用 `N=32`）：SOEM `slaveinfo` 报 N 站/WKC N*3、`simple_ng` 映射 NO+NI 且 WKC 全对；**B45 起统计每轮往返时间**（不再用 iterations/T 估频）：8 站 avg 164 µs、32 站 avg 167 µs；32 站 roundtrip <1 ms | **EXIT=0**（验证机，2026-11-13） |
| `chain_latency`（example） | ESC 链处理微基准（B45：1..64 从站+边际成本；B46：帧形状 `single`/`lrw`/`fprd`）。`single`：本机 1/8/32/64 = 0.133/0.760/2.938/5.853 µs，验证机 = 0.100/0.475/1.720/3.433 µs（≈52 ns/站）；`lrw`（真实 PDO 形状，每站 4B 出+4B 入）：32 站 4.270 µs（WKC 96）、64 站 8.736 µs（WKC 192）；`fprd`（最坏多 datagram）：32 站 13.955 µs、64 站 47.613 µs（O(N²)） | `cargo run --release -p esc-node --example chain_latency -- 20000 1,8,32,64 single,lrw,fprd` |
| `chain_1khz`（example，B46） | 原生 32 站 @1 kHz 周期基准（`hybrid`/`sleep`/`spin`，Linux 可选 `SCHED_FIFO`）：Windows 32 站 LRW hybrid 帧处理 p50 5.0/p99 17.9 µs、**0/5000 超限**；验证机隔离 CPU 忙轮询 **0/5000 超限**；`hybrid --rt` 0.10–0.12% 超限（VMware 宿主抢占 tail ≈7 ms）；RT 忙轮询被 `sched_rt_runtime_us=950000` 节流 2.21% 超限（临时 -1 后 0/5000）；WKC 96 全轮校验 | `cargo run --release -p esc-node --example chain_1khz -- 32 10 hybrid lrw`；详见 [B46](plan/B46-native-32slave-1khz.md) |
| `uds_latency`（example，原生 Rust） | UDS shim 往返：默认阻塞 p50≈93µs/p99≈157µs；sim `--poll` + 探测 `spin` 忙轮询 p50≈3.7µs/p99≈9.3µs（1 站）、8 站 p99≈14.6µs | `cargo run --release -p esc-node --example uds_latency -- <sock> 5000 spin` |
| `shim-latency.sh` | UDS shim 延迟基准（支持 SIM_CPU/PROBE_CPU/RT）：未绑核 p50≈111µs/p99≈196µs；`isolcpus=2,3` + RT（sim CPU2 `-f 50`、probe CPU3 `-f 80`）p50≈101µs/p99≈172µs；8 从站 RT p99≈181µs | **EXIT=0** |
| `soem-multi-slave-probe.sh` | 未修改 SOEM 经 UDS shim 数到 **8 从站**并 OP：`8 slaves found`、`mapped 8O+8I`、WKC 24 | **EXIT=0** |
| `soem-uds-probe.sh` | SOEM over UDS shim 的探索性夹具（以脚本内注记状态为准） | 见脚本注记 |
| `wasm-plugin-probe.sh` | WASM 行为插件宿主（B43，可选 feature `wasm-runtime`，wasmi 0.31）：`read_pdo`/`write_pdo`/`log` 契约、1 万次 `tick` 不变式 | **EXIT=0**（本机）；⚠️ 验证机未跑（首次构建需 crates.io） |
| `wasm-behavior-probe.sh` | 仓库内真实行为画像（B44 W2 子项 4）：`plugins/behavior-counter/`（manifest `behavior:` + 自写 WAT）→ 合成主站帧 APWR `0x1100` / APRD `0x1110` → SM3 = SM2 + 1；并跑 `plugin_manifests` 守卫 | **EXIT=0**（本机，2026-11-13）；⚠️ 验证机未跑（`wasm-runtime` 需 crates.io）；CLI UDS/TAP 端到端待验证 |

> `wasm-plugin-probe.sh` / `wasm-behavior-probe.sh` **不进入** `run-all-probes.sh` 门禁：它们需要
> 可选 feature 与 crates.io 拉取依赖（离线应 SKIP），属 M3-W1/W2 证据；手动运行见
> [spike-6](spikes/spike-6-wasm-host.md) 与 `plugins/behavior-counter/README.md`。

## 测试报告与 CI（B49 / M4）

| 脚本/配置 | 作用 | 最近结果 |
|---|---|---|
| `scripts/spikes/gen-test-report.py` | 一次生成 JSON + Markdown 测试报告：环境/HEAD/检查/探针/证据链接；零第三方依赖、跨平台；默认写 `target/test-reports/`（不入库） | 本机运行 **EXIT=0**（cargo fmt/clippy/test/doc + 探针；2026-11-13）；⚠️ CI 首次运行待观察 |
| `.github/workflows/ci.yml` → `docs-and-probes` job | Ubuntu 非 root：文档一致性（`control-plane-doc-check`/`mcp-tools-check`）+ `run-all-probes.sh`（UDS 类）+ 上传报告 artifact | 新增（⚠️ 推送后首次 CI 运行待验证；TAP/root 项由验证机覆盖） |

> 报告只记录"用例映射/预回归"，**不构成**认证；SKIP 表示该环境不适用（如非 root 跳过 TAP）。

## 辅助工具

| 文件 | 用途 |
|---|---|
| `uds_master_probe.py` | 合成主站（UDS）：基础 datagram、SM watchdog、ESM、SDO（expedited/segmented/CA/Info）、CiA402 行为/CSV/CST/故障注入 |
| `sniff_ecat.py` | 抓包/解析 EtherCAT 帧（调试） |
| `tap_ethernet_loopback.py` / `tap_hold.py` | TAP 自环/保持夹具（调试） |

## 运行提示

- UDS 类脚本不需要 root、不需要内核模块，适合 CI；TAP/IgH 类需要 root 与 `ec_master`/`ec_generic`。
- IgH 的 ecrt 应用夹具（`ecrt-*`）在验证机 `/opt/ecsim/work/igh-op/` 本地构建后由脚本使用，
  **源码与二进制均不入库**；脚本在夹具缺失时会给出明确 FAIL。
- 验证机状态可能被前次实验弄乱（模块占用、残留 TAP）；脚本自带清理，必要时重启验证机。
