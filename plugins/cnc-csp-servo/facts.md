# facts：cnc-csp-servo（CNC 虚拟闭环用 CiA 402 CSP 画像）

> 本文件只记录**公开接口事实**与来源 URL；不含任何厂商 ESI/PDF/固件/示例代码（R1）。
> 本画像身份使用**我方自有** Vendor ID `0x00000EC5` / Product Code `0x0000E5E3`（R2），
> 名称带 `SIMULATED`，**不宣称**与任何厂商/CiA/ETG/LinuxCNC 存在隶属、背书或认证关系（R3/R4）。

## 事实与来源

| # | 事实 | 来源 | 访问日期 | 备注 |
| --- | --- | --- | --- | --- |
| F1 | CiA 402 对象：`0x6040` controlword(U16)、`0x6041` statusword(U16)、`0x6060` modes of operation(S8)、`0x6061` modes of operation display(S8)、`0x6064` position actual value(S32)、`0x606C` velocity actual value(S32)、`0x607A` target position(S32) | [CiA 402 (public summary)](https://www.can-cia.org/cia-402) | 2026-11-13 | 标准对象语义；本仓库按对象索引/子索引/位长声明 |
| F2 | `0x6502` Supported drive modes 位：PP=bit0、HM=bit5、IP=bit6、CSP=bit7 | [CiA 402 (public summary)](https://www.can-cia.org/cia-402) | 2026-11-13 | 本画像 7 字节 RxPDO 支持 PP/HM/IP/CSP → `0x01C1` |
| F3 | EtherCAT PDO 映射对象 `0x1600`（RxPDO）/`0x1A00`（TxPDO）由主站按映射对象内容配置 | [ETG.1000 / EtherCAT device profiles](https://www.ethercat.org/) | 2026-11-13 | 本仓库 `plugins/*` 的 `pdo: rx/tx` 即生成这两个映射对象 |
| F4 | lcec（linuxcnc-ethercat）`basic_cia402` 在 `enableCSP=1` 时**隐式要求** TxPDO 含 `0x606C:00`（实际速度）；`0x6040`/`0x6041` 必须是 Rx/Tx 第一项；lcec 按从站映射对象读回匹配条目 | [linuxcnc-ethercat 公开源码（只读参考）](https://github.com/linuxcnc-ethercat/linuxcnc-ethercat)，commit `c6f8820`，由 cnc-impl 核对 | 2026-11-13 | **只读参考**，未复制代码；仅用于确定 PDO 布局 |
| F5 | 虚拟闭环 V1/V2 不配置 DC（lcec XML 无 `<dcConf>`）；模拟器仅有 Tier0 帧驱动时钟 | 本地 `.local/ops.md` 运维记录 + [virtual loop plan](D:/tokengence.ai/cnc-station/docs/plan-virtual-loop.md)（跨仓引用，仓库内不入地址） | 2026-11-13 | ⚠️ 无 SYNC0/漂移补偿 |

## 未验证 / 边界（⚠️）

- ⚠️ 本画像只保证**帧级** PDO 数据通路；Tier0 行为（PP/CSP）无轨迹、加减速、编码器分辨率与单位换算。
- ⚠️ lcec XML 的实际匹配行为由 CNC 侧在自己的验证机上实测（`scripts/vm-verify/vm-b6-v1.sh`）；
  本仓库不对其结论作认证表述。
- ⚠️ 全链路验收的是"软件闭环数据通路"，不含物理安全链/真实伺服。
