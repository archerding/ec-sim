//! 链式帧处理微基准（B16 / B45 / B46）：测量 `esc_node::handle_frame_chain` 在 1..N 个从站链上的
//! 单帧处理耗时（不含 TAP/UDS 线缆层与主站侧开销），并支持**帧形状**维度。
//!
//! 用法：
//!   `cargo run --release -p esc-node --example chain_latency [iterations] [slaves_csv] [shapes_csv]`
//!
//! 例：`cargo run --release -p esc-node --example chain_latency 20000 1,8,32,64 single,lrw,fprd`
//!
//! 帧形状（B46 补充 M3-W7 §4.1 未测的"多 datagram / PDO 式 LRW"）：
//! - `single`：1 条 BRD 读 AL Status（B45 口径，单 datagram）；
//! - `fprd`  ：**N 条 FPRD**（每站一条，按已配置站地址寻址）——多 datagram 帧最坏形状；
//! - `lrw`   ：1 条 **LRW** 覆盖全链 PDO（每站 FMMU0 映射 4 字节，WKC 期望 `3*N`）——
//!   与 SOEM `simple_ng` 的过程数据帧同形（32 站 WKC 96）。
//!
//! 目的：为 M1/M3 的「8/32 从站 @1kHz」分析提供**模拟器侧**数据，区分瓶颈来自 ESC 链处理
//! 还是线缆层/主站侧（见 `docs/adr/0001-m1-1khz-p99-limits.md`、
//! `docs/plan/M3-W7-32slave-1khz-bottleneck.md`、`docs/plan/B46-native-32slave-1khz.md`）。

use std::time::Instant;

use esc_node::{handle_frame_chain, Responder};
use wire::{Command, DatagramData};

const SIM_MAC: [u8; 6] = [0x02, 0xec, 0x51, 0x4d, 0x00, 0x02];
const AL_STATUS: u16 = 0x0130;
const STATION_ADDRESS: u16 = 0x0010;
const LRW_LOGICAL_BASE: u32 = 0x0000_1000;
/// 每个从站在 LRW 帧里占的字节数：4B 输出（写 FMMU）+ 4B 输入（读 FMMU），与真实主站
/// 「output 区写、input 区读」的布局一致（`simple_ng` 是 1B+1B，这里取 4B 便于观察）。
const LRW_BYTES_PER_SLAVE: usize = 8;
const LRW_OUTPUT_BYTES: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Single,
    Fprd,
    Lrw,
}

impl Shape {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "single" => Some(Self::Single),
            "fprd" => Some(Self::Fprd),
            "lrw" => Some(Self::Lrw),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Fprd => "fprd",
            Self::Lrw => "lrw",
        }
    }
}

fn request(datagrams: &[DatagramData]) -> Vec<u8> {
    let payload = wire::encode_frame(datagrams).expect("encode");
    let mut frame = vec![0u8; wire::ETHERNET_HEADER_LEN];
    frame[0..6].copy_from_slice(&[0xFF; 6]); // 广播目的
    frame[6..12].copy_from_slice(&SIM_MAC); // 模拟器源 MAC
    wire::set_ethertype(&mut frame, wire::ETHERCAT_ETHERTYPE);
    frame.extend_from_slice(&payload);
    frame
}

/// 配置站地址 1..=N（自增寻址 APWR 0x0010；setup 不计入测量）。
fn configure_station_addresses(slaves: &mut [Responder]) {
    for index in 0..slaves.len() {
        let frame = request(&[DatagramData::auto_increment(
            Command::Apwr,
            1,
            0u16.wrapping_sub(index as u16),
            STATION_ADDRESS,
            ((index + 1) as u16).to_le_bytes().to_vec(),
        )]);
        let _ = handle_frame_chain(slaves, &frame, SIM_MAC);
    }
}

/// 为每站配置两个 FMMU（与真实主站一致）：FMMU0 = 输出区（写方向），FMMU1 = 输入区（读方向）。
/// 逻辑 `0x1000+8i..+4`（输出）、`+4..+4`（输入）←→ 物理同址；LRW 覆盖全部输出+输入区。
fn configure_lrw_fmmus(slaves: &mut [Responder]) {
    for index in 0..slaves.len() {
        let offset = (index * LRW_BYTES_PER_SLAVE) as u32;
        for (fmmu_index, logical, physical, fmmu_type) in [
            (0u8, LRW_LOGICAL_BASE + offset, 0x1000 + offset, 0x02u8),
            (
                1u8,
                LRW_LOGICAL_BASE + offset + LRW_OUTPUT_BYTES as u32,
                0x1000 + offset + LRW_OUTPUT_BYTES as u32,
                0x01u8,
            ),
        ] {
            let page = esc_core::fmmu::Fmmu {
                logical_start_address: logical,
                length: LRW_OUTPUT_BYTES as u16,
                logical_start_bit: 0,
                logical_end_bit: 7,
                physical_start_address: physical as u16,
                physical_start_bit: 0,
                fmmu_type,
                activate: 0x01,
            }
            .to_bytes();
            let frame = request(&[DatagramData::auto_increment(
                Command::Apwr,
                1,
                0u16.wrapping_sub(index as u16),
                esc_core::fmmu::fmmu_address(fmmu_index).expect("FMMU 在范围内"),
                page.to_vec(),
            )]);
            let _ = handle_frame_chain(slaves, &frame, SIM_MAC);
        }
    }
}

