# M3 退出核对清单（2026-11-13）

> 目的：按 [M3 范围](M3-scope.md) §5 逐条核对 **M3「插件宿主 + 写路径 + 可运维性」** 的退出条件，
> 给出「已达成 / 部分 / 未达成」与可复现证据。**不是认证或承诺**；所有结论是**用例映射 / 预回归**，
> 不构成 EtherCAT 一致性测试（CTT）或任何认证（`AGENTS.md` §4）。
> 证据基线：EC-Sim HEAD `d7089da`（v0.0.1-pre-alpha）；Linux 验证机（openEuler 24.03 SP4 / 6.6 RT）；
> 本机 Windows + Rust 1.97.1；主站夹具 IgH 1.6.13 / SOEM 2.0.0（未修改）。
> 关联：[M3 范围](M3-scope.md)、[ADR 0002](../adr/0002-m3-plugin-host-and-write-paths.md)、
> [STATUS](../STATUS.md)、[VERIFICATION](../VERIFICATION.md)、[CONTROL-PLANE](../CONTROL-PLANE.md)。

## 1. 结论摘要

- **M3 = 退出候选（Exit Candidate）**：W1–W8 全部完成且证据齐；五项退出条件中 **4 项达成、1 项部分达成**。
- **唯一部分项**：W6 的**真实浏览器人工点击验收**（SDO 面板二次确认写入）尚未由真人执行。
  - 机器可检部分已全绿：`web-sdo-panel` 静态资源回归 PASS（本次整轮探针）；
  - 本班补充了**无头 Edge 渲染烟测**（截图，1280×900）：页面加载、SDO 面板、token 提示均正常渲染；
    发现一处低危缺陷（载入区重复渲染，见 §6）——属 HMI 范围，**按冻结规则未修改 `web/`**。
- **回归证据**：Linux 验证机整轮 `run-all-probes.sh` **20 PASS / 0 FAIL / 0 SKIP**（含 `mcp-tools`、`mcp-stdio`、`tap-live`）；
  本机 `cargo fmt/clippy/doc` 0 警告、`cargo test --workspace` **256 passed / 0 failed**。
- **ADR 0002 状态**：由"提议"更新为 **Accepted**（D1–D6 均有实现/证据；D5 许可核对见 §5）。
- **建议**：M3 可标记为"退出候选"，待 W6 真人点击验收补记后正式关闭；不阻塞后续 B44 增量与 M4。

## 2. 退出条件逐条核对（M3-scope §5）

| # | 退出条件（原文摘要） | 判定 | 证据 |
|---|---|---|---|
| 1 | W1–W5 完成且各有探针/单测；W6 人工验收通过；W7/W8 至少产出文档 | **部分** | W1–W5、W7、W8 全达成（§3）；W6：静态回归 + 渲染烟测 PASS，**真人点击未做** ⚠️（§6） |
| 2 | 默认配置保持只读（写/token/插件均需显式开启） | **达成** | 默认构建零第三方依赖且 `wasm-runtime` 默认关闭；`write-pdo`/`write-sdo`/`serve-token`/`mcp-stdio` 探针 PASS；`POST` 默认 405、写仅 `--serve-write` + 白名单、token 空=不鉴权 |
| 3 | `run-all-probes.sh` 在 Linux root 下全绿（含新增探针）；`control-plane-doc-check.py` 覆盖新路由 | **达成** | 验证机整轮 **20 PASS / 0 FAIL / 0 SKIP**；`doc-check`、`mcp-tools`、`mcp-stdio`、`tap-live` 均 PASS（§4） |
| 4 | 无 GPL/LGPL 代码进核心；无未声明依赖；许可与 NOTICE 复核 | **达成** | 默认依赖树 **0 个第三方 crate**；`wasm-runtime` 可选依赖 25 个，全部宽松许可（MIT / Apache-2.0 / Apache-2.0 WITH LLVM-exception / Unlicense OR MIT）；无 NOTICE 文件且默认零第三方依赖 → 无第三方 NOTICE 义务（§5） |
| 5 | 文档：ADR 0002 更新为 Accepted、CONTROL-PLANE/STATUS/M3 清单互链 | **达成** | ADR 0002 状态已更新；本清单与 [M3 范围](M3-scope.md)/[STATUS](../STATUS.md)/[CONTROL-PLANE](../CONTROL-PLANE.md)/[VERIFICATION](../VERIFICATION.md) 互链 |

## 3. W1–W8 证据索引

