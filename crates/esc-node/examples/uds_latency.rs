//! 原生 UDS shim 往返延迟基准（B17，仅 Linux）：用 Rust 直接收发 EtherCAT 帧，
//! 绕开 Python 合成主站与 SOEM 应用层，测量 `ec-sim-esc --uds` 的往返延迟。
//!
//! 用法：`cargo run --release -p esc-node --example uds_latency -- <socket> [frames]`
//! 前置：另一进程已启动 `ec-sim-esc --uds <socket> [--slaves N]`。
//!
//! 输出：`LATENCY_NATIVE frames=.. min=..us p50=..us p90=..us p99=..us p999=..us max=..us mean=..us`

#[cfg(unix)]
mod unix_impl {
    use std::io;
    use std::os::unix::net::UnixDatagram;
    use std::time::Instant;

    const ETHERCAT_ETHERTYPE: u16 = 0x88A4;

    /// 构造一帧：广播读 AL Status（BRD 0x0130，2 字节）。
    fn broadcast_read_frame() -> Vec<u8> {
        let mut frame = Vec::with_capacity(32);
        frame.extend_from_slice(&[0xff; 6]); // 目的：广播
        frame.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]); // 源 MAC
        frame.extend_from_slice(&ETHERCAT_ETHERTYPE.to_be_bytes());
        let datagram_len = 10 + 2 + 2;
        frame.extend_from_slice(&((1u16 << 12) | datagram_len as u16).to_le_bytes());
        frame.push(0x07); // BRD
        frame.push(1); // index
        frame.extend_from_slice(&0u16.to_le_bytes()); // ADP
        frame.extend_from_slice(&0x0130u16.to_le_bytes()); // ADO / 寄存器偏移
        frame.extend_from_slice(&2u16.to_le_bytes()); // 长度
        frame.extend_from_slice(&0u16.to_le_bytes()); // IRQ
        frame.extend_from_slice(&[0, 0]); // 数据
        frame.extend_from_slice(&0u16.to_le_bytes()); // WKC
        frame
    }

    fn percentiles(sorted: &[u128], q: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let index = ((sorted.len() - 1) as f64 * q).round() as usize;
        sorted[index] as f64 / 1000.0
    }

    pub fn run(socket_path: &str, frames: usize, spin: bool) -> io::Result<()> {
        let client_path = format!("{socket_path}.rustclient{}", std::process::id());
        let _ = std::fs::remove_file(&client_path);
        let sock = UnixDatagram::bind(&client_path)?;
        sock.connect(socket_path)?;
        if spin {
            sock.set_nonblocking(true)?;
        }
        let frame = broadcast_read_frame();
        let mut reply = [0u8; 2048];

        let recv_one = |sock: &UnixDatagram, reply: &mut [u8; 2048]| -> io::Result<usize> {
            if !spin {
                return sock.recv(reply);
            }
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            loop {
                match sock.recv(reply) {
                    Ok(len) => return Ok(len),
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        if Instant::now() > deadline {
                            return Err(io::Error::new(io::ErrorKind::TimedOut, "spin recv 超时"));
                        }
                        std::hint::spin_loop();
                    }
                    Err(err) => return Err(err),
                }
            }
        };

        // 预热
        for _ in 0..200 {
            sock.send(&frame)?;
            recv_one(&sock, &mut reply)?;
        }

        let mut samples = Vec::with_capacity(frames);
        for _ in 0..frames {
            let started = Instant::now();
            sock.send(&frame)?;
            let len = recv_one(&sock, &mut reply)?;
            let elapsed = started.elapsed().as_nanos();
            if len < 14 + 2 + 14 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "短应答"));
            }
            samples.push(elapsed);
        }
        let _ = std::fs::remove_file(&client_path);

        let mut sorted = samples.clone();
        sorted.sort_unstable();
        let mean = samples.iter().sum::<u128>() as f64 / samples.len() as f64 / 1000.0;
        println!(
            "LATENCY_NATIVE frames={} min={:.1}us p50={:.1}us p90={:.1}us p99={:.1}us p999={:.1}us max={:.1}us mean={:.1}us",
            samples.len(),
            sorted[0] as f64 / 1000.0,
            percentiles(&sorted, 0.50),
            percentiles(&sorted, 0.90),
            percentiles(&sorted, 0.99),
            percentiles(&sorted, 0.999),
            sorted[sorted.len() - 1] as f64 / 1000.0,
            mean,
        );
        Ok(())
    }
}

#[cfg(unix)]
fn main() {
    let mut args = std::env::args().skip(1);
    let Some(socket) = args.next() else {
        eprintln!("用法：uds_latency <socket> [frames]");
        std::process::exit(2);
    };
    let frames = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2000);
    let spin = args.next().as_deref() == Some("spin");
    if let Err(err) = unix_impl::run(&socket, frames, spin) {
        eprintln!("uds_latency 失败：{err}");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("uds_latency 仅在 Linux/Unix 上可用（UDS shim）");
}
