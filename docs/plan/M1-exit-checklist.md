# M1 退出标准核对清单（2026-11-13）

> 目的：把路线图（`ethercat-slave-simulator-tech-selection.md` §8.2）里 **M1 最小闭环** 的
> 退出标准逐条对照当前证据，给出"已达成 / 未达成 / 下一步"。**不是认证或承诺**。
> 状态快照见 [STATUS.md](../STATUS.md)，验证脚本索引见 [VERIFICATION.md](../VERIFICATION.md)。

## M1 退出标准（摘自 §8.2）

| # | 退出标准 | 状态 | 证据 / 缺口 | 下一步 |
| --- | --- | --- | --- | --- |
| 1 | 未修改的 `ethercat slaves`（IgH）与 `slaveinfo`（SOEM）**零补丁看到 OP** | ✅ 已达成 | SOEM：`soem-process-data-probe.sh`/`simple_ng` OP + WKC 3；IgH：`igh-op-probe.sh`/`igh-cia402-*-op-probe.sh`（ecrt 夹具，验证机本地）OP；`sv660n-capability-probe.sh` 同时跑两家 | 保持回归（`docs/VERIFICATION.md`） |
| 2 | **8 从站 @ 1 kHz** | 🟡 部分达成 | **UDS shim 已支持 8 从站**：`--slaves 8` + 未修改 SOEM `slaveinfo` 报 `8 slaves found`、`simple_ng` 映射 8O+8I 且 WKC 24 稳定（`soem-multi-slave-probe.sh`）；**1 kHz 频率与 TAP 多从站未验证** | B13：TAP 多从站 + 1kHz/抖动基准 |
| 3 | **p99 抖动 < 100 µs**（shim，隔离 CPU） | ❌ 未达成 | 无抖动基准脚本；shim 是帧驱动，无独立定时器 | B13：`scripts/bench/shim-latency.py`（p50/p99/p999）+ 隔离 CPU 说明 |
| 4 | **Docker 无特权跑通** | ❌ 未达成 | 无 Dockerfile / 容器 CI；UDS shim 本身无需 root（能力上可行） | B14：`docker/Dockerfile` + 容器内 `uds-loopback.sh`/合成主站冒烟 |
| 5 | CLI | ✅ 已达成 | `ec-sim-esc`（TAP/UDS/manifest/ESI/behavior/fault/dump-sii/dump-dict） | — |
| 6 | 8DI/8DO 画像 | ✅ 已达成 | `generic-io`/`beckhoff-el2008` 画像 + `manifest-slave-probe.sh`/`igh-el2008-op-probe.sh` | — |
| 7 | ESC 内核（寄存器/FMMU/SM/AL/SII/WDT） | ✅ 已达成 | `esc-core` + `esc-node`；208+ 单元/集成测试；`docs/VERIFICATION.md` | — |
| 8 | UDS shim | ✅ 已达成 | `--uds` + `uds-loopback.sh`/`soem-uds-probe.sh`（SOEM 零补丁经 UDS 到 OP） | — |

## M2 已提前完成 / 未完成（参考）

- ✅ TAP 真帧（flagship）、DC Tier0（ESC Support + 帧驱动模拟时钟 + SOEM/IgH DC 配置路径）、
  伺服 Profile（CiA 402 常用模式全覆盖 Tier0 + SV660N 兼容画像 + 固定/默认可变 PDO）、
  故障注入 v1（`--fault <code>` + 复位）。
- ❌ WebUI（拓扑/PDO 波形/SDO 编辑/ESI 导入）；32 从站 @1kHz。
- ⚠️ 故障注入矩阵仅 1 种（可注入任意 `0x603F` 错误码），"5 种故障"未覆盖。

## 多从站（B12）范围草案

**目标**：shim 上 1 个进程模拟 **N 个 ESC**（默认 N=8），未修改 SOEM 能数到 N 个从站。

**设计约束**
- 真实 EtherCAT：一帧沿链传播，**每个从站**处理并累积 WKC，最后返回一帧；
  shim 端应保持同一语义（按链序对每个 `Responder` 调 `handle_frame`，只回一帧）。
- 自增寻址：ADP 按链位置递减（现有 `wire::auto_increment_step` 已实现），每级处理后
  ADP+1；广播/逻辑寻址逐级处理；FMMU 按各从站自己的配置命中。
- 从站地址：`--slaves N` 时第 i 个从站 configured address = i+1（可用 `--slave-address` 覆盖）。
- 保持单从站行为完全兼容：N=1 时与当前一致（回归全部现有探针）。

**验收**
1. `ec-sim-esc --uds <sock> --slaves 8`，合成主站 `BRD/APRD` 逐站读到 1..8 身份；
2. 未修改 SOEM `slaveinfo` 经 UDS shim 报 **8 slaves found**（脚本 `soem-multi-slave-probe.sh`）；
3. TAP 上 SOEM `slaveinfo` 报 8 slaves（若 TAP 回环限制则记录限制，见下）；
4. 现有全部单从站探针保持 EXIT=0。

**已知风险**
- TAP 是单接口回环：多个从站的回复帧会在同一接口上重复出现，SOEM 可能把 N 个回复当成
  N 个独立帧处理而不是"一帧链式返回"；若不可行，TAP 多从站记为限制，shim 优先（ADR 记录）。
- 链路断开/拓扑检测（DL Status）需按"每级端口"语义补齐，可能超出本轮范围。

## 残留研究项（不阻塞 M1 退出）

- SV660N 固定映射位长的逐项核对（当前按 CiA 402 标准对象推算，字节数与手册一致）。
- 厂商扩展对象（0x2000 段 H 参数映射）与 `0x603F` 故障码表。
- 真实 DC SYNC0 脉冲/漂移补偿（当前 Tier0 帧驱动模拟时钟）。
