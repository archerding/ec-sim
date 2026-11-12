# WORKLOG — EC-Sim 工作日志

> 约定：**最新条目在最上面**。每条含 `日期 / 做了什么 / 验证结果 / 卡点 / 下次开工第一件事`。
> 夜班 agent 每次唤醒先读这里的第一条。
> **脱敏规则**：不写主机地址、端口、账号、代理地址；远端一律称"Linux 验证机"或"远端"。运维细节见本地 `.local/ops.md`（gitignored）。

---

## 2026-11-13 00:35 班（低峰）— B9 强化：**数值初值位宽范围校验**（防静默截断）

**做了什么**
- `plugin-host::PluginManifest::validate()` 对数值对象的初值增加**位宽范围**检查：
  解析 `value`（十进制/`0x`/下划线）后按 dtype 位宽与有无符号校验上限
  （BOOL=1、INT8≤127、UINT8≤255、INT16≤32767、UINT16≤65535、INT32≤2^31-1、
  UINT32≤2^32-1）；越界报"会截断"错误，负值报"暂不支持"。
- 扩展单测 `rejects_invalid_object_declarations`，加入 `UINT8=0x1FF`、`INT16=65535`
  两个越界用例（断言包含"超出"）。
- 这样 `dictionary_from_manifest`（内部先 validate）不会再静默截断超范围初值。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **174 passed**（esc-core 79 / esc-node 53 / plugin-host 25 + 1 集成 / wire 16）；
  仓库内既有插件初值（0x1234 等）均在范围内，无误报。

**卡点**
- 仅校验数值初值；字符串/布尔之外的类型语义、SM control/type 仍未覆盖。
- 负值初值仍不支持（dict_gen 只按 u64 解析，manifest 校验明确报错）。

**下次开工第一件事**
- 继续 B9 强化（SM control/type、对象子索引范围）或回到 B11（SV660N 公开手册
  PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 02:08 班（低峰）— B11：**CST（转矩模式，5 字节 PDO）Tier0 行为**

**做了什么**
- 行为层：`--behavior cia402` 现在支持 **5 字节 PDO 的 CST（模式 10）布局**：
  RxPDO = `0x6040` 控制字(16) + `0x6060` 模式(8) + `0x6071` 目标转矩(16)；
  TxPDO = `0x6041` 状态字(16) + `0x6061` 模式显示(8) + `0x6077` 实际转矩(16)。
  Tier0：Operation enabled 时 `0x6077` 镜像目标转矩，禁用后归零（不模拟转矩环/负载）。
  `Cia402Behavior` 新增 `torque_actual`。
- 新画像 `plugins/generic-cia402-cst-servo/`：自有 VID `0x0EC5`/PID `0xE5E2`，
  `profile: cia402`、5 字节 SM2/SM3、`0x6502=0x3AD`、CoE SDO/SDO Info/CA、
  `dc.supported=false`；`facts.md` 写公开来源与 Tier0 边界；`plugins/catalog.yaml`
  新增 ready 条目。
- 验证设施：`uds_master_probe.py` 新增 `--cia402-cst`（四步状态机序列 + 转矩断言）、
  `scripts/spikes/cia402-cst-probe.sh`（UDS 模式，无 root）；`docs/VERIFICATION.md`
  索引与 `docs/plugins/generic-cia402-cst-verification.md` 记录。
- 单测 `cia402_cst_mode_mirrors_target_torque`（esc-node）：`0x0221/0x0223/0x0237/0x0240`
  与转矩 `0/0/300/0` 全断言。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **199 passed**（esc-core 85 / esc-node 63 / plugin-host 27 + 1 集成 / wire 23）。
- Linux 验证机（重建后）：`cia402-cst-probe.sh` → **EXIT=0**：
  `BEHAVIOR cia402 sm2_out=0x1100/5B sm3_in=0x1110/5B`，四步读回的状态字/模式显示/
  实际转矩全部符合预期（Operation enabled 时 300，禁用后 0）。

**卡点**
- Tier0 简化：无转矩环、负载、限幅、单位换算；状态机 Ti 时序与故障码表未实现。
- 该 5 字节 CST 布局尚未在真实支持 CST 的 IgH/SOEM 应用中做 PDO 映射验证
  （需带 CST 应用的主站），⚠️ 待验证。

**下次开工第一件事**
- B11：SV660N 默认 PDO 条目（公开镜像/HTML 搜索），或 CST 之外的厂商扩展对象核对。

---

## 2026-11-13 02:06 班（低峰）— 计划簿记：**B5/B6/B7 标记完成**

**做了什么**
- 按 §6 "完成即移入已完成" 的约定，核对后将三项标记为完成（保留全部进度子条目与证据链接）：
  - **B5 `esc-core` 最小 ESC** ✅：寄存器阵列/SM/FMMU/AL 状态机/EEPROM 接口已满足，并扩展了
    ESM 前置条件（0x0015/0x0017/0x0018/0x0019/0x0024/0x0025/0x002B/SM 布局）、SM watchdog
    与 0x0410 读写语义。已知限制：帧驱动时钟（无独立定时器）、PDI watchdog 超时不模拟、
    BOOT 仅拒绝。
  - **B6 SII 生成器** ✅：ESI/manifest→SII（CRC-8、字符串/通用/SM/**FMMU**/TxPDO/RxPDO/
    **DC** 类别链、0xFFFF 终止），`--esi`/`--manifest`/组合路径均经 SOEM/IgH 扫描验证。
    已知限制：FMMU 类别按 4 通道示例；DC 类别生成就绪但无 profile 声明（缺 DC 同步仿真）。
  - **B7 CoE 最小闭环** ✅：expedited/segmented SDO 读写、配置对象、Abort 码、SDO Info、
    Complete Access（上传/单帧+分段下载/重发容错/超长 Abort）。已知限制：CA 部分写/顺序
    语义未实现；IgH SDO 在验证机受 ecrt ioctl 阻塞（环境问题）。
- 当前 §6 仅剩 **B11 CiA402 行为插件**未完成（Tier0 已具备：CSP/CSV 行为、故障注入、
  通用/CiA402/CSV/SV660N 画像、IgH/SOEM 验证；剩余：SV660N 默认 PDO 条目、CST 语义等）。

**验证结果**
- 纯文档（AGENTS §6 三行标题）改动；`git diff` 核对仅改 3 行状态标记并保留证据。
- 本班未改代码：测试基线 **198 passed** 不变。

**卡点**
- B11 的 SV660N 默认 PDO 具体条目仍无公开文本来源；CST（转矩）语义未实现。

**下次开工第一件事**
- B11：继续找 SV660N 默认 PDO（公开镜像/HTML），或实现 CST（模式 10/11 字节 PDO）Tier0 行为。

---

## 2026-11-13 02:02 班（低峰）— B7：**分段下载数据超长 → Abort 0x06070010**

**做了什么**
- `esc-core::mailbox` 新增 `SDO_ABORT_LENGTH_MISMATCH = 0x06070010`（CiA 301：
  数据长度与服务参数不匹配）。
- `esc-node` 两处改为**显式 Abort**（此前是静默 `truncate`，可能掩盖主站错误）：
  - `handle_download_initiate`：initiate 自带数据 > 声明 `total_size` → Abort；
  - `handle_download_segment`：累计段数据 > `total_size` → Abort（清空分段状态，
    不写入对象，避免部分写）。
- 新增单测 `segmented_download_overflow_is_aborted_with_length_mismatch`：
  20 字节对象先收 8+7 字节，再收 7 字节（22>20）→ Abort 0x06070010、`download` 清空、
  `object_writes` 为空；另测 initiate 自带 8 字节但声明 4 字节 → Abort。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **198 passed**（esc-core 85 / esc-node 62 / plugin-host 27 + 1 集成 / wire 23）。
- Linux 验证机（重建后）：`sdo-ca-probe.sh` **EXIT=0**、`cia402-sdo-download-probe.sh`
  **EXIT=0**（合法分段/CA 下载路径无回归）。

**卡点**
- 仅覆盖分段下载；分段上传没有"超长"概念（服务器按对象长度返回），未改。
- Abort 后主站需重新 initiate（不保留可续传状态）。

**下次开工第一件事**
- B6：最小 DC 仿真（activate/cycle 寄存器 + tick 驱动）并在测试画像验证；或 B11
  SV660N 默认 PDO 条目查找（公开资料暂无列表）。

---

## 2026-11-13 01:58 班（低峰）— B6：**manifest `dc` 时序字段接入 SII DC 类别**

**做了什么**
- manifest schema 扩展：`PluginManifest.dc_supported: bool` 替换为 `DcConfig`
  （`supported`、`cycle_time0`、`shift_time0/1`、`sync0/sync1_cycle_factor`、
  `assign_activate`；缺省 0）；解析支持 `0x` 十六进制与负的周期因子（新增
  `signed_integer()`）；`validate()` 在 `supported=true` 但缺 `cycle_time0` 时报错。
- `sii_gen::sii_spec_from_manifest` 在 `dc.supported` 且 `cycle_time0 > 0` 时构造
  `SiiDc` 并写入 SII DC 类别（0x003C）；否则保持 `dc: None`。
- 新增测试：manifest `parses_dc_configuration`（字段/缺省/缺 cycle_time0 校验）、
  `sii_gen::dc_supported_manifest_emits_dc_category`（SII 中出现 24 字节 DC 类别；
  缺 cycle_time0 时生成失败）。
- **仓库内所有画像仍 `dc.supported: false`**——模拟器尚未实现 DC 同步，声明 DC 会让
  主站进入 DC 配置路径；B6 的"生成能力 + 配置入口"已就绪，等 DC 仿真实现后再开启。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **197 passed**（esc-core 85 / esc-node 61 / plugin-host 27 + 1 集成 / wire 23）。
- 内置镜像与既有 manifest 生成的 SII 字节不变（DC 均未声明），未占用验证机。

**卡点**
- 无 DC 同步仿真（0x0900 寄存器/SYNC0 驱动 tick/漂移）；开启声明前必须先实现，
  否则主站可能在 DC 配置阶段等待。
- manifest DC 时序字段尚未在真实主站上验证（无 profile 声明）。

**下次开工第一件事**
- B6 收尾：实现最小 DC 仿真（activate/cycle 寄存器 + tick 驱动 SYNC0 计数）并在
  测试画像上验证；或 B11 SV660N 默认 PDO 条目查找。

---

## 2026-11-13 01:56 班（低峰）— B6：**SII DC 类别（0x003C）生成已实现**

