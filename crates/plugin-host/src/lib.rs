//! 插件宿主。
//!
//! 约束（AGENTS.md §1 / docs/DEVICE-PLUGINS.md）：
//! - 插件**永不进入实时环**；PDO 数据契约放在核心持有的共享内存 arena。
//! - 优先**声明式**插件（`plugin.yaml` + `facts.md`）；只有行为逻辑才写 WASM。
//! - 身份只用自有 VID/PID，名称必须带 `SIMULATED`（R2）；不得声称兼容厂商产品，
//!   只能写 "compatible with ...（独立实现）"（R3）。
//!
//! B1 先固化"插件"的最小契约；加载器、yaml 解析与 WASM 沙箱在 B2b/B3 之后落地。

/// 设备插件的最小契约。
///
/// 真实实现由声明式 `plugin.yaml` 生成（或 WASM 导出），宿主按此 trait 分发；
/// 插件侧不得直接触碰实时环或线缆层。
pub trait DevicePlugin {
    /// 插件自报名称，必须带 `SIMULATED` 标识（R2）。
    fn name(&self) -> &str;
}
