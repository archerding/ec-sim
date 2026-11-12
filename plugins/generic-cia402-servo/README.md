# Generic CiA 402 servo（SIMULATED）— 声明式设备插件

通用 **CiA 402 位置/速度伺服**的最小声明式画像（B11 第一步）：用于验证"声明式画像 +
双主站闭环"，也是后续 Inovance SV660N 等具体兼容画像的基线。

- 身份：自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5E0`（**不是**任何厂商的
  Vendor ID/Product Code；名称带 `SIMULATED`）。
- SII：SM0/SM1 = CoE 邮箱（各 128 字节）、SM2 = RxPDO 7 字节、SM3 = TxPDO 7 字节；
  `0x1600`（controlword `0x6040` + modes `0x6060` + target position `0x607A`）、
  `0x1A00`（statusword `0x6041` + modes display `0x6061` + position actual `0x6064`）。
- 对象字典：上述 PDO 对象 + `0x603F` 错误码、`0x6077` 实际转矩、`0x1008` 设备名
  （VISIBLE_STRING，经分段 SDO 上传）。
- 事实来源与"未模拟内容"见 [facts.md](facts.md)；验证方法见
  [docs/plugins/generic-cia402-verification.md](../../docs/plugins/generic-cia402-verification.md)。

## 已模拟 / 未模拟

- ✅ SII（身份/邮箱/SM/PDO）、CoE 对象字典、SDO 读写/分段、SDO Info、AL 状态机
  INIT→PREOP→SAFEOP→OP、SM 看门狗。
- ❌ **CiA 402 状态机行为**（状态字/控制字转换）、运动控制环、编码器反馈、Homing、
  DC 分布式时钟、厂商专有对象；PDO 数据为静态初值。

⚠️ 本插件是"兼容仿真画像"，**不声称**与任何厂商设备等价或通过一致性测试。

## 使用

```bash
# Linux（TAP）
sudo cargo run --release -p esc-node --bin ec-sim-esc -- \
  --tap escsim0 --verbose --manifest plugins/generic-cia402-servo/plugin.yaml
```

验证脚本：`scripts/spikes/generic-cia402-probe.sh`（SOEM `slaveinfo`/`simple_ng` +
IgH `ethercat slaves`）。
