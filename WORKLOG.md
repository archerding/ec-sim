# WORKLOG — EC-Sim 工作日志

> 约定：**最新条目在最上面**。每条含 `日期 / 做了什么 / 验证结果 / 卡点 / 下次开工第一件事`。
> 夜班 agent 每次唤醒先读这里的第一条。
> **脱敏规则**：不写主机地址、端口、账号、代理地址；远端一律称"Linux 验证机"或"远端"。运维细节见本地 `.local/ops.md`（gitignored）。

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
