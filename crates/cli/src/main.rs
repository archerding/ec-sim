//! `ec-sim` 命令行入口（B1 骨架）。
//!
//! 子命令规划：`run`（起虚拟总线/ESC 实例）、`plugin`（插件管理）、`sii`（ESI→SII 生成）。
//! B1 只保留 `--version`/`--help`，避免在未验证的接口上写空壳实现。

use std::process::ExitCode;

const USAGE: &str = "\
ec-sim — EtherCAT 从站模拟器（EC-Sim pre-alpha）

用法:
  ec-sim --version   打印版本
  ec-sim --help      打印本帮助

当前为 B1 骨架：workspace 与 crate 边界已就位，功能随 B2b+ 逐步落地。
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("ec-sim {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("ec-sim: 未知参数 `{other}`（B1 骨架仅支持 --version/--help）");
            ExitCode::from(2)
        }
    }
}
