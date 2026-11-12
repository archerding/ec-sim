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

**班次是"持续工作窗口"，不是"打卡做一件事"。** 每次被唤醒后按下面的循环执行，直到窗口剩余时间不足 15 分钟：

1. 读 `WORKLOG.md` 顶部条目与"下次开工第一件事"。
2. 从 §6 的 backlog 里取**顶部未完成项**（若被阻塞，取下一项并在 WORKLOG 记录原因）。
3. 实现 → 本地验证（`cargo test` / `cargo fmt --check` / `cargo clippy`，没有代码时跳过）。
4. 更新 `WORKLOG.md`：顶部插入一条，含 `日期+班次 / 做了什么 / 验证结果 / 卡点 / 下次开工第一件事`；完成项从 §6 移入"已完成"并补下一项。
5. **单次提交**（提交信息含 WORKLOG 变更）→ **`git push`**（`origin main`，fast-forward）。推送失败重试一次；仍失败把原因写进 WORKLOG 的"卡点"并继续做下一项（本地提交保留），**不得 `--force`**。
6. **回到第 2 步做下一项**，不要停下来等下一班。
7. 窗口剩余 <15 分钟时收尾：给出 ≤10 行中文总结（本班完成了几项、验证了什么、是否已推送、下一项是什么）。

> 例：一个 90 分钟班次通常应完成 **2–4 个**可独立验证的小项，而不是 1 个。
> 若某项预计超过剩余时间，改为做它的一个**可独立提交、可验证的子步骤**，把剩余部分写进 WORKLOG 的"下一步"。

**允许跨越多个 backlog 项**（这是默认行为）；真正禁止的是：空壳代码、假测试、未验证就更新完成状态、为赶进度提交半成品。

## 6. 当前 Backlog（按优先级，顶部先做）

- 质量（2026-11-13 01:49 班）：**rustdoc 0 警告**——修复 `esc-core::mailbox` 的 `#[must_use]` 文档行、`esc-core::sii` 的 `data[0x0B]/[0x0C]` 误当链接、`esc-node` 文档链接私有项 3 类警告；`cargo doc --workspace --no-deps` 无警告，测试 192 passed 不变。
- [x] ~~**B1 Rust workspace 骨架**~~ ✅ **2026-11-12 完成**：6 个成员 crate 骨架（`esc-core`/`wire`/`coe`/`plugin-host`/`control-plane`/`cli`）+ 三平台 CI 全绿。证据：commit `93a2fbf`，CI run `43158276907`（ubuntu/macos/windows 的 `fmt --check`/`clippy -D warnings`/`test` 均 success）。
- [x] ~~**B2b 最小 ESC 应答器（Python/Rust 原型均可，先验证协议）**~~ ✅ **2026-11-12 完成**：
  `crates/esc-node`（`ec-sim-esc`）+ `esc-core` 的 `sii`/`eeprom`：TAP 上应答 BRD/APRD/APWR/FPRD/FPWR，
  AL 状态机到 PREOP，SII 可被读取；**未修改的** SOEM `slaveinfo` 报 `1 slaves found and configured`
  （`Man: 00000ec5 ID: 0000e5c1`），IgH `ethercat slaves` 报 `0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`。
  证据：[spike-3](docs/spikes/spike-3-minimal-esc-responder.md)、[复现脚本](scripts/spikes/minimal-esc-responder.sh)。
  顺带修正 `wire::auto_increment_step` 的 ADP 方向（主站 ADP = 环位置相反数、转发前 +1）。
  ⚠️ **OP 状态**：**两家未修改主站都能把从站推入 OP**——SOEM 零补丁（2026-11-12 19:26 班起，
  `simple_ng`：8 位 PDO、每轮 WKC 3）；IgH 零补丁（19:53 班起，需 ecrt 应用夹具，见 B8 进度；
  单独跑 `ethercat slaves` 时无应用会显示 PREOP，这是 IgH 工具行为）。
  IgH 拓扑计算已修（DL Status 0x0110 位按 ETG.1000.4 table 34 修正为单从站正确值 `0x5610`，
  `Failed to calculate bus topology` 消失，2026-11-12 18:56 班；多从站拓扑待 B8）；
  SII 固定区 CRC-8 已补齐（word 0x0007，B6 子项，2026-11-12 晚班）。
- [x] ~~**B2 Spike 1（Linux TAP）**~~ ✅ **2026-11-12 完成**：TAP 双向承载 0x88A4，且**未修改的 SOEM 与 IgH 都能绑定 TAP 并发出真实帧**（IgH `ec_generic` 按 MAC 匹配 TAP）。证据：[spike-1](docs/spikes/spike-1-tap-bidirectional.md)、[spike-2](docs/spikes/spike-2-masters-on-tap.md)。
- [x] ~~**B3 Spike 4（SOEM oshw shim）**~~ ✅ **2026-11-12 20:03 班完成**：模拟器侧 `ec-sim-esc --uds`
  提供 `SOCK_DGRAM` 线缆层（每报文 = 一帧）；验证机本地把 SOEM 2.0.0 的
  `oshw/linux/nicdrv.c` 套接字初始化替换为 UDS 客户端（bind 自己路径 + connect `--uds` 路径，
  send/recv/poll/缓冲区逻辑不变；GPL 夹具源码不入库）。未修改的 SOEM 逻辑经 UDS 跑通
  **`ec_config_init` + `ec_config_map`**：`slaveinfo` 1 从站/8bit I/O，`simple_ng` → **OP**、
  1358 轮 WKC 3、0 次 wrong（**无需 root/TAP**）。证据：
  [spike-5](docs/spikes/spike-5-esc-over-uds.md)、`scripts/spikes/soem-uds-probe.sh`、
  `scripts/spikes/uds-loopback.sh`/`uds_master_probe.py`（合成主站）。
  限制：单从站/单客户端、无 DC/watchdog；shim 只在验证机本地，脚本缺二进制时 exit 2。
- [x] ~~**B4 `wire` crate**~~ ✅ **2026-11-12 完成**：帧/datagram 编解码、M 位串联、13 命令 WKC 规则、自增/配置/广播/逻辑寻址、FMMU 区间判定；16 个单元测试；fmt/clippy(-D warnings) 通过。证据：commit `9d32597`。
  - 进度（2026-11-13 01:29 班）：**解析健壮性模糊测试**——`Frame::parse`/`parse_ethernet`、
    mailbox 各 SDO 解析入口、`Registers` 随机地址读写各加"任意输入不得 panic"测试；测试 **190 passed**。
  - 进度（2026-11-13 00:56 班）：**补 6 个边界单测**——13 命令的 WKC 规则（读/写/读写）、
    自增 ADP 前进与 u16 回绕、More 位后无后续 datagram 拒绝、非法帧类型（frame_type=2）
    拒绝、frame_type=1 才是 EtherCAT command、命令码 0x00–0x0E 往返 + 未知码拒绝；测试 **22 个**。
