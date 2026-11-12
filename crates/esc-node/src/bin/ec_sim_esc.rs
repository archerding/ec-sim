//! `ec-sim-esc`：Linux 上运行的最小 ESC 从站应答器（两种线缆层）。
//!
//! 用法（需 root 或 CAP_NET_ADMIN；UDS 模式不需要 root）：
//! ```text
//! ec-sim-esc --tap escsim0 [--verbose] [--esm-lax] [--manifest plugins/<id>/plugin.yaml] [--slaves N]
//! ec-sim-esc --uds /tmp/ecsim.sock [--verbose] [--esm-lax] [--manifest ...] [--slaves N]
//! ec-sim-esc --tap escsim0 [--esi <file.esi.xml>]   # B6：ESI XML → SII
//! ec-sim-esc --dump-sii [--manifest ... | --esi ...]  # 打印 SII hex/类别链后退出
//! ec-sim-esc --dump-dict [--json] [--manifest ... | --esi ...] # 打印运行时对象字典（--json = dict-v1）后退出
//! ec-sim-esc --dump-state [--manifest ... | --esi ...] # 打印只读状态 JSON（M2 WebUI 数据源）
//! ec-sim-esc --version                                # 打印版本
//!
//! ESM 状态迁移前置条件检查默认**开启**（SM 未配置时拒绝进入 PREOP/SAFEOP 并给
//! AL Status Code 0x0015/0x0017）；`--esm-lax` 关闭该检查（兼容/负向对照用）。
//! `--manifest` 指定声明式插件 manifest：SII 镜像与对象字典都按该 manifest 生成。
//! `--esi` 直接读取 ESI XML（B6 子项：ESI → SII 镜像）。单独使用时运行时对象字典只有
//! 身份对象 `0x1018`（ESI 不含完整对象字典）；与 `--manifest` **组合**时，SII 的
//! SM/PDO/邮箱取自 ESI，身份与运行时字典取自 manifest（ESI 描述 + manifest 字典）。
//! 默认使用内置 8 位 I/O 画像。
//! ```
//!
//! TAP 模式：进程自己创建/附着 TAP 接口并持有 fd（无持有者时为 NO-CARRIER，
//! 主站发不出帧），外部脚本把接口 MAC 设为 IgH `main_devices` 使用的 MAC 并 `ip link set up`。
//!
//! UDS 模式：绑定 `SOCK_DGRAM` Unix socket，每个报文 = 一帧以太网数据；收到请求帧后
//! 用 [`esc_node::Responder`] 生成应答并回给发送方（对端地址由 `recv_from` 给出）。
//! 供 `ESC-over-UDS shim`（如替换 SOEM `oshw` 收发包的夹具）或 CI 中的合成主站使用。
//!
//! 仅 Linux 提供两种线缆层；其他平台编译为可执行但直接报错退出（单元测试仍全平台可跑）。

