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
| linuxcnc-ethercat `documentation/inovance.md`（公开文档，**只读参考**） | SV660N（SV660 EtherCAT 变体）的身份/PDO 上限/PDO 分配/SDO Info/DC 默认值实测事实 | https://github.com/linuxcnc-ethercat/linuxcnc-ethercat/blob/master/documentation/inovance.md |

> **R1**：以上厂商 PDF 只作为阅读入口记录 URL，**没有**把 ESI/PDF/固件/示例代码复制进仓库。
> **R2**：设备身份使用我方自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5F1`，不使用
> Inovance 的 Vendor ID/Product Code。**R3**：不声称隶属/背书/认证。

## 当前声明的事实（通用 DS402，非厂商确认）

- `0x6040` 控制字 / `0x6041` 状态字、`0x6060`/`0x6061` 模式、`0x607A` 目标位置、
  `0x6064` 实际位置、`0x603F` 错误码、`0x6077` 实际转矩（CiA 402 标准对象）。
- SM0/SM1 = CoE 邮箱（128 B），SM2/SM3 = 各 7 字节过程数据。
- RxPDO `0x1600` = controlword + modes + target position；TxPDO `0x1A00` =
  statusword + modes display + position actual（Tier0 通用布局）。

## 公开实测事实（来源：linuxcnc-ethercat `documentation/inovance.md`，访问 2026-11-13；只读参考、未复制代码）

> ⚠️ 这些是第三方公开文档中的**实测/记录事实**，用于提高画像真实性；仍**不是**厂商手册原文。
> 我方设备身份仍使用自有 VID/PID（R2），不冒用下面列出的厂商身份。

- **身份**：SV660 的 EtherCAT 变体即 SV660N，产品码 `0x000c010d`；Inovance 各驱动系列
  共用 Vendor ID `0x00100000`（IS620N `0x000c0108`，与 SV660 仅末字节不同）。
- **PDO 分配**：每方向只有 **一个** PDO——RxPDO `0x1600`、TxPDO `0x1A00`；
  `0x1C13:01` 可写 `0x1A00`（可变映射）或固定映射 `0x1B01..0x1B04`；
  **没有 `0x1A01`**（写它会 SDO 报错且从站无法进 OP）。
- **PDO 上限**：`0x1600`/`0x1A00` 最多 **10 个映射条目**（`rxpdolimit`/`txpdolimit` = 10）；
  子索引 `0x0B` 写入以 `0x06090011` 中止（"subindex does not exist"）。
- **SDO Info**：该设备**不响应 SDO Information 服务**（主站显示 `Enable SDO Info: no`），
  `ethercat sdos` 无输出；读取当前条目数以上的子索引以 `0x06090011` 中止——`0x1600:00`/
  `0x1A00:00` 的计数是唯一可直接读到的信息。本画像因此设置 `coe.sdo_info: false`。
- **DC**：设备默认启用 DC（`assignActivate 0x300`，SYNC0 跟随应用周期）；参考时钟在链上
  **第一个支持 DC 的从站**。
- **支持的模式**：`0x6502 = 0x3AD`（pp, pv, tq, hm, csp, csv, cst）；本画像已加该对象。
- **厂商参数映射**：手册 "Hgg-pp" 参数 = CoE 索引 `0x2000 + 0xgg`、子索引 `0xpp + 1`
  （例：H02-01 = `0x2002:02`）；本画像暂不声明具体厂商对象。

## ⚠️ 待核实（不要当成厂商事实）

1. **SV660N 默认 PDO 映射的具体条目**：已确认每方向仅 `0x1600`/`0x1A00` 且上限 10 条
   （见上），但**默认映射里具体包含哪些对象/顺序/位长**仍未知（第三方文档未列出），
   本画像暂用通用 7 字节布局，**未逐项确认**。
2. **厂商扩展对象**（如 `0x2000` 段参数）：未收录，需要按公开手册核对后补充。
3. **DC 参数**：SV660N 通常启用 DC；本画像 `dc.supported=false`，DC 参数未核实。
4. **错误码语义**（`0x603F` 具体故障码表）与状态机时序（Ti=0/1）：未实现。

## 后续步骤

1. 在可获取公开手册文本的条件下，逐项核对默认 PDO/扩展对象并更新 manifest（仍用
   我方 VID/PID）。
2. 行为插件（Tier0）已按通用 CiA 402 画像实现（`--behavior cia402`），可直接用于本画像
   的扫描/OP 验证；模式切换（CSP/CSV/CST）与错误码注入待后续。
