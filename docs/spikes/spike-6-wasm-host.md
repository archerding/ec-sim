# Spike 6：WASM 行为插件宿主（B43 / M3-W1）

- **日期**：2026-11-13 08:00 班（低峰）
- **结论**：✅ **采用 wasmi 0.31.2**（MIT/Apache-2.0，纯 Rust 解释器）作为 M3 行为插件运行时。
  在 `plugin-host` 新增**默认关闭**的 `wasm-runtime` feature：最小宿主契约
  `env.read_pdo` / `env.write_pdo` / `env.log` + `tick` 导出，配套 WAT 夹具与可运行示例
  `wasm_plugin_demo` 端到端通过；**默认构建与 `cargo test --workspace` 仍零第三方依赖**
  （feature 关闭时不访问 crates.io）。
- **边界**：插件只读写宿主 PDO arena 快照，**不进入实时环**、不接触寄存器/FMMU/邮箱/线缆层
  （ADR 0002 D1/D2）。arena 同步、fuel/超时、崩溃隔离属 W2，本 spike 未实现。
- **非认证**：预回归证据，不构成一致性测试/认证结论（`AGENTS.md` §4）。

## 1. 选型对比

| 维度 | **wasmi 0.31.2（选用）** | wasmtime 50.0.1 | 依据 |
| --- | --- | --- | --- |
| 许可 | MIT/Apache-2.0（双许可） | Apache-2.0 WITH LLVM-exception | 仅 wasmi 直接命中 `AGENTS.md` §3 白名单；wasmtime 的 LLVM-exception 变体需单独法务评估（虽通常视为宽松） |
| 执行模型 | 解释器（无 JIT，无运行时代码生成） | Cranelift JIT/AOT | 插件只做 PDO 读写/日志，解释器足够；沙箱面更小、不引入可写可执行内存 |
| MSRV | 0.31 系列未声明 `rust-version`；本机 1.97.1 实测通过 | 48/49/50 → 1.95/1.96/1.97 | workspace 声明 MSRV **1.75**：wasmtime 当前稳定线不兼容；wasmi 1.x/2.x 也要求 1.86 → 0.31.x 是同时满足 MSRV 1.75 的可行线（⚠️ 未在 1.75 工具链实测） |
| 依赖规模 | `wasm-runtime` 共 **20** 个包（含本仓库 2 个 crate） | 数百包、构建分钟级（公开经验，⚠️ 本班未实测） | 见 §3 `cargo tree` |
| 启动开销 | 实例化（含 WAT 解析 + 校验）**605 µs**；预编译 wasm 可再降 | ⚠️ 未测 | release 实测 |
| 单次调用 | `tick` **179.5 ns**（含宿主 arena 写入） | ⚠️ 未测 | release 实测 1 万次均值 |
| wasm 特性 | MVP 子集（wasmparser 0.100） | 完整（SIMD/线程等） | 插件契约只用 `i32` + 线性内存 |

数据来源（2026-11-13 访问）：crates.io index `https://index.crates.io/wa/sm/wasmi`、
`https://index.crates.io/wa/sm/wasmtime`、`https://crates.io/api/v1/crates/wasmi`；
wasmi 源码 `wasmi-0.31.2/Cargo.toml` 的 `license = "MIT/Apache-2.0"`、`wat-1.263.0` 的
`license = "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT"`（按 MIT/Apache-2.0 分支使用，
NOTICE 复核列入 M3 退出检查）。

## 2. 宿主契约（`crates/plugin-host/src/wasm.rs`）

导入（module `env`）：

| 导入 | 语义 | 返回值 |
| --- | --- | --- |
| `read_pdo(offset: i32) -> i32` | 读 PDO arena 偏移处的字节 | `0..=255`；越界 `-1` |
| `write_pdo(offset: i32, value: i32) -> i32` | 写 `value` 低 8 位 | 成功 `0`；越界 `-1` |
| `log(ptr: i32, len: i32) -> i32` | 从插件线性内存 `memory` 读 `len` 字节追加宿主日志 | 成功 `0`；参数非法/越界 `-1`；缓冲满 `-2` |

导出：`tick()`（宿主在**自己的**线程按需调用）。宿主侧状态 `HostState`：PDO arena（默认 64B，
W2 换双缓冲/seqlock）+ 日志缓冲（上限 64 KiB、单次调用 ≤1024B）+ 调用/丢弃计数。
错误类型 `WasmHostError::{InvalidModule, MissingExport, Trap}`；`memory_bytes()` 暴露插件内存大小。

