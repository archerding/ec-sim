# EC-Sim 当前状态快照（2026-11-13 08:00）

> 本页是**项目状态索引**，不是规范或认证结论。所有"已验证"都指仓库内脚本/单测的
> 可复现结果，**不构成**一致性测试或认证（见 `AGENTS.md` §3/§4）。

## 一句话

核心 ESC（寄存器/SM/FMMU/AL/EEPROM-SII/邮箱/watchdog）+ CoE（SDO 加速/分段/SDO Info/
Complete Access）+ CiA 402（常用模式全覆盖：PP/VL/PV/TQ/HM/IP/CSP/CSV/CST，Tier0 简化）
+ 声明式设备画像已实现；**未修改的 SOEM 2.0.0 与 IgH 1.6.13** 可发现从站并进入 OP
（IgH 无应用时到 PREOP）。测试基线 **230 passed**（另 `wasm-runtime` feature 下 7 个 WASM 单测）。

## 代码与测试

| crate | 职责 | 测试数 |
| --- | --- | --- |
| `esc-core` | 寄存器、SM/FMMU、AL 状态机、EEPROM-SII 生成、邮箱解析、watchdog | 86 |
| `esc-node` | 从站应答器、PDI 内存、邮箱处理、CiA 402 行为、`ec-sim-esc` 二进制 | 71 |
| `plugin-host` | plugin.yaml 解析/校验、SII/字典/ESI 生成；WASM 宿主（可选 feature） | 27 + 3 集成（另 `wasm-runtime` 下 +7） |
| `wire` | EtherCAT 帧/datagram 编解码 + WKC 语义 | 23 |
| `control-plane` | 只读快照 API：state-v1 / dict-v1 JSON + 路由表（`/state`、`/dict`） | 4 |

`cargo fmt --check`、`cargo clippy --workspace --all-targets -D warnings`、`cargo test --workspace`
均为 0 失败；`cargo doc --workspace --no-deps` 0 警告。

## 能力清单（已实现）

