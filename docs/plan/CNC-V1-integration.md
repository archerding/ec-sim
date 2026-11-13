# CNC 虚拟闭环：EC-Sim 侧对接包（V1/V2，2026-11-13）

> 面向 **CNC Station 虚拟闭环**（[plan-virtual-loop](D:/tokengence.ai/cnc-station/docs/plan-virtual-loop.md)，跨仓只读引用）：
> LinuxCNC + linuxcnc-ethercat（lcec）+ IgH 主站 + TAP + EC-Sim CiA 402 从站。
> 本文件是 **EC-Sim 侧**的布局核对、启动 runbook 与证据；lcec/LinuxCNC 侧配置由 `cnc-impl` 负责。
>
> 边界：只做**帧级/PDO 数据通路**的预回归。不宣称 EtherCAT 一致性、CTT、LinuxCNC 或任何厂商认证；
> 不含物理急停/安全门/真实伺服；未验证项标 ⚠️。

## 1. 结论（与 lcec `basic_cia402` 的布局核对）

| 场景 | lcec 期望（`cnc-impl` 对 lcec commit `c6f8820` 源码核对） | EC-Sim 画像 | 结论 |
| --- | --- | --- | --- |
| **V1（PP，单从站）** | Rx 0x1600 = 0x6040 U16 + 0x6060 S8 + 0x607A S32（7B）；Tx 0x1A00 = 0x6041 U16 + 0x6061 S8 + 0x6064 S32（7B）；`0x6502` 含 PP(bit0)；无 `<dcConf>` | `plugins/generic-cia402-servo/plugin.yaml`（7B/7B，`0x6502=0x01C1`） | ✅ **一致，直接用** |
| **V2/V3（CSP，多轴）** | **`enableCSP=1` 隐式要求 TxPDO 含 `0x606C:00` S32（实际速度）** → Tx 11B（6041+6061+6064+606C）；Rx 仍 7B；`0x6502` 保持 `0x01C1` | **新增 `plugins/cnc-csp-servo/plugin.yaml`**（Rx 7B / Tx 11B，`0x6502=0x01C1`） | ✅ 已落地，见 `crates/plugin-host/tests/plugin_manifests.rs` 守卫 |

lcec 侧要点（事实记录，不构成认证）：

- `0x6040` / `0x6041` 必须分别是 RxPDO / TxPDO 的**第一项**；
- 条目必须**真实出现在** `0x1600`/`0x1A00` 映射对象里（lcec 按从站映射对象读回匹配，不只看 SM 长度）；
- 字节序：每个对象小端（U16/S8/S32 按映射顺序紧排，无填充）；
- V1/V2 不配置 DC（lcec XML 无 `<dcConf>`；`appTimePeriod=1000000`）。

## 2. PDO 字节布局（可直接照抄进配置核对）

### 2.1 V1：`generic-cia402-servo`（SM2 7B / SM3 7B）

| 偏移 | RxPDO（主站→从站，SM2 @ 0x1100） | 类型 | SM3 @ 0x1110（从站→主站 TxPDO） | 类型 |
| --- | --- | --- | --- | --- |
| 0..2 | `0x6040` Controlword | U16 LE | `0x6041` Statusword | U16 LE |
| 2..3 | `0x6060` Modes of operation | S8 | `0x6061` Modes of operation display | S8 |
| 3..7 | `0x607A` Target position | S32 LE | `0x6064` Position actual value | S32 LE |

### 2.2 V2/V3：`cnc-csp-servo`（SM2 7B / SM3 11B）

| 偏移 | RxPDO（SM2 @ 0x1100） | 类型 | SM3 @ 0x1110（TxPDO） | 类型 |
| --- | --- | --- | --- | --- |
| 0..2 | `0x6040` Controlword | U16 LE | `0x6041` Statusword | U16 LE |
| 2..3 | `0x6060` Modes of operation | S8 | `0x6061` Modes of operation display | S8 |
| 3..7 | `0x607A` Target position | S32 LE | `0x6064` Position actual value | S32 LE |
| — | — | — | `0x606C` Velocity actual value（lcec CSP 隐式要求） | S32 LE（7..11） |

`0x6502`（Supported drive modes）= `0x01C1`：PP(bit0)/HM(bit5)/IP(bit6)/CSP(bit7)。

## 3. EC-Sim 侧启动 runbook（在 CNC 验证机上跑，TAP 需同内核）

