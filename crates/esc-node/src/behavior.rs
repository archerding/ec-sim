//! WASM 行为插件的**非实时桥**（B44 子项 3 / M3-W2）。
//!
//! 把 `plugin_host::behavior::BehaviorRuntime` 接到 ESC 应答器的过程数据区：
//! 每个帧周期宿主线程只做三件有界的事——
//! 1. 把 SM2/SM3 拷到固定缓冲（无堆分配）；
//! 2. `publish` 到 PDO arena（不分配、不等待）；
//! 3. `poll_output` 取回已算好的 SM3 并写回 DL 用户内存（`try_lock`，无结果就跳过）。
//!
//! 插件的编译/实例化/tick 全部发生在 `plugin-host` 的独立 ticker 线程，
//! 因此**插件永不进入实时环**（AGENTS §1 核心约束）。

use crate::Responder;
use plugin_host::behavior::{BehaviorQuota, BehaviorRuntime, BehaviorSpec, BehaviorStats};
use plugin_host::manifest::PluginManifest;
use plugin_host::wasm_worker::WorkerLimits;
use std::path::Path;
use std::time::Duration;

/// 行为桥的 SM 布局与限流参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorBridgeConfig {
    /// SM2（RxPDO，主站 → 从站）起始地址。
    pub sm2_address: u16,
    /// SM2 长度（字节，同时是插件可见映像前段容量）。
    pub sm2_len: usize,
    /// SM3（TxPDO，从站 → 主站）起始地址。
    pub sm3_address: u16,
    /// SM3 长度（字节）。
    pub sm3_len: usize,
    /// 目标 tick 频率（Hz）。
    pub tick_hz: u32,
    /// 宿主等待单次 tick 的超时（毫秒）。
    pub tick_timeout_ms: u64,
    /// 每 tick fuel 预算（`None` = 默认）。
    pub fuel_per_tick: Option<u64>,
    /// 插件线性内存上限（字节；`None` = 默认）。
    pub max_memory_bytes: Option<usize>,
}

impl BehaviorBridgeConfig {
    /// 由 manifest 的 `behavior:` 声明 + SM 布局（dir out/in）推导桥参数。
    pub fn from_manifest(manifest: &PluginManifest) -> Option<Self> {
        let behavior = manifest.behavior.as_ref()?;
        let (sm2_address, sm2_len) = manifest
            .sm
            .iter()
            .find(|sm| sm.dir == "out")
            .map(|sm| (sm.start, usize::from(sm.len)))
            .unwrap_or((0x1100, 2));
        let (sm3_address, sm3_len) = manifest
            .sm
            .iter()
            .find(|sm| sm.dir == "in")
            .map(|sm| (sm.start, usize::from(sm.len)))
            .unwrap_or((0x1110, 2));
        Some(Self {
            sm2_address,
            sm2_len: sm2_len.max(1),
            sm3_address,
            sm3_len: sm3_len.max(1),
            tick_hz: behavior.tick_hz,
            tick_timeout_ms: behavior.tick_timeout_ms,
            fuel_per_tick: behavior.fuel_per_tick,
            max_memory_bytes: behavior.max_memory_bytes,
        })
    }

    fn spec(&self) -> BehaviorSpec {
        let mut limits = WorkerLimits::new(self.sm2_len, self.sm3_len);
        if let Some(fuel) = self.fuel_per_tick {
            limits = limits.with_fuel_per_tick(Some(fuel));
        }
        if let Some(bytes) = self.max_memory_bytes {
            limits = limits.with_memory_limit(Some(bytes));
        }
        BehaviorSpec {
            tick_hz: self.tick_hz,
            tick_timeout: Duration::from_millis(self.tick_timeout_ms.max(1)),
            limits,
            max_restarts: plugin_host::behavior::DEFAULT_MAX_RESTARTS,
            restart_after_consecutive_errors: plugin_host::behavior::DEFAULT_RESTART_AFTER_ERRORS,
        }
    }
}

/// 行为桥：宿主侧句柄（内部持有 ticker 线程与 arena）。
pub struct BehaviorBridge {
    runtime: BehaviorRuntime,
    config: BehaviorBridgeConfig,
    sm2: Vec<u8>,
    sm3: Vec<u8>,
    published_frames: u64,
    applied_generations: u64,
}

impl BehaviorBridge {
    /// 从 WAT 文本启动（测试/内置画像用）。
    pub fn spawn_wat(text: &str, config: BehaviorBridgeConfig) -> Result<Self, String> {
        Self::spawn_wat_with_quota(text, config, None)
    }

