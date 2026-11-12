# Spike 3：最小 ESC 应答器 —— 未修改主站看到 1 个从站

- **日期**：2026-11-12 13:27 班（低峰）
- **假设**：在 Linux TAP 上跑一个**只实现 ESC 寄存器 + SII EEPROM 接口 + 帧应答**的进程，
  未修改的 IgH 1.6.13 与 SOEM 就能扫描到 1 个从站（不要求进 OP）。
- **结论**：✅ **成立**。IgH `ethercat slaves` 输出 `0  0:0  PREOP  +  0x00000ec5:0x0000e5c1`（无错误位），
  SOEM `slaveinfo` 输出 `1 slaves found and configured` 并读到正确的 Name/Man/ID/Rev。
- **实现**：`crates/esc-node`（新 crate，二进制 `ec-sim-esc`）+ `crates/esc-core` 的
  `sii`/`eeprom` 模块；零第三方依赖，TAP 只声明用到的 libc 符号。
- **可复现脚本**：[minimal-esc-responder.sh](../../scripts/spikes/minimal-esc-responder.sh)

## 1. 方法

1. `ec-sim-esc --tap escsim0`：进程自己 open `/dev/net/tun` + `TUNSETIFF` 附着 TAP 并持有 fd
   （Spike 2 的教训：无持有者的 TAP 是 NO-CARRIER，主站发不出帧）。
2. 外部把 TAP MAC 设为 IgH `main_devices` 使用的 MAC 并 `ip link set up`
   （`ec_generic` 按 MAC 匹配设备；SOEM 不关心 MAC）。
3. SOEM：`slaveinfo escsim0`；IgH：`modprobe ec_master main_devices=<MAC>` → `modprobe ec_generic`
   → `ethercat slaves`。主站均**未修改**，来自仓库外的既有构建。

## 2. 应答器实现了什么

- ESC 寄存器：识别寄存器（Type/Revision/Build/FMMU/SM 数/端口描述/Features）、
  站地址 0x0010（可写，主站扫描时分配）、DL Status 0x0110、AL Control/Status/Status Code；
- AL 状态机复用 `esc-core::EscAl`（INIT→PREOP 握手、非法迁移 0x0011、Error ACK）；
- SII EEPROM 接口 0x0500–0x050F：状态字（`R64=0x0040`、无 busy/error）、8 字节读、写命令计数；
- SII 镜像：固定区（Vendor `0x00000EC5`、Product `0x0000E5C1`、Rev/Serial 1，均为我方占位值，
  非 ETG 注册号）、字符串类别 0x000A（`EC-Sim SIMULATED slave`）、通用类别 0x001E（32 字节）、
  `0xFFFF` 终止；**无 mailbox**（因此主站不会协商 CoE，扫描后停在 INIT/PREOP）；
- datagram：BRD/BWR/BRW 广播命中，APRD/APWR 收到 ADP==0 命中，FPRD/FPWR 按站地址/别名命中，
  WKC 按读/写属性累加；逻辑寻址（LRD/LWR/LRW）本 spike 时未实现（2026-11-12 19:19 班已补：
  按已激活 FMMU 命中在逻辑地址与 DL 用户内存 0x1000–0xFFFF 之间搬运；19:26 班又补了
  **SII SM/TxPDO/RxPDO 类别**并修正 **LRW 的 WKC 规则**（写 FMMU +2、读 FMMU +1），
  此后未修改的 SOEM `simple_ng` 能进入 **OP** 并做 8 位 PDO 交换，见 §3.1）。

## 3. 实测输出（脱敏）

SOEM（`slaveinfo`，节选）：

```
ecx_init on escsim0 succeeded.
1 slaves found and configured.
Slave:1
 Name:EC-Sim SIMULATED slave
 State: 4
 Activeports:1.0.0.0
 Configured address: 1001
 Man: 00000ec5 ID: 0000e5c1 Rev: 00000001
 MBX length wr: 0 rd: 0 MBX protocols : 00
```

IgH（`ethercat slaves` / `ethercat master`）：

```
0  0:0  PREOP  +  0x00000ec5:0x0000e5c1

Master0
  Slaves: 1
    Main: 02:ec:51:4d:00:01 (attached)
      Link: UP
      Rx frames: 1102   Lost frames: 0
```

IgH 内核日志（2026-11-12 18:56 班，DL Status 修正后；`dmesg` 本次扫描段）：