| # | 工作流 | 交付 | 状态 | 证据（可复现） |
|---|---|---|---|---|
| W1 | WASM 插件宿主 spike | wasmi 0.31.2 选型 + 最小宿主 | ✅ | [spike-6](../spikes/spike-6-wasm-host.md)；`wasm-plugin-probe.sh`（本机 EXIT=0） |
| W2 | PDO arena 同步 | 双缓冲/seqlock + generation；worker 线程 + fuel/超时/内存上限；行为桥；真实画像 | ✅ | `plugin-host::arena` 单测 + 2 万次并发零撕裂；`wasm-behavior-probe.sh`（本机 EXIT=0）；[spike-6](../spikes/spike-6-wasm-host.md) |
| W3 | SDO 只读浏览 | `GET /sdo` | ✅ | `sdo-read-probe.sh`（整轮 PASS）；dict-v1/sdo-v1 schema |
| W4 | SDO 受控写 | `POST /sdo` + 白名单 + 审计 | ✅ | `sdo-write-probe.sh`（整轮 PASS，含默认拒绝/白名单/403/审计） |
| W5 | 可选静态 token | `--serve-token-env` | ✅ | `serve-token-probe.sh`（整轮 PASS，200/401/403/静态豁免） |
| W6 | Web SDO 面板 | 只读浏览 + 二次确认写 | **部分**（机器项 PASS；真人点击 ⚠️） | `web-sdo-panel-check.py` + `node --check`（整轮 PASS）；§6 渲染烟测 |
| W7 | 32 从站 @1 kHz 调研 | 瓶颈记录 | ✅ | [M3-W7](M3-W7-32slave-1khz-bottleneck.md)；B46 基准 [B46](B46-native-32slave-1khz.md) |
| W8 | MCP/Agent 桥接 | 只读 9 工具 + stdio 实现 | ✅ | [M3-W8](M3-W8-mcp-agent-bridge-stub.md)；`mcp-tools`/`mcp-stdio`（整轮 PASS，18 项断言） |

## 4. 回归证据（2026-11-13 本班）

### 4.1 Linux 验证机整轮探针（root；含 TAP）

```text
$ bash scripts/spikes/run-all-probes.sh        # 验证机，root，20 项注册表
PASS doc-check / mcp-tools / http-state / live-state / pdo-live / pdo-history /
     pdo-decode / topology / slave-select / metrics / metrics-filter / serve-web /
     web-sdo-panel / events / write-pdo / sdo-read / sdo-write / serve-token /
     mcp-stdio / tap-live
== 汇总：20 PASS / 0 FAIL / 0 SKIP（共 20）==
RESULT: PASS
```

> 上一轮（B47 前）为 18 PASS；本次补齐 `mcp-tools`/`mcp-stdio` 后整轮 20 项，TAP 项（`tap-live`）
> 在 root 下执行。日志目录 `/tmp/ec-sim-probes/`（验证机本地，不入库）。

### 4.2 本机静态检查与单测

```text
cargo fmt --all -- --check              # 0（EXIT=0）
cargo clippy --workspace --all-targets -- -D warnings   # 0（EXIT=0）
cargo test --workspace                  # 256 passed / 0 failed（EXIT=0）
cargo doc --workspace --no-deps         # 0 警告（EXIT=0）
```

> 测试基线 256 与 [STATUS](../STATUS.md) 一致，未回退；`wasm-runtime` feature 单测不在默认门禁（需 crates.io，
> 验证机离线不可构建，见 §7）。

## 5. 许可与依赖核对

- **默认构建（CI 门禁）**：`cargo tree --workspace --edges normal` 仅 workspace 成员 → **0 个第三方 crate**；
  `cargo metadata` 第三方包计数 = **0**。
- **可选 `wasm-runtime` feature**（默认关闭）：25 个第三方 crate，均为宽松许可：
  - MIT / Apache-2.0 / MIT OR Apache-2.0：`wasmi 0.31.2`、`wasmi_arena`、`wasmi_core`、`libm`、`spin`、
    `bitflags`、`hashbrown`、`indexmap`、`indexmap-nostd`、`smallvec`、`paste`、`semver`、`autocfg`、
    `equivalent`、`num-traits`、`unicode-width`、`bumpalo`、`downcast-rs`、`leb128fmt`、`memchr`（Unlicense OR MIT）；
  - Apache-2.0 WITH LLVM-exception（可再选 Apache-2.0/MIT）：`wasmparser 0.263`、`wasmparser-nostd`、
    `wasm-encoder`、`wast`、`wat`。该许可变体为**更宽松的 Apache-2.0 例外**，ADR 0002 D5 已将其列入
    候选并记录"逐项核对"；本清单完成逐项核对，未发现 GPL/LGPL/AGPL。
- **GPL 边界**：核心目录无 GPL/LGPL 代码；IgH/SOEM/LinuxCNC 仅验证机夹具/只读参考（`AGENTS.md` §3）。
- **NOTICE**：仓库无 `NOTICE` 文件；默认零第三方依赖，无需分发第三方 NOTICE。可选依赖的 NOTICE 义务
  随 `wasm-runtime` 发布前复核（⚠️ 发布清单项，不影响默认构建）。
- **许可文件**：`LICENSE`、`LICENSE-APACHE`、`LICENSE-MIT` 齐全。

## 6. W6 浏览器验收记录（部分完成）

