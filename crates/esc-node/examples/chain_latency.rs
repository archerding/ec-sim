//! 链式帧处理微基准（B16）：测量 `esc_node::handle_frame_chain` 在 1..N 个从站链上的
//! 单帧处理耗时（不含 TAP/UDS 线缆层与主站侧开销）。
//!
//! 用法：`cargo run --release -p esc-node --example chain_latency [iterations]`
//!
//! 目的：为 M1 的「8 从站 @1kHz / p99」分析提供**模拟器侧**数据，区分瓶颈来自 ESC 链处理
//! 还是线缆层/主站侧（见 `docs/adr/0001-m1-1khz-p99-limits.md`）。

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
    let iterations: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000);
    println!("chain_latency iterations={iterations}");
    for slaves in [1usize, 2, 4, 8] {
        let ns = measure(slaves, iterations);
        println!(
            "CHAIN slaves={slaves} ns_per_frame={ns:.1} frames_per_sec={:.0}",
            1.0e9 / ns
        );
    }
}
