# AGENTS.md — 本仓库的 AI 协作与提交约定

> 本文件是**任何 agent（含每日低峰期自动开工的夜班 agent）进入本仓库的第一读物**。
> 读完本文件 → 读 `WORKLOG.md` 最近 3 条 → 读 `ethercat-slave-simulator-tech-selection.md` 的 §2（线缆层）、§9（路线图）、§11（待拍板决策）。

## 0. 仓库位置（2026-11-12 迁移）

- **主仓库：`D:\tokengence.ai\ec-sim`**（本目录）。后续所有开发、提交、夜班任务都以这里为准。
- 旧位置 `<legacy-workspace>` 里还留有一份**同历史的副本**：那是并行会话（`<other-project>/` 项目，未纳入本仓库）正在使用的共享工作区。迁移完成前**不要删除旧副本**，也不要向旧副本提交本项目的改动（会造成分叉）。
- 迁移方式：`git clone` 旧工作区 → 本目录，提交历史与作者身份完整保留；未跟踪的 `<other-project>/` 未被带入。

## 1. 项目身份

- 项目：EtherCAT 从站模拟器（工作名 **EC-Sim**，最终产品名未定；**产品名与域名不得包含 "EtherCAT"**）。
- 定位：**虚拟总线 + 插件宿主 + AI 可操作的测试基础设施**，不是又一个协议栈。
- 核心设计约束（不可违背）：
  1. ESC 层（寄存器/FMMU/SM/AL/EEPROM-SII/邮箱/DC/watchdog）**内置**，插件只做"对象字典声明 + 应用层逻辑"。
  2. 插件**永不进入实时环**；PDO 数据契约放核心持有的共享内存 arena。
  3. 线缆层可插拔：Linux TAP（旗舰）与 ESC-over-UDS shim（全平台/CI）共用同一个 ESC 内核。
  4. 双主站开箱：未修改的 IgH 与 SOEM 都要能发现并进入 OP。

## 2. 提交身份（硬性）

本仓库 `git config user.name` 已固定为：

```
user.name  = DeepSeek Harness
user.email = deepseek-harness@users.noreply.github.com
```

- **任何 agent 提交时不得修改、覆盖或临时改写这两个值**；不得用 `--author` 伪造其他身份。
- 提交信息格式：`<type>(<scope>): <中文或英文简述>`
  - type ∈ `feat | fix | docs | test | refactor | chore | spike`
  - scope 例：`esc-core`、`wire`、`coe`、`plugin`、`web`、`ci`、`docs`
  - 例：`feat(esc-core): 实现 AL 状态机 INIT→PREOP 握手与错误位 ACK`
- 每个提交必须**自洽可构建**；不允许"半成品提交 + 下一条修"。

## 3. 许可红线（写代码前必须检查）

- 核心（本仓库根目录所有代码）许可：**Apache-2.0 OR MIT**。新增依赖只允许 Apache-2.0 / MIT / BSD / ISC / Zlib。
- **禁止**引入：GPLv2/GPLv3/AGPL/LGPL 代码、以及 CeCILL-C（KickCAT）代码进核心目录。
- SOEM（GPLv3/商业）、IgH（GPLv2 内核模块）、LinuxCNC（GPLv2）、SOES（GPLv2+文件级例外）只能作为**独立进程/独立容器**的测试夹具，或作为**只读参考**；不得链接、不得复制代码片段。
- 不随仓库分发第三方厂商 ESI 文件。

## 4. 真实性红线

- 不得声称"通过 EtherCAT 一致性测试 / CTT 认证 / certified"；只能写"用例映射、预回归"。
- 未经验证的结论必须标 `⚠️ 待验证`，并给出验证方法。
- 规范细节以 ESC Sec.II 手册、ETG.1000 为准；落地前逐项复核，不凭记忆写寄存器地址。

## 5. 每日自动开工协议（班次 agent 专用）

### 5.0 低峰/高峰口径与排班（2026-11-12 用户确认）

