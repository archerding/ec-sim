//! WASM 行为插件的**非实时运行时**（B44 子项 3 / M3-W2）。
//!
//! 职责边界：
//! - 宿主线程（`esc-node` 帧循环）只调用 [`BehaviorRuntime::publish`] 与
//!   [`BehaviorRuntime::poll_output`]：前者是一次 arena 发布（不分配、不等待），
//!   后者是 `try_lock` 取回已经算好的 SM3（绝不阻塞在插件上）。
//! - 插件的编译/实例化/tick 全部发生在独立 worker 线程；ticker 线程按 `tick_hz`
//!   节流，每个新 generation 最多 tick 一次（主站停帧时不做无用功）。
//! - 单次 tick 有界等待（`tick_timeout`）：插件卡死/死循环时宿主只放弃这一次结果，
//!   worker 由 fuel 预算兜底，ticker 线程继续服务后续 generation。
//!
//! 已知限制（与 `wasm_worker` 一致）：wasmi 0.31 无异步中断，`tick_timeout` 只是
//! "宿主不再等"，单次 tick 的 CPU 上界仍由 `fuel_per_tick` 保证。
//!
//! # 重启与实例配额（B51）
//!
//! - worker 线程死亡（panic/内部关闭）或**连续失败**（trap / out-of-fuel / timeout）达到
//!   [`BehaviorSpec::restart_after_consecutive_errors`] 时，运行时按保留的模块字节重建实例；
//!   最多重启 [`BehaviorSpec::max_restarts`] 次，预算耗尽后**隔离**（ticker 退出，宿主继续
//!   使用最后一个有效输出，不再调用插件）。
//! - [`BehaviorQuota`] 提供进程内**实例配额**（显式传入 `spawn_*_with_quota`）；不传 = 不限制，
//!   保持既有调用语义。`esc-node` 当前每进程只挂 1 个行为桥（配额未接线，属后续多实例工作）。

use crate::arena::{ArenaError, ArenaSnapshot, PdoArena};
use crate::wasm_worker::{TickReport, WasmWorker, WorkerError, WorkerLimits};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// 行为运行时参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorSpec {
    /// tick 频率（Hz，>0）；ticker 线程等待新 generation 的最长间隔 = `1/tick_hz`。
    pub tick_hz: u32,
    /// 宿主等待单次 tick 的超时。
    pub tick_timeout: Duration,
    /// worker 资源限制（SM2/SM3 容量 + fuel + 内存上限）。
    pub limits: WorkerLimits,
    /// 插件崩溃/连续失败后的**最大重启次数**（B51）；0 = 不重启，直接隔离。
    pub max_restarts: u32,
    /// 连续 trap/out-of-fuel/超时达到该次数后触发一次重启（B51；默认 3）。
    pub restart_after_consecutive_errors: u32,
}

impl BehaviorSpec {
    /// 以 SM2/SM3 容量构造（tick_hz=1000、超时 50ms、默认 fuel/内存上限；
    /// 重启策略默认 3 次/连续 3 次失败）。
    pub fn new(sm2_capacity: usize, sm3_capacity: usize) -> Self {
        Self {
            tick_hz: 1000,
            tick_timeout: Duration::from_millis(50),
            limits: WorkerLimits::new(sm2_capacity, sm3_capacity),
            max_restarts: DEFAULT_MAX_RESTARTS,
            restart_after_consecutive_errors: DEFAULT_RESTART_AFTER_ERRORS,
        }
    }

    fn period(&self) -> Duration {
        let hz = u64::from(self.tick_hz.max(1));
        Duration::from_micros(1_000_000 / hz)
    }
}

/// 默认最大重启次数。
pub const DEFAULT_MAX_RESTARTS: u32 = 3;
/// 默认触发重启所需的连续失败次数。
pub const DEFAULT_RESTART_AFTER_ERRORS: u32 = 3;