| 项 | 结果 | 证据 |
|---|---|---|
| 静态资源/控件契约/关键安全标记 | ✅ PASS | `web-sdo-panel-check.py`（整轮探针 PASS）；`node --check web/app.js` |
| 页面加载与渲染（无头 Edge，1280×900，静态 `--serve --web-root web`） | ✅ PASS | 本班截图：标题/载入区/SDO 面板/token 提示/页脚免责声明均按暗色基线渲染，无白屏/无 JS 异常 |
| SDO 只读浏览（真实浏览器点击"读取"） | ⚠️ 未做 | 需交互点击；静态资源回归 + API 探针（`sdo-read`）已覆盖链路 |
| SDO 受控写二次确认（勾选→写入→`confirm`） | ⚠️ 未做 | 需真人/浏览器自动化；受控写服务端链路已由 `sdo-write` 探针覆盖 |
| token 模式提示 | ✅ 渲染正常 | 截图含"浏览器 fetch 无法自动携带 Authorization"提示 |

**渲染烟测发现（低危，已登记，未修）**：静态 HTML 的"载入状态"面板与 `app.js` 的"未载入"空态各渲染一次，
页面出现**两个同名"载入状态"区块**（截图可复现）。属 HMI 实现范围；**Web HMI 实现冻结中**
（`docs/team/roles.md` §2、ECSIM-PRD D-PM-06），本班不修改 `web/`，登记给 HMI-DEV GO 后的实现任务。

> W6 关闭判据：真人在 Chrome/Edge 完成一次"读取 SDO → 勾选允许写入 → 写入 → confirm 取消/确认"，
> 记录浏览器/时间/结果；或 HMI 实现后补 DOM 自动化。**在此之前 W6 记"部分"，M3 不宣布正式关闭。**

## 7. 已知遗留与非目标（不阻塞 M3 退出候选）

| 项 | 状态 | 说明 |
|---|---|---|
| W6 真人点击验收 | ⚠️ 待人工 | 见 §6 |
| `wasm-plugin-probe`/`wasm-behavior-probe` 在验证机运行 | ⚠️ 未跑 | 验证机离线无 crates.io，`--features wasm-runtime` 构建需拉取依赖（本班已确认离线失败）；本机 EXIT=0 已作为 W1/W2 证据 |
| 插件 panic 重启 / 多实例配额 | ⚠️ 增量项 | B44 原范围外；不影响 M3 退出条件 |
| Linux CLI UDS/TAP 行为插件端到端 | ⚠️ 未覆盖 | 同上，`wasm-runtime` 依赖获取后补 |
| MCP live-only 工具端到端 / token 矩阵 | ⚠️ 未覆盖 | B47 已登记的覆盖缺口；M4 侧补 |
| 认证/CTT/实时 SLA | 非目标 | 任何版本不宣称（`AGENTS.md` §4） |

## 8. 复现命令（脱敏）

```bash
# Linux 验证机（root）：整轮预回归
cargo build --release -p esc-node -p mcp-server
bash scripts/spikes/run-all-probes.sh

# 本机：静态检查 + 单测 + 文档
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps

# 许可/依赖（本机）
cargo tree --workspace --edges normal
cargo metadata --format-version 1 --offline          # 第三方包计数
cargo metadata --format-version 1 --offline --features esc-node/wasm-runtime

# W6 渲染烟测（示例；无头 Edge）
ec-sim-esc --serve 127.0.0.1:18090 --web-root web --manifest plugins/generic-cia402-cst-servo/plugin.yaml
msedge --headless=new --window-size=1280,900 --virtual-time-budget=5000 \
  --screenshot=w6.png http://127.0.0.1:18090/
```

## 9. 结论与交接

1. **M3 退出候选**：W1–W8 完成，回归 20/20、单测 256、许可核对完成；ADR 0002 → Accepted。
2. **关闭前唯一动作**：补 W6 真人浏览器点击验收（§6），随后可在本清单标记 M3 正式关闭。
3. **建议下一步**：
   - 实现侧（EC-Sim）：B44 剩余增量（插件 panic 重启、多实例配额）；M4 范围（测试报告产物、CI 接入、
     Agent Skills 文档、MCP live-only 覆盖）；
   - 文档侧：`docs/ECSIM-ACCEPTANCE.md`（CNC 角色团队）与 M3/M4 状态同步。
4. 本清单只写"用例映射/预回归"，**不宣称一致性/认证/实时保证**。

## 10. 变更记录

| 日期 | 版本 | 变更 | 作者 |
|---|---|---|---|
| 2026-11-13 | v0.1 | 首版：M3 五项退出条件逐条核对；W1–W8 证据索引；验证机 20/20 整轮探针；本机 256 单测；许可/依赖核对（默认 0 第三方、可选 25 个宽松许可）；W6 部分完成（静态回归 + 无头渲染烟测 + 真人点击待补）；ADR 0002 → Accepted；遗留项与复现命令 | DeepSeek Harness |