/// `--dump-sii`：把（manifest/ESI/内置）画像生成的 SII 镜像以 hex + 类别链形式打印后退出。
/// `--dump-dict`：把（manifest/ESI/内置）运行时对象字典以对象/条目列表打印后退出。
fn dump_dict_and_exit(
    manifest_path: Option<&str>,
    esi_path: Option<&str>,
    json: bool,
) -> std::process::ExitCode {
    use esc_core::obj_dict::ObjectDictionary;

    let dict: Result<ObjectDictionary, String> = if let Some(path) = manifest_path {
        plugin_host::manifest::PluginManifest::load(path)
            .and_then(|manifest| plugin_host::dict_gen::dictionary_from_manifest(&manifest))
    } else if let Some(path) = esi_path {
        match std::fs::read_to_string(path) {
            Ok(text) => plugin_host::esi::parse_esi(&text)
                .map(|device| plugin_host::esi::dictionary_from_esi(&device)),
            Err(err) => Err(format!("读取 ESI 失败：{err}")),
        }
    } else {
        Ok(ObjectDictionary::simulated())
    };
    let dict = match dict {
        Ok(dict) => dict,
        Err(err) => {
            eprintln!("ec-sim-esc: {err}");
            return std::process::ExitCode::from(2);
        }
    };
    if json {
        use std::fmt::Write as _;
        let entry_count: usize = dict
            .objects()
            .iter()
            .map(|object| usize::from(object.max_subindex) + 1)
            .sum();
        let mut out = String::new();
        let _ = writeln!(out, "{{");
        let _ = writeln!(out, "  \"schema\": \"ec-sim/dict-v1\",");
        let _ = writeln!(
            out,
            "  \"tool\": \"ec-sim-esc {}\",",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(out, "  \"object_count\": {},", dict.objects().len());
        let _ = writeln!(out, "  \"entry_count\": {entry_count},");
        let _ = writeln!(out, "  \"objects\": [");
        for (object_pos, object) in dict.objects().iter().enumerate() {
            let _ = writeln!(out, "    {{");
            let _ = writeln!(out, "      \"index\": \"0x{:04X}\",", object.index);
            let _ = writeln!(out, "      \"max_sub\": {},", object.max_subindex);
            let _ = writeln!(out, "      \"code\": \"0x{:04X}\",", object.object_code);
            let _ = writeln!(out, "      \"name\": \"{}\",", json_escape(&object.name));
            let _ = writeln!(out, "      \"entries\": [");
            let mut wrote_entry = false;
            for subindex in 0..=object.max_subindex {
                let Some(entry) = dict.entry(object.index, subindex) else {
                    continue;
                };
                if wrote_entry {
                    out.push_str(",\n");
                }
                let access = if entry.writable { "rw" } else { "ro" };
                let value_hex: String = entry
                    .value
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                let _ = write!(
                    out,
                    "        {{\"sub\": {subindex}, \"type\": \"0x{:04X}\", \"bits\": {}, \"access\": \"{access}\", \"name\": \"{}\", \"value_hex\": \"{value_hex}\"}}",
                    entry.data_type,
                    entry.bit_length,
                    json_escape(&entry.name)
                );
                wrote_entry = true;
            }
            let _ = writeln!(out);
            let tail = if object_pos + 1 == dict.objects().len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(out, "      ]");
            let _ = writeln!(out, "    }}{tail}");
        }
        let _ = writeln!(out, "  ]");
        let _ = writeln!(out, "}}");
        print!("{out}");
        return std::process::ExitCode::SUCCESS;
    }

    println!(
        "ec-sim-esc dictionary dump: objects={} entries={}",
        dict.objects().len(),
        dict.objects()
            .iter()
            .map(|object| usize::from(object.max_subindex) + 1)
            .sum::<usize>()
    );
    for object in dict.objects() {
        println!(
            "{:#06X} max_sub={} code={:#06X} name={}",
            object.index, object.max_subindex, object.object_code, object.name
        );
        for subindex in 0..=object.max_subindex {
            let Some(entry) = dict.entry(object.index, subindex) else {
                continue;
            };
            let access = if entry.writable { "rw" } else { "ro" };
            let value = if entry.value.len() <= 8 {
                format!("{:02x?}", entry.value)
            } else {
                format!("{} bytes", entry.value.len())
            };
            println!(
                "  {:02X} type={:#06x} bits={} {} name={} value={}",
                subindex, entry.data_type, entry.bit_length, access, entry.name, value
            );
        }
    }
    std::process::ExitCode::SUCCESS
}
fn dump_sii_and_exit(
    manifest_path: Option<&str>,
    esi_path: Option<&str>,
) -> std::process::ExitCode {
    use esc_core::sii::{category, simulated_spec, SiiImage, SiiSpec};

    let spec: Result<SiiSpec, String> = match (esi_path, manifest_path) {
        (Some(esi), Some(manifest)) => {
            let device = plugin_host::esi::parse_esi(&match std::fs::read_to_string(esi) {
                Ok(text) => text,
                Err(err) => {
                    eprintln!("ec-sim-esc: 读取 ESI 失败：{err}");
                    return std::process::ExitCode::from(2);
                }
            })
            .and_then(|device| {
                plugin_host::manifest::PluginManifest::load(manifest)
                    .map(|manifest| (device, manifest))
            })
            .map_err(|err| format!("ESI/manifest 解析失败：{err}"));
            device.and_then(|(device, manifest)| {
                plugin_host::esi::sii_spec_from_esi_and_manifest(&device, &manifest)
            })
        }
        (Some(esi), None) => match std::fs::read_to_string(esi) {
            Ok(text) => plugin_host::esi::parse_esi(&text)
                .and_then(|device| plugin_host::esi::sii_spec_from_esi(&device)),
            Err(err) => Err(format!("读取 ESI 失败：{err}")),
        },
        (None, Some(manifest)) => plugin_host::manifest::PluginManifest::load(manifest)
            .and_then(|manifest| plugin_host::sii_gen::sii_spec_from_manifest(&manifest)),
        (None, None) => Ok(simulated_spec()),
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(err) => {
            eprintln!("ec-sim-esc: {err}");
            return std::process::ExitCode::from(2);
        }
    };
    let sii = SiiImage::from_spec(&spec);
    println!(
        "ec-sim-esc SII dump: vendor=0x{:08x} product=0x{:08x} revision=0x{:08x} words={}",
        spec.vendor_id,
        spec.product_code,
        spec.revision,
        sii.len_words()
    );
    for (line, chunk) in sii.words().chunks(8).enumerate() {
        let mut text = String::new();
        for word in chunk {
            text.push_str(&format!("{word:04x} "));
        }
        println!("{:04x}: {text}", line * 8);
    }
    let mut word = 0x0040usize;
    let words = sii.words();
    while word < words.len() {
        let cat_type = words[word];
        if cat_type == category::TERMINATOR {
            println!("{word:04x}: 0xffff (end)");
            break;
        }
        let words_len = usize::from(words[word + 1]);
        println!("{word:04x}: category 0x{cat_type:04x}, {words_len} words");
        word += 2 + words_len;
    }
    std::process::ExitCode::SUCCESS
}
/// JSON 字符串转义（只处理常用控制字符；`--dump-state` 输出为机器可读 JSON）。
fn json_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// `--dump-state`：把（manifest/ESI/内置）画像的只读状态以 JSON 输出后退出（M2 WebUI 数据源）。
fn dump_state_and_exit(
    manifest_path: Option<&str>,
    esi_path: Option<&str>,
) -> std::process::ExitCode {
    use esc_core::sii::{simulated_spec, SiiSpec};
    use std::fmt::Write as _;

    let manifest = match manifest_path {
        Some(path) => match plugin_host::manifest::PluginManifest::load(path) {
            Ok(manifest) => Some(manifest),
            Err(err) => {
                eprintln!("ec-sim-esc: {err}");
                return std::process::ExitCode::from(2);
            }
        },
        None => None,
    };
    let device = match esi_path {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(err) => {
                    eprintln!("ec-sim-esc: 读取 ESI 失败：{err}");
                    return std::process::ExitCode::from(2);
                }
            };
            match plugin_host::esi::parse_esi(&text) {
                Ok(device) => Some(device),
                Err(err) => {
                    eprintln!("ec-sim-esc: 解析 ESI 失败：{err}");
                    return std::process::ExitCode::from(2);
                }
            }
        }
        None => None,
    };
    let spec: Result<SiiSpec, String> = match (&device, &manifest) {
        (Some(device), Some(manifest)) => {
            plugin_host::esi::sii_spec_from_esi_and_manifest(device, manifest)
        }
        (Some(device), None) => plugin_host::esi::sii_spec_from_esi(device),
        (None, Some(manifest)) => plugin_host::sii_gen::sii_spec_from_manifest(manifest),
        (None, None) => Ok(simulated_spec()),
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(err) => {
            eprintln!("ec-sim-esc: {err}");
            return std::process::ExitCode::from(2);
        }
    };
    let dict_objects = manifest.as_ref().and_then(|manifest| {
        plugin_host::dict_gen::dictionary_from_manifest(manifest)
            .ok()
            .map(|dict| dict.objects().len())
    });

    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"schema\": \"ec-sim/state-v1\",");
    let _ = writeln!(
        out,
        "  \"tool\": \"ec-sim-esc {}\",",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(
        out,
        "  \"identity\": {{\"vendor_id\": \"0x{:08X}\", \"product_code\": \"0x{:08X}\", \"revision\": \"0x{:08X}\", \"serial\": \"0x{:08X}\"}},",
        spec.vendor_id, spec.product_code, spec.revision, spec.serial
    );
    let names: Vec<String> = spec
        .names
        .iter()
        .map(|name| format!("\"{}\"", json_escape(name)))
        .collect();
    let _ = writeln!(out, "  \"names\": [{}],", names.join(", "));
    let _ = writeln!(out, "  \"mailbox_declared\": {},", spec.mailbox.is_some());
    let _ = writeln!(out, "  \"coe_details\": {},", spec.coe_details);
    let _ = writeln!(out, "  \"general_flags\": {},", spec.general_flags);
    let _ = writeln!(out, "  \"ebus_current_ma\": {},", spec.ebus_current);

    let _ = writeln!(out, "  \"sync_managers\": [");
    for (index, sm) in spec.sync_managers.iter().enumerate() {
        if index + 1 == spec.sync_managers.len() {
            let _ = writeln!(
                out,
                "    {{\"start\": \"0x{:04X}\", \"length\": {}, \"control\": \"0x{:02X}\", \"status\": {}, \"enable\": {}}}",
                sm.physical_start_address, sm.length, sm.control, sm.status, sm.enable
            );
        } else {
            let _ = writeln!(
                out,
                "    {{\"start\": \"0x{:04X}\", \"length\": {}, \"control\": \"0x{:02X}\", \"status\": {}, \"enable\": {}}},",
                sm.physical_start_address, sm.length, sm.control, sm.status, sm.enable
            );
        }
    }
    let _ = writeln!(out, "  ],");

    for (label, pdos) in [("tx", &spec.tx_pdos), ("rx", &spec.rx_pdos)] {
        let _ = writeln!(out, "  \"pdo_{label}\": [");
        for (index, pdo) in pdos.iter().enumerate() {
            let entries: Vec<String> = pdo
                .entries
                .iter()
                .map(|entry| {
                    format!(
                        "{{\"index\": \"0x{:04X}\", \"sub\": {}, \"bits\": {}}}",
                        entry.index, entry.subindex, entry.bit_length
                    )
                })
                .collect();
            let tail = if index + 1 == pdos.len() { "" } else { "," };
            let _ = writeln!(
                out,
                "    {{\"index\": \"0x{:04X}\", \"sync_manager\": {}, \"entries\": [{}]}}{tail}",
                pdo.index,
                pdo.sync_manager,
                entries.join(", ")
            );
        }
        let _ = writeln!(out, "  ],");
    }

    match &spec.dc {
        Some(dc) => {
            let _ = writeln!(
                out,
                "  \"dc\": {{\"cycle_time0_ns\": {}, \"shift_time0_ns\": {}, \"shift_time1_ns\": {}, \"sync0_cycle_factor\": {}, \"sync1_cycle_factor\": {}, \"assign_activate\": \"0x{:04X}\"}},",
                dc.cycle_time0,
                dc.shift_time0,
                dc.shift_time1,
                dc.sync0_cycle_factor,
                dc.sync1_cycle_factor,
                dc.assign_activate
            );
        }
        None => {
            let _ = writeln!(out, "  \"dc\": null,");
        }
    }
    match &manifest {
        Some(manifest) => {
            let _ = writeln!(
                out,
                "  \"profile\": {{\"id\": \"{}\", \"name\": \"{}\"}},",
                json_escape(&manifest.id),
                json_escape(&manifest.name)
            );
        }
        None => {
            let _ = writeln!(out, "  \"profile\": null,");
        }
    }
    match dict_objects {
        Some(count) => {
            let _ = writeln!(out, "  \"dict_objects\": {count}");
        }
        None => {
            let _ = writeln!(out, "  \"dict_objects\": null");
        }
    }
    let _ = writeln!(out, "}}");
    print!("{out}");
    std::process::ExitCode::SUCCESS
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let mut tap_name: Option<String> = None;
    let mut uds_path: Option<String> = None;
    let mut manifest_path: Option<String> = None;
    let mut esi_path: Option<String> = None;
    let mut behavior: Option<String> = None;
    let mut verbose = false;
    let mut esm_preconditions = true;
    let mut fault: Option<u16> = None;
    // 同一进程模拟的 ESC 数量（B12 多从站链；默认 1）。
    let mut slave_count: usize = 1;
    // UDS 线缆层忙轮询模式（B17 延迟基准用；默认阻塞等待，省 CPU）。
    let mut poll = false;
    let mut dump_sii = false;
    let mut dump_dict = false;
    let mut dump_json = false;
    let mut dump_state = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tap" => match args.next() {
                Some(name) => tap_name = Some(name),
                None => {
                    eprintln!("ec-sim-esc: --tap 需要一个接口名");
                    return std::process::ExitCode::from(2);
                }
            },
            "--uds" => match args.next() {
                Some(path) => uds_path = Some(path),
                None => {
                    eprintln!("ec-sim-esc: --uds 需要一个 socket 路径");
                    return std::process::ExitCode::from(2);
                }
            },
            "--manifest" => match args.next() {
                Some(path) => manifest_path = Some(path),
                None => {
                    eprintln!("ec-sim-esc: --manifest 需要一个 plugin.yaml 路径");
                    return std::process::ExitCode::from(2);
                }
            },
            "--esi" => match args.next() {
                Some(path) => esi_path = Some(path),
                None => {
                    eprintln!("ec-sim-esc: --esi 需要一个 .esi.xml 路径");
                    return std::process::ExitCode::from(2);
                }
            },
            "--behavior" => match args.next() {
                Some(name) => match name.as_str() {
                    "cia402" => behavior = Some(name),
                    other => {
                        eprintln!("ec-sim-esc: 不支持的 --behavior {other}（当前仅 cia402）");
                        return std::process::ExitCode::from(2);
                    }
                },
                None => {
                    eprintln!("ec-sim-esc: --behavior 需要一个行为名（cia402）");
                    return std::process::ExitCode::from(2);
                }
            },
            "--slaves" => match args.next() {
                Some(text) => match text.parse::<usize>() {
                    Ok(count) if (1..=64).contains(&count) => slave_count = count,
                    _ => {
                        eprintln!("ec-sim-esc: --slaves 需要 1..=64 的整数");
                        return std::process::ExitCode::from(2);
                    }
                },
                None => {
                    eprintln!("ec-sim-esc: --slaves 需要数量（如 8）");
                    return std::process::ExitCode::from(2);
                }
            },
            "--fault" => match args.next() {
                Some(text) => {
                    let digits = text.trim_start_matches("0x").trim_start_matches("0X");
                    match u16::from_str_radix(digits, 16) {
                        Ok(code) => fault = Some(code),
                        Err(_) => {
                            eprintln!("ec-sim-esc: --fault 需要十六进制错误码（如 0x7500）");
                            return std::process::ExitCode::from(2);
                        }
                    }
                }
                None => {
                    eprintln!("ec-sim-esc: --fault 需要一个错误码");
                    return std::process::ExitCode::from(2);
                }
            },
            "--verbose" | "-v" => verbose = true,
            // B17：UDS 忙轮询（延迟基准用；CPU 占用高，默认关闭）。
            "--poll" => poll = true,
            "--dump-sii" => dump_sii = true,
            "--dump-dict" => dump_dict = true,
            "--json" => dump_json = true,
            "--dump-state" => dump_state = true,
            "--version" | "-V" => {
                println!("ec-sim-esc {}", env!("CARGO_PKG_VERSION"));
                return std::process::ExitCode::SUCCESS;
            }
            "--esm-lax" => esm_preconditions = false,
            // 兼容旧参数名：前置条件已是默认行为。
            "--esm-strict" => esm_preconditions = true,
            "--help" | "-h" => {
                println!(
                    "用法: ec-sim-esc [--tap <name> | --uds <path>] [--verbose] [--esm-lax] \
                     [--manifest <plugin.yaml>] [--esi <file.esi.xml>] [--behavior cia402]
                     [--fault <hex>（需 --behavior cia402）] [--slaves <1..64>] [--poll]
                     调试：--dump-sii [--manifest <yaml>] [--esi <xml>] 打印 SII 镜像 hex/类别链后退出；
                     --dump-dict [--json] [--manifest <yaml>] [--esi <xml>] 打印运行时对象字典后退出（--json 输出 dict-v1）；
                     --dump-state [--manifest <yaml>] [--esi <xml>] 打印只读状态 JSON（供 WebUI/工具消费）；
                     --version 打印版本。"
                );
                return std::process::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("ec-sim-esc: 未知参数 {other}");
                return std::process::ExitCode::from(2);
            }
        }
    }

    if dump_dict {
        return dump_dict_and_exit(manifest_path.as_deref(), esi_path.as_deref(), dump_json);
    }

    if dump_state {
        return dump_state_and_exit(manifest_path.as_deref(), esi_path.as_deref());
    }

    if dump_sii {
        return dump_sii_and_exit(manifest_path.as_deref(), esi_path.as_deref());
    }

    if tap_name.is_some() && uds_path.is_some() {
        eprintln!("ec-sim-esc: --tap 与 --uds 互斥");
        return std::process::ExitCode::from(2);
    }

    let manifest = match manifest_path {
        Some(path) => match plugin_host::manifest::PluginManifest::load(&path) {
            Ok(manifest) => {
                if !manifest.validate().is_empty() {
                    eprintln!(
                        "ec-sim-esc: manifest {} 校验未通过：{}",
                        path,
                        manifest.validate().join("；")
                    );
                    return std::process::ExitCode::from(2);
                }
                println!(
                    "MANIFEST id={} name={} vendor=0x{:08X} product=0x{:08X}",
                    manifest.id,
                    manifest.name,
                    manifest.identity.vendor_id,
                    manifest.identity.product_code
                );
                Some(manifest)
            }
            Err(error) => {
                eprintln!("ec-sim-esc: 读取 manifest {path} 失败：{error}");
                return std::process::ExitCode::from(2);
            }
        },
        None => None,
    };

    let esi = match esi_path {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(xml) => match plugin_host::esi::parse_esi(&xml) {
                Ok(device) => {
                    println!(
                        "ESI name={} vendor=0x{:08X} product=0x{:08X} revision=0x{:08X}",
                        device.name, device.vendor_id, device.product_code, device.revision
                    );
                    Some(device)
                }
                Err(error) => {
                    eprintln!("ec-sim-esc: 解析 ESI {path} 失败：{error}");
                    return std::process::ExitCode::from(2);
                }
            },
            Err(error) => {
                eprintln!("ec-sim-esc: 读取 ESI {path} 失败：{error}");
                return std::process::ExitCode::from(2);
            }
        },
        None => None,
    };

    if !cfg!(target_os = "linux") {
        eprintln!("ec-sim-esc: TAP/UDS 线缆层仅在 Linux 可用（当前平台不支持）");
        return std::process::ExitCode::from(3);
    }

    match (tap_name, uds_path) {
        (_, Some(path)) => run_uds(
            &path,
            verbose,
            esm_preconditions,
            manifest,
            esi,
            behavior,
            fault,
            slave_count,
            poll,
        ),
        (Some(name), None) => run(
            &name,
            verbose,
            esm_preconditions,
            manifest,
            esi,
            behavior,
            fault,
            slave_count,
        ),
        (None, None) => run(
            "ecsim0",
            verbose,
            esm_preconditions,
            manifest,
            esi,
            behavior,
            fault,
            slave_count,
        ),
    }
}

