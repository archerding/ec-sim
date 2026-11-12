# 通用 CiA 402 伺服画像 — 事实来源与边界

本插件是 **"通用 CiA 402 伺服（SIMULATED）"** 的声明式画像（B11 第一步），不绑定任何
具体厂商型号；后续的 Inovance SV660N 等兼容画像会在此基础上按公开资料派生。

## 采用的事实（全部来自公开规范/公开手册）

| 事实 | 值 | 公开来源 |
| --- | --- | --- |
| 控制字 / 状态字对象 | `0x6040` / `0x6041` | CiA 402 伺服驱动 profile（公开规范摘要） |
| 运行模式对象 | `0x6060` / `0x6061`（INT8） | CiA 402 |
| 目标位置 / 实际位置 | `0x607A` / `0x6064`（INT32） | CiA 402 |
| 错误码 | `0x603F`（UINT16） | CiA 402 |
| 实际转矩 | `0x6077`（INT16） | CiA 402 |
| 典型 RxPDO `0x1600` | controlword + modes + target position | 多家公开伺服手册的默认映射（Inovance/Kinco/Parker 等公开手册） |
| 典型 TxPDO `0x1A00` | statusword + modes display + position actual | 同上 |
| CoE 邮箱 | SM0 0x1000 / SM1 0x1080，各 128 字节 | EtherCAT 规范 + 公开手册 |

> 注意：不同厂商/固件版本的默认 PDO 映射与对象可用性差异很大；本画像只声明**通用
> 最小集合**，不作为任何具体型号的等价物。R1：未把任何厂商 ESI/PDF/固件/示例代码入库。

## 明确不模拟的内容（⚠️ 待后续）

- **状态机行为**：CiA 402 状态机（Switch on disabled → Ready → Switched on → Operation
  enabled）与状态字位定义、错误码/故障复位逻辑均未实现，PDO 数据为静态初值。
- **运动控制**：位置/速度/转矩环、编码器反馈、限位、回零（Homing）均未模拟。
- **DC 分布式时钟**：本画像 `dc.supported = false`；真实伺服通常启用 DC。
- **厂商专有对象**：SV660N 等厂商扩展对象（如 0x2000 段）未收录；需要时以公开手册为准。

## 后续步骤

1. 在验证机上扫描/映射/进 OP（`scripts/spikes/generic-cia402-probe.sh`）；
2. 派生 Inovance SV660N 兼容画像：补厂商公开手册中的默认 PDO 映射与关键对象（仍用
   **我方** VID/PID、名称带 SIMULATED）；
3. 行为插件（Tier0）：先实现 CiA 402 状态机骨架（控制字→状态字），PDO 数据从静态
   改为由行为驱动。
