# 验证脚本索引（可复现证据）

本页汇总 `scripts/spikes/` 下的验证脚本：**每个脚本验证什么、需要什么环境、最近一次结果**。
所有结论都是"用例映射 / 预回归"证据，**不构成**一致性测试或认证结论（见 `AGENTS.md` §4）。

> 环境基线：Linux 验证机（见 `docs/LINUX-SERVER.md`）上的**未修改** SOEM 2.0.0 与 IgH 1.6.13；
> IgH 的 ecrt 应用夹具只在验证机本地构建、**不入库**（GPL 边界，见 `AGENTS.md` §3）。
> 最近的完整验证状态：**2026-11-13 02:27**（详见 `WORKLOG.md` 同日各条目与 [STATUS.md](STATUS.md)）。

## 无特权（UDS 合成主站，任意平台可跑）

| 脚本 | 验证内容 | 最近结果 |
|---|---|---|
| `uds-loopback.sh` | ESC-over-UDS 基础：BRD/APRD/APWR/FPRD/M 位、SM watchdog（10 ms → SAFEOP+ERROR 0x001B → ACK）、ESM 前置条件（0x0015/0x0017）、segmented SDO 上传；三轮（内置/`--esm-lax`/manifest 画像） | **EXIT=0** |
| `sdo-ca-probe.sh` | Complete Access：CA 上传（0x1C12/0x1600）、CA 单帧下载、CA 分段下载（140B 对象多段）、分段上传重发容错（首段重复请求幂等） | **EXIT=0**（四模式） |
| `cia402-behavior-probe.sh` | `--behavior cia402` 7 字节 PDO：状态机序列、模式门控、位置跟随/冻结 | **EXIT=0** |
| `cia402-csv-probe.sh` | 11 字节 PDO CSV：速度模式位置累加、速度显示、禁用后归零 | **EXIT=0** |
| `igh-cia402-cst-op-probe.sh` | IgH ecrt 5B CST：OP、`al_states=0x08`、`sw=0x0237`、模式 10、`0x6077=300` | **EXIT=0** |
| `cia402-cst-op-probe.sh` | CST 画像双主站：SOEM 40/40 bits、`0x6071`/`0x6077` 映射、`5O+5I` 进 OP；IgH PREOP+身份 `0xe5e2` | **EXIT=0** |
| `cia402-cst-probe.sh` | 5 字节 PDO CST：状态机序列 + 实际转矩镜像 `0x6071`→`0x6077`、禁用后归零 | **EXIT=0** |
| `cia402-ip-probe.sh` | 7B PDO IP（模式 7）：位置跟随 + Target reached | **EXIT=0** |
| `cia402-tq-probe.sh` | 5B PDO TQ（模式 4）：实际转矩镜像目标转矩，禁用后归零 | **EXIT=0** |
| `cia402-pv-probe.sh` | 11B PDO PV/VL（模式 3/2）：速度累加/速度显示，禁用后归零 | **EXIT=0** |
| `cia402-pp-probe.sh` | 7B PDO PP（模式 1）：new set-point 位置跳变 + Target reached + Set-point acknowledge | **EXIT=0** |
| `cia402-homing-probe.sh` | 7B PDO HM（模式 6）：位置归零 + Homing attained（bit12），bit4 清除后不再置位 | **EXIT=0** |
| `cia402-fault-probe.sh` | `--fault 0x7500`：Fault bit、控制字 bit7 复位、`0x603F` 回读 | **EXIT=0** |
| `cia402-modes-probe.sh` | 三个 CiA 402 画像经 SDO 上传的 `0x6502` 与布局一致（0x01C1/0x03CD/0x0410） | **EXIT=0** |
| `dc-clock-probe.sh` | 模拟 DC 时钟：0x0910 系统时间随帧前进（Tier0 帧驱动） | **EXIT=0** |
| `cia402-sdo-download-probe.sh` | 20 字节可写配置字符串 `0x2100` 的分段 SDO 下载 + 回读一致 | **EXIT=0** |

## 需要 Linux TAP + root（真实主站/内核模块）

