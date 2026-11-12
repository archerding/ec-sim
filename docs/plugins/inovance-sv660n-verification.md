# Inovance SV660N 兼容画像（SIMULATED）— 验证记录

- 插件：`plugins/inovance-sv660n/`（plugin.yaml + facts.md + README.md）
- 身份：自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5F1` / Rev `0x00010000`
  （**不使用** Inovance 的 Vendor ID/Product Code；名称带 `SIMULATED`）
- 验证日期：2026-11-12（23:34 班）
- 验证环境：专用 Linux 验证机（TAP + 未修改 SOEM / IgH 1.6.13）
- 复现：

```bash
sudo MANIFEST=plugins/inovance-sv660n/plugin.yaml EXPECT_PID=0000e5f1 \
  bash scripts/spikes/generic-cia402-probe.sh
```

## 结果（脚本 EXIT=0）

### SOEM `slaveinfo`

```
 Output size: 56bits
 Input size: 56bits
 Man: 00000ec5 ID: 0000e5f1 Rev: 00010000
 MBX length wr: 128 rd: 128 MBX protocols : 04
```

### SOEM `slaveinfo -map`（摘录）

```
  [0x0000.0] 0x6040:0x00 0x10 UNSIGNED16   Controlword
  [0x0007.0] 0x6041:0x00 0x10 UNSIGNED16   Statusword
  [0x000A.0] 0x6064:0x00 0x20 INTEGER32    Position actual value