- [x] ~~**B5 `esc-core` 最小 ESC**~~ ✅ **2026-11-13 完成**：寄存器阵列 + SM/FMMU 寄存器块与逻辑寻址 + AL 状态机（INIT↔PREOP↔SAFEOP↔OP + INIT→BOOT 拒绝/错误 ACK）+ EEPROM 接口状态机；并扩展了 ESM 前置条件（0x0015/0x0017/0x0018/0x0019/0x0024/0x0025/0x002B/SM 布局）、SM watchdog（0x001B）与 0x0410 读写语义。证据见下方各班进度、`WORKLOG.md` 与 `docs/VERIFICATION.md`。已知限制：无独立硬件定时器（帧驱动时钟）；PDI watchdog 超时不模拟；BOOT 状态仅拒绝。
  - 进度（2026-11-12 19:02 班）：寄存器阵列/AL 状态机/EEPROM 接口已完成；**SM（0x0800–0x083F）
    与 FMMU（0x0600–0x067F）寄存器块已实现**（解码/编码、Status/PDI Control/Type/保留字节只读语义、
    主站整块写后回读；+8 条单测 +2 条 datagram 路径集成测试）。
  - 进度（2026-11-12 19:19 班）：**FMMU 逻辑→物理搬运最小闭环已完成**——LRD/LWR/LRW 按已激活
    FMMU 命中（字节对齐、Type 方向位、部分重叠），在逻辑地址与 `DlUserMemory`（0x1000–0xFFFF）
    之间搬运；同班修正了 19:02 班"FMMU Type（0x0B）只读"的错误假设（两主站都会写方向位），
    只读的只有保留字节 0x0D–0x0F。证据：`crates/esc-core/src/{fmmu,memory}.rs`、
    `crates/esc-node/src/lib.rs`，测试 79 passed。
  - 进度（2026-11-12 19:26 班）：**更正 LRW 的 WKC 规则**——写 FMMU 命中 +2、读 FMMU 命中 +1
    （IgH `master/datagram_pair.c` 与 SOEM `src/ec_main.c` 证据；19:19 班的"读写各 +1"不适用于
    LRW），SOEM 过程数据 WKC 从 2 修正为期望的 3。
  - 进度（2026-11-12 20:07 班）：**SM 过程数据看门狗已实现**——`watchdog` 模块（0x0400 divider /
    0x0420 intervals，超时 = `divider × intervals × 40 ns`；SM control bit6 使能），
    `esc-node` 帧驱动时钟 `tick()`：过程数据写重置计时，超时 → AL Status error + Code **0x001B**、
    OP → SAFEOP，Error ACK 清错。证据：`crates/esc-core/src/watchdog.rs`、UDS 合成主站
    `uds_master_probe.py`（10 ms → SAFEOP+ERROR → ACK 清错）；测试 **105 passed**；
    双主站回归无影响（IgH `EC_WD_DISABLE`、SOEM SII control 无 bit6）。
  - 进度（2026-11-12 20:44 班）：**ESM 前置条件检查已实现**——`esc-core::al_status` 码表
    （ETG.1000.6 Table 11 子集）+ `EsmPreconditions` / `EscAl::handle_control_checked()`：
    INIT→PREOP 缺邮箱 SM（SII 声明了 CoE）→ error + **0x0015**；PREOP→SAFEOP 缺过程数据
    SM → error + **0x0017**；未知状态位 → **0x0012**；只检查向上迁移。`esc-node
    --esm-strict` 开启（`Responder::with_esm_preconditions`，**默认关闭**以保持与既有双主站
    冒烟一致）；UDS probe 加 `--strict` 负向/正向断言。测试 **119 passed**。
    Linux 验证机回归：`uds-loopback.sh` 两轮全通过；`minimal-esc-responder.sh` /
    `soem-process-data.sh`（OP、1055 轮 WKC=3）/ `igh-op-probe.sh`（IgH OP）在默认与 strict
    两种模式下 **6/6 全绿** → 前置条件对现有双主站行为无回归，可择机改为默认开启。
  - 进度（2026-11-12 21:07 班）：**ESM 前置条件检查改为默认开启**——`Responder::new()` 的
    `esm_preconditions = true`，CLI 新增 `--esm-lax` 关闭（`--esm-strict` 保留为旧参数名）；
    测试夹具与 `uds_master_probe.py` 默认路径同步（默认断言 0x0015/0x0017 拒绝路径再放行）。
    测试 **119 passed**；Linux 验证机 `uds-loopback.sh` 两轮 + 三个双主站脚本在默认/lax
    两种模式下 **6/6 全绿**（SOEM OP 1055 轮 WKC=3、IgH ecrt OP `al_states=0x08 wc=3`）。
  - 进度（2026-11-12 23:52 班）：**SAFEOP→OP 前置条件已实现**——`EsmPreconditions` 增加
    `outputs_declared/outputs_configured`（RxPDO/SM2）与 `inputs_declared/inputs_configured`
    （TxPDO/SM3）；`failure_for(Op)` 返回 **0x0019**（无有效输出）/ **0x0018**（无有效输入）/
    两者都缺 **0x002B**；`esc-node` 由 SII 类别 + SM2/SM3 配置推导。测试 **166 passed**；
    验证机回归 `soem-process-data.sh`（OP 1055 轮 WKC=3）、`generic-cia402-probe.sh`、
    `igh-el2008-op-probe.sh`（IgH OP）全部 EXIT=0。
    ⚠️ 以 SM 配置近似"有效输出/输入"，未检查映射对象内容（0x0024/0x0025 待实现）。
  - 进度（2026-11-12 23:57 班）：**映射类 OP 前置条件已实现**——`EsmPreconditions` 增加
    `outputs_mapping_valid/inputs_mapping_valid`；`esc-node` 由 `0x1C12/0x1C13` 分配 +
    PDO 映射字位长与 SM2/SM3 长度比较；失败返回 **0x0024**（输出配置无效）/ **0x0025**
    （输入配置无效，文本 ⚠️ 待 ETG.1000.6 原文复核）。测试 **168 passed**；验证机回归
    `soem-process-data.sh`（OP 1055 轮 WKC=3）、`generic-cia402-probe.sh`、
    `igh-el2008-op-probe.sh`（IgH al_states=0x08）全部 EXIT=0。
  - 进度（2026-11-13 00:06 班）：**映射合法性校验细化**——`process_data_mapping_valid()`
    检查分配 PDO 索引非 0 且不重复、映射对象条目数 > 0、映射字对象索引/位长非 0、
    总位长 = SM `length×8`；重复分配等 → OP 仍返回 0x0024/0x0025。测试 **169 passed**；
    验证机回归三个脚本（SOEM OP 1055 轮 WKC=3、CiA402 扫描/OP、IgH EL2008 OP）全部 EXIT=0。
    ⚠️ 仍未校验映射字里的对象/子索引是否真实存在（需字典双向校验）。
  - 待做：映射字对象存在性/子索引范围校验；SM 配置合法性校验（地址越界/区间重叠/类型方向）；
    无独立硬件定时器（帧驱动时钟）；0x0410 PDI watchdog 仅保留读写语义。