| 脚本 | 验证内容 | 最近结果 |
|---|---|---|
| `minimal-esc-responder.sh` | SOEM 扫描到 1 个从站；IgH `ethercat slaves` 可见；单从站拓扑无报错 | **EXIT=0** |
| `soem-process-data.sh` | 未修改 SOEM `simple_ng`：进 OP、1055 轮 **WKC 3**、无 WKC 不符 | **EXIT=0** |
| `soem-mailbox-probe.sh` | SII 声明 128B/CoE 邮箱，`slaveinfo -map` 显示 mbox>0、8 位 I/O 不回归 | **EXIT=0** |
| `sdo-upload-probe.sh` | SOEM CoE 映射 `0x7000/0x6000`；IgH SDO 上传/下载（配置对象）与只读/未知拒绝 | **EXIT=0** |
| `sdo-segmented-upload-probe.sh` | 未修改 SOEM 分段上传 52 字节设备名（`0x1008`） | **EXIT=0** |
| `sdo-info-probe.sh` | SDO Info：OD list/对象描述/条目描述 | **EXIT=0** |
| `esi-sii-scan-probe.sh` | `--esi` ESI→SII：SOEM 扫描身份/PDO 正确 | **EXIT=0** |
| `manifest-slave-probe.sh` | `--manifest` 画像：SOEM 扫描身份、8 位 I/O、OP、WKC 3、`slaveinfo -sdo` | **EXIT=0** |
| `esi-manifest-op-probe.sh` | `--esi`+`--manifest` 组合：SOEM/IgH 双双正常（SOEM mapped 1O+1I、OP、WKC 3；IgH PREOP+身份） | **EXIT=0** |
| `beckhoff-el2008-probe.sh` | EL2008 兼容画像：SOEM 扫描/OP、IgH 身份 | **EXIT=0** |
| `igh-dc-op-probe.sh` | IgH `ecrt_slave_config_dc`（0x0300/1ms）+ 5B CST：仍进 OP、`al_states=0x08`、`dc_wr=8` | **EXIT=0** |
| `soem-dc-probe.sh` | 未修改 SOEM `ecx_configdc()` 路径：`Configuring distributed clock... done`、OP、WKC 3；应答器 `dc_rd/dc_wr > 0` | **EXIT=0** |
| `igh-sv660n-op-probe.sh` | IgH ecrt SV660N 画像（7B + DC 0x0300/1ms）：OP、`sw=0x0637`、模式 8、位置跟随 12345 | **EXIT=0** |
| `sv660n-capability-probe.sh` | SV660N 画像能力位复核：SOEM 56/56、`CoE details=0x0d`（无 SDO Info）、`Has DC: 1`（DC 声明）；IgH PREOP+身份 `0xe5f1` | **EXIT=0** |
| `generic-cia402-probe.sh` | 通用 CiA 402 画像：SOEM 56/56 bits、OP、WKC 3；IgH PREOP+身份 | **EXIT=0** |
| `igh-op-probe.sh` / `igh-el2008-op-probe.sh` | IgH ecrt 夹具（本地构建）配置 domain → OP（`al_states=0x08`、WKC） | **EXIT=0** |
| `igh-cia402-op-probe.sh` | IgH ecrt 7B CiA402：OP + 状态字/模式/位置 | **EXIT=0** |
| `igh-cia402-csv-op-probe.sh` | IgH ecrt 11B CSV：OP + 速度模式位置累加/速度显示 | **EXIT=0** |
| `soem-uds-probe.sh` | SOEM over UDS shim 的探索性夹具（以脚本内注记状态为准） | 见脚本注记 |

## 辅助工具

| 文件 | 用途 |
|---|---|
| `uds_master_probe.py` | 合成主站（UDS）：基础 datagram、SM watchdog、ESM、SDO（expedited/segmented/CA/Info）、CiA402 行为/CSV/CST/故障注入 |
| `sniff_ecat.py` | 抓包/解析 EtherCAT 帧（调试） |
| `tap_ethernet_loopback.py` / `tap_hold.py` | TAP 自环/保持夹具（调试） |

## 运行提示

- UDS 类脚本不需要 root、不需要内核模块，适合 CI；TAP/IgH 类需要 root 与 `ec_master`/`ec_generic`。
- IgH 的 ecrt 应用夹具（`ecrt-*`）在验证机 `/opt/ecsim/work/igh-op/` 本地构建后由脚本使用，
  **源码与二进制均不入库**；脚本在夹具缺失时会给出明确 FAIL。
- 验证机状态可能被前次实验弄乱（模块占用、残留 TAP）；脚本自带清理，必要时重启验证机。