```
EtherCAT 0: 1 slave(s) responding on main device. Re-scanning on next possibility.
EtherCAT 0: Scanning bus.
EtherCAT 0: Bus scanning completed in 6 ms.
EtherCAT 0: No DC reference clock found.
```

**没有** `Failed to calculate bus topology`（修正前该行必现，见 §4 第 4 条）。

应答器侧统计（`--verbose` 每 50 帧打印一次）：`in=1100 out=1094 dropped=6 hits=1094`，
`al=0x02`（PREOP），说明主站 AL 握手被正确处理。

### 3.1 过程数据预回归：未修改 SOEM `simple_ng` 进入 OP（2026-11-12 19:26 班）

SII 增加 **SM 类别 0x0029 + TxPDO 0x0032 + RxPDO 0x0033**（8 位输入 / 8 位输出通用画像）后，
SOEM 在无 CoE/mailbox 时回退到 SII PDO 类别，配置 SM2/SM3 与 FMMU，并完成 SAFEOP→OP：

```
Initializing SOEM on 'escsim0'... done
Finding autoconfig slaves... 1 slaves found
Sequential mapping of I/O... mapped 1O+1I bytes from 1 segments
Waiting for all slaves in safe operational... done
Setting operational state... all slaves are now operational
Iteration    1:   117 usec  WKC 3  O: 00  I: 00  T: 0
...
```

- WKC 3/轮 = 输出 FMMU +2、输入 FMMU +1（IgH `master/datagram_pair.c` 与 SOEM
  `src/ec_main.c` 对 LRW/LWR 的期望一致）；1055 轮全部 `WKC 3`、0 次 `wrong`，
  应答器侧 `al=0x08`（OP）。
- `slaveinfo` 同步显示 `Output size: 8bits` / `Input size: 8bits` / `State: 4`、
  `SM2 A:1100 L:1 F:00010004 Type:3`、`SM3 A:1110 L:1 F:00010000 Type:4`、
  `FMMU0 ... Ty:02 Act:01`。
- 复现脚本：[soem-process-data.sh](../../scripts/spikes/soem-process-data.sh)
  （断言 1 从站 / 1O+1I / OP / 每轮 WKC=3）。
- IgH 因无 mailbox/CoE 仍停在 PREOP（`0  0:0  PREOP  +`），属 B7 范围。

## 4. 过程中发现并修正的四个真问题

1. **以太网填充**：IgH 主站把短 EtherCAT 帧补零到 60 字节（`Tx bytes / Tx frames = 60`），
   帧头声明长度小于实际长度。严格"声明长度==实际长度"的校验会**静默丢弃全部 IgH 帧**
   （现象：主站 `Rx frames: 0`、全部 datagram TIMEOUT，而 SOEM 正常）。
   应答器现在接受"声明长度之后全是填充"的帧；新增单测覆盖。
2. **通用类别必须 32 字节**：IgH `ec_slave_fetch_sii_general()` 硬性要求 `data_size == 32`，
   否则报 `Wrong size of general category (16/32)` 并把从站标错（`E`）。
3. **自增地址方向错误**（修复 `wire::auto_increment_step`）：主站把 ADP 写成环位置的相反数
   （IgH `master/datagram.c`：`(int16_t) ring_position * (-1)`；SOEM `src/ec_config.c`：
   `ADPh = (uint16)(1 - slave)`），每个从站转发前把 ADP **加 1**，收到 0 的从站执行。
   原实现"ADP==1 执行、转发减 1"会整体偏移一个从站号，已按源码证据修正并更新单测。
4. **DL Status（0x0110）端口位错位**（2026-11-12 18:56 班修复）：IgH 内核日志
   `Failed to calculate bus topology`（从站列表/状态本身正常、无 `E` 标志）。
   - 真实位布局（ETG.1000.4 table 34，经 IgH `master/fsm_slave_scan.c: ec_fsm_slave_scan_state_datalink()`
     与 SOEM `src/ec_config.c: ecx_config_init()` 的解析交叉复核）：
     bit0–2 DL user operational/watchdog/extended link detection、bit3 保留；
     bit(4+i) 端口 i 物理链路；bit(8+2i) 端口 i `loop_back`；bit(9+2i) 端口 i `signal_detection`。
   - IgH `ec_master_calc_topology_rec()` 按端口 **3 → 1 → 2** 顺序检查 `loop_closed`
     （`next_table = {3, 2, 0, 1}`）：`loop_closed == 0` 表示该端口后还有从站，单从站时
     递归越界即返回 -1 → 打印上述错误。旧值 `0x503F` 把端口 1 报成"环回开放"（bit10=0），故必现。
   - 修正为单从站正确值 `0x5610`：端口 0 链路 up + 有信号，端口 1–3 环回闭合、无信号；
     `esc-node::dl_status_value()` 按位拼装，单测复刻了 IgH 拓扑判定与 SOEM 的
     `topology/activeports` 提取。修正后同一脚本下错误消失（见 §3 证据），SOEM `slaveinfo`
     （`1 slaves found and configured`、`Activeports:1.0.0.0`）与 IgH `ethercat slaves`
     均无回归。

