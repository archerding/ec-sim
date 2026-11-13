//! WASM 行为插件的**独占 worker 线程**（B44 子项 2，M3-W2）。
//!
//! 仅在 `wasm-runtime` feature 下编译（默认关闭，不引入第三方依赖）。
//!
//! # 解决什么
//!
//! `AGENTS.md` §1 + ADR 0002 D1/D2：插件**永不进入实时环**、崩溃/死循环不得拖住宿主。
//! B43 的 [`WasmPlugin`](crate::wasm::WasmPlugin) 在调用线程序里直接跑 `tick`，一个
//! `(loop (br 0))` 就能把宿主线程卡死。本模块把插件实例整体搬进**独占线程**：
//!
//! - 宿主只通过命令通道投递 **PDO arena 快照**（[`ArenaSnapshot`]，来自
//!   [`crate::arena`]），不共享可变状态；
//! - 宿主用 `recv_timeout` 等结果，**永不无限阻塞**：插件卡住 → [`WorkerError::Timeout`]，
//!   宿主照常继续（该 tick 的迟到结果按请求 id 丢弃）；
//! - 插件线程用 wasmi **fuel 计量**兜底：死循环会在耗尽预算后 trap 为
//!   [`WorkerError::OutOfFuel`]，线程仍可继续服务后续 tick；
//! - 插件线性内存用 `StoreLimits` 限制（实例化与 `memory.grow` 都受检）。
//!
//! # 已知限制（⚠️ 属 W2 未完成范围）
//!
//! - wasmi 0.31 **没有**从外部中断正在执行的 wasm 的手段：`Timeout` 只是"宿主不再等"，
//!   worker 线程会在 fuel 预算耗尽后才退出该次 tick。因此 fuel 是**唯一**的 CPU 上界，
//!   `Timeout` 必须与有限的 `fuel_per_tick` 配合使用；
//! - 插件 panic 目前会杀掉 worker 线程（之后 tick 返回 [`WorkerError::Stopped`]）；
//!   未做 panic 捕获与自动重启；
//! - 未与 `esc-node` 实时环接线（arena 仍由调用方发布），也未做多实例配额。

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::arena::ArenaSnapshot;
use crate::wasm::{HostConfig, HostState, WasmHostError, WasmPlugin};

/// 默认每个 tick 的 fuel 预算（≈ 千万条 wasm 指令，正常插件远用不到）。
pub const DEFAULT_FUEL_PER_TICK: u64 = 10_000_000;

/// 默认插件线性内存上限：16 MiB。
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 16 * 1024 * 1024;

/// worker 初始化（编译 + 实例化）握手超时。
pub const INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// worker 资源限制与人口布局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLimits {
    /// SM2（输出，主站 → 从站）方向容量，也是插件可见映像的**前段**长度。
    pub sm2_capacity: usize,
    /// SM3（输入，从站 → 主站）方向容量。
    pub sm3_capacity: usize,
    /// 每个 tick 追加的 fuel 预算；`None` = 不计 fuel（仅调试用，可能死循环）。
    pub fuel_per_tick: Option<u64>,
    /// 插件线性内存上限（字节）；`None` = 不限制。
    pub max_memory_bytes: Option<usize>,
}

impl Default for WorkerLimits {
    fn default() -> Self {
        Self {
            sm2_capacity: crate::arena::DEFAULT_PDO_CAPACITY,
            sm3_capacity: crate::arena::DEFAULT_PDO_CAPACITY,
            fuel_per_tick: Some(DEFAULT_FUEL_PER_TICK),
            max_memory_bytes: Some(DEFAULT_MAX_MEMORY_BYTES),
        }
    }
}

impl WorkerLimits {
    /// 指定 SM2/SM3 容量（fuel 与内存上限取默认值）。
    pub fn new(sm2_capacity: usize, sm3_capacity: usize) -> Self {
        Self {
            sm2_capacity,
            sm3_capacity,
            ..Self::default()
        }
    }

