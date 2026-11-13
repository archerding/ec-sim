//! WASM 行为插件宿主可运行示例（B43 / M3-W1 spike）。
//!
//! 运行（需要 crates.io 拉取可选 wasmi 依赖）：
//!
//! ```text
//! cargo run -p plugin-host --features wasm-runtime --example wasm_plugin_demo
//! ```
//!
//! 输出为稳定格式（`scripts/spikes/wasm-plugin-probe.sh` 逐行断言），并打印实例化
//! 与单次 tick 的耗时，作为 spike 证据。

use std::time::Instant;

use plugin_host::wasm::{WasmPlugin, DEFAULT_PDO_LEN};

fn hex_slice(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wat = include_str!("../fixtures/wasm/counter.wat");

    let t0 = Instant::now();
    let mut plugin = WasmPlugin::from_wat(wat)?;
    let instantiate_us = t0.elapsed().as_secs_f64() * 1e6;

    let mut initial = vec![0u8; DEFAULT_PDO_LEN];
    initial[0] = 0x29; // SM2 输出快照第 1 字节
    plugin.state_mut().set_pdo(&initial);

    println!(
        "wasm-plugin-demo: pdo_len={} memory_bytes={}",
        plugin.state().pdo().len(),
        plugin.memory_bytes().unwrap_or(0)
    );
    println!("wasm-plugin-demo: instantiate_us={instantiate_us:.1}");
    println!(
        "wasm-plugin-demo: pdo_before={}",
        hex_slice(&plugin.state().pdo()[..8])
    );

    plugin.call_tick()?;
    println!(
        "wasm-plugin-demo: pdo_after={}",
        hex_slice(&plugin.state().pdo()[..8])
    );
    println!("wasm-plugin-demo: log={:?}", plugin.state().log());
    println!(
        "wasm-plugin-demo: log_lines={} calls={}",
        plugin.state().log_lines(),
        plugin.state().calls()
    );

    // 连续 1 万次 tick：验证 arena 写入路径没有隐藏状态，并给出每次调用耗时。
    const TICKS: u32 = 10_000;
    let t1 = Instant::now();
    for i in 0..TICKS {
        plugin.state_mut().pdo_mut()[0] = (i & 0xFF) as u8;
        plugin.call_tick()?;
    }
    let total_ns = t1.elapsed().as_nanos();
    let per_tick_ns = total_ns as f64 / f64::from(TICKS);
    println!(
        "wasm-plugin-demo: ticks={TICKS} total_us={:.1} per_tick_ns={per_tick_ns:.1}",
        total_ns as f64 / 1e3
    );
    println!(
        "wasm-plugin-demo: log_lines={} log_dropped={}",
        plugin.state().log_lines(),
        plugin.state().log_dropped()
    );

    // 不变式：arena[4] == arena[0] + 1，且日志行数 = 1 + TICKS。
    let last = plugin.state().pdo()[0];
    let expected_input = last.wrapping_add(1);
    let invariants_ok = plugin.state().pdo()[4] == expected_input
        && plugin.state().log_lines() == TICKS as usize + 1
        && plugin.state().calls() == u64::from(TICKS) + 1;
    println!(
        "wasm-plugin-demo: invariants={}",
        if invariants_ok { "ok" } else { "FAILED" }
    );
    if !invariants_ok {
        return Err("plugin host invariant violated".into());
    }

    println!("wasm-plugin-demo: OK");
    Ok(())
}
