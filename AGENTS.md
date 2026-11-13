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
  - 进度（2026-11-13 02:46 班）：**SOEM DC 配置路径验证**——未修改 SOEM `simple_ng` 的 `ecx_configdc()` 在 SV660N 画像上 `Configuring distributed clock... done`、`mapped 7O+7I`、OP、`WKC 3`；应答器 `--verbose` 新增 `dc_rd/dc_wr` 统计（本次 `dc_rd=5300 dc_wr=5302`），证明主站确实读写 DC 寄存器。新增 `scripts/spikes/soem-dc-probe.sh`；测试 **202 passed**。⚠️ 仅路径验证，非同步精度。
  - 进度（2026-11-13 02:52 班）：**IgH DC 配置路径验证**——ecrt 夹具显式 `ecrt_slave_config_dc(0x0300, 1ms)` 后仍进 OP：`al_states=0x08`、`sw=0x0237`、`torque=300`；应答器 `STATS dc_rd=5 dc_wr=8`。新增 `scripts/spikes/igh-dc-op-probe.sh`（夹具为验证机本地文件，不入库）。⚠️ 仅配置路径，非同步精度。
  - 进度（2026-11-13 02:57 班）：**SV660N 画像 IgH ecrt OP+CSP+DC 验证**——`scripts/spikes/igh-sv660n-op-probe.sh` **EXIT=0**：`ethercat slaves` 显示 `OP + 0x00000ec5:0x0000e5f1`；`RESULT al_states=0x08`；`STATUS sw=0x0637 modes_display=8 pos=12345`（启用+Target reached+位置跟随）；应答器 `dc_rd=3 dc_wr=7`。ecrt 夹具为验证机本地文件（不入库）。⚠️ 默认 PDO 仍为 Tier0 通用布局。
  - 进度（2026-11-13 03:05 班）：**HM（回零，模式 6）Tier0 行为**——`Operation enabled` 且控制字 bit4=1 时位置归零并置状态字 bit12（Homing attained），bit4 清除后不再置位；新增单测 `cia402_homing_mode_zeroes_position_and_sets_attained`、UDS 探针 `--cia402-homing`、脚本 `cia402-homing-probe.sh`；验证机 **EXIT=0**。测试 **204 passed**。⚠️ Tier0：不模拟寻零过程/开关信号/回零方式 0x6098。
  - 进度（2026-11-13 03:09 班）：**PP（轮廓位置，模式 1）Tier0 行为**——enabled 且控制字 bit4（new set-point）时位置跳变并置 bit10 Target reached + bit12 Set-point acknowledge；清 bit4 后 ack 清除、位置/到位保持。新增单测 `cia402_pp_mode_applies_new_setpoint`、`--cia402-pp` 探针与 `cia402-pp-probe.sh`；验证机 **EXIT=0**。测试 **205 passed**。⚠️ Tier0：瞬时到位、无轨迹/加速度/限幅。
  - 进度（2026-11-13 03:13 班）：**PV（轮廓速度，模式 3）Tier0**——11 字节布局复用 CSV 速度累加语义（实际速度显示目标速度、禁用归零）；新增单测 `cia402_pv_mode_accumulates_velocity`、`--cia402-pv` 探针与 `cia402-pv-probe.sh`；验证机 **EXIT=0**。测试 **206 passed**。当前 Tier0 模式：PP/CSP/CSV/PV/CST/HM。⚠️ 均为 Tier0 简化。
  - 进度（2026-11-13 03:17 班）：**TQ（转矩，模式 4）Tier0**——5 字节布局复用 CST 转矩镜像语义（禁用归零）；新增单测 `cia402_tq_mode_mirrors_target_torque`、`--cia402-tq` 探针与 `cia402-tq-probe.sh`；验证机 **EXIT=0**。测试 **207 passed**。Tier0 模式覆盖：PP/PV/TQ/CSP/CSV/CST/HM（均简化）。
  - 进度（2026-11-13 03:21 班）：**VL（速度，模式 2）Tier0**（与 PV 同构，速度累加语义）；并**修复可复现性缺口**——此前 `uds_master_probe.py` 的 PV/TQ 探针块/参数因脚本写法问题未真正进入 git（验证机用的是手工同步副本），本轮重新写入并验证 PV/VL、TQ 探针均 **EXIT=0**。新增单测循环覆盖 PV=3/VL=2。测试 **207 passed**。
  - 进度（2026-11-13 03:28 班）：**IP（插补位置，模式 7）Tier0**——7 字节布局复用 CSP 位置跟随语义；新增 `--cia402-ip` 探针与 `cia402-ip-probe.sh`；验证机 **EXIT=0**。至此 **CiA 402 常用模式全覆盖（Tier0 简化）**：PP(1)/PV(3)/VL(2)/TQ(4)/IP(7)/CSP(8)/CSV(9)/CST(10)/HM(6)。测试 **207 passed**。
  - 进度（2026-11-13 03:32 班）：**`0x6502` 与各画像 PDO 布局一致**——7 字节 `0x01C1`（pp/hm/ip/csp）、11 字节 `0x03CD`（pp/vl/pv/hm/ip/csp/csv）、5 字节 `0x0410`（tq/cst）、SV660N 保留公开实测 `0x03AD`；新增守卫测试遍历 4 个画像。测试 **208 passed**。
  - 进度（2026-11-13 03:36 班）：**`0x6502` 经 SDO 上传验证**——新增 `--expect-6502=<hex>` 探针参数与 `scripts/spikes/cia402-modes-probe.sh`，对 7B/11B/5B 三个画像分别 SDO 上传 `0x6502` 并比对（0x01C1/0x03CD/0x0410）；验证机 **EXIT=0**。测试 208 passed。
  - 进度（2026-11-13 03:46 班）：**SV660N 默认可变 PDO 已按公开手册核对并落地**——本地文本提取公开通信手册（PDF **不入库**）：RPDO `0x1600`=6040h/607Ah/60B8h（8B）、TPDO `0x1A00`=603Fh/6041h/6064h/60BCh/60B9h/60BAh/60FDh（22B）；固定映射 0x1701..1705/0x1B01..1B04 条目与 1C12/1C13 选择规则、DC 寄存器细节记入 facts.md（固定映射暂不生成）。验证机：`sv660n-capability-probe` **EXIT=0**（SOEM 64/176 bits、Has DC 1）、`igh-sv660n-op-probe` **EXIT=0**（IgH 默认映射 + DC 仍进 OP、al_states=0x08、dc_wr=8）。⚠️ 固定映射位长为推算、厂商扩展对象/故障码表未收录。
  - 进度（2026-11-13 03:53 班）：**固定映射支持 + SV660N PP/CSP 固定映射落地**。manifest 新字段 `pdo.fixed`（只生成映射对象，不进 SII、非默认分配）；`0x1701`（4 对象/12B）与 `0x1B01`（9 对象/28B）已生成，位长与手册字节数吻合；守卫测试 `sv660n_fixed_pdo_maps_match_public_manual`；验证机 `--dump-dict` 确认 0x1701/0x1B01、`sv660n-capability-probe` 仍 EXIT=0。剩余 1702..1705/1B02..1B04 仅事实记录。测试 **209 passed**。
  - 进度（2026-11-13 04:03 班）：**SV660N 全部 9 个固定映射落地**（`0x1701..0x1705`、`0x1B01..0x1B04`）——映射对象全部生成（仅字典，不进 SII/非默认分配）；每组条目数/位长总和与公开手册字节数一致；守卫测试 `sv660n_fixed_pdo_maps_match_public_manual` 覆盖 9 组；新增 60FF/6071/607F/60E0/60E1/60B2/606C 对象。测试 **209 passed**。
