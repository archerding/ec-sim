//! 链式帧处理微基准（B16 / B45）：测量 `esc_node::handle_frame_chain` 在 1..N 个从站链上的
//! 单帧处理耗时（不含 TAP/UDS 线缆层与主站侧开销）。
//!
//! 用法：
//!   `cargo run --release -p esc-node --example chain_latency [iterations] [slaves_csv]`
//!
//! 例：`cargo run --release -p esc-node --example chain_latency 20000 1,8,32,64`
//!
//! 目的：为 M1/M3 的「8/32 从站 @1kHz」分析提供**模拟器侧**数据，区分瓶颈来自 ESC 链处理
//! 还是线缆层/主站侧（见 `docs/adr/0001-m1-1khz-p99-limits.md`、
//! `docs/plan/M3-W7-32slave-1khz-bottleneck.md`）。

use std::time::Instant;

use esc_node::{handle_frame_chain, Responder};

const SIM_MAC: [u8; 6] = [0x02, 0xec, 0x51, 0x4d, 0x00, 0x02];

/// 构造一帧：14B 以太网头 + 2B EtherCAT 头 + BRD 读 0x0130（AL Status，2 字节）。
fn broadcast_read_frame() -> Vec<u8> {
    let mut frame = Vec::with_capacity(60);
    frame.extend_from_slice(&[0xff; 6]); // 目的：广播
    frame.extend_from_slice(&SIM_MAC); // 源
    frame.extend_from_slice(&0x88A4u16.to_be_bytes()); // EtherType

    // datagram：cmd=BRD(0x07)、index=1、ADP=0、ADO=0x0130、len=2。
    let datagram_len = 10 + 2 + 2; // 头 + 数据 + WKC
    frame.extend_from_slice(&((1u16 << 12) | datagram_len as u16).to_le_bytes()); // EtherCAT 帧长度 + frame type=1
    frame.push(0x07); // BRD
    frame.push(1); // index
    frame.extend_from_slice(&0u16.to_le_bytes()); // ADP
    frame.extend_from_slice(&0x0130u16.to_le_bytes()); // ADO / 寄存器偏移
    frame.extend_from_slice(&2u16.to_le_bytes()); // 长度（无 M 位）
    frame.extend_from_slice(&0u16.to_le_bytes()); // IRQ
    frame.extend_from_slice(&[0, 0]); // 数据
    frame.extend_from_slice(&0u16.to_le_bytes()); // WKC
    frame
}

fn measure(slaves: usize, iterations: usize) -> f64 {
    let mut responders: Vec<Responder> = (0..slaves).map(|_| Responder::default()).collect();
    let frame = broadcast_read_frame();
    for _ in 0..1000 {
        let _ = handle_frame_chain(&mut responders, &frame, SIM_MAC);
    }
    let started = Instant::now();
    for _ in 0..iterations {
        let reply = handle_frame_chain(&mut responders, &frame, SIM_MAC);
        std::hint::black_box(reply);
    }
    started.elapsed().as_nanos() as f64 / iterations as f64
}

fn main() {
    let mut args = std::env::args().skip(1);
    let iterations: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000);
    let slave_counts: Vec<usize> = args
        .next()
        .map(|csv| {
            csv.split(',')
                .filter_map(|value| value.trim().parse().ok())
                .filter(|value| *value > 0)
                .collect()
        })
        .unwrap_or_else(|| vec![1, 2, 4, 8, 16, 32, 64]);
    if slave_counts.is_empty() {
        eprintln!("error: slaves 列表为空（例：1,8,32,64）");
        std::process::exit(2);
    }

    println!("chain_latency iterations={iterations}");
    let mut previous: Option<(usize, f64)> = None;
    for slaves in slave_counts {
        let ns = measure(slaves, iterations);
        let frames_per_sec = 1.0e9 / ns;
        println!(
            "CHAIN slaves={slaves} ns_per_frame={ns:.1} us_per_frame={:.3} frames_per_sec={frames_per_sec:.0}",
            ns / 1000.0
        );
        if let Some((previous_slaves, previous_ns)) = previous {
            if slaves > previous_slaves {
                let marginal = (ns - previous_ns) / (slaves - previous_slaves) as f64;
                println!(
                    "MARGINAL {}->{} ns_per_added_slave={marginal:.1}",
                    previous_slaves, slaves
                );
            }
        }
        previous = Some((slaves, ns));
    }
}
