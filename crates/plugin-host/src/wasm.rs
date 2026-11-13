//! WASM 行为插件宿主（M3-W1 spike，B43）。
//!
//! 仅在 `wasm-runtime` feature 下编译（**默认关闭**，因此 `cargo test --workspace`
//! 不引入任何第三方依赖）。运行时选型、许可与实测数据见
//! `docs/spikes/spike-6-wasm-host.md`。
//!
//! # 插件契约
//!
//! 导入（module `env`，全部返回 `i32` 状态码，除 `read_pdo`）：
//!
//! - `read_pdo(offset: i32) -> i32`：返回 PDO arena 中该偏移的字节（`0..=255`）；
//!   越界返回 `-1`。
//! - `write_pdo(offset: i32, value: i32) -> i32`：写入 `value` 的低 8 位；成功 `0`，越界 `-1`。
//! - `log(ptr: i32, len: i32) -> i32`：从插件线性内存 `memory` 读 `len` 字节追加到宿主日志；
//!   成功 `0`，参数非法/内存越界 `-1`，日志缓冲已满 `-2`。
//!
//! 导出：`tick()`（无参无返回）——宿主在**自己**的线程里按需调用，不进入实时环。
//!
//! # 边界（ADR 0002 D1/D2）
//!
//! - 插件只看到宿主持有的 PDO arena 快照，不接触寄存器阵列、FMMU、线缆层或 ESM；
//! - 插件崩溃/陷入死循环不得阻塞实时环：B44 子项 2 起，[`HostConfig`] 可开启
//!   **fuel 计量**（每 tick 指令预算，耗尽 → [`WasmHostError::OutOfFuel`]）与
//!   **线性内存上限**（`StoreLimits`，实例化与 `memory.grow` 都受检）；
//!   独立 worker 线程 + 宿主侧超时见 [`crate::wasm_worker`]；
//! - 单线程内单次 `tick` **不可被抢占**（wasmi 0.31 无异步中断），因此仍须用
//!   fuel 兜底 + 宿主侧 `recv_timeout` 保证宿主不被饿死。

use wasmi::core::TrapCode;
use wasmi::{
    Caller, Config, Engine, Extern, Instance, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc,
};

/// 默认 PDO arena 长度（字节）。
///
/// 与 `esc-node` 的 SM2/SM3 布局无关：arena 是宿主给插件的**只读/可写快照**，
/// 由宿主每个周期按 ESM 状态同步（W2 实现）。
pub const DEFAULT_PDO_LEN: usize = 64;

/// 单次 `log` 调用允许的最大字节数（防止插件一次调用塞爆宿主缓冲）。
pub const MAX_LOG_CALL: usize = 1024;

/// 宿主日志缓冲上限（字节）；超过后 `log` 返回 `-2` 并计入丢弃计数。
pub const MAX_LOG_BYTES: usize = 64 * 1024;

/// 插件宿主错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmHostError {
    /// wasm/wat 模块无法解析或链接。
    InvalidModule(String),
    /// 模块缺少必需的导出（如 `tick`）。
    MissingExport(&'static str),
    /// 实例化或调用时发生 trap。
    Trap(String),
    /// 本 tick 的 fuel 预算耗尽（插件疑似死循环）。
    OutOfFuel,
}

impl std::fmt::Display for WasmHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidModule(msg) => write!(f, "invalid wasm module: {msg}"),
            Self::MissingExport(name) => write!(f, "missing required export: {name}"),
            Self::Trap(msg) => write!(f, "wasm trap: {msg}"),
            Self::OutOfFuel => f.write_str("wasm plugin exhausted its per-tick fuel budget"),
        }
    }
}

impl std::error::Error for WasmHostError {}

/// 实例化配置（B44 子项 2）：PDO 长度、线性内存上限、每 tick fuel 预算。
///
/// 默认值 = B43 spike 行为（不限制内存、不计 fuel），因此 [`WasmPlugin::from_wat`] /
/// [`WasmPlugin::from_wasm_bytes`] 的语义不变；需要隔离时显式构造本结构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostConfig {
    /// 插件可见 PDO 映象长度（字节）。
    pub pdo_len: usize,
    /// 单次线性内存分配上限（字节）；`None` = 不限制。
    pub max_memory_bytes: Option<usize>,
    /// 每个 tick 追加的 fuel 预算；`None` = 不计 fuel（可能死循环）。
    ///
    /// 预算**单调累积**：上一 tick 未用完的额度可被下一 tick 使用，因此 1 个 tick
    /// 的指令数上限 = 已追加预算总和 − 已消耗量，仍然有界。
    pub fuel_per_tick: Option<u64>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            pdo_len: DEFAULT_PDO_LEN,
            max_memory_bytes: None,
            fuel_per_tick: None,
        }
    }
}