- **高峰**：北京时间周一至周五（**不含中国法定节假日**）**09:00–12:00、14:00–18:00**。
- **低峰**：其余所有时间 —— 工作日 00:00–09:00、12:00–14:00、18:00–24:00；**周末与法定节假日全天**。
- **高峰时段严禁工作**；班次必须完整落在低峰窗口内。
- 当前排班：
  - 每日 8 班：`00:30 / 02:30 / 04:30 / 06:30 / 12:30 / 18:30 / 20:30 / 22:30`（每班 ≤90 分钟）
  - 周末补充 2 班：`10:00 / 15:00`
- 排班原则：**只在"普通工作日也属于低峰"的时刻排班** → 法定节假日无需单独日历，天然安全（节假日全天低峰）。法定节假日落在工作日时，周末补充班不触发（保守）。

每次被定时任务唤醒时，按以下顺序执行，**一次只推进一件事**：

1. 读 `WORKLOG.md` 顶部条目与"下次开工第一件事"。
2. 从 §6 的 backlog 里取**顶部未完成项**；若顶部项被阻塞，取下一个并在 WORKLOG 记录原因。
3. 实现 → 本地验证（`cargo test` / `cargo fmt --check` / `cargo clippy`，没有代码时跳过）。
4. 更新 `WORKLOG.md`：在顶部插入一条，含 `日期 / 做了什么 / 验证结果 / 卡点 / 下次开工第一件事`；完成项从 §6 移入"已完成"并补下一项。
5. **单次提交**（提交信息含 WORKLOG 变更）。
6. **自动推送**：`git push`（`origin main`，normal fast-forward）。失败重试一次；仍失败则把原因写进 WORKLOG 的"卡点"并正常退出——**不得 `--force`，不得为推送而改写历史**。
7. 结束时给出一段 ≤10 行的中文总结（做了什么、验证了什么、是否已推送、下一步）。

**禁止行为**：不要一次跨多个 backlog 项；不要为了"看起来有产出"而写空壳代码或假测试；不要在未验证的情况下更新 §6 的完成状态。

## 6. 当前 Backlog（按优先级，顶部先做）

- [x] ~~**B1 Rust workspace 骨架**~~ ✅ **2026-11-12 完成**：6 个成员 crate 骨架（`esc-core`/`wire`/`coe`/`plugin-host`/`control-plane`/`cli`）+ 三平台 CI 全绿。证据：commit `93a2fbf`，CI run `43158276907`（ubuntu/macos/windows 的 `fmt --check`/`clippy -D warnings`/`test` 均 success）。
- [ ] **B2b 最小 ESC 应答器（Python/Rust 原型均可，先验证协议）**：在服务器上让持 TAP 的进程响应 `BRD 0x0000`、`APWR 0x0010`、`APRD 0x0130`，目标 = IgH `ethercat slaves` 与 SOEM `slaveinfo` 都看到 **1 个从站**（尚未要求进 OP）。产出结论写入 `docs/spikes/`。
- [x] ~~**B2 Spike 1（Linux TAP）**~~ ✅ **2026-11-12 完成**：TAP 双向承载 0x88A4，且**未修改的 SOEM 与 IgH 都能绑定 TAP 并发出真实帧**（IgH `ec_generic` 按 MAC 匹配 TAP）。证据：[spike-1](docs/spikes/spike-1-tap-bidirectional.md)、[spike-2](docs/spikes/spike-2-masters-on-tap.md)。
- [ ] **B3 Spike 4（SOEM oshw shim）**：把 SOEM 的 `oshw` 收发包替换成 UDS，跑通 `ec_config_init` + `ec_config_map`（SOEM 以独立进程/容器方式使用）。
- [ ] **B4 `wire` crate**：EtherCAT 帧/datagram 编解码 + 严格 WKC 语义 + 一帧多 datagram（M 位）+ 地址匹配（ADP/FPRD/BRD/LRD），单元测试覆盖两主站的期望行为。
- [ ] **B5 `esc-core` 最小 ESC**：寄存器阵列 + SM/FMMU + AL 状态机 INIT/PREOP 握手 + EEPROM 接口状态机（Busy/8 字节能力位）。
- [ ] **B6 SII 生成器**：ESI XML → SII 镜像（含 0x003F CRC-8、category 链 0x0029/0x0032/0x0033、0xFFFF 终止）。
- [ ] **B7 CoE 最小闭环**：加速 SDO 上传/下载 + 0x1C12/0x1C13/0x1600/0x1A00 + Abort 码；先**不做** SDO Info 与 Complete Access（SII 对应位清 0）。
- [ ] **B9 声明式插件加载器**：读取 `plugins/<id>/plugin.yaml` → 生成 SII + 对象字典 + 默认 PDO 分配 + SM/FMMU 布局（不依赖行为代码），并在 IgH/SOEM 上扫描到该从站。
- [ ] **B10 首个 P0 设备插件**：Beckhoff EL2008 兼容 8DO（声明式），完成 `facts.md`（含来源 URL）、manifest、双主站扫描验证记录（`docs/plugins/`）。
- [ ] **B11 CiA402 行为插件**：以 Inovance SV660N 兼容画像为第一个目标，实现 CSP 状态机（Tier0/内置 profile 起步）。
- [ ] **B8 双主站冒烟**：IgH `ethercat slaves` 与 SOEM `slaveinfo` 零补丁看到 OP，输出可复现脚本与截图。