/// 按 manifest / ESI（可选）构造应答器：SII 与对象字典都从画像生成，否则用内置画像。
#[cfg(target_os = "linux")]
fn build_responder(
    manifest: Option<&plugin_host::manifest::PluginManifest>,
    esi: Option<&plugin_host::esi::EsiDevice>,
    behavior: Option<&str>,
    esm_preconditions: bool,
    fault: Option<u16>,
) -> Result<esc_node::Responder, String> {
    let responder = match (manifest, esi) {
        // `--manifest` 与 `--esi` 可以组合：ESI 提供 SII 描述（SM/PDO/邮箱），manifest 提供
        // 身份（R2 自有 VID/PID）与运行时对象字典（ESI 不含 OD）。
        (Some(manifest), Some(device)) => {
            let spec = plugin_host::esi::sii_spec_from_esi_and_manifest(device, manifest)?;
            let sii = esc_core::sii::SiiImage::from_spec(&spec);
            let dict = plugin_host::dict_gen::dictionary_from_manifest(manifest)?;
            esc_node::Responder::new(sii).with_dictionary(dict)
        }
        (Some(manifest), _) => {
            let spec = plugin_host::sii_gen::sii_spec_from_manifest(manifest)?;
            let sii = esc_core::sii::SiiImage::from_spec(&spec);
            let dict = plugin_host::dict_gen::dictionary_from_manifest(manifest)?;
            esc_node::Responder::new(sii).with_dictionary(dict)
        }
        (None, Some(device)) => {
            let spec = plugin_host::esi::sii_spec_from_esi(device)?;
            let sii = esc_core::sii::SiiImage::from_spec(&spec);
            let dict = plugin_host::esi::dictionary_from_esi(device);
            esc_node::Responder::new(sii).with_dictionary(dict)
        }
        (None, None) => esc_node::Responder::default(),
    };
    // Tier0 行为插件：`--behavior cia402` 时启用控制字 → 状态字状态机（SM 长度 >=7
    // 时同时处理完整 RxPDO/TxPDO：模式与目标/实际位置）。
    let responder = if behavior == Some("cia402") {
        let (out_offset, out_len, in_offset, in_len) = match manifest {
            Some(manifest) => {
                let out = manifest.sm.iter().find(|sm| sm.dir == "out");
                let input = manifest.sm.iter().find(|sm| sm.dir == "in");
                (
                    out.map_or(0x1100, |sm| sm.start),
                    out.map_or(2, |sm| sm.len),
                    input.map_or(0x1110, |sm| sm.start),
                    input.map_or(2, |sm| sm.len),
                )
            }
            None => (0x1100, 2, 0x1110, 2),
        };
        println!(
            "BEHAVIOR cia402 sm2_out=0x{out_offset:04X}/{out_len}B sm3_in=0x{in_offset:04X}/{in_len}B"
        );
        responder.with_cia402_behavior(out_offset, out_len, in_offset, in_len)
    } else {
        responder
    };
    let mut responder = responder;
    if let Some(code) = fault {
        if behavior == Some("cia402") {
            responder.inject_cia402_fault(code);
            println!("FAULT cia402 error_code=0x{code:04X}");
        } else {
            eprintln!("ec-sim-esc: --fault 需要 --behavior cia402，已忽略");
        }
    }
    Ok(responder.with_esm_preconditions(esm_preconditions))
}