- [x] ~~**B12 多从站帧链（M1 退出项）**~~ ✅ **2026-11-13 04:09 完成（UDS shim 范围）**：`ec-sim-esc --slaves N`（N=1..64）用 N 个 `Responder` 链式处理帧；新增 `esc_node::handle_frame_chain`（按链序处理同一帧、AP 命令 ADP 每级 +1、WKC 逐级累加，只回一帧）。验证：合成主站 `uds-multi-slave-probe.sh`（广播 WKC=8、逐站分配地址、配置地址寻址）；未修改 SOEM 经 UDS shim `soem-multi-slave-probe.sh` **EXIT=0**（`8 slaves found and configured`、`mapped 8O+8I`、WKC 24 稳定、8 个 Slave 块）。⚠️ 未验证：TAP 多从站（单接口回环限制）、1 kHz 频率/抖动 → B13。
- [x] ~~**B13 TAP 多从站 + 1kHz/抖动基准**~~ ✅ **2026-11-13 04:29 完成（含环境限制）**：`tap-multi-slave-probe.sh`（TAP 8 从站：SOEM `slaveinfo` 8 slaves/WKC 24、`simple_ng` 8O+8I/WKC 24，**EXIT=0**）；实测 TAP 循环 **≈425 Hz**（非隔离 CPU，未达 1 kHz）。新增 `scripts/bench/{shim-latency.sh,uds-latency.py}`：UDS shim 1 从站 p50≈110µs/p99≈201µs、8 从站 p50≈112µs/p99≈191µs（非隔离 CPU，未达 p99<100µs）。⚠️ 1 kHz/p99 目标需隔离 CPU+实时调度 → B14 复测。
- [x] ~~**B14 M1 收尾：Docker 无特权跑通 + 隔离 CPU 复测 1kHz/p99**~~ ✅ **2026-11-13 04:45 完成（容器部分；隔离 CPU 拆出）**：`docker/Dockerfile`（标准多阶段）+ `docker/Dockerfile.prebuilt`（离线预构建变体，验证机无 registry 网络时用）+ `scripts/spikes/podman-unprivileged-probe.sh`——podman 容器**无 `--privileged/--network=host`** 内跑 `--slaves 8` UDS shim，合成主站全链路 **EXIT=0**（容器内启动、广播 WKC/地址分配、UDS 全链路；共享卷按同路径挂载以让 peer pathname 在容器内可解析）。⚠️ 标准多阶段 Dockerfile 未实际构建（验证机无 registry 网络，⚠️ 待有网环境复核）；隔离 CPU 下的 1kHz/p99 复测未做 → B15。
- [x] ~~**B15 隔离 CPU 下的 1kHz/p99 复测**~~ ✅ **2026-11-13 05:01 完成（记录未达标 + 环境限制）**：验证机配置 `isolcpus=2,3`（grubby，已写入 /etc/default/grub；`/sys/devices/system/cpu/isolated=2-3`）。复测：TAP 8 从站 `simple_ng` 在 sim/Soem 绑核 + `chrt` RT 下仍 **≈425 Hz**（单帧约 2.35 ms，1 kHz 未达成）；UDS shim 1 从站 RT+绑核 p50≈101µs / p99≈172µs（未绑核 p99≈196µs）、8 从站 p99≈181µs（p99<100µs 未达成，UDS+Python 探测开销为主）。`scripts/bench/shim-latency.sh` 与 `tap-multi-slave-probe.sh` 增加 SIM_CPU/PROBE_CPU/SIM_RT/PROBE_RT 等开关。⚠️ 结论：M1 的 1kHz/p99 在当前 TAP/UDS+SOEM/Python 组合下达不到，需原生基准/优化帧处理。
- [x] ~~**B16 1kHz/p99 优化或 ADR 记录限制**~~ ✅ **2026-11-13 05:12 完成（ADR 记录 + 微基准）**：新增 `crates/esc-node/examples/chain_latency.rs`（8 从站链处理验证机 ≈0.48 µs/帧、本机 ≈0.76 µs/帧，证明模拟器侧仅占 1 ms 周期约 0.05%）；新增 [ADR 0001](docs/adr/0001-m1-1khz-p99-limits.md)：TAP 下 ≈425 Hz 瓶颈在 SOEM TAP 收包线程/调度，UDS p99≈172 µs 主要由 UDS syscall+Python 探测+调度构成；**不宣称 M1 性能指标已达成**，M1 清单第 2/3 项标 🟡。
- [x] ~~**B17 原生主站/延迟基准（M1 性能复测）**~~ ✅ **2026-11-13 05:16 完成**：新增 `crates/esc-node/examples/uds_latency.rs`（Rust 直接收发 EtherCAT 帧）+ `ec-sim-esc --poll`（UDS 忙轮询）。验证机 `isolcpus=2,3`、sim/probe 绑核：默认阻塞 p50≈93µs/p99≈157µs（两次进程唤醒调度主导）；sim `--poll` + probe `spin` 忙轮询 **p50≈3.7µs/p99≈9.3µs（1 站）、8 站 p99≈14.6µs** → **M1 p99<100µs 在忙轮询模式下达成**（默认阻塞仍未达；忙轮询 CPU 占用高、默认不开）。8 从站 @1kHz：shim 链路能力成立（单轮 3.5–4.5µs），但 SOEM 样例 shim ≈860Hz / TAP ≈425Hz 未达。详见 [ADR 0001](docs/adr/0001-m1-1khz-p99-limits.md)。
- [x] ~~**B18 M2 起点：只读拓扑/状态视图**~~ ✅ **2026-11-13 05:26 完成**：`ec-sim-esc --dump-state [--manifest|--esi]` 输出 **state-v1 JSON**（身份/名称/邮箱/CoE/general flags/SM 列表/PDO 映射/DC/画像/字典对象数；无外部依赖手写 JSON）；`web/` 零依赖只读页（`index.html`+`app.js`，文件选择或同目录 `state.json`）渲染身份/SM/PDO/DC；`web/README.md` 用法；探针 `scripts/spikes/dump-state-probe.sh`（2 manifest + 内置画像 JSON 校验）验证机 **EXIT=0**。测试 210 passed。
- [x] ~~**B19 `--dump-dict` JSON 输出 + Web 对象字典视图**~~ ✅ **2026-11-13 05:31 完成**：`--dump-dict --json` 输出 **dict-v1**（对象/条目：子索引/类型/位长/访问/名称/值 hex）；`web/app.js` 支持 state-v1 与 dict-v1 两类输入（可分别载入或静态服务自动读取 state.json/dict.json），新增对象字典表格（值显示为大端 hex + 原始小端 hex）；探针 `scripts/spikes/dump-dict-probe.sh` 校验 2 个画像的 0x6502/0x6040 与纯文本兼容性，验证机 **EXIT=0**。测试 210 passed。
- [x] ~~**B20 控制面 `control-plane` 最小 API 骨架（M2 数据面）**~~ ✅ **2026-11-13 05:35 完成**：`control-plane` 新增 `state_json`（state-v1）/`dict_json`（dict-v1）只读快照序列化与极简路由表（`ROUTES`：`/state`、`/dict`，`route()` 查找），依赖仅 `esc-core`（零网络/序列化依赖）；`ec-sim-esc --dump-state/--dump-dict --json` 改为调用该 API（输出与之前**逐字节一致**，`fc /b` 验证）；4 个单元测试（路由、state 含身份/SM/PDO/profile、内置空 profile、dict 条目一致）。测试 **214 passed**。
- [x] ~~**B21 控制面 HTTP JSON API（M2 只读面）**~~ ✅ **2026-11-13 05:41 完成**：`ec-sim-esc --serve <addr>`（std `TcpListener`，无外部依赖）启动时生成快照，提供 `GET /state`、`GET /dict`、`GET /health`；`control-plane::handle_get` 纯函数处理路由（404 带 routes、CORS `*`、`Cache-Control: no-store`），5 个单元测试；`web/` 增加“从 API 载入”（fetch `/state`+`/dict`）；探针 `scripts/spikes/http-state-probe.sh`（manifest 画像 + 内置画像 /dict 404）验证机 **EXIT=0**。测试 **215 passed**。
- [x] ~~**B22 `--serve` 实时状态模式（M2）**~~ ✅ **2026-11-13 05:46 完成**：`--serve` 与 `--uds` 同进程时进入**实时模式**（`Arc<Mutex<Vec<Responder>>>` 共享，HTTP 线程按请求加锁读取；`--tap` 实时暂不支持，报错提示）。`control-plane` 新增 `LiveState` 与 `live_state_json`（在 state-v1 中注入 `live`：站地址/AL 状态/AL 状态码/DC 时间/帧与邮箱/DC 统计），6 个 control-plane 测试；`web/` 增加“实时刷新（1s）”开关，轮询 `/state` 更新视图；探针 `scripts/spikes/live-state-probe.sh`（UDS APWR AL Control 0x02 → `/state.live.al_status` 0x01→0x02、frames_in≥1）验证机 **EXIT=0**。测试 **216 passed**。
- [x] ~~**B23 `--serve` 实时 PDO 快照 + Web 视图**~~ ✅ **2026-11-13 05:53 完成**：实时模式新增 `GET /pdo`（pdo-v1：站地址/AL/SM2/SM3 地址+原始字节+hex）；`control-plane` 新增 `PdoSnapshot`/`pdo_json` + 1 测试（共 7）；`Responder::read_process_data` 只读快照；`web/` 新增 PDO 十六进制 + u16 小端解码视图，API 载入/实时轮询同时拉取 `/pdo`；探针 `pdo-live-probe.sh`（初始全 0 → UDS 写 0xBEEF/0x1234 后 `/pdo` 实时变化）验证机 **EXIT=0**；`live-state-probe.sh` 仍通过。测试 **217 passed**。
- [x] ~~**B24 TAP 实时 `--serve`（M2）**~~ ✅ **2026-11-13 05:59 完成**：实时共享状态模式扩展到 TAP：`run()` 同样用 `Arc<Mutex<Vec<Responder>>>` + HTTP 线程（`--tap --serve` 不再报错；无 `--uds/--tap` 仍是静态快照）；探针 `scripts/spikes/tap-live-state-probe.sh`（root）用未修改 SOEM `simple_ng` 推入 OP，断言 `/state.live.al_status` 低 4 位=0x08、`frames_in>0`、`/pdo` pdo-v1，验证机 **EXIT=0**（AL=0xc8、frames_in=187）。测试 **217 passed** 不变。
- [x] ~~**B25 `/pdo` 历史缓冲 + Web 波形（M2）**~~ ✅ **2026-11-13 06:04 完成**：`--serve` 实时模式维护按 PDO 变化去重的环形缓冲（`PDO_HISTORY_CAPACITY=256`），新增 `/pdo?history=1`（pdo-history-v1：samples[] 每项 sm2/sm3 hex + al_status）；`control-plane` 新增 `pdo_history_json` + 测试（共 8）；`web/` 新增 Canvas 波形（SM2/SM3 前 2 字节 u16 折线），API 载入与实时轮询同时拉取历史；探针 `pdo-history-probe.sh`（去重、递增、末条匹配、上限）验证机 **EXIT=0**。测试 **218 passed**。
- [x] ~~**B26 PDO 具名解码（M2）**~~ ✅ **2026-11-13 06:09 完成**：`control-plane` 新增 `decode_pdo`（按 SII TxPDO/RxPDO 映射顺序，把 SM2/SM3 字节按位长解码为 i64；有符号类型按字典 `data_type` 符号扩展；名称取自字典）与 `pdo_json_decoded`（pdo-v1 追加 `decoded[]`），3 个单测（共 11）；实时 `/pdo` 返回 decoded；`web/` 新增“PDO 具名解码”表格；探针 `pdo-decode-probe.sh`（CST 5B：0x6040=15、0x6060=8、0x6071=300、0x6041=0x237、0x6061=8、0x6077=-100，名称 Controlword/Torque actual value）验证机 **EXIT=0**；`pdo-live-probe.sh` 仍通过。测试 **221 passed**。
- [x] ~~**B27 多从站拓扑 `/topology` + Web 拓扑表（M2）**~~ ✅ **2026-11-13 06:15 完成**：实时模式新增 `/topology`（topology-v1：每站 index/站地址/AL/AL 码/收帧/发帧/SM2/SM3 地址+hex）；`control-plane` 新增 `TopologyEntry`/`topology_json` + 1 测试（共 12）；`web/` 新增“总线拓扑”表，API 载入/实时轮询同时拉取；探针 `topology-probe.sh`（`--slaves 3`；ADP=-k 配置地址 1/2/3、写第 2 站 SM2=0xBEEF）验证机 **EXIT=0**；`pdo-decode-probe.sh` 仍通过。测试 **222 passed**。
- [x] ~~**B28 多从站查看（M2）**~~ ✅ **2026-11-13 06:20 完成**：实时模式 `/state?slave=N`、`/pdo?slave=N`（默认 N=0；越界/非法 N → 404 JSON，`control-plane::not_found_json` 公开复用）；PDO 历史缓冲改为**按从站分开**；`web/` 增加“从站”输入与拓扑行点击切换（实时轮询同参数）；探针 `slave-select-probe.sh`（N=1 站地址 0x0002/SM2=beef、N=0 SM2=0000、99/abc → 404）验证机 **EXIT=0**；`topology-probe.sh` 仍通过。测试 **222 passed**。
- [x] ~~**B29 `/metrics` 文本指标（M2 运维面）**~~ ✅ **2026-11-13 06:25 完成**：实时模式新增 `GET /metrics`（Prometheus 文本、`text/plain; version=0.0.4`，无外部依赖）：`ecsim_frames_in/out/dropped_total`、`datagrams_hit_total`、`mailbox_requests/responses_total`、`dc_reg_reads/writes_total`、`dc_time_ns`、`al_status`/`al_status_code`，每站带 `slave`/`station_address` 标签；`control-plane` 新增 `MetricsSlave`/`metrics_text` + 1 测试（共 13）；探针 `metrics-probe.sh`（两站 label、发帧后 `frames_in` 单调 0→3）验证机 **EXIT=0**；`slave-select-probe.sh` 仍通过。测试 **223 passed**。
- [x] ~~**B30 指标筛选 + Web 指标卡片（M2）**~~ ✅ **2026-11-13 06:30 完成**：`/metrics?slave=N` 仅导出指定从站（越界/非法 → 404）；`web/` 解析 `/metrics` 展示关键计数卡片（收/发帧、丢弃、datagram、邮箱、DC 读写、DC 时间、AL 状态/码），AL 状态用 `ok`/`warn` 色；探针 `metrics-filter-probe.sh`（全量 3 站、筛选 1 站、99/abc→404）验证机 **EXIT=0**；`metrics-probe.sh` 仍通过。测试 **223 passed**。
- [x] ~~**B31 控制面文档 `docs/CONTROL-PLANE.md`（M2）**~~ ✅ **2026-11-13 06:34 完成**：新增 `docs/CONTROL-PLANE.md`（启动模式/路由总表/示例/schema 摘要/安全边界/与 WebUI 关系/验证清单）+ `scripts/spikes/control-plane-doc-check.py`（核对文档路由表与代码逐条一致：11 条路径、10 个探针、源码引用；本机 EXIT=0）；README 增加“Control plane & Web UI”段并更新 WebUI 行链接；STATUS 增加控制面段；VERIFICATION 增行。测试 **223 passed** 不变。
- [x] ~~**B32 Web 拓扑链式 SVG 图（M2）**~~ ✅ **2026-11-13 06:37 完成**：`web/app.js` 新增 `renderTopologyGraph`（链式节点：圆点 + 连线 + 站地址/AL 文本，AL 状态色 INIT 橙/PREOP 蓝/SAFEOP 紫/OP 绿，点击节点切站）与 `alColor`；拓扑区先 SVG 后表格；`web/README.md` 重写为当前能力（API/实时/波形/拓扑/SVG），标注 ⚠️ 无自动化 DOM 测试（仅 `node --check` + 人工浏览器验收）。测试 **223 passed** 不变。
- [x] ~~**B33 `--serve` 同源托管 `web/`（M2）**~~ ✅ **2026-11-13 06:38 完成**：新增 `--web-root <dir>`：`--serve` 时 `GET /`、`/index.html`、`/app.js` 从 `web/` 白名单读取（HTML/JS 内容类型，无目录遍历；目录缺失回退纯 API）；静态快照与 UDS/TAP 实时模式都支持；`docs/CONTROL-PLANE.md`/`STATUS`/`web/README.md` 更新同源用法；新增 `scripts/spikes/serve-web-probe.sh`（静态模式 `/`+`/app.js`+`/state`、实时模式 live、无效 web-root 回退 API）验证机 **EXIT=0**。测试 **223 passed** 不变。
- [x] ~~**B34 `--serve` SSE `/events` 推送（M2）**~~ ✅ **2026-11-13 06:47 完成**：实时模式新增 `GET /events`（`text/event-stream`）：每连接独立线程推送 state-v1（含 live），默认 500ms、`?interval_ms=100..5000`、支持 `?slave=N`；data 压成单行 JSON；客户端断开自动退出；`web/` 实时刷新优先用 `EventSource`（状态推送 + 2s 拉 PDO/拓扑/指标），无 EventSource 回退 1s 轮询；探针 `events-probe.sh`（类型、state+live、两事件间 frames_in 0→2、断开后 `/state` 200）验证机 **EXIT=0**；CONTROL-PLANE/STATUS/web README 更新。测试 **223 passed** 不变。
- [x] ~~**B35 控制面写操作第一批（M2，默认关闭）**~~ ✅ **2026-11-13 06:56 完成**：新增 `--serve-write` 显式开关与 `POST /pdo`（body=原始 SM2 字节，`?slave=N`；默认 405、其他 POST 404；`Responder::write_process_data` 只写 DL 用户内存，不改 ESM/SDO/邮箱）；HTTP 请求解析支持方法/Content-Length/正文（`read_http_request`）；`docs/CONTROL-PLANE.md` 新增“写操作（默认关闭）”章节与安全边界，doc-check 增 `--serve-write`/POST 一致性断言；探针 `write-pdo-probe.sh`（默认 405 且 PDO 不变；开启后 slave0=beef、slave1=1234；POST /state 404）验证机 **EXIT=0**。测试 **223 passed** 不变。
- [x] ~~**B36 M2 退出清单与路线图对齐（M2 收尾）**~~ ✅ **2026-11-13 07:11 完成**：新增 `docs/plan/M2-exit-checklist.md`（范围/11 条退出条件与证据/未完成范围外/验证命令/结论），STATUS/README/CONTROL-PLANE 互链，M1 清单过时的 M2 段落同步（WebUI 只读已完成）；`control-plane-doc-check.py` 扩展：清单存在、引用全部探针、被 README/STATUS/CONTROL-PLANE 链接（**EXIT=0**）；结论 = M2“只读观测面 + 显式写开关”达到退出候选，SDO 编辑/插件/ESI/鉴权/MCP/32 从站划入 M3+。测试 **223 passed** 不变。
- [x] ~~**B37 一键预回归脚本 `scripts/spikes/run-all-probes.sh`（M2 收尾）**~~ ✅ **2026-11-13 07:13 完成**：新增聚合脚本：按 host/root 自动选择可跑探针（非 Linux 只跑跨平台项；TAP 需 root 且 `RUN_TAP` 允许），`--list` 查看计划，逐项 `timeout`（Windows 自动跳过 GNU timeout）记录日志，汇总 `PASS/FAIL/SKIP` 与 FAILED 名单，退出码 0/1；本机（Windows）**2 PASS / 0 FAIL / 12 SKIP**；验证机（Linux root）**14 PASS / 0 FAIL / 0 SKIP**（含 TAP live）。：按平台（Linux/其他）与 root 能力选择可跑探针（跳过 TAP/root 项并注明），逐个执行汇总 EXIT 与失败清单，末尾输出 CI 友好的 PASS/FAIL 摘要；本地可跑、验证机可跑。：新增 `docs/plan/M2-exit-checklist.md`（控制面只读 API/SSE/Web/指标/写操作开关/安全边界逐项勾选与验证命令），STATUS/AGENTS/README 与清单互链；验收 = 文档一致性检查（doc-check 扩展 M2 清单存在性）+ 人工过一遍清单。：新增 `--serve-write` 显式开启 `POST /pdo`（写 SM2 原始字节，`?slave=N`），默认 405/404 且 Web 不调用；探针校验默认拒绝与开启后写入生效；文档明确安全边界（仅本机/受信环境）。：新增 `GET /events`（`text/event-stream`）按固定间隔推送 `/state` 快照（`data: {...}`），Web 端可用 `EventSource` 替代 1s 轮询；探针校验事件行格式与至少一条事件、断连清理；无外部依赖。：`--serve <addr> [--web-root <dir>]` 时对 `GET /`、`/index.html`、`/app.js` 等返回静态文件（其余路由仍为 JSON API），消除跨源/CORS 依赖；探针校验 `GET /` 返回 HTML、`/app.js` 为 JS，且 API 路由不受影响。：`web/` 把 `/topology` 渲染为链式节点图（节点显示站地址/AL 状态色），点击节点切换从站；`node --check` 通过，浏览器人工验收（⚠️ 无自动化 DOM 测试，仅结构/语法校验）。：汇总只读 API 路由（`/state`、`/dict`、`/pdo`、`/pdo?history=1`、`/topology`、`/metrics`、`?slave=N`）、各 schema 示例、只读/无鉴权安全边界、与 Web 的关系；README 与 STATUS 互链。验收：文档链接可点、与代码路由逐条一致（脚本/清单核对）。：`/metrics?slave=N`（仅导出指定从站）与 Web 端解析 `/metrics` 展示关键计数卡片（frames/mailbox/AL 状态色）；探针校验筛选行数。：实时模式新增 `GET /metrics`（Prometheus 文本：ecsim_frames_in/out、mailbox、dc_reg、al_status 等，带站标签），探针校验格式与计数单调性；`web/` 可选显示。：`/state?slave=N` 与 `/pdo?slave=N`（默认 0；越界 404），`web/` 拓扑行可点击切换查看对应从站；探针校验 `--slaves 3` 下 N=1 的状态/PDO 与拓扑摘要一致。：实时模式返回所有从站摘要（站地址/AL/帧统计/SM2、SM3 摘要），`web/` 增加拓扑表；探针用 `--slaves 3` 校验 3 行与逐个 AL 状态。：`web/` 结合 state 的 PDO 映射与 dict-v1（对象名/类型/位长）把 SM2/SM3 字节解码为具名对象值（int8/16/32、位域），只读表格 + 波形可选；探针校验已知布局（如 CST 5B）解码结果。：`--serve` 维护最近 N 帧 PDO 环形缓冲（默认 256），新增 `/pdo?history=1`（或 `/pdo-history`）；`web/` 用 Canvas 画 SM2/SM3 时间序列；探针校验历史长度与最新值一致。：把实时共享状态模式扩展到 TAP 线缆层（`Arc<Mutex<Responder>>` 同样处理）；探针用 TAP + SOEM 流程后校验 `/state`/`/pdo` 实时值；TAP 需要 root（验证机执行）。：新增 `/pdo`（返回 SM2/SM3 过程数据原始字节 + WKC/AL），`web/` 增加 PDO 十六进制/按位表格视图并轮询；探针校验 PDO 随主站写入变化。：与 TAP/UDS 同进程运行，`/state` 动态反映 AL 状态/统计（共享 `Responder` 状态，线程安全），`web/` 定时轮询刷新；探针校验状态随 ESM 迁移变化。：`ec-sim-esc --serve <addr>`（std `TcpListener`，无外部依赖）提供 `GET /state`、`GET /dict`（复用 control-plane 快照），`web/app.js` 支持直接 fetch；探针用 HTTP 请求校验 JSON 与 CORS/缓存头（只读、无鉴权，仅本机默认）。：定义 `StateSnapshot`/`DictSnapshot` 共享数据结构与 JSON 编解码（复用 bin 的 dump 逻辑，避免重复）、极简路由表（`/state`、`/dict`）与单元测试；为后续 HTTP/JSON API 与 Web 实时视图打基础（不引入重依赖）。：给 `--dump-dict` 增加 `--json`（对象/条目/类型/访问/值），`web/app.js` 增加对象字典表格视图；探针校验 JSON 与字典一致性。：`ec-sim-esc` 增加 `--dump-state`（JSON：从站/AL/SM/FMMU/PDO 映射/DC 摘要），并加 `web/` 静态只读页（本地打开或由极简 std TcpListener 提供），为 WebUI 打基础（不引入重依赖）。：优化 TAP 8 从站单帧处理（当前 ≈2.35ms/帧）与 UDS 探测开销；或写 ADR 明确"M1 1kHz/p99 仅 ISOLATED+原生主站可达"的限制与复现命令。：在 isolcpus/实时调度环境复测 8 从站 TAP/shim 循环频率与 p99（目标 1kHz、p99<100µs），或记录硬件限制；并在有 registry 的环境构建 `docker/Dockerfile` 复核。：`docker/Dockerfile`（UDS shim，无特权）+ 容器内 `uds-loopback`/多从站冒烟；在隔离 CPU/实时调度下复测 8 从站 1kHz 与 p99<100µs（或记录硬件限制）。：在 TAP 上验证 `--slaves 8`（若回环限制不可行则记 ADR/限制）；加 shim 延迟基准脚本（p50/p99/p999）+ 隔离 CPU 说明；对照 M1 退出标准（8 从站 @1kHz、p99<100µs）。：ESC-over-UDS shim 上 1 个进程模拟 N 个 ESC（默认 8），按链序对每个 Responder 处理同一帧并累积 WKC，只回一帧；`--slaves N`（从站地址 = 链位置+1）。验收：合成主站 BRD/APRD 读到 1..N；未修改 SOEM `slaveinfo` 经 UDS shim 报 N slaves；现有单从站探针全部保持 EXIT=0。风险：TAP 单接口回环可能无法表达链式返回，必要时 TAP 多从站记为限制（ADR 记录）。详见 [docs/plan/M1-exit-checklist.md](docs/plan/M1-exit-checklist.md)。
- [x] ~~**B11 CiA402 行为插件**~~ ✅ **2026-11-13 完成（Tier0 范围）**：CSP/CSV/CST 及 PP/VL/PV/TQ/HM/IP 常用模式 Tier0 全覆盖（PP/VL/PV/TQ/HM/IP/CSP/CSV/CST）、`--fault` 注入/复位、`0x6502` 按布局声明；SV660N 兼容画像完成公开手册核对（默认可变 PDO 8B/22B、全部 9 个固定映射、DC 声明），SOEM/IgH 零补丁到 OP 验证。残留研究项（不阻塞）：固定映射位长逐项核对、厂商扩展对象、`0x603F` 故障码表、真实 DC SYNC0。详见 `docs/plugins/cia402-modes-matrix.md`、`plugins/inovance-sv660n/facts.md`。
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