impl HostConfig {
    /// 以 PDO 长度构造默认配置（无内存/fuel 限制）。
    pub fn new(pdo_len: usize) -> Self {
        Self {
            pdo_len,
            ..Self::default()
        }
    }

    /// 开启线性内存上限。
    pub fn with_memory_limit(mut self, max_bytes: usize) -> Self {
        self.max_memory_bytes = Some(max_bytes);
        self
    }

    /// 开启每 tick fuel 预算。
    pub fn with_fuel_per_tick(mut self, fuel: u64) -> Self {
        self.fuel_per_tick = Some(fuel);
        self
    }
}

/// 宿主侧状态：PDO arena 快照 + 日志 + 资源限制。
///
/// spike 用 `Vec` 保存 arena；W2 会替换为双缓冲/seqlock 实现，字段语义不变。
/// [`StoreLimits`] 由 [`Store::limiter`] 安装，限制插件线性内存/表增长。
#[derive(Debug, Default)]
pub struct HostState {
    pdo: Vec<u8>,
    log: String,
    log_lines: usize,
    log_dropped: u64,
    calls: u64,
    limits: StoreLimits,
}

impl HostState {
    /// 建一个长度为 `pdo_len` 的全零 arena。
    pub fn new(pdo_len: usize) -> Self {
        Self {
            pdo: vec![0; pdo_len],
            ..Self::default()
        }
    }

    /// 建一个 arena，并给线性内存设上限（字节）。
    pub fn with_memory_limit(pdo_len: usize, max_memory_bytes: usize) -> Self {
        Self {
            pdo: vec![0; pdo_len],
            limits: StoreLimitsBuilder::new()
                .memory_size(max_memory_bytes)
                .build(),
            ..Self::default()
        }
    }

    /// 宿主侧资源限制（供 [`Store::limiter`] 借用）。
    pub fn limits_mut(&mut self) -> &mut StoreLimits {
        &mut self.limits
    }

    /// 只读 arena。
    pub fn pdo(&self) -> &[u8] {
        &self.pdo
    }

    /// 可写 arena（宿主侧同步用）。
    pub fn pdo_mut(&mut self) -> &mut [u8] {
        &mut self.pdo
    }

    /// 用 `bytes` 覆盖 arena 的前 `min(len, arena_len)` 字节，其余置 0。
    pub fn set_pdo(&mut self, bytes: &[u8]) {
        self.pdo.iter_mut().for_each(|b| *b = 0);
        let n = bytes.len().min(self.pdo.len());
        self.pdo[..n].copy_from_slice(&bytes[..n]);
    }

    /// 已收集的日志文本。
    pub fn log(&self) -> &str {
        &self.log
    }

    /// 日志行数（每次成功 `log` 计 1 行，不解析换行）。
    pub fn log_lines(&self) -> usize {
        self.log_lines
    }

    /// 被丢弃的日志调用次数（缓冲已满）。
    pub fn log_dropped(&self) -> u64 {
        self.log_dropped
    }

    /// 取走日志（清空缓冲与行数，保留丢弃计数）。
    pub fn take_logs(&mut self) -> String {
        self.log_lines = 0;
        std::mem::take(&mut self.log)
    }

    /// `tick` 成功调用次数。
    pub fn calls(&self) -> u64 {
        self.calls
    }
}

/// 已实例化的 WASM 行为插件。
pub struct WasmPlugin {
    store: Store<HostState>,
    instance: Instance,
    tick: TypedFunc<(), ()>,
    fuel_per_tick: Option<u64>,
}

impl WasmPlugin {
    /// 从 WAT 文本实例化（spike/测试便利路径；生产路径应使用预编译 wasm）。
    pub fn from_wat(text: &str) -> Result<Self, WasmHostError> {
        Self::from_wat_with_config(text, &HostConfig::default())
    }

    /// 从 WAT 文本按 `config` 实例化（可开 fuel / 内存上限）。
    pub fn from_wat_with_config(text: &str, config: &HostConfig) -> Result<Self, WasmHostError> {
        let bytes =
            wat::parse_str(text).map_err(|e| WasmHostError::InvalidModule(e.to_string()))?;
        Self::from_wasm_bytes_with_config(&bytes, config)
    }

    /// 从 wasm 二进制实例化，arena 长度为 `pdo_len`（无 fuel / 内存限制）。
    pub fn from_wasm_bytes(bytes: &[u8], pdo_len: usize) -> Result<Self, WasmHostError> {
        Self::from_wasm_bytes_with_config(bytes, &HostConfig::new(pdo_len))
    }