    /// 从 WAT 文本启动，并占用一个实例配额（多从站/多实例接线用）。
    pub fn spawn_wat_with_quota(
        text: &str,
        config: BehaviorBridgeConfig,
        quota: Option<&BehaviorQuota>,
    ) -> Result<Self, String> {
        let runtime = BehaviorRuntime::spawn_wat_with_quota(text, config.spec(), quota)?;
        Ok(Self::wrap(runtime, config))
    }

    /// 从 wasm 二进制启动。
    pub fn spawn_wasm_bytes(bytes: &[u8], config: BehaviorBridgeConfig) -> Result<Self, String> {
        Self::spawn_wasm_bytes_with_quota(bytes, config, None)
    }

    /// 从 wasm 二进制启动，并占用一个实例配额（多从站/多实例接线用）。
    pub fn spawn_wasm_bytes_with_quota(
        bytes: &[u8],
        config: BehaviorBridgeConfig,
        quota: Option<&BehaviorQuota>,
    ) -> Result<Self, String> {
        let runtime = BehaviorRuntime::spawn_wasm_bytes_with_quota(bytes, config.spec(), quota)?;
        Ok(Self::wrap(runtime, config))
    }

    /// 按 manifest 的 `behavior:` 声明读取代码文件并启动（`.wat` 文本或 `.wasm` 二进制）。
    ///
    /// `manifest_path` 用于把 `behavior.code` 解析成 manifest 目录内的相对路径。
    pub fn from_manifest(
        manifest: &PluginManifest,
        manifest_path: impl AsRef<Path>,
    ) -> Result<Option<Self>, String> {
        Self::from_manifest_with_quota(manifest, manifest_path, None)
    }

    /// 同 [`Self::from_manifest`]，并占用一个实例配额（B51 配额在多实例里的接线）。
    pub fn from_manifest_with_quota(
        manifest: &PluginManifest,
        manifest_path: impl AsRef<Path>,
        quota: Option<&BehaviorQuota>,
    ) -> Result<Option<Self>, String> {
        let Some(behavior) = manifest.behavior.as_ref() else {
            return Ok(None);
        };
        let config = BehaviorBridgeConfig::from_manifest(manifest)
            .ok_or_else(|| "manifest 缺少 behavior 段".to_string())?;
        let code_path = behavior.code_path(manifest_path)?;
        let is_wat = code_path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wat"));
        if is_wat {
            let text = std::fs::read_to_string(&code_path)
                .map_err(|error| format!("读取行为插件 {} 失败：{error}", code_path.display()))?;
            Self::spawn_wat_with_quota(&text, config, quota).map(Some)
        } else {
            let bytes = std::fs::read(&code_path)
                .map_err(|error| format!("读取行为插件 {} 失败：{error}", code_path.display()))?;
            Self::spawn_wasm_bytes_with_quota(&bytes, config, quota).map(Some)
        }
    }

    fn wrap(runtime: BehaviorRuntime, config: BehaviorBridgeConfig) -> Self {
        Self {
            runtime,
            sm2: vec![0u8; config.sm2_len],
            sm3: vec![0u8; config.sm3_len],
            config,
            published_frames: 0,
            applied_generations: 0,
        }
    }

    /// 帧周期调用：发布 SM2/SM3 快照 → 非阻塞取回插件输出并写回 SM3。
    ///
    /// 全程不等待插件、不做堆分配（`Vec` 容量固定）。
    pub fn on_frame(&mut self, responder: &mut Responder) {
        let _ = responder.copy_process_data(self.config.sm2_address, &mut self.sm2);
        let _ = responder.copy_process_data(self.config.sm3_address, &mut self.sm3);
        if self.runtime.publish(&self.sm2, &self.sm3).is_ok() {
            self.published_frames += 1;
        }
        if let Some(output) = self.runtime.poll_output() {
            let len = output.sm3.len().min(self.config.sm3_len);
            if responder.write_process_data(self.config.sm3_address, &output.sm3[..len]) {
                self.applied_generations += 1;
            }
        }
    }

    /// 已发布帧数。
    pub fn published_frames(&self) -> u64 {
        self.published_frames
    }

    /// 已写回 SM3 的插件结果数。
    pub fn applied_generations(&self) -> u64 {
        self.applied_generations
    }

    /// 插件运行时统计。
    pub fn stats(&self) -> BehaviorStats {
        self.runtime.stats()
    }

    /// SM 布局（用于日志）。
    pub fn config(&self) -> &BehaviorBridgeConfig {
        &self.config
    }
}

