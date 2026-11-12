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
