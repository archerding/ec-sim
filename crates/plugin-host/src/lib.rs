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

/// PDO arena 同步（M3-W2 子项 1，B44）：实时环 → 插件 worker 的双缓冲/seqlock 快照。
///
/// 默认编译、零第三方依赖；不含 `unsafe`。写者（实时环）发布不分配内存、
/// 不等待读者，读者（插件 worker）拿到的 SM2/SM3 永远来自同一次发布。
pub mod arena;
pub mod dict_gen;
pub mod esi;
pub mod manifest;
pub mod sii_gen;

/// WASM 行为插件宿主（B43 / M3-W1 spike）。
///
/// 仅在 `wasm-runtime` feature 下编译（默认关闭）：依赖 wasmi 0.31（MIT），
/// 提供 `env.read_pdo` / `env.write_pdo` / `env.log` 三个导入与 `tick` 导出，
/// 插件只看到 PDO arena 快照、不进入实时环。选型与实测证据见
/// `docs/spikes/spike-6-wasm-host.md`；arena 同步与 fuel/超时属 W2 后续工作。
#[cfg(feature = "wasm-runtime")]
pub mod wasm;

/// WASM 插件**独占 worker 线程**（B44 子项 2 / M3-W2）。
///
/// 仅在 `wasm-runtime` feature 下编译。把插件实例整体搬进独立线程：宿主投递
/// PDO arena 快照、有界等待结果；插件死循环/崩溃由 wasmi fuel 与内存上限兜底，
/// 宿主线程**永不阻塞**。见 [`wasm_worker`] 模块文档与「已知限制」。
#[cfg(feature = "wasm-runtime")]
pub mod wasm_worker;

/// WASM 行为插件的**非实时运行时**（B44 子项 3 / M3-W2）。
///
/// 仅 `wasm-runtime` feature：宿主线程只 publish arena / try-poll 输出，
/// 插件编译/实例化/tick 全在独立 ticker 线程，宿主永不阻塞在插件上。
#[cfg(feature = "wasm-runtime")]
pub mod behavior;

/// 设备插件的最小契约。
///
/// 真实实现由声明式 `plugin.yaml` 生成（或 WASM 导出），宿主按此 trait 分发；
/// 插件侧不得直接触碰实时环或线缆层。
pub trait DevicePlugin {
    /// 插件自报名，必须带 `SIMULATED` 标识（R2）。
    fn name(&self) -> &str;
}
