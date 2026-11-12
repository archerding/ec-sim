# 通用 CiA 402 伺服画像（SIMULATED）— 验证记录

- 插件：`plugins/generic-cia402-servo/`（manifest + facts.md + README.md）
- 身份：自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5E0` / Rev `0x00010000`
  （**不是**任何厂商的 VID/PID；名称带 `SIMULATED`）
- 验证日期：2026-11-12（23:12 班）
- 验证环境：专用 Linux 验证机（openEuler 24.03 / kernel 6.6，TAP + 未修改 SOEM / IgH 1.6.13）
- 复现：`sudo bash scripts/spikes/generic-cia402-probe.sh`

## 结果（脚本 EXIT=0）

### SOEM `slaveinfo`

```
 Output size: 56bits
 Input size: 56bits
 Man: 00000ec5 ID: 0000e5e0 Rev: 00010000
 MBX length wr: 128 rd: 128 MBX protocols : 04
```

### SOEM `slaveinfo -map`（CoE 读取 PDO 映射；摘录）

```
  [0x0000.0] 0x6040:0x00 0x10 UNSIGNED16   Controlword
  [0x0007.0] 0x6041:0x00 0x10 UNSIGNED16   Statusword
  [0x000A.0] 0x6064:0x00 0x20 INTEGER32    Position actual value
```

（PDO 结构：RxPDO `0x1600` = `0x6040` 16bit + `0x6060` 8bit + `0x607A` 32bit = 56 bit；
TxPDO `0x1A00` = `0x6041` 16bit + `0x6061` 8bit + `0x6064` 32bit = 56 bit。）

### SOEM `simple_ng`（零补丁，进 OP）

```
Sequential mapping of I/O... mapped 7O+7I bytes from 1 segments
Waiting for all slaves in safe operational... done
Setting operational state... all slaves are now operational
Iteration    1:   131 usec  WKC 3  O: 00 00 00 00 00 00 00  I: 00 00 00 00 00 00 00  T: 0
```

### IgH 扫描

```
0  0:0  PREOP  +  0x00000ec5:0x0000e5e0
```

（IgH 无 ecrt 应用时只配置到 PREOP；OP 用例见 `igh-el2008-op-probe.sh` 的夹具方式。）

## 结论

- 声明式 manifest 生成的 SII（身份/邮箱/SM/PDO）被未修改的 SOEM 正确识别：
  56/56 位映射、CoE 邮箱、RxPDO/TxPDO 对象名可见，`simple_ng` 零补丁进入 **OP**。
- IgH 正确显示身份与 PREOP。
- **本画像不包含 CiA 402 行为**：状态机、运动控制、编码器反馈、DC 均未实现（见
  `plugins/generic-cia402-servo/facts.md` 的"明确不模拟"清单）。本记录只证明
  **画像/SII/双主站扫描与 OP 闭环**，不构成任何一致性测试结论。

## ⚠️ 待验证 / 后续

- IgH 侧进入 OP：需按 `igh-el2008-op-probe.sh` 的方式写一个匹配本画像的 ecrt 夹具
  （7O+7I 的 domain + SDO 上传设备名）。
- CiA 402 状态机骨架（Tier0 行为插件）与 PDO 动态数据：下一步。
- 具体厂商兼容画像（Inovance SV660N 等）：在通用画像上按公开手册补默认 PDO/对象。
