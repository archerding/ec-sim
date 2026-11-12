# Beckhoff EL2008 兼容 8 通道数字输出（SIMULATED）

> **免责声明**：本插件是**独立实现**的仿真设备画像，仅依据公开文档制作；
> 与 Beckhoff Automation 无隶属关系，未获其背书，也**不是**官方认证或
> "通过 EtherCAT 一致性测试"的产品（R3）。
> 本插件不包含厂商 ESI/PDF/固件/示例代码（R1）。

## 是什么

`plugin.yaml` 是一个**声明式**设备描述：EC-Sim 读取它生成 SII 镜像、对象字典与
进程数据布局，用未修改的 IgH / SOEM 主站即可扫描到"EL2008 兼容（SIMULATED）"从站。
没有行为代码（WASM）——纯声明式插件。

设备身份使用**我方自有** Vendor ID/Product Code（`0x00000EC5` / `0x0000E5D2`），
不冒用厂商身份（R2）；名称含 `SIMULATED`。EL2008 的真实 PID 仅作为兼容性事实记录在
`facts.md`，不写入设备身份。

## 画像内容（接口事实见 `facts.md`）

| 项 | 值 |
|---|---|
| 输出 | 8 通道数字输出（进程映像 8 bit / 1 字节，SM2） |
| 默认 RxPDO | `0x1600`..`0x1607`，每个映射一个 1 bit 输出对象 |
| 输出对象 | `0x7000:01`、`0x7010:01` … `0x7070:01`（BOOL） |
| 输入 | 无（EL2008 无输入通道） |
| 邮箱 | CoE（SDO + SDO Info；不声明 Complete Access） |
| DC | 不支持 |

## 已模拟 / 未模拟（诚实清单）

- ✅ 已模拟：8 位输出进程数据、SII、CoE 对象字典与 SDO 上传/SDO Info、
  SM/PDO 布局（与公开资料的默认映射一致）。
- ⚠️ 未模拟：实际电气行为（0.5 A 驱动、短路/过载保护、开关时间）、输出回读/诊断对象
  （EL2008 无输入）、Distributed Clock、厂商固件/诊断相关对象。

## 用法

```bash
# 用该 manifest 启动从站（Linux TAP；UDS 同理）
cargo build --release -p esc-node
sudo target/release/ec-sim-esc --tap escsim0 --manifest plugins/beckhoff-el2008/plugin.yaml

# 端到端预回归（未修改的 SOEM/IgH；需 root 与验证机夹具）
sudo bash scripts/spikes/beckhoff-el2008-probe.sh
```

验证记录：[docs/plugins/beckhoff-el2008-verification.md](../../docs/plugins/beckhoff-el2008-verification.md)。

## 来源（R5）

见 `facts.md` 的「来源」一节（Beckhoff 产品页、Beckhoff Information System、
linuxcnc-ethercat `esi-data`；均记录访问日期）。