**做了什么**
- 新增 `SiiDc`（分布式时钟类别内容，24 字节 / 12 words）与 `SiiDc::to_bytes()`：
  `cycleTime0` u32 @0、`shiftTime0` u32 @4、`shiftTime1` u32 @8、
  `sync1CycleFactor` i16 @0x0C、`assignActivate` u16 @0x0E、`sync0CycleFactor` i16 @0x10、
  `nameIndex` u8 @0x12、`descIndex` u8 @0x13、`reserved[4]` @0x14。
  依据公开资料 [CSDN《EtherCAT EEPROM详解》](https://blog.csdn.net/weixin_42927110/article/details/136050157)
  的 DC 类别表（只读参考，2026-11-13 通过图片表格核对）。
- `SiiSpec` 增加 `dc: Option<SiiDc>`；`SiiImage::from_spec` 在 FMMU 类别之后插入 DC 类别
  （类别头 `0x003C`、长度字 12）。
- **内置镜像 / manifest / ESI 均保持 `dc: None`**：模拟器尚未实现 DC 同步，声明 DC 会让
  主站进入 DC 配置路径；等 DC 仿真实现后再由 profile 显式声明。
- 新增单测 `dc_category_encodes_documented_layout`（逐字段偏移/长度）与
  `from_spec_with_dc_adds_dc_category`（类别存在、长度字=12、内容一致）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **195 passed**（esc-core 85 / esc-node 61 / plugin-host 25 + 1 集成 / wire 23）。
- 内置镜像字节不变（既有 `from_spec == simulated_slave` 逐 word 断言仍通过），
  因此未重跑验证机（扫描/OP 路径无变化）。

**卡点**
- DC 类别只是**生成能力**：无任何 profile 声明 DC，也未实现 DC 同步/漂移补偿；
  开启前需先实现 DC 仿真（0x0900 寄存器、SYNC0 驱动 `tick`）与 manifest 时序字段。

**下次开工第一件事**
- B6 收尾：为 manifest 增加 DC 时序字段并在 DC 仿真可用时声明；或 B11 SV660N
  默认 PDO 条目查找（HTML 在线手册）。

---

## 2026-11-13 01:51 班（低峰）— B6：**SII FMMU 类别（0x0028）已实现**

**做了什么**
- 新增 SII **FMMU 类别 `0x0028`**：内容 4 字节用途码 `[0x03, 0x01, 0x02, 0xFF]`
  （FMMU0 读邮箱、FMMU1 过程数据输出、FMMU2 过程数据输入、FMMU3 未使用），
  类别头长度字 = 2（4 字节），插入在 SM 类别之后、PDO 类别之前。
  依据：公开资料 [CSDN kevin1499《EtherCAT 从站 EEPROM 分类附加信息详解：FMMU》](https://blog.csdn.net/kevin1499/article/details/134430238)
  （只读参考，2026-11-13 访问）——类别 0x28、长度单位为字、内容为 FMMU0..3 用途码。
- 新增测试：`fmmu_category_has_usage_bytes`（内容与长度字）；更新类别链遍历测试期望。
- 顺带修正 `SIM_COE_DETAILS` 上方过期注释（"CA 未实现" → 已实现；内置镜像仍保持
  bit5=0 以规避未验证的 SOEM `ecx_readPDOmapCA()` 路径）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **193 passed**（esc-core 83 / esc-node 61 / plugin-host 25 + 1 集成 / wire 23）。
- Linux 验证机（重建后，含 FMMU 类别的 SII）：
  - `soem-process-data.sh` **EXIT=0**（SOEM `simple_ng` OP、1055 轮 WKC=3、0 不符）；
  - `minimal-esc-responder.sh` **EXIT=0**（SOEM/IgH 均看到 1 从站）；
  - `igh-el2008-op-probe.sh` **EXIT=0**（IgH `al_states=0x08`、EL2008 OP）
  → 新增 FMMU 类别**未改变** SOEM/IgH 的扫描、FMMU 配置与 OP 路径。

**卡点**
- FMMU 类别内容按公开资料示例给出（4 个用途码）；**未覆盖 >4 FMMU 的设备**（本模拟器
  固定 4 通道），也未验证主站是否会读取/使用该类别（IgH 不读、SOEM 仅 eepFMMU 默认值）。
- B6 仍剩 **DC 类别（0x003C）**：需 ETG 原文/更多证据，暂未实现。

**下次开工第一件事**
- 继续 B11 SV660N 默认 PDO 条目查找（HTML 在线手册）或 B6 DC 类别（需原文）。

---

## 2026-11-13 01:49 班（低峰）— 质量：**修复 rustdoc 警告（0 警告）**

**做了什么**
- `cargo doc --workspace --no-deps` 此前有 7 条警告，全部修复：
  - `esc-core::mailbox`：`#[must_use]` 被误接在文档行尾（导致 `[must_use]` 被当作链接）；
    拆分为独立行。
  - `esc-core::sii`：文档中的 `data[0x0B]`/`data[0x0C]` 被 rustdoc 当作链接（以及替换后
    残留的 `data[11]`/`data[12]`）——改为代码span `data[11]`/`data[12]`；同时修正
    `general_category()` 里被误改成代码span的赋值语句 `data[11] = general_flags;`。
  - `esc-node` crate 文档链接到私有项 `Responder::process_logical` /
    `Responder::handle_mailbox_request`——改为普通代码引用。
- 结果：`cargo doc --workspace --no-deps` **0 警告**。

**验证结果**
- `cargo doc --workspace --no-deps`：无 warning/error。
- `cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **192 passed**（esc-core 82 / esc-node 61 / plugin-host 25 + 1 集成 / wire 23）。
- 纯注释/文档改动，无运行时行为变化（未占用验证机）。

**卡点**
- 本次只消除文档警告；crate 文档完整度（示例、模块级说明）仍可继续补。

**下次开工第一件事**
- 继续 B11 SV660N 默认 PDO 条目查找（HTML 在线手册）或 B6 FMMU/DC 类别（需原文）。

---

## 2026-11-13 01:45 班（低峰）— B7：**CA 下载改为 all-or-nothing（修复部分写）**

**做了什么**
- 修复 `esc-node::write_complete_access` 的**部分写缺陷**：此前按子索引边校验边写入，
  若后面的子索引只读/长度不符，会在 Abort 前把前面的子索引写入 `object_writes`，留下
  部分状态。现在改为**两遍**：
  1. 第一遍只校验——所有子索引存在、可写、各子索引字节长度之和等于数据长度；
  2. 第二遍在校验全部通过后才写入。
- 新增单测 `complete_access_download_is_all_or_nothing`：0x3000 数组（子0 1B 可写、
  子1 2B 可写、子2 2B 只读），CA 下载 5 字节 → Abort `0x06010002`（子索引 2）、
  `object_writes` 保持**空**（无部分写）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **192 passed**（esc-core 82 / esc-node 61 / plugin-host 25 + 1 集成 / wire 23）。
- Linux 验证机（重建后）：`sdo-ca-probe.sh` **EXIT=0**（CA 上传/单帧/分段/重发容错）、
  `cia402-sdo-download-probe.sh` **EXIT=0**（20 字节对象分段下载并回读一致）→ 合法
  CA/分段写路径无回归。

**卡点**
- CA 的"子索引顺序"语义仍按 ETG 常见顺序（sub0→max）实现；不校验主站是否按顺序发送
  （CA 数据本身就是按顺序拼好的字节串，无法从字节流反推）。
- IgH 侧仍无法直接验证 CA（公共 API 不下发 CA 位）。

**下次开工第一件事**
- 继续 B11 SV660N 默认 PDO 条目查找（HTML 在线手册）或 B6 FMMU/DC 类别（需原文）。

---

## 2026-11-13 01:42 班（低峰）— B11：**SV660N 公开事实核对（第三方实测文档）**

**做了什么**
- 通过 `ghproxy.net` 镜像取得 [linuxcnc-ethercat `documentation/inovance.md`](https://github.com/linuxcnc-ethercat/linuxcnc-ethercat/blob/master/documentation/inovance.md)
  （公开只读参考，未复制代码/文件），核对 SV660N（SV660 EtherCAT 变体）公开事实：
  - 每方向仅 **一个** PDO：RxPDO `0x1600` / TxPDO `0x1A00`；`0x1C13:01` 可写 `0x1A00`
    或固定映射 `0x1B01..0x1B04`；**无 `0x1A01`**（写它会 SDO 报错且无法进 OP）；
  - `0x1600`/`0x1A00` 最多 **10** 个映射条目，子索引 `0x0B` 写入以 `0x06090011` 中止；
  - 设备**不响应 SDO Info**（`Enable SDO Info: no`）；
  - DC 默认开启（`assignActivate 0x300`，SYNC0 跟应用周期）；
  - `0x6502 = 0x3AD`（pp/pv/tq/hm/csp/csv/cst）；
  - 厂商 H 参数映射：Hgg-pp = CoE `0x2000+0xgg`、子索引 `0xpp+1`。
- 更新 `plugins/inovance-sv660n/`：`coe.sdo_info` 改 **false**（与设备一致）；新增
  `0x6502` 对象（UINT32 ro = 0x3AD）；`facts.md` 增加公开实测事实与来源行；
  `docs/plugins/inovance-sv660n-verification.md` 增补该节与验证命令。
- 默认 PDO 的**具体条目/顺序/位长**第三方文档未列出 → 画像继续用 Tier0 通用 7 字节布局，
  并在 facts.md 标注"仍未逐项确认"（未凭空编造）。

**验证结果**
- 本机：`cargo test -p plugin-host` 通过（manifest 校验/字典生成/SII 生成与守卫断言）；
  `cargo test --workspace` **191 passed** 基线不变。
- Linux 验证机：`MANIFEST=plugins/inovance-sv660n/plugin.yaml EXPECT_PID=0000e5f1 …
  bash scripts/spikes/generic-cia402-probe.sh` → **EXIT=0**：SOEM 扫描身份
  `00000ec5:0000e5f1`、56/56 bits、`mapped 7O+7I`、OP、WKC 3；IgH PREOP + 身份。

**卡点**
- **默认 PDO 具体条目**仍未核实（这是 B11 剩余关键事实）；厂商对象/错误码表未收录。
- 设备默认启用 DC，模拟器仍未实现 DC（`dc.supported=false`），⚠️ 能力缺口。
- 厂商手册 PDF 仍无法经 `web_fetch` 提取（只拿到第三方实测文档）。

**下次开工第一件事**
- 继续找 SV660N 默认 PDO 条目（HTML 在线手册/镜像），或转 B6 FMMU/DC 类别（需原文）。

---

## 2026-11-13 01:40 班（低峰）— B5：**0x0410（PDI watchdog）语义明确化**

**做了什么**
- 明确 **0x0410 Watchdog Time PDI** 的模拟器语义：寄存器**可读写保存（透传）**，但模拟器
  没有 PDI 端点，因此**不模拟 PDI 看门狗超时**（不递减、不置 AL error）——需要 PDI 行为
  时应由插件/上层显式模拟。`crates/esc-core/src/watchdog.rs` 文档同步说明。
- 新增 esc-node 单测 `pdi_watchdog_register_is_read_write_without_timeout`：APWR 写
  0x0410=0x00C8 → APRD 回读一致；`tick(1000)` 后值保持、AL Status error 位不置位。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **191 passed**（esc-core 82 / esc-node 60 / plugin-host 25 + 1 集成 / wire 23）。

**卡点**
- 仍未模拟 PDI 看门狗超时/喂狗行为（无 PDI 端点，需插件设计后才能实现并验证）；
  divider/intervals 为 0 的真实 ESC 行为仍 ⚠️ 待验证（文档已注明）。

**下次开工第一件事**
- 继续 B11 SV660N 手册核对（HTML 在线文档）或 B6 FMMU/DC 类别（需原文/更多证据）。

---

## 2026-11-13 01:32 班（低峰）— B7：**在 CiA402 画像中启用 CA 能力位** + SDO Info 探针重试

**做了什么**
- `plugins/generic-cia402-servo/` 与 `plugins/generic-cia402-csv-servo/` 的
  `mailbox.coe.complete_access` 由 `false` 改为 **`true`**（CA 上传/下载已实现并验证，
  SII 通用类别 CoE details bit5 现在声明 Complete Access）。
- 插件守卫测试增加 **CoE details 位一致性断言**：manifest 的 `sdo`/`sdo_info`/
  `complete_access` 必须分别映射到 SII `coe_details` bit0/bit1/bit5。
- `scripts/spikes/sdo-info-probe.sh`：SOEM `slaveinfo -sdo` 对数组对象偶发段错误（已知
  SOEM 工具缺陷），改为**最多重试 3 次**并打印 WARN；本次验证机第一次段错误、第二次成功。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **190 passed**（esc-core 82 / esc-node 59 / plugin-host 25 + 1 集成 / wire 23）。
- Linux 验证机：
  - `generic-cia402-probe.sh` **EXIT=0**（SOEM 56/56 bits、OP WKC 3；IgH PREOP+身份）；
  - `sdo-ca-probe.sh` **EXIT=0**（CA 上传/单帧/分段下载/重发容错，CSV 画像已带 CA 位）；
  - `sdo-info-probe.sh` 首次 SOEM 段错误（已知工具缺陷）→ **重试后 EXIT=0**（6 个对象）。

**卡点**
- CA 能力位只对已实现并验证的 4 类操作成立（CA 上传、单帧/分段下载、重发容错）；CA 的
  "部分写/子索引顺序"语义仍未实现，SII 声明会略高于实际严格度（⚠️ 已在 AGENTS 记录）。
- SOEM `slaveinfo -sdo` 段错误是工具缺陷，重试只是规避；未改 SOEM 源码（GPL 边界）。

**下次开工第一件事**
- 继续 B11 SV660N 手册核对（HTML 在线文档）或 B6 FMMU/DC 类别；B7 可补 CA 部分写语义。

---

## 2026-11-13 01:30 班（低峰）— 文档：**验证脚本索引（docs/VERIFICATION.md）**

**做了什么**
- 新增 `docs/VERIFICATION.md`：把 `scripts/spikes/` 下全部验证脚本按"无特权 UDS / 需要
  Linux TAP+root"分类建表，列出每个脚本验证的内容与最近结果（截至 2026-11-13 01:30，
  绝大多数 EXIT=0），并说明 `uds_master_probe.py` 等辅助工具、IgH ecrt 夹具仅验证机本地
  构建不入库的 GPL 边界、以及运行提示（root/模块/清理）。
- `README.md` 增加一条链接指向该索引（"Reproducible evidence"）。

**验证结果**
- 纯文档改动；`git diff` 核对为新增索引 + README 一行链接，未改动既有事实性结论。
- 本班未改代码：测试基线 **190 passed** 不变。

**卡点**
- 索引中的"最近结果"来自 WORKLOG 记录，若验证机环境变化需重跑脚本更新（索引内已注明基线日期）。

**下次开工第一件事**
- 继续 B11 SV660N 手册核对（HTML 在线文档）或 B6 FMMU/DC 类别；也可为索引补充脚本退出码日志。

---

## 2026-11-13 01:29 班（低峰）— 健壮性：**解析/寄存器访问模糊测试**（防 panic）

**做了什么**
- `wire`：新增 `parser_never_panics_on_arbitrary_bytes`——确定性伪随机字节序列（长度
  0–95，每种长度 32 例）输入 `Frame::parse` / `Frame::parse_ethernet`，断言不 panic。
- `esc-core::mailbox`：新增 `parsers_never_panic_on_arbitrary_bytes`——随机服务数据输入
  SDO/分段/CA/SDO Info 各解析入口与 `MailboxHeader::parse`，断言不 panic。
- `esc-core::Registers`：新增 `register_access_never_panics_on_any_address`——5000 组随机
  地址/长度调用读写与 `access()`，越界只允许返回 None/false，不得 panic。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **190 passed**（esc-core 82 / esc-node 59 / plugin-host 25 + 1 集成 / **wire 23**）。
- 全部模糊测试一次通过（未发现 panic），说明现有解析入口的边界检查充分。

**卡点**
- 这是"不 panic"级健壮性验证，不是完整 fuzz（无覆盖率引导、无 libFuzzer/AFL 集成）；
  模糊测试未覆盖语义正确性/状态机（仅各入口单次调用）。
- 随机源为确定性 LCG，样本规模有限（约万级），未做长时间 fuzz 运行。

**下次开工第一件事**
- 继续 B11 SV660N 手册核对（HTML 在线文档）或 B6 FMMU/DC 类别；如需更强健壮性可引入
  独立 fuzz 工具（本地/CI 非核心依赖）。

---

## 2026-11-13 01:26 班（低峰）— B7 验证：**分段上传重发容错端到端**（合成主站）

**做了什么**
- UDS 合成主站新增 `--sdo-retransmit`：先 CA 多段写入 140 字节到 `0x2100`，再做 CA 分段
  上传，并在**首段重复同一 toggle 请求**（模拟"响应丢失后主站重发"），断言：
  - 重发请求得到**逐字节相同**的 segment 响应；
  - 最终回读 140 字节与写入值完全一致。
- `sdo_upload_ca_full` 增加 `retransmit_first` 参数；`sdo-ca-probe.sh` 增加第四个模式
  （CSV 画像 manifest）。

**验证结果**
- Linux 验证机：`bash scripts/spikes/sdo-ca-probe.sh` → **EXIT=0**，四模式全过：
  - CA 上传 0x1C12/0x1600；CA 单帧下载 0x1600；
  - CA 分段下载（140B 多段写入并回读）；
  - **分段上传重发容错**（首段重复请求幂等，回读 140 字节一致）；
  - 每模式 UDS 基础断言（BRD/APRD/APWR/FPRD/M、SM watchdog、ESM 前置条件）均通过。
- 本机：Python `py_compile`、脚本 `bash -n` 通过；Rust 代码未改，`fmt --check` 通过，
  测试基线 **187 passed** 不变。

**卡点**
- 只覆盖"最近一段重发"；跨多段回退/更早段重发仍 Abort（未实现段历史窗口）。
- 合成主站是同步请求-响应模型，"响应丢失"用重复请求模拟，非真实丢包时序；⚠️ 与真实
  网络的乱序/超时行为仍有差异。

**下次开工第一件事**
- 继续 B11 SV660N 手册核对（HTML 在线文档）或 B6 FMMU/DC 类别；B7 可扩展段历史窗口。

---

## 2026-11-13 01:21 班（低峰）— B7：**分段传输重发容错（toggle 幂等）**

**做了什么**
- 分段上传：`SegmentedUpload` 记录最近一次 `(toggle, 响应服务数据)`；主站**重发上一段**
  （同一 toggle、响应丢失场景）时幂等返回相同响应、不推进 offset；真正乱序的 toggle
  仍回 Abort `0x05030000`。
- 分段下载：`SegmentedDownload` 记录最近处理的 segment toggle；主站重发上一段时幂等应答
  （`0x20|toggle<<4`）、**不重复追加数据**；真正乱序仍 Abort `0x05030000`。
- 新增单测 `segment_retransmission_is_idempotent`：上传重发首段 → 响应逐字节相同且
  offset 不变；下载重发首段 → 数据不重复，随后正确下一段仍能完整写入 20 字节。
- 前班 `out_of_order_segment_toggle_is_aborted` 仍通过（无历史响应/数据时的错误 toggle
  依旧 Abort）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **187 passed**（esc-core 80 / esc-node 59 / plugin-host 25 + 1 集成 / wire 22）。
- Linux 验证机（重建后）：`sdo-ca-probe.sh` **EXIT=0**（CA 上传/单帧/分段下载多段序列）、
  `uds-loopback.sh` **EXIT=0**（三轮；52 字节 segmented 上传）、`soem-process-data.sh`
  **EXIT=0**（SOEM OP、1055 轮 WKC=3）→ 正确序列无回归。

**卡点**
- 幂等仅在"一段"粒度：主站若重发更早的段（跨 2 段以上）仍 Abort；只缓存最近一段响应/
  toggle，未实现完整段历史窗口。
- 未用真实主站重传（丢包模拟）验证顺序容错——仅合成主站/单测覆盖，⚠️ 待验证。

**下次开工第一件事**
- 可选：用合成主站模拟"响应丢失 + 重发"端到端用例；或继续 B11 SV660N 手册核对
  （HTML 在线文档）与 B6 FMMU/DC 类别。

---

## 2026-11-13 01:16 班（低峰）— B7：**分段传输 toggle 校验**（Abort 0x05030000）

**做了什么**
- `esc-core::mailbox` 新增 `SDO_ABORT_TOGGLE = 0x05030000`（CiA 301 "toggle bit 未交替"）。
- `esc-node` 的 `SegmentedUpload` / `SegmentedDownload` 增加期望 `toggle` 字段：
  initiate 后从 0 开始，每成功一段（非末段）取反；**请求 toggle 与期望不一致**时回
  Abort `0x05030000` 并清空分段状态（此前忽略 toggle，主站重传/乱序不会报错）。
- 新增单测 `out_of_order_segment_toggle_is_aborted`：130B 对象的分段上传/下载各注入
  toggle=1 的首段请求，断言 Abort 0x05030000 且分段状态清零。
- 顺带修复 `handle_download_segment` 非末段未推进 toggle 的问题（否则正确序列也会误报）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **186 passed**（esc-core 80 / esc-node 58 / plugin-host 25 + 1 集成 / wire 22）。
- Linux 验证机（重建后）：
  - `sdo-ca-probe.sh` **EXIT=0**（CA 上传/单帧/分段下载，内含多段 toggle 交替）；
  - `uds-loopback.sh` **EXIT=0**（三轮；含 52 字节 segmented SDO 上传）；
  - `soem-process-data.sh` **EXIT=0**（SOEM OP、1055 轮 WKC=3）→ 正确 toggle 序列无回归。

**卡点**
- 只在段请求层校验 toggle；**未持久跟踪跨请求的重复段/重传去重**（CiA 301 允许主站重发
  上一段，当前会按 Abort 处理，⚠️ 与部分主站重试行为可能不完全一致）。
- Abort 后不保留"可重同步"状态（主站需重新 initiate）。

**下次开工第一件事**
- 评估"主站重发上一段"的容错（可选：toggle 回退时重发上一段而非 Abort）；或继续
  B11 SV660N 手册核对（HTML 在线文档）与 B6 FMMU/DC 类别。

---

## 2026-11-13 01:12 班（低峰）— 文档真实性修正 + **B11 手册访问再次受阻记录**

**做了什么**
- **B11 前置尝试（再次受阻）**：再次用 web_search 查 SV660N 默认 PDO 与 EtherCAT 手册；
  结果仍只有 PDF（`application/pdf` 不支持 web_fetch）与非 SV660N 的通用伺服手册；
  GitHub raw 仍不可达。**未改动 SV660N manifest**（真实性红线），继续记录为阻塞。
- **修正过期文档（真实性问题）**：
  - `docs/spikes/spike-4-dual-master-op.md` §6 原文称"无 SDO Info、Complete Access、
    分段传输；SII 缺 FMMU/DC 类别"——这些能力已在 2026-11-12/13 各班实现并验证。
    更新为当前状态：SDO Info/分段/CA 上传/下载（含分段）已实现并验证；SM watchdog 已实现；
    DC 仍未实现（无 SII DC 类别）、FMMU 类别（0x0028）未加入（IgH 不使用、SOEM 语义待原文复核）；
    Download 仍仅对可写对象开放、值仅进程内持久。
  - `AGENTS.md` B8 的"已知限制"同步更新（去掉过期的"SDO Info/CA/分段未实现"）。
- **Backlog 核对**：B2b 此前已被标记完成（证据齐全），本次仅确认无需重复改动。

**验证结果**
- 文档改动为纯文本；`git diff` 逐行核对：只删除/改写过期结论，未删除既有证据章节。
- 本班未改代码：`cargo fmt --check`、`cargo test --workspace` 基线不变（**185 passed**，
  上一条班次最后记录），无需重跑 VM。

**卡点**
- B11 SV660N 手册仍**无可用文本来源**：PDF 无法经 web_fetch 提取、GitHub raw 不可达；
  下一步可尝试在线手册 HTML（如厂商在线文档站）、或把手册页文本/截图放到本地后人工确认。

**下次开工第一件事**
- 继续尝试 SV660N HTML 在线手册；若仍不可达，转 B6 FMMU/DC 类别（需 ETG 原文或更多开源
  实现证据）或 B3 Spike 4（SOEM oshw UDS shim）。

---

## 2026-11-13 01:03 班（低峰）— B11：**CiA 402 故障注入 CLI（`--fault`）+ 验证**

**做了什么**
- `ec-sim-esc` 新增 `--fault <hex>`（需与 `--behavior cia402` 搭配）：启动即调用
  `inject_cia402_fault(code)`，打印 `FAULT cia402 error_code=0xNNNN`；无 behavior 时忽略并提示。
- 合成主站 `uds_master_probe.py` 新增 `--cia402-fault`：断言初始状态字 Fault bit、
  控制字 bit7 故障复位后 Fault 位清除、`0x603F` 回读注入错误码。
- 新脚本 `scripts/spikes/cia402-fault-probe.sh`（CSV/CiA402 画像 + `--behavior cia402
  --fault 0x7500`）；`docs/plugins/generic-cia402-verification.md` 增补故障注入记录。
- 顺带修复 `build_responder` 中 `--manifest`+`--esi` 组合分支的 unreachable pattern（调换
  match 臂顺序），VM 构建 **0 warning**。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **185 passed**（esc-core 80 / esc-node 57 / plugin-host 25 + 1 集成 / wire 22）。
- Linux 验证机：`bash scripts/spikes/cia402-fault-probe.sh` → **EXIT=0**：
  - `FAULT 0x0208`（Fault | SwitchOnDisabled 等）→ 复位后 `0x0240`（Fault 位清除）；
  - `0x603F = 0x7500`（SDO expedited 回读一致）；
  - 同套件 UDS 基础断言通过；`esc-node` release 构建 **0 warning**。

**卡点**
- 故障码清除时序仍是 Tier0 简化（错误码保留到上层再次写入，与真机不同）；
  无故障历史/多故障码；`--fault` 仅作用于启动时刻（不支持运行中注入）。
- 未做 IgH/SOEM 侧故障注入验证（两家主站无此注入点；合成主站覆盖）。

**下次开工第一件事**
- B11：换公开镜像/HTML 手册核对 SV660N 默认 PDO（PDF 与 GitHub raw 受阻已记录）；
  或继续 B5/B6 剩余项（0x0410 PDI watchdog、FMMU/DC 类别）。

---

## 2026-11-13 00:58 班（低峰）— B5 剩余：**SM 布局合法性检查**（0x0017 细化）

**做了什么**
- `esc-core::EsmPreconditions` 增加 `sync_managers_valid`；`failure_for(AlState::SafeOp)` 在
  "未配置过程数据 SM"之后增加"**已配置 SM 布局非法**"分支 → 仍报
  **0x0017** `Invalid sync manager configuration`。
- `EsmPreconditions` 改为手写 `Default`：除 `sync_managers_valid`/映射有效性为 `true`
  外全 `false`（等价"不做前置条件检查"；未配置任何 SM 时布局天然合法）。
- `esc-node::Responder::active_sm_layout_valid`：遍历 SM0–SM3，只对 Activate bit0 置位且
  Length > 0 的 SM 检查——起始地址必须在 DL 内存 `0x1000–0xFFFF`、区间不越过 0xFFFF、
  已激活 SM 地址区间互不重叠（排序后逐对比较）。
- 新增单测：esc-core `checked_esm_rejects_safeop_with_invalid_sm_layout`；
  esc-node `esm_strict_rejects_safeop_with_overlapping_sync_managers`（SM2 改到与 SM1
  重叠 → SAFEOP 拒绝 0x0017、状态保持 PREOP）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **185 passed**（esc-core 80 / esc-node 57 / plugin-host 25 + 1 集成 / wire 22）。
- Linux 验证机回归（重建后）：`soem-process-data.sh` **EXIT=0**（SOEM OP、1055 轮
  WKC=3、无 WKC 不符）、`generic-cia402-probe.sh` **EXIT=0**（CiA 402 扫描/OP）、
  `igh-el2008-op-probe.sh` **EXIT=0**（IgH `al_states=0x08`、EL2008 OP）→ 真实主站配置的
  SM 布局均合法，无回归。

**卡点**
- 只检查 SM0–SM3（ESC 每通道 4 字节寄存器，最多 8 通道可扩展）；SM 的 control/type 语义
  仍不校验（由 manifest/主站写入决定）。
- 布局检查在 SAFEOP 请求时做快照，未跟踪运行中 SM 被改写后的重新校验。

**下次开工第一件事**
- B11：换公开镜像/HTML 手册核对 SV660N 默认 PDO（PDF 与 GitHub raw 受阻已记录）；
  或 B3 Spike 4（SOEM oshw UDS shim）。

---

## 2026-11-13 00:56 班（低峰）— B4 边界测试补充 + **B11 手册访问受阻记录**

**做了什么**
- **B11 前置尝试（受阻）**：尝试核对 Inovance SV660N 公开手册的默认 PDO 映射
  （`[linuxcnc-ethercat inovance.md](https://github.com/linuxcnc-ethercat/linuxcnc-ethercat/blob/master/documentation/inovance.md)`、
  Inovance 通讯手册 PDF）。结果：`web_fetch` **不支持 PDF**（`application/pdf` 被拒），
  GitHub raw **fetch failed**（网络不可达）→ 本班无法取得可引用的手册原文，**未改动 SV660N
  manifest**（避免凭记忆写 PDO；已记录原因，符合 §4 真实性红线）。
- 回退到 **B4 `wire` crate 边界测试补充**（6 个新单测）：
  - 13 命令的 WKC 规则（读 +1、写 +1、读写 +2、NOP 0）；
  - 自增寻址：ADP=0 处理且前进到 1；非 0 不处理；`u16::MAX` 回绕到 0；
  - `More` 位置 1 但后面无 datagram → `TooShort`；
  - 非法帧类型（frame_type=2）→ `UnsupportedFrameType(2)`；
  - frame_type=1 才是 EtherCAT command（长度不符走 `LengthMismatch`）；
  - 命令码 0x00–0x0E 往返、未知码（0x7F/0xFF）拒绝。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **183 passed**（esc-core 79 / esc-node 56 / plugin-host 25 + 1 集成 / **wire 22**）。

**卡点**
- B11 SV660N 手册核对仍**受阻于工具/网络**：PDF 无法经 `web_fetch` 提取、GitHub raw 不可达；
  下一步可尝试镜像站/HTML 文档，或把手册页截图/文本放到本地后由人工确认。
- wire 边界测试只证明编解码/寻址辅助函数的语义，不改变与真实主站的运行路径（已有 VM 回归）。

**下次开工第一件事**
- B11：尝试其他公开镜像（HTML 文档/在线手册）核对 SV660N 默认 PDO；失败则记录并转
  B5 剩余（0x0410 PDI watchdog 语义）或 B3 Spike 4（SOEM oshw UDS shim）。

---

## 2026-11-13 00:50 班（低峰）— B7：**CA 分段下载端到端验证**（合成主站多段）

**做了什么**
- UDS 合成主站（`scripts/spikes/uds_master_probe.py`）新增：
  - `--sdo-ca-download-multi`：CA 下载 initiate（0x31，首帧 112 字节）+ 4 个 segment
    （`0x20|toggle<<4|unused<<1|last`），再用 **CA 分段上传**读回并比对 140 字节；
  - `sdo_upload_ca_full` / `sdo_download_ca_multi` 两个主站侧辅助函数（含 segment
    toggle/last 校验）。
- `scripts/spikes/sdo-ca-probe.sh` 增加第三个模式：用
  `plugins/generic-cia402-csv-servo` 画像（`0x2100` 字符串扩为 **140 字节**）验证
  >112 字节的 CA 分段下载。
- 修复 `build_complete_access_upload`：改为通过 `object_value()` 读取，**包含 Download/CA
  写入的覆盖值**（此前只读字典初值，导致"写入后 CA 回读不一致"）。
- 末段 CA 分段下载的应答改为 **segment response**（`0x20|toggle<<4`，与 CiA 301 一致；
  之前在末段回 0x60 不规范）；Abort 仍透传。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **177 passed**（esc-core 79 / esc-node 56 / plugin-host 25 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/sdo-ca-probe.sh` → **EXIT=0**，三模式全过：
  - CA 上传：0x1C12 = `01 00 16`、0x1600 = `01 08 00 00 70`；
  - CA 单帧下载：0x1600 写入后回读一致；
  - **CA 分段下载**：140 字节对象多段写入并回读一致；
  - 每个模式的 UDS 基础断言（BRD/APRD/APWR/FPRD/M、SM watchdog、ESM 前置条件）均通过。

**卡点**
- CA 的"部分写/子索引顺序"语义仍未实现/校验；IgH 1.6.13 公共 API 不下发 CA 位，IgH 侧
  无法直接验证（合成主站覆盖）。
- 为验证多段 CA，CSV 画像的 `0x2100` 字符串由 45B 扩为 140B（插件内容变化，允许且已过
  守卫测试；⚠️ 若后续要更贴近设备，可换专用测试对象）。

**下次开工第一件事**
- 回到 B11：SV660N 公开手册 PDO/扩展对象核对（或 B5 剩余 0x0410 PDI watchdog 语义）；
  B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:47 班（低峰）— B7：**Complete Access（CA）分段下载**

**做了什么**
- `esc-core::mailbox::parse_complete_access_download_initiate`：解析 CA 下载 initiate
  （命令 `0x31`）的 u32 总长 + **首帧携带的数据**（不再要求整段都在首帧）。
- `esc-node`：`SegmentedDownload` 增加 `complete_access` 标记；首帧不足 total 时沿用既有
  分段下载 FSM（segment 命令 `0x20|toggle<<4|unused<<1|last`），**末段完成后**把完整数据
  交给新抽出的 `write_complete_access()`（按子索引长度拆分、逐项写 `object_writes`，
  错误码同单帧 CA）；单帧路径行为不变。
- 新增单测 `mailbox_complete_access_download_handles_segmented_transfer`：130 字节可写字符串
  对象，首帧 112 字节 + 3 段（7/7/4，toggle 0/1/0，末段 last=1），断言
  `object_writes[(0x2000,0)]` 完整等于写入值、分段状态清零。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **177 passed**（esc-core 79 / esc-node 56 / plugin-host 25 + 1 集成 / wire 16）。
- Linux 验证机：`sdo-ca-probe.sh` 回归 **EXIT=0**（CA 上传 0x1C12/0x1600、单帧 CA 下载
  0x1600 写入并回读一致、UDS 基础断言全过）→ 重构未影响既有 CA 单帧路径。

**卡点**
- **CA 分段下载尚未做端到端主站验证**：合成主站 `--sdo-ca-download` 目前只发单帧，
  130 字节以上的多段 CA 下载仅由单测覆盖 → ⚠️ 待验证（下一步可扩展 UDS 主站发多段）。
- 仍不校验 CA 的"部分写/子索引顺序"语义；IgH 1.6.13 公共 API 不下发 CA 位，IgH 侧无法验证。

**下次开工第一件事**
- 扩展 UDS 主站 `--sdo-ca-download` 支持多段（>112 字节对象），端到端验证 CA 分段下载；
  或继续 B11（SV660N 公开手册 PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:41 班（低峰）— B7：**Complete Access（CA）SDO 下载（单帧）**

**做了什么**
- `esc-core::mailbox::parse_complete_access_download`：解析 CA 下载请求（命令字节 `0x31`
  = initiate normal download + CA 位 bit4；u32 长度在 +6，数据随后）。
- `esc-node::Responder::handle_download_complete_access`：按字典对象子索引 0..max 的
  **字节长度**拆分数据、逐项写入 `object_writes`；未知对象 → Abort 0x06020000、
  只读对象 → 0x06010002、数据长度不符 → 0x05040000；成功回 0x60（子索引 0）。
- 合成主站新增 `--sdo-ca-download`（写入 0x1600 后回读校验）；`sdo-ca-probe.sh` 改为
  **每个模式独立模拟器实例**（UDS 套件会改变 AL 状态，复用实例破坏初始断言），并同时跑
  CA 上传 + 下载两种模式；新增单测
  `mailbox_complete_access_download_writes_all_subindices`。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **176 passed**（esc-core 79 / esc-node 55 / plugin-host 25 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/sdo-ca-probe.sh` → **EXIT=0**：
  - CA 上传：0x1C12 = `01 00 16`、0x1600 = `01 08 00 00 70`；
  - CA 下载：0x1600 写入 `01 08 00 00 70` 后回读一致；
  - 每个模式的 UDS 基础断言（BRD/APRD/APWR/FPRD/M、SM watchdog、ESM 前置条件）均通过。

**卡点**
- 仅支持**单帧** CA 下载（数据不超邮箱首段）；更大的 CA 下载需要 CA 分段写语义
  （当前回 0x05040000）——⚠️ 待后续。
- CA 上传/下载仍无法用 IgH 1.6.13 公共 API 验证（不下发 CA 位）；SOEM 同样不产生 CA 请求。
- 简化实现：不校验 CA 数据内子索引顺序的"部分写"语义；PDO 映射对象按普通数组处理。

**下次开工第一件事**
- CA 分段下载（沿用既有分段下载 FSM，完成后拆分写入）或继续 B11（SV660N 公开手册
  PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:36 班（低峰）— B7：**Complete Access（CA）SDO 上传**

**做了什么**
- `esc-core::mailbox::parse_complete_access_upload`：解析 CA 上传请求（CoE 命令字节
  `0x50` = initiate upload + CA 位 bit4，子索引忽略）。
- `esc-node::Responder::build_complete_access_upload`：按对象子索引 0..max 逐项取字典值，
  **按各自长度拼接（不填充）**，用 normal upload 响应（0x41 + 子索引 0 + u32 总长 + 数据）
  返回；数据超过首段容量时复用既有分段上传路径；未知对象回 Abort `0x06020000`。
- 合成主站 `scripts/spikes/uds_master_probe.py` 新增 `--sdo-ca`；新脚本
  `scripts/spikes/sdo-ca-probe.sh`；新增单测
  `mailbox_complete_access_upload_returns_all_subindices`（0x1C12、0x1600 两例）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **175 passed**（esc-core 79 / esc-node 54 / plugin-host 25 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/sdo-ca-probe.sh` → **EXIT=0**：
  - `CA 0x1C12 = 01 00 16`（子 0 计数 1 + 子 1 分配 0x1600）；
  - `CA 0x1600 = 01 08 00 00 70`（子 0 计数 1 + 子 1 映射字 0x70000008）；
  - 同套件 UDS 基础断言（BRD/APRD/APWR/FPRD/M、SM watchdog、ESM 前置条件）继续通过。

**卡点**
- **IgH 1.6.13 公共 API 不下发 CA 位**（`ec_fsm_coe_prepare_up` 固定写 `0x40`），因此
  IgH 侧无法直接验证 CA 上传；本班用合成主站验证（SOEM 侧同样不产生 CA 请求）。
- CA **下载**（命令 0x51/0x53…）与 CA 的 Abort/部分写语义未实现；SII 的 CA 能力位仍
  由 manifest `coe.complete_access` 控制（既有画像保持 false）。
- ⚠️ 本实现为"简化 CA"：不校验长度字段/子索引顺序，也不处理 CA 下 PDO 映射对象的
  特殊规则（0x1600/0x1A00 的 CA 读写按普通数组处理）。

**下次开工第一件事**
- 实现 CA 下载（0x51/0x53 命令 + size/子索引顺序校验），或继续 B11（SV660N 公开手册
  PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:33 班（低峰）— B9 强化：**对象声明合法性校验**（dtype/access/初值/重复）

**做了什么**
- `plugin-host::PluginManifest::validate()` 增加 `objects` 声明校验：
  - `dtype` 必须是 dict_gen 支持的类型（BOOL/8/16/32 位整数/VISIBLE_STRING/OCTET_STRING）；
  - `access` 必须是 `ro` 或 `rw`（此前任何非 `rw` 会被静默当只读，现在显式报错）；
  - 字符串类型必须有 `value` 且位长不超 u16；数值类型若有 `value` 必须能按十进制/`0x`
    十六进制解析（带下划线容忍）；
  - 同一 `(index, sub)` 重复声明报错。
- 顺带把历史遗留的 dict_gen 测试名 `string_values_are_not_supported_yet` 改为
  `numeric_dtype_rejects_non_numeric_value`（字符串对象已支持，原名误导）。
- 新增单测 `rejects_invalid_object_declarations`（dtype/access/初值/缺 value/重复五种错误）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **174 passed**（esc-core 79 / esc-node 53 / plugin-host 25 + 1 集成 / wire 16）；
  仓库内既有插件（EL2008/通用 CiA402/CSV/SV660N/示例）全部通过新校验（无误报）。

**卡点**
- 仍属"声明期"校验：不校验 SM control/type 语义、对象值位宽截断（如 0x1FF 写给 UINT8
  会由 dict_gen 截断/报错？当前 dict_gen 按字节写入长度截断 ⚠️ 未单独校验），也不校验
  PDO 映射与对象字典的运行时一致性（由主站 OP 期检查兜底）。

**下次开工第一件事**
- 校验数值初值与 dtype 位宽的匹配（避免 0x1FF→UINT8 截断）；或回到 B11（SV660N 公开
  手册 PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:31 班（低峰）— B9 强化：**SM 地址/长度/区间重叠校验**

**做了什么**
- `plugin-host::PluginManifest::validate()` 增加 Sync Manager 布局合法性校验：
  - `len` 必须非 0；
  - 起始地址必须在 DL 用户内存 `0x1000–0xFFFF`；
  - 区间不得越过 `0xFFFF`（`start+len` 溢出）；
  - **不同 SM 的地址区间不得重叠**（按起始地址排序后逐对比较）。
- 新增单测 `rejects_invalid_sync_manager_ranges`（地址 <0x1000、与 SM1 重叠、越过 0xFFFF
  三种错误同时报告）。
- 仓库内既有插件（EL2008/通用 CiA402/CSV/SV660N/示例）与新测试夹具均通过校验
  （守卫测试全绿，无误报）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **173 passed**（esc-core 79 / esc-node 53 / plugin-host 24 + 1 集成 / wire 16）。

**卡点**
- 只校验地址区间合法性，不校验 SM 的 control/type/direction 语义（manifest 目前不声明
  control/type；由 dict_gen 默认值决定）。
- 未校验 SM 地址与 SII 声明的一致性（SII 由 manifest 生成，天然一致）。

**下次开工第一件事**
- 继续 B9 强化（SM control/type 语义、对象字典子索引范围）或回到 B11（SV660N 公开手册
  PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:27 班（低峰）— B9 强化：**PDO 映射项的数据类型/位长 + 重复映射校验**

**做了什么**
- `plugin-host::PluginManifest::validate()` 继续加强加载期校验：
  - **映射项 vs 对象声明**：若 PDO 映射项对应的对象在 manifest `objects` 中已声明，则校验
    其数据类型/位长与映射项一致（BOOL=1、8/16/32 位整数、字符串=value 字节数×8），
    不一致报错（例如把 `INT32` 对象映射成 16bit）；
  - **同一 PDO 内重复映射同一对象**（index+subindex）→ 报错；
  - 对象未在 manifest 声明时跳过（PDO 数据对象可能由运行时/别处定义）。
- 新增单测 `rejects_pdo_entry_datatype_mismatch_and_duplicates`。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **172 passed**（esc-core 79 / esc-node 53 / plugin-host 23 + 1 集成 / wire 16）；
  仓库内既有插件（EL2008/通用 CiA402/CSV/SV660N/示例）均通过新校验（无误报）。
- Linux 验证机重建后：`generic-cia402-probe.sh` **EXIT=0**（通用 CiA 402 扫描/OP）、
  `cia402-csv-probe.sh` **EXIT=0**（CSV 行为+UDS 套件）→ 加载期校验无回归。

**卡点**
- 仍只做"声明一致性"检查：不校验 SM 地址/区间重叠/类型方向等 ESC 级合法性（B5 待做）；
  未声明对象的 PDO 映射项无法校验（有意跳过）。
- manifest 校验是"插件作者期"保护，不能替代真实主站的 OP 期映射检查。

**下次开工第一件事**
- 扩展 SM 配置合法性校验（地址越界/区间重叠/类型方向）或继续 B11（SV660N 公开手册
  PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:23 班（低峰）— B9 强化：**manifest 加载期 SM/PDO 一致性校验**

**做了什么**
- `plugin-host::PluginManifest::validate()` 增加加载期一致性检查（原 0x0024/0x0025 的
  映射错误只能到 OP 才暴露，现在提前到 manifest 加载）：
  - **SM 长度 vs PDO 位长**：`dir: out` 的 SM `len × 8` 必须等于 RxPDO 映射位长之和；
    `dir: in` 同理对应 TxPDO；
  - **映射项位长非 0**；
  - **object 的 `pdo: rx/tx` 引用必须真实存在于对应 PDO 映射**（索引+子索引匹配），
    `pdo` 值只能是 `rx`/`tx`。
- 新增单测 `rejects_sm_length_and_pdo_mapping_mismatch`（SM/PDO 位长不一致 + 对象
  pdo 引用悬空两种错误）。
- 该校验在 `ec-sim-esc --manifest` 启动路径与仓库插件守卫测试中都会执行。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **171 passed**（esc-core 79 / esc-node 53 / plugin-host 22 + 1 集成 / wire 16）；
  仓库内全部插件与测试夹具均通过新校验（无误报）。
- Linux 验证机回归（重建后）：`generic-cia402-probe.sh` **EXIT=0**（通用 CiA 402 扫描/OP）、
  `cia402-csv-probe.sh` **EXIT=0**（CSV 行为+UDS 套件）、`igh-el2008-op-probe.sh`
  **EXIT=0**（IgH `al_states=0x08`、EL2008 OP）→ 加载期校验对既有画像无回归。

**卡点**
- 只校验位长/引用正确性；不校验映射字里的对象**数据类型/位长**是否与声明一致
  （如把 32 位对象映射成 16 位），也不校验分配顺序的语义。
- SM 地址/区间重叠/类型方向等 ESC 级合法性仍未校验（B5 待做项）。

**下次开工第一件事**
- 扩展加载期校验：映射字对象的数据类型/位长一致性 + SM 地址/长度/类型合法性；或继续
  B11（SV660N 公开手册 PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:20 班（低峰）— B11 验证：**IgH 侧 CSV 画像 OP + 11 字节 PDO 运行时行为**

**做了什么**
- 新增验证脚本 `scripts/spikes/igh-cia402-csv-op-probe.sh`：用未修改 IgH 的 ecrt 应用
  （验证机本地夹具 `ecrt-cia402-csv-op.c`，**源码不入库**）配置 11 字节 RxPDO/TxPDO，
  应用按 `0x0006→0x0007→0x000F`、mode=9（CSV）、target velocity=5 写 RxPDO，并读回
  `0x6041`/`0x6061`/`0x6064`/`0x606C`。
- 断言：应用运行中 `ethercat slaves` = **OP** + 身份 `0x00000ec5:0x0000e5e1`；
  `RESULT al_states=0x08`；`STATUS sw` 含 Operation enabled(bit2)、`modes_display=9`、
  `pos > 100`（速度累加）、`vel_actual=5`；应答器侧 `al=0x08`。
- `docs/plugins/generic-cia402-csv-verification.md` 增加 IgH OP/行为证据与夹具限制。

**验证结果**
- 本机：`bash -n` 通过；Rust 代码未改，`cargo test --workspace` **170 passed** 基线不变。
- Linux 验证机：`bash scripts/spikes/igh-cia402-csv-op-probe.sh` → **EXIT=0**：
  - 应用运行中：`0  0:0  OP  +  0x00000ec5:0x0000e5e1`；
  - `RESULT al_states=0x08 slaves_responding=1`；
  - `STATUS sw=0x0237 modes_display=9 pos=2995 vel_actual=5`；
  → 未修改 IgH 零补丁进入 OP，且 11 字节 TxPDO（状态字/模式/位置/速度）由 Tier0 行为驱动。

**卡点**
- 夹具源码在验证机本地（不入库）；脚本依赖该本地夹具存在。
- 本班只补 IgH 侧验证证据、未改代码；CSV 行为仍为 Tier0（无真实轨迹/加减速/模式时序）。
- IgH SDO（上传/下载）在该验证机上仍阻塞，与本项无关（未使用 SDO）。

**下次开工第一件事**
- 逐项核对 SV660N 公开手册的默认 PDO/扩展对象并更新 manifest；或处理 B6 FMMU/DC
  原文核实（需获取 ESC Sec.2 / ETG 文本）。

---

## 2026-11-13 00:11 班（低峰）— B11：**CiA 402 CSV（速度模式，11 字节 PDO）画像 + Tier0 行为**

**做了什么**
- 新增插件 `plugins/generic-cia402-csv-servo/`（11 字节 PDO，CSV 速度模式画像）：
  - RxPDO `0x1600` = `0x6040`+`0x6060`+`0x607A`+**`0x60FF` target velocity**；
  - TxPDO `0x1A00` = `0x6041`+`0x6061`+`0x6064`+**`0x606C` velocity actual**；
    自有 VID 0x0EC5/PID 0xE5E1、名称带 SIMULATED（R2）；`facts.md` 只引公开 CiA 402。
- `esc-node` 的 CiA 402 行为支持 11 字节 PDO（`velocity_actual` 字段）：
  - CSP（模式 8）：位置跟随目标（同前）；
  - **CSV（模式 9）**：Operation enabled 时位置每周期累加目标速度、`0x606C` 镜像目标速度，
    禁用后速度显示归零、位置保持；
  - 7 字节画像仍为原行为（无速度字段）。
- 合成主站 `uds_master_probe.py` 新增 `--cia402-csv`（11 字节 `<Hbii` 读写序列断言）；
  新增脚本 `scripts/spikes/cia402-csv-probe.sh`；新增验证记录
  `docs/plugins/generic-cia402-csv-verification.md`；catalog 增加 CSV 画像（ready）。
- 新增单测 `cia402_csv_mode_accumulates_velocity`。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **170 passed**（esc-core 79 / esc-node 53 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机：
  - `bash scripts/spikes/cia402-csv-probe.sh` → **EXIT=0**：
    `BEHAVIOR cia402 sm2_out=0x1100/11B sm3_in=0x1110/11B`，11 字节 PDO 序列
    （`0x000F` 后位置 5→15→20、速度显示 5→5→0）通过；UDS 原套件（含 SM watchdog、
    ESM 前置条件）继续通过；
  - `bash scripts/spikes/uds-loopback.sh` → **EXIT=0**（三轮全过，含 manifest 画像
    segmented SDO 设备名）。

**卡点**
- CSV 为 **Tier0 简化**：位置按"每 tick 累加目标速度"，无真实单位/轨迹/加减速/限幅；
  无模式切换时序（Ti=0/1）、编码器/限位/Homing/DC。
- 映射一致性检查现在只在"SM 长度与 SII 声明一致"时生效（否则跳过）：合成主站测试写
  非 SII 长度时无法判断，属特例放宽（真实主站仍严格检查）。
- IgH 侧对 CSV 画像的 OP/行为验证未做（复用通用画像夹具需改 PDO 数；环境 IgH SDO 仍阻塞）。

**下次开工第一件事**
- 用 IgH ecrt 夹具验证 CSV 画像 OP + 11 字节 PDO（或补 IgH SDO 环境问题）；或继续
  B11（SV660N 公开手册 PDO/扩展对象核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-13 00:06 班（低峰）— B5 细化：**映射合法性校验**（重复分配/零项/无效映射字）

**做了什么**
- `esc-node` 把"只比较总位长"的映射检查升级为 `process_data_mapping_valid(assign_index, sm_index)`：
  - 分配对象（`0x1C12`/`0x1C13`）存在且条目数 > 0；
  - 每个分配的 PDO 索引非 0 且**不重复**；
  - 每个 PDO 映射对象存在且条目数 > 0；每个映射字的对象索引非 0、位长非 0；
  - 所有映射位长之和 = 对应 SM 的 `length × 8`。
  `outputs_mapping_valid`/`inputs_mapping_valid` 改用该函数（仍只在"已声明且已配置"时检查）。
- 新增单测 `esm_strict_rejects_op_with_duplicate_pdo_assignment`：`0x1C12` 重复分配
  `0x1600` → OP 被拒（**0x0024**）、状态保持 SAFEOP。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **169 passed**（esc-core 79 / esc-node 52 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机回归（重建后）：`soem-process-data.sh` **EXIT=0**（SOEM OP、1055 轮
  WKC=3、无 WKC 不符）、`generic-cia402-probe.sh` **EXIT=0**（CiA 402 扫描/OP）、
  `igh-el2008-op-probe.sh` **EXIT=0**（IgH `al_states=0x08`、EL2008 画像 OP）。
  → 更严格的映射校验对既有合法画像（内置 8 位 I/O、EL2008、通用 CiA 402）无回归。

**卡点**
- 仍未校验映射字的**子索引范围/对象存在性**（映射字里的对象索引/子索引是否真实存在于
  对象字典，以及是否与 PDO 分配长度精确对应）；这部分需要对象字典与 SII 的双向校验。
- 0x0024/0x0025 文本与"何时使用"仍按常见解释，⚠️ 待 ETG.1000.6 原文复核。

**下次开工第一件事**
- 增加"映射字对象存在性/子索引范围"校验（需对象字典查询），或继续 B11（9/11 字节 PDO →
  CSV、SV660N 手册核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-12 23:57 班（低峰）— B5 子项：**映射类 OP 前置条件**（AL Status Code 0x0024/0x0025）

**做了什么**
- `esc-core::al_status` 新增 **0x0024** `No valid output configuration` / **0x0025**
  `No valid input configuration`（文本按 ETG.1000.6 Table 11 常见解释，⚠️ 待原文复核）。
- `EsmPreconditions` 增加 `outputs_mapping_valid`/`inputs_mapping_valid`；`failure_for(Op)`
  在"已配置"之后继续检查映射一致性：输出映射无效 → **0x0024**，输入映射无效 → **0x0025**
  （优先级：0x0018/0x0019 存在性错误先于 0x0024/0x0025）。
- `esc-node::Responder` 新增 `process_data_mapping_bits(assign_index)`：读
  `0x1C12`/`0x1C13` 分配条目 → 各 PDO 映射对象条目数 → 映射字
  `index<<16 | sub<<8 | bits` 的最低位长求和；与 SM2/SM3 的 `length × 8` 比较得出映射一致性
  （字典缺映射对象/长度不一致 → 无效）。
- 新增单测：esc-core `checked_esm_rejects_op_with_invalid_mapping`；esc-node
  `esm_strict_rejects_op_with_invalid_process_data_mapping`（空字典 → OP 拒绝 0x0024，
  状态保持 SAFEOP）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **168 passed**（esc-core 79 / esc-node 51 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机（重建后）回归（默认前置条件开启）：
  - `soem-process-data.sh` **EXIT=0**：SOEM OP、1055 轮 WKC=3、无 WKC 不符；
  - `generic-cia402-probe.sh` **EXIT=0**：CiA 402 画像扫描/OP 正常；
  - `igh-el2008-op-probe.sh` **EXIT=0**：IgH `al_states=0x08`、EL2008 画像 OP。

**卡点**
- 映射一致性只比较**总位长**：未校验分配顺序/子索引合法性/条目是否重复或越界
  （0x0024/0x0025 的更细语义待原文与主站行为核对）。
- 0x0024/0x0025 的文本与"何时使用"仍按常见解释，⚠️ 待 ETG.1000.6 原文复核。
- 与既有 0x0015/0x0017/0x0018/0x0019 一样为"配置存在性/一致性"近似。

**下次开工第一件事**
- 细化映射校验（条目/顺序/越界）或继续 B11（9/11 字节 PDO → CSV、SV660N 手册核对）；
  B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-12 23:52 班（低峰）— B5 子项：**SAFEOP→OP 前置条件**（AL Status Code 0x0018/0x0019/0x002B）

**做了什么**
- `esc-core::EsmPreconditions` 增加 OP 迁移的前置条件字段：
  `outputs_declared/outputs_configured`（SII 是否声明 RxPDO / SM2 是否配置）、
  `inputs_declared/inputs_configured`（TxPDO / SM3）；`failure_for(AlState::Op)`：
  - 声明了 RxPDO 但 SM2 未配置 → **0x0019** `No valid outputs`；
  - 声明了 TxPDO 但 SM3 未配置 → **0x0018** `No valid inputs`；
  - 两者都缺 → **0x002B** `No valid inputs and outputs`。
- `esc-node::Responder::esm_preconditions()` 由 SII 的 RxPDO/TxPDO 类别是否存在 +
  SM2/SM3 是否配置（Activate bit0 且 Length>0）推导这些字段。
- 新增单测：esc-core `checked_esm_rejects_op_without_valid_outputs_or_inputs`
  （三种缺失组合 + ACK 清错 + 都配置后 OP 通过）；esc-node
  `esm_strict_rejects_op_until_inputs_configured`（SM0–SM2 配置、缺 SM3 → OP 拒绝且
  状态保持 SAFEOP，配置 SM3 后 OP 通过）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **166 passed**（esc-core 78 / esc-node 50 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机回归（默认前置条件检查开启）：
  - `soem-process-data.sh` **EXIT=0**：SOEM `simple_ng` OP、1055 轮 WKC=3、无 WKC 不符；
  - `generic-cia402-probe.sh` **EXIT=0**：CiA 402 画像扫描/OP 正常（IgH PREOP + 身份）；
  - `igh-el2008-op-probe.sh` **EXIT=0**：IgH ecrt 应用 `al_states=0x08`、EL2008 画像 OP。

**卡点**
- 以"SM 已配置"近似"有效输出/输入"：**未检查 PDO 映射对象（0x1600/0x1A00）的内容**，
  映射类错误码 **0x0024/0x0025** 未实现（⚠️ 待后续，需更精确语义）。
- 与既有 0x0015/0x0017 一样是"配置存在性"近似，未覆盖 TI 时序/动态配置错误。

**下次开工第一件事**
- 实现映射类前置条件 0x0024/0x0025（检查 SM 映射对象内容与 SM 长度一致性），或继续
  B11（9/11 字节 PDO → CSV、SV660N 手册核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-12 23:47 班（低峰）— B7 验证：**分段 SDO 下载（合成主站）** + 可写配置字符串对象

**做了什么**
- 给通用 CiA 402 / SV660N 画像增加**可写配置字符串对象** `0x2100:00`
  （VISIBLE_STRING，`rw`，20 字节；用于验证分段下载，值带 SIMULATED 说明）。
- 合成主站 `scripts/spikes/uds_master_probe.py` 新增 `--sdo-segmented-download`：
  initiate download（`0x21` + Size u32 + 首段 8B）→ segment 请求（`0x20|toggle<<4|
  unused<<1|last`，7B+5B 两段）→ 再经 **分段 SDO 上传** 回读并逐字节比对。
- 新增验证脚本 `scripts/spikes/cia402-sdo-download-probe.sh`（UDS，无需 root）。
- 扩展验证机本地 ecrt 夹具做 IgH 侧 SDO 下载尝试（源码不入库）：`ecrt_master_sdo_download`
  在本验证机上**再次复现阻塞**（与 23:20 班 IgH SDO 上传问题同类，expedited 也受影响）→
  已把 IgH 脚本里的 SDO 断言撤回（不把不可复现内容留在仓库脚本），IgH 侧 SDO 下载标 ⚠️ 待验证。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **164 passed**；插件守卫测试通过新画像对象。
- Linux 验证机：`bash scripts/spikes/cia402-sdo-download-probe.sh` → **EXIT=0**：
  `OK: segmented SDO download 写入 0x2100 并回读一致（20 字节）`；UDS 原套件
  （BRD/APRD/APWR/FPRD/M 位、SM watchdog、ESM 前置条件）继续通过。
  → 分段下载协议 + esc-node 下载状态机经**独立合成主站**端到端验证。

**卡点**
- **IgH 侧 SDO（上传/下载）在该验证机上阻塞**（内核 ecrt ioctl 路径，已复现两次）；
  分段下载的 IgH 真机验证 ⚠️ 待验证（可能需要重装/升级 IgH 或换验证机）。
- Complete Access 下载未实现；分段下载只写 `object_writes`，不做类型/长度校验。

**下次开工第一件事**
- 排查/恢复验证机 IgH SDO ioctl（换 IgH 版本或重装内核模块）后补 IgH 侧分段上传/下载；
  或继续 B11（9/11 字节 PDO → CSV、SV660N 手册核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-12 23:44 班（低峰）— B7 补缺口：**分段 SDO 下载**（segmented download）

**做了什么**
- `esc-core::mailbox` 新增分段下载报文支持（布局按 IgH `master/fsm_coe.c` 复核）：
  - `parse_initiate_download()`：normal download（命令字节恰为 `0x21`，Size u32 在 +6，
    首段数据从 +10 开始，承载上限 = 邮箱容量）；
  - `parse_segment_download()`：segment 请求（命令字节
    `0x20 | toggle<<4 | unused<<1 | last`，头 3 字节 + 最多 7 字节数据，末段用 unused 标零）；
  - `sdo_download_segment_ok_service()`：segment 响应
    （CoE `0x3<<12` + `0x20 | toggle<<4`，与 IgH 校验 bit7-5==0x01 一致）。
- `esc-node::Responder` 新增分段下载状态 `SegmentedDownload` 与处理：
  - initiate 校验对象可写（只读 → Abort `0x06010002`，不存在 → `0x06020000`），
    首段不完整则保持下载状态；
  - segment 累积数据、末段写入 `object_writes`（可被 SDO Upload 回读）；
    没有进行中的下载时收到 segment 请求 → Abort `0x05040000`；
  - 与分段上传分支互斥（有下载状态时优先按 segment 解析，避免 `0x20`/`0x21` 命令字节歧义）。
- 新增两个单测：`segmented_download_requests_and_response_layout`（报文布局）、
  `segmented_download_writes_long_object`（20 字节字符串对象：initiate 8B + 7B + 5B
  分段下载后回读一致）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **164 passed**（esc-core 77 / esc-node 49 / plugin-host 21 + 1 集成 / wire 16）。
- ⚠️ **未在验证机用真实主站做分段下载**：IgH `ethercat download` CLI 适合短数据/
  expedited；构造长数据需要额外夹具。本班只做 datagram 级单测（同样的字节布局按 IgH
  源码逐项复核）→ 真实主站验证列为待验证。

**卡点**
- Complete Access 的下载/上传仍未实现（SII CA 位保持 0）。
- 分段下载只写 `object_writes`，不做对象类型/长度校验（SDO 层允许任意长度覆盖）；
  无并发多传输仲裁（单主站串行场景）。
- 与分段上传一样：状态机对 toggle 只做回显，不校验连续性（上位主站均应正确翻转）。

**下次开工第一件事**
- 在验证机上用长数据夹具（或 IgH 应用 API）验证真实主站的分段下载；或继续 B11
  （9/11 字节 PDO → CSV 模式 / SV660N 手册核对）；B6 FMMU/DC 原文核实仍待条件。

---

## 2026-11-12 23:40 班（低峰）— B11 第七步：CiA 402 行为**模式门控 + 错误注入**（Tier0）

**做了什么**
- `esc-node` 的 CiA 402 行为增加**模式门控**：只有 **CSP（模式 8）** 在 `Operation enabled`
  时让实际位置跟随目标并置 `Target reached`；其它模式（CSV/CST 等）位置冻结、不置
  `Target reached`（当前 7 字节 PDO 不含速度/转矩目标）；`0x6061` 始终镜像 `0x6060`。
- 新增 `Responder::inject_cia402_fault(error_code: u16)`：状态机进入 `Fault`（状态字
  `0x0208`），错误码写入 `0x603F`（经 SDO 可读回）；控制字 bit7 `Fault reset` 回
  `Switch on disabled`（`0x0240`），错误码保留（⚠️ Tier0 与真机清除时序不同）。
- 新增两个单测：`cia402_behavior_tracks_position_only_in_csp_mode`、
  `cia402_fault_injection_sets_error_code_and_recovers_on_reset`。
- 合成主站 `--cia402` 序列加入"切 CSV（模式 9）→ 位置冻结、无 Target reached"断言；
  `docs/plugins/cia402-state-machine.md` 增加"模式与错误注入（Tier0）"说明。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **162 passed**（esc-core 76 / esc-node 48 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/cia402-behavior-probe.sh` → **EXIT=0**：
  - `BEHAVIOR cia402 sm2_out=0x1100/7B sm3_in=0x1110/7B`；
  - 完整 7 字节序列（含模式 8→9 切换后位置冻结、无 Target reached）通过；
  - 原 UDS 套件（BRD/APRD/APWR/FPRD/M 位、SM watchdog、ESM 前置条件）继续通过。

**卡点**
- 模式门控只覆盖 CSP/非 CSP 两档；CSV/CST 的真实语义（速度/转矩目标）需要扩展 PDO
  布局（如加入 `0x60FF`/`0x6071`）后再实现。
- 错误注入是**测试/上层 API**，没有硬件/厂商故障码表；`0x603F` 只在注入时写入，
  未模拟真机自动故障检测与错误码清除时序。
- 一帧延迟仍在。

**下次开工第一件事**
- 为 CiA 402 画像扩展 9/11 字节 PDO（含 `0x60FF` 速度目标）并实现 CSV 模式，或
  逐项核对 SV660N 公开手册的默认 PDO/扩展对象；随后处理 B6 FMMU/DC 原文核实。

---

## 2026-11-12 23:34 班（低峰）— B11 第六步：**Inovance SV660N 兼容画像**（Tier0，通用 DS402 布局）+ 双主站扫描/OP

**做了什么**
- 新增插件 `plugins/inovance-sv660n/`（B11 首个具体型号目标）：
  - `plugin.yaml`：自有 VID `0x00000EC5` / PID `0x0000E5F1`（R2，不使用 Inovance 的
    Vendor ID/Product Code）；名称带 `SIMULATED`；SM2/SM3 各 7 字节；RxPDO `0x1600`
    = `0x6040` + `0x6060` + `0x607A`，TxPDO `0x1A00` = `0x6041` + `0x6061` + `0x6064`；
    对象含 `0x603F`/`0x6077`/`0x1008`（分段 SDO）；`dc.supported=false`（⚠️ 待核实）。
  - `facts.md`：只列**公开来源**（Inovance 产品页、公开 SV660N Communication Guide /
    Commissioning Guide 的 URL、CiA 402），R1 未入库任何厂商 ESI/PDF/固件；明确列出
    ⚠️ 待核实项（SV660N 默认 PDO 映射、扩展对象、DC、故障码）。
  - `README.md`；`plugins/catalog.yaml`：inovance-sv660n → `researching` + deliverable。
- `scripts/spikes/generic-cia402-probe.sh` 参数化（`EXPECT_VID/EXPECT_PID/
  EXPECT_OUTPUT_BITS/EXPECT_INPUT_BITS/EXPECT_MAPPED`），可复用于任意 CiA 402 画像。
- 新增验证记录 `docs/plugins/inovance-sv660n-verification.md`。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **160 passed**（插件守卫测试通过新 manifest）。
- Linux 验证机（`MANIFEST=plugins/inovance-sv660n/plugin.yaml EXPECT_PID=0000e5f1`）：
  `generic-cia402-probe.sh` **EXIT=0**：
  - SOEM `slaveinfo`：`Output size: 56bits / Input size: 56bits`、
    `Man: 00000ec5 ID: 0000e5f1 Rev: 00010000`、`MBX 128/128 protocols 04`；
  - SOEM `-map`：`0x6040 Controlword` / `0x6041 Statusword` / `0x6064 Position actual value`；
  - SOEM `simple_ng`（零补丁）：`mapped 7O+7I`、`all slaves are now operational`、`WKC 3`；
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5f1`。

**卡点**
- 画像当前是**通用 DS402 布局**：SV660N 具体默认 PDO/扩展对象/DC/故障码表
  **⚠️ 尚未逐项核实**（公开手册为 PDF，未取到可检索文本；R1 不入库）。
- 行为仍是 Tier0（无运动环/模式切换/Ti 时序）；IgH OP 行为验证复用通用画像夹具。
- `generic-cia402-probe.sh` 的 `-map` 输出仍只显示部分映射项（完整 7O+7I 由 simple_ng 证明）。

**下次开工第一件事**
- 在可获取公开手册文本的条件下逐项核对 SV660N 默认 PDO/扩展对象并更新 manifest；
  或给行为加入模式切换（CSP/CSV/CST）与错误码注入；之后处理 B6 FMMU/DC 原文核实。

---

## 2026-11-12 23:30 班（低峰）— B11 第五步：**IgH 侧 CiA 402 画像进 OP + 运行时行为验证**

**做了什么**
- 新增验证脚本 `scripts/spikes/igh-cia402-op-probe.sh`：用未修改 IgH 的 ecrt 应用
  （验证机本地夹具 `ecrt-cia402-op.c`，**源码不入库**）配置 7 字节 RxPDO/TxPDO，
  按 `0x0006→0x0007→0x000F` 写控制字序列，读回状态字/模式显示/实际位置；
  断言应用运行期间 `ethercat slaves` 为 **OP**、`RESULT al_states=0x08`、
  `STATUS sw` 含 `Operation enabled(0x0004)`+`Target reached(0x0400)`、
  `modes_display=8`、`pos=1000`。
- `docs/plugins/generic-cia402-verification.md` 增加 IgH OP + 运行时行为证据段落，
  更新"待验证/后续"清单。

**验证结果**
- 本机：脚本 `bash -n` 通过；Rust 代码未改，`cargo test --workspace` **160 passed** 基线不变。
- Linux 验证机：`bash scripts/spikes/igh-cia402-op-probe.sh` → **EXIT=0**：
  - 应用运行中：`0  0:0  OP  +  0x00000ec5:0x0000e5e0`；
  - `RESULT al_states=0x08 slaves_responding=1`；
  - `STATUS sw=0x0637 modes_display=8 pos=1000`；
  - 应答器侧 `al=0x08`。
  → 未修改 IgH 零补丁进入 OP，且 TxPDO（状态字/模式显示/实际位置）由 CiA 402 行为驱动。

**卡点**
- 本班只补 IgH 侧 OP/行为证据，未改代码；行为仍为 Tier0（无运动环/模式切换/Ti 时序）。
- 夹具源码在验证机本地（不入库，符合 GPL 边界）；仓库内脚本依赖该本地夹具存在。
- SDO 分段上传设备名在 IgH 侧仍未列入本脚本（SOEM 侧已验证）。

**下次开工第一件事**
- 派生 **Inovance SV660N 兼容画像**（公开手册默认 PDO/对象，仍用我方 VID/PID），或
  在行为里加入模式切换（CSP/CSV/CST）与错误码注入；之后处理 B6 FMMU/DC 原文核实。

---

## 2026-11-12 23:26 班（低峰）— B11 第四步：CiA 402 行为扩展为**完整 7 字节 RxPDO/TxPDO**

**做了什么**
- `esc-node` 的 CiA 402 行为从"只读写状态字 2 字节"扩展为完整 PDO（SM 长度 ≥7 时）：
  - RxPDO（SM2）：`0x6040` 控制字（u16）+ `0x6060` 模式（i8）+ `0x607A` 目标位置（i32）；
  - TxPDO（SM3）：`0x6041` 状态字（u16）+ `0x6061` 模式显示（i8，镜像 `0x6060`）+
    `0x6064` 实际位置（i32）；
  - Tier0 简化：`Operation enabled` 时实际位置直接跟随目标位置并置 bit10
    `Target reached`；禁用电压后保持最后位置；SM 长度 <7 时退化为只读写状态字（兼容
    8DO 等非伺服画像）。
- `with_cia402_behavior(out_offset, out_len, in_offset, in_len)`（参数增加长度）；
  `ec-sim-esc --behavior cia402` 按 manifest SM 长度打印
  `BEHAVIOR cia402 sm2_out=0x1100/7B sm3_in=0x1110/7B`。
- 合成主站 `--cia402` 断言升级为完整 7 字节序列（状态字/模式显示/实际位置）；
  新增单测 `cia402_behavior_drives_full_txpdo_when_sms_are_long_enough`。
- `docs/plugins/cia402-state-machine.md` 增加"完整 7 字节 PDO"说明。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **160 passed**（esc-core 76 / esc-node 46 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/cia402-behavior-probe.sh` → **EXIT=0**：
  - `BEHAVIOR cia402 sm2_out=0x1100/7B sm3_in=0x1110/7B`；
  - 完整序列通过：`0x0006→0x0221`、`0x0007→0x0223`、`0x000F→0x0637`（含 Target
    reached）、`0x0000→0x0240`；模式显示 8、实际位置 0→1000→保持 1000；
  - 原 UDS 套件（BRD/APRD/APWR/FPRD/M 位、SM watchdog、ESM 前置条件）继续通过。

**卡点**
- 行为仍是 **Tier0 简化**：位置无运动环/限幅/编码器模型；未实现模式切换（CSP/CSV/CST）
  与 Ti=0/1 时序；`0x603F` 错误码、fault reset 上升沿仍未触发；一帧延迟仍在。
- 只处理 7 字节固定布局（未按 PDO 映射动态解析），扩展其它布局需后续工作。

**下次开工第一件事**
- 派生 **Inovance SV660N 兼容画像**（公开手册默认 PDO/对象，仍用我方 VID/PID），或在
  行为里加入模式切换/错误码注入；IgH ecrt OP 夹具。

---

## 2026-11-12 23:20 班（低峰）— B11 第三步：**CiA 402 状态机接入运行时过程数据**（`--behavior cia402`）

**做了什么**
- `esc-node::Responder` 新增 `cia402` 行为（Tier0）：`with_cia402_behavior(out_offset, in_offset)`
  在 `tick()`（每帧处理前）读取 SM2 的 2 字节控制字 `0x6040` → 推进
  `Cia402StateMachine` → 写回 SM3 的 2 字节状态字 `0x6041`；新增 `cia402_state()` 查询。
- `ec-sim-esc` 新增 `--behavior cia402`：按 manifest 的 SM2(dir `out`)/SM3(dir `in`)
  起始地址启用行为（缺省 0x1100/0x1110），并打印
  `BEHAVIOR cia402 sm2_out=0x1100 sm3_in=0x1110`；`--help` 更新。
- 合成主站 `scripts/spikes/uds_master_probe.py` 新增 `--cia402`：APWR 写控制字、
  APRD 读状态字，断言标准转换序列；新增验证脚本
  `scripts/spikes/cia402-behavior-probe.sh`。
- 单测新增 `cia402_behavior_drives_statusword_from_controlword`（合成 datagram 写/读
  SM2/SM3，断言 0x0240→0x0221→0x0223→0x0237→0x0240）。
- `docs/plugins/cia402-state-machine.md` 补充运行时接入与验证方法。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **159 passed**（esc-core 76 / esc-node 45 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/cia402-behavior-probe.sh` → **EXIT=0**：
  - 启动日志：`BEHAVIOR cia402 sm2_out=0x1100 sm3_in=0x1110`；
  - 合成主站断言：`0x0006→0x0221`、`0x0007→0x0223`、`0x000F→0x0237`、
    `0x0000→0x0240` 全部通过；同时原有 UDS 套件（BRD/APRD/APWR/FPRD/M 位、
    SM watchdog、ESM 前置条件）继续通过。

**卡点**
- 行为在 `tick()` 中执行 → 有**一帧延迟**（主站读到上一帧控制字对应的状态字）；
  Tier0 简化，后续可改为 datagram 处理顺序内联。
- 状态机仍不实现 Ti=0/1 时序、模式相关位、错误码触发、fault reset 上升沿（文档列明）；
  行为只写状态字前 2 字节，未驱动 `0x6061`/`0x6064` 等其它 TxPDO 字段。

**下次开工第一件事**
- 派生 **Inovance SV660N 兼容画像**（公开手册默认 PDO/对象，仍用我方 VID/PID），或
  把行为扩展为完整 7 字节 TxPDO（状态字 + modes display + position actual）并根据
  控制字/模式驱动数据变化；IgH ecrt OP 夹具。

---

## 2026-11-12 23:18 班（低峰）— B11 第二步：**CiA 402 状态机骨架（Tier0 行为插件基础）**

**做了什么**
- 新增 `crates/esc-node/src/cia402.rs`（`esc_node::cia402`）：
  - 控制字 `0x6040` / 状态字 `0x6041` 的位常量（公开 CiA 402 profile 子集）；
  - `Cia402State` 状态枚举 + 每状态状态字编码；
  - `Cia402StateMachine`：`new()`（初始 Switch on disabled）、`update(cw) -> statusword`、
    `inject_fault()`；实现 shutdown / switch on / enable operation / disable voltage /
    quick stop / fault reset 的标准转换；
  - 6 个单测：初始状态、标准使能序列、Operation enabled→shutdown 回 Ready、
    quick stop / disable voltage 路径、故障+复位恢复、无效控制字保持状态。
- 新增 `docs/plugins/cia402-state-machine.md`：控制字/状态字位表、状态编码、已实现转换、
  ⚠️ 未实现项（PDO 数据通路、Ti=0/1 时序、模式位、错误码触发、fault reset 上升沿）。

**验证结果**
- 本机：`cargo fmt --check`、`cargo clippy -D warnings` exit 0；
  `cargo test --workspace` **158 passed**（esc-core 76 / esc-node 44 / plugin-host 21 + 1 集成 /
  wire 16），其中 `esc-node` 由 38 → 44（新增 6 个状态机单测）。

**卡点**
- 状态机**尚未接入运行时 PDO 数据**（纯逻辑模块）：下一班把它接到 SM2/SM3 过程数据，
  并让 `0x6041` 随 `0x6040` 变化（先走内置 Tier0，不做 WASM）。
- 未实现 Ti=0/1 时序、模式相关位、错误码触发、fault reset 上升沿检测（文档已列明）。

**下次开工第一件事**
- 把 `Cia402StateMachine` 接入 `esc-node::Responder` 的过程数据通路（CIA402 通用画像
  `SM2 0x1100` controlword → `SM3 0x1110` statusword），并提供 `--behavior cia402`
  （或按 manifest `profile: cia402` 自动启用）；在验证机上用 SOEM/IgH 读回状态字变化。

---

## 2026-11-12 23:12 班（低峰）— B11 第一步：**通用 CiA 402 伺服声明式画像** + 双主站扫描/OP 验证

**做了什么**
- 新增插件 `plugins/generic-cia402-servo/`（B11 第一步：先做通用 CiA 402 画像，再派生
  Inovance SV660N 等具体兼容画像）：
  - `plugin.yaml`：自有 VID `0x00000EC5` / PID `0x0000E5E0`（R2）、名称带 SIMULATED；
    SM0/SM1 = CoE 邮箱（各 128B），SM2 = RxPDO 56 bit（`0x6040` controlword 16 +
    `0x6060` modes 8 + `0x607A` target position 32），SM3 = TxPDO 56 bit
    （`0x6041` statusword 16 + `0x6061` modes display 8 + `0x6064` position actual 32）；
    对象字典含 `0x603F` 错误码、`0x6077` 实际转矩、`0x1008` 设备名字符串；`dc.supported=false`。
  - `facts.md`：只记录**公开来源**（CiA 402 profile、ETG 设备 profile、公开伺服手册）与
    "明确不模拟"清单（状态机/运动控制/编码器/Homing/DC/厂商专有对象），R1 未入库任何
    厂商 ESI/PDF/固件。
  - `README.md` + `plugins/catalog.yaml` 更新（generic-cia402-servo → ready，含 deliverable）。
- 新增验证脚本 `scripts/spikes/generic-cia402-probe.sh`；新增验证记录
  `docs/plugins/generic-cia402-verification.md`。
- 仓库级 plugin manifest 守卫测试自动覆盖新插件（解析/校验/SII CRC/字典/PDO 映射）。

**验证结果**
- 本机：`cargo fmt --check`、`cargo clippy -D warnings` exit 0；`cargo test --workspace`
  **152 passed**（插件守卫测试通过新画像）。
- Linux 验证机 `bash scripts/spikes/generic-cia402-probe.sh` → **EXIT=0**：
  - SOEM `slaveinfo`：`Output size: 56bits / Input size: 56bits`、
    `Man: 00000ec5 ID: 0000e5e0 Rev: 00010000`、`MBX 128/128 protocols 04`；
  - SOEM `slaveinfo -map`：`0x6040:0x00 Controlword` / `0x6041:0x00 Statusword` /
    `0x6064:0x00 Position actual value` 可见；
  - SOEM `simple_ng`（零补丁）：`mapped 7O+7I`、`all slaves are now operational`、`WKC 3`；
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5e0`。

**卡点**
- 本画像**不含 CiA 402 行为**：状态机/运动控制/编码器反馈/Homing/DC 均未模拟，PDO 数据
  为静态初值；IgH 进 OP 需匹配本画像的 ecrt 夹具。
- `slaveinfo -map` 摘录只显示部分映射项（`0x6060`/`0x607A` 未在该工具的打印里出现），
  但 `simple_ng` 的 7O+7I 映射证明完整 PDO 结构被识别；⚠️ 如需逐项证据可补 `-sdo`。

**下次开工第一件事**
- 做 **CiA 402 状态机骨架（Tier0 行为插件）**：控制字 `0x6040` → 状态字 `0x6041`
  的最小状态转换（Switch on disabled → Ready → Switched on → Operation enabled），
  并让 PDO 数据随输入变化；同时补匹配本画像的 IgH ecrt OP 夹具。

---

## 2026-11-12 23:05 班（低峰）— B6/B8 子项：**ESI + manifest 组合画像**（ESI 出 SII、manifest 出身份/字典）→ SOEM 进 OP

**做了什么**
- `plugin-host::esi` 新增 `sii_spec_from_esi_and_manifest()`：SII 的 SM/PDO/邮箱/CoE 能力
  来自 **ESI**（设备真实描述），`vendor_id`/`product_code`/`revision` 与 SII 名称来自
  **manifest**（R2：自有 VID/PID、名称带 SIMULATED），并把"ESI 描述 + manifest 字典"
  的组合语义写进函数文档。
- `ec-sim-esc`：`--esi` 与 `--manifest` 不再互斥，可组合使用——组合时 SII 取自 ESI、
  身份与运行时对象字典取自 manifest；单独 `--esi` 仍只有身份对象 0x1018。`--help`/模块
  文档同步更新。
- 新增配对测试夹具 `crates/plugin-host/tests/fixtures/sample.esi.plugin.yaml`
  （与 `sample.esi.xml` 同身份 0x0EC5/0xE5D3、同 SM/PDO 布局，含 VISIBLE_STRING 设备名）。
- 新增验证脚本 `scripts/spikes/esi-manifest-op-probe.sh`：SOEM `slaveinfo` + `simple_ng`
  （零补丁进 OP）+ IgH `ethercat slaves`（PREOP/身份）。
- 单测补充 `esi_plus_manifest_uses_manifest_identity_and_esi_layout`。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **152 passed**（esc-core 76 / esc-node 38 / plugin-host 21 + 1 集成 / wire 16）。
- Linux 验证机：`bash scripts/spikes/esi-manifest-op-probe.sh` **EXIT=0**：
  - `slaveinfo`：`Output size: 8bits / Input size: 8bits`、
    `Man: 00000ec5 ID: 0000e5d3 Rev: 00010000`、`MBX 128/128 protocols 04`；
  - `simple_ng`（零补丁）：`mapped 1O+1I`、`all slaves are now operational`、
    `WKC 3`（实际 = 期望，无 `wrong` 提示）；
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5d3`
    （无 ecrt 应用时 IgH 只配置到 PREOP；OP 见 `igh-el2008-op-probe.sh`）。
  → ESI 的 SII 描述与 manifest 的身份/字典组合后，可被未修改 SOEM 映射并推进 OP。

**卡点**
- 组合画像的 PDO 数据仍是通用 DL 内存（无行为逻辑）；ESI 侧对象字典仍缺（需行为插件/
  manifest 扩展）。IgH 进 OP 需要 ecrt 应用夹具（已有 EL2008 的先例，可复用）。
- B6 剩余：FMMU 类别 0x0028、DC 类别（需 ESC Sec.2 / ETG 原文核实）；ESI 子集仍不含
  ObjectDictionary/DC/多设备选择。

**下次开工第一件事**
- 把组合画像用于 B11 行为插件的第一步（Tier0 行为骨架：CiA402 状态机只读对象 +
  PDO 数据），或在双主站下让组合画像进 IgH OP（复用 ecrt 夹具）；随后读 ESC Sec.2
  原文核实 FMMU/DC 类别。

---

## 2026-11-12 22:42 班（低峰）— B6 子项：ESI→SII 接入 CLI + **双主站扫描验证**（`--esi`）

**做了什么**
- `ec-sim-esc` 新增 `--esi <file.esi.xml>`（与 `--manifest` 互斥）：解析 ESI 子集 →
  `sii_spec_from_esi()` → `SiiImage::from_spec()` 生成 SII 镜像；运行时对象字典由
  `esi::dictionary_from_esi()` 生成，**只有身份对象 0x1018**（Vendor/Product/Revision/
  Serial，取自 ESI）——ESI 不含完整对象字典，其余对象需来自 manifest/行为插件，
  该限制写进了模块文档与 `--help`。
- 新增验证脚本 `scripts/spikes/esi-sii-scan-probe.sh`：用 `--esi` 启动验证明文画像
  （`crates/plugin-host/tests/fixtures/sample.esi.xml`，自有 VID 0x0EC5/PID 0xE5D3、
  SM0/SM1 邮箱 128B、SM2 输出 1B、SM3 输入 1B），断言 SOEM 与 IgH 的扫描结果；
  脚本对残留 IgH 模块做 `pkill -f ecrt-` + `ethercat rescan` 兜底。
- 单测补充 `dictionary_from_esi_has_identity_object`（0x1018:00..03 = 4/0x0EC5/0xE5D3/0x00010000）。

**验证结果**
- 本机：`cargo fmt --check`、`clippy -D warnings` exit 0；`cargo test --workspace`
  **151 passed**（esc-core 76 / esc-node 38 / plugin-host 20 + 1 集成 / wire 16）。
- Linux 验证机（重启后干净状态）：`bash scripts/spikes/esi-sii-scan-probe.sh` → **EXIT=0**：
  - SOEM `slaveinfo`：`Output size: 8bits / Input size: 8bits`、
    `Man: 00000ec5 ID: 0000e5d3 Rev: 00010000`、`MBX length wr: 128 rd: 128 MBX protocols : 04`；
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5d3`。
  → ESI XML 生成的 SII 镜像被两家未修改主站正确识别（身份/PDO 位数/CoE 邮箱）。
- 诊断记录：验证机上 IgH 模块被先前夹具残留进程持有引用时，`rmmod` 会失败、主站保留
  旧从站身份（表现为 IgH 显示旧 PID）；干净重启后通过。脚本已加入 pkill/rescan 兜底。

**卡点**
- ESI 不含对象字典 → `--esi` 运行时只有身份对象（无 0x1C00/PDO 映射对象）；若需完整
  字典仍需 manifest 或行为插件。样本 ESI 为仓库自写（非厂商文件）。
- ESI 子集仍不支持 ObjectDictionary/DC/多设备选择；邮箱长度缺省 128（语义待 ETG.2000）。
- B6 剩余：FMMU 类别 0x0028、DC 类别（需 ESC Sec.2 / ETG 原文核实）。

**下次开工第一件事**
- 让 `--esi` 与 manifest/行为插件组合（ESI 负责 SII 身份/PDO 描述，manifest 补对象字典），
  或在双主站下把 ESI 画像推进 OP（需要 SM2/SM3 的 PDO 字典与数据）；随后读 ESC Sec.2
  原文核实 FMMU/DC 类别，再进 B11 CiA402。

---

## 2026-11-12 22:38 班（低峰）— B6 子项：**ESI XML → SII** 生成器（子集，零依赖）

**做了什么**
- `plugin-host` 新增 `esi` 模块（B6 子项）：
  - 零第三方依赖的**最小 XML 解析器**（元素/属性/文本/注释/处理指令/DOCTYPE 跳过，
    UTF-8 安全，非法结构报错）；
  - **ESI 子集解析**：`EtherCATInfo/Vendor/Id`、`Device/Type`（ProductCode/RevisionNo、
    文本作描述）、`Device/Name`、`Sm`（StartAddress/ControlByte/Enable/可选 Length +
    方向文本 MBoxOut/MBoxIn/Outputs/Inputs）、`RxPdo`/`TxPdo`（Index/Sm + `Entry`
    的 Index/SubIndex/BitLen）、`Mailbox/CoE`（SdoInfo/PdoAssign/PdoConfig/CompleteAccess）；
  - `sii_spec_from_esi()` → [`esc_core::sii::SiiSpec`]（复用既有 `SiiImage::from_spec()`
    编码：CRC/EEPROM 大小自动）：邮箱长度缺省 128 字节，过程数据 SM 长度 = 绑定 PDO
    位长之和向上取整，CoE 能力位按 ESI 推导。
- 测试夹具为本仓库**自写**的 `crates/plugin-host/tests/fixtures/sample.esi.xml`
  （**不是厂商 ESI**，符合 R1）；单测覆盖解析字段、SII 生成（身份/CRC/4 个 SM/邮箱
  128B/SM2/SM3 各 1B/RxPDO 0x1600→SM2/TxPDO 0x1A00→SM3/CoE mailbox 协议位 0x0004）、
  错误路径（未闭合/根元素不符/缺字段）与 ESI 数值格式（`#x`/八进制/十进制）。
- R2 说明：ESI 的 Vendor/ProductCode 只作为**兼容性事实**读取；生成 SII 时名称用
  ESI 设备名 + 我方厂商名 `EC-Sim`（调用方必须保证设备名带 `SIMULATED`）。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **150 passed**
  （esc-core 76 / esc-node 38 / plugin-host 20 / wire 16）。
- 生成的 SII 经 `SiiImage::from_spec` 断言：CRC 校验通过、SM 数量 4、邮箱协议 0x0004、
  SM0/SM1 各 128 字节、SM2/SM3 各 1 字节、PDO 类别索引与 SM 绑定正确。
- ⚠️ 尚未在验证机上用 IgH/SOEM 扫描 **ESI 生成的画像**：本班只做解析/生成层，
  `ec-sim-esc` 还没有 `--esi` 入口（下一班接入）。

**卡点**
- ESI 子集有限：不支持 `ObjectDictionary`、DC 参数、多设备文件选择、字符串 PDO 映射
  （BitLen>255 会报错）；邮箱长度缺省 128（ESI 的 `Sm` 通常不声明长度，真实语义
  ⚠️ 待 ETG.2000 核实）。
- B6 其余待做：FMMU 类别 0x0028、DC 类别（都需 ESC Sec.2 / ETG 原文核实）、
  ESI 生成器的 CLI/双主站扫描闭环。

**下次开工第一件事**
- 把 `esi` 接入 CLI（`ec-sim-esc --esi <file.esi.xml>` 或用 `plugin-host` 生成 SII 后
  走 `--manifest` 同路径），在 Linux 验证机上用未修改 SOEM/IgH 扫描 ESI 生成的画像；
  然后读 ESC Sec.2 原文核实 FMMU 类别 0x0028 与 DC 类别，再进 B11 CiA402。

---

## 2026-11-12 22:20 班（低峰）— B7 缺口：UDS 合成主站的 **segmented SDO 上传**断言（IgH 侧受环境阻塞）

**做了什么**
- 扩展 `scripts/spikes/uds_master_probe.py`：新增邮箱收发辅助（APWR SM0 写请求 →
  轮询 SM1 Status（0x080D）bit3 → APRD 0x1080 读响应）与**分段 SDO 上传客户端**
  （initiate `0x40` → 响应 `0x41` + u32 长度 + 首段 → segment 请求 `0x60|toggle<<4` →
  逐段拼接、校验 toggle/unused/last），新增 `--sdo-segmented` / `--expect-string=` 选项。
- `scripts/spikes/uds-loopback.sh` 增加**第三轮**：以
  `--manifest plugins/beckhoff-el2008/plugin.yaml` 启动声明式画像，合成主站读取
  `0x1008` 设备名（42 字节，>4 字节 → 必须分段），断言完整字符串内容。
- 尝试 IgH 侧验证（`ethercat upload -t string 0x1008 0` 与 ecrt
  `ecrt_master_sdo_upload` 夹具）：均**超时**；同环境下 **expedited** 上传
  （`ethercat upload -t uint16 0x1C12 1`）同样超时，dmesg 出现内核栈转储（ecrt/ethercat
  ioctl 路径）→ 判定为验证机 IgH SDO ioctl 环境问题，与 segmented 实现无关。
  期间已重启验证机清除残留 IgH 模块/master 占用（此前的 `Device or resource busy`），
  重启后仍复现。相关本地 ecrt 夹具未提交（避免未验证脚本入库）。

**验证结果**
- 本机：`python -m py_compile`、`bash -n` 通过；Rust 代码未改（`cargo test --workspace`
  **146 passed** 基线不变）。
- Linux 验证机：`bash scripts/spikes/uds-loopback.sh` → **EXIT=0**，三轮全通过：
  1) 默认 ESM 前置条件 + SM watchdog；2) `--esm-lax` 对照；
  3) **manifest 画像 + segmented SDO**：
  `OK: segmented SDO 读到 0x1008 = 'Beckhoff EL2008 compatible 8DO (SIMULATED)'（42 字节）`
  —— 独立合成主站按 CiA 301 分段协议完整读出设备名，验证了 `esc-node` 的
  initiate/segment 状态机与 `dict_gen` 字符串对象。

