# B46：原生 32 站 @ 1 kHz 基准（能力证据，**不承诺对外 SLA**）

> 对应 backlog [B46](../../AGENTS.md)；上承 [M3-W7 瓶颈调研](M3-W7-32slave-1khz-bottleneck.md)。
> 本文只记录**方法、实测数据、瓶颈归因与限制**；不宣称 32 站 @1 kHz 已达成/未达成，
> 也不把本机/验证机的调度抖动当作产品指标（AGENTS §4）。

## 1. 问题与范围

- 目标：补上 M3-W7 §7 的 ⚠️——在**隔离 CPU + `SCHED_FIFO`** 下按 1 kHz 实际发 32 站帧，
  给出 p50/p99/max 与截止时间超限比例；同时补 M3-W7 §4.1 未测的**多 datagram / PDO 式 LRW**
  帧形状的链处理成本。
- 本轮做法：模拟器**单进程内**构建 N 个 `Responder` 组成的链，用原生 Rust 循环当"主站"，
  按 1 ms 周期发帧并统计（`crates/esc-node/examples/chain_1khz.rs`）；
  帧形状扩展进 `chain_latency`（`single` / `lrw` / `fprd`）。
- 非目标：不改 SOEM/IgH（只作只读参考）；不测 TAP/UDS 线缆层往返（已有 `tap-multi-slave-probe.sh`
  / `uds_latency` 覆盖）；不测多从站 DC 同步；不宣称实时保证。

## 2. 方法

### 2.1 帧形状（`chain_latency.rs`）

| shape | 构造 | WKC 期望（N 从站） |
| --- | --- | --- |
| `single` | 1 条 BRD 读 AL Status（B45 口径） | `N` |
| `lrw` | 1 条 **LRW** 覆盖 PDO 区：每站 2 个 FMMU——输出 4B（写，Type=0x02）+ 输入 4B（读，Type=0x01），逻辑/物理同址 `0x1000+8i` | `3N`（写 +2、读 +1） |
| `fprd` | **N 条 FPRD**（每站一条，配置站地址寻址）——多 datagram 最坏形状，**非**典型周期帧 | `N` |

> `lrw` 与真实主站（SOEM `simple_ng` / IgH）的 PDO 帧同形：输出区写、输入区读，
> 32 站 WKC=96。`fprd` 每帧 N 条 datagram，每个从站都要解析全部 datagram，链处理是 O(N²)。

### 2.2 1 kHz 基准（`chain_1khz.rs`）

- 每轮：等到周期点（`hybrid`=先 sleep 到临界再自旋 / `sleep` / `spin`）→ 发 LRW 帧 →
  记录**帧处理耗时**（handle_frame_chain 往返）、**唤醒延迟**（send−target）、
  **周期偏差**（相邻 send 间隔−1000 µs）、**截止超限**（done−(t0+周期) > 0 的比例）。
- 预热 200 轮并校验 WKC（不符直接退出 1，防止"假打中"的测量）。
- Linux 可 `--rt`：直接声明 libc `sched_setscheduler(0, SCHED_FIFO, prio=80)`（零第三方依赖）。
- 环境：Windows 开发机（非隔离/非 RT，5 s=5000 轮）；Linux 验证机（openEuler 24.03，6 vCPU，
  `isolcpus=2-3`，VMware；`taskset -c 2`，10 s=10000 轮，除注明外）。

## 3. 实测数据

### 3.1 链处理成本 vs 帧形状（`chain_latency`，20 000 轮/点）

Linux 验证机（`taskset -c 2`）：

| 从站数 | `single` µs/帧 | `lrw` µs/帧（WKC） | `fprd` µs/帧（WKC） |
| --- | --- | --- | --- |
| 1 | 0.100 | 0.153 (3) | 0.075 (1) |
| 8 | 0.475 | 1.045 (24) | 1.243 (8) |
| **32** | **1.720** | **4.270 (96)** | **13.955 (32)** |
| 64 | 3.433 | 8.736 (192) | 47.613 (64) |

Windows 开发机：

| 从站数 | `single` | `lrw` | `fprd` |
| --- | --- | --- | --- |
| 32 | 2.938 | 4.790 | 14.437 |
| 64 | 5.853 | 9.706 | 46.412 |

- `single` 与 M3-W7 一致（32 站 1.72 µs，边际 ≈52 ns/站）。
- `lrw`（真实 PDO 帧形状）32 站 **4.27 µs ≈ 1 ms 的 0.43%**，边际 ≈134–150 ns/站，线性。
- `fprd` 是**最坏形状**：32 站 13.96 µs（1.4% 预算），64 站 47.6 µs（线性假设失效，O(N²)）；
  真实主站不会每周期发 N 条 FPRD，但该数据给出了"每站一条 datagram"的保守上界。

### 3.2 1 kHz 周期统计

Windows（5 s / 5000 轮，非隔离、非 RT）：

