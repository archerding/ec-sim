# 设备插件：文档引用规则与制作流程

> 目标：依据**公开文档**制作"兼容某知名品牌设备"的从站模拟插件，可闭源、可付费、可上架市场。
> 本文是**法律与工程的双重边界**。违反红线 = 插件不能上架 + 市场品牌受损，比功能缺失更严重。
> 不构成法律意见；正式上架前需律师复核（尤其厂商手册的使用条款与商标）。

---

## 1. 三条红线（不可越）

| # | 红线 | 为什么 | 正确做法 |
|---|---|---|---|
| **R1** | **不得把厂商 ESI(XML)、PDF 手册、固件、示例代码放进仓库或插件包** | 这些文件有版权，且 ESI 通常有"仅限配套使用"的许可 | 只提取**接口事实**（对象索引/子索引/数据类型/位宽/默认 PDO 映射/SM 布局），用自己的话写成 manifest，并登记来源 URL 与提取日期 |
| **R2** | **不得使用厂商 Vendor ID / Product Code / 商标作为设备身份** | 冒充真实设备 = 商标侵权 + 不正当竞争；一致性测试也无效 | 用**我方自有 Vendor ID**（走 ETG 免费申请）+ 自有 Product Code；在 manifest 的 `compatibility` 字段里说明"兼容某某型号"，UI 与 ESI 名称一律带 `SIMULATED` |
| **R3** | **不得宣称"完全一致 / 通过一致性测试 / 官方认证"** | 未经测试的声明是可诉的虚假宣传 | 只写"compatible with（基于公开文档的独立实现）"，并附免责声明："Not affiliated with, endorsed by, or certified by <vendor>." |

补充两条：
- **R4**：若某厂商手册明确禁止"用于开发竞争产品"，则该设备**不进市场**（登记到 `catalog.yaml` 的 `blocked` 区并写明原因）。
- **R5**：插件作者须在提交材料里给出 `sources`（标题 + URL + 访问日期）。市场审核以此作为证据链。

---

## 2. 允许做 / 不允许做（落到操作）

**允许**
- 阅读官网产品页、公开手册、应用笔记、开源社区讨论（论坛/issue）。
- 记录**事实性接口数据**：对象字典条目、PDO 默认映射、SM2/SM3 偏移与长度、DC 参数、AL 状态机行为、错误码含义。
- 用这些事实**独立实现**从站行为（PDO 值生成/消费、CiA402 状态机）。
- 在文案里做**指示性使用**（"用于 Inovance SV660N 的仿真"，带免责声明）。

**不允许**
- 复制 ESI/手册/截图入包；转售或分发厂商 ESI。
- 用厂商 Logo、设备照片做市场图标。
- 把厂商 VID/PID 写进 Identity 对象（0x1018）。
- 把"仿真通过"包装成"设备通过一致性测试"。
- 把厂商固件/逆向工程产物打包。

---

## 3. 制作流程（每台设备一次）

```
① 选型        → 按市场需求挑型号（见 plugins/catalog.yaml）
② 资料收集    → 公开手册/官网/应用笔记；只读，不入库
③ 事实提取    → 填 devices/<id>/facts.md（对象字典子集、PDO 默认映射、SM、DC）
④ 写 manifest → plugins/<id>/plugin.yaml（声明式优先，见 §4）
⑤ 本地验证    → 模拟器起从站 + IgH/SOEM 扫描，核对 PDO 布局/长度/对象可读
⑥ 行为实现    → 仅当需要状态机/私有逻辑时写 code/behavior.wasm
⑦ 打包签名    → 加密载荷 + Ed25519 签名 + entitlement
⑧ 上架审核    → 来源证据、许可证、商标合规、SBOM、定价
```

**每步的交付物**（都进本地 `.local` 或仓库对应目录，注意 R1）：
- `plugins/<id>/facts.md`：事实表（可入库；只写索引/类型/位宽等数据 + 来源链接）
- `plugins/<id>/plugin.yaml`：manifest（入库）
- `plugins/<id>/code/`：行为插件源码（闭源插件的源码**不入库**，只入库接口描述与测试）
- `docs/plugins/<id>-verification.md`：验证记录（命令 + 输出摘要，脱敏）

---

## 4. Manifest 格式（v1）

设计原则：**声明式优先**。90% 的"兼容某设备"需求不需要写代码，只需要把 PDO 画像描述清楚；只有行为逻辑（状态机/私有协议）才写 WASM。