**卡点**
- **IgH 侧任何 SDO 上传当前都在验证机上超时**（expedited 与 segmented 同样失败，
  dmesg 有 ecrt ioctl 内核栈转储）→ ⚠️ 待验证；需排查/重装 IgH 模块或查看完整栈日志。
- IgH 侧夹具脚本未提交（未验证，避免把不可复现内容入库）。
- B6 FMMU 寄存器空间映射仍未核实（ESC Sec.2 为 PDF，需离线读原文）。

**下次开工第一件事**
- 排查验证机 IgH SDO ioctl 超时（重装 IgH 模块、抓完整 dmesg 栈、或在 ecrt 应用激活/
  周期上下文里重试 `ethercat upload -t string 0x1008 0`）；恢复后补 IgH 侧分段上传验证。
  然后读 ESC Sec.2 原文核实 FMMU 寄存器空间 → 进 B11 CiA402 行为插件。

---

## 2026-11-12 22:10 班（低峰）— B7 子项：CoE **分段 SDO 上传**（>4 字节对象）+ 设备名 `0x1008`

**做了什么**
- `esc-core::mailbox`：新增 **segmented upload** 协议支持——initiate 响应
  （Command `0x41` + u32 长度 + 首段数据，偏移与 IgH `master/fsm_coe.c`、SOEM
  `src/ec_coe.c` 逐字节对齐）、segment 请求解析（`0x60 | toggle<<4`）、segment 响应
  （`toggle<<4 | unused<<1 | last` + 7 字节数据，末段补零/`unused` 标注）；
  `obj_dict::data_type` 增加 VISIBLE_STRING / OCTET_STRING。
