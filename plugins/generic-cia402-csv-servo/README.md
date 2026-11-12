# Generic CiA 402 CSV servo（SIMULATED）— 声明式设备插件

B11 的 **CSV（速度模式）** 画像：11 字节 RxPDO/TxPDO、带目标速度 `0x60FF` 与实际速度
`0x606C`。用于验证速度模式行为（Tier0）。

- 身份：自有 VID `0x00000EC5` / PID `0x0000E5E1`（R2），名称带 `SIMULATED`。
- 布局：RxPDO `0x1600` = `0x6040`+`0x6060`+`0x607A`+`0x60FF`；TxPDO `0x1A00` =
  `0x6041`+`0x6061`+`0x6064`+`0x606C`（各 11 字节）。
- 行为：`--behavior cia402` — CSP 模式位置跟随目标；CSV 模式位置每周期累加目标速度
  （⚠️ Tier0 简化，见 [facts.md](facts.md)）。

## 使用与验证

```bash
cargo run --release -p esc-node --bin ec-sim-esc -- \
  --uds /tmp/ecsim.sock --manifest plugins/generic-cia402-csv-servo/plugin.yaml --behavior cia402
./scripts/spikes/cia402-csv-probe.sh     # 合成主站（UDS，无需 root）
```

- ✅ SII/邮箱/SM/PDO、CoE 对象字典、SDO、行为（CSP/CSV Tier0）。
- ❌ 真实运动学/编码器/限位/加减速、DC、厂商专有对象、模式切换时序。

⚠️ 兼容仿真画像，不声称与任何厂商设备等价或通过一致性测试（R3）。