- [x] ~~**B38 M3 范围与 ADR（插件宿主/WASM、SDO 写路径、鉴权/TLS）（M3 定界）**~~ ✅ **2026-11-13 07:28 完成**：新增 docs/adr/0002-m3-plugin-host-and-write-paths.md（D1 插件两类/行为插件不进实时环、D2 最小宿主 API、D3 SDO 写白名单+审计、D4 进程外 TLS+可选 token、D5 许可约束、D6 非目标、备选方案与未决问题）与 docs/plan/M3-scope.md（W1–W8 工作流/验收/风险/退出条件）；M2 清单/STATUS/README 互链；control-plane-doc-check 扩展 M3 文档存在性与互链（EXIT=0）。纯文档班，未跑 VM。
- [x] ~~**B39 M3-W3：SDO 只读浏览 `GET /sdo?index&sub`（M3）**~~ ✅ **2026-11-13 07:30 完成**：`control-plane::sdo_json`（运行时字典，不 mailbox；未知对象 None）+ 1 单测（共 14）；`GET /sdo?index=0x6040&sub=0` 静态与实时 `--serve` 均可用（sdo-v1：index/sub/object_name/name/type/bits/access/value_hex），缺 index/非法/未知对象/未知子索引 → 404；探针 `sdo-read-probe.sh` 验证机 **EXIT=0**；CONTROL-PLANE 路由/schema/验证、M3 清单 W3 状态、run-all-probes 清单同步；测试 **224 passed**。
- [x] ~~**B40 M3-W4：SDO 受控写 `POST /sdo`（M3）**~~ ✅ **2026-11-13 07:35 完成**：`--serve-write` 下启用 `POST /sdo?index&sub`（body=原始值字节）：仅 manifest `writable` 对象可写、安全排除表（0x1000/0x1008/0x1009/0x1010/0x1011/0x1018/0x1C12/0x1C13）403、未知 404、长度错 400；写入进 `Responder::write_object_override`，`/sdo` 读回覆盖值；新增 `GET /sdo-audit`（sdo-audit-v1，≤64 条）与 `/metrics` 的 `ecsim_sdo_writes_total`；静态模式非 GET 统一 405；探针 `sdo-write-probe.sh` 验证机 **EXIT=0**；CONTROL-PLANE/M3 清单/doc-check/run-all 同步；测试 **225 passed**。
- [x] ~~**B41 M3-W5：可选静态 token（M3）**~~ ✅ **2026-11-13 08:20 完成**：`--serve-token-env VAR` 从环境变量读静态 Bearer token（空/未设置 = 现状）；`control-plane::{AuthDecision,authorize,bearer_token,is_protected_route}` 纯函数 + 5 单测（共 20）；非空时 `/state`、`/dict`、`/pdo`、`/sdo`、`/sdo-audit`、`/topology`、`/metrics`、`/events`（含 `POST /sdo`/`POST /pdo`）必须 `Authorization: Bearer <token>`：缺头/非 Bearer → 401（`WWW-Authenticate: Bearer`）、错误 token → 403、正确 → 200；静态资源与 `/health` 豁免；http 请求解析统一为 `HttpRequest`（含请求头）并抽出 `write_http_response`；探针 `serve-token-probe.sh` 验证机 **EXIT=0**（静态矩阵 + UDS 实时写/审计/指标/SSE）；run-all-probes 验证机 **17 PASS / 0 FAIL**（新增 serve-token）；测试 **230 passed**。
- [x] ~~**B42 M3-W6：Web SDO 面板（M3）**~~ ✅ **2026-11-13 08:00 完成**：`web/` 新增 SDO 面板——按 `index`/`sub` 调 `GET /sdo` 只读浏览（对象名/条目名/类型/位长/访问/值大端+原始小端 hex，`cache: no-store`）；写入默认关闭：勾选“允许写入”→ 填小端 hex → `window.confirm()` **二次确认**后才 `POST /sdo`（`Uint8Array` 原始字节体），并映射 405（未开 `--serve-write`）/401（token 需反向代理或 curl）/403/404/400；页面提示 token 模式限制、不保存 token。验证：`node --check web/app.js`、新增 `scripts/spikes/web-sdo-panel-check.py`（静态资源回归：控件 id + 逻辑标记 + README 断言 + node 语法；⚠️ 无 DOM 自动化）、`serve-web-probe.sh` 增加同源资源断言；验证机 `run-all-probes.sh` **18 PASS / 0 FAIL / 0 SKIP**；CONTROL-PLANE/web/README/M3 清单/VERIFICATION/STATUS/README 同步。⚠️ 浏览器人工点击验收待补。
- [x] ~~**B43 M3-W1：WASM 插件宿主 spike（M3 主阻塞项）**~~ ✅ **2026-11-13 08:05 完成**：选型 **wasmi 0.31.2**（MIT/Apache-2.0 解释器；MSRV 兼容 workspace 1.75，wasmtime 50 需 Rust ≥1.97/依赖数百包）；`plugin-host` 新增**默认关闭**的 `wasm-runtime` feature——`env.read_pdo`/`env.write_pdo`/`env.log` + `tick` 导出、PDO arena 快照（64B）、日志上限 64KiB、`WasmHostError`；WAT 夹具 `fixtures/wasm/counter.wat` + 示例 `wasm_plugin_demo` + 探针 `wasm-plugin-probe.sh`；+7 feature 单测（plugin-host 34）、release 实测实例化 605µs / tick 179.5ns / 20 依赖包；默认 `cargo test --workspace` 仍 **230 passed** 且零第三方依赖。证据：[spike-6](docs/spikes/spike-6-wasm-host.md)。⚠️ 无 fuel/超时/arena 同步（W2）、验证机未跑该探针（需 crates.io）。
- [x] ~~**B44 M3-W2：PDO arena 同步与插件隔离（M3）**~~ ✅ **2026-11-13 08:31 完成**：双缓冲/seqlock + generation（`plugin-host::arena`）、wasm 插件独占 worker 线程（`wasm_worker`，宿主永不 join 卡住的插件）、fuel/超时 + 内存上限、零撕裂/零分配压测、实例生命周期接 `esc-node` 非实时帧循环（`esc_node::behavior::BehaviorBridge` + manifest `behavior:`）、仓库内真实行为画像 `plugins/behavior-counter/` + 端到端探针 `wasm-behavior-probe.sh`（合成主站帧写 SM2 → worker tick → SM3 = SM2 + 1）。证据：各班进度、`crates/esc-node/tests/behavior_counter_plugin.rs`、`plugins/behavior-counter/README.md`。增量项（不在原范围）：插件 panic 重启/多实例配额、Linux CLI UDS/TAP 端到端（验证机需 crates.io）。
  - 进度（2026-11-13 08:35 班）：**W2 子项 1：arena 双缓冲/seqlock + generation 已实现**——
    `plugin-host::arena`（默认编译、零第三方依赖、模块内无 `unsafe`）：`PdoArena::new`（容量固定）/
    `publish`（写者：长度预检 + 单写者 CAS + 双缓冲原子拷贝，**不分配、不等待读者**）/
    `snapshot`/`try_snapshot`（读者：前后序列号一致才接受）+ `ArenaSnapshot`（含 generation / 陈旧判断）；
    4 个单测（往返/非法容量/并发写拒绝/2 万次发布 × 并发读者**零撕裂**压测）+
    `crates/plugin-host/tests/arena_no_alloc.rs` 计数分配器守卫（发布与失败路径 **0 次分配**）。
    测试 **236 passed**。⚠️ W2 剩余：插件独占 worker 线程、fuel/超时、内存上限、崩溃隔离、
    接 `esc-node` 非实时线程。
  - 进度（2026-11-13 08:45 班）：**W2 子项 2：worker 线程隔离 + fuel/超时 + 内存上限已实现**——
    `plugin-host::wasm` 增加 `HostConfig{pdo_len,max_memory_bytes,fuel_per_tick}` 与
    `from_wat_with_config`/`from_wasm_bytes_with_config`（wasmi `consume_fuel` + `StoreLimits`；
    默认值仍等于 B43 行为）、`WasmHostError::OutOfFuel`；新增 feature 门控的
    `plugin-host::wasm_worker`：`WasmWorker::spawn_*` 把编译/实例化整体放进**独占线程**，
    宿主投递 `ArenaSnapshot`（`[SM2|SM3]`）并用 `recv_timeout` 有界等待——超时
    `WorkerError::Timeout`（迟到事件按请求 id 丢弃，worker 继续服务）、死循环由 fuel 终止为
    `OutOfFuel`、`Drop`/`shutdown` **只发信号并 detach（绝不 join 卡住的插件）**。
    9 个新单测（含「死循环 <2s 内被 fuel 终止」「宿主 1ms 超时不被阻塞且下一 tick 恢复」
    「drop 繁忙 worker <200ms 返回」「4 页内存被 64KiB 上限拒绝」）；`wasm-runtime` 下
    **52 passed**，默认 `cargo test --workspace` 仍 **236 passed**。
    ⚠️ 未在 Linux 验证机跑（需 crates.io 拉 wasmi）；wasmi 0.31 无异步中断 →
    `Timeout` 只是宿主不再等，CPU 上界仍须配有限 `fuel_per_tick`；插件 panic 会杀线程；
    未接 `esc-node`、未做多插件配额。
  - 进度（2026-11-13 08:20 班）：**W2 子项 3：行为插件接 `esc-node` 非实时线程已实现**——
    `plugin-host::manifest` 新增 `behavior:`（`BehaviorConfig`：code/tick_hz/fuel_per_tick/
    max_memory_bytes/tick_timeout_ms，默认 1kHz/50ms；code 相对 manifest 目录并拒绝绝对路径/`..`）；
    `plugin-host::behavior::BehaviorRuntime`（`wasm-runtime`）——宿主只 `publish` arena +
    `poll_output`（`try_lock`、同 generation 只交付一次），独立 ticker 线程按 `tick_hz` 节流、
    每 generation 至多一次 tick、超时只放弃本次结果、`Drop` 只置位并 detach（不 join 卡死的插件）；
    `esc-node` 新增 feature `wasm-runtime` + `esc_node::behavior::BehaviorBridge`（SM2/SM3 ↔
    arena ↔ worker，无分配读路径 `copy_process_data`）；`ec-sim-esc` 的 UDS/TAP 帧循环在
    `--manifest` 声明 `behavior:` 后每帧 publish+poll，未启用 feature 时打印忽略提示。
    测试 **238 passed**（默认）/ `wasm-runtime` 下 plugin-host **53**、esc-node **73**；
    Linux 目标交叉 `cargo check`（默认 + feature）exit 0。⚠️ 仓库内暂无声明 behavior 的画像、
    bin 运行时未在验证机端到端（验证机无 crates.io）；未做 panic 重启/多实例配额。
  - 进度（2026-11-13 08:31 班）：**W2 子项 4（收尾）：仓库内真实行为画像 + 端到端探针**——
    新增 `plugins/behavior-counter/`（自写 `behavior.wat`：SM2 u32 小端 + 1 写回 SM3，首 tick 写日志；
    manifest `behavior:{code: behavior.wat, tick_hz: 1000, fuel_per_tick: 1e6, tick_timeout_ms: 50,
    max_memory_bytes: 65536}`，身份自有 VID/PID + SIMULATED + sources）。
    新增 `crates/esc-node/tests/behavior_counter_plugin.rs`（`wasm-runtime` 门控）：磁盘 manifest →
    `validate()` → `BehaviorBridge::from_manifest` → 合成主站帧 APWR `0x1100`=41 / APRD `0x1110`
    → 断言 SM3 = 42、单帧 <50ms、`published_frames>0`、ticks ≥ 1、超时/fuel/错误为 0、SM2 未被改写。
    新增 `scripts/spikes/wasm-behavior-probe.sh`（本机可直接跑；无 cargo/crates.io 时 exit 2 = SKIP），
    并同步 plugins/README、M3 清单 W2、STATUS、VERIFICATION、README。
    测试 **238 passed**（默认，新集成测试在默认 feature 下为空）；`cargo test -p esc-node --features
    wasm-runtime` **74 passed**（含新集成）、plugin-host 同 feature **58 passed**；
    fmt/clippy（默认 + 两 crate feature）全 0。⚠️ Linux CLI UDS/TAP 行为插件端到端仍未跑（验证机无
    crates.io）；验证方法：`cargo run -p esc-node --features wasm-runtime -- --uds /tmp/x --manifest
    plugins/behavior-counter/plugin.yaml`。