    /// 设置每 tick fuel 预算（`None` = 关闭计量）。
    pub fn with_fuel_per_tick(mut self, fuel: Option<u64>) -> Self {
        self.fuel_per_tick = fuel;
        self
    }

    /// 设置线性内存上限（`None` = 不限制）。
    pub fn with_memory_limit(mut self, max_bytes: Option<usize>) -> Self {
        self.max_memory_bytes = max_bytes;
        self
    }

    fn validate(&self) -> Result<(), WorkerError> {
        if self.sm2_capacity == 0 || self.sm3_capacity == 0 {
            return Err(WorkerError::InvalidLimits(
                "SM2/SM3 capacity must be non-zero".to_string(),
            ));
        }
        if self.sm2_capacity > crate::arena::MAX_PDO_CAPACITY
            || self.sm3_capacity > crate::arena::MAX_PDO_CAPACITY
        {
            return Err(WorkerError::InvalidLimits(format!(
                "SM2/SM3 capacity must not exceed {}",
                crate::arena::MAX_PDO_CAPACITY
            )));
        }
        if self.fuel_per_tick == Some(0) {
            return Err(WorkerError::InvalidLimits(
                "fuel_per_tick must be positive".to_string(),
            ));
        }
        Ok(())
    }

    /// 插件可见 PDO 映像总长度（SM2 段 + SM3 段）。
    pub fn pdo_len(&self) -> usize {
        self.sm2_capacity + self.sm3_capacity
    }
}

/// worker 侧错误（也用于宿主侧返回）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    /// 线程创建失败。
    Spawn(String),
    /// 限制参数非法。
    InvalidLimits(String),
    /// 模块无法解析/链接/实例化（含内存上限拒绝）。
    InvalidModule(String),
    /// 调用 trap（非 fuel）。
    Trap(String),
    /// 本 tick 的 fuel 预算耗尽（疑似死循环）。
    OutOfFuel,
    /// 宿主等待超时：worker 仍在跑该 tick，结果会被丢弃。
    Timeout {
        /// 宿主等待的时长。
        timeout: Duration,
        /// 被放弃的 tick 请求 id（便于日志关联）。
        tick_id: u64,
    },
    /// worker 线程已退出（被关闭、panic 或通道断开）。
    Stopped,
    /// 内部协议错误（不应发生）。
    Protocol(&'static str),
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(msg) => write!(f, "failed to spawn plugin worker: {msg}"),
            Self::InvalidLimits(msg) => write!(f, "invalid worker limits: {msg}"),
            Self::InvalidModule(msg) => write!(f, "invalid wasm module: {msg}"),
            Self::Trap(msg) => write!(f, "wasm plugin trapped: {msg}"),
            Self::OutOfFuel => f.write_str("wasm plugin exhausted its per-tick fuel budget"),
            Self::Timeout { timeout, tick_id } => write!(
                f,
                "plugin worker did not answer tick {tick_id} within {timeout:?}"
            ),
            Self::Stopped => f.write_str("plugin worker has stopped"),
            Self::Protocol(msg) => write!(f, "plugin worker protocol error: {msg}"),
        }
    }
}

impl std::error::Error for WorkerError {}

impl WorkerError {
    fn from_host(err: WasmHostError) -> Self {
        match err {
            WasmHostError::OutOfFuel => Self::OutOfFuel,
            WasmHostError::Trap(msg) => Self::Trap(msg),
            WasmHostError::MissingExport(name) => {
                Self::InvalidModule(format!("missing required export: {name}"))
            }
            WasmHostError::InvalidModule(msg) => Self::InvalidModule(msg),
        }
    }
}

