# CiA 402 状态机骨架（Tier0 行为插件）— 实现说明

- 状态机逻辑：`crates/esc-node/src/cia402.rs`（`esc_node::cia402`）
- 运行时接入：`esc-node::Responder::with_cia402_behavior(out_offset, in_offset)`；
  `ec-sim-esc --behavior cia402` 启用（SM2 控制字 `0x6040` → SM3 状态字 `0x6041`，
  各 2 字节小端；偏移取 manifest 的 SM2(dir `out`)/SM3(dir `in`) 起始地址）。
- 行为在 `Responder::tick()`（每收到一帧前）执行一次，因此主站读到的是**上一帧**写入
  控制字对应的状态字（一帧延迟，Tier0 简化）。
- 验证：`scripts/spikes/cia402-behavior-probe.sh`（合成主站 UDS；断言
  `0x0006→0x0221`、`0x0007→0x0223`、`0x000F→0x0237|0x0400`、禁用电压回 `0x0240`）。

## 完整 7 字节 PDO（2026-11-12 23:26 班起）

当 manifest 的 SM2/SM3 长度 ≥ 7（通用 CiA 402 画像即 7 字节）时，行为同时处理完整
PDO 数据：

- **RxPDO（SM2，7 字节）**：`0x6040` 控制字（u16，+0）、`0x6060` 模式（i8，+2）、
  `0x607A` 目标位置（i32，+3）。
- **TxPDO（SM3，7 字节）**：`0x6041` 状态字（u16，+0）、`0x6061` 模式显示（i8，+2，
  镜像最近一次 `0x6060`）、`0x6064` 实际位置（i32，+3）。

Tier0 简化行为：`Operation enabled` 时**实际位置直接跟随目标位置**（无运动环/限幅），
并置状态字 bit10 `Target reached`；禁用电压后位置保持最后值。SM 长度 < 7 时退化为只读写
状态字 2 字节（用于 8DO 等非伺服画像）。

## 模式与错误注入（Tier0）

- **模式门控**：只有 **CSP（模式 8）** 在 `Operation enabled` 时让实际位置跟随目标并置
  `Target reached`；其它模式（如 CSV=9、CST=10）位置冻结、不置 `Target reached`
  （当前 7 字节 PDO 不含速度/转矩目标，无法模拟这些模式）。`0x6061` 始终镜像 `0x6060`。
- **错误注入**：`Responder::inject_cia402_fault(error_code)` 让状态机进入 `Fault`
  （状态字 `0x0208`），并把错误码写入对象 `0x603F`（经 SDO 可读回）；控制字 bit7
  `Fault reset` 使状态机回到 `Switch on disabled`（`0x0240`），**错误码保留**
  ⚠️（Tier0 简化：未模拟真机"错误码清除"时序）。

⚠️ 以上是**模拟器行为**，不是真实伺服运动模型；不构成一致性结论。

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