/// 行为插件实例配额（B51）：进程内允许多少个 [`BehaviorRuntime`] 同时存活。
///
/// 配额是**显式可选**的（`spawn_*_with_quota`）；不传配额 = 不限制，保持既有调用语义。
/// 配额按运行时句柄计数，`BehaviorRuntime` 被丢弃时释放名额。
#[derive(Debug, Clone)]
pub struct BehaviorQuota {
    max: usize,
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl BehaviorQuota {
    /// 创建配额；`max` 至少为 1。
    pub fn new(max: usize) -> Self {
        Self {
            max: max.max(1),
            active: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// 配额上限。
    pub fn max(&self) -> usize {
        self.max
    }

    /// 当前已占用实例数。
    pub fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    fn acquire(&self) -> Result<QuotaGuard, String> {
        let current = self.active.fetch_add(1, Ordering::AcqRel);
        if current >= self.max {
            self.active.fetch_sub(1, Ordering::AcqRel);
            return Err(format!("行为插件实例配额已满（max {}）", self.max));
        }
        Ok(QuotaGuard {
            active: std::sync::Arc::clone(&self.active),
        })
    }
}

struct QuotaGuard {
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for QuotaGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// 已计算完成的插件输出（SM3 段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorOutput {
    /// 本次 tick 对应的 arena generation。
    pub generation: u64,
    /// 插件写回的 SM3 字节。
    pub sm3: Vec<u8>,
}

/// 行为运行时统计（只增计数 + 最近一次状态）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BehaviorStats {
    /// 成功 tick 次数。
    pub ticks: u64,
    /// 超时放弃的次数。
    pub timeouts: u64,
    /// fuel 耗尽次数。
    pub out_of_fuel: u64,
    /// 其他错误（trap/协议）次数。
    pub errors: u64,
    /// 最近一次成功 tick 的 worker 侧耗时（ns）。
    pub last_tick_ns: u64,
    /// 最近一次成功 tick 的 generation。
    pub last_generation: u64,
    /// 插件实例重启次数（B51）。
    pub restarts: u64,
    /// 是否已隔离（重启预算耗尽或无法重建）。
    pub quarantined: bool,
    /// worker 是否已停止（panic/关闭/隔离）。
    pub stopped: bool,
    /// 最近一次错误文本。
    pub last_error: Option<String>,
}

#[derive(Default)]
struct Counters {
    ticks: AtomicU64,
    timeouts: AtomicU64,
    out_of_fuel: AtomicU64,
    errors: AtomicU64,
    last_tick_ns: AtomicU64,
    last_generation: AtomicU64,
    restarts: AtomicU64,
    quarantined: AtomicBool,
    stopped: AtomicBool,
}

struct Shared {
    arena: PdoArena,
    /// 最新待取输出（单槽：新结果覆盖旧结果）。
    pending: Mutex<Option<BehaviorOutput>>,
    /// 宿主已取走的 generation（避免重复交付同一结果）。
    delivered: AtomicU64,
    wake: Mutex<bool>,
    wake_cv: Condvar,
    stop: AtomicBool,
    counters: Counters,
    last_error: Mutex<Option<String>>,
    /// 插件模块字节：worker 崩溃后按此重建实例（B51）。
    module: Vec<u8>,
    /// worker 资源限制（重建时复用）。
    limits: WorkerLimits,
    /// 重启预算与触发阈值（来自 [`BehaviorSpec`]）。
    max_restarts: u32,
    restart_after_consecutive_errors: u32,
}

/// 非实时 WASM 行为运行时：宿主句柄 + 独立 ticker 线程。
pub struct BehaviorRuntime {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// 实例配额守卫（`None` = 未启用配额）；Drop 时释放名额。
    _quota_guard: Option<QuotaGuard>,
}

impl BehaviorRuntime {
    /// 从 WAT 文本启动行为插件（不限制实例配额）。
    pub fn spawn_wat(text: &str, spec: BehaviorSpec) -> Result<Self, String> {
        Self::spawn_wat_with_quota(text, spec, None)
    }

    /// 从 WAT 文本启动行为插件，并占用一个实例配额（B51）。
    pub fn spawn_wat_with_quota(
        text: &str,
        spec: BehaviorSpec,
        quota: Option<&BehaviorQuota>,
    ) -> Result<Self, String> {
        let bytes = wat::parse_str(text).map_err(|error| format!("启动行为插件失败：{error}"))?;
        Self::spawn_wasm_bytes_with_quota(&bytes, spec, quota)
    }

    /// 从 wasm 二进制启动行为插件（不限制实例配额）。
    pub fn spawn_wasm_bytes(bytes: &[u8], spec: BehaviorSpec) -> Result<Self, String> {
        Self::spawn_wasm_bytes_with_quota(bytes, spec, None)
    }

    /// 从 wasm 二进制启动行为插件，并占用一个实例配额（B51）。
    pub fn spawn_wasm_bytes_with_quota(
        bytes: &[u8],
        spec: BehaviorSpec,
        quota: Option<&BehaviorQuota>,
    ) -> Result<Self, String> {
        let guard = match quota {
            Some(quota) => Some(quota.acquire()?),
            None => None,
        };
        let worker = WasmWorker::spawn_wasm_bytes(bytes, spec.limits)
            .map_err(|error| format!("启动行为插件失败：{error}"))?;
        Self::spawn_worker(worker, spec, bytes.to_vec(), guard)
    }

    fn spawn_worker(
        worker: WasmWorker,
        spec: BehaviorSpec,
        module: Vec<u8>,
        guard: Option<QuotaGuard>,
    ) -> Result<Self, String> {
        let arena = PdoArena::new(spec.limits.sm2_capacity, spec.limits.sm3_capacity)
            .map_err(|error| format!("创建 PDO arena 失败：{error}"))?;
        let shared = Arc::new(Shared {
            arena,
            pending: Mutex::new(None),
            delivered: AtomicU64::new(0),
            wake: Mutex::new(false),
            wake_cv: Condvar::new(),
            stop: AtomicBool::new(false),
            counters: Counters::default(),
            last_error: Mutex::new(None),
            module,
            limits: spec.limits,
            max_restarts: spec.max_restarts,
            restart_after_consecutive_errors: spec.restart_after_consecutive_errors.max(1),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("ec-sim-behavior-ticker".to_string())
            .spawn(move || ticker_loop(worker, thread_shared, spec))
            .map_err(|error| format!("启动行为 ticker 线程失败：{error}"))?;
        Ok(Self {
            shared,
            thread: Some(thread),
            _quota_guard: guard,
        })
    }

    /// 发布一帧的 PDO 映像（`[SM2 | SM3]` 两段）。
    ///
    /// 宿主线程调用：不分配、不等待读者；返回 arena generation（失败 = 长度超容量，
    /// 说明调用方与 `BehaviorSpec` 的容量不一致）。
    pub fn publish(&self, sm2: &[u8], sm3: &[u8]) -> Result<u64, ArenaError> {
        let generation = self.shared.arena.publish(sm2, sm3)?;
        if let Ok(mut wake) = self.shared.wake.lock() {
            *wake = true;
            self.shared.wake_cv.notify_all();
        }
        Ok(generation)
    }

    /// 非阻塞取回最新插件输出；没有新结果时返回 `None`。
    ///
    /// 同一 generation 只交付一次（重复调用返回 `None`）；`try_lock` 失败说明
    /// ticker 正在写入，直接返回 `None`（下一帧再取，绝不阻塞）。
    pub fn poll_output(&self) -> Option<BehaviorOutput> {
        let mut pending = self.shared.pending.try_lock().ok()?;
        let output = pending.as_ref()?;
        let generation = output.generation;
        if generation <= self.shared.delivered.load(Ordering::Acquire) {
            return None;
        }
        let output = pending.take()?;
        self.shared.delivered.store(generation, Ordering::Release);
        Some(output)
    }

    /// 统计快照。
    pub fn stats(&self) -> BehaviorStats {
        let counters = &self.shared.counters;
        BehaviorStats {
            ticks: counters.ticks.load(Ordering::Relaxed),
            timeouts: counters.timeouts.load(Ordering::Relaxed),
            out_of_fuel: counters.out_of_fuel.load(Ordering::Relaxed),
            errors: counters.errors.load(Ordering::Relaxed),
            last_tick_ns: counters.last_tick_ns.load(Ordering::Relaxed),
            last_generation: counters.last_generation.load(Ordering::Relaxed),
            restarts: counters.restarts.load(Ordering::Relaxed),
            quarantined: counters.quarantined.load(Ordering::Relaxed),
            stopped: counters.stopped.load(Ordering::Relaxed),
            last_error: self
                .shared
                .last_error
                .lock()
                .ok()
                .and_then(|error| error.clone()),
        }
    }

    /// 当前 arena generation（宿主侧便于观察）。
    pub fn latest_generation(&self) -> u64 {
        self.shared.arena.latest_generation()
    }
}

impl Drop for BehaviorRuntime {
    /// 只置停止位并唤醒 ticker，**不 join**：插件卡在 tick 里也不会拖住宿主。
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Ok(mut wake) = self.shared.wake.lock() {
            *wake = true;
            self.shared.wake_cv.notify_all();
        }
        // detach：JoinHandle 丢弃即分离线程；worker 自己会在 Drop 时发关闭信号。
        let _ = self.thread.take();
    }
}

fn ticker_loop(mut worker: WasmWorker, shared: Arc<Shared>, spec: BehaviorSpec) {
    let period = spec.period();
    let mut last_generation = 0u64;
    let mut consecutive_errors = 0u32;
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        if shared.arena.latest_generation() == last_generation {
            // 没有新帧：最多等一个 tick 周期（可被 publish/drop 立即唤醒）。
            if let Ok(guard) = shared.wake.lock() {
                let (mut guard, _timeout) = shared
                    .wake_cv
                    .wait_timeout(guard, period)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = false;
            }
            continue;
        }
        let snapshot: ArenaSnapshot = match shared.arena.try_snapshot(4) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        let started = Instant::now();
        match worker.tick(&snapshot, spec.tick_timeout) {
            Ok(report) => {
                consecutive_errors = 0;
                shared.counters.ticks.fetch_add(1, Ordering::Relaxed);
                shared.counters.last_tick_ns.store(
                    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );
                last_generation = report.generation;
                store_report(&shared, report);
            }
            Err(WorkerError::Timeout { .. }) => {
                // 这一代放弃；迟到结果由 worker 内部按请求 id 丢弃。避免对同一代反复排队。
                shared.counters.timeouts.fetch_add(1, Ordering::Relaxed);
                consecutive_errors += 1;
                last_generation = snapshot.generation;
                if consecutive_errors >= shared.restart_after_consecutive_errors {
                    match maybe_restart_worker(&shared) {
                        Some(new_worker) => {
                            worker = new_worker;
                            consecutive_errors = 0;
                        }
                        None => return,
                    }
                }
            }
            Err(error @ WorkerError::OutOfFuel) => {
                shared.counters.out_of_fuel.fetch_add(1, Ordering::Relaxed);
                record_error(&shared, error.to_string());
                consecutive_errors += 1;
                last_generation = snapshot.generation;
                if consecutive_errors >= shared.restart_after_consecutive_errors {
                    match maybe_restart_worker(&shared) {
                        Some(new_worker) => {
                            worker = new_worker;
                            consecutive_errors = 0;
                        }
                        None => return,
                    }
                }
            }
            Err(WorkerError::Stopped) => {
                // worker 线程已死（panic/内部关闭）：B51 按模块字节重建实例。
                last_generation = snapshot.generation;
                record_error(&shared, "插件 worker 已停止，尝试重启".to_string());
                match maybe_restart_worker(&shared) {
                    Some(new_worker) => {
                        worker = new_worker;
                        consecutive_errors = 0;
                    }
                    None => return,
                }
            }
            Err(error) => {
                shared.counters.errors.fetch_add(1, Ordering::Relaxed);
                record_error(&shared, error.to_string());
                consecutive_errors += 1;
                last_generation = snapshot.generation;
                if consecutive_errors >= shared.restart_after_consecutive_errors {
                    match maybe_restart_worker(&shared) {
                        Some(new_worker) => {
                            worker = new_worker;
                            consecutive_errors = 0;
                        }
                        None => return,
                    }
                }
            }
        }
    }
}

fn record_error(shared: &Shared, message: String) {
    if let Ok(mut slot) = shared.last_error.lock() {
        *slot = Some(message);
    }
}

/// 按保留的模块字节重建 worker；预算耗尽/重建失败 → 隔离（ticker 退出）。
fn maybe_restart_worker(shared: &Shared) -> Option<WasmWorker> {
    let restarts = shared.counters.restarts.load(Ordering::Relaxed);
    if restarts >= u64::from(shared.max_restarts) {
        record_error(
            shared,
            format!("插件已隔离：重启预算耗尽（{} 次）", shared.max_restarts),
        );
        quarantine(shared);
        return None;
    }
    match WasmWorker::spawn_wasm_bytes(&shared.module, shared.limits) {
        Ok(worker) => {
            shared.counters.restarts.fetch_add(1, Ordering::Relaxed);
            Some(worker)
        }
        Err(error) => {
            record_error(shared, format!("插件重启失败：{error}"));
            quarantine(shared);
            None
        }
    }
}

fn quarantine(shared: &Shared) {
    shared.counters.quarantined.store(true, Ordering::Release);
    shared.counters.stopped.store(true, Ordering::Release);
}

fn store_report(shared: &Shared, report: TickReport) {
    shared
        .counters
        .last_generation
        .store(report.generation, Ordering::Relaxed);
    if let Ok(mut pending) = shared.pending.lock() {
        *pending = Some(BehaviorOutput {
            generation: report.generation,
            sm3: report.sm3,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 计数器插件：SM3[0] = SM2[0] + 1（SM2 容量 4、SM3 容量 4 → 映像 8 字节）。
    const COUNTER_WAT: &str = r#"
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "tick")
    (local $v i32)
    (local.set $v (call $read_pdo (i32.const 0)))
    (if (i32.ge_s (local.get $v) (i32.const 0))
      (then
        (drop (call $write_pdo (i32.const 4) (i32.add (local.get $v) (i32.const 1))))))))
"#;

    /// 慢插件：每 tick 空转约 300 万次（模拟"卡一会儿"的行为逻辑）。
    const SLOW_WAT: &str = r#"
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "tick")
    (local $i i32)
    (local $v i32)
    (local.set $v (call $read_pdo (i32.const 0)))
    (block $done
      (loop $loop
        (br_if $done (i32.ge_s (local.get $i) (i32.const 3000000)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $loop)))
    (if (i32.ge_s (local.get $v) (i32.const 0))
      (then
        (drop (call $write_pdo (i32.const 4) (i32.add (local.get $v) (i32.const 1))))))))
"#;