- [x] ~~**B45 M3-W7/W8：32 从站 @1 kHz 瓶颈调研 + MCP/Agent 桥接 stub（M3）**~~ ✅ **2026-11-13 完成**：
  两个子项均为文档/静态检查交付（W7 **不承诺达成**；W8 **只读、不含写**；均无外部依赖）。
  - 进度（2026-11-13 09:34 班）：**M3-W7 完成**——新增
    [docs/plan/M3-W7-32slave-1khz-bottleneck.md](docs/plan/M3-W7-32slave-1khz-bottleneck.md)：
    ① 修正测量口径（SOEM `simple_ng` 样例含 fixed `osal_usleep(5000)`，`iterations/T`
    ≈158–170/s 不是控制环频率；主站侧指标改用每轮 roundtrip_time，ADR 0001 的 425/860 Hz
    记录标 ⚠️ 待复核）；② `chain_latency` 扩展到 1..64 从站并输出边际成本——
    验证机 32 站 **1.70 µs/帧**（Windows 2.99 µs），≈0.17% 的 1 ms 预算；
    ③ `tap-multi-slave-probe.sh` 支持任意 `N` 并改为统计 `usec WKC` 往返——
    TAP + 未修改 SOEM：8 站 avg **164 µs**、32 站 avg **167 µs**（往返与从站数近似无关，
    TAP 唤醒主导；32 站单轮仍在 1 ms 内）；④ 瓶颈分解/可行路径/下一轮 RT 验证建议。
    ⚠️ 未测多 datagram/segment 帧形状与隔离 CPU+RT 下 1 kHz 实际发帧 p99（建议 B46）。
  - 进度（2026-11-13 08:50 班）：**M3-W8 完成**——新增
    [docs/plan/M3-W8-mcp-agent-bridge-stub.md](docs/plan/M3-W8-mcp-agent-bridge-stub.md)
    （D1–D7：stdio JSON-RPC 2.0、独立 `ec-sim-mcp` 进程经 `--api` 只发 GET、只读 allowlist、
    零第三方依赖、原样透传端点 JSON、一工具一端点 `ecsim_*`、快照语义如实告知）+
    机器可读目录 [m3-w8-mcp-tools-v1.json](docs/plan/m3-w8-mcp-tools-v1.json)
    （**9 个只读工具** + 6 个排除项：`/`、`/index.html`、`/app.js`、`/events`(SSE)、
    `POST /sdo`、`POST /pdo`——写路径**永不映射**）+ 静态校验
    `scripts/spikes/mcp-tools-check.py`（工具全 GET、查询参数变体有声明、文档 GET 路由
    全覆盖或显式排除、设计文档/ M3 清单/ run-all 互链）本机 **EXIT=0**；
    M3 scope W8 ✅、VERIFICATION/STATUS/run-all-probes 同步。MCP server 实现（S1/S2）列为 B47。
    ⚠️ 未实现 server、未跑 stdio 探针（B47）。