/// 构造 `count` 个从站应答器（B12）：同一画像克隆 N 份，链序即向量顺序。
#[cfg(target_os = "linux")]
fn build_responders(
    manifest: Option<&plugin_host::manifest::PluginManifest>,
    esi: Option<&plugin_host::esi::EsiDevice>,
    behavior: Option<&str>,
    esm_preconditions: bool,
    fault: Option<u16>,
    count: usize,
) -> Result<Vec<esc_node::Responder>, String> {
    let first = build_responder(manifest, esi, behavior, esm_preconditions, fault)?;
    Ok((0..count).map(|_| first.clone()).collect())
}

/// `--verbose` 时每 50 帧打印一次应答统计（TAP 与 UDS 两种线缆层共用）。
#[cfg(target_os = "linux")]
fn maybe_print_stats(responder: &esc_node::Responder, verbose: bool) {
    if !verbose {
        return;
    }
    let stats = responder.stats();
    if stats.frames_in % 50 != 0 {
        return;
    }
    eprintln!(
        "STATS in={} out={} dropped={} hits={} mbox={} mboxresp={} dc_rd={} dc_wr={} al=0x{:02x} station=0x{:04x}",
        stats.frames_in,
        stats.frames_out,
        stats.frames_dropped,
        stats.datagrams_hit,
        stats.mailbox_requests,
        stats.mailbox_responses,
        stats.dc_reg_reads,
        stats.dc_reg_writes,
        responder.al_status(),
        responder.station_address()
    );
}