fn build_frame(slaves: usize, shape: Shape) -> Vec<u8> {
    match shape {
        Shape::Single => request(&[DatagramData::auto_increment(
            Command::Brd,
            1,
            0,
            AL_STATUS,
            vec![0u8; 2],
        )]),
        Shape::Fprd => {
            let datagrams: Vec<DatagramData> = (0..slaves)
                .map(|index| {
                    DatagramData::auto_increment(
                        Command::Fprd,
                        1,
                        (index + 1) as u16,
                        AL_STATUS,
                        vec![0u8; 2],
                    )
                })
                .collect();
            request(&datagrams)
        }
        Shape::Lrw => request(&[DatagramData::logical(
            Command::Lrw,
            1,
            LRW_LOGICAL_BASE,
            vec![0u8; slaves * LRW_BYTES_PER_SLAVE],
        )]),
    }
}

fn expected_wkc(slaves: usize, shape: Shape) -> u16 {
    match shape {
        // BRD 读：每站 +1；FPRD：每条 datagram 命中 1 站 +1；LRW：每站读 +1、写 +2。
        Shape::Single | Shape::Fprd => slaves as u16,
        Shape::Lrw => (slaves * 3) as u16,
    }
}

fn reply_wkc(reply: &[u8]) -> u16 {
    wire::Frame::parse_ethernet(reply)
        .map(|frame| frame.datagrams.iter().map(|d| d.working_counter).sum())
        .unwrap_or(0)
}

fn measure(slaves: usize, iterations: usize, shape: Shape) -> (f64, u16) {
    let mut responders: Vec<Responder> = (0..slaves).map(|_| Responder::default()).collect();
    match shape {
        Shape::Single => {}
        Shape::Fprd => configure_station_addresses(&mut responders),
        Shape::Lrw => configure_lrw_fmmus(&mut responders),
    }
    let frame = build_frame(slaves, shape);
    let expected = expected_wkc(slaves, shape);

    for _ in 0..1000 {
        let _ = handle_frame_chain(&mut responders, &frame, SIM_MAC);
    }
    let reply = handle_frame_chain(&mut responders, &frame, SIM_MAC).expect("reply");
    let wkc = reply_wkc(&reply);

    let started = Instant::now();
    for _ in 0..iterations {
        let reply = handle_frame_chain(&mut responders, &frame, SIM_MAC);
        std::hint::black_box(reply);
    }
    let ns = started.elapsed().as_nanos() as f64 / iterations as f64;
    assert!(
        wkc == expected,
        "WKC 不符：slaves={slaves} shape={} expected={expected} got={wkc}",
        shape.name()
    );
    (ns, wkc)
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
    let shapes: Vec<Shape> = args
        .next()
        .map(|csv| {
            csv.split(',')
                .filter_map(|value| Shape::parse(value.trim()))
                .collect()
        })
        .unwrap_or_else(|| vec![Shape::Single, Shape::Lrw, Shape::Fprd]);
    if slave_counts.is_empty() {
        eprintln!("error: slaves 列表为空（例：1,8,32,64）");
        std::process::exit(2);
    }
    if shapes.is_empty() {
        eprintln!("error: shapes 列表为空（可选 single,lrw,fprd）");
        std::process::exit(2);
    }

    println!("chain_latency iterations={iterations}");
    for shape in shapes {
        let mut previous: Option<(usize, f64)> = None;
        for slaves in &slave_counts {
            let (ns, wkc) = measure(*slaves, iterations, shape);
            println!(
                "CHAIN shape={} slaves={slaves} ns_per_frame={ns:.1} us_per_frame={:.3} frames_per_sec={:.0} wkc={wkc}",
                shape.name(),
                ns / 1000.0,
                1.0e9 / ns
            );
            if let Some((previous_slaves, previous_ns)) = previous {
                if *slaves > previous_slaves {
                    let marginal = (ns - previous_ns) / (*slaves - previous_slaves) as f64;
                    println!(
                        "MARGINAL shape={} {}->{} ns_per_added_slave={marginal:.1}",
                        shape.name(),
                        previous_slaves,
                        slaves
                    );
                }
            }
            previous = Some((*slaves, ns));
        }
    }
}
