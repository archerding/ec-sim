# Inovance SV660N 兼容画像 — 事实来源与边界

本插件是 **"Inovance SV660N 兼容伺服的 SIMULATED 画像"**（B11 第一个具体型号目标）。
当前只声明**通用 DS402 默认布局**，具体型号细节标注 ⚠️ 待核实——**不冒充厂商 ESI/固件**。

## 采用/引用的公开来源

| 来源 | 用途 | 链接 |
| --- | --- | --- |
| Inovance 官网产品页 | 确认 SV660N 为 EtherCAT 总线伺服（型号存在性） | https://www.inovance.com/ |
| Inovance SV660N Servo Communication Guide（公开手册） | EtherCAT 通讯/对象字典/PDO 结构的公开入口（**未入库**，仅引用） | https://idea-tech.in/wp-content/uploads/2024/04/INOVANCE-SV660N-SERVO-COMMUNICATION-GUIDE-ENGLISH-PAGE-292-06-04-2024.pdf |
| SV660N Commissioning Guide（公开手册） | 同上（**未入库**，仅引用） | https://documents.mrosupply.com/product_documents/64/86/6486865/SV660N_Commissioning_Guide_EN_C00_19011433_r3R1eXs.pdf |
| CiA 402（公开规范摘要） | DS402 对象索引/控制字/状态字/模式语义 | https://www.can-cia.org/cia-402 |

> **R1**：以上厂商 PDF 只作为阅读入口记录 URL，**没有**把 ESI/PDF/固件/示例代码复制进仓库。
> **R2**：设备身份使用我方自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5F1`，不使用
> Inovance 的 Vendor ID/Product Code。**R3**：不声称隶属/背书/认证。

## 当前声明的事实（通用 DS402，非厂商确认）

- `0x6040` 控制字 / `0x6041` 状态字、`0x6060`/`0x6061` 模式、`0x607A` 目标位置、
  `0x6064` 实际位置、`0x603F` 错误码、`0x6077` 实际转矩（CiA 402 标准对象）。
- SM0/SM1 = CoE 邮箱（128 B），SM2/SM3 = 各 7 字节过程数据。
- RxPDO `0x1600` = controlword + modes + target position；TxPDO `0x1A00` =
  statusword + modes display + position actual（Tier0 通用布局）。

## ⚠️ 待核实（不要当成厂商事实）

1. **SV660N 默认 PDO 映射**：公开手册里的默认 RxPDO/TxPDO 的索引、长度与条目顺序
   可能不同（例如包含 target velocity `0x60FF`、更长的 9 字节布局等）；本画像暂用
   通用 7 字节布局，**未逐项确认**。
2. **厂商扩展对象**（如 `0x2000` 段参数）：未收录，需要按公开手册核对后补充。
3. **DC 参数**：SV660N 通常启用 DC；本画像 `dc.supported=false`，DC 参数未核实。
4. **错误码语义**（`0x603F` 具体故障码表）与状态机时序（Ti=0/1）：未实现。

## 后续步骤

1. 在可获取公开手册文本的条件下，逐项核对默认 PDO/扩展对象并更新 manifest（仍用
   我方 VID/PID）。
2. 行为插件（Tier0）已按通用 CiA 402 画像实现（`--behavior cia402`），可直接用于本画像
   的扫描/OP 验证；模式切换（CSP/CSV/CST）与错误码注入待后续。