- [x] ~~**B46 原生 32 站 @1 kHz 基准（M3-W7 后续）**~~ ✅ **2026-11-13 完成**：
  新增 `crates/esc-node/examples/chain_1khz.rs`（原生 Rust 1 ms 周期循环：`hybrid`/`sleep`/`spin`，
  Linux 可选 `SCHED_FIFO` 且零第三方依赖）+ `chain_latency` 扩展 `single`/`lrw`/`fprd` 帧形状；
  新增 2 个单测（多 datagram 链、32 站 LRW WKC 96）。数据见
  [docs/plan/B46-native-32slave-1khz.md](docs/plan/B46-native-32slave-1khz.md)：
  32 站 PDO 式 LRW 链处理 **4.27 µs（0.43% 预算，WKC 96）**、最坏多 datagram 13.96 µs；
  1 kHz 5000 轮 Windows 忙轮询/混合与验证机隔离 CPU 忙轮询均 **0 超限**；`hybrid --rt` 0.10–0.12%
  超限（VMware 宿主抢占 tail ≈7 ms，非链处理）；**RT 忙轮询被 `sched_rt_runtime_us=950000`
  节流 → 2.21% 超限**（临时设 -1 后 0/5000，测后已恢复 950000）。
  **不承诺对外 SLA**；未测 TAP/UDS 闭环 1 kHz、DC、小时级稳定性（⚠️ 待验证）。