    fn spec(tick_hz: u32) -> BehaviorSpec {
        BehaviorSpec {
            tick_hz,
            tick_timeout: Duration::from_millis(200),
            limits: WorkerLimits::new(4, 4).with_fuel_per_tick(Some(100_000_000)),
            max_restarts: DEFAULT_MAX_RESTARTS,
            restart_after_consecutive_errors: DEFAULT_RESTART_AFTER_ERRORS,
        }
    }

    /// 输入为 1 时 trap、否则写 SM3 = SM2 + 1；用于验证重启/隔离。
    const TRAP_ON_ONE_WAT: &str = r#"
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "tick")
    (local $v i32)
    (local.set $v (call $read_pdo (i32.const 0)))
    (if (i32.eq (local.get $v) (i32.const 1))
      (then (unreachable)))
    (drop (call $write_pdo (i32.const 4) (i32.add (local.get $v) (i32.const 1))))))
"#;

    fn restart_spec(max_restarts: u32, restart_after: u32) -> BehaviorSpec {
        BehaviorSpec {
            tick_hz: 2000,
            tick_timeout: Duration::from_millis(200),
            limits: WorkerLimits::new(4, 4).with_fuel_per_tick(Some(100_000_000)),
            max_restarts,
            restart_after_consecutive_errors: restart_after,
        }
    }

    #[test]
    fn trap_triggers_restart_then_quarantine_after_budget() {
        let runtime =
            BehaviorRuntime::spawn_wat(TRAP_ON_ONE_WAT, restart_spec(1, 1)).expect("spawn");
        // 第一代 trap → 重启一次（预算 1）。
        runtime
            .publish(&[1, 0, 0, 0], &[0, 0, 0, 0])
            .expect("publish");
        let deadline = Instant::now() + Duration::from_secs(2);
        while runtime.stats().restarts == 0 {
            assert!(
                Instant::now() < deadline,
                "应触发一次重启：{:?}",
                runtime.stats()
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!runtime.stats().quarantined);
        // 第二代再 trap → 预算耗尽 → 隔离，ticker 退出。
        runtime
            .publish(&[1, 0, 0, 0], &[0, 0, 0, 0])
            .expect("publish");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !runtime.stats().quarantined {
            assert!(
                Instant::now() < deadline,
                "重启预算耗尽后应隔离：{:?}",
                runtime.stats()
            );
            thread::sleep(Duration::from_millis(5));
        }
        let stats = runtime.stats();
        assert_eq!(stats.restarts, 1, "{stats:?}");
        assert!(stats.stopped, "{stats:?}");
        // 隔离后不再处理新 generation。
        runtime
            .publish(&[5, 0, 0, 0], &[0, 0, 0, 0])
            .expect("publish");
        thread::sleep(Duration::from_millis(100));
        assert_eq!(runtime.poll_output(), None);
    }

    #[test]
    fn restart_recovers_on_next_generation() {
        let runtime =
            BehaviorRuntime::spawn_wat(TRAP_ON_ONE_WAT, restart_spec(3, 1)).expect("spawn");
        runtime
            .publish(&[1, 0, 0, 0], &[0, 0, 0, 0])
            .expect("publish");
        let deadline = Instant::now() + Duration::from_secs(2);
        while runtime.stats().restarts == 0 {
            assert!(
                Instant::now() < deadline,
                "应触发一次重启：{:?}",
                runtime.stats()
            );
            thread::sleep(Duration::from_millis(5));
        }
        // 重启后的新实例处理下一代输入并产出结果。
        runtime
            .publish(&[5, 0, 0, 0], &[0, 0, 0, 0])
            .expect("publish");
        let deadline = Instant::now() + Duration::from_secs(2);
        let output = loop {
            if let Some(output) = runtime.poll_output() {
                break output;
            }
            assert!(
                Instant::now() < deadline,
                "重启后应恢复输出：{:?}",
                runtime.stats()
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(output.sm3, vec![6, 0, 0, 0]);
        assert_eq!(runtime.stats().restarts, 1, "{:?}", runtime.stats());
        assert!(!runtime.stats().quarantined);
    }

    #[test]
    fn instance_quota_limits_concurrent_runtimes() {
        let quota = BehaviorQuota::new(2);
        let first = BehaviorRuntime::spawn_wat_with_quota(COUNTER_WAT, spec(1000), Some(&quota))
            .expect("first");
        assert_eq!(quota.active(), 1);
        let second = BehaviorRuntime::spawn_wat_with_quota(COUNTER_WAT, spec(1000), Some(&quota))
            .expect("second");
        assert_eq!(quota.active(), 2);
        let error =
            match BehaviorRuntime::spawn_wat_with_quota(COUNTER_WAT, spec(1000), Some(&quota)) {
                Ok(_) => panic!("over quota must fail"),
                Err(error) => error,
            };
        assert!(error.contains("配额已满"), "{error}");
        assert_eq!(quota.active(), 2);
        drop(first);
        assert_eq!(quota.active(), 1);
        let third = BehaviorRuntime::spawn_wat_with_quota(COUNTER_WAT, spec(1000), Some(&quota))
            .expect("third");
        assert_eq!(quota.active(), 2);
        drop(second);
        drop(third);
        assert_eq!(quota.active(), 0);
    }

    #[test]
    fn publish_then_poll_returns_sm3() {
        let runtime = BehaviorRuntime::spawn_wat(COUNTER_WAT, spec(2000)).expect("spawn");
        assert_eq!(
            runtime
                .publish(&[41, 0, 0, 0], &[0, 0, 0, 0])
                .expect("publish"),
            1
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let output = loop {
            if let Some(output) = runtime.poll_output() {
                break output;
            }
            assert!(
                Instant::now() < deadline,
                "2s 内应有输出：{:?}",
                runtime.stats()
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(output.generation, 1);
        assert_eq!(output.sm3, vec![42, 0, 0, 0]);
        let stats = runtime.stats();
        assert_eq!(stats.ticks, 1, "{stats:?}");
        assert_eq!(stats.last_generation, 1);
        assert!(stats.last_tick_ns > 0);
        // 同一 generation 只交付一次。
        assert_eq!(runtime.poll_output(), None);
    }

    #[test]
    fn host_side_publish_and_poll_never_block_on_slow_plugin() {
        // 慢插件：单次 tick 约数百毫秒（debug 解释器），但最终会结束。宿主侧仍必须有界返回。
        let slow_spec = BehaviorSpec {
            tick_timeout: Duration::from_secs(2),
            ..spec(1000)
        };
        let runtime = BehaviorRuntime::spawn_wat(SLOW_WAT, slow_spec).expect("spawn");
        let mut worst_publish = Duration::ZERO;
        for value in 1..=20u8 {
            let started = Instant::now();
            runtime
                .publish(&[value, 0, 0, 0], &[0, 0, 0, 0])
                .expect("publish");
            worst_publish = worst_publish.max(started.elapsed());
            // 宿主侧取输出同样必须有界（没有结果就立刻返回 None）。
            let started = Instant::now();
            let _maybe = runtime.poll_output();
            assert!(
                started.elapsed() < Duration::from_millis(50),
                "poll_output 阻塞了 {:?}",
                started.elapsed()
            );
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            worst_publish < Duration::from_millis(50),
            "publish 最坏耗时 {worst_publish:?}"
        );
        // 慢插件最终仍会产出结果（ticker 线程没有被宿主拖死）。
        let deadline = Instant::now() + Duration::from_secs(5);
        let output = loop {
            if let Some(output) = runtime.poll_output() {
                break output;
            }
            assert!(
                Instant::now() < deadline,
                "慢插件应有输出：{:?}",
                runtime.stats()
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert!(
            output.sm3[0] >= 2,
            "插件输出应由某个发布的 SM2 值 +1 得到：{:?}",
            output.sm3
        );
        assert!(runtime.stats().ticks >= 1, "{:?}", runtime.stats());
    }

    #[test]
    fn invalid_module_is_rejected_at_spawn() {
        let error = match BehaviorRuntime::spawn_wat("(module (not wasm))", spec(1000)) {
            Ok(_) => panic!("非法 WAT 应失败"),
            Err(error) => error,
        };
        assert!(error.contains("启动行为插件失败"), "{error}");
    }

    #[test]
    fn drop_returns_promptly_even_when_worker_is_busy() {
        let runtime = BehaviorRuntime::spawn_wat(SLOW_WAT, spec(1000)).expect("spawn");
        runtime
            .publish(&[1, 0, 0, 0], &[0, 0, 0, 0])
            .expect("publish");
        thread::sleep(Duration::from_millis(1));
        let started = Instant::now();
        drop(runtime);
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "drop 阻塞了 {:?}",
            started.elapsed()
        );
    }
}
