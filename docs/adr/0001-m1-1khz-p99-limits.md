# ADR 0001：M1 的「8 从站 @1kHz / p99<100µs」为什么当前测不出来

- 状态：已接受（记录限制，不阻塞主线）
- 日期：2026-11-13
- 相关：`docs/plan/M1-exit-checklist.md`、`scripts/bench/shim-latency.sh`、
  `crates/esc-node/examples/chain_latency.rs`、`scripts/spikes/{tap,soem}-multi-slave-probe.sh`

## 背景

路线图 M1 的退出标准包含两条**性能**指标：

1. **8 从站 @ 1 kHz**（一轮 1 ms 内完成收发）；
2. **p99 抖动 < 100 µs**（shim，隔离 CPU）。

功能面已达成：未修改 SOEM 在 UDS shim 与 TAP 上都能数到 **8 个从站**，`slaveinfo` 报
`8 slaves found and configured`、workcounter 24，`simple_ng` 映射 `8O+8I` 且 WKC 24 稳定。

## 实测数据（2026-11-13，验证机 8 核、`isolcpus=2,3`、非实时内核）

| 测量 | 结果 | 说明 |
| --- | --- | --- |
| ESC 链处理（`handle_frame_chain`，8 从站，Rust release） | **483.5 ns/帧**（约 207 万帧/s；本机 Windows 756 ns） | `crates/esc-node/examples/chain_latency.rs` |
| TAP + 未修改 SOEM `simple_ng`（8 从站） | **≈425 Hz**（≈2.35 ms/轮），绑核 + `chrt -f 50/80` 仍 ≈425 Hz | `tap-multi-slave-probe.sh` |
| UDS shim 往返（1 从站，Python 合成主站，2000 帧） | 未绑核 p99≈196 µs；`isolcpus` + 绑核 p99≈184 µs；绑核 + sim RT50/probe RT80 **p99≈172 µs**（p50≈101 µs） | `scripts/bench/shim-latency.sh` |
| UDS shim 往返（8 从站，RT/绑核） | p99≈181 µs | 同上 |

## 分析

- **模拟器侧不是瓶颈**：8 从站链式处理 <1 µs/帧，仅占 1 ms 周期的 **约 0.05%**。
- **TAP 路径**：每轮 ≈2.35 ms，远大于仿真处理时间；瓶颈在 SOEM 的 TAP 收包线程/调度与
  VM 环境（AF_PACKET + 线程唤醒），不是 ESC 链处理。RT/绑核未显著改善。
- **UDS shim p99**：≈172–196 µs，主要由 UDS syscall、调度、以及 **Python 合成主站**自身
  开销构成；p50≈100 µs 的量级与 `simple_ng` 的 1 ms 周期内等待一致。

## 决策

1. **不宣称** M1 的 1 kHz / p99<100 µs 已达成；`docs/plan/M1-exit-checklist.md` 中这两项
   记为 🟡（功能达成，性能待原生基准）。
2. 保留现有基准脚本与微基准，作为**同机回归对比**；数字随环境变化，不能作为对外性能承诺。
3. 后续提升路径（B17）：
   - 用**原生 C/Rust 主站侧基准**（直接收发 EtherCAT 帧，不经 Python/SOEM 应用层）测
     shim 往返与 TAP 往返，分离测量工具开销；
   - 对 SOEM TAP 路径评估收包线程优先级/轮询参数（只改夹具配置，不改 SOEM 源码纳入仓库）；
   - 若仍达不到，按路线图止损线记录"性能指标仅在隔离 CPU + 原生主站下有意义"。

## 影响

- M1 功能退出标准（两家未修改主站零补丁 OP、8 从站、UDS shim、CLI、8DI/8DO、容器无特权）
  均有可复现证据；**性能指标**单独列为未达标项，避免把功能达成混同为性能达成。
- 复现命令见 `docs/plan/M1-exit-checklist.md` 与 `docs/VERIFICATION.md` 对应脚本行。