- [x] ~~**B47 `ec-sim-mcp` stdio 实现（M3-W8 后续）**~~ ✅ **2026-11-13 完成**：
  新增 crate `crates/mcp-server`（bin `ec-sim-mcp`，**零第三方依赖**）：JSON-RPC 2.0 over stdio
  （`initialize`/`ping`/`tools/list`/`tools/call`），工具表 `include_str!` 嵌入
  [m3-w8-mcp-tools-v1.json](docs/plan/m3-w8-mcp-tools-v1.json)（单一事实来源）；HTTP 客户端只发 GET、
  默认只连回环地址、`--token-env` 读 Bearer token、5 s 超时/1 MiB 上限；16 个单测。
  端到端探针 `scripts/spikes/mcp-stdio-probe.sh` + `mcp-stdio-probe.py`：9 个只读工具、
  state/dict/sdo 读取、未知工具/缺参/静态模式 live-only 工具/未知方法/坏 JSON、以及
  `--serve-write` 下工具表不变（18 项断言）——**本机与 Linux 验证机均 EXIT=0**；已登记进
  `run-all-probes.sh`（`mcp-stdio`，any）。**不含写**。证据：
  [M3-W8 实现说明](docs/plan/M3-W8-mcp-agent-bridge-stub.md) §8/§9。
  ⚠️ 实时模式（UDS/TAP）live-only 工具端到端、token 模式矩阵、调用审计日志未覆盖（待补）。