```

### SOEM `simple_ng`（零补丁）

```
Sequential mapping of I/O... mapped 7O+7I bytes from 1 segments
Waiting for all slaves in safe operational... done
Setting operational state... all slaves are now operational
Iteration    1:    99 usec  WKC 3  O: 00 00 00 00 00 00 00  I: 00 00 00 00 00 00 00  T: 0
```

### IgH 扫描

```
0  0:0  PREOP  +  0x00000ec5:0x0000e5f1
```

（IgH 无 ecrt 应用时只配置到 PREOP；OP + 行为验证见通用画像的
`igh-cia402-op-probe.sh`，本画像布局相同。）

## 结论

- 声明式 manifest 生成的 SII（身份/邮箱/SM/PDO）被未修改 SOEM 正确识别：
  56/56 位映射、`0x6040/0x6041/0x6064` 可见、`simple_ng` 零补丁进入 **OP**（WKC 3）；
  IgH 正确显示身份与 PREOP。
- `--behavior cia402` 的 Tier0 行为按通用 CiA 402 布局工作（本画像 SM2/SM3 均为 7 字节，
  与已通过 `cia402-behavior-probe.sh` 的通用画像一致）。

## ⚠️ 重要限制（详见 facts.md）

- 本画像当前使用**通用 DS402 默认布局**；**SV660N 具体默认 PDO 映射/扩展对象/DC 参数
  尚未逐项核实**（公开手册未入库，仅记录 URL）。不要把它当作厂商等价物。
- 行为无运动环/编码器/模式切换/Ti 时序；厂商故障码表未实现。
- 本记录只证明画像/SII/双主站扫描与 OP 闭环，不构成任何一致性测试结论（R3）。

## 公开实测事实补充（2026-11-13）

来源：[linuxcnc-ethercat `documentation/inovance.md`](https://github.com/linuxcnc-ethercat/linuxcnc-ethercat/blob/master/documentation/inovance.md)
（公开文档，只读参考；通过镜像取得文本，未复制代码/文件）。据此把以下事实写入
`plugins/inovance-sv660n/facts.md` 并更新 manifest：

- SV660N（SV660 EtherCAT 变体）产品码 `0x000c010d`，厂商 Vendor ID `0x00100000`
  （**我方画像仍用自有 VID/PID**，R2）。
- 每方向只有 **一个** PDO：RxPDO `0x1600` / TxPDO `0x1A00`；`0x1C13:01` 可写
  `0x1A00` 或固定映射 `0x1B01..0x1B04`；无 `0x1A01`。
- `0x1600`/`0x1A00` 最多 **10** 个映射条目；子索引 `0x0B` 写入以 `0x06090011` 中止。
- 设备**不支持 SDO Info**（`Enable SDO Info: no`）→ 画像改为 `coe.sdo_info: false`。
- DC 默认开启（`assignActivate 0x300`，SYNC0 跟应用周期）——模拟器仍未实现 DC，
  `dc.supported=false` 保持不变（⚠️ 能力缺口）。
- `0x6502 = 0x3AD`（pp/pv/tq/hm/csp/csv/cst）→ 画像新增该对象。

## 验证命令（更新后）

```bash
MANIFEST=plugins/inovance-sv660n/plugin.yaml \
EXPECT_VID=00000ec5 EXPECT_PID=0000e5f1 \
EXPECT_OUTPUT_BITS=56 EXPECT_INPUT_BITS=56 EXPECT_MAPPED=7O+7I \
bash scripts/spikes/generic-cia402-probe.sh
```

- 预期：SOEM 扫描身份 `00000ec5:0000e5f1`、56/56 bits、`mapped 7O+7I` 进 OP、WKC 3；
  IgH PREOP + 身份 `0x00000ec5:0x0000e5f1`。
- ⚠️ 默认 PDO 的**具体条目/顺序**仍未知（第三方文档只给出上限与分配），画像继续使用
  Tier0 通用 7 字节布局并标注待核实。

## 能力位/PDO 复核（SOEM + IgH，2026-11-13 02:21 班）

```bash
sudo bash scripts/spikes/sv660n-capability-probe.sh
```

结果（Linux 验证机 **EXIT=0**）：

- SOEM `slaveinfo`：`Man: 00000ec5 ID: 0000e5f1`、`Output size: 56bits` /
  `Input size: 56bits`、`MBX protocols : 04`、`CoE details: 0d`
  （bit0 SDO=1、**bit1 SDO Info=0**、bit2/3 PDO assign/config=1）——与公开事实
  "设备不响应 SDO Information" 一致；
- `Has DC: 1` —— 画像**声明 DC**（真实 SV660N 默认启用 DC：`assignActivate 0x300`）。模拟器
  提供 **Tier0 帧驱动模拟时钟**（0x0900–0x0917 随帧前进，见 `dc-clock-probe.sh`），但
  ⚠️ **无真实 SYNC0 脉冲/漂移补偿/传播延迟测量**；
- IgH `ethercat slaves`：`PREOP + 0x00000ec5:0x0000e5f1`。
## SOEM DC 配置路径（2026-11-13 02:46 班）

```bash
sudo bash scripts/spikes/soem-dc-probe.sh
```

结果（Linux 验证机 **EXIT=0**）：

- SOEM `simple_ng`：`mapped 7O+7I bytes`、`Configuring distributed clock... done`、
  `all slaves are now operational`、`WKC 3` 稳定；`T:` 时间列随模拟时钟递增；
- 应答器 `--verbose` 统计：`dc_rd=5300 dc_wr=5302`（SOEM 每周期读 0x0910 系统时间、
  DC 配置阶段写激活/启动/周期寄存器）；
- ⚠️ 这是**路径验证**（DC 寄存器可读可写、主站不报错、仍进 OP），不是 DC 同步精度验证。
## IgH ecrt 应用（CSP + DC，2026-11-13 02:57 班）

```bash
sudo bash scripts/spikes/igh-sv660n-op-probe.sh
```

ecrt 夹具（验证机本地、不入库）按 SV660N 画像的 Tier0 7 字节 PDO 配置，启用 DC
（`ecrt_slave_config_dc(0x0300, 1ms)`）并跑 CSP（模式 8、目标位置 12345）：

- 应用运行期间 `ethercat slaves`：`0  0:0  OP  +  0x00000ec5:0x0000e5f1`；
- `RESULT al_states=0x08 slaves_responding=1 link_up=1`；
- `STATUS sw=0x0637 modes_display=8 pos=12345`（Operation enabled + Target reached，
  位置跟随正确）；
- 应答器 `STATS ... dc_rd=3 dc_wr=7 al=0x08`。
- ⚠️ 默认 PDO 具体条目仍是 Tier0 通用布局（非厂商默认映射）；DC 仅配置路径验证。

## 2026-11-13 03:46 更新：默认 PDO 已按公开手册核对

> 本节**取代**上文早期"Tier0 7 字节通用布局"的临时画像说明；早期记录保留为历史。

公开手册（SV660N Servo Communication Guide）文本已本地提取核对（PDF **未入库**），据此：

- **默认可变映射**已写入 `plugins/inovance-sv660n/plugin.yaml`：
  - RPDO `0x1600` = `6040h`(16) + `607Ah`(32) + `60B8h`(16) → **8 字节**；
  - TPDO `0x1A00` = `603Fh`(16) + `6041h`(16) + `6064h`(32) + `60BCh`(32) + `60B9h`(16) +
    `60BAh`(32) + `60FDh`(32) → **22 字节**；
  - 默认映射**不含** `6060h` 模式对象（模式经 SDO 或固定映射）。
- 手册另有固定映射 `0x1701..0x1705`（RPDO）与 `0x1B01..0x1B04`（TPDO），条目/字节数已记录在
  [facts.md](facts.md)；Tier0 画像暂不生成固定映射对象。
- 验证（2026-11-13 03:46 班，Linux 验证机）：
  - `sv660n-capability-probe.sh` **EXIT=0**：SOEM `Output size: 64bits` / `Input size: 176bits`、
    `Has DC: 1`、`CoE details: 0d`、`MBX protocols : 04`；IgH PREOP + 身份 `0xe5f1`；
  - `igh-sv660n-op-probe.sh` **EXIT=0**：IgH ecrt 按默认映射（8B/22B）配置 + DC
    （0x0300/1ms）仍进 OP：`al_states=0x08`、`slaves_responding=1`、`dc_wr=8`；
  - ⚠️ 默认映射不含模式对象，`--behavior cia402` 的行为验证改用通用 CiA 402/CST/CSV 画像。

**仍待核实**：固定映射对象的逐项位长（表里位长按 CiA 402 标准对象推算）、厂商扩展对象、
`0x603F` 故障码表。