WAT 示例夹具 `crates/plugin-host/fixtures/wasm/counter.wat`（自写，非厂商代码）：
`arena[4] = arena[0] + 1` 并写一行日志，演示 SM2 输出快照 → SM3 输入快照的典型用法。

## 3. 验证与证据

本机（Windows 11，rustc 1.97.1）— 探针：

```
$ bash scripts/spikes/wasm-plugin-probe.sh
wasm-plugin-demo: pdo_len=64 memory_bytes=65536
wasm-plugin-demo: instantiate_us=2247.1
wasm-plugin-demo: pdo_before=29 00 00 00 00 00 00 00
wasm-plugin-demo: pdo_after=29 00 00 00 2a 00 00 00
wasm-plugin-demo: log="tick"
wasm-plugin-demo: log_lines=1 calls=1
wasm-plugin-demo: ticks=10000 total_us=16579.1 per_tick_ns=1657.9
wasm-plugin-demo: log_lines=10001 log_dropped=0
wasm-plugin-demo: invariants=ok
wasm-plugin-demo: OK
OK: WASM 插件宿主（wasmi 0.31，feature wasm-runtime）契约与调用路径全部通过
```

（debug 构建；release 单次 tick 179.5 ns，见下。）

独立 target 目录、从零 release 构建与运行：

```
$ CARGO_TARGET_DIR=/tmp/target-wasm cargo build --release \
    -p plugin-host --features wasm-runtime --example wasm_plugin_demo
   Finished `release` profile [optimized] target(s) in 13.12s
     BUILD_EXIT=0 build_seconds=13.2
     exe_size_bytes=2227200
$ ./wasm_plugin_demo
wasm-plugin-demo: pdo_len=64 memory_bytes=65536
wasm-plugin-demo: instantiate_us=605.5
wasm-plugin-demo: pdo_after=29 00 00 00 2a 00 00 00
wasm-plugin-demo: ticks=10000 total_us=1795.3 per_tick_ns=179.5
wasm-plugin-demo: log_lines=10001 log_dropped=0
wasm-plugin-demo: invariants=ok
wasm-plugin-demo: OK
```

测试与静态检查：

- `cargo test -p plugin-host --features wasm-runtime` → **34 passed**（新增 7 个 wasm 单测：
  正常 tick、越界读/写、缺失 `tick` 导出、非法 wasm、trap 不计数、超大 log 拒绝、`take_logs` 语义）；
- `cargo clippy -p plugin-host --features wasm-runtime --all-targets -- -D warnings` → 通过；
- 默认回归：`cargo test --workspace` → **230 passed**（feature 关闭，不新增依赖、不需 crates.io）；
- `cargo tree -p plugin-host --features wasm-runtime -e normal --depth 1` → `esc-core` + `wasmi 0.31.2` + `wat 1.263.0`，全树 20 包。

⚠️ **Linux 验证机本轮未跑**该探针：首次构建需从 crates.io 拉取可选依赖（验证机源码树是拷贝、
未同步本 feature 的依赖缓存）。验证方法：同步仓库后在验证机执行
`cargo test -p plugin-host --features wasm-runtime` 与 `scripts/spikes/wasm-plugin-probe.sh`（网络经本机代理）。
该探针**不加入** `run-all-probes.sh` 门禁（可选 feature + 需 crates.io，离线应为 SKIP）。

## 4. 已知限制（W2 必须解决）

- **无 fuel/超时/独立线程**：死循环插件会挂住宿主调用线程 → 严禁在实时环线程调用；
  W2 需加 fuel 预算 + 独立 worker 线程 + 崩溃隔离。
- **arena 无同步**：当前是普通 `Vec` 快照，无 seqlock/双缓冲/撕裂保护 → W2。
- **插件内存上限未校验**：`memory` 页数由插件声明（示例 64 KiB）；无 WASI、无文件/网络/时间导入，
  导入白名单只有 3 个函数（实例化时链接器即拒绝未知导入）。
- 无插件签名/来源校验；未接 `plugin.yaml` 的 `behavior:`/WAT 路径声明（W2 定义字段）；
  单实例、无并发/多设备隔离测试；`--features wasm-runtime` 不为 CI 默认构建的一部分。

## 5. 下一步

1. **W2 PDO arena 同步**：双缓冲或 seqlock + generation；插件 worker 线程；压测无撕裂、
   实时环零阻塞；把 wasm 实例生命周期接到 `esc-node` 的非实时线程。
2. fuel/超时与内存上限校验（含 `Config::consume_fuel` 实测）。
3. 许可 NOTICE 复核（wasmi/wat 及传递依赖）并更新 `docs/DEVICE-PLUGINS.md` 的插件宿主章节。