- [x] ~~**B48 M3 退出条件核对与收尾**~~ ✅ **2026-11-13 完成**：新增
  [docs/plan/M3-exit-checklist.md](docs/plan/M3-exit-checklist.md)——五项退出条件逐条核对（4 达成、1 部分：
  W6 真人浏览器点击待补）；验证机整轮 `run-all-probes.sh` **20 PASS / 0 FAIL / 0 SKIP**（含 `mcp-tools`/
  `mcp-stdio`/`tap-live`）；本机 fmt/clippy/test/doc 全 0（**256 passed**）；许可/依赖核对（默认 **0 第三方**，
  可选 `wasm-runtime` 25 个均宽松许可；无 NOTICE 义务）；ADR 0002 → **Accepted**；STATUS/VERIFICATION/M3
  清单互链。W6 渲染烟测（无头 Edge 截图）发现"载入区重复渲染"低危缺陷，**Web HMI 冻结**未修，转交 HMI GO 后。
  只写"用例映射/预回归"，**不宣称 CTT/认证**。
- [x] ~~**B49 M4 测试报告与 CI 接入**~~ ✅ **2026-11-13 完成**：新增
  `scripts/spikes/gen-test-report.py`（零第三方依赖、跨平台；生成 `report.json` + `report.md`：
  Git HEAD/dirty、OS/rustc/cargo、fmt/clippy/test/doc、`run-all-probes.sh` 明细与证据链接；
  默认写 `target/test-reports/` 不入库；失败退出 1）。CI 新增 `docs-and-probes` job
  （ubuntu 非 root：文档一致性 + UDS 探针 + artifact 上传）；`run-all-probes.sh` 的 Python 探测改为
  `python3 || python`（修复 Windows/Git Bash 下 `python3` 不存在导致探针整体 exit 1 的问题）。
  本机整轮报告 **PASS**（cargo 4 项 + 探针 any 项）；⚠️ CI 首次运行待推送后观察。
- [x] ~~**B50 MCP 覆盖矩阵**~~ ✅ **2026-11-13 完成**：`ec-sim-mcp` 每次 `tools/call` 向 stderr 写
  **JSON Lines 审计**（ts_ms/event/tool/duration_ms/ok[/error_code]；不含参数值与响应体；2 个单测，
  `mcp-server` 16→18）。`mcp-stdio-probe` 扩至 **31 项断言**：token 矩阵（缺→AUTH_REQUIRED / 错→FORBIDDEN /
  对→200）、审计字段与失败码、Linux 实时模式（`--uds`+`--serve`）live-only 工具端到端
  （topology/pdo/history/metrics/sdo_audit；先受控写 0x6040 产生审计）。本机与验证机 PASS；
  验证机整轮 **20 PASS / 0 FAIL / 0 SKIP**；`cargo test --workspace` **258 passed**。只写预回归，不宣称认证。
- [x] ~~**B51 B44 增量：插件 panic 重启 + 多实例配额**~~ ✅ **2026-11-13 完成**：
  `plugin-host::behavior` 在 worker 线程死亡（panic/Stopped）或连续失败（trap/out-of-fuel/timeout）
  达阈值时按保留模块字节重建实例（`max_restarts` 默认 3；预算耗尽 → **隔离**，ticker 退出、宿主继续用
  最后有效输出）；新增 `BehaviorQuota` 显式实例配额（`spawn_*_with_quota`，不传 = 不限制）；
  `BehaviorStats` 增 `restarts/quarantined`。feature 单测 **56**（+3：重启恢复 / 隔离 / 配额）。
  ⚠️ `esc-node` 仍每进程 1 个行为桥（配额未接线，多实例接线后续）；线程死亡(panic)与 trap 共用重建路径，
  trap 路径已测、真实 panic 路径未单独注入测试。
- [x] ~~**B52 M4 Agent Skills 只读技能文档与示例**~~ ✅ **2026-11-13 完成**：新增 `docs/skills/`
  （README 格式/索引 + `ecsim-diagnose/SKILL.md` + 静态/实时示例 `.jsonl`），把 9 个只读 MCP 工具映射为
  诊断技能（步骤/成功判据/错误码/只读与快照边界/归档摘要模板）；新增 `scripts/spikes/agent-skills-check.py`
  （校验 frontmatter、mode=read-only、工具目录一致且无写工具、示例 JSON-RPC 合法且无 token、相对链接）
  并登记进 `run-all-probes.sh`（`agent-skills`，any）。本机 `EXIT=0`、run-all 本机 **7 PASS/14 SKIP**。
  不引入写工具、不宣称认证。
- [ ] **B53 DC 仿真 Tier1（建议，backlog 顶部）**：DC 寄存器 + SYNC0/tick（帧驱动）实现与画像验证；
  **不承诺**真实同步精度/抖动（需真机/HIL），SII DC 声明需在验证后复核。
- [x] ~~**B54 CNC 虚拟闭环对接包：CiA402 布局 ↔ lcec + 多实例行为插件 + TAP 多从站探针**~~ ✅ **2026-11-13 团队班完成**：
  （任务板 `task-8`）① `esc-node` 多实例接线——新增 `BehaviorBridges`（每从站一个行为插件实例，共享
  `BehaviorQuota`；`--behavior-max-instances` 收紧上限，超限显式失败），补 2 条 feature 单测（各自写回
  自己的 SM3 / 配额满拒绝）+ 仓库画像守卫；② 新增 `plugins/cnc-csp-servo/`（Rx 7B `0x6040/0x6060/0x607A` +
  Tx 11B `0x6041/0x6061/0x6064/0x606C`，`0x6502=0x01C1`）——与 lcec `basic_cia402` 的 CSP 期望一致
  （`0x606C` 为 `enableCSP` 隐式要求），守卫测试 `cnc_csp_servo_layout_matches_lcec_csp_expectations`；
  ③ 对接说明 [CNC-V1-integration](docs/plan/CNC-V1-integration.md)（布局表/启动 runbook/排查表/二进制交付/边界）；
  ④ 新增 `scripts/spikes/tap-cia402-multi-slave-probe.sh`（TAP N 个 CiA402 从站：slaveinfo N 站 + WKC 3N +
  `mapped 7N O+11N I` + `/topology` 全 OP + `/pdo` 具名解码）并登记 `run-all-probes.sh`（`tap-cia402`，root）；
  ⑤ EC-Sim 验证机实测：`igh-cia402-op-probe.sh` 对 `generic-cia402-servo`（V1 PP）与 `cnc-csp-servo`
  （11B Tx）均 **EXIT=0**（`ethercat slaves` OP + `al_states=0x08` + `sw=0x0637 modes_display=8 pos=1000`）；
  ⚠️ `tap-cia402` 整轮因验证机被 B-1 验收占用未跑（登记为下一轮证据）。
  二进制交付：release `ec-sim-esc`（openEuler 24.03 SP4 / glibc 2.38 / rustc 1.99.1，
  sha256 `878712fe…88b8ee`）放 `.local/artifacts/`（gitignored，不入库）。不宣称一致/认证。
- [x] ~~**B55 ACC-27② P1 修复：控制面 `?slave=` 统一校验（0-based）**~~ ✅ **2026-11-13 团队班完成**：
  `control-plane::validate_slave_param`（缺省=0；`0..=N-1` 合法；非数字/负数/空串/越界 → **404 JSON**，
  **不静默回退站 0**）；`esc-node` 实时/静态两模式接线：按站路由 `/state` `/pdo` `/metrics` `/events`
  `/sdo` `/dict` `/sdo-audit` + `POST /sdo`/`POST /pdo`；全局路由（`/topology` 等）带 `?slave` 显式 404；
  `/sdo-audit?slave=N` 按站过滤（事件体保留 `slave` 字段）；新增探针 `scripts/spikes/invalid-slave-probe.sh`
  （静态/UDS/TAP × {非数字,负,空,越界,0,N-1}，写路由按站隔离）并登记 run-all（注册表 **23 项**）；
  2 条 control-plane 单测；CONTROL-PLANE §2.1 写明 0-based 语义与静态模式仅 `slave=0`。
  证据：WORKLOG 2026-11-13 团队班（B55 段）、`docs/VERIFICATION.md`。不宣称认证。
- [x] ~~**B56 B-2 验收探针聚合脚本（PV-18 / task-18）**~~ ✅ **2026-11-13 团队班完成（脚本+本机自测；VM 整轮待补）**：
  新增 `scripts/spikes/run-acceptance-probes.sh`（零第三方依赖，35 项注册表由脚本自算；`--list`；
  `name|kind|EXIT|status|秒[|skip_reason]` 输出 + `RESULT: PASS|FAIL`；SKIP 必带原因；超时/缺失=FAIL；
  日志落 `$LOG_DIR`）；与 `run-all-probes.sh` 分工：B-1 观测面 vs **B-2 行业协议验收**（可串联为 Gate B）。
  本机（非 root）整轮 **3 PASS / 0 FAIL / 32 SKIP**（SKIP 均平台原因）；本机首跑即抓到"证据二进制=旧构建"
  的真实问题（重 build 后 PASS）。证据：`docs/verification/2026-11-13-gate-b2-acceptance-probes.md`
  （含 35 项 ACC↔探针追溯表）。⚠️ VM（root/TAP/IgH）整轮与 ACCEPTANCE §3.2.2 状态更新待 VM 恢复/sim-expert 采认。
