# EC-Sim 当前状态快照（2026-11-13 02:27）

> 本页是**项目状态索引**，不是规范或认证结论。所有"已验证"都指仓库内脚本/单测的
> 可复现结果，**不构成**一致性测试或认证（见 `AGENTS.md` §3/§4）。

## 一句话

核心 ESC（寄存器/SM/FMMU/AL/EEPROM-SII/邮箱/watchdog）+ CoE（SDO 加速/分段/SDO Info/
Complete Access）+ CiA 402（CSP/CSV/CST/故障注入）+ 声明式设备画像已实现；**未修改的
SOEM 2.0.0 与 IgH 1.6.13** 可发现从站并进入 OP（IgH 无应用时到 PREOP）。测试基线
**199 passed**。

## 代码与测试

| crate | 职责 | 测试数 |
| --- | --- | --- |
| `esc-core` | 寄存器、SM/FMMU、AL 状态机、EEPROM-SII 生成、邮箱解析、watchdog | 85 |
| `esc-node` | 从站应答器、PDI 内存、邮箱处理、CiA 402 行为、`ec-sim-esc` 二进制 | 63 |
| `plugin-host` | plugin.yaml 解析/校验、SII/字典/ESI 生成 | 27 + 1 集成 |
| `wire` | EtherCAT 帧/datagram 编解码 + WKC 语义 | 23 |

`cargo fmt --check`、`cargo clippy --workspace --all-targets -D warnings`、`cargo test --workspace`
均为 0 失败；`cargo doc --workspace --no-deps` 0 警告。

## 能力清单（已实现）

- **ESC 核心**：寄存器阵列、SM 配置/状态、FMMU 逻辑寻址（LRW WKC 写+2/读+1）、
  AL 状态机（INIT↔PREOP↔SAFEOP↔OP，BOOT 拒绝、错误 ACK）、EEPROM 接口状态机、
  SII 生成（CRC-8、字符串/通用/SM/**FMMU**/TxPDO/RxPDO/**DC** 类别链、0xFFFF 终止）、
  SM watchdog（0x001B）、ESM 前置条件错误码（0x0015/0x0017/0x0018/0x0019/0x0024/
  0x0025/0x002B）、0x0410 读写语义（无 PDI 超时）。
- **线缆层**：Linux TAP（旗舰，AF_PACKET 双向）与 ESC-over-UDS shim（全平台/CI）。
- **CoE**：expedited + segmented SDO 上传/下载、SDO Info、Complete Access（上传/单帧+
  分段下载/重发幂等/超长 Abort 0x06070010）、Abort 码、PDO assign/map 对象
  （0x1C12/0x1C13/0x1600/0x1A00）、CA 下载 all-or-nothing。
- **CiA 402 Tier0**：CSP（位置跟随 + Target reached）、CSV（速度累加）、**CST**（实际
  转矩镜像目标转矩）、`--fault` 故障注入与复位、`0x6502=0x3AD`。
- **设备画像**：`generic-io`、`beckhoff-el2008`、`generic-cia402-servo`、
  `generic-cia402-csv-servo`、`generic-cia402-cst-servo`（均自有 VID/PID + SIMULATED），
  `inovance-sv660n` 兼容画像（researching）。

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
| **DC 同步未实现** | 无任何画像声明 DC（SV660N 真实设备默认开 DC，画像 `Has DC: 0`） | SII DC 类别生成 + manifest `dc` 字段已就绪，等 DC 仿真后再声明 |
| SV660N 默认 PDO 具体条目未知 | 画像用 Tier0 7 字节布局，非厂商默认映射 | 公开文本无来源；facts.md 已标 ⚠️ |
| CA 部分写/子索引顺序语义 | 主站异常 CA 序列的细节行为未覆盖 | 已做 all-or-nothing；顺序语义未实现 |
| 无独立硬件定时器 | watchdog/DC 由帧驱动，停帧不主动超时 | 设计选择，已文档化 |
| IgH SDO 上传/下载在验证机阻塞 | IgH 侧 SDO 行为只能用 SOEM/合成主站验证 | 环境问题（ecrt ioctl），⚠️ 待验证 |
| SOEM `slaveinfo -sdo` 偶发段错误 | 探针需重试规避 | 已知 SOEM 2.0.0 数组读缺陷 |
| fuzz 仅确定性 LCG | 无覆盖率引导模糊测试 | 解析器均有 `parser_never_panics` 测试 |

## 快速开始（本机，无 root）

```bash
cargo build --release -p esc-node
bash scripts/spikes/uds-loopback.sh        # UDS 合成主站基础闭环
bash scripts/spikes/cia402-cst-probe.sh    # CST Tier0 行为
cargo test --workspace
```

真实主站验证（Linux 验证机，root）：见 `docs/VERIFICATION.md` 的 TAP/root 表。

## 下一步（按优先级）

1. B11：继续找 SV660N 默认 PDO 条目；厂商扩展对象核对。
2. DC 仿真（寄存器 + SYNC0/tick），实现后在测试画像上验证再声明 SII DC。
3. CA 部分写/子索引顺序语义完善；fuzz 升级。