> ⚠️ TAP 只能同内核互通，所以 **`ec-sim-esc` 必须跑在 CNC 验证机上**（不是 EC-Sim 自己的 VM）。
> EC-Sim 侧不提供/不接触 CNC 的 VM；二进制与本文档走交付通道（见 §5）。

```bash
# 0. 前置：二进制 ec-sim-esc（见 §5）+ 画像目录（plugins/generic-cia402-servo 或 plugins/cnc-csp-servo）
sha256sum ec-sim-esc

# 1. 建 TAP（root）
ip tuntap add dev tapcnc0 mode tap
ip link set tapcnc0 address 02:ec:51:4d:00:01
ip link set tapcnc0 up

# 2. 起 EC-Sim 从站（V1：单从站 + PP 行为；非实时进程）
./ec-sim-esc --tap tapcnc0 --slaves 1 \
  --manifest plugins/generic-cia402-servo/plugin.yaml \
  --behavior cia402 \
  --serve 127.0.0.1:18090 &
# 预期 stdout（按顺序）：
#   READY tap=tapcnc0 mac=02:ec:51:4d:00:01
#   MANIFEST id=com.ecsim.profile.cia402-servo name=Generic CiA 402 position/velocity servo (SIMULATED) vendor=0x00000EC5 product=0x0000E5E0
#   （--verbose 时还有 STATS in=... al=0x08 行）
# READY 之后 stdout 不再刷新；日志里出现 STATS 需加 --verbose。

# 3. 再加载 IgH 主站（顺序见下；具体路径以 CNC 侧安装为准）
modprobe ec_master main_devices=02:ec:51:4d:00:01
modprobe ec_generic

# 4. 主站侧核对
/opt/.../bin/ethercat slaves        # 期望：0  0:0  OP  +  0x00000ec5:0x0000e5e0
ethercat master                     # 期望：Slaves: 1、状态 OP（随 ecrt 应用/或 slaves 工具）

# 5. EC-Sim 只读控制面（可 curl，用于 PDO 双向可见性证据）
curl -s http://127.0.0.1:18090/topology
curl -s http://127.0.0.1:18090/pdo?slave=0      # pdo-v1 + decoded（0x6040/0x6041/0x6064/…）
```

要点：

- **`--esm-lax` 不需要**：manifest 画像的 SM/PDO 布局自洽，ESM 前置条件默认开启即可；
  若真机报 AL Status Code `0x0015/0x0017/0x0018/0x0019/0x0024/0x0025/0x002B`，先把
  `manifest` 的 SM/PDO 与 lcec XML 对照（本文件 §2），而不是直接加 `--esm-lax`。
- `--behavior cia402` 是**内置 Tier0 状态机**（控制字→状态字、模式行为），不依赖 WASM feature；
  交付二进制用默认 feature 构建即可（无第三方依赖）。
- `--serve` 是只读 HTTP 观测面（默认无鉴权、仅回环地址）；写路径需显式 `--serve-write`，虚拟闭环不需要。
- 从站 SM2/SM3 物理地址：SM2=0x1100、SM3=0x1110（manifest 声明，主站配置 FMMU 后以逻辑地址访问）。

### 3.1 V1 验收最小断言（由 CNC 侧脚本做，EC-Sim 侧提供数据面）

1. `ethercat slaves` 出现 `0x00000ec5:0x0000e5e0` 且状态 `OP`；
2. lcec `lcec.0.*` HAL pin 出现（参考 `halcmd show pin | grep lcec`）；
3. `curl /pdo?slave=0` 的 `sm2/sm3` 字节随 LinuxCNC 侧写 PDO 变化（或至少 decoded 值合理）；
4. 无 AL Status Code 错误（`/state.live.al_status` 低 4 位 = 0x08，`al_status_code=0x0000`）。

### 3.2 多从站（V2 准备）

- `--slaves N` 时 N 个从站**克隆同一画像**（B12 语义）：现在支持每站一个 **WASM 行为实例**（B51 后续），
  但**混合画像**（3 伺服 + IO 不同 manifest）尚不支持 → ⚠️ V2 若需要混合拓扑，需先做"每从站画像列表"
  （已登记为后续项）。
- EC-Sim 侧多从站 TAP 预回归：`scripts/spikes/tap-cia402-multi-slave-probe.sh`（SOEM，默认 N=2，
  断言 `N slaves`、WKC `3N`、`mapped 7N O + 11N I`、`/topology` 全 OP、`/pdo` 具名解码）。