- [x] ~~**B57 `--al-error <code>`：AL Status 错误注入（task-34 / AC-E-203）**~~ ✅ **2026-11-13 团队班完成（本机验证；VM 端到端待排队）**：
  `esc-core::al_status::is_known()` + `Responder::force_al_error(code)`（置 AL Status error 位 + `0x0134` 码，
  不改状态、主站 Error ACK 可清）；CLI `--al-error <hex>`（只接受已知码，未知/0 **拒启动 exit 2**，
  启动打印 `AL_ERROR inject code=… name=…`）；默认关闭、不影响既有探针。单测 +2（esc-core 码表/esc-node
  注入+ACK 清除）；探针 `scripts/spikes/al-error-probe.sh`（Linux/UDS：`/state.live.al_status=0x11`、
  `al_status_code`、`/metrics`；本机跑参数校验段）；登记 B-2 验收聚合（36 项）。文档：CONTROL-PLANE §1
  调试开关 + README。⚠️ 交付二进制需在 EC-Sim VM 重建（新特性）后再给 cnc-expert 做 AC-E-203 端到端。
- [x] ~~**B58 GAP-14：`cnc-csp-servo` TxPDO 顺序按 lcec 读方向修正（V2 硬阻塞）**~~ ✅ **2026-11-13 团队班完成（离线单测；端到端由 cnc-expert 复跑）**：
  TxPDO 0x1A00 改为 **`0x6041 → 0x6064 → 0x606C → 0x6061`**（SM3 仍 11B；依据 lcec `c6f8820`
  `lcec_cia402_add_input_sync()` 的 `FOR_ALL_READ_PDOS_DO` 顺序，cnc-impl/cnc-expert 定案）；
  `esc-node` 新增 `Cia402TxLayout { Standard, LcecCsp }` + `with_cia402_tx_layout`，11 字节 TxPDO 编码按布局分支，
  `ec-sim-esc --behavior cia402` 从 manifest 0x1A00 顺序自动选择并打印 `tx_layout=`；Standard 保持既有行为
  （CSV 画像/探针不变）。单测：已知值→期望字节序列 `37 06 e8 03 00 00 00 00 00 00 08`；守卫测试更新。
  文档：plugin README/facts、CNC-V1-integration §1/§2.2/§6（GAP-14 排查行）。⚠️ 需新 manifest + 新二进制交付
  （二进制在 EC-Sim VM 重建后）。
- [x] ~~**B59 探针陈旧断言修正（VM 整轮暴露）**~~ ✅ **2026-11-13 团队班**：
  ① `soem-dc-probe.sh` 期望 `mapped 7O+7I` → SV660N 默认可变映射早已按公开手册改为 **8B/22B**，改断言
  `mapped 8O+22I`；② `manifest-slave-probe.sh` 依赖 SOEM 英文文案（该构建把 `r` 渲染为 `n`）与固定对象数
  （7→现 8）→ 改为按稳定内容断言（`0x1018/0x1c12/0x1c13/0x2000` + `0x1234 / 4660`）。修后两探针在验证机
  **EXIT=0**（DC: `dc_rd=5335 dc_wr=5337`）；B-2 由 27 PASS/2 FAIL/7 SKIP 变为 **29 PASS / 0 FAIL / 7 SKIP（36）**。
  ② 同轮 clean VM 整轮：**B-1 23 PASS / 0 FAIL / 0 SKIP（23）**、`invalid-slave` 全三路径 EXIT=0（ACC-27② TAP 段闭环）、
  `al-error` 实时段 EXIT=0、`tap-cia402` N=2/N=3 EXIT=0；新 release 二进制 sha256
  `7ec957e8e59019811c8ab9ba8c068a1caff18f554348df7a96bfb900f609344c`（1,060,800 B，含 GAP-14 + B57）。
- [x] ~~**B60 GAP-14 端到端 PDO 字节序探针（task-41）**~~ ✅ **2026-11-13 团队班**：
  新增 `scripts/spikes/pdo-byteorder-probe.sh`（Linux/UDS，无 root）——合成主站写 SM2
  （controlword `0x000F` / mode 8 / target 1000）→ 断言 SM3 原始字节 **`3706e80300000000000008`**
  （statusword 0x0637 | pos 1000 | vel 0 | mode 8）；同探针断言 manifest Tx 顺序
  `[0x6041,0x6064,0x606C,0x6061]` + SM3 11B，并做**旧顺序负例判别**（offset2/3..7 不同时成立 → 旧顺序必 FAIL）；
  登记 B-2 验收聚合（36 → **37**，脚本自算）；验证机整轮 **B-2 `30 PASS / 0 FAIL / 7 SKIP（37）` PASS**。
  证据：`docs/verification/2026-11-13-gate-b2-acceptance-probes.md` §4 轮次 C。
- [x] ~~**B61 H1 EC-Sim Web 实现（task-43 / GO-B DOD-02/03/06/13）**~~ ✅ **2026-11-13 团队班**：
  `web/` 实现 DOD-03 SSE 降级（6 态状态机 + STALE/PAUSED/STOPPED + E-10 banner + 数据时间/Δ + 单定时器 +
  visibilitychange）、DOD-02（静态模式说明、`/sdo-audit` 展示、confirm 信息增强、错误码「原因/影响/下一步」）、
  DOD-13 UI 半（`/sdo?slave=` 0-based、站上下文、AL 派生 chip）、DOD-06（≥44px 触控/focus 环）；
  DOD-11 断言迁移**同 commit**（原 10 控件/13 标记全覆盖保留 + H1 新增断言，差异记录在证据文档）。
  证据：`docs/verification/2026-11-13-hmi-h1-web.md`（node --check / web-sdo-panel-check / 静态同源 smoke 全 EXIT=0）。
  ⚠️ **DOD-03 T-SSE-01..06 / DOD-06 DOM 实测 / 审计/confirm 浏览器行为待 sim-ux 走查**（不写"降级 PASS"）；
  发布仍冻结；`0x18` 维持 NOT-JUDGED（BL-20 暂停/v1.x）。

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
- [ ] **B62 H2 EC-Sim Web：契约/testid + 写作边界（task-44，进行中）**：
  - ✅ **DOD-01**：`scripts/spikes/web-contract-check.py`（16 `#id` + 87 固定 `data-testid` + 9 参数化模板 +
    4 通用态槽位；唯一性/命名/`#id` 冻结）**EXIT=0**；为承载钩子补齐：topbar/上下文条/导航/toast/页脚、
    总览 health+拓扑+从站表+指标卡、从站/PDO/字典（搜索+过滤+L1 抽屉）、SDO（错误态/L2 确认对话框/审计）、
    证据导出（V-06）、关于页（V-07）；`window.confirm` 保留为无对话框宿主的回退路径（DOD-11 原断言仍成立）。
  - ✅ **DOD-10**：`scripts/spikes/writing-boundary-check.py`（14 类禁用词 + 页脚常驻边界 + 导出模板边界 +
    脱敏）**EXIT=0**。
  - ⚠️ **DOD-07 性能 / DOD-08 分辨率缩放 / DOD-09 i18n 未完成**（需浏览器窗口/实现）：不得记 PASS；
    证据与下一步见 `docs/verification/2026-11-13-hmi-h2-web.md`。

  - ✅ **DOD-07/08 参考证据（headless）**：`scripts/spikes/hmi-browser-probe.mjs`（Edge headless + CDP）+
    `docs/verification/assets/hmi-h2/`（4 视口截图/metrics）：渲染 7 section、可见交互 43、**<44px=0**、
    横向溢出=0、静态 FCP 32–112 ms；并修 nav 链接/checkbox 的 44px 缺口。**8 站实时性能与 7 视图人工确认仍 ⚠️ 未测**。

- [ ] **B63 DOD-09 i18n（task-44，实现完成待走查）**：`web/i18n.js`（zh/en 77 键、`t()`/`text()`、
  `applyStatic`/`setLang`、`?lang=en` 覆盖、localStorage、`<html lang>` 切换）+ `#lang-toggle`（44px/aria-pressed）；
  静态外壳 `data-i18n`（保留中文 fallback，DOD-11/DOD-10 不破）；动态文案（徽标/banner/说明/章节/表头/kv/按钮/
  消息/证据/关于/L2 confirm）全量迁移；`scripts/spikes/web-i18n-check.py` 契约检查。证据
  `docs/verification/2026-11-13-hmi-h2-web.md` §8。⚠️ 待 sim-ux en 模式走查确认；DOD-07/08 仍"部分"。