/// 多从站行为桥集合（B51 后续：把实例配额接到 `esc-node` 的多从站链）。
///
/// `--slaves N` 时每个从站一个插件实例（各自独立 arena/worker/ticker），
/// 全部实例共享**同一个** [`BehaviorQuota`]，因此"一个进程最多跑几个插件实例"
/// 是显式、可观测的；配额上限默认等于从站数（每个从站恰一个实例），
/// 也可由 `--behavior-max-instances` 收紧以验证超限拒绝路径。
pub struct BehaviorBridges {
    quota: BehaviorQuota,
    bridges: Vec<BehaviorBridge>,
}

impl BehaviorBridges {
    /// 为 `instances` 个从站各启动一个插件实例（配额 = instances）。
    pub fn from_manifest(
        manifest: &PluginManifest,
        manifest_path: impl AsRef<Path>,
        instances: usize,
    ) -> Result<Option<Self>, String> {
        Self::from_manifest_with_quota(manifest, manifest_path, instances, None)
    }

    /// 同 [`Self::from_manifest`]，可显式传入共享配额（`None` = 按 instances 新建）。
    ///
    /// 任一次实例化失败即返回错误（已启动的实例随 `Vec` 丢弃自动释放配额名额）。
    pub fn from_manifest_with_quota(
        manifest: &PluginManifest,
        manifest_path: impl AsRef<Path>,
        instances: usize,
        quota: Option<BehaviorQuota>,
    ) -> Result<Option<Self>, String> {
        let instances = instances.max(1);
        if manifest.behavior.is_none() {
            return Ok(None);
        }
        let quota = quota.unwrap_or_else(|| BehaviorQuota::new(instances));
        let mut bridges = Vec::with_capacity(instances);
        for index in 0..instances {
            let bridge = BehaviorBridge::from_manifest_with_quota(
                manifest,
                manifest_path.as_ref(),
                Some(&quota),
            )?
            .ok_or_else(|| "manifest 缺少 behavior 段".to_string())
            .map_err(|error| format!("第 {index} 个行为插件实例启动失败：{error}"))?;
            bridges.push(bridge);
        }
        Ok(Some(Self { quota, bridges }))
    }

    /// 帧周期调用：第 i 个从站的过程数据交给第 i 个插件实例（多出来的从站跳过）。
    ///
    /// 每个 `on_frame` 都不等待插件（见 [`BehaviorBridge::on_frame`]），
    /// 因此 N 个实例也不会把插件耗时带进帧循环。
    pub fn on_frame(&mut self, responders: &mut [Responder]) {
        for (bridge, responder) in self.bridges.iter_mut().zip(responders.iter_mut()) {
            bridge.on_frame(responder);
        }
    }

    /// 实例配额（`max` 上限 / `active` 当前占用）。
    pub fn quota(&self) -> &BehaviorQuota {
        &self.quota
    }

    /// 已启动的插件实例数。
    pub fn instances(&self) -> usize {
        self.bridges.len()
    }

    /// 各实例统计（顺序同从站链）。
    pub fn stats(&self) -> Vec<BehaviorStats> {
        self.bridges.iter().map(BehaviorBridge::stats).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// SM2 容量 2 / SM3 容量 2：SM3[0] = SM2[0] + 1。
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
        (drop (call $write_pdo (i32.const 2) (i32.add (local.get $v) (i32.const 1))))))))
