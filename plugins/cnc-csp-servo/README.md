# cnc-csp-servo（SIMULATED）

**用途**：CNC Station 虚拟闭环（LinuxCNC + linuxcnc-ethercat + IgH + TAP）里的 CiA 402 伺服从站画像。
这是**模拟器画像**，不是厂商设备、不是 ESI、不构成任何兼容性/认证结论。

## PDO 布局（V2/V3 用）

| 方向 | 映射对象 | 条目（按字节流顺序） | 字节 |
| --- | --- | --- | --- |
| RxPDO 0x1600（SM2） | 0x6040 U16 → 0x6060 S8 → 0x607A S32 | controlword / modes / target position | 7 |
| TxPDO 0x1A00（SM3） | 0x6041 U16 → 0x6061 S8 → 0x6064 S32 → 0x606C S32 | statusword / modes display / position actual / velocity actual | 11 |

字节序：每个对象小端。`0x6040`/`0x6041` 分别是 Rx/Tx 第一项。
`0x6502`（Supported drive modes）= `0x01C1`（PP bit0 / HM bit5 / IP bit6 / CSP bit7）。

## 为什么多一个 0x606C

lcec 的 `basic_cia402` 在 `enableCSP=1` 时**隐式**要求 TxPDO 含 `0x606C:00`（实际速度）；
V1 只用 PP 模式时 `generic-cia402-servo`（7B/7B）即可，V2 起用本画像（7B/11B）。

## 与 lcec 配置的对应

CNC 侧 lcec XML 用 `basic_cia402` 且不写 `<dcConf>`；条目必须真实出现在 0x1600/0x1A00 映射对象里
（lcec 按从站映射对象读回匹配）。布局核对来源与结论见
[docs/plan/CNC-V1-integration.md](../../docs/plan/CNC-V1-integration.md)。

## 验证

- `crates/plugin-host/tests/plugin_manifests.rs` 守卫：布局/`0x6502`/映射对象一致性；
- `scripts/spikes/cia402-modes-probe.sh`（SDO 上传 `0x6502`）；
- TAP 多从站：`scripts/spikes/tap-cia402-multi-slave-probe.sh`（SOEM，EC-Sim 验证机）。

⚠️ Tier0 行为：CSP/PP 都是简化语义（无轨迹/动态/单位换算），仅用于闭环数据通路验证。