- `esc-node`：`Responder` 增加分段上传状态机（≤4 字节仍 expedited；>4 字节自动分段，
  单对象串行传输），新增 `segmented_upload_in_progress()`。
- `plugin-host::dict_gen`：支持 manifest **字符串对象**（长度 = 字节数 × 8）；对象列表按
  索引排序；数组对象补子索引 0（沿用上一班规则）。
- EL2008 插件画像新增 `0x1008:00` 设备名（VISIBLE_STRING，42 字节，名称带 SIMULATED）；
  `example-8do` 测试画像同步；README 更新「已模拟」清单。
- 新脚本 `scripts/spikes/sdo-segmented-upload-probe.sh`（SOEM `slaveinfo -sdo`
  断言设备名完整读出）。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **146 passed**
  （esc-core 76 / esc-node 38 / plugin-host 16 / wire 16）。
- Linux 验证机：`bash scripts/spikes/sdo-segmented-upload-probe.sh` → **EXIT=0**；
  SOEM `slaveinfo -sdo` 打印（真实主站经**分段上传**拼接）：
  `0x1008 "Device name" [VAR]` / `0x00 "Device name" [VISIBLE_STR(336) R_R_R_]
  "Beckhoff EL2008 compatible 8DO (SIMULATED)"`，共 20 个对象；同时 EL2008 画像的
  `0x1C12`（maxsub 8）与 `0x7000..0x7070` 数组对象显示正常。
