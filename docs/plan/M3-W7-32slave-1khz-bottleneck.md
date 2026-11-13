# M3-W7：32 从站 @ 1 kHz 瓶颈调研（结论型文档，**不承诺达成**）

> 对应 backlog [B45](../../AGENTS.md) 的 M3-W7；关联 [ADR 0001](../adr/0001-m1-1khz-p99-limits.md)、
> [M3 范围](M3-scope.md)、[M1 退出清单](M1-exit-checklist.md)。
> 本文只记录**测量口径、实测数据、瓶颈分解与可行路径**；**不宣称** 32 从站 @1 kHz 已达成，
> 也不把任何厂商/主站样例的性能当作产品承诺（AGENTS §4）。

## 1. 问题与范围

- 目标：回答"32 从站、1 kHz 周期（1 ms/轮）在当前 EC-Sim 路径上卡在哪"。
- 本轮交付：可复现的**模拟器侧链处理扩展性**数据、**TAP 往返**数据、SOEM 周期模型事实、
  可行路径与成本评估、下一轮验证建议。
- 非目标：修改 SOEM/IgH（只作独立夹具/只读参考，AGENTS §3）、宣称 CTT/一致性、
  给出对外性能 SLA、达成 1 kHz 里程碑。

## 2. 既有数据（本轮之前）

| 来源 | 数据 |
| --- | --- |
| ADR 0001（M1） | 8 从站 TAP + 未修改 SOEM `simple_ng` "≈425 Hz"；UDS shim 8 从站 "≈860 Hz"；UDS shim RT/绑核 p99≈172–196 µs |
| `chain_latency`（B16） | 1/2/4/8 从站 = 0.13/0.21/0.39/0.76 µs/帧（Windows）；验证机 8 从站 ≈0.48 µs/帧 |
| `uds_latency`（B17） | 忙轮询 p99 9.3 µs（1 站）/14.6 µs（8 站）；默认阻塞 p99≈157 µs |

## 3. 测量口径修正（本轮关键发现）

1. `simple_ng` 样例每次迭代含**固定 `osal_usleep(5000)`**（见 SOEM 源码
   `samples/simple_ng/simple_ng.c`，只读参考），所以 `iterations / 输出总时间` 得到的是
   **样例节拍上限（≈158–170 轮/s）**，**不是控制环频率**，不能用于 1 kHz 判据。
2. 主站侧可比的指标是每轮**往返时间**（`fieldbus.roundtrip_time`，输出行
   `<usec> usec  WKC <n>`）：本轮已把 `tap-multi-slave-probe.sh` 改为统计该值，
   并移除原先会误导的"估算循环频率"。
3. ⚠️ 因此 ADR 0001 里的 "≈425 Hz（TAP，8 站）" 与 "≈860 Hz（UDS shim）" **口径存疑**：
   两者若来自同一样例的 `iterations/T`，会被 fixed sleep 限制到 ≈170/s 量级，与记录不一致，
   说明当时可能用了不同样例/版本或另有计数方式。**验证方法**：在验证机用同一 SOEM 夹具重跑
   `simple_ng`，同时保留 `usec WKC` 行与 `Iteration` 行数，按 roundtrip 统计复核。

## 4. 本轮实测

环境：Linux 验证机（openEuler 24.03，`nproc=6` 可见，**非隔离 CPU、未实时调度**）；
Windows 开发机（非实时）。均为 `--release`。

### 4.1 模拟器侧 ESC 链处理（`chain_latency`，单 datagram BRD 读 0x0130）

| 从站数 | 验证机 ns/帧 | 验证机 µs/帧 | Windows ns/帧 | Windows µs/帧 |
| --- | --- | --- | --- | --- |
| 1 | 72.8 | 0.073 | 157.8 | 0.158 |
| 2 | 139.8 | 0.140 | 214.5 | 0.214 |
| 4 | 253.5 | 0.253 | 391.0 | 0.391 |
| 8 | 460.6 | 0.461 | 753.5 | 0.753 |
| 16 | 874.3 | 0.874 | 1489.8 | 1.490 |
| **32** | **1704.0** | **1.704** | **2992.4** | **2.992** |
| 64 | 3481.7 | 3.482 | 5831.7 | 5.832 |

- 边际成本 ≈ **52 ns/从站**（验证机）、≈90 ns/从站（Windows），近线性。
- **32 从站链处理 1.70 µs ≈ 1 ms 周期的 0.17%**（Windows 约 0.30%）→ 模拟器 ESC 链**不是瓶颈**。
- ⚠️ 本测量是**单 datagram 帧**；`simple_ng` 每周期实际帧的 datagram 数由
  `IOsegment` 分割决定（SOEM `ec_main.c` 中按 segment 循环 `ecx_setupdatagram(EC_CMD_LRW)`），
  32 站/更大 PDO 下可能多帧/周期，**该形状的链处理成本本轮未测**（下一步：扩展
  `chain_latency` 为每站一个 datagram 的帧形状）。

### 4.2 TAP 往返（未修改 SOEM `simple_ng`，8 秒窗口）

