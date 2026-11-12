# CiA 402 状态机骨架（Tier0 行为插件基础）— 实现说明

- 代码：`crates/esc-node/src/cia402.rs`（`esc_node::cia402`）
- 状态：**Tier0 骨架**（只有状态机，尚未接入运行时 PDO 数据）

## 采用的事实（CiA 402 公开 profile）

控制字 `0x6040` 位（本实现用到的子集）：

| 位 | 名称 | 说明 |
| --- | --- | --- |
| bit0 `0x0001` | Switch on | |
| bit1 `0x0002` | Enable voltage | |
| bit2 `0x0004` | Quick stop | **低有效** |
| bit3 `0x0008` | Enable operation | |
| bit7 `0x0080` | Fault reset | 上升沿（Tier0 按电平处理） |

状态字 `0x6041` 位（本实现）：

| 位 | 名称 |
| --- | --- |
| bit0 `0x0001` | Ready to switch on |
| bit1 `0x0002` | Switched on |
| bit2 `0x0004` | Operation enabled |
| bit3 `0x0008` | Fault |
| bit4 `0x0010` | Voltage enabled |
| bit5 `0x0020` | Quick stop（低有效） |
| bit6 `0x0040` | Switch on disabled |
| bit7 `0x0080` | Warning |
| bit9 `0x0200` | Remote（本实现恒置 1） |

状态编码（状态字 & 0x4F 相关位）：

| 状态 | 状态字（不含 bit9） |
| --- | --- |
| Switch on disabled | `0x40` |
| Ready to switch on | `0x21` |
| Switched on | `0x23` |
| Operation enabled | `0x37`（含 bit4） |
| Quick stop active | `0x07` |
| Fault | `0x08` |
| Fault reaction active | `0x0F` |

已实现转换（`Cia402StateMachine::update(cw)`）：

- Switch on disabled —Shutdown(`0x06`)→ Ready to switch on
- Ready to switch on —Switch on(`0x07`)→ Switched on
- Switched on —Enable operation(`0x0F`)→ Operation enabled
- Operation enabled —Shutdown(`0x06`)→ Ready to switch on
- Operation enabled —Quick stop(bit2=0)→ Quick stop active
- Operation enabled / Ready / Switched on —Disable voltage(bit1=0)→ Switch on disabled
- Quick stop active —Disable voltage / Fault reset→ Switch on disabled
- Fault —Fault reset(bit7)→ Switch on disabled
- `inject_fault()`：Fault reaction active → Fault（Tier0 不做反应延时）

## ⚠️ 未实现（下一班/后续）

- **PDO 数据通路**：尚未把状态机接到运行时过程数据（SM2 `0x1600` 的 controlword →
  SM3 `0x1A00` 的 statusword）；当前只是纯逻辑模块。
- **Ti=0/Ti=1 时序**：真机状态机对命令有"至少一个周期"的时序要求，本骨架按电平即时转换。
- **模式相关位**（bit4–6 new set-point / change set immediately / halt）与模式处理
  （CSP/CSV/CST）未实现；`0x6060`/`0x6061` 仅作为对象透传。
- **错误码 `0x603F` 触发条件**、警告位、target reached / internal limit（上层决定）未实现。
- **Fault reset 上升沿检测**：按电平处理。

## 验证

`cargo test -p esc-node cia402`（6 个单测：初始状态、标准使能序列、Operation enabled
shutdown 回 Ready、quick stop/disable voltage 路径、故障+复位恢复、无效控制字保持状态）。

本文件与代码里的位定义来自 CiA 402 公开 profile（伺服驱动 profile），不包含厂商专有内容；
不构成任何一致性测试结论（R3）。