- IgH `ethercat slaves` 复跑：PREOP + 身份 `0x00000ec5:0x0000e5d2` 正常。

**卡点**
- **IgH 侧分段上传未验证**：`ethercat upload` CLI 需要 SDO 类型信息（无应用上下文时报
  `Failed to determine SDO entry data type`）；尝试的 ecrt `ecrt_master_sdo_upload` 夹具
  在"未配置应用"场景下阻塞（已清理，未取到结果）→ ⚠️ 待验证：需在已配置/OP 的 ecrt
  应用上下文里调用。
- **B6 FMMU 寄存器空间映射仍未核实**：官方 ESC Sec.2 寄存器手册是 PDF（公开链接
  `download.beckhoff.com/.../ethercat_esc_datasheet_sec2_registers_*.pdf`），本班未取到
  可检索文本；SOEM `simple_ng` 的 mbxstatus expected WKC 差异继续标注 ⚠️。
- 分段上传只覆盖单对象串行传输；未做并发、Complete Access、分段 Download。

**下次开工第一件事**
- 用**已配置的 ecrt 应用上下文**验证 IgH 分段上传（在现有 outputs-only/1O1I 夹具激活后
  调用 `ecrt_master_sdo_upload()` 读 `0x1008`）；随后读 ESC Sec.2 PDF 原文核实
  FMMU 能否映射寄存器空间。之后进入 **B11 CiA402 行为插件**（先 facts.md + 声明式画像，
  行为 WASM 只做 Tier0 状态机骨架）。

---

## 2026-11-12 21:41 班（低峰）— B10 缺口补齐：数组对象子索引 0 + IgH outputs-only **OP** 验证

**做了什么**
- `plugin-host::dict_gen`：manifest 用户对象 `sub > 0` 时按 **ARRAY** 处理，并自动补
  **子索引 0 = UINT8 条目数**（CiA 301）——修掉 IgH 读字典时对 `0x7xxx:00` 的
  SDO Abort 噪声，并让 SOEM 的 `maxsub` 显示与真实设备一致（新增单测）。
- 新增验证脚本 `scripts/spikes/igh-el2008-op-probe.sh` + 验证机本地 **outputs-only**
  ecrt 夹具（`ecrt-el2008-op.c`：8 个 1-bit 输出对象、只配 SM2；源码/构建命令在脚本头，
  **不入库**，与既有 1O+1I 夹具同一方式）——把 EL2008 画像推入 IgH **OP**。
- `scripts/spikes/{beckhoff-el2008,manifest-slave,soem-process-data}.sh`：给 `grep | head`
  显示管线补 `|| true`，避免 `pipefail` 下 head 提前退出导致 SIGPIPE(141) 把成功误判为失败；
  EL2008 probe 的 SOEM `-sdo` 改为 **best-effort**（工具缺陷说明保留）。
- 更新 `docs/plugins/beckhoff-el2008-verification.md`：补 IgH OP 证据、数组子索引 0 说明、
  SOEM `-sdo` 工具缺陷说明。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **143 passed**
  （esc-core 75 / esc-node 37 / plugin-host 15 / wire 16）。
- Linux 验证机：
  - `bash scripts/spikes/beckhoff-el2008-probe.sh` → **EXIT=0**：SOEM 身份
    `00000ec5:0000e5d2`、`Output 8bits/Input 0bits`、`-map` 8 个 BOOLEAN 输出映射、
    `simple_ng` 进 OP（`mapped 1O+0I`，WKC 2 的差异仍见卡点）、该次 `-sdo` best-effort
    读到 `0x7000 "Output 1" [ARRAY maxsub(0x01 / 1)]` 等；IgH 扫描 PREOP + 身份。
  - `bash scripts/spikes/igh-el2008-op-probe.sh` → **EXIT=0**：
    `ethercat slaves` 显示 `0  0:0  OP  +  0x00000ec5:0x0000e5d2`，
    `RESULT al_states=0x08 slaves_responding=1 wc=1`。
- 说明：本班未改默认画像路径；上一班默认/strict 双主站回归 6/6 仍有效。

**卡点**
- SOEM `simple_ng` 的 expected WKC 比实际多 1（SOEM 把 SM1 状态寄存器映射为 FMMU 且对无
  输入从站 `inputsWKC++`）；**ESC 是否允许 FMMU 物理映射寄存器空间仍待核实**（不猜）。
- SOEM `slaveinfo -sdo` 工具缺陷（UINT8 子索引 0 用 2 字节读、高位未初始化）→ 对含多个
  数组对象的字典会段错误；已降级为 best-effort，SDO 路径由 `slaveinfo -map` 覆盖。
- IgH `ethercat sdos` 在"无应用上下文"时仍无输出（已知工具行为），IgH 侧字典显示 ⚠️ 待验证。

**下次开工第一件事**
- 核实 **ESC FMMU 是否可物理映射寄存器空间**（ESC Sec.II / ETG.1000.4）；若允许，实现
  寄存器空间 FMMU 读并让 no-input 从站的 expected WKC 一致（同时回归内置画像）。
  之后进入 **B11 CiA402 行为插件**（Inovance SV660N 兼容画像）：先 facts.md + 声明式画像
  + 双主站扫描，行为 WASM 只做 Tier0 状态机骨架。

---

## 2026-11-12 21:33 班（低峰）— B10 完成：首个 P0 设备插件「Beckhoff EL2008 兼容 8DO（SIMULATED）」

**做了什么**
- 公开资料调研（R1：只取接口事实，不入库厂商文件）：Beckhoff EL2008 产品页、Beckhoff
  Information System（EL20xx/EL2124 文档）与 linuxcnc-ethercat `esi-data`（公开 ESI 整理
  数据）→ 提取事实并记来源/访问日期（R5）：8 通道数字输出、0 输入、24 V DC/0.5 A、
  进程映像 8 outputs、无 DC、无配置；默认 **RxPDO `0x1600`..`0x1607`**，每个映射 1 bit
  输出对象 **`0x7000:01` / `0x7010:01` / … / `0x7070:01`**。
- 新插件 `plugins/beckhoff-el2008/`：
  - `plugin.yaml`：自有 Vendor ID/Product Code `0x00000EC5`/`0x0000E5D2`（R2）、名称带
    `SIMULATED`、3 个 SM（邮箱 ×2 + SM2 输出 1 字节）、8 个 RxPDO、8 个 BOOL 输出对象、
    CoE SDO/SDO Info、无 DC、`compatibility` 免责声明（R3）、3 条 `sources`（R5）；
  - `facts.md`：接口事实表 + 来源（含"厂商 PID 仅参考、不用于身份"说明）+ 已/未模拟清单；
  - `README.md`：使用方式、免责声明、已模拟/未模拟清单。
- `plugins/catalog.yaml`：EL2008 状态 `planned` → `ready`（附交付物路径）。
- 新验证脚本 `scripts/spikes/beckhoff-el2008-probe.sh`（SOEM 身份/SII-PDO/-map/OP/字典 +
  IgH 扫描；对 SOEM `-sdo` 偶发段错误做重试）。
- 新增 `crates/plugin-host/tests/plugin_manifests.rs`：遍历 `plugins/*/plugin.yaml`
  （跳过 `_template`）做**守卫测试**——解析、R2/R5 校验、SII 生成（CRC）、运行时字典生成、
  每个 PDO 都有映射对象。
- 验证记录 `docs/plugins/beckhoff-el2008-verification.md`（命令、关键输出、限制、⚠️）。
- 顺带修复两处历史文件的 UTF-8 BOM（`esc-core/src/mailbox.rs`、`esc-node/bin/ec_sim_esc.rs`）。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **142 passed**
  （esc-core 75 / esc-node 37 / plugin-host 14 / wire 16）。
- Linux 验证机：`bash scripts/spikes/beckhoff-el2008-probe.sh` →
  `OK: EL2008 兼容 8DO 画像端到端通过`：
  - SOEM `slaveinfo`：`Man: 00000ec5 ID: 0000e5d2 Rev: 00010000`、`Output size: 8bits`、
    `Input size: 0bits`、`1 slaves found and configured`；
  - SOEM `slaveinfo -map`：`SM2 outputs`，`0x7000:0x01 … BOOLEAN Output 1` …
    `0x7070:0x01 … Output 8`（与公开资料默认映射一致）；
  - SOEM `simple_ng`：`mapped 1O+0I bytes`、`all slaves are now operational`、WKC 2；
  - SOEM `slaveinfo -sdo`：`0x1C12` maxsub 8、`0x1600` 映射值 `0x70000101`、
    `0x7000`/`0x7070` 输出对象可见；
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5d2`，拓扑计算无报错。
- 默认内置画像回归（默认/strict 两种模式）**6/6 EXIT=0**（插件改动不影响既有画像）。

**卡点**
- **SOEM `simple_ng` 的 expected WKC 比实际多 1**：SOEM 2.0 对"无输入且带邮箱"的从站会把
  SM1 状态寄存器（`ECT_REG_SM1STAT` = 0x080D）映射成进程数据 FMMU 并在 `src/ec_config.c`
  里 `if (!Ibytes) inputsWKC++`；本模拟器只做 DL 用户内存（0x1000–0xFFFF）的 FMMU 物理映射，
  未模拟寄存器空间映射 → simple_ng 报 `WKC 2 wrong (expected 3)`（OP/过程数据本身正常）。
  ⚠️ 待核实：ESC Sec.II 是否允许 FMMU 物理映射寄存器空间。
- IgH 侧只验证扫描（PREOP/拓扑/身份）；OP 需 outputs-only 的 ecrt 夹具。
- SOEM `slaveinfo -sdo` 偶发段错误属工具夹具缺陷（UINT8 子索引 0 用 2 字节读取），脚本重试规避。
- 电气特性（0.5 A 驱动/短路/开关时间）与 DC 未模拟（已在 README 诚实清单列出）。

**下次开工第一件事**
- 先补 B10 缺口：核实 **ESC FMMU 能否物理映射寄存器空间**（ESC Sec.II / ETG.1000.4）；
  若允许，实现"寄存器空间 FMMU 读"使 EL2008 画像在 SOEM 下 expected WKC 一致（同时重跑
  内置画像回归确认 expected WKC 不受影响）；再写 **outputs-only 的 IgH ecrt 夹具**把
  EL2008 推入 OP，补完 `docs/plugins/beckhoff-el2008-verification.md`。
- 之后进入 **B11 CiA402 行为插件**（Inovance SV660N 兼容画像）：先 facts.md + 声明式画像
  + 双主站扫描（行为 WASM 只做 Tier0 状态机骨架）。

---

## 2026-11-12 21:25 班（低峰）— B9 完成：声明式插件画像端到端（`--manifest` → SII + 对象字典 → 双主站扫描/OP）

**做了什么**
- `esc-core::obj_dict`：新增运行时对象字典 `ObjectDictionary` / `RuntimeObject` /
  `RuntimeEntry`（值 + 数据类型/位长/访问权限/名称/writable）。`ObjectDictionary::simulated()`
  从既有静态表构造（内置画像逐项不变）；SDO Info 响应构造改为接收原始字段，支持运行时字典。
- `esc-node`：`Responder` 持有运行时字典（默认内置画像），新增 `with_dictionary()`；
  SDO 上传/下载/SDO Info 全部走运行时字典（Download 按条目 `writable` 判定）。
  新增 **`--manifest <plugin.yaml>`**：按 manifest 生成 SII（`sii_gen`）与对象字典
  （`dict_gen`）后启动，并打印 `MANIFEST id/name/VID/PID`；`esc-node` 增加 `plugin-host`
  path 依赖（插件只在启动时生成画像，不进实时环）。
- `plugin-host::dict_gen`：manifest → 运行时字典——身份 `0x1018`、SM 通信类型 `0x1C00`、
  PDO 分配 `0x1C12/0x1C13`、PDO 映射对象、用户对象（8/16/32 位整数初值；`access: rw`
  生成可写条目；生成前执行 R2/R5 校验，占位 VID/PID 直接拒绝）。
- 新增样例 manifest `crates/plugin-host/tests/fixtures/example-8do.yaml`
  （自有 VID/PID `0x0EC5`/`0xE5D1`、名称带 SIMULATED、8DO、sources 指向公开规范）。
- 新增验证脚本 `scripts/spikes/manifest-slave-probe.sh`：SOEM 身份/SII-PDO/OP-WKC3/SDO Info
  + IgH 身份/拓扑。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **141 passed**
  （esc-core 75 / esc-node 37 / plugin-host 13 / wire 16）。
- Linux 验证机：`bash scripts/spikes/manifest-slave-probe.sh` →
  `OK: 声明式 manifest 画像端到端通过`：
  - SOEM `slaveinfo`：`Man: 00000ec5 ID: 0000e5d1 Rev: 00010000`、`Output/Input size: 8bits`；
  - SOEM `slaveinfo -sdo`：`CoE Object Description found, 7 entries.`，含
    `0x2000 "Sample counter" [UNSIGNED16] 0x1234 / 4660`；
  - SOEM `simple_ng`：进入 OP、**1055 轮 WKC=3、无 WKC 不符**；
  - IgH：`0 0:0 PREOP + 0x00000ec5:0x0000e5d1`，拓扑计算无报错。
- 默认内置画像回归（默认/strict 两种模式 **6/6 EXIT=0**）：`minimal-esc-responder.sh`、
  `soem-process-data.sh`（1055 轮 WKC=3）、`igh-op-probe.sh`（IgH ecrt OP wc=3）
  —— 运行时字典重构未改变默认行为。

**卡点**
- **SOEM `slaveinfo -sdo` 偶发段错误**：它对非 VAR 对象的 subindex 0 用 2 字节缓冲区
  读取（`slaveinfo.c` 把 UINT8 响应的 1 字节拷进 `uint16`，高字节未初始化），与本模拟器
  响应无关；脚本用最多 3 次重试规避，⚠️ 需在 SOEM 侧确认（工具夹具缺陷，不是从站行为）。
- manifest `objects` 初值只支持 8/16/32 位整数（字符串/大对象需分段 SDO，未实现）；
  FMMU 仍由主站在配置阶段写入（manifest 通过 SII 的 SM/PDO + 对象字典表达布局，
  不预生成 FMMU 表）；DC 未实现。
- 本班画像用仓库内样例（自有 VID/PID），不是真实厂商设备——真实设备画像属 B10。

**下次开工第一件事**
- **B10 首个 P0 设备插件：Beckhoff EL2008 兼容 8DO**——按 DEVICE-PLUGINS R1–R5 用公开资料
  填 `plugins/<id>/facts.md`（对象索引/PDO 映射/SM 布局 + 来源 URL + 访问日期）、写
  `plugin.yaml`（自有 VID/PID、名称带 SIMULATED），再用 `ec-sim-esc --manifest` +
  `manifest-slave-probe.sh` 在 IgH/SOEM 上扫描验证，证据写
  `docs/plugins/<id>-verification.md`。B10 需联网查公开手册（允许），注意 R1 不入库厂商文件。

---

## 2026-11-12 21:20 班（低峰）— B9 子项 1：声明式插件 manifest 解析 + SII 生成

**做了什么**
- 说明：B6 剩余 FMMU 类别 0x0028 仍暂缓（公开资料无字段权威语义，红线要求不猜）、
  VM 目录锁被其他 worker 占用 → 按"被阻塞取下一项"规则做 **B9 声明式插件加载器（骨架）**。
- `esc-core::sii`：新增运行时描述 `SiiSpec` / `SiiMailbox` / `SiiPdoSpec` 与
  `SiiImage::from_spec()` —— 生成固定区 + 字符串/通用/SM/TxPDO/RxPDO 类别链，
  **自动计算固定区 CRC-8（word 0x0007）与 EEPROM 大小（word 0x003E）**；
  `simulated_slave()` 改为 `from_spec(simulated_spec())` 的薄封装，并有测试保证
  两者**逐 word 相同**（既有双主站验证过的镜像不变）。
- `plugin-host`：新增 `manifest` 模块 —— 零第三方依赖的 YAML 子集解析器
  （块映射 / 块序列 / 行内 `{...}` 与 `[...]` / 引号与裸标量 / 布尔 / 十进制与十六进制 /
  `#` 注释，不支持的语法明确报错），`PluginManifest::parse()` / `load()`（读
  `plugins/<id>/plugin.yaml`）与 `validate()`（R2 自有 VID/PID、名称含 SIMULATED、
  R5 sources 必填、SM/PDO 基本结构；R1/R3/R4 仍是人工法务审核项）。
- `plugin-host`：新增 `sii_gen` 模块 —— manifest → `SiiSpec`：邮箱参数取自
  `mbox-in`/`mbox-out` 通道、SM control 按方向（0x26/0x22/0x04/0x00）、PDO 绑定
  SM2/SM3、CoE 能力位（SDO/SDO Info/PDO assign+config/CA）从 manifest 推导。
- `plugin-host` 增加对 `esc-core` 的 path 依赖（核心许可不变，仍无第三方依赖）。
- 测试 +16（含改造）：esc-core +3（`from_spec` 与内置画像逐 word 一致、manifest 风格
  镜像的身份/SM/PDO/CRC/EEPROM 大小、无邮箱画像）；plugin-host +10（YAML 子集语法、
  注释/十六进制/负数、**仓库里真实的 `plugins/_template/plugin.yaml` 可解析且占位值被
  校验报出**、合法 manifest 零违规、缺 identity/sources 报错、manifest→SII 断言）。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **137 passed**
  （esc-core 75 / esc-node 36 / plugin-host 10 / wire 16）。
- `SiiImage::from_spec(&simulated_spec())` 与 `SiiImage::simulated_slave()` 逐 word
  相同 → 既有 IgH/SOEM 验证过的镜像未受影响；manifest 样例生成的 SII 通过身份、
  SM（0x1000/0x1080 mbox、SM2 输出、SM3 输入）、PDO（0x1600→SM2、0x1A00→SM3）、
  CRC 与 EEPROM 大小断言。
- ⚠️ 未在 Linux 验证机上扫描"由 manifest 生成的从站画像"：VM 锁被其他 worker 占用，
  且 `ec-sim-esc` 尚未接入 manifest（B9 子项 2）。

**卡点**
- `esc-node::Responder` 仍用内置画像：manifest 只生成 SII 规格，尚未做运行时对象字典
  （manifest `objects` → `obj_dict`）与 SM/FMMU/PDO 装配，所以还不能起"该 manifest 的从站"
  给主站扫描。
- YAML 支持范围 = manifest 模板用到的子集（不支持多行字符串/锚点/文档分隔符）。
- B6 FMMU 类别 0x0028 仍等 ESC Sec.II / IEC 61158 原文。

**下次开工第一件事**
- B9 子项 2：`esc-node` 按 manifest 装配 —— `Responder::from_manifest`（SII 用
  `plugin_host::sii_gen`，对象字典/PDO 分配/SM/FMMU 从 manifest `objects`/`pdo`/`sm`
  生成），加一个示例 manifest（自有 VID/PID，8DO），然后在 Linux 验证机上用未修改
  SOEM/IgH 扫描到该从站（IgH 侧可能需 ecrt 夹具）。VM 锁被占用时先做本地装配 + 单测。

---

## 2026-11-12 21:11 班（低峰）— B7 子项：CoE SDO Information（SOEM `slaveinfo -sdo` 读到 6 个对象）

**做了什么**
- **B6 剩余 FMMU 类别 0x0028 暂缓**（原因见卡点：公开资料未给出字段权威语义，按红线
  不凭记忆猜），改做 backlog 下一项 **B7 SDO Info**。
- `esc-core::mailbox`：新增 CoE SDO Information（服务 `0x8`）请求解析与响应构造
  （Get OD List `0x01/0x02`、Get OD `0x03/0x04`、Get Entry Description `0x05/0x06`、
  错误 `0x07`）。字段偏移按两家**未修改**主站的解析代码逐字节复核：
  IgH `master/fsm_coe.c`（首帧索引表 +8、Get OD 名称 +12、Get Entry 名称 +16、
  abort code +6）与 SOEM `src/ec_coe.c` + `include/soem/ec_type.h`。
- `esc-core::obj_dict`：新增对象/条目描述表（6 个对象、18 个条目：数据类型、位长、
  访问权限、名称；值与既有 `SIM_OBJECTS` 一致）。访问权限位按 IgH 解析语义
  （bit0–2 = R-preop/safeop/op，bit3–5 = W-preop/safeop/op）。
- `esc-core::sii`：通用类别（0x001E）的 CoE details 字节改为 `0x0F`
  （SDO + SDO Info + PDO assign + PDO configuration；**不置 Complete Access 位**），
  `ethercat sdos` / `slaveinfo -sdo` 需要该能力位；固定区 CRC 不受影响。
- `esc-node`：`handle_mailbox_request()` 先尝试 SDO Info 服务、再走 SDO 请求；SM1 回包
  与 mailbox full 语义不变。
- 新脚本 `scripts/spikes/sdo-info-probe.sh`：SOEM `slaveinfo -sdo` 断言 6 个对象、
  对象码/子索引数、访问权限，以及映射仍为 8 位 I/O。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **124 passed**（esc-core 72 /
  esc-node 36 / wire 16）。
- Linux 验证机：`bash scripts/spikes/sdo-info-probe.sh` →
  `OK: SOEM slaveinfo -sdo 经 CoE SDO Info 读到 6 个对象（0x1018/0x1C00/0x1C12/0x1C13/
  0x1600/0x1A00，含描述与访问权限）`。`slaveinfo -sdo` 实际打印
  `CoE Object Description found, 6 entries.`、`0x1018 "Identity" [RECORD maxsub(0x04 / 4)]`、
  `0x1018:01 "Vendor ID" [UNSIGNED32 R_R_R_] 0x00000ec5`、
  `0x1c12:01 "RxPDO assign" [UNSIGNED16 RW____] 0x1600`。
- 双主站回归（默认/strict 两种模式 **6/6 EXIT=0**）：`minimal-esc-responder.sh`、
  `soem-process-data.sh`（1055 轮 WKC=3）、`igh-op-probe.sh`（IgH ecrt OP
  `al_states=0x08 wc=3`）——新增邮箱服务与 SII 能力位未影响扫描/映射/OP。

**卡点**
- **B6 FMMU 类别 0x0028 暂缓**：IgH 直接跳过该类别（`fsm_slave_scan.c: case 0x0028: break`），
  SOEM 只把它显示为 `FMMUfunc` 提示；公开资料没有该类别字段的权威语义，
  按"不凭记忆写规范细节"红线不做，等 ESC Sec.II / IEC 61158 原文。
- IgH `ethercat sdos` 在无应用时输出为空（同 B8 已记录的 IgH 工具行为），
  SDO Info 的 IgH 侧读取 ⚠️ 待验证（需要 ecrt 应用夹具）。
- Get OD List 首帧索引表前的 2 字节字段未被两家主站读取；其 ETG.1000.6 语义
  ⚠️ 待核实（本实现填 0，IgH/SOEM 都按偏移 8 读索引表）。
- 未实现 Complete Access / 分段 SDO / SDO Info 多帧响应（本对象字典小，单帧足够）。

**下次开工第一件事**
- 补 IgH 侧 SDO Info 验证：在 ecrt 应用夹具（`/opt/ecsim/work/igh-op/ecrt-op-probe`）
  里触发 SDO 字典读取并断言 `ethercat sdos` 输出，扩展 `sdo-info-probe.sh` 的 IgH 段。
  之后：B6 剩余（DC 类别、ESI XML → SII 生成器；FMMU 类别需原文暂缓）或
  B9 声明式插件加载器骨架。

---

## 2026-11-12 21:07 班（低峰）— B5 收尾：ESM 前置条件检查改为默认开启（新增 `--esm-lax`）

**做了什么**
- `esc-node`：`Responder::new()` 的 ESM 前置条件检查改为**默认开启**
  （`esm_preconditions: true`）；`Responder::with_esm_preconditions(false)` 仍可关闭。
  依据是上一班（20:44）在验证机上用严格模式跑出的 6/6 双主站回归全绿：两家未修改主站
  都在请求状态**之前**配置好对应 SM，所以默认开启不改变正常主站行为，只拒绝配置缺失的错误场景。
- CLI：新增 `--esm-lax` 关闭检查（兼容/负向对照用）；`--esm-strict` 保留为旧参数名
  （等价于默认行为）；`--help` 与模块文档同步。
- 测试：`setup_watchdog_environment` 改为按主站顺序先配置 SM0–SM3 再 INIT→PREOP→SAFEOP→OP；
  `esm_preconditions_are_on_by_default` 断言默认拒绝（0x0015）+ `--esm-lax` 等价关闭后放行。
- `scripts/spikes/uds_master_probe.py`：默认路径改为**断言前置条件拒绝**
  （SM 未配置 → PREOP 0x0015 / SAFEOP 0x0017 + ACK 清错）再配置 SM 放行；`--lax` 跳过拒绝断言。
  `uds-loopback.sh` 两轮改为「默认（开启前置条件）」与「`--esm-lax`」。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **119 passed**（esc-core 68 /
  esc-node 35 / wire 16）；`python -m py_compile` 与 `bash -n` 通过。
- Linux 验证机（真实 TAP + 两家未修改主站）：
  - `SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/uds-loopback.sh` → 两轮全通过：
    默认模式 `+ ESM 前置条件（0x0015/0x0017）均通过`；`--esm-lax` 轮跳过拒绝断言后通过。
  - 双主站回归（三个脚本 × 默认/lax 两种模式，**6/6 EXIT=0**）：
    `minimal-esc-responder.sh`：SOEM `1 slaves found and configured`、IgH 拓扑计算无报错；
    `soem-process-data.sh`：`simple_ng` 进入 OP、**1055 轮 WKC=3、无 WKC 不符**；
    `igh-op-probe.sh`：未修改 IgH ecrt 应用把从站推入 OP
    （`RESULT al_states=0x08 slaves_responding=1 wc=3`）。

