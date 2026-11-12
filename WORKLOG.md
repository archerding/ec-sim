# WORKLOG — EC-Sim 工作日志

> 约定：**最新条目在最上面**。每条含 `日期 / 做了什么 / 验证结果 / 卡点 / 下次开工第一件事`。
> 夜班 agent 每次唤醒先读这里的第一条。
> **脱敏规则**：不写主机地址、端口、账号、代理地址；远端一律称"Linux 验证机"或"远端"。运维细节见本地 `.local/ops.md`（gitignored）。

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