## 7. 目录约定

```
crates/         Rust workspace（核心可执行与库）
docs/           设计、ADR、spike 结论
docs/spikes/    每个 spike 一篇结论（方法/结论/证据/是否推翻假设）
web/            前端（M2 起）
tests/          跨 crate 集成测试（不含 GPL 夹具）
tests-gpl/      只跑 GPL 主站的夹具（独立目录、独立许可、不进发行物）
```

## 8. 运行环境（不入库）

- 服务器地址、账号、代理、端口等**运维信息不写入仓库**（本仓库后续会开源），统一放在本地 `.local/ops.md`（已被 `.gitignore` 排除）。
- 仓库内提到远端时只用中性说法（"Linux 验证机""远端"），不写主机名/IP/端口/账号/代理地址；WORKLOG 记录验证结果时同样脱敏。
- 需要 TAP / AF_PACKET / SOEM / IgH 等 Linux 能力时，在远端验证机执行，把命令要点与关键输出（脱敏）写进 WORKLOG。
- 不要把 IgH/SOEM 源码或二进制提交进本仓库。

## 9. 设备插件与互联网调研

- **允许联网检索**知名品牌电机/IO/编码器等的**公开文档**，用于制作"兼容仿真"插件；但必须遵守 **[docs/DEVICE-PLUGINS.md](docs/DEVICE-PLUGINS.md)** 的 R1–R5 红线：
  - R1 不把厂商 ESI/PDF/固件/示例代码放进仓库或插件包（只提取接口事实 + 记来源 URL）；
  - R2 设备身份（0x1018/ESI 名称）用**我方自有 Vendor ID/PID**，不用厂商 VID/PID/Logo；
  - R3 不宣称"完全一致 / 通过一致性测试 / 官方认证"；
  - R4 厂商手册禁止开发竞争产品的，登记到 `plugins/catalog.yaml` 的 `blocked`；
  - R5 每个插件必须有 `sources` 证据链（标题 + URL + 访问日期）。
- 插件默认**声明式**（manifest 描述对象字典/PDO/SM），只有行为逻辑才写 WASM；闭源插件的源码不入库。
- 市场/审核/定价/免责声明细则见 docs/DEVICE-PLUGINS.md §5–§6。

## 10. 自动推送规则（2026-11-12 启用）

- 仓库是 **public**：`https://github.com/archerding/ec-sim`（remote `origin` 走 SSH：`git@github.com:archerding/ec-sim.git`）。
- 每一班在**验证通过并提交后**自动 `git push`；早上的 `git log` / GitHub 页面应能看到夜间进度。
- **前置条件**：本次改动必须有可复现验证（测试/脚本输出/证据文档）；没有验证的改动不许提交，更不许推送。
- **禁止**：`--force`、`-f`、改写已推送历史、绕过测试直接推、推送未完成半成品、改 `origin` 地址。
- **推送失败**（网络/认证）：重试一次；仍失败就在 WORKLOG 卡片记录原因并退出，下一班继续（本地提交保留，不丢工作）。
- 合并冲突/非 fast-forward 时**停止**，在 WORKLOG 记录为卡点，等人处理——不要自动 merge/rebase。