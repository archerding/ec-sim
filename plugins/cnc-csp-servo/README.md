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

## 已知 lcec 行为（2026-11-13，CNC 侧实测反馈）

- lcec `basic_cia402` **默认 `enable_pv=1`**：即使 XML 只写 `enableCSP`，它也会把 RxPDO 配成
  11B（多 `0x60FF` target velocity）并对 `0x60FF` 做一次 init CoE SDO upload。
- 本画像的默认 PDO **仍是 CSP 7B/11B**；但已把 `0x60FF`（INT32, rw）放进 OD（**不进默认 PDO**），
  使该 SDO upload 可读（避免 `error -5`）。CNC 侧仍建议 XML 显式 `enablePV=false` 保持 7B。
- V1 单从站 OP 已按本画像在 CNC 验证机通过（CNC 侧证据；本仓库只引用事实，见
  [对接文档 §4.1](../../docs/plan/CNC-V1-integration.md)）。

## 与 lcec 配置的对应

CNC 侧 lcec XML 用 `basic_cia402` 且不写 `<dcConf>`；条目必须真实出现在 0x1600/0x1A00 映射对象里
（lcec 按从站映射对象读回匹配）。布局核对来源与结论见
[docs/plan/CNC-V1-integration.md](../../docs/plan/CNC-V1-integration.md)。

## 验证

- `crates/plugin-host/tests/plugin_manifests.rs` 守卫：布局/`0x6502`/映射对象一致性；
- `scripts/spikes/cia402-modes-probe.sh`（SDO 上传 `0x6502`）；
- TAP 多从站：`scripts/spikes/tap-cia402-multi-slave-probe.sh`（SOEM，EC-Sim 验证机）。

⚠️ Tier0 行为：CSP/PP 都是简化语义（无轨迹/动态/单位换算），仅用于闭环数据通路验证。