## 4. EC-Sim 侧证据（自有验证机，与 CNC VM 分离）

| 证据 | 命令 | 结果 |
| --- | --- | --- |
| 单从站 PP 画像 + IgH ecrt 进 OP | `sudo ./scripts/spikes/igh-cia402-op-probe.sh`（MANIFEST=generic-cia402-servo） | **EXIT=0**：`ethercat slaves` = `OP + 0x00000ec5:0x0000e5e0`、`RESULT al_states=0x08 slaves_responding=1`、`STATUS sw=0x0637 modes_display=8 pos=1000` |
| 新 CSP 画像（11B Tx）IgH 进 OP | `sudo MANIFEST=plugins/cnc-csp-servo/plugin.yaml EXPECT_PID=0000e5e3 ./scripts/spikes/igh-cia402-op-probe.sh` | ✅ 识别身份并到 OP；❌ 该 ecrt 夹具的 **SDO 下载段**在验证机**内核 D 状态卡死**（`/proc/<pid>/stack`: `ec_ioctl_slave_sdo_download`；PV-01/PV-14 类环境问题），`kill -9` 无效、需 reboot。完整 RESULT 未取到 → 不作为证据 |
| TAP + 2 个 CiA402 从站 OP | `sudo ./scripts/spikes/tap-cia402-multi-slave-probe.sh`（N=2，cnc-csp-servo） | **EXIT=0**：slaveinfo 2 站/workcounter 6/每站 56bits+88bits、`simple_ng mapped 14O+22I bytes` + 全站 operational、`/topology` 2/2 站 OP、`/pdo?slave=0` decoded 含 0x6040/0x6041/0x6060/0x6061/0x6064/0x606C/0x607A |
| 单测守卫 | `cargo test -p plugin-host --test plugin_manifests` | `cnc_csp_servo_layout_matches_lcec_csp_expectations` PASS（工作区 259 passed） |

> ⚠️ EC-Sim VM 的 IgH 是自建/验证机本地 ecrt 夹具（源码不入库）；CNC VM 的 IgH/lcec 由其 RPM 安装，
> 两者独立验证，互不冒用日志。ecrt 夹具的 SDO 段阻塞在 EC-Sim VM 上**已复现多次**（PV-01/PV-14；
> `generic-cia402-servo` 与 `cnc-csp-servo` 都能复现），V1（lcec 零 SDO）不依赖该路径。

### 4.1 CNC 侧 V1 联合验收（引用，非 EC-Sim 侧结论）

CNC 侧 2026-11-13 用本对接包跑通 **V1 单从站 OP**（`cnc-impl` 在其验证机，证据在其仓库
`docs/verification/2026-11-13-b6-v1-igh-tap-infra.md`；本仓库只引用事实、不复制其日志）：

- `sha256` 校验本交付二进制一致；`READY tap=tapcnc0 mac=02:ec:51:4d:00:01` + `BEHAVIOR cia402 ...` 与
  §3 runbook 一致；
- 从站 `0:0 OP 0x00000ec5:0x0000e5e3`、master `Phase: Operation/Active: yes`、lcec HAL pin 正常、
  `statusword=0x0240`、`lcec.0.wkc=3`；
- 发现并解决：lcec 默认 `enable_pv=1` 会写 11B Rx 并对 `0x60FF` 做 init SDO（本镜像 OD 缺该对象时
  报非致命 `error -5`）→ CNC 侧 XML 显式 `enablePV=false` 后恢复 7B/0x60FF 不再报错；
  本仓库已在 `cnc-csp-servo` 的 OD 补 `0x60FF`（不进默认 PDO）作为兼容改进。
- **边界**：V1 只验"单从站 OP + PDO/HAL 可见"，不宣称一致性/认证；物理安全链不在范围。

## 5. 二进制交付（CNC 验证机无 cargo/rustc）

- 构建环境：**openEuler 24.03 LTS-SP4 / x86_64**（与 CNC 验证机同 glibc），
  `cargo build --release -p esc-node`（默认 feature，零第三方依赖）。
- 产物：`ec-sim-esc`（release）+ `sha256`；放在 EC-Sim 仓库 `.local/artifacts/`（gitignored，
  不进仓库、不上传公开 release 之前先确认许可/来源）。
