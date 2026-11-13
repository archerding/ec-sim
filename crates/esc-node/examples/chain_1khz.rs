//! B46：原生 32 站 @ 1 kHz 基准（跨平台；Linux 可选 `SCHED_FIFO`）。
//!
//! 在**单进程内**构建 N 个 `Responder` 组成的链，作为原生 Rust"主站循环"按 1 ms 周期发帧，
//! 统计每轮 **帧处理耗时 / 唤醒延迟 / 周期抖动 / 截止时间超限** 的 p50/p99/max。
//! 这只测**模拟器侧 ESC 链处理 + 本机调度**；不含 TAP/UDS 线缆层（后者见
//! `tap-multi-slave-probe.sh` / `uds_latency`）。**不承诺对外 SLA**，只做能力证据。
//!
//! 用法：
//!   `cargo run --release -p esc-node --example chain_1khz -- [slaves] [seconds] [mode] [shape] [--rt]`
//!   - `mode` ：`hybrid`（默认，sleep 到临界再自旋）| `sleep` | `spin`
//!   - `shape`：`lrw`（默认，1 条 LRW 覆盖全链 PDO，WKC=3N）| `fprd`（每站 1 条）| `single`（1 条 BRD）
//!   - `--rt` ：Linux 上尝试 `SCHED_FIFO` 优先级 80（需 root；验证机用 `taskset -c 2` 绑隔离核）
//!
//! 例：`taskset -c 2 cargo run --release -p esc-node --example chain_1khz -- 32 10 hybrid lrw --rt`
//!
//! 结果记录：`docs/plan/B46-native-32slave-1khz.md`。

use std::time::{Duration, Instant};

use esc_node::{handle_frame_chain, Responder};
use wire::{Command, DatagramData};

const SIM_MAC: [u8; 6] = [0x02, 0xec, 0x51, 0x4d, 0x00, 0x02];
const AL_STATUS: u16 = 0x0130;
const STATION_ADDRESS: u16 = 0x0010;
const LRW_LOGICAL_BASE: u32 = 0x0000_1000;
const LRW_BYTES_PER_SLAVE: usize = 8;
const LRW_OUTPUT_BYTES: usize = 4;
const PERIOD: Duration = Duration::from_micros(1000);

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaitMode {
    Hybrid,
    Sleep,
    Spin,
}

impl WaitMode {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "hybrid" => Some(Self::Hybrid),
            "sleep" => Some(Self::Sleep),
            "spin" => Some(Self::Spin),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::Sleep => "sleep",
            Self::Spin => "spin",
        }
    }
}

fn request(datagrams: &[DatagramData]) -> Vec<u8> {
    let payload = wire::encode_frame(datagrams).expect("encode");
    let mut frame = vec![0u8; wire::ETHERNET_HEADER_LEN];
    frame[0..6].copy_from_slice(&[0xFF; 6]);
    frame[6..12].copy_from_slice(&SIM_MAC);
    wire::set_ethertype(&mut frame, wire::ETHERCAT_ETHERTYPE);
    frame.extend_from_slice(&payload);
    frame
}

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
        Shape::Single | Shape::Fprd => slaves as u16,
        Shape::Lrw => (slaves * 3) as u16,
    }
}

fn reply_wkc(reply: &[u8]) -> u16 {
    wire::Frame::parse_ethernet(reply)
        .map(|frame| frame.datagrams.iter().map(|d| d.working_counter).sum())
        .unwrap_or(0)
}

/// 等待到 `target`（hybrid：远距离 sleep、临界自旋；Windows 计时器粒度差，阈值放大）。
fn wait_until(target: Instant, mode: WaitMode) {
    loop {
        let now = Instant::now();
        if now >= target {
            return;
        }
        let remaining = target - now;
        match mode {
            WaitMode::Spin => std::hint::spin_loop(),
            WaitMode::Sleep => std::thread::sleep(remaining),
            WaitMode::Hybrid => {
                let threshold = if cfg!(target_os = "windows") {
                    Duration::from_millis(20)
                } else {
                    Duration::from_micros(200)
                };
                if remaining > threshold {
                    std::thread::sleep(remaining - threshold);
                } else {
                    std::hint::spin_loop();
                }
            }
        }
    }
}

