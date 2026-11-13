# EC-Sim 二进制交付记录：`ec-sim-esc` release → CNC 验证机（2026-11-13）

> 任务板 `task-26`（Owner `sim-impl`）。本文件是**交付记录**，不是认证/兼容性结论。
> 主机地址、账号、代理一律不写（见 EC-Sim AGENTS §8）；EC-Sim 侧**不登录 CNC 验证机**，
> CNC 侧落盘由 `cnc-impl`（CNC 验证机 owner）执行，双方各自出证据。

## 1. 交付物

| 字段 | 值 |
| --- | --- |
| 文件名 | `ec-sim-esc-linux-x86_64` |
| 字节数 | 1,053,952 |
| sha256 | `878712fe8e8034d79a55e6a66937be14e4de02baba195ea4c40499348879b8ee` |
| ELF 头 | `7f 45 4c 46 02 01 01 ...`（ELF64、LSB、`e_machine=0x3e` = x86-64） |
| 源码版本 | EC-Sim `main` @ `182147d`（B54 CNC 虚拟闭环对接包，2026-11-13 团队班） |
| 构建环境 | openEuler 24.03 LTS-SP4 / x86_64 / glibc 2.38 / rustc 1.99.1 / cargo 1.99.1 |
| 构建命令 | `cargo build --release -p esc-node --bin ec-sim-esc`（默认 feature，零第三方依赖） |
| 构建时间 | 2026-11-13 12:41（EC-Sim 自有验证机 UTC+8） |
| 存放（EC-Sim 侧） | `D:\tokengence.ai\ec-sim\.local\artifacts\ec-sim-esc-linux-x86_64` + `BUILD-INFO.txt`（gitignored，**不入库**） |
| 目标落盘（CNC 侧） | CNC 验证机 `/opt/cnc/bin/ec-sim-esc`（目录由 `cnc-impl` 创建/校验） |

## 2. 交付流程（分工）

1. **EC-Sim 侧（本文件）**：在 EC-Sim 自己的验证机构建 release、记录 sha256/ELF/环境、放到 Windows 主机共享路径
   `.local/artifacts/`（gitignored）。`--version`/`--help` 的实测见 §3。
2. **CNC 侧（`cnc-impl`）**：从共享路径取二进制 → scp 到 CNC 验证机 `/opt/cnc/bin/`（或 `/opt/cnc/b6/`）
   → `sha256sum` 比对 → `chmod +x` → `./ec-sim-esc --version` / `--help` → 按
   [CNC-V1-integration](D:/tokengence.ai/ec-sim/docs/plan/CNC-V1-integration.md) 的 runbook 跑 V1。
   CNC 侧落盘/校验证据由 `cnc-impl` 写入 CNC 仓库 `docs/verification/`（本文件不冒用）。
3. 二进制与画像版本对应：本二进制对应 EC-Sim `182147d`；V1/V2 使用该提交里的
   `plugins/cnc-csp-servo/plugin.yaml`（PID `0x0000E5E3`、SM2 7B / SM3 11B、`0x6502=0x01C1`、无 DC）。
   画像与二进制**无需同目录**：`--manifest` 传 plugin.yaml 的路径即可。

## 3. 校验记录（EC-Sim 侧可复核项）

| 检查 | 结果 |
| --- | --- |
| `sha256sum`（EC-Sim 验证机构建产物 vs Windows 共享路径副本） | 均 `878712fe8e8034d79a55e6a66937be14e4de02baba195ea4c40499348879b8ee`（一致） |
| 文件大小 | 1,053,952 B |
| ELF 头 | ELF64 / LSB / x86-64（`e_machine=0x3e`），可作为 x86_64 Linux 可执行文件 |
| 构建 | 同一提交在 EC-Sim 验证机上 `cargo build --release -p esc-node` 成功（默认 feature，无第三方依赖） |
| 运行期行为（同源二进制相关证据） | 该提交的 release 二进制在 EC-Sim 验证机上跑 `igh-cia402-op-probe.sh`（generic-cia402-servo，V1 PP）**EXIT=0**：`ethercat slaves` OP、`al_states=0x08`、`sw=0x0637 modes_display=8 pos=1000`；`tap-cia402-multi-slave-probe.sh`（N=2）**EXIT=0** |
| `--version` / `--help` 直接执行 | **PASS**（2026-11-13 团队班，EC-Sim 验证机）：`./ec-sim-esc --version` → `ec-sim-esc 0.0.1`；`./ec-sim-esc --help` 打印用法（`--tap/--uds`、`--manifest`、`--slaves`、`--behavior cia402`、`--behavior-max-instances`、`--serve` 等） |

预期输出（`--version`）：`ec-sim-esc 0.0.1`；`--help` 打印用法（TAP/UDS、`--manifest`、`--slaves`、
`--behavior cia402`、`--serve` 等）。

## 4. 卡点/环境（⚠️ 诚实记录）

- 二进制**可执行性已在 EC-Sim 验证机实测**（§3 的 `--version`/`--help` PASS）；CNC 验证机上的落盘/校验
  由 `cnc-impl` 执行（CNC 侧证据由其记录），本文件不冒用。
- 交付物为**默认 feature** 构建：WASM 行为插件（`wasm-runtime`）不在其中；V1/V2 的 CiA402 行为是
  内置 `--behavior cia402`（Tier0 状态机），不依赖 WASM，因此该二进制满足 V1/V2 需要。
- 环境提示：EC-Sim 验证机上 IgH ecrt 夹具在 `ecrt_master_sdo_download` 会卡进内核 D 状态
  （PV-01/PV-14 类，**generic 与 csp 画像均可复现**，`timeout`/`kill -9` 无效、需 reboot）；
  该问题属 ecrt 夹具/环境，与交付二进制无关（V1 lcec 零 SDO 路径不经过它）。

## 5. 边界

- 不宣称 EtherCAT 一致性 / CTT / LinuxCNC 认证；这是**虚拟闭环的帧级/PDO 数据通路**预回归交付物。
- 无鉴权/无 TLS：`--serve` 是只读观测面，仅回环地址；写路径需显式 `--serve-write`，V1 不使用。
- 不含任何厂商 ESI/PDF/固件；插件身份为自有 VID/PID + SIMULATED。