/// 一次成功 tick 的结果（宿主线程私有副本，不引用 worker 内部存储）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickReport {
    /// 本次 tick 使用的 arena 快照序号。
    pub generation: u64,
    /// 本次 tick 消耗的 fuel（未开启计量时为 0）。
    pub fuel_consumed: u64,
    /// worker 侧执行耗时。
    pub elapsed: Duration,
    /// tick 后的 SM2 段。
    pub sm2: Vec<u8>,
    /// tick 后的 SM3 段。
    pub sm3: Vec<u8>,
    /// 本次 tick 追加的日志。
    pub log: String,
    /// 插件累计成功 tick 次数。
    pub calls: u64,
}

enum Command {
    Tick { id: u64, snapshot: ArenaSnapshot },
    Shutdown,
}

enum Event {
    Ready,
    Ticked { id: u64, report: TickReport },
    Failed { id: Option<u64>, error: WorkerError },
}

/// 独占线程的 WASM 插件实例。
///
/// 宿主侧句柄：投递 tick、有界等待结果、关闭。`Drop` 只发关闭信号并**detach**
/// worker 线程（不 join），因此即使插件卡在 tick 里也不会阻塞宿主。
pub struct WasmWorker {
    commands: Sender<Command>,
    events: Receiver<Event>,
    join: Option<JoinHandle<()>>,
    limits: WorkerLimits,
    next_tick_id: u64,
}

impl WasmWorker {
    /// 从 WAT 文本启动 worker。
    pub fn spawn_wat(text: &str, limits: WorkerLimits) -> Result<Self, WorkerError> {
        let bytes = wat::parse_str(text).map_err(|e| WorkerError::InvalidModule(e.to_string()))?;
        Self::spawn_wasm_bytes(&bytes, limits)
    }