| 从站数 | 解析样本 n | roundtrip min / avg / max（µs） | WKC |
| --- | --- | --- | --- |
| 8 | 1392 | 20 / **164** / 813 | 24（全轮正确） |
| 32 | 1344 | 26 / **167** / 344 | 96（全轮正确） |

- **8 → 32 从站，往返 avg 几乎不变（164 → 167 µs）**：主导项是 TAP 唤醒/调度开销，
  不是每从站处理；32 站单轮 167 µs **在 1 ms 预算内（≈16.7%）**。
- 8 站出现 813 µs 长尾（非隔离 CPU/未 RT）→ 1 kHz 下 tail 抖动仍需隔离 CPU + RT 复核。
- 样例节拍 ≈158–170 轮/s 由 fixed 5 ms sleep 决定（§3），**不能**据此判定 1 kHz 未达成。

### 4.3 合成 UDS 主站 / 原生探测（既有，供对比）

- UDS shim 忙轮询：p50 3.7 µs / p99 9.3 µs（1 站）、p99 14.6 µs（8 站）→ 线缆+ESC 合计远小于 1 ms。
- 默认阻塞模式 p99≈157 µs → 主要是两次进程唤醒调度，不是协议处理。

## 5. 瓶颈分解（32 从站 @ 1 kHz 预算 = 1000 µs/轮）

| 环节 | 实测/估计 | 占预算 | 判定 |
| --- | --- | --- | --- |
| ESC 链处理（32 站，单 datagram 帧） | 1.7 µs | 0.17% | 不是瓶颈 |
| TAP 往返（未修改 SOEM，8/32 站） | ≈164–167 µs | ≈17% | 单轮可行；长尾需 RT |
| SOEM 样例节拍 | 158–170 轮/s | — | **口径问题**（fixed 5 ms sleep），非能力上限 |
| 帧内多 datagram/segment（32 站大 PDO） | ⚠️ 未测 | 未知 | 待测 |
| 主站应用/调度抖动（非隔离 CPU、非 RT） | 8 站 max 813 µs | 峰值 >1 ms | 需隔离 CPU + `SCHED_FIFO` 复核 |

## 6. 可行路径与成本（按性价比排序，**均需下一轮验证**）

1. **主站应用实时化（最低成本）**：去掉样例 `osal_usleep`、改为 `clock_nanosleep` 周期 + 
   `SCHED_FIFO` + `isolcpus` 绑核；按 roundtrip 与 p99 复测 32 站 1 kHz。
   预期：单轮 167 µs + 链处理 1.7 µs，余量充足；风险是调度抖动与 TAP 长尾。
2. **UDS shim 路线（跨平台/CI）**：忙轮询已 p99 9.3–14.6 µs；32 站扩展性下一步用
   `uds-master` 原生探测（`uds_latency`）按 32 站测。
3. **TAP 收包优化（中成本）**：评估 `PACKET_MMAP`/`AF_XDP`/XDP redirect；需自研夹具或
   独立进程主站，不能改 SOEM 源码入库（AGENTS §3）。收益是减少每帧 syscall/唤醒。
4. **原生 Rust 主站基准（建议作为 B46）**：不依赖 SOEM 样例，直接 1 kHz 发
   32 站 LRW 帧并统计 p50/p99/max/jitter，作为"模拟器+线缆能力"的独立证据。
5. **帧形状验证**：确认 32 站实际帧是否单帧（`IOsegment`）还是多帧；多帧时链处理与线缆
   roundtrip 都会线性增长。

## 7. 结论

- **不承诺也不宣称** "32 从站 @1 kHz 已达成/未达成"；本轮证据只支持：
  - 模拟器 ESC 链处理在 32 站仅 ~1.7 µs（0.17% 预算），**不是瓶颈**；
  - TAP 上未修改 SOEM 的单轮往返 ≈167 µs（32 站），**单轮在 1 ms 内**；
  - 既有 "425/860 Hz" 记录口径存疑，应复核（§3）。
- 是否达成 1 kHz 取决于：主站应用实时调度 + 非隔离环境长尾 + 帧形状（多 datagram/多帧）。
  **⚠️ 待验证**：隔离 CPU + `SCHED_FIFO` 下按 1 kHz 实际发帧 32 站的 p50/p99/max。
- 下一步（建议立项 B46）：原生 32 站 1 kHz 基准（Rust，`clock_nanosleep` + RT），
  输出 p50/p99/max/jitter 并与本文数据对齐；同时补 §4.1 的多 datagram 帧形状测量。

## 8. 复现

```bash
# 1) 模拟器侧链处理扩展性（任意平台，零第三方依赖）
cargo run --release -q -p esc-node --example chain_latency -- 20000 1,2,4,8,16,32,64

# 2) TAP 往返（Linux 验证机，root；未修改 SOEM 夹具需本机构建）
N=8  bash scripts/spikes/tap-multi-slave-probe.sh
N=32 bash scripts/spikes/tap-multi-slave-probe.sh
```

> 夹具路径：验证机本地 SOEM 构建（`SOEM_SLAVEINFO` / `SOEM_SIMPLE_NG` 可覆盖），
> 源码/二进制**不入库**（AGENTS §3/§8）。