    /// 从 wasm 二进制按 `config` 实例化。
    ///
    /// 实例化会执行模块的 start 段；导入函数只允许 `env.read_pdo`/`env.write_pdo`/`env.log`，
    /// 未声明的导入会导致实例化失败。`config.max_memory_bytes` 在实例化时即生效
    /// （wasmi 对初始内存分配同样调用 [`ResourceLimiter::memory_growing`]）。
    pub fn from_wasm_bytes_with_config(
        bytes: &[u8],
        config: &HostConfig,
    ) -> Result<Self, WasmHostError> {
        let mut engine_config = Config::default();
        engine_config.consume_fuel(config.fuel_per_tick.is_some());
        let engine = Engine::new(&engine_config);
        let module =
            Module::new(&engine, bytes).map_err(|e| WasmHostError::InvalidModule(e.to_string()))?;
        let state = match config.max_memory_bytes {
            Some(max) => HostState::with_memory_limit(config.pdo_len, max),
            None => HostState::new(config.pdo_len),
        };
        let mut store = Store::new(&engine, state);
        if config.max_memory_bytes.is_some() {
            store.limiter(|state: &mut HostState| state.limits_mut());
        }
        let mut linker = Linker::<HostState>::new(&engine);
        link_imports(&mut linker)?;
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| WasmHostError::InvalidModule(e.to_string()))?
            .start(&mut store)
            .map_err(|e| map_wasmi_error(&e))?;
        let tick = instance
            .get_typed_func::<(), ()>(&store, "tick")
            .map_err(|_| WasmHostError::MissingExport("tick"))?;
        Ok(Self {
            store,
            instance,
            tick,
            fuel_per_tick: config.fuel_per_tick,
        })
    }

    /// 调一次插件 `tick`。宿主侧 PDO arena 在调用前后由宿主同步。
    ///
    /// 若开启 fuel：调用前追加 `fuel_per_tick` 预算；预算耗尽返回
    /// [`WasmHostError::OutOfFuel`]，且该次调用不计入 `calls`。
    pub fn call_tick(&mut self) -> Result<(), WasmHostError> {
        if let Some(fuel) = self.fuel_per_tick {
            self.store
                .add_fuel(fuel)
                .map_err(|e| WasmHostError::Trap(e.to_string()))?;
        }
        self.tick
            .call(&mut self.store, ())
            .map_err(|e| map_trap(&e))?;
        self.store.data_mut().calls += 1;
        Ok(())
    }

    /// 累计消耗的 fuel；未开启 fuel 计量时为 `None`。
    pub fn fuel_consumed(&self) -> Option<u64> {
        self.store.fuel_consumed()
    }

    /// 只读宿主状态。
    pub fn state(&self) -> &HostState {
        self.store.data()
    }

    /// 可写宿主状态（同步 arena / 注入激励）。
    pub fn state_mut(&mut self) -> &mut HostState {
        self.store.data_mut()
    }

    /// 插件 `memory` 导出的大小（字节）；模块没有导出内存时为 `None`。
    pub fn memory_bytes(&self) -> Option<usize> {
        match self.instance.get_export(&self.store, "memory") {
            Some(Extern::Memory(memory)) => Some(memory.data(&self.store).len()),
            _ => None,
        }
    }
}

/// 安装全部允许的导入（`env.read_pdo` / `env.write_pdo` / `env.log`）。
pub(crate) fn link_imports(linker: &mut Linker<HostState>) -> Result<(), WasmHostError> {
    link_pdo_read(linker)?;
    link_pdo_write(linker)?;
    link_log(linker)
}

/// 把 wasmi 错误映射为宿主错误（fuel 耗尽单独识别为 [`WasmHostError::OutOfFuel`]）。
pub(crate) fn map_wasmi_error(err: &wasmi::Error) -> WasmHostError {
    if let wasmi::Error::Trap(trap) = err {
        return map_trap(trap);
    }
    WasmHostError::Trap(err.to_string())
}

/// 把 wasmi trap 映射为宿主错误（fuel 耗尽单独识别）。
pub(crate) fn map_trap(trap: &wasmi::core::Trap) -> WasmHostError {
    if matches!(trap.trap_code(), Some(TrapCode::OutOfFuel)) {
        return WasmHostError::OutOfFuel;
    }
    WasmHostError::Trap(trap.to_string())
}

fn link_pdo_read(linker: &mut Linker<HostState>) -> Result<(), WasmHostError> {
    linker
        .func_wrap(
            "env",
            "read_pdo",
            |caller: Caller<'_, HostState>, offset: i32| -> i32 {
                let Ok(offset) = usize::try_from(offset) else {
                    return -1;
                };
                caller
                    .data()
                    .pdo()
                    .get(offset)
                    .map(|byte| i32::from(*byte))
                    .unwrap_or(-1)
            },
        )
        .map(|_| ())
        .map_err(|e| WasmHostError::InvalidModule(e.to_string()))
}