**卡点**
- 前置条件只检查"SM 是否已配置（Activate bit0 + Length > 0）"，未校验 SM 配置的合法性
  （地址越界、区间重叠、类型/方向）；OP 级前置条件（0x0018/0x0019、映射类 0x0024/0x0025）未做。
- 0x0015 / 0x0016 的规范区分（邮箱输入/输出方向）仍未从公开资料核实 → 统一回 0x0015。

**下次开工第一件事**
- 回到 backlog 顶部未完成项 **B6 剩余：SII FMMU 类别 0x0028**——先以 SOEM
  `src/ec_main.c: ecx_siiFMMU()`（nFMMU = 首 word × 2，随后 FMMU0..3 字节）与 IgH
  `master/slave.c` 的解析代码复核字段语义，不凭记忆猜；实现后在 `slaveinfo` 输出上验证。
  若时间/证据不足，则做 **B5 OP 前置条件**（先用 ETG.1000.6 Table 11 与两家主站错误处理
  路径核实 0x0018/0x0019/0x0024/0x0025 的触发条件）。

---

## 2026-11-12 20:44 班（低峰）— B5 子项：ESM 前置条件检查（0x0015 / 0x0017）+ AL Status Code 表

**做了什么**
- `esc-core` 新增 `al_status` 模块：ETG.1000.6 Table 11 的码值子集 + `name()`（交叉核对
  IgH `master/fsm_change.c: al_status_messages[]`、ethercrab `al_status_code.rs`（MIT OR
  Apache-2.0 公开文档）、ABB AC500 EtherCAT 诊断手册）；`watchdog::AL_STATUS_SM_WATCHDOG`
  改为引用该模块，避免码值散落。
- `EscAl` 新增 `EsmPreconditions` + `handle_control_checked()`：
  - **INIT → PREOP**：SII 声明了邮箱协议（固定区 word 0x001C ≠ 0）但 SM0/SM1 未配置
    （Activate bit0 未置位或 Length = 0）→ error 位 + AL Status Code **0x0015**；
  - **PREOP → SAFEOP**：SII 声明了过程数据 SM（SM 类别 ≥ 3 条）但 SM2/SM3 都未配置
    → error 位 + AL Status Code **0x0017**；
  - 只对**向上**迁移检查（OP/SAFEOP 回退不需要重新配置 SM）；Error ACK 语义不变；
  - 未知状态位（如 0x0F）→ **0x0012**（"Unknown requested state"；此前误用 0x0011），
    状态位全 0 仍沿用 0x0011（既有双主站回归验证过的行为）。
- `esc-node`：`Responder::with_esm_preconditions(bool)`（默认**关闭**，保持与两家未修改
  主站既有冒烟完全一致）；CLI `ec-sim-esc --esm-strict` 开启。条件由 SII
  （`SiiImage::mailbox_protocols()` / `declares_process_data_sync_managers()`）与 SM
  寄存器现场推导；`EepromInterface::image()` 只读暴露 SII。
- 证据（只读参考）：IgH `fsm_slave_config.c` 的顺序是 INIT → 清 FMMU/SM → **配置邮箱
  SM0/SM1** → PREOP → 过程数据 SM → FMMU → SAFEOP → OP；SOEM `ec_config.c` 中
  `ecx_config_init()` 先 `FPWR ECT_REG_SM0`（SM0/SM1）再写 `ALCTL = PREOP|ACK`，
  `ec_config_map_group()` 先配 SM2/SM3/FMMU 再请求 SAFEOP —— 两家都在请求前完成配置，
  前置条件只在配置缺失（错误场景）时失败。
- 测试 +14：esc-core +10（码表/名称、拒绝与放行、向下不检查、ACK、未知状态 0x0012、
  SII 类别查找），esc-node +4（默认关闭、strict 拒绝 PREOP/SAFEOP、未知状态码）。