```yaml
schema: ecsim.plugin/v1
id: com.ecsim.compat.inovance-sv660n
name: "SV660N compatible servo (simulated)"
version: 1.0.0
kind: device                # device | app-layer | pdo-extension
license: proprietary        # 付费闭源插件
pricing: {model: one-time, amount: 299, currency: CNY}

compatibility:              # 说明性元数据，不用于设备身份
  brands: [Inovance]
  models: [SV660N]
  disclaimer: "Independent re-implementation from public documentation. Not affiliated with, endorsed by, or certified by Inovance."

identity:                   # 必须是自有 VID / 自有 PID
  vendor_id: 0x00000000     # TODO: 填入 ETG 分配给我方的 VID
  product_code: 0x00000001
  revision: 0x00010000
  simulated: true           # 强制：所有名称/ESI 带 SIMULATED 标

profile: cia402-csp         # 行为模板（内置 Tier0）
mailbox: {coe: {sdo: true, sdo_info: true, complete_access: true}}
sm:
  - {index: 0, dir: mbox-in,  start: 0x1000, len: 128}
  - {index: 1, dir: mbox-out, start: 0x1080, len: 128}
  - {index: 2, dir: out,      start: 0x1100, len: 64}
  - {index: 3, dir: in,       start: 0x1200, len: 64}

pdo:
  rx:  # 主站 → 从站（SM2）
    - index: 0x1600
      entries:
        - {index: 0x6040, sub: 0, bits: 16}
        - {index: 0x607A, sub: 0, bits: 32}
        - {index: 0x6060, sub: 0, bits: 8}
  tx:  # 从站 → 主站（SM3）
    - index: 0x1A00
      entries:
        - {index: 0x6041, sub: 0, bits: 16}
        - {index: 0x6064, sub: 0, bits: 32}
        - {index: 0x6061, sub: 0, bits: 8}
  # 可选：固定映射 PDO（只生成映射对象，不进 SII、不作为默认分配；
  # 需要主站经 0x1C12/0x1C13 选择。SV660N 画像的 0x1701/0x1B01 即用此字段。）
  fixed:
    - index: 0x1701
      entries:
        - {index: 0x6040, sub: 0, bits: 16}
        - {index: 0x607A, sub: 0, bits: 32}
objects:                    # 需要可 SDO 访问的最小对象集
  - {index: 0x1000, sub: 0, name: Device type,        dtype: UINT32, access: ro}
  - {index: 0x1018, sub: 1, name: Vendor ID,          dtype: UINT32, access: ro}
  - {index: 0x6041, sub: 0, name: Statusword,         dtype: UINT16, access: ro}
  - {index: 0x6064, sub: 0, name: Position actual,    dtype: INT32,  access: ro}
  # …按 facts.md 补齐

dc: {supported: true, sync0: true}   # 不确定就写 false，宁可少声明

behavior:                   # 可选：只有需要状态机时才出现
  code: code/behavior.wasm
  tick_hz: 1000

sources:                    # R5：证据链，必填
  - {title: "SV660N 用户手册（公开）", url: "https://…", accessed: "2026-11-12"}
```

> 注：`identity` 与 `compatibility` 必须分离 —— 前者是"我们是谁"，后者是"我们像谁"。这是 R2 的技术落地。

---

## 5. 闭源与付费（商业侧）

| 项 | 做法 |
|---|---|
| 核心许可 | Apache-2.0 OR MIT，插件为**独立作品**（SDK 已声明"可闭源"） |
| 插件载荷 | 加密 WASM 或 Tier3 独立进程二进制（强 IP 保护走 Tier3） |
| 签名/授权 | Ed25519 签名 + 离线 JWT entitlement（machine binding + 短 TTL）+ 在线吊销 |
| 定价（建议） | 单设备 ¥99–299；同系列包 ¥999；品牌伺服包 ¥1,999–4,999；企业含 SLA |
| 市场分成 | 平台抽成 20–30%；插件方自定 EULA |
| 支持 | 付费插件必须提供"仿真范围说明 + 已知差异清单"（诚实标注不模拟的行为） |

**诚实条款**：每个插件必须在 README 里列"已模拟 / 未模拟"清单（例如：模拟了 PDO 与 CiA402 CSP，未模拟厂商私有诊断对象）。这既是合规，也是口碑。

---

## 6. 审核清单（上架前逐项打勾）

- [ ] `sources` 完整（URL + 访问日期），无厂商文件本体入库
- [ ] Identity 使用自有 VID/PID，名称含 `SIMULATED`，无厂商 Logo
- [ ] 免责声明存在（README + 市场页 + 插件内 About）
- [ ] 未声明"一致性测试/认证/官方"
- [ ] 许可证与 SBOM 齐全；无 GPL/AGPL 代码
- [ ] 已模拟/未模拟清单
- [ ] 在 IgH 与 SOEM 上各跑一遍扫描验证（记录进 docs/plugins/）