- [x] ~~**B6 SII 生成器**~~ ✅ **2026-11-13 完成**：ESI XML / manifest → SII 镜像（固定区 CRC-8 word 0x0007、字符串/通用/SM/FMMU/TxPDO/RxPDO/**DC** 类别链、0xFFFF 终止）；`--esi`/`--manifest`/组合路径均经 SOEM/IgH 扫描验证。已知限制：FMMU 类别按 4 通道示例生成；DC 类别生成能力已就绪但**无 profile 声明**（模拟器未实现 DC 同步，开启前需先做 DC 仿真）。
  - 进度（2026-11-13 01:51 班）：**FMMU 类别 0x0028 已实现**——内容 4 字节用途码
    `[0x03,0x01,0x02,0xFF]`（邮箱读/过程数据输出/输入/未使用），长度字=2；依据公开 CSDN
    资料（只读参考，2026-11-13 访问）；新增测试并更新类别链期望。验证机 soem-process-data/
    minimal-esc-responder/igh-el2008-op-probe 全部 EXIT=0（FMMU 类别未影响 OP）。测试 **193 passed**。
    ⚠️ 仍剩 **DC 类别（0x003C）**未实现（需 ETG 原文）。
  - 进度（2026-11-13 01:56 班）：**DC 类别（0x003C）生成已实现**——`SiiDc` 结构按公开资料
    文档化布局（cycleTime0/shiftTime0/1、sync1/0CycleFactor、assignActivate、name/desc、
    reserved，24 字节）编码；`SiiSpec.dc = Some(..)` 时在 FMMU 后插入 DC 类别；新增 2 单测。
    ⚠️ 内置镜像/ESI 仍不声明 DC；**SV660N 画像自 2026-11-13 02:27 起声明 DC**（Tier0 帧驱动模拟时钟，无真实 SYNC0/漂移）；
  - 进度（2026-11-13 01:58 班）：**manifest `dc` 时序字段已接入 SII DC 类别**——新增
    `DcConfig{ supported, cycle_time0, shift_time0/1, sync0/1_cycle_factor, assign_activate }`，
    `sii_gen` 在 `supported && cycle_time0>0` 时写入 DC 类别；`validate()` 拒绝缺 cycle_time0；
    新增 manifest 解析/校验与 SII 生成测试。**所有仓库画像仍 dc.supported=false**（DC 仿真未实现）。
    测试 **197 passed**。⚠️ 开启 DC 声明前必须先实现 DC 同步仿真。
    开启需先实现 DC 仿真与 manifest 时序字段。测试 **195 passed**。
  - 进度（2026-11-12 19:26 班）：**SM 类别 0x0029 + TxPDO 0x0032 + RxPDO 0x0033 已实现**
    （SM0/SM1 = mailbox、SM2 输出 0x1100、SM3 输入 0x1110；PDO 0x1A00/0x1600，
    对象 0x6000:00/0x7000:00）。字节布局按 IgH `slave.c` 与 SOEM `ec_main.c` 的解析代码逐字节复核；
    EEPROM 大小 word 0x003E 改为 2（384 字节）。证据：`crates/esc-core/src/sii.rs`，测试 90 passed；
    SOEM `slaveinfo` 显示 `Output size: 8bits / Input size: 8bits`、`MBX protocols : 04`。
  - 进度（2026-11-12 22:38 班）：**ESI XML → SII 生成器（子集）已实现**——`plugin-host::esi`
    零依赖最小 XML 解析器 + ESI 子集解析（Vendor/Id、Device/Type 的 ProductCode/RevisionNo、
    Device/Name、Sm、RxPdo/TxPdo+Entry、Mailbox/CoE），`sii_spec_from_esi()` 复用
    `SiiSpec`/`SiiImage::from_spec()` 编码（邮箱长度缺省 128、过程数据 SM 长度按 PDO 位长求和）。
    测试夹具为仓库**自写** `sample.esi.xml`（非厂商 ESI，R1）；测试 **150 passed**。
    ⚠️ 尚未接入 CLI、未在 IgH/SOEM 扫描 ESI 生成画像（22:42 班已补齐，见下）。
  - 进度（2026-11-12 22:42 班）：**`--esi` 已接入 CLI 并双主站扫描验证**——`ec-sim-esc
    --esi <file.esi.xml>`（与 `--manifest` 互斥）生成 SII 镜像；运行时字典只有身份对象
    0x1018（ESI 无完整 OD，其余需 manifest/行为插件）。验证机
    `scripts/spikes/esi-sii-scan-probe.sh` **EXIT=0**：SOEM 读 `8bits/8bits`、
    `Man 00000ec5 ID 0000e5d3 Rev 00010000`、`MBX 128/128 protocols 04`；IgH 显示
    `0 0:0 PREOP + 0x00000ec5:0x0000e5d3`。测试 **151 passed**。
  - 进度（2026-11-12 23:05 班）：**ESI + manifest 组合画像已验证到 OP**——`--esi` 与
    `--manifest` 可组合（SII 的 SM/PDO/邮箱来自 ESI；身份/名称/运行时字典来自 manifest，
    `sii_spec_from_esi_and_manifest()`）；配对夹具 `sample.esi.plugin.yaml`；验证机
    `scripts/spikes/esi-manifest-op-probe.sh` **EXIT=0**：SOEM `simple_ng` 零补丁
    `mapped 1O+1I` 且 `all slaves are now operational`（WKC 3，无 wrong），IgH PREOP +
    身份 `0x00000ec5:0x0000e5d3`。测试 **152 passed**。
  - 待做：组合画像接入行为插件（B11 Tier0 骨架）、IgH OP（复用作 ecrt 夹具）、
    FMMU 类别 0x0028、DC 类别（需 ESC Sec.II / IEC 61158 原文）。
  ⚠️ 旧文档把 CRC 写成 word 0x003F，经 SOEM `eepromtool`/IgH `tool/sii_crc.cpp` 复核已更正（0x003F 是 SII 版本 = 1）。
- [x] ~~**B7 CoE 最小闭环**~~ ✅ **2026-11-13 完成**：expedited/segmented SDO 上传/下载、0x1C12/0x1C13/0x1600/0x1A00 配置对象、Abort 码（含 0x05030000/0x06070010/0x06010002/0x06020000/0x05040000）、SDO Info、Complete Access（上传/单帧+分段下载/重发容错/超长 Abort）。已知限制：CA 部分写/子索引顺序语义未实现；IgH 侧 SDO 上传/下载在验证机受 ecrt ioctl 阻塞（环境问题，⚠️ 待验证）；SOEM `slaveinfo -sdo` 有已知偶发段错误（探针已重试规避）。
  **SDO Info 已实现**（SII 通用类别 CoE details = `0x0F`，声明 SDO/Info/PDO assign/config）；
  仍**不做** Complete Access（SII 对应位保持 0，CA 未实现）。
  - 进度（2026-11-12 19:36 班）：**邮箱数据链路已通**——SII 声明 CoE mailbox（固定区 0x0014–0x001C，
    SM0 0x1000 / SM1 0x1080 各 128 字节；协议位 = `0x0004` CoE，注意不是 0x0001=AoE），
    `esc-node` 对 CoE SDO 请求回 **SDO Abort**（0x06020000）并置 SM1 Status bit3，主站读走后清除。
    证据：`crates/esc-core/src/mailbox.rs`、`scripts/spikes/soem-mailbox-probe.sh`
    （SOEM `slaveinfo -map` 断言 mbox>0）；未修改 SOEM 侧 `mbox=1 mboxresp=1`，过程数据 OP/WKC3 无回归。
  - 进度（2026-11-12 19:44 班）：**最小对象字典 + SDO expedited upload 已实现**——`obj_dict`
    （0x1018、0x1C00、0x1C12/0x1C13、0x1600/0x1A00，与 SII SM/PDO 一致），`mailbox` 对
    initiate upload 按字典回 expedited response（0x4F/0x4B/0x47/0x43），未知对象回 Abort。
    证据：`crates/esc-core/src/obj_dict.rs`、`scripts/spikes/sdo-upload-probe.sh`；
    未修改 SOEM `slaveinfo -map` 打印 `0x7000:0x00 0x08 / 0x6000:0x00 0x08`，
    未修改 IgH `ethercat upload` 读到 `0x1C12:01=0x1600`、`0x1600:01=0x70000008`，测试 98 passed。
  - 进度（2026-11-12 19:49 班）：**SDO Download 已实现（配置对象可写）**——`parse_sdo_request()`
    支持 expedited download（0x2F/0x2B/0x27/0x23），`is_writable()` 开放 0x1C12/0x1C13/0x1600/
    0x1A00/0x1C32/0x1C33，写入值保存在 `Responder.object_writes` 并可被 Upload 回读；只读对象回
    Abort `0x06010002`。证据：扩展后的 `scripts/spikes/sdo-upload-probe.sh`；未修改 IgH
    `ethercat download 0x1C13:01=0x1a01` 成功后 `upload` 回读 `0x1a01`，只读/未知对象被拒绝；测试 100 passed。
  - 进度（2026-11-12 21:11 班）：**CoE SDO Information 已实现**——`mailbox` 解析/构造
    SDO Info 服务（Get OD List `0x01/0x02`、Get OD `0x03/0x04`、Get Entry Description
    `0x05/0x06`、错误 `0x07`；偏移按 IgH `fsm_coe.c` / SOEM `ec_coe.c` 复核），`obj_dict`
    增加 6 对象/18 条目的描述表（类型/位长/访问权限/名称），SII 通用类别 CoE details = `0x0F`。
    测试 **124 passed**；Linux 验证机 `scripts/spikes/sdo-info-probe.sh` →
    `slaveinfo -sdo` 打印 6 个对象及描述/访问权限；双主站回归默认/strict **6/6 全绿**。
    ⚠️ IgH `ethercat sdos` 无应用时无输出（需 ecrt 应用上下文），IgH 侧待验证。
  - 进度（2026-11-12 22:10 班）：**分段 SDO 上传（segmented upload）已实现**——`mailbox`
    支持 initiate 非 expedited 响应（`0x41` + u32 长度 + 首段）与 segment 请求/响应
    （`toggle/unused/last` + 固定 7 字节）；`esc-node` 增加分段传输状态机（≤4 字节仍
    expedited）；`dict_gen` 支持字符串对象（VISIBLE_STRING / OCTET_STRING），对象列表按索引
    排序。EL2008 画像新增 `0x1008:00` 设备名（42 字节，名称带 SIMULATED）。测试 **146 passed**；
    Linux 验证机 `scripts/spikes/sdo-segmented-upload-probe.sh` → SOEM `slaveinfo -sdo`
    经**分段上传**完整读出 `0x1008 "Device name" ... "Beckhoff EL2008 compatible 8DO (SIMULATED)"`
    （20 个对象）。
    ⚠️ IgH 侧分段上传待验证（`ethercat upload` CLI 需要 SDO 类型上下文；ecrt
    `ecrt_master_sdo_upload` 夹具在未配置应用场景下阻塞）；B6 FMMU 寄存器空间仍待原文核实。
  - 进度（2026-11-12 22:20 班）：**合成主站（UDS）segmented SDO 上传断言已加入**——
    `uds_master_probe.py` 新增邮箱收发辅助与 `--sdo-segmented`（initiate `0x41` +
    segment `0x60|toggle` 拼接），`uds-loopback.sh` 第三轮用 EL2008 manifest 画像读
    `0x1008` 设备名：验证机 `EXIT=0`，输出
    `segmented SDO 读到 0x1008 = 'Beckhoff EL2008 compatible 8DO (SIMULATED)'（42 字节）`。
    ⚠️ IgH 侧 SDO 上传在该验证机上**整体超时**（expedited 与 segmented 同样失败，dmesg 有
    ecrt ioctl 内核栈转储），判定为环境问题；恢复后需补 `ethercat upload -t string 0x1008 0`。
  - 进度（2026-11-12 23:44 班）：**分段 SDO 下载（segmented download）已实现**——`mailbox`
    支持 initiate（`0x21` + Size u32）与 segment 请求/响应（`0x20|toggle<<4|unused<<1|last`，
    布局按 IgH `fsm_coe.c` 的 `prepare_down_start`/`prepare_segment_request` 复核）；
    `esc-node` 增加分段下载状态与写入（只读/未知对象 Abort）。测试 **164 passed**。
    ⚠️ 仅 datagram 级单测，**未在验证机用真实主站验证**（需长数据夹具）。
  - 进度（2026-11-12 23:47 班）：**分段下载已用合成主站端到端验证**——通用 CiA 402 /
    SV660N 画像新增可写配置字符串 `0x2100:00`（20B）；`uds_master_probe.py`
    `--sdo-segmented-download`（initiate `0x21` + 7B/5B 分段 + 分段上传回读比对），
    验证机 `scripts/spikes/cia402-sdo-download-probe.sh` **EXIT=0**。IgH 侧 SDO
    （上传/下载）在该验证机上仍**阻塞**（ecrt ioctl，已复现两次）→ ⚠️ 待验证。
  - 进度（2026-11-13 01:32 班）：**CiA402 画像启用 CA 能力位**——通用 CiA402 与 CSV 画像的 `coe.complete_access` 改为 true（SII CoE details bit5），插件守卫测试新增 CoE details 位一致性断言；`sdo-info-probe.sh` 对 SOEM 已知偶发段错误加 3 次重试。验证机 generic-cia402/sdo-ca/sdo-info 均 EXIT=0。测试 **190 passed**。⚠️ CA 部分写/顺序语义未实现，声明略高于实际严格度。
  - 进度（2026-11-13 01:45 班）：**CA 下载改为 all-or-nothing**——`write_complete_access` 先
    校验全部子索引（存在/可写/长度和）再写入，避免"写入一部分后 Abort"留下部分状态；新增
    单测 `complete_access_download_is_all_or_nothing`（子 2 只读 → Abort 0x06010002 且 object_writes 为空）。
    验证机 sdo-ca-probe / cia402-sdo-download-probe 均 EXIT=0。测试 **192 passed**。
  - 进度（2026-11-13 02:02 班）：**分段下载数据超长 → Abort 0x06070010**——新增
    `SDO_ABORT_LENGTH_MISMATCH`；`handle_download_initiate`（initiate 自带数据超声明长度）与
    `handle_download_segment`（累计段数据超总长）都会回 Abort 并清空分段状态，避免静默截断/部分写；
    新增单测覆盖两处。验证机 sdo-ca-probe/cia402-sdo-download-probe 均 EXIT=0。测试 **198 passed**。
  - 待做：CA 部分写/顺序语义细化（IgH 侧仍无法验证 CA）。
  - 进度（2026-11-13 01:16 班）：**分段传输 toggle 校验**——`SegmentedUpload`/`SegmentedDownload`
    增加期望 toggle（initiate 后 0，每成功一段取反），请求 toggle 不符回 Abort
    `0x05030000` 并清状态；修复非末段未推进 toggle 的缺陷；新增单测
    `out_of_order_segment_toggle_is_aborted`。验证机 `sdo-ca-probe.sh`/`uds-loopback.sh`/
    `soem-process-data.sh` 全部 EXIT=0（正确 toggle 序列无回归）。测试 **186 passed**。
    ⚠️ 已支持"重发上一段"的幂等（2026-11-13 01:21 班：缓存最近一段响应/toggle；更早的段仍 Abort）。
  - 进度（2026-11-13 01:26 班）：**重发容错端到端验证**——合成主站 `--sdo-retransmit`：
    先 CA 多段写 140B，再分段上传并在首段重复同一 toggle 请求，断言响应逐字节相同、
    回读一致；验证机 `sdo-ca-probe.sh` 四模式 **EXIT=0**（详见 WORKLOG 同班条目）。
  - 进度（2026-11-13 00:50 班）：**CA 分段下载端到端验证通过**——UDS 合成主站新增
    `--sdo-ca-download-multi`（140 字节对象：0x31 首帧 112 + 0x20|toggle|last 4 段，
    再用 CA 分段上传回读比对）；`sdo-ca-probe.sh` 第三个模式用
    `plugins/generic-cia402-csv-servo`（其 `0x2100` 字符串扩为 140 字节）；修复
    CA 上传从**覆盖值**（object_writes）读取（此前只读字典初值，回读不一致）。验证机
    **EXIT=0**（CA 上传/单帧下载/分段下载三模式全过 + UDS 基础断言）。测试 **177 passed**。
    ⚠️ 末段 CA 分段下载应答改为 segment response（与 CiA 301 一致）；仍不校验部分写语义，
    IgH 公共 API 不下发 CA 位。
  - 进度（2026-11-13 00:47 班）：**CA 分段下载已实现**——`mailbox::parse_complete_access_download_initiate`
    解析 CA initiate 的 size + 首帧数据；首帧不足 total 时转入 `SegmentedDownload`（保留既有
    分段 FSM），末段完成后按子索引长度拆分写入；命令 `0x20|toggle<<4|unused<<1|last` 与普通
    分段下载一致。证据：单测 `mailbox_complete_access_download_handles_segmented_transfer`
    （130 字节对象：首帧 112 + 3 段 7/7/4，断言完整写入）；验证机 `sdo-ca-probe.sh`
    回归 **EXIT=0**（CA 上传/单帧下载不回归）。测试 **177 passed**。
    ⚠️ CA 分段下载未做端到端主站验证（合成主站仅覆盖单帧；`⚠️ 待验证`）；仍不校验 CA
    部分写/顺序语义；IgH 公共 API 不下发 CA 位。
  - 进度（2026-11-13 00:41 班）：**Complete Access（CA）下载（单帧）已实现**——
    `mailbox::parse_complete_access_download` 解析命令 `0x31`（initiate normal download +
    CA 位），应答器按字典各子索引字节长度拆分数据逐项写入 `object_writes`（未知对象
    Abort 0x06020000、只读对象 0x06010002、长度不符 0x05040000），回 0x60。
    证据：`scripts/spikes/sdo-ca-probe.sh` 验证机 **EXIT=0**（CA 上传 0x1C12/0x1600 +
    CA 下载 0x1600 写入并回读一致，每模式独立模拟器实例）；单测
    `mailbox_complete_access_download_writes_all_subindices`；测试 **176 passed**。
    ⚠️ 仅单帧 CA 下载（数据不超邮箱首段）；CA 分段下载未实现；IgH 侧仍无法直接验证 CA。
  - 进度（2026-11-13 00:36 班）：**Complete Access（CA）上传已实现**——`mailbox::parse_complete_access_upload`
    解析命令 `0x50`（initiate upload + CA 位 bit4），应答器按对象子索引 0..max 的值
    **按各自长度拼接**（不填充）一次返回（超过首段容量走既有分段上传路径）；未知对象回
    Abort 0x06020000。证据：`scripts/spikes/sdo-ca-probe.sh`（合成主站）验证机
    **EXIT=0**（0x1C12 CA = `01 00 16`、0x1600 CA = `01 08 00 00 70`）；单测
    `mailbox_complete_access_upload_returns_all_subindices`；测试 **175 passed**。
    ⚠️ IgH 1.6.13 公共 API 不下发 CA 位（`prepare_up` 只写 0x40），故 IgH 侧无法直接验证
    CA 上传；CA **下载**与 Abort 语义仍未实现。
- [x] ~~**B9 声明式插件加载器**~~ ✅ **2026-11-12 21:25 班完成**：`plugin-host` 零依赖解析  `plugin.yaml`（YAML 子集 + R2/R5 校验）→ `sii_gen` 生成 SII 镜像、`dict_gen` 生成运行时
  对象字典（0x1018 身份 / 0x1C00 SM 类型 / 0x1C12+0x1C13 分配 / PDO 映射 / 用户对象）；
  `esc-node` 新增运行时 `ObjectDictionary` 与 **`ec-sim-esc --manifest`**，SDO 上传/下载/
  SDO Info 全走运行时字典。Linux 验证机端到端（`scripts/spikes/manifest-slave-probe.sh`）：
  SOEM 扫描到 manifest 身份（`00000ec5:0000e5d1`）、8 位 I/O、**OP + 1055 轮 WKC=3**、
  `slaveinfo -sdo` 读到 7 个对象（含用户对象 `0x2000=0x1234`）；IgH 身份/拓扑正常。测试 **141 passed**。
  - 已知限制：SOEM `slaveinfo -sdo` 对 UINT8 子索引 0 的 2 字节读取有未初始化高位（偶发
    段错误，属 SOEM 工具夹具缺陷，脚本重试规避）；manifest 初值仅 8/16/32 位整数；
    FMMU 仍由主站配置阶段写入；DC 未实现。
  - 进度（2026-11-13 00:23 班）：**加载期 SM/PDO 一致性校验**——`validate()` 增加
    `dir: out/in` 的 SM `len×8` 与 RxPDO/TxPDO 位长之和比较、映射位长非 0、object 的
    `pdo: rx/tx` 引用必须真实存在于对应映射（悬空/非法值报错）；新增单测。测试 **171 passed**；
    验证机回归 `generic-cia402-probe.sh`/`cia402-csv-probe.sh`/`igh-el2008-op-probe.sh`
    全部 EXIT=0（既有插件无误报）。⚠️ 仍未校验映射字的数据类型/位长一致性与 SM 地址合法性。
  - 进度（2026-11-13 00:27 班）：**PDO 映射项数据类型/位长 + 重复映射校验**——
    `validate()` 对已声明对象校验 dtype 位长与映射项一致（BOOL/8/16/32/字符串），
    同一 PDO 内重复映射同一对象报错；新增单测。测试 **172 passed**；验证机
    `generic-cia402-probe.sh`/`cia402-csv-probe.sh` EXIT=0（无误报）。
    ⚠️ 仍未校验 SM 地址/区间/类型方向合法性；未声明对象的映射项跳过校验。
  - 进度（2026-11-13 00:31 班）：**SM 地址/长度/区间重叠校验**——`validate()` 检查
    `len != 0`、起始地址在 `0x1000–0xFFFF`、区间不越过 `0xFFFF`、不同 SM 区间不重叠；
    新增单测。测试 **173 passed**；既有插件无回归（守卫测试全绿）。
    ⚠️ 未校验 SM control/type 语义（manifest 未声明）；地址与 SII 天然一致（同一来源）。
  - 进度（2026-11-13 00:33 班）：**对象声明合法性校验**——`validate()` 检查 `objects` 的
    dtype（仅支持的类型）、access（ro/rw）、字符串 value 必填与位长上限、数值 value 可解析、
    (index,sub) 不重复；新增单测。测试 **174 passed**；既有插件无误报。
    ⚠️ 未校验数值初值与 dtype 位宽匹配（截断风险）、SM control/type 语义。
  - 进度（2026-11-13 01:40 班）：**0x0410（PDI watchdog）语义明确化**——寄存器可读写保存
    （透传），但模拟器无 PDI，因此**不模拟 PDI 超时**（不递减、不置 AL error）；新增
    esc-node 单测验证 APWR/APRD 回读与 tick 后行为；`watchdog.rs` 文档同步说明。测试 **191 passed**。
  - 进度（2026-11-13 00:58 班）：**SM 布局合法性检查**——`EsmPreconditions.sync_managers_valid`
    + `esc-node::active_sm_layout_valid()`：已激活 SM 必须落在 DL 内存 `0x1000–0xFFFF`、
    区间不越界且互不重叠，否则 SAFEOP 请求报 **0x0017**（状态保持 PREOP）；`EsmPreconditions`
    改为手写 `Default`（未配置 SM 时布局天然合法）。测试 **185 passed**；验证机回归
    `soem-process-data.sh`（OP 1055 轮 WKC=3）/`generic-cia402-probe.sh`/
    `igh-el2008-op-probe.sh`（IgH OP）全部 EXIT=0。⚠️ 只查 SM0–SM3 且不校验 control/type。
  - 进度（2026-11-13 00:35 班）：**数值初值位宽范围校验**——`validate()` 按 dtype 位宽/
    有无符号校验初值上限（BOOL/INT8/UINT8/INT16/UINT16/INT32/UINT32），越界报"会截断"、
    负值报"暂不支持"；扩展单测覆盖 `UINT8=0x1FF`、`INT16=65535`。测试 **174 passed**；
    既有插件无误报。⚠️ 仍未覆盖 SM control/type 与对象子索引范围。
- [x] ~~**B10 首个 P0 设备插件**~~ ✅ **2026-11-12 21:33 班完成**：`plugins/beckhoff-el2008/`
  声明式画像（自有 VID/PID `0x00000EC5`/`0x0000E5D2`，名称带 SIMULATED；8 通道输出、
  0 输入、SM2 输出 1 字节、默认 RxPDO `0x1600`..`0x1607` 各映射 1 bit 输出对象
  `0x7000:01`..`0x7070:01`）；`facts.md` 记录公开资料事实与来源（R5：Beckhoff 产品页 /
  Information System / linuxcnc-ethercat esi-data，访问日期 2026-11-12），未入库厂商文件（R1）。
  Linux 验证机：SOEM 扫描身份 `00000ec5:0000e5d2`、`Output 8bits/Input 0bits`、`-map`
  显示 `SM2 outputs` 与 8 个 BOOLEAN 输出映射、`simple_ng` 进 OP（`mapped 1O+0I`）；
  IgH 扫描 PREOP/拓扑/身份正常；**IgH outputs-only ecrt 夹具已验证 OP**
  （`scripts/spikes/igh-el2008-op-probe.sh`：`ethercat slaves` = OP，
  `RESULT al_states=0x08 wc=1`）。证据：[验证记录](docs/plugins/beckhoff-el2008-verification.md)、
  `scripts/spikes/beckhoff-el2008-probe.sh`；`plugin-host` 增加仓库插件 manifest 守卫测试。
  - 2026-11-12 21:41 班补齐：`dict_gen` 为数组对象自动补子索引 0（UINT8 条目数，CiA 301），
    消除 IgH 字典读取的 SDO Abort 噪声；验证脚本的 `grep | head` 管线补 `|| true`
    （pipefail/SIGPIPE 误判）。
  - 遗留 ⚠️：SOEM `simple_ng` expected WKC 比实际多 1（SOEM 把 SM1 状态寄存器映射为 FMMU 并在
    无输入时 `inputsWKC++`；本模拟器未做寄存器空间 FMMU，ESC 是否允许待核实）；
    SOEM `slaveinfo -sdo` 虽已可用但受工具自身缺陷影响（best-effort）；电气特性/DC 未模拟。
  - 进度（2026-11-13 02:08 班）：**CST（转矩模式，5 字节 PDO）Tier0 行为已实现**——RxPDO `0x6040`+`0x6060`+`0x6071`、TxPDO `0x6041`+`0x6061`+`0x6077`，Operation enabled 时 `0x6077` 镜像目标转矩、禁用归零；新增画像 `plugins/generic-cia402-cst-servo/`（自有 VID/PID，CoE SDO/SDO Info/CA）、合成主站探针 `--cia402-cst`、脚本 `scripts/spikes/cia402-cst-probe.sh`、验证记录 `docs/plugins/generic-cia402-cst-verification.md`。验证机 **EXIT=0**（四步状态字/模式/转矩全部符合），测试 **199 passed**。⚠️ Tier0 无转矩环/负载/单位换算。
  - 进度（2026-11-13 02:13 班）：**CST 画像双主站验证**——`scripts/spikes/cia402-cst-op-probe.sh` 在未修改 SOEM/IgH 下 **EXIT=0**：SOEM `40/40 bits`、`5O+5I` 进 OP、`WKC 3`；`-map` 见 `0x6071`/`0x6077`；IgH PREOP + 身份 `0x00000ec5:0x0000e5e2`。记录见 `docs/plugins/generic-cia402-cst-verification.md`。
  - 进度（2026-11-13 02:17 班）：**IgH ecrt CST 进 OP 验证**——`scripts/spikes/igh-cia402-cst-op-probe.sh`（ecrt 夹具为验证机本地文件，不入库）**EXIT=0**：`ethercat slaves` 显示 `OP`、`al_states=0x08`、`sw=0x0237`、`modes_display=10`、`0x6077=300`；记录见 CST 验证文档。
  - 进度（2026-11-13 02:21 班）：**SV660N 画像能力位复核**——`scripts/spikes/sv660n-capability-probe.sh` **EXIT=0**：SOEM 56/56 bits、`CoE details=0d`（不声明 SDO Info，符合公开事实）、`Has DC: 0`（⚠️ 与真实设备默认开 DC 有差距）、MBX 04；IgH PREOP + 身份 `0xe5f1`。
  - 进度（2026-11-13 02:27 班）：**DC Tier0 落地**——ESC Support(0x0008) 声明 DC、0x0900–0x0917 帧驱动模拟时钟（`DC_TICK_NS=1000`）、SV660N 画像声明 SII DC 类别（assignActivate 0x300）；新增 `dc-clock-probe.sh`。验证机：SOEM `Has DC: 1`、`dc-clock-probe.sh` EXIT=0（0x0910 前进）、`soem-process-data.sh` 仍 OP/WKC 3 无回归；测试 **201 passed**。⚠️ 无真实 SYNC0 脉冲/漂移/传播延迟测量。
- [ ] **B11 CiA402 行为插件**：以 Inovance SV660N 兼容画像为第一个目标，实现 CSP 状态机（Tier0/内置 profile 起步）。
  - 依赖 B10 已完成（声明式画像 + `--manifest` + 验证脚本可复用）；先 facts.md + 声明式画像，
    行为 WASM 只做 Tier0 状态机骨架。
  - 进度（2026-11-12 23:12 班）：**通用 CiA 402 声明式画像已上架**——
    `plugins/generic-cia402-servo/`（自有 VID 0x0EC5/PID 0xE5E0；SM2 56bit RxPDO
    `0x6040`+`0x6060`+`0x607A`，SM3 56bit TxPDO `0x6041`+`0x6061`+`0x6064`；`facts.md`
    只引用公开来源）。验证机 `scripts/spikes/generic-cia402-probe.sh` **EXIT=0**：SOEM
    `56bits/56bits`、`Man 00000ec5 ID 0000e5e0`、`mapped 7O+7I` 进 OP（WKC 3），IgH
    PREOP + 身份 `0x00000ec5:0x0000e5e0`；验证记录 `docs/plugins/generic-cia402-verification.md`。
  - 进度（2026-11-12 23:18 班）：**CiA 402 状态机骨架已实现**——`esc-node::cia402`
    （控制字/状态字位常量、`Cia402State`、`Cia402StateMachine::update/inject_fault`），
    shutdown/switch on/enable operation/disable voltage/quick stop/fault reset 标准转换，
    6 个单测；文档 `docs/plugins/cia402-state-machine.md`。测试 **158 passed**。
    ⚠️ 尚未接入运行时 PDO 数据（纯逻辑）；Ti=0/1、模式位、错误码触发未实现。
  - 进度（2026-11-12 23:20 班）：**状态机已接入运行时过程数据**——`Responder::with_cia402_behavior`
    + `--behavior cia402`（按 manifest SM2/SM3 起始地址；tick 中读控制字 `0x6040`→状态机→
    写状态字 `0x6041`）。验证机 `scripts/spikes/cia402-behavior-probe.sh` **EXIT=0**：
    合成主站断言 `0x0006→0x0221`、`0x0007→0x0223`、`0x000F→0x0237`、回 `0x0240`；
    同时 UDS 原套件继续通过。测试 **159 passed**。⚠️ 一帧延迟、只写状态字 2 字节。
  - 进度（2026-11-12 23:26 班）：**行为扩展为完整 7 字节 RxPDO/TxPDO**——SM2 解析
    `0x6040`+`0x6060`+`0x607A`，SM3 写 `0x6041`+`0x6061`+`0x6064`；Tier0 简化：Operation
    enabled 时实际位置跟随目标并置 Target reached，禁用后保持；SM 长度 <7 退化为状态字。
    验证机 `cia402-behavior-probe.sh` **EXIT=0**（`10/7B` 布局、`0x000F→0x0637`、模式显示 8、
    位置 1000）；测试 **160 passed**。⚠️ 无运动环/模式切换/Ti 时序。
  - 进度（2026-11-12 23:30 班）：**IgH 侧 CiA 402 画像已进 OP 并验证运行时行为**——
    `scripts/spikes/igh-cia402-op-probe.sh`（验证机本地 ecrt 夹具，源码不入库）：
    应用运行中 `ethercat slaves` = **OP** + `0x00000ec5:0x0000e5e0`，
    `RESULT al_states=0x08`，`STATUS sw=0x0637 modes_display=8 pos=1000`（控制字序列
    `0x0006→0x0007→0x000F` 驱动 TxPDO）。证据：`docs/plugins/generic-cia402-verification.md`。
  - 进度（2026-11-12 23:34 班）：**Inovance SV660N 兼容画像（Tier0）已建档**——
    `plugins/inovance-sv660n/`（自有 VID 0x0EC5/PID 0xE5F1，通用 DS402 7B PDO 布局，
    `facts.md` 只列公开来源并标注 ⚠️ 待核实项），catalog 状态 `researching`。
    验证机 `MANIFEST=... EXPECT_PID=0000e5f1 generic-cia402-probe.sh` **EXIT=0**：
    SOEM `56bits/56bits`、`Man 00000ec5 ID 0000e5f1`、`mapped 7O+7I` 进 OP（WKC 3），
    IgH PREOP + 身份 `0x00000ec5:0x0000e5f1`。记录见
    `docs/plugins/inovance-sv660n-verification.md`。
    ⚠️ SV660N 具体默认 PDO/扩展对象/DC/故障码表尚未逐项核实（公开手册为 PDF）。
  - 进度（2026-11-12 23:40 班）：**行为加入模式门控 + 错误注入（Tier0）**——只有 CSP
    （模式 8）在 Operation enabled 时位置跟随目标并置 Target reached，其它模式位置冻结；
    `Responder::inject_cia402_fault(error_code)` 使状态机进入 Fault、写 `0x603F` 错误码，
    fault reset 回 Switch on disabled（错误码保留，⚠️ 与真机时序不同）。合成主站序列加入
    CSV 切换断言；测试 **162 passed**、验证机 `cia402-behavior-probe.sh` **EXIT=0**。
  - 进度（2026-11-13 00:11 班）：**CSV（速度模式，11 字节 PDO）画像 + 行为已上架**——
    `plugins/generic-cia402-csv-servo/`（RxPDO `0x1600` 含 `0x60FF`、TxPDO `0x1A00`
    含 `0x606C`；自有 VID 0x0EC5/PID 0xE5E1）；行为支持 CSP（位置跟随）与 **CSV（位置每
    周期累加目标速度、速度显示镜像，Tier0 简化）**。验证机 `cia402-csv-probe.sh` **EXIT=0**
    （11 字节序列 + UDS 原套件），`uds-loopback.sh` 三轮 EXIT=0；测试 **170 passed**。
    ⚠️ CSV 为 Tier0（无真实轨迹/加减速/模式时序）；映射一致性检查在 SM 长度≠SII 声明时
    跳过（合成主站特例放宽）。
  - 进度（2026-11-13 00:20 班）：**IgH 侧 CSV 画像已进 OP 并验证 11 字节 PDO 行为**——
    `scripts/spikes/igh-cia402-csv-op-probe.sh`（验证机本地 ecrt 夹具，源码不入库）：
    应用运行中 `ethercat slaves` = **OP** + `0x00000ec5:0x0000e5e1`，
    `RESULT al_states=0x08`，`STATUS sw=0x0237 modes_display=9 pos=2995 vel_actual=5`。
    证据：`docs/plugins/generic-cia402-csv-verification.md`。
  - 进度（2026-11-13 01:03 班）：**CiA 402 故障注入 CLI 已上架**——`ec-sim-esc --fault <hex>`
    （需 `--behavior cia402`）启动即注入故障（打印 `FAULT cia402 error_code=0xNNNN`）；
    合成主站 `--cia402-fault` + 脚本 `cia402-fault-probe.sh` 验证机 **EXIT=0**
    （Fault 0x0208 → 复位 0x0240、`0x603F=0x7500`）；顺带修复 `--manifest`+`--esi` 组合分支
    unreachable pattern，VM 构建 0 warning。测试 **185 passed**。
    ⚠️ 故障码清除时序仍 Tier0 简化、仅启动时注入，未做 IgH/SOEM 侧注入。
  - 进度（2026-11-13 01:42 班）：**SV660N 公开事实核对（第三方实测文档）**——经 linuxcnc-ethercat
    `documentation/inovance.md`（公开只读参考）确认：SV660N 每方向仅 0x1600/0x1A00、
    0x1C13 可写 0x1A00 或固定映射 0x1B01..0x1B04、无 0x1A01；PDO 上限 10 条、子 0x0B
    写以 0x06090011 中止；**设备不支持 SDO Info**；DC 默认开（assignActivate 0x300）；
    `0x6502=0x3AD`；H 参数映射 Hgg-pp = 0x2000+0xgg / sub 0xpp+1。画像更新为
    `sdo_info: false` + 新增 `0x6502` 对象；默认 PDO **具体条目**仍未知（保持 Tier0 7B 并标注）。
    验证机 `generic-cia402-probe.sh`（SV660N manifest，PID 0xE5F1）**EXIT=0**（SOEM 56/56、OP、WKC 3；IgH PREOP+身份）。
  - 待做：逐项核对 SV660N 公开手册 PDO/扩展对象；B6 FMMU/DC 原文核实；
    IgH SDO 环境阻塞（上传/下载，与 CSV 无关）。
- [x] ~~**B8 双主站冒烟**~~ ✅ **2026-11-12 19:56 班完成（单从站 8 位 I/O 画像）**：
  **未修改的 SOEM 与 IgH 都在本模拟器上看到从站进入 OP**，输出可复现脚本见
  [spike-4](docs/spikes/spike-4-dual-master-op.md)：
  - SOEM：`simple_ng` → `mapped 1O+1I`、`all slaves are now operational`、1055 轮 **WKC 3**；
    `slaveinfo -map` 打印 CoE 映射 `0x7000:0x00 8bit / 0x6000:0x00 8bit`。
  - IgH 1.6.13：ecrt 应用配置 domain 后 `ethercat slaves` 显示 **OP**、应用 `al_states=0x08`、
    domain `WKC 3`；内核日志 `Working counter changed to 3/3` / `Slave states: OP`；
    `ethercat upload/download` 可读写对象字典。
  - 断言脚本：`soem-process-data.sh`、`soem-mailbox-probe.sh`、`sdo-upload-probe.sh`、
    `igh-op-probe.sh`、`minimal-esc-responder.sh`。
  - 已知限制（见 spike-4 §6，2026-11-13 已更新）：单从站；过程数据内容静态（无 PDI 回环）；
    DC 未实现（无 SII DC 类别）；SM watchdog 已实现（SET/ACK+0x001B）；OP 下 AL 错误恢复
    仅基础 ACK；无 GUI 截图（以脱敏文本输出为证据）；SDO Info/分段/Complete Access 均已实现
    并验证（IgH 侧 SDO 上传/下载在验证机仍受 ecrt ioctl 阻塞，⚠️ 属于环境问题）。

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