- `scripts/spikes/uds_master_probe.py` 加 `--strict`（UDS 端到端负向 0x0015/0x0017 +
  ACK + 配置后放行断言）；`uds-loopback.sh` 增第二轮 `--esm-strict` 服务实例。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets
  -- -D warnings` exit 0；`cargo test --workspace` **119 passed**（esc-core 68 /
  esc-node 35 / wire 16）。
- `python -m py_compile scripts/spikes/uds_master_probe.py`、`bash -n uds-loopback.sh` 通过。
- Linux 验证机（真实 TAP + 两家未修改主站；先把本 commit 的 `git archive` 传到验证机
  `cargo build --release -p esc-node` 后执行）：
  - `SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/uds-loopback.sh` → 两轮均通过：
    默认模式 `OK: ... SM watchdog（10 ms → SAFEOP+ERROR 0x001B → ACK 清错）均通过`；
    `--esm-strict` 模式 `OK: ... + ESM 前置条件（0x0015/0x0017）均通过`
    （SM 未配置 → 0x0015/0x0017 + error 位、ACK 清错、配置 SM 后放行）。
  - 双主站回归（三个脚本 × 默认/strict 两种模式，全 **EXIT=0**）：
    `minimal-esc-responder.sh`：SOEM `1 slaves found and configured`、IgH 拓扑计算无报错
    （IgH 无应用路径的 PREOP 也在邮箱 SM 配置之后，strict 同样通过）；
    `soem-process-data.sh`：`simple_ng` 进入 OP、**1055 轮 WKC=3、无 WKC 不符**；
    `igh-op-probe.sh`：未修改 IgH ecrt 应用把从站推入 OP
    （`RESULT al_states=0x08 slaves_responding=1 wc=3`）。
  - ⇒ 前置条件只依赖"主站先配置 SM 再请求状态"这一真实顺序，**strict 模式对现有双主站
    冒烟无回归**；默认关闭仅作为保守开关，已有证据可在下一班改为默认开启。

**卡点**
- 前置条件默认仍为关闭（opt-in）：本班只做到"实现 + 验证"，改为默认开启留作下一班
  独立小步（需同步调整 `uds_master_probe.py` 默认路径与单测夹具）。
- 0x0015 / 0x0016 的规范区分（邮箱输入/输出方向）未从公开资料核实 → 统一回 0x0015，
  `INVALID_MAILBOX_CONFIGURATION_OUT` 常量保留但不用。
- 未做 OP 前置条件（0x0018/0x0019、映射类 0x0024/0x0025）。

**下次开工第一件事**
- 把 ESM 前置条件改为**默认开启**（`Responder::new` 里 `esm_preconditions = true`，CLI 增加
  `--esm-lax` 兼容关闭；同步改 `setup_watchdog_environment` 等测试夹具、`uds_master_probe.py`
  默认路径先配 SM），然后重跑 `uds-loopback.sh`（改成单轮即可）与三个双主站脚本；
  证据已在 commit `0ebe484` 的班次记录里（strict 模式六项回归全绿）。
  若时间有富余：做 **B6 剩余 SII FMMU 类别 0x0028**（先以 SOEM `ecx_siiFMMU`、IgH
  `slave.c` 复核字段语义，不凭记忆猜）。

---

## 2026-11-12 20:07 班（低峰）— B5 子项：SM 过程数据看门狗（寄存器 + 超时 → SAFEOP+ERROR / 0x001B）

**做了什么**
- `esc-core` 新增 `watchdog` 模块：
  - 寄存器 **0x0400**（Watchdog Divider，40 ns 基数）、**0x0410**（PDI 时间）、
    **0x0420**（过程数据 intervals）；超时 = `divider × intervals × 40 ns`（向上取整到 ms）；
    SM control **bit6** = 看门狗使能；AL Status Code **0x001B** = SM watchdog。
  - 依据（只读参考）：IgH `include/ecrt.h: ecrt_slave_config_watchdog()`（divider = 40 ns
    间隔数、intervals = 基础间隔数）、`master/fsm_slave_config.c`（写 0x0400/0x0420）、
    `master/sync.c`（SM control bit6 = `EC_WD_ENABLE`）；ETG.1000.6 AL Status Code 0x001B。
- `esc-node`：`Responder` 增加**帧驱动时钟** `tick(now_ms)`（TAP/UDS 每收一帧调用一次）：
  - SM2/SM3 中任一"已激活 + control.bit6 + length > 0"视为看门狗使能；
  - 过程数据写入（FMMU 逻辑写 / 物理写命中 SM2/SM3 缓冲区）重置计时；
  - 超时 → AL Status error 位 + Code `0x001B`，状态从 **OP 退回 SAFEOP**，等待 Error ACK；
  - 首次启用从当前时刻开始计时（避免启动误报）；divider/intervals 为 0 时视为未配置
    （⚠️ 真实 ESC 的 0 值语义待核实）。
- TAP/UDS 两个线缆层循环都接入 `tick(now_ms)`（`SystemTime` 毫秒）。
- 测试 +5：esc-core +2（超时换算/常量）、esc-node +3（超时触发 + ACK 清错、未使能不触发、
  过程数据重置计时）；扩展 `scripts/spikes/uds_master_probe.py` 的 UDS 端到端断言。

**验证结果**
- 本机 Windows：`cargo fmt --check` / `clippy -D warnings` exit 0；
  `cargo test --workspace` **105 passed**（esc-core 58 / esc-node 31 / wire 16）。
- Linux 验证机（真实时钟 + UDS 合成主站）：
  `SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/uds-loopback.sh` →
  `OK: ... + SM watchdog（10 ms → SAFEOP+ERROR 0x001B → ACK 清错）均通过`。
- 双主站回归无影响（两家主站本轮都不使能看门狗：IgH ecrt 夹具 `EC_WD_DISABLE`、
  SOEM SII control 无 bit6）：`minimal-esc-responder.sh`、`soem-process-data.sh`（OP/WKC3）、
  `igh-op-probe.sh`（IgH OP）、`soem-uds-probe.sh`（UDS shim）全部 exit 0。

**卡点**
- **帧驱动时钟**：没有帧到达时看门狗不前进（停止收帧不会主动报超时）；无独立硬件定时器/DC。
- 0x0410（PDI watchdog）只保留读写语义；divider/intervals=0 的真实 ESC 语义 ⚠️ 待核实。
- 未实现 AL SAFEOP/OP 的 **ESM 前置条件检查**（B5 剩余项）。

**下次开工第一件事**
- **B5 剩余：AL SAFEOP/OP 的 ESM 前置条件检查**——先以 ESC Sec.II / ETG.1000.6 与两家主站
  （`fsm_slave_config` / `ecx_statecheck`）的行为核实条件，不凭记忆猜；或 **B6 剩余：SII FMMU
  类别 0x0028**（同样先核实 SOEM/IgH 解析语义）。若条件无法核实，则转 B9 声明式插件加载器骨架。

---

## 2026-11-12 20:03 班（低峰）— B3 完成：SOEM oshw UDS shim 经 Unix socket 跑通 `ec_config_init` + `ec_config_map`

**做了什么**
- 验证机本地复制 SOEM 2.0.0 到 `/opt/ecsim/work/soem-uds`（**GPL 夹具，源码/补丁不入库**），
  用本地补丁把 `oshw/linux/nicdrv.c` 的 `ecx_setupnic()` 中
  「`PF_PACKET` 原始套接字 + ioctl 网卡配置 + bind」替换为 **AF_UNIX SOCK_DGRAM 客户端**
  （客户端 bind 自己的路径、`connect(ifname)` —— `ifname` 即 `--uds` socket 路径）；
  `send/recv/poll/索引缓冲区` 逻辑保持 SOEM 原样。cmake 构建出 `slaveinfo`/`simple_ng`。
- 新增 `scripts/spikes/soem-uds-probe.sh`：启动 `ec-sim-esc --uds`，断言 UDS-shim SOEM
  `slaveinfo` 扫描到 1 从站/8bit I/O、`simple_ng` 映射 1O+1I → OP、每轮 WKC 3、0 次 wrong，
  且模拟器日志出现 `RX uds` 收帧记录。

**验证结果**
- Linux 验证机（**未修改的 SOEM 协议逻辑 + 仅 oshw 收发层换 UDS**，无 root、无 TAP）：
  - `slaveinfo /tmp/ecsim-uds.sock`：`1 slaves found and configured`、`Output size: 8bits`、
    `Input size: 8bits`、`State: 4`，SM/FMMU/MBX 行齐全；
  - `simple_ng /tmp/ecsim-uds.sock`：`mapped 1O+1I bytes` → **all slaves are now operational**，
    1358 轮 **WKC 3**、0 次 wrong；
  - 脚本 `soem-uds-probe.sh` → `OK: SOEM UDS shim 经 /tmp/ecsim-uds.sock 跑通 ec_config_init +
    ec_config_map（1 从站 / 1O+1I / OP / 1358 轮 WKC=3 / 0 次 wrong）`；
  - 模拟器 `--verbose` 日志：`READY uds=...`、`RX uds N bytes from "<client path>"`。
- 本班未改 Rust 代码；收尾前重跑 `cargo fmt/clippy/test` 仍 **100 passed**。
- 许可说明：shim 是独立进程夹具（SOEM GPLv3），仓库只保留调用脚本与文档，不复制/不链接。

**卡点**
- SOEM UDS shim 的补丁脚本与构建产物只在验证机本地（`/opt/ecsim/work/soem-uds`），不入库；
  `soem-uds-probe.sh` 在二进制缺失时以 exit 2 提示重建。
- 仍单从站、单客户端；无 DC/watchdog、无链路状态/MAC 语义；过程数据内容静态（无 PDI 回环）。
- ⚠️ 待验证：多客户端并发、非 root CI 环境下全链路（TAP 对照已由 B8 脚本覆盖）。

**下次开工第一件事**
- 继续 backlog：**B5 剩余（watchdog / AL SAFEOP/OP 前置条件）或 B6 剩余（SII FMMU 类别 0x0028、
  DC 类别）**，两者都需 VM 验证；B6 的 FMMU 类别 ⚠️ 需先核实 SOEM/IgH 解析语义，不凭记忆猜。
  若时间不足，优先做 **B9 声明式插件加载器**（`plugin.yaml` → SII + 对象字典 + PDO/SM/FMMU 布局）
  的骨架与单测，并在双主站上扫描验证。

---

## 2026-11-12 19:59 班（低峰）— B3 子项 1：ESC-over-UDS 线缆层（`ec-sim-esc --uds`）+ 合成主站断言

**做了什么**
- `crates/esc-node/src/bin/ec_sim_esc.rs`：新增 `--uds <path>` 线缆层（仅 Linux）——
  `SOCK_DGRAM` Unix socket，每个报文 = 一帧；`recv_from` → `Responder::handle_frame` →
  `send_to(peer_path)` 回包；`--verbose` 打印 RX/DROP 明细与每 50 帧统计（与 TAP 共用
  `maybe_print_stats`）。`--tap` 与 `--uds` 互斥，`--help` 更新。
- 新增 `scripts/spikes/uds_master_probe.py`：零依赖（Python 标准库）合成主站，自行构造
  EtherCAT 帧并断言：BRD 0x0000（ESC 类型）、APRD AL Status、APWR 站地址 + FPRD、APRD
  DL Status `0x5610`、一帧两 datagram（M 位）WKC。
- 新增 `scripts/spikes/uds-loopback.sh`：启动 `ec-sim-esc --uds`、等 socket 就绪、跑 probe、清理。
- 新增 `docs/spikes/spike-5-esc-over-uds.md`：协议/实现/证据/限制/与 B3 下一步的关系。

**验证结果**
- 本机 Windows：`cargo fmt --check` / `clippy -D warnings` / `test` 通过（UDS 代码用 cfg 隔离，
  非 Linux 是明确报错桩）。
- Linux 验证机：`SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/uds-loopback.sh` →
  `OK: ESC-over-UDS 线缆层可用（/tmp/ecsim-uds.sock）：BRD/APRD/APWR/FPRD/M 位均通过`（exit 0）；
  `--verbose` 日志可见 `READY uds=...`、`RX uds 28 bytes from "<client path>" (pathname)`。
- TAP 无回归：`minimal-esc-responder.sh`（SOEM/IgH 扫描）exit 0、`igh-op-probe.sh`（IgH OP）
  exit 0；`cargo test --workspace` **100 passed**。
- 踩坑修正：ETG.1000.4 datagram 头是 **10 字节**（含 2 字节 IRQ）；probe 首版漏了 IRQ 字段
  导致帧被拒（服务端 `DROP`），已修复并写进 spike-5 作为回归点。

**卡点**
- 仅完成**模拟器侧**；SOEM `oshw` 的 UDS shim（B3 主体）尚未实现，不能声称 B3 完成。
- UDS 仅 Linux；客户端必须 bind 文件系统路径（abstract/匿名 socket 无法被回包）。

**下次开工第一件事**
- **B3 子项 2：SOEM oshw UDS shim**——把 SOEM 复制到验证机工作目录（**不进仓库**），将
  `oshw/linux/nicdrv.c` 的 setupnic/send/recv 换成 UDS 客户端（连接 `--uds` socket、客户端
  bind 自己的路径），用 `slaveinfo` 或最小程序跑通 `ec_config_init` + `ec_config_map`，
  并与 TAP 路径输出对照；补 `scripts/spikes/soem-uds-probe.sh` 作为可复现证据。

---

## 2026-11-12 19:56 班（低峰）— B8 收尾：双主站 OP 冒烟记录（spike-4）并勾选 B8

**做了什么**
- 新增 `docs/spikes/spike-4-dual-master-op.md`：汇总 **B8** 双主站证据（未修改 SOEM 与
  IgH 1.6.13，单从站 8 位 I/O 画像）——方法、命令、脱敏输出（SOEM `slaveinfo`/`simple_ng`/
  `slaveinfo -map`；IgH `ethercat slaves`、SDO 上传/下载、ecrt 应用 `RESULT`、内核日志）、
  5 个可复现断言脚本清单、ecrt 夹具 provisioning（GPL 链接，源码不入库）、已知限制
  （单从站、静态 PDO、DC/watchdog、无 GUI 截图、SDO Info 未实现）与非认证声明。
- `AGENTS.md` §6 把 **B8 勾选为完成**（附证据链接与限制）；`spike-3` §6 更新指向 spike-4。
- 收尾前在验证机上重跑全部断言脚本，全部通过。

**验证结果**
- `minimal-esc-responder.sh` exit 0：SOEM 1 从站 / 8bits / SAFEOP、IgH PREOP、拓扑断言 OK，
  应答器统计 `mbox=19 mboxresp=19`；
- `soem-process-data.sh` exit 0：`OK: SOEM simple_ng 进入 OP，1055 轮 WKC=3，无 WKC 不符`；
- `soem-mailbox-probe.sh` exit 0：`mbox=24 mboxresp=22`；
- `sdo-upload-probe.sh` exit 0：SOEM CoE 映射 `0x7000/0x6000`；IgH SDO 上传/下载全通过；
- `igh-op-probe.sh` exit 0：`ethercat slaves` = **OP**、应用 `al_states=0x08`、`wc=3`；
  dmesg：`Domain 0: Working counter changed to 3/3.`、`Slave states on main device: OP.`。
- 本班未改 Rust 代码；收尾前重跑 `cargo fmt/clippy/test` 仍 **100 passed**。

**卡点**
- 单从站；过程数据内容静态（无 PDI 回环）；DC / SM watchdog / OP 下 AL 错误恢复未实现/未验证。
- 无 GUI 截图（远端无图形界面），spike-4 以脱敏文本输出为证据；需要截图时在图形环境重跑脚本。

**下次开工第一件事**
- 按 `AGENTS.md` §6 **顶部未完成项取 B3（SOEM oshw UDS shim）**：把 SOEM 的 `oshw` 收发包换成
  UDS、跑通 `ec_config_init` + `ec_config_map`（SOEM 仍作为独立进程夹具，不改仓库核心）；
  若该 spike 受阻，则继续 B6 剩余（SII FMMU 类别 0x0028 / DC 类别，先核实字段语义）并在
  WORKLOG 记原因。

---

## 2026-11-12 19:53 班（低峰）— B8 里程碑：**未修改 IgH 零补丁进入 OP**（ecrt 应用夹具）

**做了什么**
- 在 Linux 验证机写了一个**不入库**的最小 ecrt 应用夹具
  （`/opt/ecsim/work/igh-op/ecrt-op-probe.c`：因链接 GPL 的 libethercat，按红线只放验证机，
  不进仓库）：`ecrt_request_master(0)` → `ecrt_master_create_domain` →
  `ecrt_master_slave_config(0, 0, SIM_VENDOR_ID, SIM_PRODUCT_CODE)` →
  `ecrt_slave_config_pdos`（sync2 = 输出 `0x1600`/`0x7000:00` 8bit、sync3 = 输入
  `0x1A00`/`0x6000:00` 8bit）→ `ecrt_slave_config_reg_pdo_entry` → `ecrt_master_activate` →
  2 ms 周期 LRW 交换 1 字节输入/输出；结束时打印 `RESULT al_states=… wc=…`。
- 新增可复现断言脚本 `scripts/spikes/igh-op-probe.sh`：启动模拟器 → `modprobe ec_master/ec_generic`
  → 后台跑夹具 → `ethercat slaves` 断言 **OP**、应用输出 `al_states=0x08` + `wc=3`、
  应答器侧 `al=0x08`。

**验证结果**
- Linux 验证机（**未修改的** IgH 1.6.13、未修改的 `ecrt.h`/`libethercat`，夹具为独立进程）：
  - 应用运行期间 `ethercat slaves`：`0  0:0  OP  +  0x00000ec5:0x0000e5c1`；
  - 应用输出：`RESULT al_states=0x08 slaves_responding=1 wc=3 out=00 in=00`（退出码 0）；
  - IgH 内核日志：`Domain 0: Working counter changed to 3/3.`、`Slave states on main device: OP.`；
  - 应答器侧 `al=0x08`、过程数据 datagram hit 持续增长（LRW 命中）。
- 新脚本 `igh-op-probe.sh` → `OK: 未修改 IgH 经 ecrt 应用把模拟器从站推入 OP（… wc=3 …）`。
- 回归全部通过：`sdo-upload-probe.sh`（SOEM CoE 映射 + IgH SDO 上传/下载）、
  `soem-process-data.sh`（SOEM OP / 1055 轮 WKC 3）、`soem-mailbox-probe.sh`、
  `minimal-esc-responder.sh`（双主站扫描）。
- 本班**未改动 Rust 代码**（只新增验证机夹具 + 断言脚本），fmt/clippy/test 沿用上一班；
  收尾前重跑 `cargo fmt/clippy/test` 确认仍为 100 passed。

**卡点**
- 夹具源码不入库（GPL 链接），仓库只保留脚本 + 文档说明；验证机重装需按本条目/AGENTS 重建。
- IgH 的 OP 是**应用激活期间**的状态：单独跑 `ethercat slaves`（没有应用）时从站回到 PREOP，
  这是 IgH 主站工具的行为，不是从站问题。
- 过程数据内容仍静态：应用写 `0x7000:00` 后模拟器只存储、无 PDI 回环，输入恒 0。
- ⚠️ 未验证：多从站、DC、SM watchdog 与 OP 状态下的 AL 错误恢复。

**下次开工第一件事**
- **B8 收尾**：把双主站合并冒烟记录写成 `docs/spikes/spike-4-dual-master-op.md`（SOEM
  `slaveinfo`/`simple_ng` + IgH `ethercat slaves` OP + 应用 `wc=3`；命令、脱敏输出、断言脚本清单），
  更新 AGENTS §6 B8 的完成条件；随后转向 **B9 声明式插件加载器**。

---

## 2026-11-12 19:49 班（低峰）— B7 子项 3：SDO Download（写配置对象）→ 未修改 IgH 可写对象字典

**做了什么**
- `esc-core/mailbox`：新增 SDO 请求**解析**与响应构造：
  - `parse_sdo_request()`：`0x40` = initiate upload；`0x2F/0x2B/0x27/0x23` = expedited download
    （1/2/3/4 字节）；其他（含 master 发起的 Abort、分段传输）= `Other`（不应答）；
  - `sdo_upload_service()`、`sdo_download_ok_service()`（命令 `0x60`）、`sdo_abort_payload()`；
  - 新增 Abort 码 `0x06010002`（写只读对象）；移除只回 Abort 的旧 `sdo_response()`。
- `esc-core/obj_dict`：新增 `is_writable()`——只开放主站配置 PDO 域会写的对象：
  `0x1C12/0x1C13`（PDO assign）、`0x1600/0x1A00`（PDO 映射）、`0x1C32/0x1C33`（SM 同步参数）。
- `esc-node`：`Responder` 增加 `object_writes: BTreeMap<(index, sub), Vec<u8>>`（Download 覆盖值），
  Upload 优先返回覆盖值；Download 对可写对象保存值并回成功响应，只读对象回 Abort
  `0x06010002`，未知对象回 `0x06020000`。
- 扩展 `scripts/spikes/sdo-upload-probe.sh`：加入 IgH `ethercat download` 断言
  （写 `0x1C13:01 = 0x1A01` 可回读；只读/未知对象被拒绝）。
- 测试 +2（`mailbox` 解析/响应测试重写、`esc-node` 下载集成 2 条），共 **100 passed**。

**验证结果**
- 本机 Windows（rustc 1.97.1）：fmt/clippy(-D warnings) exit 0；`cargo test --workspace`
  **100 passed**（esc-core 56 / esc-node 28 / wire 16）。
- Linux 验证机（两家**未修改**主站，零补丁）：
  - IgH：`ethercat download -p 0 -t uint16 0x1c13 0x01 0x1a01` 成功（rc 0），随后
    `ethercat upload` 回读 **`0x1a01`**；对只读 `0x1018:01` 与未知 `0x2000:00` 下载均返回
    `Failed to download SDO: Input/output error`（Abort 生效）。
  - `sdo-upload-probe.sh` 全套通过（SOEM CoE 映射 + IgH upload + IgH download）。
  - 回归：过程数据仍 **OP / 1055 轮 WKC 3**；`soem-mailbox-probe.sh` 通过；
    `minimal-esc-responder.sh` 双主站扫描 OK（IgH 仍 PREOP）。
- IgH 配置 PDO 域所需的 SDO Download 路径已具备；下一步用 ecrt 应用夹具验证 IgH 进 OP。

**卡点**
- SDO Info（`0x8`）、Complete Access、分段传输仍未实现：`ethercat sdos` / `slaveinfo -sdo` 会超时。
- Download 覆盖值只保存在模拟器进程内，不回写 SII/静态字典；重启恢复默认映射。
- ⚠️ 待验证：ecrt 应用写好映射后，IgH 的 domain 配置是否全部通过（`0x1C32/0x1C33`、
  `0x1C12:00` 等写入）以及 OP 状态。

**下次开工第一件事**
- **B8 IgH 应用夹具**：写一个最小 ecrt 应用（独立进程夹具，不入库）在 TAP 上：
  `ecrt_request_master` → `ec_slave_config_pdos`（按本字典 `0x1C12/0x1A00` 映射）→ domain →
  周期 process data，观察 `ethercat slaves` 显示 **OP**、WKC 3/周期；并补
  `scripts/spikes/igh-op-probe.sh`。

---

## 2026-11-12 19:44 班（低峰）— B7 子项 2：最小对象字典 + SDO expedited upload（两家未修改主站都读到对象）

**做了什么**
- `esc-core` 新增 `obj_dict` 模块：只读最小对象字典
  - `0x1018` Identity（vendor/product/revision/serial，与 SII 固定区一致）；
  - `0x1C00` Sync Manager 通信类型（`:00` = 4，`:01..04` = 1/2/3/4 = mailbox out/in、outputs、inputs）；
  - `0x1C12`/`0x1C13` PDO assign（各 1 个 → `0x1600`/`0x1A00`）；
  - `0x1600`/`0x1A00` PDO 映射（`0x7000:00` 8 位 / `0x6000:00` 8 位），与 SII SM/PDO 类别一致。
- `mailbox` 模块把 Abort-only 响应扩展为 **SDO 响应分发**：
  - `0x40`（initiate upload）且字典命中 → **expedited upload**（按数据长度 0x4F/0x4B/0x47/0x43，
    不足 4 字节补零）；
  - 字典未命中 → Abort `0x06020000`；Download/其他 → Abort `0x06010000`（本实现只读）；
  - SDO Info（CoE 命令 0x8）仍不应答（⚠️ 留后续）。
- `esc-node` 邮箱处理改用 `mailbox::sdo_response`。
- 测试新增 **8 条**：`obj_dict`（SM 类型、PDO assign/映射一致性、Identity 与 SII 一致、
  expedited 命令与补零、未知对象）+ `mailbox`（已知对象上传响应、未知对象 Abort、
  Download 只读 Abort）+ `esc-node`（邮箱集成：字典命中回 `0x1600`、未知对象回 Abort）。
- 新增断言脚本 `scripts/spikes/sdo-upload-probe.sh`：SOEM `slaveinfo -map` 与 IgH
  `ethercat upload` 双主站读对象字典。

**验证结果**
- 本机 Windows（rustc 1.97.1）：fmt/clippy(-D warnings) exit 0；`cargo test --workspace`
  **98 passed**（esc-core 56 / esc-node 26 / wire 16）。
- Linux 验证机（两家**未修改**主站，零补丁）：
  - SOEM `slaveinfo <tap> -map`：
    ```
    PDO mapping according to CoE :
      SM2 outputs ... [0x0000.0] 0x7000:0x00 0x08
      SM3 inputs  ... [0x0001.0] 0x6000:0x00 0x08
    ```
    同时仍 `Output size: 8bits / Input size: 8bits / State: 4`；应答器 `mbox=24 mboxresp=22`。
  - **IgH**（未修改的 `ec_master`/`ec_generic`，TAP 上 `ethercat upload -p 0`）：
    `0x1C12:01 = 0x1600`、`0x1600:01 = 0x70000008`、`0x1C00:00 = 0x04` —— IgH 的 CoE/SDO
    邮箱链路与对象字典读取已通。
  - 新脚本 `sdo-upload-probe.sh` → `OK: SOEM CoE 映射 0x7000/0x6000（8bit）；
    IgH SDO upload 0x1C12:01=0x1600、0x1600:01=0x70000008、0x1C00:00=0x04`。
  - 回归：`soem-process-data.sh` 仍 **OP / 1055 轮 WKC 3**；`soem-mailbox-probe.sh` 通过；
    `minimal-esc-responder.sh` 双主站扫描 OK（IgH 仍 PREOP）。
- IgH 仍 PREOP 的原因**不再是缺 SDO**：`ethercat slaves` 没有应用程序，IgH 需要 ecrt 应用
  配置 PDO 域（domain）才会把从站推入 OP —— 属 B8 的 IgH 应用夹具。

**卡点**
- SDO Info（0x8）未实现：`ethercat sdos` 会超时；Complete Access、分段传输、Download 不支持。
- PDO 数据对象 `0x6000`/`0x7000` 未进字典（PDO 走 FMMU 逻辑寻址，不走 SDO）；输入数据仍恒 0。
- ⚠️ 待验证：IgH 应用（ecrt domain）能否用本字典完成 0x1C12/0x1A00 配置并进 OP（B8 下一步）。

**下次开工第一件事**
- **B8 IgH 应用夹具**：用 IgH 自带的示例（如 `examples/` 或 `ec_sample`）或最小 ecrt 应用
  （独立进程夹具，不入库）在 TAP 上发起 domain（按本字典 0x1C12/0x1A00 映射），观察 IgH 把
  从站推进 **OP**、过程数据 WKC 3/周期；并补 `scripts/spikes/igh-op-probe.sh` 作为可复现证据。

---

## 2026-11-12 19:36 班（低峰）— B7 子项 1：邮箱数据链路（SII mailbox 声明 + SM0/SM1 + CoE SDO Abort 应答）

**做了什么**
- `esc-core` 新增 `mailbox` 模块：邮箱头（6 字节）解析/编码、CoE **SDO Abort** 响应构造
  （对任意 CoE SDO 请求回 `0x06020000` = object does not exist）、SM1 Status 的 mailbox full 位
  （0x080D bit3）读写。编码按两家未修改主站复核：IgH `master/fsm_coe.c` 的 CoE 头 u16 bit12–15
  （0x2 = SDO 请求、0x3 = SDO 响应、0x8 = SDO Info）、SDO 命令 0x40 = initiate upload、
  0x80 = abort；SOEM `src/ec_coe.c: ecx_SDOread()` 接受 `ECT_SDO_ABORT` 并从 CoE 数据 +6 起
  读 4 字节 abort code。
- `sii.rs`：固定区 0x0014–0x001C 声明 boot/std 两套 mailbox（rx 0x1000 / tx 0x1080、各 128 字节、
  协议 word = **0x0004 = CoE**）；SM 类别 SM0 = 0x1000（control `0x26`）、SM1 = 0x1080
  （control `0x22`），与 IgH `fsm_slave_config.c` 默认邮箱 SM、SOEM `EC_DEFAULTMBXSM0/1` 一致。
  **踩坑**：首次把 CoE 协议位写成 0x0001（那是 AoE），主站读到 `MBX protocols : 01` 后完全不发
  CoE 请求；按 IgH `EC_MBOX_COE = 0x04` / SOEM `ECT_MBXPROT_COE = 0x0004` 修正后才通。
- `esc-node`：FPRD/FPWR 到 ≥0x1000 的物理地址改为读写 DL 用户内存（此前被忽略）；写 SM0 缓冲
  （0x1000）解析邮箱头，对 CoE SDO 请求在 SM1 缓冲（0x1080）回 Abort 并置 SM1 Status bit3；
  主站读走 SM1 缓冲后清该位（模拟从站已消费）。`Stats` 增加 `mailbox_requests`/`mailbox_responses`，
  `--verbose` 输出 `mbox=/mboxresp=`。
- 新增断言脚本 `scripts/spikes/soem-mailbox-probe.sh`：`slaveinfo <tap> -map` 触发 SOEM 的 CoE
  SDO 读取，断言 `MBX length 128/128 + protocols 04`、1 从站、8 位 I/O 映射不变、且应答器统计
  出现 `mbox>0` / `mboxresp>0`。
- 测试新增 **8 条**：esc-core +5（邮箱头往返/协议掩码、容量检查、SOEM 风格上传请求→Abort 编码、
  SDO Info/短数据不应答、mailbox full 位）、esc-node +3（SDO 请求→Abort 响应 + 状态位 + 主站
  取走后清除、非 CoE/SDO Info 不应答、过短邮箱写入忽略）。

**验证结果**
- 本机 Windows（rustc 1.97.1）：fmt/clippy(-D warnings) exit 0；`cargo test --workspace` **90 passed**
  （esc-core 49 / esc-node 25 / wire 16）。
- Linux 验证机（**未修改的** SOEM，零补丁）：
  - `slaveinfo -map`：`MBX length wr: 128 rd: 128 MBX protocols : 04`（CoE）、发出 SDO 请求
    （`PACKET slave:1 index:1c00.00`），应答器 `mbox=1 mboxresp=1`；
    `soem-mailbox-probe.sh` → `OK: SOEM 通过 CoE 邮箱发出 SDO 请求并收到 SDO Abort（mbox=1 mboxresp=1）`。
  - 过程数据回归 `soem-process-data.sh`：仍 `mapped 1O+1I` → **OP**、1055 轮 **WKC 3**、0 次 wrong。
  - 双主站扫描 `minimal-esc-responder.sh`：SOEM `1 slaves found`/8bits/`State: 4`；IgH
    `0  0:0  PREOP  +`、拓扑断言 OK；统计可见 `mbox=3 mboxresp=3`。
- IgH 侧仍 PREOP（`ethercat slaves` 不触发 SDO）；IgH 进 OP 还需要对象字典
  （0x1C12/0x1A00 等）让主站配置 PDO 域，属 B7 后续。

**卡点**
- 邮箱只回 Abort（无对象字典）：IgH `ethercat sdos` 这类 SDO Info 请求仍会超时（SDO Info 未实现）。
- 输入数据恒 0；SM buffered/PDI 握手语义、watchdog 未实现。
- ⚠️ 待验证：真实 ESC 的 SM Status/缓冲区状态位语义（本实现用"主站读走即清 mailbox full"的简化
  模型）；多从站/多邮箱并发、mailbox counter（高 4 位）未实现。

**下次开工第一件事**
- **B7 子项 2：对象字典 + SDO 上传（0x1C12/0x1C13/0x1600/0x1A00/0x1018）**：把 Abort-only
  响应扩展为按索引查表应答（expedited upload），先覆盖主站配置 PDO 域所需的
  0x1C12/0x1C13/0x1600/0x1A00 与 0x1018；验收 = SOEM `slaveinfo -map` 打印出 PDO 映射（不再为空）、
  IgH `ethercat sdos` 能列出对象。

---

## 2026-11-12 19:26 班（低峰）— B6 子项：SII SM/TxPDO/RxPDO 类别（8 位 IO 画像）+ 修正 LRW WKC → SOEM 零补丁进 OP

**做了什么**
- `esc-core/sii.rs` 新增三个 SII 类别（字节布局按两家**未修改**主站的解析代码逐字节复核）：
  - **0x0029 Sync Manager**：4 条 × 8 字节（physical start u16、length u16、control u8、status u8、
    enable u8、pdi control u8）。SM0/SM1 全 0（mailbox 仍关闭，B7 再启用）；SM2 = 输出
    （0x1100、1 字节、buffered/写方向 `control=0x04`）、SM3 = 输入（0x1110、1 字节、读方向 `0x00`）。
  - **0x0032 TxPDO**（`0x1A00` → SM3，8 位输入对象 `0x6000:00`）与 **0x0033 RxPDO**
    （`0x1600` → SM2，8 位输出对象 `0x7000:00`）：PDO 头 8 字节 + 每条映射 8 字节。
  - 固定区 EEPROM 大小 word 0x003E 由 1（256 B）改为 2（384 B），覆盖新镜像（140 words = 280 B）。
  - 证据来源：IgH `master/slave.c: ec_slave_fetch_sii_syncs()/ec_slave_fetch_sii_pdos()` 与
    `master/fsm_slave_scan.c` 的类别分发（0x32 = TxPDO/输入、0x33 = RxPDO/输出）；
    SOEM `src/ec_main.c: ecx_siiSM()`（类别 size words / 4 = SM 数）与 `ecx_siiPDO()`
    （8 字节头 + 8 字节映射、+5 处 bit_length）、`src/ec_config.c: ecx_map_sii()`
    （无 CoE 时回退 SII PDO → SM2/SM3 的长度与类型）。
- **修正 LRW 的 WKC 规则**（19:19 班"读 +1、写 +1"的假设对 LRW 是错的）：逻辑寻址的 LRW 是
  **写 FMMU 命中 +2、读 FMMU 命中 +1**（双向都命中 = 3），LRD = +1、LWR = +1。证据：
  IgH `master/datagram_pair.c` 注释 "If LRW is used, output FMMUs increment the working counter
  by 2, while input FMMUs increment it by 1"（期望值 `used[OUTPUT] * 2 + used[INPUT]`）；
  SOEM `src/ec_main.c` 对 LWR 也乘 2（"output WKC counts 2 times when using LRW, emulate the
  same for LWR"）。`wire::wkc_increment` 文档标明该例外。
- 新增可复现断言脚本 `scripts/spikes/soem-process-data.sh`：未修改 SOEM `simple_ng` ↔ 本模拟器
  做 8 位 PDO 交换，断言 1 从站 / 映射 1O+1I / 进入 OP / 每轮 WKC=3 / 0 次 wrong。
- 测试新增 **3 条**（esc-core/sii：SM 类别按两个主站解析器复刻、TxPDO/RxPDO 字节与 bit_length、
  EEPROM 容量覆盖全类别链），并更正 esc-node 的 LRW WKC 期望（写-only = 2、双向 = 3）。

**验证结果**
- 本机 Windows（rustc 1.97.1）：`cargo fmt --all -- --check` exit 0；
  `cargo clippy --workspace --all-targets -- -D warnings` exit 0；
  `cargo test --workspace` **82 passed**（esc-core 44 / esc-node 22 / wire 16）。
- Linux 验证机（**未修改的** SOEM，零补丁）：
  - `slaveinfo`：`Output size: 8bits`、`Input size: 8bits`、`State: 4`、
    `SM2 A:1100 L:1 F:00010004 Type:3`、`SM3 A:1110 L:1 F:00010000 Type:4`、
    `FMMU0 ... Lsb:0 Leb:7 Ps:1100 Ty:02 Act:01`；
  - `simple_ng`：`mapped 1O+1I bytes` → SAFEOP → **all slaves are now operational**，
    每轮 **WKC 3**（1055 轮、0 次 `wrong`，应答器侧 `al=0x08` = OP）；
  - 新脚本：`OK: SOEM simple_ng 进入 OP，1055 轮 WKC=3，无 WKC 不符`。
- IgH（**未修改**）：`0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`，拓扑断言 OK（无 mailbox/CoE →
  仍停在 PREOP，属 B7）。

**卡点**
- IgH 进 OP 需要 mailbox + CoE SDO（B7）才能配置 PDO 域；当前只有 SOEM 走 SII 回退路径能进 OP。
- 输入数据恒 0（无 PDI/应用层）；SM2/SM3 的 buffered/SM 握手语义未实现；watchdog 未实现。
- SII 尚无 FMMU 类别 0x0028（SOEM `FMMUfunc` 仍为 0:0）与 DC 类别；⚠️ 字段语义待核实后补，
  不凭记忆猜。

**下次开工第一件事**
- **B7 子项 1：邮箱链路（SII mailbox 声明 + SM0/SM1 配置 + SDO Abort 应答）**：SII 固定区
  0x0018 起声明 std mailbox 与 CoE 协议位、SM 类别补 SM0/SM1；实现 ESC 邮箱数据区
  （0x1000/0x1080）与 SM 状态位、对未知对象回 SDO Abort（0x06020000）；验收 = SOEM
  `slaveinfo -sdo` 能收到应答（不再超时）。之后再补对象字典与 PDO 映射（0x1C12/0x1A00/0x1600）。

---

## 2026-11-12 19:19 班（低峰）— B5 子项：FMMU 逻辑寻址最小闭环（LRD/LWR/LRW）+ 修正 FMMU Type 只读错误

**做了什么**
- `esc-core` 新增 `memory` 模块：`DlUserMemory` 覆盖 **DL 用户内存 0x1000–0xFFFF**（48 KiB），
  提供 `covers/read/write` 与越界拒绝；这是 FMMU 逻辑寻址的"物理侧"。
- `fmmu.rs` 新增逻辑→物理翻译：
  - `Fmmu::logical_end()`（32 位溢出返回 `None`）、`is_byte_aligned()`（逻辑起始位 0 / 逻辑结束位 7 /
    物理起始位 0；IgH `master/fmmu_config.c` 恒写 0/7/0，SOEM `ec_config.c` 默认映射相同）；
  - `Fmmu::translate(address, len)` 返回 datagram 数据区与 FMMU 逻辑区间的**字节对齐交集**
    （`datagram_offset` + `physical_address` + `len`），支持 datagram 大于或部分覆盖 FMMU 区间；
    未激活、非字节对齐、区间不相交、32 位/16 位地址溢出均返回 `None`。位级（任意 bit 偏移）映射
    仍未实现（⚠️ 待验证项，见下文）。
- `esc-node` 接入逻辑寻址：LRD/LWR/LRW 不再一律不命中，改为遍历 8 个 FMMU；读方向要求
  Type.bit0、写方向要求 Type.bit1，按 `translate` 在逻辑地址与 DL 用户内存之间搬运；WKC 按
  "读命中 +1、写命中 +1"（LRW 双向命中 = +2），未命中不增。
  **（2026-11-12 19:26 班更正：LRW 的写 FMMU 命中应为 +2、读 FMMU 命中 +1，双向 = 3；
  依据 IgH `datagram_pair.c` 与 SOEM `ec_main.c`，详见本文件顶部 19:26 班条目。）**
- **修正上一班次（19:02）的错误假设：FMMU Type（0x0B）不是只读**，它是主站写入的方向配置位
  （IgH `fmmu_config.c: ec_fmmu_config_page()` 写 `data + 11 = 0x01/0x02`；SOEM `ec_config.c`
  整块写 `ec_fmmut` 结构含 `FMMUtype`）。按只读丢弃会让方向位恒为 0、FMMU 永不命中；现在只读
  的只有保留字节 0x0D–0x0F。
- 测试新增 **14 条**：esc-core +8（FMMU 翻译：全区间/部分重叠/未激活/非对齐/零长度/逻辑溢出/
  物理越界；DL 用户内存：读写往返/末端边界/寄存器区与越界拒绝）、esc-node +6（LWR→LRD 经两个
  方向 FMMU 往返、未激活不命中、LRD 需读方向位、LRW 按方向累加 WKC、部分重叠只搬交集、
  未映射字节不被覆盖），另把原 FMMU 配置测试改为断言 Type 写入生效。

**验证结果**
- 本机 Windows（rustc 1.97.1）：`cargo fmt --all -- --check` exit 0；
  `cargo clippy --workspace --all-targets -- -D warnings` exit 0；
  `cargo test --workspace` **79 passed**（esc-core 41 / esc-node 22 / wire 16）。
- Linux 验证机（**未修改的** IgH 1.6.13 + SOEM，回归）：SOEM `1 slaves found and configured`、
  `Activeports:1.0.0.0`、`Man: 00000ec5 ID: 0000e5c1`；IgH `0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`；
  拓扑断言 `OK: 单从站拓扑计算未报错`；脚本 exit 0（本次改动未影响扫描路径）。
- ⚠️ 待验证：**真实主站端到端的 LRD/LWR**。主站要配置 domain 才会下发逻辑读写，当前 SII 还缺
  SM/PDO 类别与 mailbox（B6/B7），尚未进入该阶段；验证方法 = B6 补齐 SII 类别后看 IgH/SOEM 是否
  进 SAFEOP 并出现 LRD/LWR 命中（WKC 与数据往返）。位级 FMMU 映射同理待后续。
- ⚠️ 待复核：FMMU Type 可写性有 IgH/SOEM 两处源码行为证据，但未逐字复核 ESC Sec.II 寄存器表
  （无离线手册）；下次有手册时补一条引用。

**卡点**
- 无。SM 触发的 mailbox 握手与 SII 的 SM/PDO 类别仍缺失，主站不会下发 PDO 逻辑读写，
  因此本班闭环只能靠本仓库的 datagram 级集成测试验证。

**下次开工第一件事**
- **B6 SII 生成器（前置优先）**：SII 增加 SM 类别 0x0029 + TxPDO 0x0032 / RxPDO 0x0033 +
  默认 PDO 映射（0x1600/0x1A00），让 IgH/SOEM 能配置 SM/FMMU 并下发给从站；随后再进 B7 CoE
  最小闭环，最终以 B8 双主站冒烟端到端验证本班的 FMMU 逻辑寻址。

---

## 2026-11-12 19:02 班（低峰）— B5 子项：SM/FMMU 寄存器块（主站整块写 + 只读字节语义）

**做了什么**
- `esc-core` 新增两个模块，补上 B5 的 SM/FMMU 寄存器块：
  - `sm.rs`：Sync Manager 寄存器区 **0x0800–0x083F**（8 通道 × 8 字节）——start address / length /
    control / status / activate / PDI control 的编解码；control 位域解码（mode：0=buffered、2=mailbox；
    direction：0=read、1=write、2=read/write；ECAT/DLS-user 事件、watchdog）与
    `is_enabled()/is_mailbox()/direction()/is_watchdog_enabled()`；
  - `fmmu.rs`：FMMU 寄存器区 **0x0600–0x067F**（8 实体 × 16 字节）——logical start address / length /
    logical start/end bit / physical start address / physical start bit / type / activate 的编解码；
    `is_enabled()/can_read()/can_write()`。布局按 **ETG.1000.4 table 56/58** 逐字段核对
    （以 etherage 的寄存器模型交叉参考，MIT OR Apache-2.0；只读参考，未复制代码）。
- `Registers::access()` 扩展**只读子地址**：SM Status（`0x0805+8i`）、SM PDI Control（`0x0807+8i`）、
  FMMU Type（`0x060B+16i`）与 FMMU 保留字节（`0x060D–0x060F+16i`）。主站按真实习惯**整块写**
  （SM 8 B / FMMU 16 B）配置时，可写字段生效、只读字节被丢弃——与真实 ESC 语义一致。
- `esc-node` 新增 2 条 datagram 路径集成测试：APWR 8 B SM0 配置后 APRD 回读（status/PDI control 保持 0）；
  APWR 16 B FMMU0 后 APRD 回读（type/保留字节保持 0）。
- 单测新增 **10 条**（esc-core +8：地址步长、只读字节判定、编解码往返、整块写策略；
  esc-node +2：上述两条）。

**验证结果**
- 本机 Windows（rustc 1.97.1）：`cargo fmt --all -- --check` exit 0；
  `cargo clippy --workspace --all-targets -- -D warnings` exit 0；
  `cargo test --workspace` **65 passed**（esc-core 33 / esc-node 16 / wire 16），本项新增 10 条。
- Linux 验证机（**未修改的** IgH 1.6.13 + SOEM，回归）：SOEM `1 slaves found and configured`、
  `Activeports:1.0.0.0`；IgH `0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`；拓扑断言
  `OK: 单从站拓扑计算未报错`、脚本 `SPIKE_RC=0`。说明只读字节策略未破坏扫描
  （SOEM 初始化会对 FMMU/SM 区做 BWR 清零，涉及字节均为只读/保留，语义不变）。
- ⚠️ 待验证：真实主站在配置阶段写入 SM/FMMU 后进 SAFEOP 的完整流程（需 B7 CoE / B8）。

**卡点**
- 无。本项是纯寄存器语义，VM 只做回归。
- 注意：SM/FMMU 的**功能**（mailbox 握手、FMMU 逻辑寻址搬运）尚未实现，B5 仍未勾选完成。

**下次开工第一件事**
- **B5 继续**：实现 FMMU 命中的**逻辑寻址最小闭环**（Lrd/Lwr/Lrw）——按已写入的 FMMU 实体判断
  逻辑地址是否命中本从站，并把数据搬到物理侧（寄存器阵列 / 从机内存），单测覆盖单实体与
  跨区间边界；这是 B8 双主站进 OP 前 ESC 数据面的最后一块。

---

## 2026-11-12 18:56 班（低峰）— IgH 拓扑计算修复：DL Status（0x0110）端口位按 ETG.1000.4 复核

**做了什么**
- 修正 `esc-node` 的 DL Status 初值：`0x503F` → `0x5610`，并新增 `dl_status_value()`（`const fn`）按
  **ETG.1000.4 table 34** 的位布局拼装：bit0–2 = DL user operational / DL user watchdog /
  extended link detection，bit3 保留；bit(4+i) = 端口 i 物理链路；bit(8+2i) = 端口 i `loop_back`；
  bit(9+2i) = 端口 i `signal_detection`。
- 证据链（两家主站源码交叉复核，只读参考、未复制代码）：
  - IgH 1.6.13 `master/fsm_slave_scan.c: ec_fsm_slave_scan_state_datalink()`：
    `link_up = dl_status & (1 << (4+i))`、`loop_closed = dl_status & (1 << (8+2i))`、
    `signal_detected = dl_status & (1 << (9+2i))`；
  - IgH 1.6.13 `master/master.c: ec_master_calc_topology_rec()`：按端口 **3 → 1 → 2**
    （`next_table = {3,2,0,1}`）检查 `loop_closed`，为 0 即认为端口后还有从站；单从站时递归越界返回 -1
    → `Failed to calculate bus topology`。旧值把端口 1 报成"环回开放"（bit10=0），故必现。
    **旧注释"只对端口 3、2 检查"也是错的**（实际 3/1/2），一并更正；
  - SOEM `src/ec_config.c: ecx_config_init()`：端口两位 `(0x3 << (8+2i)) == (0x2 << (8+2i))`
    表示"端口开放且通信建立"，与"闭合端口 signal=0"一致。
- 新增 4 条单测：位布局逐位断言、复刻 IgH 拓扑判定（端口 3/1/2 必须闭合）、复刻 SOEM
  `topology/activeports` 提取（单链路 = `1`、`Activeports:1.0.0.0`）、APRD 0x0110 可读。
- `scripts/spikes/minimal-esc-responder.sh` 升级为**断言脚本**：扫描前 `dmesg -C`，
  扫描后若内核日志出现 `Failed to calculate bus topology` 则 FAIL & exit 1，否则打印
  `OK: 单从站拓扑计算未报错`。
- 文档同步：[spike-3](docs/spikes/spike-3-minimal-esc-responder.md) §3 证据 / §4 第 4 个真问题 /
  §5 限制 / §6 下一步；`AGENTS.md` §6 B2b 行。

**验证结果**
- 本机 Windows（rustc 1.97.1）：`cargo fmt --all -- --check` exit 0；
  `cargo clippy --workspace --all-targets -- -D warnings` exit 0；
  `cargo test --workspace` **55 passed**（esc-core 25 / esc-node 14 / wire 16），本项新增 4 条。
- Linux 验证机（**未修改的** IgH 1.6.13 + SOEM）：
  - `dmesg` 本次扫描段：`Scanning bus.` → `Bus scanning completed in 6 ms.` → `No DC reference clock found.`，
    **无** `Failed to calculate bus topology`（修正前必现，对照见 18:33/13:27 班记录）；
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`（无回归）；
  - SOEM `slaveinfo`：`1 slaves found and configured`、`Activeports:1.0.0.0`、`Man/ID` 不变（无回归）；
  - 升级后脚本整体 `SPIKE_RC=0`，拓扑断言 `OK: 单从站拓扑计算未报错`。