"#;

    fn config() -> BehaviorBridgeConfig {
        BehaviorBridgeConfig {
            sm2_address: 0x1100,
            sm2_len: 2,
            sm3_address: 0x1110,
            sm3_len: 2,
            tick_hz: 2000,
            tick_timeout_ms: 200,
            fuel_per_tick: Some(10_000_000),
            max_memory_bytes: None,
        }
    }

    #[test]
    fn bridge_writes_plugin_sm3_without_blocking_frame_loop() {
        let mut responder = Responder::default();
        assert!(responder.write_process_data(0x1100, &[41, 0]));
        let mut bridge = BehaviorBridge::spawn_wat(COUNTER_WAT, config()).expect("spawn");
        let mut worst_frame = std::time::Duration::ZERO;
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let started = Instant::now();
            bridge.on_frame(&mut responder);
            worst_frame = worst_frame.max(started.elapsed());
            if responder.read_process_data(0x1110, 1)[0] == 42 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "3s 内应把插件输出写回 SM3：{:?}",
                bridge.stats()
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            worst_frame < std::time::Duration::from_millis(50),
            "单帧耗时 {worst_frame:?} 不应等待插件"
        );
        assert!(bridge.published_frames() > 0);
        assert_eq!(bridge.applied_generations(), 1);
        // SM2 只读快照不应被插件改写。
        assert_eq!(responder.read_process_data(0x1100, 2), vec![41, 0]);
        assert_eq!(bridge.stats().ticks, 1, "{:?}", bridge.stats());
    }

    #[test]
    fn from_manifest_returns_none_without_behavior() {
        let manifest = PluginManifest::parse(
            "schema: ecsim.plugin/v1\nid: com.ecsim.x\nname: \"X (SIMULATED)\"\nversion: 1\nkind: device\n\
             identity:\n  vendor_id: 1\n  product_code: 1\n  simulated: true\n\
             sm:\n  - {index: 0, dir: mbox-in, start: 0x1000, len: 128}\n\
             sources:\n  - {title: t, url: \"https://example.invalid\", accessed: \"2026-11-13\"}\n",
        )
        .expect("parse");
        assert!(
            BehaviorBridge::from_manifest(&manifest, "plugins/x/plugin.yaml")
                .expect("no behavior")
                .is_none()
        );
    }

    /// B51 后续：两个从站各一个插件实例，共享配额；配额满时第三个实例被拒绝，
    /// 且每个从站的 SM3 只由自己的实例写回（互不串扰）。
    #[test]
    fn multi_instance_bridges_share_quota_and_apply_per_slave() {
        let mut responders = vec![Responder::default(), Responder::default()];
        assert!(responders[0].write_process_data(0x1100, &[1, 0]));
        assert!(responders[1].write_process_data(0x1100, &[41, 0]));

        let quota = BehaviorQuota::new(2);
        let first = BehaviorBridge::spawn_wat_with_quota(COUNTER_WAT, config(), Some(&quota))
            .expect("第 1 个实例");
        let second = BehaviorBridge::spawn_wat_with_quota(COUNTER_WAT, config(), Some(&quota))
            .expect("第 2 个实例");
        assert_eq!(quota.active(), 2, "两个实例应各占一个名额");
        let error = match BehaviorBridge::spawn_wat_with_quota(COUNTER_WAT, config(), Some(&quota))
        {
            Ok(_) => panic!("配额已满时第 3 个实例应被拒绝"),
            Err(error) => error,
        };
        assert!(error.contains("配额"), "错误应说明配额已满：{error}");
        assert_eq!(quota.active(), 2, "被拒绝的实例不得占用名额");

        let mut bridges = BehaviorBridges {
            quota: quota.clone(),
            bridges: vec![first, second],
        };
        assert_eq!(bridges.instances(), 2);
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            bridges.on_frame(&mut responders);
            let low = responders[0].read_process_data(0x1110, 1)[0];
            let high = responders[1].read_process_data(0x1110, 1)[0];
            if low == 2 && high == 42 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "两个从站应各自得到 SM2+1（当前 {low} / {high}）：{:?}",
                bridges.stats()
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // 每个实例只写自己的从站：SM2 不应被改写。
        assert_eq!(responders[0].read_process_data(0x1100, 1), vec![1]);
        assert_eq!(responders[1].read_process_data(0x1100, 1), vec![41]);
        let stats = bridges.stats();
        assert_eq!(stats.len(), 2);
        assert!(stats.iter().all(|entry| entry.ticks >= 1), "{stats:?}");
        assert_eq!(bridges.quota().active(), 2);
    }

    /// 仓库内真实画像：按从站数启动 N 个实例；配额收紧时启动明确失败。
    #[test]
    fn from_manifest_spawns_one_instance_per_slave_and_rejects_quota_overflow() {
        let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/behavior-counter/plugin.yaml");
        let manifest = PluginManifest::load(&manifest_path).expect("加载 behavior-counter");

        let bridges = BehaviorBridges::from_manifest(&manifest, &manifest_path, 3)
            .expect("启动 3 个实例")
            .expect("manifest 声明了 behavior");
        assert_eq!(bridges.instances(), 3);
        assert_eq!(bridges.quota().max(), 3);
        assert_eq!(bridges.quota().active(), 3);
        assert_eq!(bridges.stats().len(), 3);

        // 配额上限 1 却要求 2 个实例 → 第二个实例被拒绝（错误链里带配额信息）。
        let error = match BehaviorBridges::from_manifest_with_quota(
            &manifest,
            &manifest_path,
            2,
            Some(BehaviorQuota::new(1)),
        ) {
            Ok(_) => panic!("配额不足时应返回错误"),
            Err(error) => error,
        };
        assert!(error.contains("配额"), "错误应说明配额已满：{error}");
    }
}