/// 帧驱动时钟（毫秒）：TAP/UDS 每个收到的帧调用一次 `Responder::tick`。
#[cfg(target_os = "linux")]
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)]
fn run_uds(
    path: &str,
    verbose: bool,
    esm_preconditions: bool,
    manifest: Option<plugin_host::manifest::PluginManifest>,
    esi: Option<plugin_host::esi::EsiDevice>,
    behavior: Option<String>,
    fault: Option<u16>,
    slave_count: usize,
    poll: bool,
) -> std::process::ExitCode {
    use esc_node::DEFAULT_SIM_MAC;
    use std::io::Write;
    use std::os::unix::net::UnixDatagram;

    // 清理上次运行留下的 socket 文件；随后绑定 SOCK_DGRAM（保留报文边界 = 帧边界）。
    let _ = std::fs::remove_file(path);
    let socket = match UnixDatagram::bind(path) {
        Ok(socket) => socket,
        Err(err) => {
            eprintln!("ec-sim-esc: 绑定 UDS {path} 失败: {err}");
            return std::process::ExitCode::from(4);
        }
    };
    println!("READY uds={path}");
    let _ = std::io::stdout().flush();

    let mut responders = match build_responders(
        manifest.as_ref(),
        esi.as_ref(),
        behavior.as_deref(),
        esm_preconditions,
        fault,
        slave_count,
    ) {
        Ok(responders) => responders,
        Err(error) => {
            eprintln!("ec-sim-esc: 按 manifest/ESI 构造应答器失败：{error}");
            return std::process::ExitCode::from(7);
        }
    };
    let mut buf = [0u8; 2048];
    if poll {
        if let Err(err) = socket.set_nonblocking(true) {
            eprintln!("ec-sim-esc: --poll 设置非阻塞失败：{err}");
            return std::process::ExitCode::from(4);
        }
        println!("POLL uds 忙轮询模式已启用（CPU 占用高，仅基准用）");
    }
    loop {
        let (len, peer) = match socket.recv_from(&mut buf) {
            Ok(received) => received,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            // B17：忙轮询模式（--poll）下无数据时自旋，减少唤醒延迟。
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::hint::spin_loop();
                continue;
            }
            // 对端已关闭/不可达时继续等待下一个客户端（CI 场景常见）。
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => continue,
            Err(err) => {
                eprintln!("ec-sim-esc: 读取 UDS 失败: {err}");
                return std::process::ExitCode::from(5);
            }
        };
        if verbose {
            eprintln!("RX uds {len} bytes from {peer:?}");
        }
        for responder in &mut responders {
            responder.tick(now_ms());
        }
        let Some(reply) =
            esc_node::handle_frame_chain(&mut responders, &buf[..len], DEFAULT_SIM_MAC)
        else {
            if verbose {
                eprintln!("DROP uds {len} bytes（非 EtherCAT 或非法帧）");
            }
            continue;
        };
        // 回包需要客户端绑定到文件系统路径（abstract/匿名对端无法 send_to，直接跳过）。
        let Some(peer_path) = peer.as_pathname() else {
            continue;
        };
        if let Err(err) = socket.send_to(&reply, peer_path) {
            eprintln!("ec-sim-esc: 写回 UDS 失败（对端可能已退出）: {err}");
            continue;
        }
        maybe_print_stats(&responders[0], verbose);
        let _ = DEFAULT_SIM_MAC;
    }
}

