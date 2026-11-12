//! 插件宿主。
//!
//! 约束（AGENTS.md §1 / docs/DEVICE-PLUGINS.md）：
//! - 插件**永不进入实时环**；PDO 数据契约放在核心持有的共享内存 arena。
//! - 优先**声明式**插件（`plugin.yaml` + `facts.md`）；只有行为逻辑才写 WASM。
//! - 身份只用自有 VID/PID，名称必须带 `SIMULATED`（R2）；不得声称兼容厂商产品（R3）。
//!
//! 本班次落地 B9 子项 1（2026-11-12）：[`manifest`] 解析 `plugin.yaml`（零第三方依赖的
//! YAML 子集解析器 + R2/R5 校验），[`sii_gen`] 把 manifest 的身份/邮箱/SM/PDO 声明转成
//! [`esc_core::sii::SiiSpec`]，再用 `SiiImage::from_spec()` 生成可被 IgH/SOEM 解析的 SII 镜像。
//! [`dict_gen`] 由 manifest 生成运行时对象字典（身份/SM 类型/PDO 分配与映射/用户对象）。
//! [`esi`] 提供 **ESI XML → SII** 的解析子集（B6 子项，零第三方依赖的最小 XML 解析器）。

pub mod dict_gen;
pub mod esi;
pub mod manifest;
pub mod sii_gen;

/// 设备插件的最小契约。
///
/// 真实实现由声明式 `plugin.yaml` 生成（或 WASM 导出），宿主按此 trait 分发；
/// 插件侧不得直接触碰实时环或线缆层。
pub trait DevicePlugin {
    /// 插件自报名，必须带 `SIMULATED` 标识（R2）。
    fn name(&self) -> &str;
}