- 交付字段：文件名、字节数、sha256、构建命令、`git rev-parse HEAD`、`rustc -V`。
- CNC 侧落盘后先 `sha256sum -c` 再运行；**不要在 CNC VM 上跑 cargo**。

### 5.1 本班实际交付（2026-11-13）

| 字段 | 值 |
| --- | --- |
| 文件 | `.local/artifacts/ec-sim-esc-linux-x86_64`（+ `BUILD-INFO.txt`，均在 gitignored 目录） |
| 字节数 | 1,053,952 |
| sha256 | `878712fe8e8034d79a55e6a66937be14e4de02baba195ea4c40499348879b8ee` |
| 构建环境 | openEuler 24.03 LTS-SP4 / x86_64 / glibc 2.38 / rustc 1.99.1 / cargo 1.99.1 |
| 构建命令 | `cargo build --release -p esc-node`（默认 feature，零第三方依赖） |
| 源码 | EC-Sim 仓库 B54 提交（2026-11-13 团队班；`WORKLOG.md` 同条目） |

> CNC 侧由 `cnc-impl` 负责把二进制从共享盘取到 CNC VM（EC-Sim 侧**不登录/不碰 CNC VM**），
> 落盘后 `sha256sum` 校验 + `chmod +x` + `--version` 自检再跑 V1。

## 6. 排查表（EC-Sim 侧常见失败）

| 失败现象 | 可能原因 | 处理 |
| --- | --- | --- |
| IgH `ethercat slaves` 只看到 PREOP、无 OP | 无 ecrt 应用（IgH 工具行为）；TAP 未 up/无流量 | 先用 SOEM `simple_ng` 或 ecrt 应用驱动；确认 `ip link set tapcnc0 up` |
| lcec 把 RxPDO 配成 11B（多 `0x60FF`）且 init SDO 报 `error -5`（非致命） | lcec `basic_cia402` 默认 **`enable_pv=1`**（即使 XML 只写 `enableCSP` 也会写 `0x60FF` 并做一次 CoE SDO upload） | lcec XML 显式写 **`enablePV=false`**（cnc-impl 2026-11-13 在其验证机实测：加上后 Rx 回到 7B、SDO error=0）；本画像已在 OD 补 `0x60FF`（不进默认 PDO）使该 SDO 也可读，但布局仍按 CSP 7B/11B |
| AL Status Code `0x0017`（PREOP→SAFEOP） | SM2/SM3 未配置或长度不符 | 对照 §2：SM2 7B、SM3 7B/11B；检查 lcec XML 的 SM 长度 |
| AL Status Code `0x0024/0x0025`（映射无效） | 0x1600/0x1A00 映射位长与 SM 长度不一致 | 用 lcec 的 XML PDO 条目核对 §2（条目必须在映射对象里） |
| 身份不符 | 拿错 manifest 或 PID 过期 | `0x00000e5e0`（V1 PP 画像）/`0x0000e5e3`（CSP 画像） |
| 0x606C 相关 mismatch | 用 7B Tx 画像跑 CSP | V2 起改用 `plugins/cnc-csp-servo`（Tx 11B） |
| `/pdo` 无 decoded | 静态模式（无 `--tap/--uds`） | 实时模式才返回 live/decoded |

## 7. 边界（必须随验收报告复述）

- **不是一致性测试/CTT/认证**：只用例映射与预回归。
- **软件闭环 ≠ 安全认证**：无物理急停/安全门/真实伺服；安全链不在本验收范围。
- Tier0 行为（PP/CSP）是简化语义：无轨迹规划、加减速、编码器分辨率/单位换算。
- DC 未实现真实同步（仅 Tier0 帧驱动时钟）；V1/V2 不配置 DC。

## 8. 下一步（EC-Sim 侧）

1. ✅ 新增 `cnc-csp-servo` 画像（V2 CSP 布局）；
2. ✅ 多实例行为插件 + 共享配额接线（`--slaves N` 每站一个实例）；
3. ⏳ 混合画像（每从站不同 manifest）→ V2 混合拓扑需要；
4. ⏳ 交付 release 二进制 + sha256 并跑通 EC-Sim VM 侧 IgH/TAP 预回归（本班）；
5. LinuxCNC/lcec 侧配置与真实验收由 `cnc-impl` 在其 VM 上写 `scripts/vm-verify/vm-b6-v1.sh`。