#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)]
fn run(
    tap_name: &str,
    verbose: bool,
    esm_preconditions: bool,
    manifest: Option<plugin_host::manifest::PluginManifest>,
    esi: Option<plugin_host::esi::EsiDevice>,
    behavior: Option<String>,
    fault: Option<u16>,
    slave_count: usize,
) -> std::process::ExitCode {
    use esc_node::DEFAULT_SIM_MAC;
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

    let mut responders = match build_responders(
        manifest.as_ref(),
        esi.as_ref(),
        behavior.as_deref(),
        esm_preconditions,
        fault,
        slave_count,
    ) {
        Ok(responders) => responders,
        Err(error) => {
            eprintln!("ec-sim-esc: 按 manifest/ESI 构造应答器失败：{error}");
            return std::process::ExitCode::from(7);
        }
    };
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
        for responder in &mut responders {
            responder.tick(now_ms());
        }
        let Some(reply) = esc_node::handle_frame_chain(&mut responders, &buf[..len], mac) else {
            continue;
        };
        if let Err(err) = tap.send(&reply) {
            eprintln!("ec-sim-esc: 写回 TAP 失败: {err}");
            return std::process::ExitCode::from(6);
        }
        maybe_print_stats(&responders[0], verbose);
        let _ = DEFAULT_SIM_MAC;
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::too_many_arguments)]
fn run(
    _tap_name: &str,
    _verbose: bool,
    _esm_preconditions: bool,
    _manifest: Option<plugin_host::manifest::PluginManifest>,
    _esi: Option<plugin_host::esi::EsiDevice>,
    _behavior: Option<String>,
    _fault: Option<u16>,
    _slave_count: usize,
) -> std::process::ExitCode {
    eprintln!("ec-sim-esc: TAP/UDS 线缆层仅在 Linux 可用（当前平台不支持）");
    std::process::ExitCode::from(3)
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::too_many_arguments)]
fn run_uds(
    _path: &str,
    _verbose: bool,
    _esm_preconditions: bool,
    _manifest: Option<plugin_host::manifest::PluginManifest>,
    _esi: Option<plugin_host::esi::EsiDevice>,
    _behavior: Option<String>,
    _fault: Option<u16>,
    _slave_count: usize,
    _poll: bool,
) -> std::process::ExitCode {
    eprintln!("ec-sim-esc: TAP/UDS 线缆层仅在 Linux 可用（当前平台不支持）");
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
