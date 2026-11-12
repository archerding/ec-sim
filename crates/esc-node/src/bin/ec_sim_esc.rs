//! `ec-sim-esc`：在 Linux TAP 上运行的最小 ESC 从站应答器。
//!
//! 用法（需 root 或 CAP_NET_ADMIN）：
//! ```text
//! ec-sim-esc --tap escsim0 [--verbose]
//! ```
//! 进程自己创建/附着 TAP 接口并持有 fd（TAP 无持有者时为 NO-CARRIER，主站发不出帧），
//! 随后由外部脚本把接口 MAC 设为 IgH `main_devices` 使用的 MAC 并 `ip link set up`。
//!
//! 仅 Linux 有 TAP；其他平台编译为可执行但直接报错退出（单元测试仍可在所有平台跑）。

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let mut tap_name = String::from("ecsim0");
    let mut verbose = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tap" => match args.next() {
                Some(name) => tap_name = name,
                None => {
                    eprintln!("ec-sim-esc: --tap 需要一个接口名");
                    return std::process::ExitCode::from(2);
                }
            },
            "--verbose" | "-v" => verbose = true,
            "--help" | "-h" => {
                println!("用法: ec-sim-esc [--tap <name>] [--verbose]");
                return std::process::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("ec-sim-esc: 未知参数 {other}");
                return std::process::ExitCode::from(2);
            }
        }
    }

    if !cfg!(target_os = "linux") {
        eprintln!("ec-sim-esc: TAP 链路仅在 Linux 可用（当前平台不支持）");
        return std::process::ExitCode::from(3);
    }

    run(&tap_name, verbose)
}

#[cfg(target_os = "linux")]
fn run(tap_name: &str, verbose: bool) -> std::process::ExitCode {
    use esc_node::{Responder, DEFAULT_SIM_MAC};
    use std::io::Write;

    let mut tap = match tap::Tap::open(tap_name) {
        Ok(tap) => tap,
        Err(err) => {
            eprintln!("ec-sim-esc: 打开 TAP {tap_name} 失败: {err}");
            return std::process::ExitCode::from(4);
        }
    };
    let mac = tap.mac;
    println!(
        "READY tap={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        tap.name, mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    let _ = std::io::stdout().flush();

    let mut responder = Responder::default();
    let mut buf = [0u8; 2048];
    loop {
        let len = match tap.recv(&mut buf) {
            Ok(0) => continue,
            Ok(len) => len,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => {
                eprintln!("ec-sim-esc: 读取 TAP 失败: {err}");
                return std::process::ExitCode::from(5);
            }
        };
        let Some(reply) = responder.handle_frame(&buf[..len], mac) else {
            continue;
        };
        if let Err(err) = tap.send(&reply) {
            eprintln!("ec-sim-esc: 写回 TAP 失败: {err}");
            return std::process::ExitCode::from(6);
        }
        let stats = responder.stats();
        if verbose && stats.frames_in % 50 == 0 {
            eprintln!(
                "STATS in={} out={} dropped={} hits={} al=0x{:02x} station=0x{:04x}",
                stats.frames_in,
                stats.frames_out,
                stats.frames_dropped,
                stats.datagrams_hit,
                responder.al_status(),
                responder.station_address()
            );
        }
        let _ = DEFAULT_SIM_MAC;
    }
}

#[cfg(not(target_os = "linux"))]
fn run(_tap_name: &str, _verbose: bool) -> std::process::ExitCode {
    eprintln!("ec-sim-esc: TAP 链路仅在 Linux 可用（当前平台不支持）");
    std::process::ExitCode::from(3)
}

#[cfg(target_os = "linux")]
mod tap {
    //! 最小的 Linux TAP 封装：不引入第三方依赖，只声明用到的 libc 符号与常量。

    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::unix::io::AsRawFd;

    /// `struct ifreq` 的 x86_64 布局（40 字节）：16 字节接口名 + flags。
    #[repr(C)]
    struct IfReq {
        name: [u8; 16],
        flags: i16,
        _pad: [u8; 22],
    }

    extern "C" {
        fn ioctl(fd: i32, request: u64, ...) -> i32;
    }

    /// `_IOW('T', 202, int)`：创建/附着 TAP 接口。
    const TUNSETIFF: u64 = 0x4004_54ca;
    const IFF_TAP: i16 = 0x0002;
    const IFF_NO_PI: i16 = 0x1000;
    /// 未指定 MAC 时的占位（真实 MAC 从 sysfs 读取）。
    const FALLBACK_MAC: [u8; 6] = [0x02, 0xec, 0x51, 0x4d, 0x00, 0x02];

    pub struct Tap {
        file: std::fs::File,
        pub name: String,
        pub mac: [u8; 6],
    }

    impl Tap {
        pub fn open(name: &str) -> std::io::Result<Self> {
            let bytes = name.as_bytes();
            if bytes.is_empty() || bytes.len() >= 16 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "TAP 接口名长度必须在 1..15 之间",
                ));
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/net/tun")?;
            let mut request = IfReq {
                name: [0u8; 16],
                flags: IFF_TAP | IFF_NO_PI,
                _pad: [0u8; 22],
            };
            request.name[..bytes.len()].copy_from_slice(bytes);
            // SAFETY: fd 来自已打开的 /dev/net/tun，IfReq 布局与内核 struct ifreq 一致（40 字节）。
            let rc = unsafe { ioctl(file.as_raw_fd(), TUNSETIFF, &mut request as *mut IfReq) };
            if rc < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mac = read_mac(name).unwrap_or(FALLBACK_MAC);
            Ok(Self {
                file,
                name: name.to_string(),
                mac,
            })
        }

        pub fn recv(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.file.read(buf)
        }

        pub fn send(&mut self, frame: &[u8]) -> std::io::Result<()> {
            self.file.write_all(frame)
        }
    }

    fn read_mac(name: &str) -> Option<[u8; 6]> {
        let text = std::fs::read_to_string(format!("/sys/class/net/{name}/address")).ok()?;
        let mut mac = [0u8; 6];
        let mut count = 0;
        for octet in text.trim().split(':') {
            if count == 6 {
                return None;
            }
            mac[count] = u8::from_str_radix(octet, 16).ok()?;
            count += 1;
        }
        (count == 6).then_some(mac)
    }
}