| 配置 | 帧处理 p50/p99/max µs | 周期偏差 p50/p99/max_abs µs | 截止超限 |
| --- | --- | --- | --- |
| 32 站 LRW `hybrid` | 5.0 / 17.9 / 89.5 | 0 / 2.8 / 259.3 | **0/5000** |
| 8 站 LRW `hybrid` | 1.4 / 7.6 / 36.2 | 0 / 3.1 / 183.6 | **0/5000** |
| 32 站 LRW `sleep` | 18.8 / 48.0 / 352.1 | 15.4 / 527.0 / 1344.6 | 14/5000（0.28%） |
| 32 站 `single` `hybrid` | 3.2 / 12.4 / 72.2 | 0 / 1.0 / 166.8 | 0/5000 |
| 32 站 `fprd` `hybrid` | 14.9 / 30.8 / 199.4 | 0 / 0.5 / 267.6 | 0/5000 |

Linux 验证机（`taskset -c 2`，isolcpus=2-3；10 s / 10000 轮）：

| 配置 | 帧处理 p50/p99/max µs | 唤醒延迟 p50/p99/max µs | 周期偏差 p50/p99/max_abs µs | 截止超限 |
| --- | --- | --- | --- | --- |
| 32 站 LRW `hybrid --rt` | 6.48 / 12.45 / 21.4 | 51.3 / 716.5 / 6969.5 | 22.5 / 537.2 / 6612.6 | 12/10000（0.12%） |
| 32 站 LRW `hybrid`（无 RT） | 6.35 / 13.17 / 82.6 | 227.2 / 834.6 / 1815.7 | 35.7 / 564.1 / 1569.3 | 27/10000（0.27%） |
| 8 站 LRW `hybrid --rt` | 2.38 / 6.00 / 66.6 | 192.6 / 821.9 / 7451.1 | 34.3 / 558.0 / 7165.8 | 24/10000（0.24%） |
| 32 站 LRW `spin`（无 RT，5 s） | 5.05 / 54.33 / 104.5 | 0 / 47.6 / 113.8 | 0 / 46.3 / 113.7 | **0/5000** |
| 32 站 LRW `spin --rt`（默认 RT 带宽） | 4.95 / 13.94 / 110.6 | 0 / 12943.6 / 23787.1 | 0 / 38.3 / 23781.7 | **221/10000（2.21%）** |
| 32 站 LRW `spin --rt`（`rt_runtime_us=-1`，5 s） | 4.99 / 15.43 / 111.4 | 0 / 45.1 / 158.4 | 0 / 44.7 / 158.3 | **0/5000** |

### 3.3 关键归因（两个反直觉结果）

1. **VMware 宿主机抢占**：`hybrid --rt` 的 max 唤醒延迟 ≈7 ms 只出现一次，来自宿主 vCPU 被抢占，
   **不是** ESC 链处理（帧处理 max 仅 21 µs）。这类 tail 无法在被测 VM 内用 RT 策略消除。
2. **Linux RT 带宽节流**：`spin --rt` 反而 2.21% 超限、出现 12–24 ms 停顿——纯自旋的
   `SCHED_FIFO` 线程超过默认 `sched_rt_runtime_us=950000`（95% CPU/秒）后被内核节流。
   把 `rt_runtime_us` 临时设为 `-1`（**测后已恢复 950000**）后同一配置 **0/5000 超限**，
   p99 唤醒 45 µs。→ 结论：**忙轮询不要与受限 RT 策略叠加**；或改用 `hybrid`（sleep 到临界）。
   另：`sleep` 模式受内核唤醒粒度影响（Linux ≈300–400 µs 唤醒延迟，Windows 计时器 ≈15.6 ms 粒度 → 0.28% 超限）。

## 4. 结论（仅限本轮证据）

- **模拟器侧 ESC 链处理不是 1 kHz 瓶颈**：真实 PDO 形状（32 站 LRW）4.27 µs ≈ 0.43% 周期预算；
  即使最坏的多 datagram 形状 13.96 µs 也只占 1.4%。
- 32 站 @1 kHz **在 Windows（非 RT）与验证机（隔离 CPU + 忙轮询/`hybrid`）上本轮均无超限
  （0/5000）**；带 RT 的 `hybrid` 仍有 0.10–0.12% 超限，主要来自虚拟化宿主抢占。
- **不承诺对外 SLA**；本轮未覆盖：TAP/UDS 线缆层往返与 32 站 1 kHz 的组合、真实 PDO 数据行为、
  DC 同步、长时间（小时级）稳定性、硬件主机（非 VM）RT 表现。⚠️ 待验证。

## 5. 复现

```bash
# 链处理（任意平台，零第三方依赖）
cargo run --release -p esc-node --example chain_latency -- 20000 1,8,32,64 single,lrw,fprd

# 1 kHz 周期（任意平台；Linux 可加 --rt，验证机建议 taskset -c 2）
cargo run --release -p esc-node --example chain_1khz -- 32 10 hybrid lrw
taskset -c 2 cargo run --release -p esc-node --example chain_1khz -- 32 10 spin lrw --rt
# 若 RT 忙轮询出现毫秒级停顿，先检查 sched_rt_runtime_us（默认 950000 = 95% 上限）：
#   cat /proc/sys/kernel/sched_rt_runtime_us
```

> 验证机要跑 baseline 需先把源码同步到本地目录（本仓库不在验证机上）；本例零第三方依赖，
> `cargo build --release --offline -p esc-node --examples` 即可（本次实测 1.3 s）。
