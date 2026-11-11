# EtherCAT 从站模拟器：技术选型与产品架构决策

> 核实时点：**2026-11-12**。版本口径沿用本会话四份调研报告（esc-protocol / license-legal / arch-runtime / market-priorart），外部事实以 2026-11 可访问的一手文档为准。
> 读者：要拍板的创始人 / 架构师。本文不给"选项菜单"，只给推荐、代价与止损线。
> 标注约定：✅ 已核实；⚠️ 待核实或必须 spike；🔴 风险；🟡 有争议或依赖解释。

---

## 结论：这不是"造一个协议栈"

**一句话：这不是"造一个协议栈"，而是"造一条虚拟总线 + 一个插件宿主 + 一套 AI 可操作的测试基础设施"。**

展开成四句：

1. **协议栈是存量市场。** IgH、SOEM、KickCAT、ethercrab 已经把"能跑"做到了；再写一个"更快的 SOEM"没有商业位置（[IgH](https://gitlab.com/etherlab.org/ethercat)、[SOEM](https://github.com/OpenEtherCATsociety/SOEM)、[KickCAT](https://github.com/leducp/KickCAT)、[ethercrab](https://github.com/ethercrab-rs/ethercrab)）。
2. **虚拟总线是信任入口。** 客户真正缺的不是栈，是"没有硬件也能让**未修改的**主站看到 OP"——这件事只有真帧 + 真 ESC 才成立（[IgH libfakeethercat 的边界](https://docs.etherlab.org/ethercat/1.6/doxygen/libfakeethercat.html)）。
3. **插件宿主是商业模式。** 设备厂商的 PDO 画像是长尾，只有"声明式接入 + 市场分发 + 许可/吊销"才能规模化；这层今天没人做。
4. **AI 可操作是差异化。** MCP / Skills 把"扫总线、配 PDO、跑测试、出报告"变成 agent 可调用的事务，这是 KickCAT 这类 C++ 协议栈厂商的 DNA 盲区（[MCP 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/)）。

给创始人的五条速览：

| 问题 | 答案 |
|---|---|
| 做不做 | **做**，但定位是"虚拟总线 + 插件宿主 + 测试基础设施"，不是协议栈 |
| 最难的一刀 | **线缆层**（§2）：TAP 真帧 vs 端口 shim，决定叙事与平台 |
| 护城河 | **ESC-over-UDS 开放帧协议 + 插件市场 + AI 可操作**，不是协议实现 |
| 时间 | Spike 1 周 → M1 6–8 周 → M2 6–8 周 → M3 6–8 周 → M4 4–6 周（估算，按 3–4 人） |
| 最大合规风险 | ETG 对"纯软件从站仿真"的书面态度 + "EtherCAT"商标使用（§7） |

---

## §0 三个决定一切的硬事实

> 本节结论：**事实 1 证明需求真实但基线很低；事实 2 证明路线可行但窗口很窄；事实 3 决定架构必须"核心宽松 + 边界隔离"。** 三条合起来，架构不是审美选择，是许可与差异化的必然结果。

### 事实 1：IgH 1.6 官方自带 `libfakeethercat`，但只到 PDO 级

IgH EtherCAT Master **1.6.13** 提供 `libfakeethercat`：与 `libethercat` **同一套 API**，用于"没有主站、没有从站"的 dry-run；配置 `--enable-fakeuserlib` 即构建，靠 `LD_LIBRARY_PATH` 把 `libethercat.so.1` 指向它（[官方文档](https://docs.etherlab.org/ethercat/1.6/doxygen/libfakeethercat.html)）。

| 它能做 | 它不能做 | 对我们的含义 |
|---|---|---|
| 创建 master/domain、activate、send/receive | 收发**真实 EtherCAT 帧** | 无法验证帧解析、WKC、拓扑、邮箱 |
| 处理/排队 domain、配置 PDO | 模拟**总线错误**（文档明确"bus error cannot be simulated"） | 故障注入是空白区 |
| 用 `ecrt_slave_config_sdo*()` 配 SDO | 真实 ESC 寄存器 / FMMU / SM / DC | 无法做寄存器级调试 |
| 把 SDO 配置 dump 成 JSON | 真实 ESI / SII / EEPROM | 无法"导入厂商 ESI 就得到一个从站" |
| 通过 RtIPC 共享内存做**进程数据级**从站模拟 | 多主站（只有 IgH API） | 无法做 SOEM/TwinCAT 兼容验收 |

**这是"无硬件开发"需求的官方证据**：连 IgH 自己都要造一个假库，说明需求不是我们编的。**但它也是最低基线**：如果我们只做到 PDO 级，就是一个更难用的 `libfakeethercat`，没有存在理由。

**差异化必须是四件事同时成立**：真帧（线缆层）＋ 真 ESC 寄存器/FMMU/SM/AL/DC/watchdog/SII ＋ 真 ESI 驱动 ＋ 故障注入。少一件，就退回"PDO 模拟器"的红海。

| 维度 | `libfakeethercat` | 我们 M1 目标 |
|---|---|---|
| 真实帧 | ❌ | ✅（UDS shim；M2 加 TAP） |
| ESC 寄存器 | ❌ | ✅ 寄存器阵列 + FMMU/SM/AL |
| 总线错误注入 | ❌（官方明说不能） | ✅ M2（丢帧/CRC/断链/错序） |
| ESI 导入 | ❌（PDO 手工配） | ✅ ESI → SII + OD + 默认 PDO |
| 多主站 | ❌ 仅 IgH | ✅ IgH / SOEM / ethercrab（TwinCAT 作为验收） |

### 事实 2：KickCAT 是唯一技术逼近的开源竞品，但它是"协议栈厂商"

KickCAT（**CeCILL-C**，C++）同时提供主站与从站栈，并自带完整的软件 ESC 与网络仿真（[README](https://github.com/leducp/KickCAT)、[SIMULATION.md](https://github.com/leducp/KickCAT/blob/master/docs/SIMULATION.md)）：

| KickCAT 已经做到 | 细节（来自其文档） | 对我们的含义 |
|---|---|---|
| `EmulatedESC` | 16 SM、16 FMMU、AL 状态机、DL 端口/链路/错误计数、EEPROM、DC 本地时钟（可配 ppm 漂移/接收时间锁存/SYNC0）、PDI 与过程数据看门狗、端口 loopback | "软件 ESC"路线被验证，不是伪需求 |
| ESI 驱动 | ESI XML → SII 镜像 + CoE 对象字典；`od_generator` 从 ESI 生成 OD 代码 | 声明式接入可行，但它是**编译期代码生成**，不是运行时插件 |
| `EmulatedNetwork` | 端口级拓扑路由、线缆冗余、运行时断线/恢复（故障注入）、DC 转发延迟 | 故障注入也不是空白，但只覆盖"线" |
| 两种跑法 | 同进程 loopback；或 `network_simulator` 通过共享内存 **TAP socket** 与独立主站进程对接 | 与我们的 UDS 思路同构，说明接口方向对 |
| 工具与生态 | KickUI（拓扑/SDO/PDO/DS402 面板）、EEPROM 编辑器、`scan_topology`、Python bindings、AF_XDP、FoE | 它已经是"能用的产品"，不是 demo |
| 边界 | Windows 仅"tools and testing"；无 Web 控制面、无插件市场、无 MCP/AI、无"双主站开箱验收"叙事；定位是给设备厂商/嵌入式团队用的协议栈 | **它是"更好的栈"，不是"更好的测试基础设施"** |

**策略结论**：

- **不重复造 wheel。** 它的 ESC/网络仿真设计可以读、可以对照，但 **CeCILL-C 是文件级弱 copyleft，不能把代码抄进 Apache-2.0 核心**（[CeCILL-C 全文](https://cecill.info/licences/Licence_CeCILL-C_V1-en.html)）。要么独立进程集成并遵守其义务，要么 clean-room 重写。
- **窗口 12–18 个月**（⚠️ 估算）：KickCAT 要补 Web/插件/AI/托管，需要换一拨人和一种产品 DNA；但它也可能与一家 Web 团队合作，或直接被 EDA/伺服厂商收编。窗口不是"它做不到"，是"它没动力"。
- **我们的差异不是"更真的 ESC"，是"总线 + 宿主 + AI"**：KickCAT 让你**开发**从站；我们让你**验证、演示、卖**从站。

### 事实 3：许可格局决定商业架构

| 组件 | 许可证 | 关键义务 | 架构后果 |
|---|---|---|---|
| SOEM 2.x | **GPLv3 + 商业双许可**（[LICENSE.md](https://github.com/OpenEtherCATsociety/SOEM/blob/master/LICENSE.md)） | 分发衍生作品须 GPLv3；闭源商用需向 rt-labs 购买 | **只能做独立进程/容器的测试夹具**，不能链接进核心 |
| IgH 内核模块 | **GPLv2**（仅内核态） | 分发二进制须提供对应源码 | 只能"外部主站"角色，不能进产品 |
| IgH 用户态库 | **LGPL-2.1** | 动态链接可闭源，但修改库须开源 | 可做兼容目标，不做依赖 |
| LinuxCNC | **GPLv2** | 整体衍生须 GPLv2 | 只做"独立进程模板"，不做链接 |
| KickCAT | **CeCILL-C** | 文件级 copyleft；修改其文件须以 CeCILL-C 发布 | 读设计、独立实现；或独立进程集成 |
| SOES | **GPLv2 + 文件级例外**（[LICENSE](https://github.com/OpenEtherCATsociety/SOES/blob/master/LICENSE)） | 例外只覆盖"本文件被链接/实例化"，不是整体链接例外 | 同样不能进核心；别被"有例外"误导 |
| ETG 会员 | **免费、无分级**（[会员权益](https://www.ethercat.org/en/membership_benefits.html)） | 需遵守章程与商标规则 | 第 1 周就加入 |
| EtherCAT Vendor ID | **免费**，但必须是 ETG 会员（[Vendor ID](https://www.ethercat.org/en/vendor_id.html)） | 每个设备须实现唯一 VID | 仿真从站用插件厂商 VID，或我们申请自有 VID |
| EtherCAT MainDevice License | **免费**，协议要求兼容性（[ETG FAQ 3.3](https://www.ethercat.org/en/faq.html)） | 需签署；开源/共享源码场景建议书面沟通 | 即使做从站侧，也建议签，换取"主站兼容"的法律确定性 |
| "EtherCAT" 商标 | Beckhoff 所有，ETG 管理 | 遵守 ETG.9001 标记规则；不得暗示认证 | 命名与市场材料必须法务过审 |

**合成约束**：

1. **真帧** ⇒ 必须有线缆层（§2），不能只在 API 层假装。
2. **真 ESC** ⇒ 内核必须自研（GPL 栈不可链接），并用 C ABI 隔离。
3. **许可** ⇒ 核心 **Apache-2.0 OR MIT**；GPL 组件只做**独立进程**；插件必须**进程/沙箱隔离**；市场必须能"杀进程 + 远程吊销"。

> ⚠️ **待核实（P0）**：ETG 对"纯软件从站仿真器"是否要求 SubDevice License、是否允许在营销中使用 "EtherCAT" 字样。ETG FAQ 明确"EtherCAT 技术本身不是 Open Source"，并建议开源/共享源码场景直接联系 ETG（[FAQ 3.4](https://www.ethercat.org/en/faq.html)）。这不是可以靠"我认为"解决的问题，必须拿书面答复。

---

## §1 产品拆成 5 层

> 本节结论：**五层里只有 L2（ESC 内核）和 L3（OD/CoE）是"必须自己做对"的；L1 是选型，L4 是商业模式，L5 是差异化。** 层间契约一旦定错，后面全部返工。

| 层 | 名称 | 职责 | 对外契约 | 明确不做 |
|---|---|---|---|---|
| **L1** | 线缆层 | 把主站字节流送达 ESC 内核，并把响应送回；TAP / veth / UDS / VM 四种形态 | `FrameIo`：`recv_frame(&mut [u8]) -> Result<Frame>`、`send_frame(&[u8])` | 不解析 EtherCAT 语义、不碰寄存器 |
| **L2** | ESC 内核 | 寄存器阵列、FMMU、SM、AL 状态机、SII/EEPROM、DC、watchdog、邮箱 | C ABI vtable + Rust trait；确定性 `tick()` | 不做应用逻辑、不加载插件、不做网络 |
| **L3** | 对象字典与 CoE | ESI → SII + OD + PDO 映射；SDO / CA / SDO Info / Emergency | `ObjectDictionary`、`CoeServer`、`EsiCompiler` | 不碰线缆、不跑 WASM、不做 UI |
| **L4** | 应用与插件宿主 | PDO 值生成/消费、CiA402 状态机、自定义协议 | `Plugin::step(&PdoView, &mut PdoView)` | **不进实时环**、不直接收帧、不持有 socket |
| **L5** | 控制面与客户端 | CLI / WebUI / MCP / Skills / Python SDK / marketplace client | HTTP、JSON-RPC、MCP、UDS 控制通道 | 不参与每周期调度、不直接映射 PDO 内存 |

### 1.1 数据流（一张图看完）

```
   ┌──────────────┐   帧字节   ┌──────────────┐   datagram   ┌──────────────┐
   │ 主站          │ ─────────▶ │ L1 线缆层     │ ───────────▶ │ L2 ESC 内核   │
   │ IgH/SOEM/     │            │ TAP / shim /  │              │ 寄存器/FMMU/  │
   │ ethercrab     │ ◀───────── │ UDS / veth    │ ◀─────────── │ SM/AL/DC/SII  │
   └──────────────┘   响应      └──────────────┘   响应        └──────┬───────┘
                                                                     │ 邮箱/PDO
                                                              ┌──────▼───────┐
                                                              │ L3 OD / CoE   │
                                                              │ SDO/ESI/SII   │
                                                              └──────┬───────┘
                                                                     │ PDO arena（核心持有）
                                                              ┌──────▼───────┐
                                                              │ L4 插件宿主   │
                                                              │ T0/T1/T2/T3   │
                                                              └──────┬───────┘
                                                                     │ 控制/观测
                                                              ┌──────▼───────┐
                                                              │ L5 CLI/Web/   │
                                                              │ MCP/SDK       │
                                                              └──────────────┘
```

### 1.2 三条铁律

1. **ESC 内置，插件只做"对象字典声明 + 应用层逻辑"。**
   插件不能实现 FMMU、不能改 SM 配置、不能伪造 AL 状态。否则同一份 ESI 在不同插件上行为不一致，主站兼容性无法保证。

2. **插件永远不进实时环。**
   唯一例外是 **Tier0**——它编译进核心，不是运行时插件。Tier1 原生库默认跑在独立线程，结果下一周期生效；只有白名单 + WCET 认证的 Tier1 才允许 RT 直调（§4）。

3. **PDO 数据契约放核心持有的共享内存 arena。**
   arena 由 L2 分配、双缓冲 + seqlock；L4 只拿到 `offset/len`，不拿到指针所有权。这样：跨进程（Tier3）零拷贝、ABI 稳定、崩溃可恢复、可观测（每个 PDO 的写入者/时间戳可追踪）。

### 1.3 层间契约表

| 契约 | 方向 | 同步性 | 延迟预算 | 失败模式 |
|---|---|---|---|---|
| `FrameIo` | L1 ↔ L2 | 同步（每周期） | < 50 µs | 帧丢失 → WKC 不增；内核记录 |
| `EscTick` | L2 内部 | 同步，固定步长 | < 100 µs | 超时 → AL 状态码 + 看门狗 |
| `PdoArena` | L2 ↔ L4 | 双缓冲，周期边界交换 | 0（零拷贝） | 插件写越界 → 核心拒绝并停插件 |
| `ObjectDictionary` | L3 ↔ L2/L4 | 读多写少，COW | < 10 µs | 索引冲突 → 加载期报错 |
| `ControlPlane` | L5 ↔ L2/L3 | 异步（tokio） | 10–100 ms | 断连不影响总线 |

> 🔴 **反模式**：让插件直接读 `FrameIo`。这会让插件可以伪造任意从站行为，市场审核不可能通过，且实时环被插件代码污染。**帧只属于 L1/L2。**

---

## §2 最关键的叉路：线缆层怎么做

> 本节结论：**推荐"混合双通道，共用同一个 ESC 内核"**——Linux 旗舰走 TAP 真帧，全平台走 UDS shim；Windows 不做 TAP，只做 shim。并把 **ESC-over-UDS 帧协议**产品化为开放规范，这是护城河。

这是全文最重要的技术决策，没有之一。选错，后面所有"真帧"叙事都是假的。

### 2.1 方案 A：网络级（Linux TAP）

**做法**：创建 `IFF_TAP | IFF_NO_PI` 的 TAP 设备，让内核把以太网帧原样交给用户态；EtherType `0x88A4` 原生透传。主站（IgH `ec_generic` / SOEM `oshw`）绑定 `tap0`，我们的进程从 TAP 读帧、喂给 ESC 内核、把响应写回。

| 优点 | 代价 |
|---|---|
| **最真**：主站看到的就是一张网卡，帧格式、时序、WKC 全部真实 | **Linux-only 旗舰**；Windows/macOS 没有开箱即用的等价 L2 虚拟网卡 |
| **未修改的 IgH/SOEM 可直接连**（只需配置接口名） | 需要 `CAP_NET_RAW` + `CAP_NET_ADMIN`；容器要 `--cap-add` 或 host network |
| 可用 `tcpdump -i tap0 -w` 抓包，调试体验与真总线一致 | Intel 网卡的 GRO/LRO/VLAN offload 可能改写/丢弃非 IP 帧 ⚠️ |
| 可用 veth + bridge 做多从站/多主站拓扑 | IgH `ec_generic` 对 netdev 的额外要求（promisc、link state、ethtool）**待 spike** |
| 可作为"真帧"营销证据 | 无法进普通 CI（GitHub Actions 默认无 NET_ADMIN） |

### 2.2 方案 B：端口级 shim

**做法**：不经过内核网络栈，直接替换主站的"网卡访问层"：

| 主站 | 替换点 | 工作量 |
|---|---|---|
| SOEM | `oshw` 层的 `ecx_setupnic` / `ecx_outframe` / `ecx_inframe`（或 `ecx_outframe_red`） | 小：实现 3–5 个函数，把帧写到 UDS |
| IgH | 复用 `libfakeethercat` 思路，替换 `libethercat` 的 40+ 符号；或 `LD_PRELOAD` 一层 `libethercat` 兼容库 | 中：符号多，但 API 稳定 |
| ethercrab | `std::tx_rx_task` 的 Pcap/UDS 后端，或直接驱动 `PduTx`/`PduRx` 结构体 | 中：需自己写 async 收发循环 |
| TwinCAT | 不支持 shim；只能真网卡 / 虚拟机直通 | 大：不进 CI |

| 优点 | 代价 |
|---|---|
| **全平台**（Linux/Windows/macOS） | **"不是真网卡"**，说服力弱于 TAP |
| **无特权**，可进 CI、可进 Docker（普通用户） | 需要为每个主站维护适配层 |
| 延迟更低（避开内核网络栈） | 主站若绕过 oshw 直接 `AF_PACKET`，shim 失效 |
| 可精确注入延迟/丢帧/错序 | 无法用 `tcpdump` 验证，调试靠自己的抓包 |

### 2.3 Windows 现实：旗舰只能走 shim

| 方案 | 层级 | 许可 | 权限 | 能否承载 `0x88A4` | 结论 |
|---|---|---|---|---|---|
| **WinTun** | L3（IP 包） | MIT | 管理员安装驱动 | ❌ 只处理 IP，无 EtherType | 直接排除（[WinTun](https://git.zx2c4.com/wintun/about/)） |
| **TAP-Windows6** | L2 | **GPLv2** | 管理员安装 | ✅ | 可做，但 GPL 驱动 + 安装体验差（[tap-windows6](https://github.com/OpenVPN/tap-windows6)） |
| **Npcap** | L2 | 免费版受限；**商用分发需 OEM 授权** | 管理员安装 | ✅ | 成本与合规都高（[Npcap OEM](https://npcap.com/oem/)） |
| **UDS shim** | 用户态 | 我们自己 | 无 | ✅（语义等价） | **Windows 唯一务实路线** |

**结论**：Windows 不做 TAP 旗舰；Windows 用户通过 shim + WebUI 使用，叙事是"跨平台开发/CI"，不是"Windows 上的真总线"。

### 2.4 推荐：混合双通道，同一个 ESC 内核

```
                       ┌─────────────────────────────┐
  Linux 旗舰            │  tap0 (IFF_TAP|IFF_NO_PI)    │──┐
  IgH/SOEM ────────────▶│  EtherType 0x88A4 原生透传    │  │
                       └─────────────────────────────┘  │
                                                        ▼
  Windows/macOS/CI      ┌─────────────────────────────┐  ┌──────────────────┐
  SOEM/IgH/ethercrab ──▶│  UDS shim（SEQPACKET）       │─▶│  L2 ESC 内核      │
                       │  帧 = 长度 + 方向 + 时间戳     │  │  同一份代码       │
                       └─────────────────────────────┘  │  同一份 ESI/OD    │
                                                        │  同一份测试套件   │
  VM / 远程实验室        ┌─────────────────────────────┐  │                  │
  TwinCAT ─────────────▶│  veth pair / 物理网卡直通     │─▶│                  │
                       └─────────────────────────────┘  └──────────────────┘
```

**关键设计**：L1 只产出 `Frame`（字节切片 + 方向 + 时间戳 + 端口号），L2 完全不知道帧从 TAP 还是 UDS 来。两条通道跑**同一套 conformance 测试**，差异只在"传输适配器"。

### 2.5 把 ESC-over-UDS 产品化为开放规范（护城河）

不要把它做成"我们的私有协议"。做成**任何主站 20 行适配即接**的公开规范：

| 要素 | 设计 |
|---|---|
| 传输 | `AF_UNIX` + `SOCK_SEQPACKET`（Linux 保边界）；Windows 用命名管道消息模式或 stream + 长度前缀；高吞吐可选共享内存环 + `eventfd` |
| 帧消息 | `u32 len` + `u8 dir`（M→S / S→M）+ `u8 port` + `u64 ts_ns` + `payload`（完整 EtherCAT 帧） |
| 握手 | `{"hello":{"ver":1,"master":"igh-1.6.13","features":["dc","foe"]}}` |
| 控制 | 独立 JSON-RPC 通道：拓扑查询、寄存器 peek/poke、ESI 热加载、故障注入 |
| 流控 | 每方向 credit-based；溢出丢帧并计数（模拟真实丢包） |
| 版本 | 语义化；`ver` 不匹配时降级到最小公共集 |

核心适配器（Python / Linux，9 行；完整适配器 < 50 行）：

```python
import socket
s = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
s.connect("/run/esim/bus0.sock")
s.send(b'{"hello":{"ver":1,"master":"my-master"}}')
while True:
    frame = s.recv(2048)           # 完整 EtherCAT 帧（含 0x88A4）
    resp = my_esc.process(frame)   # 你的 ESC 逻辑，或转发给真实硬件
    s.send(resp)
```

**为什么这是护城河**：TAP 是 Linux 的能力，shim 是我们的接口；当第三方主站/测试工具/CI 都按这个规范接入时，我们成为"虚拟 EtherCAT 总线的默认接口"，而不是"又一个模拟器"。规范开放（Apache-2.0 或 CC-BY-4.0），实现与市场归我们。

### 2.6 第 1 周必须做的 5 个 spike

| # | Spike | 方法 | 期望结论 | 失败后备选 | 人日 |
|---|---|---|---|---|---|
| S1 | **IgH `ec_generic` over TAP** | 编译 IgH 1.6.13 + generic，`ip tuntap add tap0 mode tap`，`ethercat slaves` | TAP 可被 `ec_generic` 当网卡；promisc + 无 IP + link up 即可 | 换 veth；仍失败 → IgH 走 shim，TAP 只保 SOEM | 2 |
| S2 | **SOEM over TAP** | `slaveinfo tap0`（raw socket） | 无需 libpcap，`CAP_NET_RAW` 即可 | oshw shim；TAP 降级为 beta | 1 |
| S3 | **`0x88A4` 过 Docker bridge / veth** | 两个容器用自定义 EtherType 互发 | bridge 转发非 IP 帧；需排除 `br_netfilter` 干扰 | CI 用 shim；TAP 只跑 host 模式 | 1 |
| S4 | **Intel 网卡 offload 矩阵** | `ethtool -K` 逐项关 GRO/LRO/VLAN；发 `0x88A4` | 关闭 offload 后可稳定收发 | 文档化"支持网卡清单"；CI 用 USB/Realtek | 1 |
| S5 | **DC / 周期抖动** | 1 kHz、8 从站，TAP vs shim，测 p50/p99 | TAP p99 < 200 µs；DC 为仿真时钟，不宣称 µs 级 | 不承诺硬件级 DC；标"experimental" | 1 |

> ⚠️ **Go/No-Go**：S1 与 S2 任一失败，Linux 旗舰就必须降级为 shim；"未修改主站 + 真帧"的叙事要改写。**不要等到 M2 才发现。**

### 2.7 方案对比（1–5 分，越高越好）

| 维度 | A 网络级 TAP | B 端口 shim | 混合双通道 |
|---|---|---|---|
| 真实性/说服力 | 5 | 2 | 5（Linux）/ 3（其他） |
| 跨平台 | 1 | 5 | 5 |
| 无特权/CI | 1 | 5 | 4（CI 用 shim） |
| 工程可控性（越省事越高） | 2（最大） | 5（最小） | 3（两套适配器，但共用内核） |
| 叙事上限 | 5 | 2 | 5 |
| **总分** | 14 | 19 | **22** |

**推荐：混合双通道。** 不是两个产品，是**两个 L1 适配器 + 一个 L2 内核**。Linux 旗舰证明"真"，shim 保证"人人能跑"。

---

## §3 分层技术栈选型

> 本节结论：**Rust 做核心、独立线程做实时环、WASM 做插件沙箱、React 做控制面、单二进制交付。** 选型理由不是"先进"，是"实时性 + 内存安全 + 可嵌入 + 可分发"四个约束的交集。

| 层 | 选型 | 版本（2026-11；未标小版本以 stable 为准） | 理由 | 风险 | 备选 |
|---|---|---|---|---|---|
| 核心语言 | **Rust** | stable，edition 2024 | 无 GC、内存安全、wasmtime 同进程、单二进制、C ABI 友好 | 招聘曲线；编译时间；借用检查拖慢原型 | C++20（性能同档，但安全债高） |
| 跨语言边界 | **C ABI vtable** | `abi_version = 1` | 稳定 ABI；Tier1 插件；可被 C/Python/Go 嵌入 | `unsafe` 边界；结构体布局要冻结 | gRPC（重，不适合 RT） |
| 实时环 | **独立 `std::thread` + `SCHED_FIFO` + SPSC 环形队列** | `libc` 当前 stable | 与 tokio 物理隔离；无锁；可绑核 | 需 `CAP_SYS_NICE`/root；调度策略依内核 | RTIC（偏嵌入式）；tokio runtime（不可控） |
| 帧编解码 | **etherparse + 自研 EtherCAT datagram 层** | crates.io 当前 stable | 零拷贝解析 EtherType/L2；EtherCAT 头自己写更可控 | etherparse 不解析 EtherCAT 头；命令需自实现 | `pnet`（更重）；手写 |
| 内核旁路（可选） | **aya / AF_XDP** | 当前 stable | 时间戳、低抖动、绕开协议栈 | 驱动/网卡支持；复杂度高 | DPDK（重）；纯 TAP |
| ESC 内核 | **自研** | v0.1 | 差异化；确定性；可测试；许可干净 | 6–8 周；边界条件多（WKC/FMMU/SM/DC） | SOES（GPL，不可链接）；KickCAT（CeCILL-C，不可抄） |
| WASM 宿主 | **wasmtime 50 + Component Model + WASI 0.3** | 50.x | 同进程、低启动、fuel/epoch/内存上限、多语言 | API 漂移；WASI 0.3 成熟度 ⚠️ | wasmer（生态小）；wazero（无 Component Model） |
| 前端框架 | **React 19 + Vite** | 19.x / Vite 当前 stable | 生态/招人/组件库；Vite 快 | 包体积；并发渲染心智 | Svelte 5（更小，生态小） |
| 样式/组件 | **Tailwind 4 + Radix / shadcn** | 4.x | 速度快、可访问性、可定制 | 类名膨胀；设计一致性靠规范 | MUI（重，难定制） |
| 数据/表格 | **TanStack Query / Table / Virtual** | v5 | headless、TS 友好、虚拟滚动 | 学习曲线 | AG Grid（商业授权） |
| 图表 | **uPlot（实时波形）+ ECharts（拓扑/DC）** | 当前 stable | uPlot 万点级流畅；ECharts 拓扑/关系图成熟 | 两套库；主题统一成本 | Chart.js（性能弱） |
| 编辑器 | **Monaco** | 当前 stable | VSCode 同源；XML/YAML Schema 校验；ESI/OD 编辑 | 体积大；移动端弱 | CodeMirror 6（更轻） |
| 静态资源内嵌 | **rust-embed** | 8.x | 单二进制；无 Node 运行时 | 资源更新需重编译 | `include_dir` |
| 后端 | **axum** | 0.8.x | tokio 生态；WS/SSE；类型安全提取器 | 宏编译时间 | actix-web |
| MCP | **rmcp** | **3.9** | 官方 Rust SDK；工具/资源/提示原语 | **规范 2026-07-28 破坏性变更；SDK 可能滞后** ⚠️ | 手写 JSON-RPC |
| CLI | **clap** | 4.x | derive、补全、手册生成 | 无 | argh |
| 打包 | **musl 静态 + distroless Docker + 可选 Tauri 2** | — | 单文件；容器小；桌面可选 | 插件 glibc 兼容；Tauri 体积 | AppImage；Electron（否决） |
| 观测 | **tracing + OpenTelemetry** | 当前 stable | 标准；跨进程追踪 | 开销；采样策略 | Prometheus-only |
| 存储 | **SQLite（WAL）** | 3.x | 嵌入式；无服务；事务 | 并发写；迁移 | Postgres（重） |
| 配置 | **serde + JSON Schema** | 当前 stable | 声明式；可校验；可生成 UI | YAML 坑；Schema 演进 | TOML |

### 3.1 为什么不是 Go（这条要写进决策记录）

- **wazero 不支持 Component Model**，只支持 core module + WASI Preview 1；我们的插件体系要求"组件 + 强类型接口 + 多语言"，Go 宿主会被迫自研或退回 core module。
- **GC 抖动 + cgo 成本**：1 kHz 环里，Go 的 STW 即使 100 µs 也不可接受；cgo 调用开销与生命周期管理，做 Tier1 原生插件不划算。

> 结论：Go 适合做**控制面/CLI 工具**，不适合做**ESC 内核 + 实时环 + WASM 宿主**。不要为了"团队会 Go"把核心做成两套。

### 3.2 为什么不是纯 C++

C++ 能做到同样性能，但：内存安全债、依赖管理、跨平台构建、WASM 宿主绑定都更贵；而 Rust 的 `wasmtime` 是同一家（Bytecode Alliance）的一等公民。**C++ 的位置是 Tier1 插件和既有 IP 复用**，不是核心。

### 3.3 实时环长什么样

```rust
// 伪代码：RT 线程只做四件事，绝不 await、绝不分配
fn rt_loop(mut io: FrameIo, mut esc: EscCore, arena: PdoArena, rx: SpscConsumer) {
    set_sched_fifo(80); mlockall();
    let mut next = Instant::now();
    loop {
        next += CYCLE;                              // 1 ms
        let frame = io.recv_until(next);            // 阻塞到点
        let resp = esc.tick(frame, &mut arena);     // 寄存器/FMMU/SM/AL/DC
        io.send(&resp);
        while let Some(cmd) = rx.try_pop() {        // 控制面命令，非阻塞
            esc.apply(cmd);                         // 只做 O(1) 操作
        }
        sleep_until(next);                          // clock_nanosleep(ABSOLUTE)
    }
}
```

**纪律**：

- tokio 运行时**不碰** RT 线程；控制面通过 SPSC 队列 + `watch` 通道通信。
- RT 线程内**零堆分配、零锁、零日志格式化**；日志走 ring buffer，由控制面消费。
- 每周期预算：8 从站、64 字节 PDO，目标 < 250 µs；超时计数并降级（丢帧而不是阻塞）。

---

## §4 插件体系：商业模式的根

> 本节结论：**四层插件不是"架构洁癖"，是实时性、IP 保护、可终止性三个约束的唯一解。** 市场默认 Tier2（WASM），高价值闭源走 Tier3，性能敏感走 Tier1，标准 Profile 走 Tier0。

### 4.1 四层插件模型

| Tier | 形态 | 运行位置 | 延迟预算（设计目标） | 谁可以发布 | 隔离/终止 | 典型用途 |
|---|---|---|---|---|---|---|
| **T0** | 内置 Rust trait | 编译进核心，RT 环 | < 10 µs | 我们 | 进程级；随核心升级 | 8DI/8DO/CiA402 标准 Profile |
| **T1** | 原生 dylib（C ABI） | 独立线程；白名单可 RT 直调 | 50–200 µs | 签约 ISV（签名 + 审核） | 签名 + WCET 契约 + 看门狗；崩溃由宿主进程兜底 | 高性能伺服算法、专有协议、既有 C/C++ IP |
| **T2** | WASM Component | wasmtime 线程池 | 1–5 ms | 任何开发者（市场默认） | fuel/epoch/内存上限；无 ambient 权限；可即时终止 | 市场长尾、ESI/OD 声明式 + 轻逻辑 |
| **T3** | 独立进程 | UDS + 共享内存 | 5–50 ms | 不可信/远程/企业私有 | cgroup/namespace；`kill -9`；远程鉴权 | 闭源高价值、需要远程许可、客户自托管 |

> 延迟是**设计目标**，不是实测；M3 用 1 kHz/8 从站基准固化，写入文档。

### 4.2 为什么不能只做一种

| 只做 | 会发生什么 |
|---|---|
| **只做 WASM** | 1 kHz 环承受不了 JIT 预热/epoch 检查/跨边界拷贝；需要原生库的厂商无法接入；WASM 反编译成本低，高价值 IP 不敢放 |
| **只做原生 dylib** | 一个野插件段错误拖垮整个模拟器；市场无法审核；无法远程吊销；不可信开发者不敢收 |
| **只做独立进程** | 每周期 IPC 往返 5–50 ms，做不了 1 kHz；部署复杂；小插件也要起进程 |
| **只做内置** | 每接一个设备都要发版；长尾永远覆盖不完；没有市场 |

**所以必须分层**：T0 保证开箱即用，T2 保证市场可规模化，T3 保证"能杀 + 能吊销"，T1 给性能/存量 IP 一个出口。

### 4.3 插件包格式

```
acme-servo-1.2.0.zip
├── manifest.json          # JSON Schema 校验；含 abi_version / product_major
├── manifest.sig           # Ed25519，对 manifest + 所有载荷的规范哈希签名
├── esi/ACME_SD750.xml     # 厂商 ESI（需授权，见 §7）
├── od/od.yaml             # OD/PDO 声明（声明式优先，见 4.5）
├── wasm/servo.wasm.enc    # XChaCha20-Poly1305 加密（license 派生密钥）
├── sbom.cdx.json          # CycloneDX SBOM
└── LICENSE / EULA.pdf
```

**manifest 必填字段**：

| 字段 | 说明 |
|---|---|
| `id` | 反向域名，全局唯一 |
| `version` | SemVer |
| `abi_version` | 宿主 ABI 主版本；不匹配拒绝加载 |
| `product_major` | 设备产品主版本；ESI/OD 变更时递增 |
| `vendor_id` / `product_code` / `revision` | 与 ESI/0x1018 一致 |
| `entrypoints` | `t0/t1/t2/t3` 各自的入口；T2 为 WIT world |
| `permissions` | 需要的控制面能力（读 SDO、写 PDO、网络、文件） |
| `signature` | 发布者 Ed25519 公钥指纹 + 签名 |

### 4.4 许可与吊销（entitlement）

1. 购买/订阅 → license server 签发**离线 JWT**：`sub`、`plugin_id`、`machine_id`、`exp`、`entitlements`、`iss`。
2. 客户端离线校验签名 + machine binding + TTL；时钟回拨检测。
3. 在线时拉**吊销列表（CRL）**，默认 24 h；离线超过 TTL 则降级为只读/试用。
4. 解密 WASM 载荷：`key = HKDF(JWT, plugin_id)`；密钥只在内存中存在。
5. 启动 wasmtime：fuel/epoch/内存上限；越界即终止。
6. 吊销后：下次启动拒绝；运行中通过 epoch 中断。

> **诚实说明**：WASM **不是强 IP 保护**。字节码可 dump，密钥在内存中可被调试器读取。高价值算法请用 **T3 独立进程 + 服务端鉴权**，或 T1 原生库 + 法律合同。不要把"加密的 WASM"当 DRM 卖。

### 4.5 对象字典的"声明式优先"设计（回应"开放 PDO 生态"）

**大多数设备厂商不需要写代码。** 他们只需要提交：

1. 厂商 ESI XML（设备描述）；
2. 一份 OD/PDO 声明（YAML/JSON）：对象索引、类型、访问、默认 PDO 映射。

核心编译成：**SII 镜像 + 对象字典 + 默认 PDO 分配 + SM/FMMU 配置**。厂商零代码就能得到一个真实设备的 PDO 画像。

```yaml
id: com.acme.servo.ds402
abi_version: 1
product_major: 3
esi: esi/ACME_SD750.xml
device: ACME-SD750
od_pdo:
  rx: [0x6040:0, 0x607A:0, 0x6060:0, 0x6071:0]
  tx: [0x6041:0, 0x6064:0, 0x6061:0, 0x6077:0]
behavior: wasm/servo.wasm   # 省略 = 纯声明式，核心用默认 CiA402 行为
```

**只有需要行为逻辑时才写代码**：CiA402 状态机、自定义协议、特殊错误处理。这时才升级到 T2/T3。

> 这条设计直接决定市场供给量：如果每个插件都要写 C/Rust，市场永远只有几十个；如果只要 ESI + 8 行 YAML，市场可以有几千个。

### 4.6 市场治理：Catalog 与 Marketplace 分开

| 维度 | **Catalog（目录）** | **Marketplace（市场）** |
|---|---|---|
| 内容 | 开源插件 | 可闭源、可付费 |
| 许可证 | 仅 OSS 白名单（Apache/MIT/BSD；LGPL 动态链接需法务确认） | 厂商 EULA；核心不感染 |
| 审核 | 自动构建 + 签名 + SBOM + 许可证扫描 | 人工审核 + 恶意代码扫描 + 法务 |
| 分发 | 源码/可复现构建 | 加密载荷 + entitlement |
| 收费 | 免费 | 抽成（建议 20–30%）+ 订阅 |
| 下架 | 社区举报 + 维护者 | 侵权投诉 + 安全事件 + 远程吊销 |
| 参考 | Grafana Catalog | Grafana 商业插件（但**核心不用 AGPL**） |

**治理红线**：

- **签名**：所有上架插件必须 Ed25519 签名；密钥轮换有公告期。
- **SBOM**：CycloneDX，随包分发；CI 扫描高危依赖。
- **EULA**：市场方与开发者/买家三方条款；明确"仿真，不用于安全相关认证"。
- **侵权**：ESI 版权归厂商；收到投诉 24 h 下架，走 counter-notice。
- **出口管制**：加密插件/远程鉴权需法务评估。

---

## §5 内置对象字典与开箱 Profile

> 本节结论：**核心只内置 3 个 Profile，其余全部插件。** 这不是"功能少"，是产品哲学：核心小、升级稳、市场有活路。

### 5.1 必选标准对象（每个从站都必须有）

| 索引 | 名称 | 类型 | 访问 | 默认/说明 |
|---|---|---|---|---|
| `0x1000` | Device type | UNSIGNED32 | ro | 伺服 `0x00020192`（CiA402）；DI/DO `0x00000000` |
| `0x1008` | Device name | VISIBLE_STRING | ro | 来自 ESI `<Name>` |
| `0x1009` | Hardware version | VISIBLE_STRING | ro | 来自 ESI |
| `0x100A` | Software version | VISIBLE_STRING | ro | 核心版本 + 插件版本 |
| `0x1018` | Identity | RECORD（4 sub） | ro | sub1 VID、sub2 product code、sub3 revision、sub4 serial |
| `0x1C00` | SM type | ARRAY of U8 | ro | `[1,2,3,4]`：mailbox out/in、process out/in |
| `0x1C12` | RxPDO assign | ARRAY of U16 | rw | 默认 `[0x1600]` |
| `0x1C13` | TxPDO assign | ARRAY of U16 | rw | 默认 `[0x1A00]` |
| `0x1600` | RxPDO mapping | RECORD | rw | 模板；按 Profile 填 |
| `0x1A00` | TxPDO mapping | RECORD | rw | 模板；按 Profile 填 |
| `0x1C32` | SM2 sync | ARRAY | rw | 周期/移位/模式；DC 相关 |
| `0x1C33` | SM3 sync | ARRAY | rw | 同上 |

### 5.2 三个开箱 Profile

| Profile | 关键对象 | 默认 RxPDO（SM2） | 默认 TxPDO（SM3） | ESI 对应 |
|---|---|---|---|---|
| **8DI** | `0x6000`（8×BOOL） | 无 | `0x1A00` ← `0x6000:1..8`（1 byte） | `<TxPdo Sm="3">` |
| **8DO** | `0x7000`（8×BOOL） | `0x1600` ← `0x7000:1..8`（1 byte） | 无 | `<RxPdo Sm="2">` |
| **虚拟伺服（CiA402 CSP）** | `0x6040` 控制字、`0x6041` 状态字、`0x6060` 模式、`0x6061` 模式显示、`0x6064` 实际位置、`0x607A` 目标位置、`0x6071` 目标扭矩 | `0x1600` ← `0x6040, 0x607A, 0x6060, 0x6071` | `0x1A00` ← `0x6041, 0x6064, 0x6061, 0x6077` | `<RxPdo>` / `<TxPdo>` + `<Dc>` |

**CiA402 状态机**必须真的跑：`Switch on disabled → Ready to switch on → Switched on → Operation enabled`，以及 Fault / Quick stop。控制字 bit0/1/2/3/7、状态字 bit0–3/5/6/9/10/11 的语义不能错——主站（尤其 TwinCAT）会用这些位判断"伺服是否真的准备好了"。

### 5.3 ESI 与 PDO 的对应关系（实现者最容易错的地方）

| ESI 元素 | 生成 SII category | 生成 OD | 默认 SM |
|---|---|---|---|
| `<RxPdo Sm="2">` | `0x0033`（RxPDO） | `0x1600` + `0x1C12` | SM2（输出） |
| `<TxPdo Sm="3">` | `0x0032`（TxPDO） | `0x1A00` + `0x1C13` | SM3（输入） |
| `<Mailbox>` | `0x0029`（SM 配置） | — | SM0/SM1 |
| `<Dc>` | `0x003C`（DC） | `0x1C32`/`0x1C33` | — |
| `<Eeprom>` | `0x001E`（General） | `0x1018` | — |

> ⚠️ **SII category 数值是高频错误点**：`0x0029` 是 SyncManager，`0x0032` 是 TxPDO，`0x0033` 是 RxPDO；`0x0028`/`0x002A` 是 FMMU 及其扩展，不是 PDO（[etherage CategoryType](https://docs.rs/etherage/0.5.1/etherage/sii/enum.CategoryType.html)）。网上大量文章把三者写成 `0x0028/0x0029/0x002A`，照抄会做出主站读不到 PDO 的"假从站"。

### 5.4 产品哲学：内置只有这几个

- **核心小**：3 个 Profile + 标准对象，二进制可控，升级不破坏兼容；**其余全插件**——每多一个内置设备就多一份测试矩阵和发版负担，长尾应该由市场解决。
- **内置是"质量基准"**：这三个 Profile 必须通过 IgH/SOEM/TwinCAT/ethercrab 四主站验收；插件可以只过其中两个（在清单里标注）。

---

## §6 主站兼容性矩阵

> 本节结论：**验收标准只有一条——`ethercat slaves` 与 SOEM `slaveinfo` 零补丁看到 OP。** 其他都是辅助。

### 6.1 四类主站的角色

| 主站 | 版本 | 接入方式 | 发现/寻址 | 在我们这里的角色 | 已知坑 |
|---|---|---|---|---|---|
| **IgH** | 1.6.13 | 内核 `ec_generic` + `tap0`；或 `libethercat` shim | 自动增量 + 配置站地址/alias | **Linux 旗舰验收** | `ec_generic` 对 netdev 的要求；DC；alias/position 语义 |
| **SOEM** | 2.x | `oshw` + `tap0`；或自定义 oshw → UDS | `ec_config_init` 自动增量 | **CI 主力**（GPL 隔离在容器） | raw socket 权限；PDO map 顺序；`slaveinfo` 细节 |
| **TwinCAT** | 3.x | Windows + 实时网卡；VM 直通 | ESI 扫描 + EEPROM | **客户现场验收**（不进 CI） | 许可；Windows；实时网卡；ESI 严格性 |
| **ethercrab** | 0.7 | 纯 Rust；`tx_rx_task` + 自定义收发循环 | `init` + 配置 | **开发者生态/跨平台** | async 运行时；自定义 backend；文档少 |

### 6.2 关键行为差异（验收时必须逐项对齐）

> 来源：esc-protocol 报告 §8.1 + 各主站文档；⚠️ 具体语义以实现为准，M1 用测试固化。

| 行为 | IgH | SOEM | TwinCAT | ethercrab | 我们要做的 |
|---|---|---|---|---|---|
| 帧组织 | 每周期一帧多 datagram | 类似，`ecx_send_processdata` | 可能拆多帧 | 可配置 | 支持任意 datagram 组合；不假设顺序 |
| 从站发现 | 广播读 0x0000/0x0010 | `ec_config_init` | ESI + EEPROM 扫描 | `init` | 自动增量地址必须正确递增 |
| 站地址/alias | alias + position | `configadr` | 拓扑位置 | 配置 | 0x0010/0x0014 语义一致 |
| SII 读取 | 字/双字读 | `ec_SIIread` | EEPROM 读 | 读 SII | SII 校验和、category 链必须合法 |
| 邮箱 | SM0/SM1 大小来自 SII | `mbx_*` | 严格 | 支持 | 邮箱满/空、重试、错误码 |
| PDO 分配 | 写 0x1C12/0x1C13 | `ec_config_map` 读 | Startup 写 | 读/写 | 写入后立即生效；越界拒绝 |
| DC | `ecrt_slave_config_dc` | `ec_configdc` | DC 选项卡 | `DcConfiguration` | 0x1C32/0x1C33 + 0x09xx 一致 |
| 状态机 | AL control/status + ACK | `ec_statecheck` | 严格 | 支持 | 状态转换时序、错误码 |
| 看门狗 | `watchdog_time` | `watchdog` | 严格 | 支持 | 0x0400/0x0420；超时进 SAFEOP/ERROR |
| 帧大小 | 1518 MTU | 同 | 同 | 同 | 多帧/分片边界 |

### 6.3 验收标准（写进合同/里程碑）

| # | 验收项 | 通过标准 | 测量方式 |
|---|---|---|---|
| A1 | IgH 看到 OP | `ethercat slaves` 全部 `OP`，无 `E` 标志 | 录屏 + 命令输出存档 |
| A2 | SOEM 看到 OP | `slaveinfo` 全部 `State: OP`，VID/PID/PDO 大小正确 | CI 日志 |
| A3 | 零补丁 | 主站源码未改；仅配置文件 + 我们文档化的 TAP/shim 接口 | 构建哈希 + diff |
| A4 | PDO 一致 | `ethercat pdos` / SOEM 输出与 ESI 映射逐字节一致 | 自动比对 |
| A5 | SDO 正确 | 读 0x1018 四子项、0x1000、0x1008 正确 | 自动断言 |
| A6 | DC（若声明） | 从站跟随 SYNC0；漂移在标称范围内 | 示波器/日志；**不宣称 µs 级** |
| A7 | 故障注入 | 丢帧/断链/CRC 错时，主站进入预期状态并可恢复 | 脚本化用例 |

> **"零补丁"定义**：不改主站源码、不打内核补丁、不 `LD_PRELOAD` 非文档化库；允许改配置文件、允许使用我们发布的 `tap`/shim 适配器。含糊的定义会在验收时吵架。

### 6.4 CI 矩阵

| 维度 | 取值 | 频率 |
|---|---|---|
| 主站 | SOEM 2.x、IgH 1.6.13、ethercrab 0.7 | 每 PR（SOEM/ethercrab）；IgH nightly |
| 线缆 | UDS shim（无特权）、TAP（host runner） | shim 每 PR；TAP nightly |
| Profile | 8DI、8DO、伺服 | 每 PR |
| 规模 | 1 / 8 / 32 从站 | nightly |
| 故障 | 丢帧、断链、CRC、错序、DC 漂移 | nightly |
| 主站验收 | TwinCAT（Windows lab） | 每里程碑 |

---

## §7 许可与合规

> 本节结论：**核心 Apache-2.0 OR MIT，GPL 只做独立进程，市场必须能吊销，ETG 书面澄清是 P0。** 任何"先上线再补"的想法都会在融资尽调或大客户采购时爆炸。

以下为工程判断，不构成法律意见；正式发布前须由律师出具意见。

### 7.1 许可证组合表

| 组件 | 许可证 | 能否进核心 | 允许的使用方式 | 义务 | 结论 |
|---|---|---|---|---|---|
| 我们的核心 | **Apache-2.0 OR MIT** | ✅ | 任意 | 保留版权/许可声明；专利授权 | 推荐 |
| 我们的 ABI 头文件 | Apache-2.0 OR MIT | ✅ | 插件可自由链接 | 同上 | 推荐 |
| IgH 内核模块 | GPLv2 | ❌ | 仅作为外部主站进程/VM | 分发二进制须提供源码 | 测试夹具 |
| IgH 用户态库 | LGPL-2.1 | ⚠️ | 动态链接可闭源 | 修改库须开源 | 尽量不依赖 |
| SOEM 2.x | GPLv3 / 商业 | ❌ | 独立进程/容器；或购买商业许可 | GPLv3 传染；商业许可付费 | 测试夹具 |
| LinuxCNC | GPLv2 | ❌ | 独立进程/容器模板 | 整体 GPLv2 | 模板市场 |
| KickCAT | CeCILL-C | ❌ | 读设计、独立实现；或独立进程集成 | 修改其文件须 CeCILL-C | 对照实现 |
| SOES | GPLv2 + 文件级例外 | ❌ | 不链接；仅参考 | 例外不覆盖整体 | 不用 |
| WASM 插件 | 开发者自选 | ✅（沙箱） | 市场分发 | 无链接传染 | 推荐 |
| 第三方 ESI | 厂商版权 | ❌ | 用户本地导入；不随产品分发 | 需厂商授权才能分发 | 见 7.4 |
| EtherCAT 商标 | Beckhoff/ETG | — | 描述性使用；遵守 ETG.9001 | 不得暗示认证 | 法务审核 |

### 7.2 P0 行动清单（D0 = 立项日）

| # | 行动 | 为什么现在做 | 负责 | 截止 | 产出 |
|---|---|---|---|---|---|
| P0-1 | **加入 ETG**（免费） | 拿规范、SSC、工作组资格；会员身份是 VID 前提 | CEO/法务 | D0+7 | 会员确认邮件 |
| P0-2 | **申请 Vendor ID**（免费） | 仿真从站的 Identity 需要合法 VID；开发期可用占位，对外发布须用自有/授权 VID | 法务 | D0+14 | VID 分配 |
| P0-3 | **签署 MainDevice License**（免费） | 主站兼容性的法律确定性；ETG FAQ 明确免费 | 法务 | D0+14 | 签署副本 |
| P0-4 | **书面澄清"纯软件从站仿真"** | 是否触发 SubDevice License、是否可用 "EtherCAT" 命名 | CEO/法务 | D0+7 发出；D0+45 期待回复 | ETG 书面答复 |
| P0-5 | **商标/命名审查** | 产品名、域名、UI 文案、市场材料 | 法务 | 公测前 | 审查备忘录 |
| P0-6 | **第三方 ESI 政策** | 不分发厂商 ESI；用户导入；市场需授权 | 法务/产品 | 市场 MVP 前 | 政策 + 下架流程 |
| P0-7 | **CI 许可证扫描** | `cargo-deny` + `reuse` + SBOM | 工程 | D0+7 | CI job |
| P0-8 | **GPL 夹具隔离验证** | SOEM/LinuxCNC 只在独立容器；无链接 | 工程 | M1 | 隔离说明 + 镜像 |
| P0-9 | **EULA/隐私政策** | 市场交易、数据（总线抓包可能含客户机密） | 法务 | M3 | 文档 |
| P0-10 | **出口/加密合规** | WASM 加密、远程鉴权可能触发 | 法务 | M3 | 评估 |

### 7.3 商标与命名规则（可直接给市场）

- 使用 **"EtherCAT®"** 时保留符号，首次出现注明"EtherCAT® 是 Beckhoff Automation GmbH 的注册商标"；不使用 ETG/Beckhoff logo，除非会员权益明确允许；不暗示"ETG 认证/一致性测试通过"。
- 产品名建议：**"EtherCAT-compatible simulator"** 或 **"EtherCAT 从站仿真器"**，不用 "EtherCAT Stack" 这类易混淆表述。
- 市场页面加免责声明：**"本产品用于开发/测试，不替代一致性测试；不得用于功能安全认证。"**

### 7.4 第三方 ESI 政策

1. **不分发**厂商 ESI；只提供导入入口与本地缓存（用户数据目录）。
2. 市场插件若需内置 ESI，必须上传**厂商授权书**；否则只允许"用户自备 ESI"。
3. 收到厂商投诉：24 h 下架 + 通知发布者 + counter-notice 流程。
4. 文档明确：ESI 版权归厂商；我们只做格式解析。

### 7.5 CI 许可证扫描（最小实现）

```bash
cargo deny check licenses bans sources     # Rust 依赖
reuse lint                                 # SPDX 头
syft dir:. -o cyclonedx-json > sbom.cdx.json
```

**政策**：核心依赖白名单 = Apache-2.0 / MIT / BSD-2/3 / ISC / Zlib / Unicode；LGPL 需法务逐项批准；GPL/AGPL **一律禁止**进入核心依赖图。

---

## §8 路线图

> 本节结论：**Spike 1 周先证伪，M1 只做"最小闭环"，M2 才加网络级与 Web，M3 才做市场。** 任何"先做市场再做内核"的顺序都会死。

### 8.1 Spike（第 1 周）

见 §2.6 的 5 项。**退出标准**：5 项都有明确结论（可行/不可行/降级方案），并写入 ADR（架构决策记录）。**Go/No-Go**：S1/S2 至少一个通，否则主线改 shim。

### 8.2 里程碑

| 阶段 | 周期（估算） | 范围 | 退出标准（必须可演示） | 止损线 |
|---|---|---|---|---|
| **M1 最小闭环** | 6–8 周 | ESC 内核（寄存器/FMMU/SM/AL/SII/WDT）、UDS shim、SOEM + IgH 兼容、CLI、8DI/8DO | `ethercat slaves` 与 `slaveinfo` 零补丁 OP；8 从站 @1 kHz；p99 抖动 < 100 µs（shim，隔离 CPU）；Docker 无特权跑通 | 第 10 周仍不能 OP → 砍范围到 SOEM-only，IgH 延后 |
| **M2 网络级 + Web** | 6–8 周 | TAP 真帧、WebUI（拓扑/PDO 波形/SDO 编辑/ESI 导入）、DC 仿真、伺服 Profile、故障注入 v1 | 未修改 IgH 通过 `tap0` 进 OP；32 从站 @1 kHz；Web 实时显示；5 种故障可注入/恢复 | TAP 仍不稳 → 旗舰改 shim，TAP 标 beta |
| **M3 插件宿主 + 市场 MVP** | 6–8 周 | T0/T2 宿主、包格式、签名、entitlement、OD 编译器（ESI + YAML → SII/OD）、Catalog/Marketplace | 3 个第三方插件（含 1 个付费）；零代码 ESI 接入 < 10 分钟；签名/吊销全链路 | 3 个月无 3 个插件 → 市场转"模板库"，先不抽成 |
| **M4 MCP + Skills + CI 报告** | 4–6 周 | MCP server（2026-07-28 无状态）、Skills、Python SDK、JUnit/HTML 报告、GitHub Action | 任一 MCP 客户端可完成 scan → config → run → report；Action 产出可读报告 | SDK 不支持新规范 → 先做 2025-11-25 兼容层 |
| **M5 深水区** | 持续 | T3 独立进程、故障矩阵 10+、LinuxCNC 模板市场、Windows shim beta、KickCAT 互操作 | 10 个付费插件；3 家设计伙伴；Windows beta 可用 | 付费转化 < 2% → 转企业授权/服务 |

### 8.3 团队与关键路径（估算）

| 角色 | 人数 | 关键产出 |
|---|---|---|
| Rust 系统/实时 | 1 | L2 内核、RT 环、性能 |
| 协议/ESC | 1 | 寄存器/FMMU/SM/AL/DC/SII、主站兼容 |
| 全栈/前端 | 1 | WebUI、MCP、SDK、市场前端 |
| DevRel/法务（兼职） | 0.5–1 | ETG、插件作者、文档、合规 |

**关键路径**：ESC 内核 → IgH/SOEM 兼容 → TAP → 插件宿主 → 市场。**Web 不是关键路径**，但它是演示与融资的杠杆。

### 8.4 M1 明确不做（防止 scope creep）

- ❌ WebUI、插件市场/签名/支付
- ❌ Windows 原生、DC 精度承诺（只做"仿真时钟"）
- ❌ TwinCAT 自动化、T1/T3 插件（只做 T0 + 未来 T2 的接口占位）

---

## §9 关键风险

> 本节结论：**两个 P0 风险（TAP+IgH、ETG 澄清）决定项目生死；其余是 P1，可用架构缓解。** 每个风险都要有"触发信号"和"止损线"，否则会拖到没钱。

| # | 风险 | 触发信号 | 影响 | 对策 | 止损线 |
|---|---|---|---|---|---|
| R1 | **TAP + IgH `ec_generic` 不兼容** | Spike S1 失败；`ethercat slaves` 无响应 | "真帧 + 未修改主站"叙事破产 | 降级 shim 旗舰；TAP 仅 SOEM/beta；文档写清 | 第 2 周仍无 OP → 主线切 shim，TAP 移出 M2 |
| R2 | **KickCAT 产品化/被收编** | 发布 Web UI/插件市场/AI 功能；或获大厂投资 | 窗口关闭，被正面比较 | 加速 + 开放 UDS 规范 + 市场冷启动；评估合作 | 6 个月内无合作且对方出 Web → 退守"AI + CI"细分 |
| R3 | **ETG 书面澄清迟迟不回/否定** | 8 周无答复；或要求 SubDevice License | 命名/分发/市场合规风险 | 会员 + 律师意见 + 免责声明；准备"EtherCAT-compatible"备选名 | 12 周无答复 → 限制商标使用，法务出风险评估后再发布 |
| R4 | **WASM 被提取/盗版** | 出现 dump 工具；付费插件被逆向 | IP 流失，ISV 不信任 | T3 优先；载荷加密；服务端鉴权；法律条款 | 首个高价值插件被逆向 → 强制 T3，WASM 只做免费/低价值 |
| R5 | **MCP 规范漂移** | 新 spec 删除/改名 API；SDK major 升级 | M4 返工 | 适配层 + 版本协商 + conformance 测试；锁 rmcp 版本 | 2 周内不能迁移 → 锁旧版，双栈并行 |
| R6 | **SOEM GPLv3 / 安全公告** | CVE；或客户法务拒绝 GPL 夹具 | CI 污染；交付受阻 | 独立容器、固定版本、镜像扫描；必要时买商业许可 | CVE 影响核心镜像 → 移除 SOEM 夹具，换 ethercrab |
| R7 | **实时性不达标** | p99 > 1 ms；客户现场抖动 | "1 kHz"卖点失效 | 隔离 CPU、关 offload、AF_XDP；诚实标称 | 3 个月做不到 100 µs → 放弃硬实时叙事，转"功能验证" |
| R8 | **ESI 侵权投诉** | 厂商律师函 | 市场下架、声誉 | 不分发政策；授权书；24 h 下架 | 首起投诉 → 全量审计 ESI 来源 |

**风险预算**：R1/R3 是 P0，必须在 M1 前有结论；R2 是战略风险，每季度复盘；R4–R8 用架构/流程缓解，不阻塞 M1。

---

## §10 待你拍板的 6 个决策

> 本节结论：**先拍 D1/D2/D3，它们决定架构；D4/D5 决定商业模式；D6 是外交，越早越好。**

| # | 决策 | 我的建议 | 备选 | 不决定的后果 |
|---|---|---|---|---|
| **D1** | 双主站（IgH + SOEM）开箱是不是第一优先级？ | **是，P0。** 这是"未修改主站"的唯一可信证明；只做 SOEM 会被当成"又一个 SOEM 封装" | 先 SOEM，IgH 放 M2 | 失去最难替代的信任状；销售只能讲"支持 SOEM" |
| **D2** | 核心许可 Apache-2.0 OR MIT，是否双许可给 GPLv2 生态？ | **Apache-2.0 OR MIT，不做 GPLv2 双许可。** 多数企业法务对 GPLv2 保守；双许可会污染贡献者预期 | 双许可（GPLv2 + 商业） | 大客户采购卡住；插件作者不敢基于核心开发 |
| **D3** | Windows 是否一级支持？ | **否，二级（shim + WebUI）。** Linux 旗舰做"真"，Windows 做"广"；TAP-Windows6 是 GPLv2 且体验差 | 是，做 TAP-Windows6 旗舰 | 线缆层工作量翻倍、GPL 驱动分发、M1 延期 4–6 周 |
| **D4** | 插件默认沙箱：WASM 还是 Tier3-only？ | **WASM 默认 + Tier3 给高价值。** 市场需要低门槛；IP 敏感的走独立进程 | 仅 Tier3（更安全，但市场冷启动慢） | 选错会导致"市场没供给"或"ISV 不敢上架" |
| **D5** | 商业模式边界：免费核心 + 市场抽成，还是双版本？ | **免费核心 + 市场抽成（20–30%）+ 企业支持。** 核心开源建立信任；钱从"连接"和"合规"赚 | 双版本（社区版/企业版功能阉割） | 双版本会分裂社区、增加测试矩阵；抽成模式变现慢 |
| **D6** | 是否现在就联系 KickCAT 上游合作？ | **2 周内接触，探索性。** 可谈互操作、ESI 工具、甚至联合标准；但核心 clean-room | 不联系，直接竞争 | 重复造轮；被对方先发制人；公开比较时被动 |

**决策依赖**：

```
D2（许可）──▶ D5（商业模式）──▶ D4（沙箱默认）
   └──▶ D1（双主站）──▶ D3（Windows 优先级）
D6（KickCAT）──────────────────┘（影响叙事与节奏，不改变架构）
```

**如果只能先拍一个：D2。** 许可定错，后面所有架构都要重做；许可定对，D1/D3/D4 都有回旋余地。

---

## 附录 A：关键事实核查表

> 这 8 条是网上错误信息最密集、且会直接导致返工/合规风险的事实。全部按 2026-11 一手来源核对。

| # | 常见错误说法 | 正确事实 | 来源 |
|---|---|---|---|
| A-1 | "SOEM 2.x 是 GPLv2（带链接例外）" | **SOEM 2.x 是 GPLv3 + 商业双许可**；闭源商用需购买 | [LICENSE.md](https://github.com/OpenEtherCATsociety/SOEM/blob/master/LICENSE.md) |
| A-2 | "SOES 是 GPLv2 + 链接例外，可以随便链接" | GPLv2 + **文件级例外**：仅覆盖"本文件被实例化/宏/内联/链接"，不构成整体链接例外 | [SOES LICENSE](https://github.com/OpenEtherCATsociety/SOES/blob/master/LICENSE) |
| A-3 | "SII 的 PDO category 是 0x0028/0x0029/0x002A" | **0x0029 = SyncManager，0x0032 = TxPDO，0x0033 = RxPDO**；0x0028/0x002A 是 FMMU 及其扩展 | [etherage CategoryType](https://docs.rs/etherage/0.5.1/etherage/sii/enum.CategoryType.html) |
| A-4 | "IgH `libfakeethercat` 能模拟总线错误/真帧" | **不能**：PDO 级、RtIPC 共享内存；SDO 配置只 dump JSON；官方明说 bus error 不能模拟 | [IgH 1.6 文档](https://docs.etherlab.org/ethercat/1.6/doxygen/libfakeethercat.html) |
| A-5 | "ETG 会员要收费/有分级；VID 要钱" | **会员免费、无分级；VID 免费；MainDevice License 免费**（需签协议） | [会员权益](https://www.ethercat.org/en/membership_benefits.html)、[FAQ 3.3](https://www.ethercat.org/en/faq.html)、[VID](https://www.ethercat.org/en/vendor_id.html) |
| A-6 | "WinTun 可以跑 EtherCAT" | **WinTun 是 L3（IP）**，不承载 EtherType `0x88A4`；Windows 只能 shim、GPL 的 TAP-Windows6，或付费 OEM 的 Npcap | [WinTun](https://git.zx2c4.com/wintun/about/)、[tap-windows6](https://github.com/OpenVPN/tap-windows6)、[Npcap OEM](https://npcap.com/oem/) |
| A-7 | "MCP 2026-07 只是弃用 initialize" | **已删除** `initialize`/`initialized` 与 `Mcp-Session-Id`；改为无状态请求 + `Mcp-Method`/`Mcp-Name` 头 | [MCP 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/)、[变更说明](https://raw.githubusercontent.com/microsoft/mcp-for-beginners/refs/heads/main/01-CoreConcepts/mcp-2026-07-28.md) |
| A-8 | "EtherCAT 是开源协议，随便实现" | ETG 明确 **"EtherCAT technology itself is not Open Source"**；实现需许可/会员，商标受保护 | [ETG FAQ 3.4](https://www.ethercat.org/en/faq.html) |

---

## 附录 B：参考实现与链接清单

### 协议与 ESC

- IgH EtherCAT Master（1.6.13）：https://gitlab.com/etherlab.org/ethercat
- IgH `libfakeethercat` 文档：https://docs.etherlab.org/ethercat/1.6/doxygen/libfakeethercat.html
- IgH RtIPC：https://gitlab.com/etherlab.org/rtipc
- SOEM（2.x，GPLv3/商业）：https://github.com/OpenEtherCATsociety/SOEM
- SOES（GPLv2 + 文件级例外）：https://github.com/OpenEtherCATsociety/SOES
- KickCAT（CeCILL-C，C++，主/从站 + 仿真）：https://github.com/leducp/KickCAT
- KickCAT 仿真文档：https://github.com/leducp/KickCAT/blob/master/docs/SIMULATION.md
- ethercrab（Rust 主站，0.7）：https://github.com/ethercrab-rs/ethercrab
- etherage（Rust，SII 类型参考）：https://docs.rs/etherage/0.5.1/etherage/sii/enum.CategoryType.html
- LinuxCNC：https://www.linuxcnc.org/

### EtherCAT 组织与合规

- ETG 会员权益（免费）：https://www.ethercat.org/en/membership_benefits.html
- ETG Vendor ID（免费）：https://www.ethercat.org/en/vendor_id.html
- ETG FAQ（MainDevice License 免费、非开源、商标）：https://www.ethercat.org/en/faq.html
- ETG 下载/规范入口：https://www.ethercat.org/en/downloads.html
- ETG 一致性测试：https://www.ethercat.org/en/conformance.html
- Beckhoff ESI 下载：https://download.beckhoff.com/download/configuration-files/io/ethercat/xml-device-description

### 线缆/虚拟网卡

- Linux TUN/TAP 内核文档：https://www.kernel.org/doc/html/latest/networking/tuntap.html
- WinTun（L3）：https://git.zx2c4.com/wintun/about/
- TAP-Windows6（GPLv2）：https://github.com/OpenVPN/tap-windows6
- Npcap OEM 授权：https://npcap.com/oem/
- AF_XDP 文档：https://docs.kernel.org/networking/af_xdp.html
- aya（eBPF）：https://github.com/aya-rs/aya

### 技术栈

- wasmtime 50：https://github.com/bytecodealliance/wasmtime/releases/tag/v50.0.0
- WASI：https://wasi.dev/
- Component Model：https://github.com/bytecodealliance/wasm-tools
- rmcp（Rust MCP SDK 3.9）：https://crates.io/crates/rmcp
- MCP 2026-07-28 规范：https://modelcontextprotocol.io/specification/2026-07-28/
- MCP 变更说明：https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/
- React 19：https://react.dev/
- Vite：https://vite.dev/
- Tailwind 4：https://tailwindcss.com/
- shadcn/ui：https://ui.shadcn.com/
- TanStack：https://tanstack.com/
- uPlot：https://github.com/leeoniya/uPlot
- ECharts：https://echarts.apache.org/
- Monaco：https://microsoft.github.io/monaco-editor/
- rust-embed：https://github.com/pyrossh/rust-embed
- axum：https://github.com/tokio-rs/axum
- clap：https://github.com/clap-rs/clap
- Tauri 2：https://v2.tauri.app/
- etherparse：https://github.com/JulianSchmid/etherparse
- pnet：https://github.com/libpnet/pnet

### 许可证与合规工具

- Apache-2.0：https://www.apache.org/licenses/LICENSE-2.0
- MIT：https://opensource.org/license/mit
- GPLv3：https://www.gnu.org/licenses/gpl-3.0.html
- LGPL-2.1：https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html
- CeCILL-C：https://cecill.info/licences/Licence_CeCILL-C_V1-en.html
- cargo-deny：https://github.com/EmbarkStudios/cargo-deny
- reuse（SPDX）：https://github.com/fsfe/reuse-tool
- CycloneDX：https://cyclonedx.org/
- Syft：https://github.com/anchore/syft

> **最后一句给拍板的人**：这份文档里唯一不能拖的是 **D2（许可）** 和 **P0-4（ETG 书面澄清）**。技术可以迭代，许可和合规的坑，越晚越贵。