fn link_pdo_write(linker: &mut Linker<HostState>) -> Result<(), WasmHostError> {
    linker
        .func_wrap(
            "env",
            "write_pdo",
            |mut caller: Caller<'_, HostState>, offset: i32, value: i32| -> i32 {
                let Ok(offset) = usize::try_from(offset) else {
                    return -1;
                };
                match caller.data_mut().pdo_mut().get_mut(offset) {
                    Some(byte) => {
                        *byte = value as u8;
                        0
                    }
                    None => -1,
                }
            },
        )
        .map(|_| ())
        .map_err(|e| WasmHostError::InvalidModule(e.to_string()))
}

fn link_log(linker: &mut Linker<HostState>) -> Result<(), WasmHostError> {
    linker
        .func_wrap(
            "env",
            "log",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                let (Ok(ptr), Ok(len)) = (usize::try_from(ptr), usize::try_from(len)) else {
                    return -1;
                };
                if len > MAX_LOG_CALL {
                    return -1;
                }
                let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                    return -1;
                };
                let mut buffer = vec![0u8; len];
                if memory.read(&caller, ptr, &mut buffer).is_err() {
                    return -1;
                }
                let text = String::from_utf8_lossy(&buffer).into_owned();
                let state = caller.data_mut();
                if state.log.len() + text.len() > MAX_LOG_BYTES {
                    state.log_dropped += 1;
                    return -2;
                }
                state.log.push_str(&text);
                state.log_lines += 1;
                0
            },
        )
        .map(|_| ())
        .map_err(|e| WasmHostError::InvalidModule(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn counter_plugin_writes_input_plus_one_and_logs() {
        let mut plugin = WasmPlugin::from_wat(COUNTER_WAT).expect("instantiate");
        plugin.state_mut().set_pdo(&[41]);
        plugin.call_tick().expect("tick");
        assert_eq!(plugin.state().pdo()[0], 41);
        assert_eq!(plugin.state().pdo()[4], 42);
        assert_eq!(plugin.state().log(), "tick");
        assert_eq!(plugin.state().log_lines(), 1);
        assert_eq!(plugin.state().calls(), 1);
        assert!(plugin.memory_bytes().is_some_and(|n| n >= 65536));
    }

    #[test]
    fn out_of_range_pdo_access_returns_minus_one() {
        let wat = r#"
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (func (export "tick")
    (local $v i32)
    (local.set $v (call $read_pdo (i32.const 1000)))
    (drop (call $write_pdo (i32.const 1) (local.get $v)))
    (drop (call $write_pdo (i32.const 1000) (i32.const 7)))))
"#;
        let mut plugin = WasmPlugin::from_wat(wat).expect("instantiate");
        plugin.call_tick().expect("tick");
        // read_pdo 越界返回 -1，低 8 位写入 arena[1]。
        assert_eq!(plugin.state().pdo()[1], 0xFF);
        // 越界写被静默忽略，不影响 arena。
        assert_eq!(plugin.state().pdo().len(), DEFAULT_PDO_LEN);
    }

    #[test]
    fn missing_tick_export_is_reported() {
        let wat = r#"(module (memory (export "memory") 1))"#;
        match WasmPlugin::from_wat(wat) {
            Ok(_) => panic!("must fail"),
            Err(err) => assert_eq!(err, WasmHostError::MissingExport("tick")),
        }
    }

    #[test]
    fn invalid_wasm_bytes_are_rejected() {
        match WasmPlugin::from_wasm_bytes(b"not a wasm module", 8) {
            Ok(_) => panic!("must fail"),
            Err(err) => assert!(matches!(err, WasmHostError::InvalidModule(_))),
        }
    }

    #[test]
    fn trap_in_tick_is_reported_and_not_counted() {
        let wat = r#"(module (func (export "tick") (unreachable)))"#;
        let mut plugin = WasmPlugin::from_wat(wat).expect("instantiate");
        assert!(matches!(plugin.call_tick(), Err(WasmHostError::Trap(_))));
        assert_eq!(plugin.state().calls(), 0);
    }

    #[test]
    fn oversized_log_call_is_rejected() {
        let wat = r#"
(module
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "tick")
    (drop (call $log (i32.const 0) (i32.const 4096)))))
"#;
        let mut plugin = WasmPlugin::from_wat(wat).expect("instantiate");
        plugin.call_tick().expect("tick");
        assert_eq!(plugin.state().log_lines(), 0);
        assert_eq!(plugin.state().log(), "");
    }

    #[test]
    fn take_logs_resets_buffer_but_keeps_calls() {
        let mut plugin = WasmPlugin::from_wat(COUNTER_WAT).expect("instantiate");
        plugin.state_mut().set_pdo(&[1]);
        plugin.call_tick().expect("tick");
        assert_eq!(plugin.state_mut().take_logs(), "tick");
        assert_eq!(plugin.state().log_lines(), 0);
        assert_eq!(plugin.state().calls(), 1);
    }
}