- **ESC 核心**：寄存器阵列、SM 配置/状态、FMMU 逻辑寻址（LRW WKC 写+2/读+1）、
  AL 状态机（INIT↔PREOP↔SAFEOP↔OP，BOOT 拒绝、错误 ACK）、EEPROM 接口状态机、
  SII 生成（CRC-8、字符串/通用/SM/**FMMU**/TxPDO/RxPDO/**DC** 类别链、0xFFFF 终止）、
  SM watchdog（0x001B）、ESM 前置条件错误码（0x0015/0x0017/0x0018/0x0019/0x0024/
  0x0025/0x002B）、0x0410 读写语义（无 PDI 超时）。
- **线缆层**：Linux TAP（旗舰，AF_PACKET 双向）与 ESC-over-UDS shim（全平台/CI）；`--slaves N` 支持 **多从站链**（UDS shim 与 TAP 均验证 8 从站：未修改 SOEM `slaveinfo` 报 `8 slaves found`、`simple_ng` 映射 8O+8I 且 WKC 24 稳定；TAP 实测循环 ≈425 Hz，未隔离 CPU）。
- **CoE**：expedited + segmented SDO 上传/下载、SDO Info、Complete Access（上传/单帧+
  分段下载/重发幂等/超长 Abort 0x06070010）、Abort 码、PDO assign/map 对象
  （0x1C12/0x1C13/0x1600/0x1A00）、CA 下载 all-or-nothing。
- **DC Tier0**：ESC Support 声明 DC（0x0008 bit2）、0x0900–0x0917 帧驱动模拟时钟、
  SV660N 画像声明 SII DC 类别（assignActivate 0x300）。
- **CiA 402 Tier0**：常用模式全覆盖（PP/VL/PV/TQ/HM/IP/CSP/CSV/CST，均简化）、
  `--fault` 故障注入与复位；`0x6502` 按各画像布局声明（7B=0x01C1、11B=0x03CD、
  5B=0x0410、SV660N=0x03AD 公开实测）。详见 [cia402-modes-matrix.md](plugins/cia402-modes-matrix.md)。
- **设备画像**：`generic-io`、`beckhoff-el2008`、`generic-cia402-servo`、
  `generic-cia402-csv-servo`、`generic-cia402-cst-servo`（均自有 VID/PID + SIMULATED），
  `inovance-sv660n` 兼容画像（默认可变 PDO 已按公开手册核对：RPDO 8B / TPDO 22B + DC）。
- **插件宿主（M3-W1 spike）**：`plugin-host` 的 `wasm-runtime` feature（**默认关闭**，默认构建仍零第三方
  依赖）提供 wasmi 0.31 最小宿主——`env.read_pdo`/`env.write_pdo`/`env.log` + `tick` 导出、PDO arena
  快照、日志上限 64KiB；WAT 夹具 + `wasm-plugin-probe.sh`；release 实测实例化 605µs / tick 179.5ns。
  插件**不进入实时环**；arena 同步/fuel 超时/崩溃隔离属 W2。见 [spike-6](spikes/spike-6-wasm-host.md)。

## 控制面与 WebUI

- 控制面（`--serve`）：只读 API（`/state`、`/dict`、`/pdo`、`/topology`、`/metrics`，支持 `?slave=N`）+ M3 写路径（`--serve-write` 下的 `POST /sdo`/`POST /pdo`、SDO 只读浏览、`/sdo-audit`）+ 可选静态 Bearer token（`--serve-token-env`，默认关闭）；零依赖 Web 页含 **SDO 面板**（`GET /sdo` 浏览 + 勾选/`confirm` 二次确认的受控写；⚠️ 无 DOM 自动化，静态回归 + 人工验收）：见 **[CONTROL-PLANE.md](CONTROL-PLANE.md)**、[web/README.md](../web/README.md)；M2 退出条件核对见 **[M2 退出清单](plan/M2-exit-checklist.md)**；M3 范围见 **[M3 范围](plan/M3-scope.md)** 与 **[ADR 0002](adr/0002-m3-plugin-host-and-write-paths.md)**。

## 验证证据（详见 [docs/VERIFICATION.md](VERIFICATION.md)）

- **SOEM**：`slaveinfo` 扫描身份/PDO 位宽、`simple_ng` 进 OP（WKC 3 稳定）、
  `-map` 读到 PDO 映射、SDO Info / 分段上传 / ESI→SII / UDS shim。
- **IgH 1.6.13**：`ethercat slaves` 识别身份；ecrt 应用（验证机本地夹具，不入库）
  把 8DO / CiA402 7B / CSV 11B / CST 5B 画像推入 OP，并验证状态字/模式/位置/速度/转矩。
- **合成 UDS 主站**：datagram/WKC、SM watchdog、ESM 前置条件、SDO（expedited/segmented/
  CA/Info）、CiA402 行为/CSV/CST/故障、分段下载超长 Abort。

## 已知差距（诚实清单）

| 差距 | 影响 | 现状 |
| --- | --- | --- |
| **DC 无真实同步** | SV660N 画像已声明 DC（`Has DC: 1`）；SOEM `ecx_configdc()` 与 IgH `ecrt_slave_config_dc()` 路径均已验证（`dc_rd/dc_wr>0`、仍进 OP），但只有 Tier0 帧驱动模拟时钟；无 SYNC0 脉冲/漂移补偿/传播延迟测量 | DC 寄存器+模拟时钟已实现；真实同步待硬件级仿真 |
| SV660N 固定映射位长待逐项核对 | 默认可变映射（RPDO 8B / TPDO 22B）与**全部 9 个固定映射**（0x1701..0x1705 / 0x1B01..0x1B04）均已生成映射对象；每组的条目数/位长总和与手册字节数一致（守卫测试覆盖） | 位长按 CiA 402 标准对象推算；厂商扩展对象/故障码表未收录 |
| CA 部分写/子索引顺序语义 | 主站异常 CA 序列的细节行为未覆盖 | 已做 all-or-nothing；顺序语义未实现 |
| 无独立硬件定时器 | watchdog/DC 由帧驱动，停帧不主动超时 | 设计选择，已文档化 |
| IgH SDO 上传/下载在验证机阻塞 | IgH 侧 SDO 行为只能用 SOEM/合成主站验证 | 环境问题（ecrt ioctl），⚠️ 待验证 |
| SOEM `slaveinfo -sdo` 偶发段错误 | 探针需重试规避 | 已知 SOEM 2.0.0 数组读缺陷 |
| fuzz 仅确定性 LCG | 无覆盖率引导模糊测试 | `wire`/`esc-core` 解析器与 `esc-node::handle_frame` 均有无 panic 随机测试（确定性 LCG） |

## 快速开始（本机，无 root）

```bash
cargo build --release -p esc-node
cargo run -q -p esc-node --bin ec-sim-esc -- --version          # 版本
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-sii --manifest plugins/generic-cia402-cst-servo/plugin.yaml
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-dict --manifest plugins/generic-cia402-cst-servo/plugin.yaml
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-state --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/state.json
cargo run -q -p esc-node --bin ec-sim-esc -- --dump-dict --json --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/dict.json
python3 -m http.server 8080 --directory web   # 只读状态页（M2 起点）
cargo run -q -p esc-node --bin ec-sim-esc -- --serve 127.0.0.1:8090 --manifest plugins/generic-cia402-cst-servo/plugin.yaml   # 只读 HTTP API：/state,/dict
cargo run -q -p esc-node --bin ec-sim-esc -- --uds /tmp/ecsim.sock --serve 127.0.0.1:8091   # 实时模式：/state 含 live（AL/统计）、/pdo 过程数据、/pdo?history=1 时间序列（Web 波形）、/pdo 含 decoded 具名值、/topology 多从站摘要、/state?slave=N 与 /pdo?slave=N 逐站查看、/metrics Prometheus 文本指标（支持 ?slave=N 筛选；Web 指标卡片）、/events SSE 推送、`GET /sdo` 只读浏览、`--serve-write` 显式开启的 `POST /pdo`/`POST /sdo` 受控写（默认关闭）
EC_SIM_TOKEN='<token>' cargo run -q -p esc-node --bin ec-sim-esc -- --serve 127.0.0.1:8091 --serve-token-env EC_SIM_TOKEN   # 可选静态 token：API 需 Authorization: Bearer（空/未设置 = 现状）
cargo run -q -p esc-node --bin ec-sim-esc -- --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8091 --web-root web   # 同源托管 WebUI + API
bash scripts/spikes/uds-loopback.sh        # UDS 合成主站基础闭环
bash scripts/spikes/cia402-cst-probe.sh    # CST Tier0 行为
cargo test --workspace
```

真实主站验证（Linux 验证机，root）：见 `docs/VERIFICATION.md` 的 TAP/root 表。

## 下一步（按优先级）

1. B44（M3-W2）：PDO arena 同步（双缓冲/seqlock + generation）+ WASM 插件独占 worker 线程 + fuel/超时/内存上限。
2. 验证机补跑 `wasm-plugin-probe.sh`（需 crates.io；同步依赖后执行）。
3. B11：固定映射（0x1701..0x1705/0x1B01..0x1B04）生成与位长逐项核对；厂商扩展对象核对。
4. DC 仿真（寄存器 + SYNC0/tick），实现后在测试画像上验证再声明 SII DC。
5. CA 部分写/子索引顺序语义完善；fuzz 升级。
