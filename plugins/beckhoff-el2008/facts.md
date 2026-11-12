# EL2008 接口事实（兼容仿真用）

> 本文件只记录从**公开文档**提取的接口事实，用于制作"兼容某型号"的独立实现。
> 不含厂商 ESI/PDF/固件/示例代码（R1）。来源见文末（R5）。
> 身份声明见 `plugin.yaml`：使用**我方自有** VID/PID（R2），不是 Beckhoff 的 PID。

## 设备概况

| 事实 | 值 | 来源 |
|---|---|---|
| 类型 | EtherCAT Terminal，8 通道数字输出 | S1/S2 |
| 输出数量 | 8 | S1 |
| 额定电压 | 24 V DC（-15%/+20%） | S1 |
| 每通道最大输出电流 | 0.5 A（短路保护） | S1 |
| 过程映像位宽 | 8 outputs | S1 |
| Distributed Clocks | 不支持（–） | S1 |
| 配置方式 | 无地址/无参数设置 | S1 |
| 输入通道 | 无 | S1/S2 |

## PDO 默认映射（RxPDO，主站 → 从站）

| PDO 索引 | 映射对象 | 数据类型 / 位宽 | 说明 | 来源 |
|---|---|---|---|---|
| 0x1600 | 0x7000:01 | BOOL / 1 bit | Channel 1 Output | S3 |
| 0x1601 | 0x7010:01 | BOOL / 1 bit | Channel 2 Output | S3 |
| 0x1602 | 0x7020:01 | BOOL / 1 bit | Channel 3 Output | S3 |
| 0x1603 | 0x7030:01 | BOOL / 1 bit | Channel 4 Output | S3 |
| 0x1604 | 0x7040:01 | BOOL / 1 bit | Channel 5 Output | S3 |
| 0x1605 | 0x7050:01 | BOOL / 1 bit | Channel 6 Output | S3 |
| 0x1606 | 0x7060:01 | BOOL / 1 bit | Channel 7 Output | S3 |
| 0x1607 | 0x7070:01 | BOOL / 1 bit | Channel 8 Output | S3 |

- 8 个 RxPDO 合计 8 bit 输出，进程映像 1 字节。
- 无 TxPDO（该端子无输入）。
- 默认 PDO 分配对象 `0x1C12`（RxPDO assign）：子索引 1..8 依次指向 0x1600..0x1607。

## 参考信息（**不用于设备身份**，R2）

| 事实 | 值 | 来源 |
|---|---|---|
| 厂商 Product Code（参考） | `0x07d83052` | S3 |
| Revision（参考） | r16 `0x00100000`、r17 `0x00110000`、r18 `0x00120000` | S3 |

> 本仿真器使用自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5D2`。
> 名称含 `SIMULATED`；不宣称与厂商存在隶属、背书或认证关系（R3）。

## 本仿真画像的取舍（诚实清单）

已模拟：
- 8 通道数字输出的进程数据（8 个 RxPDO × 1 bit，SM2 1 字节输出）；
- SII（同一组 SM/PDO 类别）、CoE 对象字典（0x1C12 分配、0x1600..0x1607 映射、
  0x7000..0x7070 输出对象）、SDO 上传/SDO Info 读取。

未模拟（⚠️ 待验证/不实现）：
- 实际电气特性（0.5 A 驱动能力、短路/过载保护、开关时间）——纯软件仿真无电气行为；
- 输出回读/诊断对象（EL2008 无输入，输出状态不在 PDO 中回读）；
- Distributed Clock（EL2008 不支持 DC）；
- 厂商诊断/固件相关对象（本画像不含）。

## 来源（R5）

- S1：Beckhoff EL2008 产品页（技术数据：8 outputs、24 V DC、0.5 A、bit width 8 outputs、
  无 DC、无配置）
  <https://www.beckhoff.com/en-en/products/i-o/ethercat-terminals/el-ed2xxx-digital-output/el2008.html>
  访问日期 2026-11-12。
- S2：Beckhoff Information System — EL20xx/EL2124 文档「EL2008 - LEDs and connection」
  （8 个输出端子、无输入）
  <https://infosys.beckhoff.com/content/1033/el20xx_el2124/1636470795.html>
  访问日期 2026-11-12。
- S3：linuxcnc-ethercat `esi-data` — EL2008（由公开 ESI 数据整理：RxPDO 0x1600..0x1607，
  0x7000:01..0x7070:01 BOOL；PID/Revision 仅作参考）
  <https://linuxcnc-ethercat.github.io/esi-data/devices/EL2008>
  访问日期 2026-11-12。