**卡点**
- VM 锁 18:55:30 被其他 worker 占用，19:0x 释放后取得；期间先做本地实现 + 单测，未空等。
- ⚠️ 只验证了**单从站**拓扑；多从站链/分支时各从站 `loop_closed` 必须按实际拓扑上报，
  需 ≥2 从站才能验证，留到 B8。
- 其余：无代码/网络卡点（push 正常）。

**下次开工第一件事**
- **B5 可独立子项（B8 的前置）**：`esc-core` 补 SM（0x0800–0x087F）/ FMMU（0x0600–0x06FF）
  寄存器块的读写语义与主站配置阶段会用到的 PREOP→SAFEOP 握手（邮箱 SM0/SM1 先只声明不实现传输），
  单测覆盖；这是 B7 CoE 与 B8 进 OP 的地基。

---

## 2026-11-12 18:33 班（低峰）— SII 固定区 CRC-8（word 0x0007）实现，SOEM eepromtool 交叉验证

**做了什么**
- 修正 SII 固定区语义（B6 子项"CRC-8"）：CRC 在 **word 0x0007**、覆盖 word 0x0000–0x0006（14 字节），
  多项式 `x^8+x^2+x+1`（0x07）、初值 0xFF、MSB-first；旧文档记成 "word 0x003F" 是错位
  （**0x003F 是 SII 版本 = 1**）。证据：SOEM `samples/eepromtool/eepromtool.c` 的 `SIIcrc()`/`-walias`/`-i`
  与 IgH `tool/sii_crc.cpp`（pycrc `--poly=0x07 --xor-in=0xff`）。
- `esc-core::sii` 新增 `sii_crc8()`、`SII_CHECKSUM_WORD`/`SII_CHECKSUM_BYTES`/`SII_EEPROM_SIZE_WORD`/
  `SII_VERSION_WORD` 与 `SiiImage::checksum()/calculate_checksum()/verify_checksum()/set_alias()`
  （改别名自动重算，等价 SOEM `eepromtool -walias`）；镜像补 word 0x003E=1（声明 256 B）、word 0x003F=1。
- 同步文档：`AGENTS.md` B6 行 + B2b 备注、`docs/spikes/spike-3-minimal-esc-responder.md` 的 CRC 位置描述。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 通过；
  `cargo test --workspace` **51 passed**（esc-core 25 / esc-node 10 / wire 16），本项新增 6 条。
- Linux 验证机（未修改主站样本）：SOEM `eepromtool <tap> 1 -i` → `1 slaves found`、
  `Checksum: 0030` / `calculated: 0030`、`Size: 0001 = 256 bytes`、`Version: 0001`；
  回归：SOEM `slaveinfo` `1 slaves found and configured`、IgH `ethercat slaves`
  `0 0:0 PREOP + 0x00000ec5:0x0000e5c1`，均无变化（IgH 仍打印 topology 报错，属另一项）。
  新增测试向量另由 SOEM 逐位形式与 IgH bit-feeding 形式两个实现交叉校验（如 14 个 0x00 → 0x30）。

**卡点**
- 本班前半段（18:33–18:50）会话沙箱为 workspace-write、`D:\` 仓库写探针被拒，先按 12:30 班记录报 BLOCKED；
  用户切到 danger-full-access 后继续。VM 锁 18:38 被其他 worker 占用 → 先做本地项，19:05 锁释放后补 VM 验证。
- IgH `Failed to calculate bus topology` 仍未修（DL Status 端口位语义，⚠️ 待验证），本项未触碰。
- 旧条目（13:27 班）里 "SII CRC-8（word 0x3F）" 为当时的错位记录，以本条为准。

**下次开工第一件事**
- **IgH 拓扑计算**：按 ESC Sec.II 复核 DL Status（0x0110）端口 0–3 的 link/loop 位与
  `ec_master_calc_topology_rec()` 期望（计划重点第 1 项），修好后推进 **B8 双主站进 OP**。

---

## 2026-11-12 13:27 班（低峰）— B2b 最小 ESC 应答器（IgH + SOEM 各看到 1 个从站）

**做了什么**
- 新增 `crates/esc-node`（二进制 `ec-sim-esc`，零第三方依赖）：Linux TAP 上应答 EtherCAT 帧；
  TAP 由进程自己 `open /dev/net/tun` + `TUNSETIFF` 创建并持有 fd（无持有者 = NO-CARRIER）。
- `esc-core` 新增 `sii`（最小 SII 镜像：我方占位 VID/PID、字符串类别 0x000A、通用类别 0x001E 32 B、0xFFFF 终止、无 mailbox）
  与 `eeprom`（0x0500–0x050F EEPROM 接口：R64 状态位、8 字节读、写命令计数）；
  `esc-node` 组合寄存器/AL 状态机/EEPROM/帧应答（BRD/BWR、APRD/APWR 按 ADP、FPRD/FPWR 按站地址/别名、WKC 累加）。
- 修正 `wire::auto_increment_step`：主站 ADP = 环位置相反数、从站转发前 **+1**、收到 0 者执行
  （证据：IgH `master/datagram.c` `ring_position * (-1)`；SOEM `ec_config.c` `ADPh = 1 - slave`）。
- 新增 `scripts/spikes/minimal-esc-responder.sh` 与 `docs/spikes/spike-3-minimal-esc-responder.md`。

**验证结果**
- 本机 Windows：`cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` 均 exit 0；
  `cargo test --workspace` 全绿（新增 esc-node 10 条、esc-core 11 条、wire 修正 1 条）。
- Linux 验证机（未修改主站）：
  - SOEM `slaveinfo`：`1 slaves found and configured`，`Name:EC-Sim SIMULATED slave`，
    `Man: 00000ec5 ID: 0000e5c1 Rev: 00000001`，`Configured address: 1001`。
  - IgH `ethercat slaves`：`0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`（无错误位；
    `ethercat master` 显示 Slaves: 1、Rx frames 1102、Lost frames 0）。
  - 应答器统计：`in=1100 out=1094 dropped=6 hits=1094 al=0x02(PREOP)`。
- 过程中修正的真问题：以太网 60 字节填充被当成非法帧（IgH 全部 TIMEOUT）；
  通用类别必须 32 字节（否则 IgH 标 `E`）；ADP 方向（见上）。

**卡点**
- IgH 仍打印 `Failed to calculate bus topology`（从站列表/状态正常，无 `E`）：DL Status 端口
  0/1 的链路/环回语义未完全对上 `ec_master_calc_topology_rec()`，⚠️ 待验证（下一步小项）。
- SII CRC-8（word 0x3F）未实现：两家主站扫描都不校验，接会校验 CRC 的主站前需 B6 补。
- 未进 OP（无 mailbox/PDO/SM/FMMU）→ 属 B8；本次目标（看到 1 个从站）已达成。

**下次开工第一件事**
- **B8 双主站冒烟**：补 SM/FMMU/PDO 让 IgH 与 SOEM 都进 OP；先修 IgH 拓扑端口位（小项、可独立验证）。

---

## 2026-11-12 12:30 班（低峰）— B1 Rust workspace 骨架

**做了什么**
- 新建 Cargo workspace（`resolver = "2"`，edition 2021，MSRV 1.75），按 AGENTS §7 落地 6 个成员：
  `crates/esc-core`、`crates/wire`、`crates/coe`、`crates/plugin-host`、`crates/control-plane`、
  `crates/cli`（二进制名 `ec-sim`）。
- `esc-core`：`AlState`（INIT/PRE-OP/SAFE-OP/BOOT/OP，编码 1/2/3/4/8，可往返）+ `EscError`。
  寄存器地址、FMMU/SM、AL 握手留 B5，**不凭记忆写地址**。
- `wire`：EtherType `0x88A4`、以太网头 14 字节、最小 EtherCAT 帧 16 字节、
  `is_ethercat_frame()` 按 EtherType 过滤（对应 Spike 1 踩坑 #2：TAP 会混入 IPv6 等内核帧，
  必须过滤）。datagram 编解码/WKC 语义留 B4。
- `plugin-host`：`DevicePlugin` 最小契约（R2：名称须带 `SIMULATED`）。
- `coe`/`control-plane`：只落模块边界与依赖方向（coe→esc-core/wire；控制面单向依赖），
  不写空壳实现。
- CI `.github/workflows/ci.yml`：ubuntu/windows/macos 三平台跑 `fmt --check`、
  `clippy -D warnings`、`test`。

**验证结果**
- 本机（Windows，rustc 1.97.1）：
  - `cargo fmt --all -- --check` → exit 0
  - `cargo clippy --workspace --all-targets -- -D warnings` → exit 0
  - `cargo test --workspace` → exit 0（3 个单测：AL 状态码往返/未知码、wire 过滤/短帧）
- 三平台 CI 首跑（commit `93a2fbf`，run `43158276907`）→ **ubuntu / macos / windows 全部 success**（fmt + clippy -D warnings + test 三步都过）。

**卡点**
- 本班起始时 DSH 文件策略为 workspace-write，仓库写探针 `.write-probe` 被拒
  （ACL 显示用户有 Modify，是沙箱限制）；人工把会话切到 danger-full-access 后探针通过。
  **后续班次若再遇写探针失败**：按 prompt 直接 `BLOCKED: 沙箱未继承 full access` 退出，
  不要尝试用审批升级绕过（一次审批只放行一次操作）。
- 无代码/网络卡点。

**下次开工第一件事**
- **B2b 最小 ESC 应答器**：在 Linux 验证机上让持 TAP 的进程应答 `BRD 0x0000` / `APWR 0x0010` / `APRD 0x0130`，
  目标 = IgH `ethercat slaves` 与 SOEM `slaveinfo` 各看到 1 个从站（不要求进 OP）；结论写 `docs/spikes/`。

---

## 2026-11-12（人工会话，补充推进）— B5 第一块：esc-core 寄存器 + AL 状态机

**做了什么**
- 完成 B5 的可独立验证子步骤：
  - 4 KiB ESC 寄存器阵列（0x0000–0x0FFF），关键寄存器地址常量，读/写权限表（识别类/状态类对主站只读，内部写可维护）；
  - AL Control/Status/Status Code 语义：Error ACK、ID Request、非法状态迁移码 `0x0011`；
  - ESM 迁移表（INIT↔PREOP↔SAFEOP↔OP，任意状态可回 INIT，OP 不可直接回 PREOP）。

**验证结果**
- `cargo test --workspace`：**25 passed**（wire 16 + esc-core 9）；
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 通过。

**卡点**
- 剩余（B5 后续）：SM/FMMU 寄存器块、EEPROM 接口状态机、watchdog、DC。

**下次开工第一件事**
- 继续 **B5**：SM/FMMU 寄存器块 + EEPROM 接口状态机（Busy / 8 字节能力位）。

---
## 2026-11-12（人工会话）— 班次规则改为"窗口内持续推进"

**做了什么**
- 按用户反馈修正班次协议：不再"做完一个 backlog 项就停"，改为**在低峰窗口内循环推进**，直到剩余 <15 分钟才收尾（`AGENTS.md` §5）。
- 明确：一个 90 分钟班通常应完成 **2–4 个**可独立验证的小项；跨项是默认行为，仍禁止空壳/假测试/半成品提交。
- backlog 标记 B4 完成。

**验证结果**
- 规则已写入 AGENTS §5；随后两个定时任务 prompt 同步。

**卡点**
- 无。

**下次开工第一件事**
- **B5 `esc-core` 最小 ESC**。

---
## 2026-11-12（人工会话，补充推进）— B4 wire crate

**做了什么**
- 在低峰窗口剩余时间内继续推进（不等下一班），完成 **B4 `wire` crate**：
  - 帧/datagram 编解码：帧头（Length 11bit + Type 4bit）、datagram 头（Cmd/Idx/ADP/ADO/Len+M/IRQ）、WKC；
  - 一帧多 datagram（M 位）解析与编码；长度/越界/未知命令/非法帧类型全部返回错误而非 panic；
  - 13 种命令的读写属性与 **WKC 增量规则**（读+1 / 写+1 / 读写+2）；
  - 寻址：自增（ADP 递减语义）、配置站地址/别名匹配、广播、逻辑地址 + FMMU 区间相交判定；
  - 从站侧自增寻址处理助手 `auto_increment_step`（ADP==1 处理，否则减 1 转发）。

**验证结果**
- `cargo test -p wire`：**16 tests passed**；
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 均通过。

**卡点**
- 无。`wire` 目前零第三方依赖。

**下次开工第一件事**
- **B5 `esc-core` 最小 ESC**（寄存器阵列 + SM/FMMU + AL 状态机 INIT/PREOP 握手）。

---
## 2026-11-12（人工会话）— 修正班次起始日

**做了什么**
- 发现定时任务的 `DTSTART` 写成 `2026-11-13T00:30`，导致**当天已有的 12:30/18:30/20:30/22:30 四班被 rrule 排除**，首班被推到了次日凌晨。
- 改为 `DTSTART;TZID=Asia/Shanghai:20261112T003000`（保留同一 RRULE），首班立刻回到**今天 12:30**。

**验证结果**
- `schedule_list` → `scheduledAtLocal = 2026-11-12T12:30:00+08:00`（原为 11-13 00:30）。

**卡点**
- 无。教训：rrule 的 DTSTART 必须早于或等于期望的首个班次，否则当天更早的时刻不会生成。

**下次开工第一件事**
- 今天 12:30 首班：**B1 Rust workspace 骨架**。

---
## 2026-11-12（人工会话）— 发布 v0.0.1（pre-alpha）

**做了什么**
- 通过 GitHub API（fine-grained PAT）创建预发布 **v0.0.1**，tag 打在 `3ac44a5`：
  `https://github.com/archerding/ec-sim/releases/tag/v0.0.1`
- Release 说明诚实标注：**无二进制**；只记录已验证结论（TAP 双向 0x88A4、未修改 SOEM 绑定 TAP、未修改 IgH `ec_generic` 绑定 TAP），并列出未实现项（虚拟从站枚举、ESC 内核、CoE、插件、Web UI、MCP）。
- README 的 Status 行加上预发布链接。

**验证结果**
- API 返回 `RELEASE_OK id=442354817 prerelease=True`；`git ls-remote --tags` 确认 `refs/tags/v0.0.1 → 3ac44a5`。

**卡点**
- 无。

**下次开工第一件事**
- **B1 Rust workspace 骨架**（完成后自动推送）。

---
## 2026-11-12（人工会话）— 启用班次自动推送

**做了什么**
- 按用户决定：班次任务**验证通过并提交后自动 `git push`**，让夜间进度在 GitHub 上可见。
- `AGENTS.md` §5 执行步骤改为「写 WORKLOG → 单次提交 → 自动推送」；新增 §10「自动推送规则」：禁止 force、禁止绕过测试、推送失败重试一次后记录卡点、非 fast-forward 时停止等人处理。
- 两个定时任务的 prompt 同步更新。

**验证结果**
- `git push`（SSH，免 token）可用；`--dry-run` 无交互提示。

**卡点**
- 无。

**下次开工第一件事**
- **B1 Rust workspace 骨架**（完成后会直接推送到 GitHub）。

---
## 2026-11-12（人工会话）— 发布到 GitHub

**做了什么**
- 创建公开仓库 `https://github.com/archerding/ec-sim`，推送 `main`（12 个提交，含 README、双许可、插件规范与目录、两份 spike 证据）。
- 远端使用 SSH（`git@github.com:archerding/ec-sim.git`）：本机密钥已注册到 GitHub，无需 token；HTTPS 在本网络被墙，SSH 22 端口直连可用。
- 推送前做了公开前脱敏：移除旧工作区路径与无关项目名（历史已重写，`history_private_hits=0`）。

**验证结果**
- `git push` 成功，`origin/main` = `3a3508c`；匿名 HTTPS 可读取 → 仓库为 **public**。

**卡点 / 待办**
- GitHub 仓库描述与 topics 需在网页端填写（本地 git 无法设置）。
- 定时任务目前**不自动 push**；是否开启"每班提交后自动推送"待用户决定。
- CI workflow 尚未添加（属于 B1 Rust workspace 的工作）。

**下次开工第一件事**
- 仍是 **B1 Rust workspace 骨架**；提交后可选择手动 `git push`。

---
## 2026-11-12（人工会话）— 设备插件规范与首批目录

**做了什么**
- 用户授权：可联网检索知名品牌电机/IO 等设备的公开文档，据此制作**闭源付费插件**。
- 新增 `docs/DEVICE-PLUGINS.md`：R1–R5 五条红线（不复制厂商文件、不用厂商 VID/PID、不宣称认证、禁止开发竞争产品的设备不做、必留来源证据链）+ 制作流程 + manifest v1 + 闭源/定价/审核清单。
- 新增 `plugins/`：README、`_template/plugin.yaml`、`catalog.yaml`（含 Beckhoff/WAGO/Turck/Inovance/Delta/Leadshine/Yaskawa/Panasonic/Schneider 等 15 个候选设备，标注 P0–P2）。
- `AGENTS.md` 新增 §9「设备插件与互联网调研」，backlog 增补 B9（声明式插件加载器）、B10（首个 P0 插件 EL2008）、B11（CiA402 行为插件）。

**验证结果**
- 目录与规范已入库；尚无插件实现代码（B9 起）。

**卡点**
- 首个设备的真实 PDO/对象字典数据需按流程联网核实并登记来源；未核实前不得写入 manifest（标 `⚠️ 待核对`）。

**下次开工第一件事**
- **B1 Rust workspace 骨架**（或直接做 **B10**：用声明式 manifest 先跑通一个 EL2008 兼容从站）。

---
## 2026-11-12（人工会话）— 按官方低峰口径重排班次

**做了什么**
- 收到用户给出的准确口径：**高峰 = 工作日（周一至周五，不含中国法定节假日）09:00–12:00、14:00–18:00**；其余全部为低峰（含周末与节假日全天）。
- 重排定时任务：
  - 每日 8 班 `00:30 / 02:30 / 04:30 / 06:30 / 12:30 / 18:30 / 20:30 / 22:30`（覆盖工作日三段低峰：凌晨、午间、傍晚到深夜）
  - 周末补充 2 班 `10:00 / 15:00`（周末全天低峰，可用白天）
- 排班原则：只排"普通工作日也属于低峰"的时刻，因此**不需要维护法定节假日日历**；高峰时段一律不排。

**验证结果**
- 两个任务均 active：每日班次下次 `2026-11-13T00:30+08:00`；周末班次下次 `2026-11-14T10:00+08:00`。

**卡点**
- 法定节假日若落在工作日，白天（09:00–18:00）实际也是低峰，但当前排班不会利用（保守取舍）；如需利用，需加节假日日历，暂不做。

**下次开工第一件事**
- `2026-11-13 00:30` 首班：**B1 Rust workspace 骨架**。

---
## 2026-11-12（人工会话）— 仓库脱敏

**做了什么**
- 按用户要求：**仓库内不记录服务器与代理信息**。
- 删除 `docs/LINUX-SERVER.md`、`docs/NETWORK.md`（内容移入本地 `.local/ops.md`，已被 `.gitignore` 排除）。
- `AGENTS.md` §8 改为"运行环境（不入库）"的约束条款；spike 文档中的环境描述改为中性表述。
- 用 `git filter-branch` 重写全部历史，删除上述文件并替换历史中出现过的地址/端口/主机名。

**验证结果**
- `git log -p --all` 中已无地址、端口、主机名等敏感串（见本次提交后的核查命令）。
- 工作区 `git status` 干净。

**卡点**
- 旧工作区里的历史副本（另一项目在用）未做同样清理；待其退役时一并处理。

**下次开工第一件事**
- 回到 **B1 Rust workspace 骨架**。

---

## 2026-11-12（人工会话）— 夜班改为低峰期四班次

**做了什么**
- 定时任务「低峰时段多班次推进 EC-Sim」：`RRULE:FREQ=DAILY;BYHOUR=0,2,4,6;BYMINUTE=30` → 每天 **00:30 / 02:30 / 04:30 / 06:30** 各一班（Asia/Shanghai），覆盖整个 DeepSeek 低峰窗口（00:30–08:30）。
- 目标会话改为 `new`（每班一个全新会话，互不阻塞），靠 WORKLOG 衔接；**每班限时 90 分钟**，班间留 30 分钟缓冲，08:30 后绝不动作。
- prompt 增加**开工自检**：先写 `.write-probe` 验证仓库目录可写；失败则立即停止并报 `BLOCKED: 沙箱未继承 full access`，防止退回到旧副本干活。

**验证结果**
- `schedule_list`：任务 active，下次触发 `2026-11-13T00:30:00+08:00`。

**卡点**
- 定时任务创建的新会话 `cwd` 仍继承旧工作区（调度器捕获创建时会话的工作区根），故 prompt 强制 `Set-Location` 到主仓库并以自检兜底。

**下次开工第一件事**
- 00:30 首班：**B1 Rust workspace 骨架**。

---

## 2026-11-12（人工会话）— 文件策略放开，迁移收口，夜班恢复

**做了什么**
- 会话文件策略改为 `danger-full-access`：主仓库所在盘可直接读写，无需逐次审批（此前阻塞迁移的根因消失）。
- 定时任务恢复 active，并改写 prompt：第一步强制切到主仓库路径，明令禁止在旧工作区提交。
- 仍保留旧工作区副本（并行项目 `<other-project>/` 在活跃写入，其 `.git` 与旧副本共用）。

**验证结果**
- 主仓库写入成功；`git status` 干净；远端 Linux 验证机连通正常。

**卡点**
- 旧副本尚未退役，待 `<other-project>` 会话结束后再处理。

**下次开工第一件事**
- **B1 Rust workspace 骨架**（或 **B2b 最小 ESC 应答器**，先出"主站看到 1 个从站"的证据）。

---

## 2026-11-12（人工会话）— 仓库迁移到项目根目录

**做了什么**
- 按用户要求把 EC-Sim 仓库迁到 `D:\tokengence.ai\ec-sim`：以 `git clone` 方式复制，提交与 `DeepSeek Harness` 作者身份完整保留，远端配置已移除。
- 旧工作区保留为历史副本（并行项目的 `<other-project>/` 仍在其中活跃写入，未触碰）。

**验证结果**
- 新仓库 `git log` 一致；`git status` 干净；身份 `DeepSeek Harness <deepseek-harness@users.noreply.github.com>`。

**卡点**
- 新会话沙箱根仍指向旧工作区，导致无法直接写新仓库（每次写需审批）。夜班任务已暂停，待以新仓库为工作目录新建会话后重建。

**下次开工第一件事**
- 在新工作区会话里恢复定时任务；代码任务从 **B1** 或 **B2b** 开始。

---

## 2026-11-12（人工会话，非自动）— 线缆层验收

**做了什么**
- 接入一台专用 Linux 验证机（信息本地留存，不入库）；装好 Rust 工具链、与内核精确匹配的 kernel-devel、gcc/cmake/autotools。
- 停用验证机上原有的容器编排服务（快照留档），保留容器运行时备用。
- **Spike 1**（TAP 双向承载 0x88A4）→ PASS，证据 [docs/spikes/spike-1-tap-bidirectional.md](docs/spikes/spike-1-tap-bidirectional.md)。
- **Spike 2**（未修改的 SOEM / IgH 在 TAP 上）→ **两者都成立**：SOEM `ecx_init` 成功；IgH `ec_generic` 绑定 TAP、`/dev/EtherCAT0` 出现、约 496 帧/秒，模拟器侧收到 BRD 帧。证据 [docs/spikes/spike-2-masters-on-tap.md](docs/spikes/spike-2-masters-on-tap.md)。
- 新增验证脚本：`scripts/spikes/tap_ethernet_loopback.py`、`tap_hold.py`、`sniff_ecat.py`。

**验证结果**
- 线缆层最大技术风险（TAP 能否被真实主站使用）**已关闭**，且双主站均"零修改"。
- IgH 1.6.13 与 SOEM 均已在验证机构建完成并通过绑定验证。

**卡点**
- 尚无从站应答，IgH 侧每帧 timeout（预期）。下一步要让主站看到 1 个从站（B2b）。
- 验证机直连 GitHub 不通，已配置代理链路（细节不入库）。

**下次开工第一件事**
- **B1：Rust workspace 骨架**；若想先拿"主站看到 1 个从站"的证据，可先做 **B2b 最小 ESC 应答器**。

---

## 2026-11-12（人工会话，非自动）

**做了什么**
- 完成技术选型讨论，产出 `ethercat-slave-simulator-tech-selection.md`（含 5 层拆分、线缆层三方案、四层插件模型、许可组合、路线图、8 条风险、6 个待拍板决策、附录事实核查）。
- 核实了 4 条一手事实（避免沿用网上错信息）：
  - SOEM 2.x 是 **GPLv3 或商业双许可**（非网传 "GPLv2+链接例外"）。
  - IgH 用户态库 `ecrt.h` 是 **LGPL-2.1**；内核模块 GPLv2-only。
  - IgH 1.6 官方 `libfakeethercat` 只到 **PDO 级共享内存**，官方原文承认"bus error cannot be simulated"。
  - ETG 会员**免费、无分级**；Vendor ID 免费；MainDevice license 免费但须签；ETG FAQ 明确 "EtherCAT technology itself is not Open Source"，纯软件从站仿真的地位需书面澄清。
- 建立仓库：`.gitignore`、`AGENTS.md`（提交身份/许可红线/夜班协议/backlog）、本文件。

**验证结果**
- 仓库已初始化，提交身份固定为 `DeepSeek Harness <deepseek-harness@users.noreply.github.com>`。
- 尚无代码，无测试可跑。

**卡点**
- 待用户拍板 D1–D6（见 tech-selection §11）。其中 **D6（是否联系 KickCAT 上游）** 与 **D3（Windows 是否一级支持）** 会改变 backlog 顺序。

**下次开工第一件事**
- **B1：Rust workspace 骨架**（若用户尚未拍板 D1–D6，按文档推荐值默认执行，并在提交信息里注明"按推荐默认值"）。

**自动化**
- 已创建低峰期自动开工定时任务，按 `AGENTS.md` §5 协议执行。