    /// 从 wasm 二进制启动 worker（模块编译与实例化都发生在 worker 线程内）。
    pub fn spawn_wasm_bytes(bytes: &[u8], limits: WorkerLimits) -> Result<Self, WorkerError> {
        limits.validate()?;
        let (commands, command_rx) = mpsc::channel::<Command>();
        let (event_tx, events) = mpsc::channel::<Event>();
        let owned = bytes.to_vec();
        let join = thread::Builder::new()
            .name("ec-sim-wasm-plugin".to_string())
            .spawn(move || worker_main(owned, limits, command_rx, event_tx))
            .map_err(|e| WorkerError::Spawn(e.to_string()))?;
        match events.recv_timeout(INIT_TIMEOUT) {
            Ok(Event::Ready) => Ok(Self {
                commands,
                events,
                join: Some(join),
                limits,
                next_tick_id: 1,
            }),
            Ok(Event::Failed { error, .. }) => {
                let _ = join.join();
                Err(error)
            }
            Ok(_) => {
                let _ = join.join();
                Err(WorkerError::Protocol("unexpected event during init"))
            }
            Err(RecvTimeoutError::Timeout) => Err(WorkerError::Spawn(format!(
                "worker did not become ready within {INIT_TIMEOUT:?}"
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(WorkerError::Stopped),
        }
    }

    /// 生效的限制参数。
    pub fn limits(&self) -> WorkerLimits {
        self.limits
    }

    /// 投递一次 tick 并在 `timeout` 内等待结果。
    ///
    /// - 成功：返回插件 tick 后的 PDO 映像与统计；
    /// - 超时：[`WorkerError::Timeout`]，worker 不会被杀死，迟到的结果按请求 id 丢弃；
    /// - fuel 耗尽：[`WorkerError::OutOfFuel`]，worker 仍可继续处理后续 tick。
    pub fn tick(
        &mut self,
        snapshot: &ArenaSnapshot,
        timeout: Duration,
    ) -> Result<TickReport, WorkerError> {
        let id = self.next_tick_id;
        self.next_tick_id += 1;
        self.commands
            .send(Command::Tick {
                id,
                snapshot: snapshot.clone(),
            })
            .map_err(|_| WorkerError::Stopped)?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let received = if remaining.is_zero() {
                Err(RecvTimeoutError::Timeout)
            } else {
                self.events.recv_timeout(remaining)
            };
            match received {
                Ok(Event::Ticked { id: got, report }) => {
                    if got == id {
                        return Ok(report);
                    }
                    // 上一次超时 tick 的迟到结果：丢弃。
                }
                Ok(Event::Failed {
                    id: Some(got),
                    error,
                }) => {
                    if got == id {
                        return Err(error);
                    }
                }
                Ok(Event::Failed { id: None, error }) => return Err(error),
                // 初始化事件只会在 spawn 阶段出现。
                Ok(Event::Ready) => {}
                Err(RecvTimeoutError::Timeout) => {
                    return Err(WorkerError::Timeout {
                        timeout,
                        tick_id: id,
                    })
                }
                Err(RecvTimeoutError::Disconnected) => return Err(WorkerError::Stopped),
            }
        }
    }

    /// 请求 worker 退出并**不等待**线程结束（卡住的 tick 不可抢占）。
    pub fn shutdown(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        // detach：join 会阻塞宿主，正是本模块要避免的。
        self.join.take();
    }
}

impl Drop for WasmWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// worker 线程主体：实例化插件，然后串行处理命令。
fn worker_main(
    bytes: Vec<u8>,
    limits: WorkerLimits,
    command_rx: Receiver<Command>,
    event_tx: Sender<Event>,
) {
    let mut config = HostConfig::new(limits.pdo_len());
    if let Some(max) = limits.max_memory_bytes {
        config = config.with_memory_limit(max);
    }
    if let Some(fuel) = limits.fuel_per_tick {
        config = config.with_fuel_per_tick(fuel);
    }
    let mut plugin = match WasmPlugin::from_wasm_bytes_with_config(&bytes, &config) {
        Ok(plugin) => plugin,
        Err(err) => {
            let _ = event_tx.send(Event::Failed {
                id: None,
                error: WorkerError::from_host(err),
            });
            return;
        }
    };
    if event_tx.send(Event::Ready).is_err() {
        return;
    }
    while let Ok(command) = command_rx.recv() {
        match command {
            Command::Shutdown => return,
            Command::Tick { id, snapshot } => {
                apply_snapshot(plugin.state_mut(), &snapshot, limits.sm2_capacity);
                let fuel_before = plugin.fuel_consumed();
                let started = Instant::now();
                let result = plugin.call_tick();
                let elapsed = started.elapsed();
                let event = match result {
                    Ok(()) => {
                        let state = plugin.state();
                        let pdo = state.pdo();
                        let fuel_after = plugin.fuel_consumed();
                        Event::Ticked {
                            id,
                            report: TickReport {
                                generation: snapshot.generation,
                                fuel_consumed: match (fuel_after, fuel_before) {
                                    (Some(after), Some(before)) => after.saturating_sub(before),
                                    _ => 0,
                                },
                                elapsed,
                                sm2: pdo[..limits.sm2_capacity].to_vec(),
                                sm3: pdo[limits.sm2_capacity..].to_vec(),
                                log: state.log().to_string(),
                                calls: state.calls(),
                            },
                        }
                    }
                    Err(err) => Event::Failed {
                        id: Some(id),
                        error: WorkerError::from_host(err),
                    },
                };
                if event_tx.send(event).is_err() {
                    return;
                }
            }
        }
    }
}

/// 把 arena 快照写进插件可见映像：`[SM2 | SM3]`，尾部按容量补零。
fn apply_snapshot(state: &mut HostState, snapshot: &ArenaSnapshot, sm2_capacity: usize) {
    let pdo = state.pdo_mut();
    pdo.fill(0);
    let sm2 = snapshot.sm2.len().min(sm2_capacity);
    pdo[..sm2].copy_from_slice(&snapshot.sm2[..sm2]);
    let sm3 = snapshot.sm3.len().min(pdo.len() - sm2_capacity);
    pdo[sm2_capacity..sm2_capacity + sm3].copy_from_slice(&snapshot.sm3[..sm3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 计数器插件：arena[0]（SM2 首字节）→ +1 写到 arena[sm2_capacity]（SM3 首字节），
    /// 并写一行 `tick` 日志。与 `wasm.rs` 的 B43 夹具语义一致。
    const COUNTER_WAT: &str = r#"
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "tick")
  (func (export "tick")
    (local $v i32)
    (local.set $v (call $read_pdo (i32.const 0)))
    (if (i32.ge_s (local.get $v) (i32.const 0))
      (then
        (drop (call $write_pdo (i32.const 4) (i32.add (local.get $v) (i32.const 1))))
        (drop (call $log (i32.const 0) (i32.const 4)))))))
"#;

    fn snapshot(generation: u64, sm2: &[u8], sm3: &[u8]) -> ArenaSnapshot {
        ArenaSnapshot {
            generation,
            sm2: sm2.to_vec(),
            sm3: sm3.to_vec(),
        }
    }

    fn counter_worker() -> WasmWorker {
        let limits = WorkerLimits::new(4, 4).with_fuel_per_tick(Some(1_000_000));
        WasmWorker::spawn_wat(COUNTER_WAT, limits).expect("spawn worker")
    }

    #[test]
    fn worker_tick_applies_snapshot_and_reports_output() {
        let mut worker = counter_worker();
        let report = worker
            .tick(&snapshot(7, &[41], &[]), Duration::from_secs(5))
            .expect("tick");
        assert_eq!(report.generation, 7);
        assert_eq!(report.sm2, vec![41, 0, 0, 0]);
        assert_eq!(report.sm3, vec![42, 0, 0, 0]);
        assert_eq!(report.log, "tick");
        assert_eq!(report.calls, 1);
        assert!(report.fuel_consumed > 0, "fuel metering should be on");
        assert!(report.elapsed < Duration::from_secs(5));
    }

    #[test]
    fn tick_sequence_is_serial_and_keeps_plugin_state() {
        let mut worker = counter_worker();
        let first = worker
            .tick(&snapshot(1, &[10], &[]), Duration::from_secs(5))
            .expect("tick 1");
        assert_eq!(first.sm3[0], 11);
        let second = worker
            .tick(&snapshot(2, &[20], &[]), Duration::from_secs(5))
            .expect("tick 2");
        assert_eq!(second.sm3[0], 21);
        assert_eq!(second.calls, 2);
        // 日志累计：第二次报告里包含两行同样的 "tick"。
        assert_eq!(second.log, "ticktick");
    }

    #[test]
    fn runaway_plugin_is_stopped_by_fuel_not_by_host() {
        // 纯死循环：没有 fuel 兜底会永久卡死 worker。
        let wat = r#"(module (func (export "tick") (loop $l (br $l))))"#;
        let limits = WorkerLimits::new(4, 4).with_fuel_per_tick(Some(100_000));
        let mut worker = WasmWorker::spawn_wat(wat, limits).expect("spawn worker");
        let started = Instant::now();
        let err = worker
            .tick(&snapshot(1, &[], &[]), Duration::from_secs(10))
            .expect_err("infinite loop must be bounded");
        assert_eq!(err, WorkerError::OutOfFuel);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "fuel must bound a runaway tick, took {:?}",
            started.elapsed()
        );
        // worker 仍存活，可继续服务（fuel 预算会再次追加）。
        let err = worker
            .tick(&snapshot(2, &[], &[]), Duration::from_secs(10))
            .expect_err("still infinite");
        assert_eq!(err, WorkerError::OutOfFuel);
    }

    #[test]
    fn host_times_out_without_blocking_and_recovers_on_next_tick() {
        // 有限但耗时的循环：让 worker 忙 ≈ 数十毫秒。
        let wat = r#"
(module
  (func (export "tick")
    (local $i i32)
    (local.set $i (i32.const 2000000))
    (loop $l
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))
      (br_if $l (local.get $i)))))
"#;
        let limits = WorkerLimits::new(4, 4).with_fuel_per_tick(Some(500_000_000));
        let mut worker = WasmWorker::spawn_wat(wat, limits).expect("spawn worker");
        let started = Instant::now();
        let err = worker
            .tick(&snapshot(1, &[], &[]), Duration::from_millis(1))
            .expect_err("host must not wait for a slow tick");
        assert!(
            matches!(err, WorkerError::Timeout { .. }),
            "expected Timeout, got {err:?}"
        );
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "host was blocked by the worker"
        );
        // 宿主未被拖死：后续 tick 仍能拿到结果（上一次的迟到事件被按 id 丢弃）。
        let report = worker
            .tick(&snapshot(2, &[], &[]), Duration::from_secs(10))
            .expect("recovered tick");
        assert_eq!(report.generation, 2);
        assert_eq!(report.calls, 2);
    }

    #[test]
    fn memory_limit_rejects_oversized_linear_memory() {
        // 4 页（256 KiB）初始内存 > 64 KiB 上限 → 实例化即失败。
        let wat = r#"(module (memory (export "memory") 4) (func (export "tick")))"#;
        let limits = WorkerLimits::new(8, 8).with_memory_limit(Some(64 * 1024));
        match WasmWorker::spawn_wat(wat, limits) {
            Err(WorkerError::InvalidModule(_)) => {}
            Err(other) => panic!("expected InvalidModule, got {other:?}"),
            Ok(_) => panic!("oversized memory must be rejected"),
        }
        // 1 页在限额内 → 正常启动。
        let wat = r#"(module (memory (export "memory") 1) (func (export "tick")))"#;
        let limits = WorkerLimits::new(8, 8).with_memory_limit(Some(64 * 1024));
        let mut worker = WasmWorker::spawn_wat(wat, limits).expect("1 page fits");
        worker
            .tick(&snapshot(1, &[], &[]), Duration::from_secs(5))
            .expect("tick");
    }

    #[test]
    fn missing_tick_export_is_reported_at_spawn() {
        let wat = r#"(module (memory (export "memory") 1))"#;
        match WasmWorker::spawn_wat(wat, WorkerLimits::new(8, 8)) {
            Err(WorkerError::InvalidModule(msg)) => assert!(msg.contains("tick")),
            other => panic!("expected InvalidModule, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn invalid_limits_are_rejected() {
        let wat = r#"(module (func (export "tick")))"#;
        match WasmWorker::spawn_wat(wat, WorkerLimits::new(0, 8)) {
            Err(WorkerError::InvalidLimits(_)) => {}
            other => panic!("expected InvalidLimits, got {:?}", other.map(|_| ())),
        }
        match WasmWorker::spawn_wat(wat, WorkerLimits::new(8, 8).with_fuel_per_tick(Some(0))) {
            Err(WorkerError::InvalidLimits(_)) => {}
            other => panic!("expected InvalidLimits, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn dropping_a_busy_worker_does_not_block_the_host() {
        let wat = r#"
(module
  (func (export "tick")
    (local $i i32)
    (local.set $i (i32.const 50000000))
    (loop $l
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))
      (br_if $l (local.get $i)))))
"#;
        let limits = WorkerLimits::new(4, 4).with_fuel_per_tick(Some(10_000_000_000));
        let mut worker = WasmWorker::spawn_wat(wat, limits).expect("spawn worker");
        let err = worker
            .tick(&snapshot(1, &[], &[]), Duration::from_millis(1))
            .expect_err("slow tick");
        assert!(matches!(err, WorkerError::Timeout { .. }));
        let started = Instant::now();
        drop(worker);
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "Drop must detach instead of joining the busy worker"
        );
    }

    #[test]
    fn pdo_len_is_sum_of_capacities() {
        let limits = WorkerLimits::new(8, 12);
        assert_eq!(limits.pdo_len(), 20);
    }
}