## 5. 已知限制 / ⚠️ 待验证

- ~~**拓扑计算仍报错**~~ ✅ **2026-11-12 18:56 班已修**：DL Status 端口位按 ETG.1000.4 table 34
  修正（见 §4 第 4 条），IgH `Failed to calculate bus topology` 消失。
  ⚠️ 仅覆盖**单从站**拓扑；多从站链/分支的端口位语义（各从站 `loop_closed` 应按实际拓扑）
  留到 B8 有 2 个以上从站时验证。
- **SII 固定区 CRC-8 已实现（2026-11-12 晚班，B6 子项）**：CRC 在 **word 0x0007**，
  覆盖 word 0x0000–0x0006（14 字节），多项式 `x^8 + x^2 + x + 1`（0x07）、初值 0xFF、MSB-first；
  早期文档把位置记成 word 0x003F，经 SOEM `eepromtool`（`SIIcrc()` 写 word 0x0007、`-i` 同屏打印
  `Checksum`/`calculated`）与 IgH `tool/sii_crc.cpp`（`pycrc --poly=0x07 --xor-in=0xff`）复核更正
  ——**0x003F 是 SII 版本（=1）**。镜像同时补 word 0x003E=1（声明 256 字节）与 word 0x003F=1。
  验证证据：未修改的 SOEM `eepromtool -i` 对 TAP 上的应答器输出
  `Checksum: 0030` / `calculated: 0030`、`Size: 0001 = 256 bytes`、`Version: 0001`；
  同时 `slaveinfo` 与 IgH `ethercat slaves` 无回归。仍属"预回归"，**不构成**一致性测试/认证结论。
- ~~**未进 OP**~~ ✅ **SOEM 已进 OP（2026-11-12 19:26 班）**：SII SM/TxPDO/RxPDO 类别 + FMMU
  逻辑寻址 + LRW WKC 修正后，未修改 SOEM `simple_ng` 进入 OP 并做 8 位 PDO 交换（§3.1）。
  **IgH 仍停在 PREOP**：需要 mailbox + CoE SDO 才能配置 PDO 域，属 B7；当前 SII 未声明 mailbox。
- **输入/输出数据是静态的**：模拟器没有 PDI/应用层，输入恒 0；SM2/SM3 的 buffered/SM 握手语义、
  邮箱、watchdog 均未实现。
- **SII 暂无 FMMU 类别 0x0028 与 DC 类别**：SOEM `FMMUfunc` 仍显示 `0:0`；
  ⚠️ 待验证其确切字段语义（不凭记忆猜）后再补。
- **非认证**：以上只是"用例映射/预回归"，**不构成**一致性测试或 CTT 认证结论。
- 验证机侧模块加载/卸载顺序、TAP MAC 等运维细节见本地 `.local/ops.md`（不入库）。

## 6. 下一步

- **B7 CoE 最小闭环**：SII 声明 mailbox（SM0/SM1 + 固定区 0x0018 起）与 CoE 协议位，实现邮箱
  数据链路（SM 状态位、0x1000/0x1080 缓冲区）；先对未知对象回 SDO Abort（0x06020000）证明
  链路通，再补对象字典（0x1C12/0x1C13/0x1600/0x1A00、0x1018）让 **IgH** 也能配置 PDO 域并进 OP；
- **B8 双主站冒烟**：SOEM 已零补丁进 OP（§3.1，断言脚本 `soem-process-data.sh`）；待 IgH 进 OP 后
  把两主站证据合并为最终冒烟记录；
- DL Status 将在**多从站**场景下按实际拓扑继续验证（单从站已 ✅）。
