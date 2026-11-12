# Inovance SV660N 兼容伺服（SIMULATED）— 声明式设备插件

B11 的**第一个具体型号目标**：以 Inovance SV660N 为兼容对象的 SIMULATED 画像。
当前为 **Tier0 通用 DS402 布局**（具体型号默认 PDO/扩展对象 ⚠️ 待按公开手册核实）。

- 身份：自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5F1`（**不使用** Inovance 的
  Vendor ID/Product Code；名称带 `SIMULATED`）。
- SII：SM0/SM1 = CoE 邮箱（128 B）、SM2/SM3 = 各 7 字节；RxPDO `0x1600` =
  controlword `0x6040` + modes `0x6060` + target position `0x607A`；TxPDO `0x1A00` =
  statusword `0x6041` + modes display `0x6061` + position actual `0x6064`。
- 对象：上述 DS402 标准对象 + `0x603F` 错误码、`0x6077` 实际转矩、`0x1008` 设备名。
- 事实来源与 ⚠️ 待核实项见 [facts.md](facts.md)；验证记录见
  [docs/plugins/inovance-sv660n-verification.md](../../docs/plugins/inovance-sv660n-verification.md)。

## 已模拟 / 未模拟

- ✅ SII/邮箱/SM/PDO、CoE 对象字典、SDO（含分段）、SDO Info、AL 状态机、SM 看门狗；
  `--behavior cia402` 提供 Tier0 状态机 + 7 字节 PDO 行为（通用 DS402）。
- ❌ 厂商专有对象/默认映射差异（⚠️ 待核实）、CiA 402 完整状态机时序、运动环/编码器/
  Homing、DC、故障码表。

⚠️ 本插件是兼容仿真画像，**不声称**与 Inovance 有任何隶属/背书/认证关系，也不构成
一致性测试结论。R1：未把厂商 ESI/PDF/固件/示例代码入库。

## 使用

```bash
sudo cargo run --release -p esc-node --bin ec-sim-esc -- \
  --tap escsim0 --verbose --manifest plugins/inovance-sv660n/plugin.yaml --behavior cia402
```

验证：`scripts/spikes/generic-cia402-probe.sh`（设 `MANIFEST=plugins/inovance-sv660n/plugin.yaml`
与 `EXPECT_PID=0000e5f1`）。