/// 仅 Linux：对调用线程尝试 `SCHED_FIFO`（不经第三方 crate，直接声明 libc 符号）。
#[cfg(target_os = "linux")]
fn enable_realtime(priority: i32) -> Result<(), String> {
    #[repr(C)]
    struct SchedParam {
        sched_priority: i32,
    }
    extern "C" {
        fn sched_setscheduler(pid: i32, policy: i32, param: *const SchedParam) -> i32;
    }
    let param = SchedParam {
        sched_priority: priority,
    };
    // SAFETY: 布局与 Linux `struct sched_param` 的单字段（sched_priority）一致；
    // pid=0 表示调用线程；SCHED_FIFO = 1。
    let rc = unsafe { sched_setscheduler(0, 1, &param) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

#[cfg(not(target_os = "linux"))]
fn enable_realtime(_priority: i32) -> Result<(), String> {
    Err("仅 Linux 支持 SCHED_FIFO".to_string())
}

/// 返回 (min, p50, p99, max)，单位 ns。
fn stats(values: &[i64]) -> (i64, i64, i64, i64) {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return (0, 0, 0, 0);
    }
    let pick = |q: f64| -> i64 {
        let index = ((sorted.len() - 1) as f64 * q).round() as usize;
        sorted[index]
    };
    (sorted[0], pick(0.50), pick(0.99), sorted[sorted.len() - 1])
}

fn us(ns: f64) -> f64 {
    ns / 1000.0
}

fn main() {
    let mut positional: Vec<String> = Vec::new();
    let mut rt_requested = false;
    for argument in std::env::args().skip(1) {
        if argument == "--rt" {
            rt_requested = true;
        } else {
            positional.push(argument);
        }
    }

    let slaves: usize = positional
        .first()
        .and_then(|value| value.parse().ok())
        .unwrap_or(32);
    let seconds: u64 = positional
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let mode = positional
        .get(2)
        .and_then(|value| WaitMode::parse(value))
        .unwrap_or(WaitMode::Hybrid);
    let shape = positional
        .get(3)
        .and_then(|value| Shape::parse(value))
        .unwrap_or(Shape::Lrw);
    if slaves == 0 || seconds == 0 {
        eprintln!("error: slaves/seconds 必须为正（例：32 5 hybrid lrw）");
        std::process::exit(2);
    }

    let rt = if rt_requested {
        match enable_realtime(80) {
            Ok(()) => "yes".to_string(),
            Err(error) => {
                eprintln!("warning: SCHED_FIFO 启用失败（{error}），继续非实时测量");
                "no".to_string()
            }
        }
    } else {
        "no".to_string()
    };

    let mut responders: Vec<Responder> = (0..slaves).map(|_| Responder::default()).collect();
    match shape {
        Shape::Single => {}
        Shape::Fprd => configure_station_addresses(&mut responders),
        Shape::Lrw => configure_lrw_fmmus(&mut responders),
    }
    let frame = build_frame(slaves, shape);
    let expected = expected_wkc(slaves, shape);

    // 预热（不计入统计）并校验 WKC，保证帧形状真的打中了全链。
    for _ in 0..200 {
        let _ = handle_frame_chain(&mut responders, &frame, SIM_MAC);
    }
    let reply = handle_frame_chain(&mut responders, &frame, SIM_MAC).expect("reply");
    let wkc = reply_wkc(&reply);
    if wkc != expected {
        eprintln!(
            "error: WKC 不符 shape={} slaves={slaves} expected={expected} got={wkc}",
            shape.name()
        );
        std::process::exit(1);
    }

    let rounds = (seconds * 1000) as usize;
    let mut handling: Vec<i64> = Vec::with_capacity(rounds);
    let mut wake: Vec<i64> = Vec::with_capacity(rounds);
    let mut interval_dev: Vec<i64> = Vec::with_capacity(rounds);
    let mut lateness: Vec<i64> = Vec::with_capacity(rounds);
    let mut missed = 0usize;

    let started = Instant::now();
    let mut previous_send: Option<Instant> = None;
    for round in 0..rounds {
        let target = started + PERIOD * round as u32;
        wait_until(target, mode);
        let send = Instant::now();
        let reply = handle_frame_chain(&mut responders, &frame, SIM_MAC).expect("reply");
        let done = Instant::now();
        std::hint::black_box(reply);

        handling.push(done.duration_since(send).as_nanos() as i64);
        wake.push(send.duration_since(target).as_nanos() as i64);
        if let Some(previous) = previous_send {
            interval_dev
                .push(send.duration_since(previous).as_nanos() as i64 - PERIOD.as_nanos() as i64);
        }
        previous_send = Some(send);

        let deadline = started + PERIOD * (round as u32 + 1);
        if done > deadline {
            missed += 1;
            lateness.push(done.duration_since(deadline).as_nanos() as i64);
        } else {
            lateness.push(0);
        }
    }
    let elapsed = started.elapsed();

    let (_, handle_p50, handle_p99, handle_max) = stats(&handling);
    let (_, wake_p50, wake_p99, wake_max) = stats(&wake);
    let (interval_min, interval_p50, interval_p99, interval_max) = stats(&interval_dev);
    let interval_max_abs = interval_max.max(interval_min.saturating_neg());
    let (_, late_p50, late_p99, late_max) = stats(&lateness);

    println!(
        "1KHZ slaves={slaves} seconds={seconds} rounds={rounds} period_us=1000 mode={} shape={} rt={rt} elapsed_ms={}",
        mode.name(),
        shape.name(),
        elapsed.as_millis()
    );
    println!(
        "1KHZ_HANDLING_us p50={:.3} p99={:.3} max={:.3}",
        us(handle_p50 as f64),
        us(handle_p99 as f64),
        us(handle_max as f64)
    );
    println!(
        "1KHZ_WAKE_us p50={:.1} p99={:.1} max={:.1}",
        us(wake_p50 as f64),
        us(wake_p99 as f64),
        us(wake_max as f64)
    );
    println!(
        "1KHZ_INTERVAL_DEV_us p50={:.1} p99={:.1} max_abs={:.1}",
        us(interval_p50 as f64),
        us(interval_p99 as f64),
        us(interval_max_abs as f64)
    );
    println!(
        "1KHZ_LATENESS_us p50={:.1} p99={:.1} max={:.1} missed={missed}/{} ({:.2}%)",
        us(late_p50 as f64),
        us(late_p99 as f64),
        us(late_max as f64),
        rounds,
        100.0 * missed as f64 / rounds as f64
    );
    println!("1KHZ_WKC got={wkc} expected={expected}");
}
