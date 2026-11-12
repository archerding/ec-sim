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
//! ec-sim-esc --serve 127.0.0.1:8090 [--manifest ...] # 只读 HTTP JSON API（/state,/dict）
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
        print!("{}", control_plane::dict_json(&dict));
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

/// `--dump-state`：把（manifest/ESI/内置）画像的只读状态以 JSON 输出后退出（M2 WebUI 数据源）。
fn dump_state_and_exit(
    manifest_path: Option<&str>,
    esi_path: Option<&str>,
) -> std::process::ExitCode {
    use esc_core::sii::{simulated_spec, SiiSpec};

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
    let profile = manifest
        .as_ref()
        .map(|manifest| (manifest.id.as_str(), manifest.name.as_str()));
    let dict_objects = manifest.as_ref().and_then(|manifest| {
        plugin_host::dict_gen::dictionary_from_manifest(manifest)
            .ok()
            .map(|dict| dict.objects().len())
    });
    print!(
        "{}",
        control_plane::state_json(&spec, dict_objects, profile)
    );
    std::process::ExitCode::SUCCESS
}

#[cfg(target_os = "linux")]
/// `/pdo` 历史缓冲容量（按 PDO 变化去重的时间序列）。
const PDO_HISTORY_CAPACITY: usize = 256;

/// 解析后的 HTTP 请求（静态与实时 `--serve` 共用）。
struct HttpRequest {
    method: String,
    path: String,
    /// 请求正文；仅实时模式的写路由（`POST /sdo`、`POST /pdo`）读取，
    /// 静态模式（跨平台）不读，故非 Linux 构建下显式豁免 dead_code。
    #[allow(dead_code)]
    body: Vec<u8>,
    headers: Vec<(String, String)>,
}

/// 读取一个完整 HTTP 请求（方法、路径、正文、请求头）。
fn read_http_request(stream: &mut std::net::TcpStream) -> Option<HttpRequest> {
    use std::io::Read;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..read]);
        if let Some(position) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length = 0usize;
    for line in lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim().to_string();
        if key.eq_ignore_ascii_case("content-length") {
            content_length = value.parse::<usize>().unwrap_or(0).min(64 * 1024);
        }
        headers.push((key, value));
    }
    while buf.len() < header_end + content_length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    let body = buf[header_end..buf.len().min(header_end + content_length)].to_vec();
    Some(HttpRequest {
        method,
        path,
        body,
        headers,
    })
}

/// 写一个 HTTP 响应（`extra_headers` 为附加头行，需自带 `\r\n`；无附加头时传空串）。
fn write_http_response(
    stream: &mut std::net::TcpStream,
    response: &control_plane::HttpResponse,
    extra_headers: &str,
) {
    use std::io::Write as _;

    let headers = format!(
        "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: {}\r\n{extra_headers}Connection: close\r\n\r\n",
        response.status_line,
        response.content_type,
        response.body.len(),
        control_plane::CORS_ALLOW_ORIGIN
    );
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(response.body.as_bytes());
    let _ = stream.flush();
}

/// B41：需要鉴权的 API 路由在 token 不匹配时返回 401/403（否则 `None`）。
///
/// 返回 `(响应, 附加头)`：401 需要 `WWW-Authenticate: Bearer`，403 不需要。
fn auth_gate(
    path_only: &str,
    headers: &[(String, String)],
    token: Option<&str>,
) -> Option<(control_plane::HttpResponse, &'static str)> {
    if !control_plane::is_protected_route(path_only) {
        return None;
    }
    match control_plane::authorize(headers, token) {
        control_plane::AuthDecision::Disabled | control_plane::AuthDecision::Allowed => None,
        control_plane::AuthDecision::MissingToken => Some((
            control_plane::unauthorized_json(),
            "WWW-Authenticate: Bearer realm=\"ec-sim\"\r\n",
        )),
        control_plane::AuthDecision::InvalidToken => {
            Some((control_plane::forbidden_token_json(), ""))
        }
    }
}

fn static_web_response(path: &str, web_root: Option<&str>) -> Option<control_plane::HttpResponse> {
    let root = web_root?;
    let path = path.split('?').next().unwrap_or(path);
    let (file, content_type) = match path {
        "/" | "/index.html" => ("index.html", "text/html; charset=utf-8"),
        "/app.js" => ("app.js", "application/javascript; charset=utf-8"),
        _ => return None,
    };
    let body = std::fs::read_to_string(format!("{root}/{file}")).ok()?;
    Some(control_plane::HttpResponse {
        status_line: "HTTP/1.1 200 OK",
        content_type,
        body,
    })
}

fn serve_http(
    manifest_path: Option<&str>,
    esi_path: Option<&str>,
    addr: &str,
    web_root: Option<&str>,
    serve_token: Option<&str>,
) -> std::process::ExitCode {
    use std::io::Write as _;
    use std::net::TcpListener;

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
    let spec = match (&device, &manifest) {
        (Some(device), Some(manifest)) => {
            plugin_host::esi::sii_spec_from_esi_and_manifest(device, manifest)
        }
        (Some(device), None) => plugin_host::esi::sii_spec_from_esi(device),
        (None, Some(manifest)) => plugin_host::sii_gen::sii_spec_from_manifest(manifest),
        (None, None) => Ok(esc_core::sii::simulated_spec()),
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(err) => {
            eprintln!("ec-sim-esc: {err}");
            return std::process::ExitCode::from(2);
        }
    };

    let (dict_objects, profile, dict_json, dict_for_decode) = match &manifest {
        Some(manifest) => match plugin_host::dict_gen::dictionary_from_manifest(manifest) {
            Ok(dict) => (
                Some(dict.objects().len()),
                Some((manifest.id.as_str(), manifest.name.as_str())),
                Some(control_plane::dict_json(&dict)),
                Some(dict),
            ),
            Err(err) => {
                eprintln!("ec-sim-esc: 生成对象字典失败：{err}");
                return std::process::ExitCode::from(2);
            }
        },
        None => (None, None, None, None),
    };
    let state = control_plane::state_json(&spec, dict_objects, profile);

    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("ec-sim-esc: --serve 绑定 {addr} 失败：{err}");
            return std::process::ExitCode::from(4);
        }
    };
    println!("SERVE http://{addr} routes=/state,/dict");
    let _ = std::io::stdout().flush();

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let Some(HttpRequest {
            method,
            path,
            body: _,
            headers,
        }) = read_http_request(&mut stream)
        else {
            continue;
        };
        let path_only = path.split('?').next().unwrap_or(path.as_str());
        if let Some((response, extra_headers)) = auth_gate(path_only, &headers, serve_token) {
            write_http_response(&mut stream, &response, extra_headers);
            continue;
        }
        if method != "GET" {
            let response = control_plane::HttpResponse {
                status_line: "HTTP/1.1 405 Method Not Allowed",
                content_type: "application/json; charset=utf-8",
                body: "{\"error\": \"静态模式仅支持 GET；写操作需实时模式 --serve-write\"}"
                    .to_string(),
            };
            write_http_response(&mut stream, &response, "");
            continue;
        }
        let response = if let Some(response) = static_web_response(&path, web_root) {
            response
        } else if path_only == "/sdo" {
            sdo_response(&path, dict_for_decode.as_ref())
        } else {
            control_plane::handle_get(&path, &state, dict_json.as_deref())
        };
        write_http_response(&mut stream, &response, "");
    }
    std::process::ExitCode::SUCCESS
}

fn query_param<'a>(path: &'a str, name: &str) -> Option<&'a str> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == name {
            return Some(value);
        }
    }
    None
}

fn parse_u16_param(text: &str) -> Option<u16> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16).ok()
    } else {
        text.parse::<u16>().ok()
    }
}

fn parse_sdo_query(path: &str) -> Result<(u16, u8), control_plane::HttpResponse> {
    let Some(index_text) = query_param(path, "index") else {
        return Err(control_plane::not_found_json("需要 ?index=0x....[&sub=N]"));
    };
    let Some(index) = parse_u16_param(index_text) else {
        return Err(control_plane::not_found_json(
            "index 非法（支持 0x 十六进制或十进制）",
        ));
    };
    let sub = query_param(path, "sub")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    Ok((index, sub))
}

#[cfg(target_os = "linux")]
fn forbidden_json(message: &str) -> control_plane::HttpResponse {
    control_plane::HttpResponse {
        status_line: "HTTP/1.1 403 Forbidden",
        content_type: "application/json; charset=utf-8",
        body: format!(
            "{{\"error\": \"{}\", \"hint\": \"仅 manifest 声明 writable 的对象可写，且身份/配置对象被排除\"}}",
            control_plane::json_escape(message)
        ),
    }
}

#[cfg(target_os = "linux")]
fn bad_request_json(message: &str) -> control_plane::HttpResponse {
    control_plane::HttpResponse {
        status_line: "HTTP/1.1 400 Bad Request",
        content_type: "application/json; charset=utf-8",
        body: format!("{{\"error\": \"{}\"}}", control_plane::json_escape(message)),
    }
}

#[cfg(target_os = "linux")]
fn is_excluded_object(index: u16) -> bool {
    matches!(
        index,
        0x1000 | 0x1008 | 0x1009 | 0x1010 | 0x1011 | 0x1018 | 0x1C12 | 0x1C13
    )
}

#[cfg(target_os = "linux")]
fn sdo_response_with_override(
    path: &str,
    dict: Option<&esc_core::obj_dict::ObjectDictionary>,
    responders: &std::sync::Mutex<Vec<esc_node::Responder>>,
    slave_index: usize,
) -> control_plane::HttpResponse {
    let (index, sub) = match parse_sdo_query(path) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    let guard = responders.lock().unwrap();
    let value = guard
        .get(slave_index)
        .and_then(|responder| responder.object_override(index, sub))
        .map(|data| data.to_vec());
    match dict
        .and_then(|dict| control_plane::sdo_json_with_value(dict, index, sub, value.as_deref()))
    {
        Some(body) => control_plane::HttpResponse {
            status_line: "HTTP/1.1 200 OK",
            content_type: "application/json; charset=utf-8",
            body,
        },
        None => control_plane::not_found_json(&format!("对象不存在：0x{index:04X}:{sub:02X}")),
    }
}

fn sdo_response(
    path: &str,
    dict: Option<&esc_core::obj_dict::ObjectDictionary>,
) -> control_plane::HttpResponse {
    let (index, sub) = match parse_sdo_query(path) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    match dict.and_then(|dict| control_plane::sdo_json(dict, index, sub)) {
        Some(body) => control_plane::HttpResponse {
            status_line: "HTTP/1.1 200 OK",
            content_type: "application/json; charset=utf-8",
            body,
        },
        None => control_plane::not_found_json(&format!("对象不存在：0x{index:04X}:{sub:02X}")),
    }
}

#[cfg(target_os = "linux")]
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(target_os = "linux")]
fn pdo_layout(
    manifest: &Option<plugin_host::manifest::PluginManifest>,
    dir: &str,
    default_address: u16,
    default_len: usize,
) -> (u16, usize) {
    manifest
        .as_ref()
        .and_then(|manifest| manifest.sm.iter().find(|sm| sm.dir == dir))
        .map(|sm| (sm.start, usize::from(sm.len)))
        .unwrap_or((default_address, default_len))
}

#[cfg(target_os = "linux")]
fn serve_live_http(
    addr: String,
    manifest: Option<plugin_host::manifest::PluginManifest>,
    esi: Option<plugin_host::esi::EsiDevice>,
    responders: std::sync::Arc<std::sync::Mutex<Vec<esc_node::Responder>>>,
    web_root: Option<String>,
    serve_write: bool,
    serve_token: Option<String>,
) {
    use std::io::Write as _;
    use std::net::TcpListener;

    let spec = match (&esi, &manifest) {
        (Some(device), Some(manifest)) => {
            plugin_host::esi::sii_spec_from_esi_and_manifest(device, manifest)
        }
        (Some(device), None) => plugin_host::esi::sii_spec_from_esi(device),
        (None, Some(manifest)) => plugin_host::sii_gen::sii_spec_from_manifest(manifest),
        (None, None) => Ok(esc_core::sii::simulated_spec()),
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(err) => {
            eprintln!("ec-sim-esc: 实时 --serve 构建快照失败：{err}");
            return;
        }
    };
    let (dict_objects, profile, dict_json, dict_for_decode) = match &manifest {
        Some(manifest) => match plugin_host::dict_gen::dictionary_from_manifest(manifest) {
            Ok(dict) => (
                Some(dict.objects().len()),
                Some((manifest.id.as_str(), manifest.name.as_str())),
                Some(control_plane::dict_json(&dict)),
                Some(dict),
            ),
            Err(err) => {
                eprintln!("ec-sim-esc: 实时 --serve 构建字典失败：{err}");
                return;
            }
        },
        None => (None, None, None, None),
    };
    let slave_count = responders.lock().unwrap().len();
    let mut histories: Vec<std::collections::VecDeque<control_plane::PdoSnapshot>> = (0
        ..slave_count)
        .map(|_| std::collections::VecDeque::with_capacity(PDO_HISTORY_CAPACITY))
        .collect();
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("ec-sim-esc: 实时 --serve 绑定 {addr} 失败：{err}");
            return;
        }
    };
    println!("SERVE (live) http://{addr} routes=/state,/dict");
    let _ = std::io::stdout().flush();
    let mut sdo_writes: u64 = 0;
    let mut sdo_audit: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(64);
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let Some(HttpRequest {
            method,
            path,
            body,
            headers,
        }) = read_http_request(&mut stream)
        else {
            continue;
        };
        let path_only = path.split('?').next().unwrap_or(path.as_str());
        if let Some((response, extra_headers)) =
            auth_gate(path_only, &headers, serve_token.as_deref())
        {
            write_http_response(&mut stream, &response, extra_headers);
            continue;
        }
        let slave_param = query_param(&path, "slave");
        let slave_count = responders.lock().unwrap().len();
        let slave_index = match slave_param {
            Some(value) => value
                .parse::<usize>()
                .ok()
                .filter(|index| *index < slave_count),
            None => Some(0),
        };
        let invalid_slave = matches!(path_only, "/state" | "/pdo" | "/metrics" | "/events")
            && slave_index.is_none();
        let slave_index = slave_index.unwrap_or(0);
        if method == "POST" && path_only == "/sdo" {
            let response = if !serve_write {
                control_plane::HttpResponse {
                    status_line: "HTTP/1.1 405 Method Not Allowed",
                    content_type: "application/json; charset=utf-8",
                    body: "{\"error\": \"写操作未启用（需要 --serve-write）\"}".to_string(),
                }
            } else {
                match parse_sdo_query(&path) {
                    Err(response) => response,
                    Ok((index, sub)) => {
                        let mut guard = responders.lock().unwrap();
                        match guard.get_mut(slave_index) {
                            None => control_plane::not_found_json("从站不存在"),
                            Some(responder) => match responder.object_writable(index, sub) {
                                None => control_plane::not_found_json(&format!(
                                    "对象不存在：0x{index:04X}:{sub:02X}"
                                )),
                                Some(false) => {
                                    forbidden_json("对象不可写（manifest 未声明 writable）")
                                }
                                Some(true) if is_excluded_object(index) => {
                                    forbidden_json("安全排除对象（身份/配置类，不允许控制面写）")
                                }
                                Some(true) => {
                                    let old_hex = responder
                                        .object_override(index, sub)
                                        .map(hex_encode)
                                        .or_else(|| {
                                            dict_for_decode.as_ref().and_then(|dict| {
                                                control_plane::sdo_value_hex(dict, index, sub)
                                            })
                                        })
                                        .unwrap_or_default();
                                    if responder.write_object_override(index, sub, &body) {
                                        let written: String =
                                            body.iter().map(|byte| format!("{byte:02x}")).collect();
                                        sdo_writes += 1;
                                        let event = format!(
                                            "{{\"ts_ms\": {}, \"slave\": {slave_index}, \"index\": \"0x{index:04X}\", \"sub\": {sub}, \"old_hex\": \"{old_hex}\", \"new_hex\": \"{written}\"}}",
                                            now_ms()
                                        );
                                        sdo_audit.push_back(event);
                                        if sdo_audit.len() > 64 {
                                            sdo_audit.pop_front();
                                        }
                                        control_plane::HttpResponse {
                                            status_line: "HTTP/1.1 200 OK",
                                            content_type: "application/json; charset=utf-8",
                                            body: format!(
                                                "{{\"ok\": true, \"slave\": {slave_index}, \"index\": \"0x{index:04X}\", \"sub\": {sub}, \"old_hex\": \"{old_hex}\", \"written_hex\": \"{written}\"}}"
                                            ),
                                        }
                                    } else {
                                        bad_request_json("值长度不匹配对象位长")
                                    }
                                }
                            },
                        }
                    }
                }
            };
            write_http_response(&mut stream, &response, "");
            continue;
        }
        if method == "POST" {
            let response = if path_only != "/pdo" {
                control_plane::not_found_json("仅支持 POST /pdo")
            } else if !serve_write {
                control_plane::HttpResponse {
                    status_line: "HTTP/1.1 405 Method Not Allowed",
                    content_type: "application/json; charset=utf-8",
                    body: "{\"error\": \"写操作未启用（需要 --serve-write）\"}".to_string(),
                }
            } else if body.is_empty() || body.len() > 1024 {
                control_plane::not_found_json("请求体为空或过长（≤1024 字节）")
            } else {
                let (sm2_address, sm2_len) = pdo_layout(&manifest, "out", 0x1100, 2);
                let write_len = body.len().min(sm2_len);
                let mut guard = responders.lock().unwrap();
                match guard.get_mut(slave_index) {
                    Some(responder) => {
                        if responder.write_process_data(sm2_address, &body[..write_len]) {
                            let written: String = body[..write_len]
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect();
                            control_plane::HttpResponse {
                                status_line: "HTTP/1.1 200 OK",
                                content_type: "application/json; charset=utf-8",
                                body: format!(
                                    "{{\"ok\": true, \"slave\": {slave_index}, \"sm2_address\": \"0x{sm2_address:04X}\", \"written_hex\": \"{written}\"}}"
                                ),
                            }
                        } else {
                            control_plane::not_found_json("写入失败（长度越界）")
                        }
                    }
                    None => control_plane::not_found_json("从站不存在"),
                }
            };
            write_http_response(&mut stream, &response, "");
            continue;
        }
        let state = {
            let guard = responders.lock().unwrap();
            match guard.get(slave_index) {
                Some(responder) => {
                    let stats = responder.stats();
                    let live = control_plane::LiveState {
                        station_address: responder.station_address(),
                        al_status: responder.al_status(),
                        al_status_code: responder.al_status_code(),
                        dc_time_ns: responder.dc_time_ns(),
                        frames_in: stats.frames_in,
                        frames_out: stats.frames_out,
                        frames_dropped: stats.frames_dropped,
                        datagrams_hit: stats.datagrams_hit,
                        mailbox_requests: stats.mailbox_requests,
                        mailbox_responses: stats.mailbox_responses,
                        dc_reg_reads: stats.dc_reg_reads,
                        dc_reg_writes: stats.dc_reg_writes,
                    };
                    control_plane::live_state_json(&spec, dict_objects, profile, &live)
                }
                None => "{\"schema\": \"ec-sim/state-v1\", \"live\": null}".to_string(),
            }
        };
        let want_history = path_only == "/pdo" && path.contains("history=1");
        let topology_body = if path_only == "/topology" {
            let (sm2_address, sm2_len) = pdo_layout(&manifest, "out", 0x1100, 2);
            let (sm3_address, sm3_len) = pdo_layout(&manifest, "in", 0x1110, 2);
            let guard = responders.lock().unwrap();
            let entries: Vec<control_plane::TopologyEntry> = guard
                .iter()
                .enumerate()
                .map(|(index, responder)| {
                    let stats = responder.stats();
                    control_plane::TopologyEntry {
                        index,
                        station_address: responder.station_address(),
                        al_status: responder.al_status(),
                        al_status_code: responder.al_status_code(),
                        frames_in: stats.frames_in,
                        frames_out: stats.frames_out,
                        sm2_address,
                        sm2_hex: hex_encode(&responder.read_process_data(sm2_address, sm2_len)),
                        sm3_address,
                        sm3_hex: hex_encode(&responder.read_process_data(sm3_address, sm3_len)),
                    }
                })
                .collect();
            Some(control_plane::topology_json(&entries))
        } else {
            None
        };
        let metrics_body = if path_only == "/metrics" {
            let guard = responders.lock().unwrap();
            let slaves: Vec<control_plane::MetricsSlave> = guard
                .iter()
                .enumerate()
                .filter(|entry| slave_param.is_none() || entry.0 == slave_index)
                .map(|(index, responder)| {
                    let stats = responder.stats();
                    control_plane::MetricsSlave {
                        index,
                        station_address: responder.station_address(),
                        al_status: responder.al_status(),
                        al_status_code: responder.al_status_code(),
                        frames_in: stats.frames_in,
                        frames_out: stats.frames_out,
                        frames_dropped: stats.frames_dropped,
                        datagrams_hit: stats.datagrams_hit,
                        mailbox_requests: stats.mailbox_requests,
                        mailbox_responses: stats.mailbox_responses,
                        dc_reg_reads: stats.dc_reg_reads,
                        dc_reg_writes: stats.dc_reg_writes,
                        dc_time_ns: responder.dc_time_ns(),
                    }
                })
                .collect();
            Some(control_plane::metrics_text_with_sdo(&slaves, sdo_writes))
        } else {
            None
        };
        let pdo_snapshot = if path_only == "/pdo" {
            let guard = responders.lock().unwrap();
            guard.get(slave_index).map(|responder| {
                let (sm2_address, sm2_len) = pdo_layout(&manifest, "out", 0x1100, 2);
                let (sm3_address, sm3_len) = pdo_layout(&manifest, "in", 0x1110, 2);
                control_plane::PdoSnapshot {
                    station_address: responder.station_address(),
                    al_status: responder.al_status(),
                    sm2_address,
                    sm2_bytes: responder.read_process_data(sm2_address, sm2_len),
                    sm3_address,
                    sm3_bytes: responder.read_process_data(sm3_address, sm3_len),
                }
            })
        } else {
            None
        };
        let pdo_response = pdo_snapshot.map(|pdo| {
            let history = &mut histories[slave_index];
            let changed = history.back().map_or(true, |last| {
                last.sm2_bytes != pdo.sm2_bytes || last.sm3_bytes != pdo.sm3_bytes
            });
            if changed {
                history.push_back(pdo.clone());
                if history.len() > PDO_HISTORY_CAPACITY {
                    history.pop_front();
                }
            }
            if want_history {
                control_plane::pdo_history_json(history.make_contiguous())
            } else {
                let decoded = control_plane::decode_pdo(
                    &spec,
                    dict_for_decode.as_ref(),
                    &pdo.sm2_bytes,
                    &pdo.sm3_bytes,
                );
                control_plane::pdo_json_decoded(&pdo, &decoded)
            }
        });
        if path_only == "/events" && !invalid_slave {
            let interval_ms = query_param(&path, "interval_ms")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(500)
                .clamp(100, 5000);
            let shared = std::sync::Arc::clone(&responders);
            let spec_snapshot = spec.clone();
            let profile_owned = manifest
                .as_ref()
                .map(|manifest| (manifest.id.clone(), manifest.name.clone()));
            let dict_objects_snapshot = dict_objects;
            let want_slave = slave_index;
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: {}\r\nConnection: keep-alive\r\n\r\n",
                control_plane::CORS_ALLOW_ORIGIN
            );
            if stream.write_all(headers.as_bytes()).is_ok() {
                let _ = stream.flush();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    loop {
                        let live = {
                            let guard = shared.lock().unwrap();
                            match guard.get(want_slave) {
                                Some(responder) => {
                                    let stats = responder.stats();
                                    control_plane::LiveState {
                                        station_address: responder.station_address(),
                                        al_status: responder.al_status(),
                                        al_status_code: responder.al_status_code(),
                                        dc_time_ns: responder.dc_time_ns(),
                                        frames_in: stats.frames_in,
                                        frames_out: stats.frames_out,
                                        frames_dropped: stats.frames_dropped,
                                        datagrams_hit: stats.datagrams_hit,
                                        mailbox_requests: stats.mailbox_requests,
                                        mailbox_responses: stats.mailbox_responses,
                                        dc_reg_reads: stats.dc_reg_reads,
                                        dc_reg_writes: stats.dc_reg_writes,
                                    }
                                }
                                None => break,
                            }
                        };
                        let state = control_plane::live_state_json(
                            &spec_snapshot,
                            dict_objects_snapshot,
                            profile_owned
                                .as_ref()
                                .map(|(id, name)| (id.as_str(), name.as_str())),
                            &live,
                        );
                        // SSE 的 data 字段不能含裸换行：压成单行 JSON。
                        let state = state.replace('\n', " ");
                        if write!(stream, "data: {state}\n\n").is_err() {
                            break;
                        }
                        if stream.flush().is_err() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                    }
                });
            }
            continue;
        }
        let response = if let Some(response) = static_web_response(&path, web_root.as_deref()) {
            response
        } else if path_only == "/sdo" {
            sdo_response_with_override(&path, dict_for_decode.as_ref(), &responders, slave_index)
        } else if path_only == "/sdo-audit" {
            let events: Vec<String> = sdo_audit.iter().cloned().collect();
            control_plane::HttpResponse {
                status_line: "HTTP/1.1 200 OK",
                content_type: "application/json; charset=utf-8",
                body: control_plane::sdo_audit_json(&events),
            }
        } else if invalid_slave {
            control_plane::not_found_json(&format!(
                "slave {} 不存在（共 {slave_count} 个从站）",
                slave_param.unwrap_or("")
            ))
        } else {
            match topology_body {
                Some(body) => control_plane::HttpResponse {
                    status_line: "HTTP/1.1 200 OK",
                    content_type: "application/json; charset=utf-8",
                    body,
                },
                None => match metrics_body {
                    Some(body) => control_plane::HttpResponse {
                        status_line: "HTTP/1.1 200 OK",
                        content_type: "text/plain; version=0.0.4; charset=utf-8",
                        body,
                    },
                    None => match pdo_response {
                        Some(body) => control_plane::HttpResponse {
                            status_line: "HTTP/1.1 200 OK",
                            content_type: "application/json; charset=utf-8",
                            body,
                        },
                        None => control_plane::handle_get(&path, &state, dict_json.as_deref()),
                    },
                },
            }
        };
        write_http_response(&mut stream, &response, "");
    }
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
    let mut serve_addr: Option<String> = None;
    let mut web_root: Option<String> = None;
    let mut serve_write = false;
    // B41：可选静态 Bearer token 的环境变量名（空/未设置 = 不鉴权，保持现状）。
    let mut serve_token_env: Option<String> = None;
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
            "--serve-write" => serve_write = true,
            "--serve-token-env" => match args.next() {
                Some(name) => serve_token_env = Some(name),
                None => {
                    eprintln!("ec-sim-esc: --serve-token-env 需要环境变量名（如 EC_SIM_TOKEN）");
                    return std::process::ExitCode::from(2);
                }
            },
            "--web-root" => match args.next() {
                Some(dir) => web_root = Some(dir),
                None => {
                    eprintln!("ec-sim-esc: --web-root 需要目录（如 web）");
                    return std::process::ExitCode::from(2);
                }
            },
            "--serve" => match args.next() {
                Some(addr) => serve_addr = Some(addr),
                None => {
                    eprintln!("ec-sim-esc: --serve 需要监听地址（如 127.0.0.1:8090）");
                    return std::process::ExitCode::from(2);
                }
            },
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
                     --serve <addr> [--web-root <dir>] [--serve-write] [--serve-token-env <VAR>]
                       HTTP JSON API（默认只读；--serve-write 开写；VAR 非空时需 Authorization: Bearer）；
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

    if serve_token_env.is_some() && serve_addr.is_none() {
        eprintln!("ec-sim-esc: --serve-token-env 需要同时给出 --serve <addr>");
        return std::process::ExitCode::from(2);
    }
    // B41：从环境变量读静态 token；未设置/为空 = 保持现状（不鉴权）。
    let serve_token: Option<String> = match serve_token_env.as_deref() {
        Some(name) => match std::env::var(name) {
            Ok(token) if !token.is_empty() => {
                println!(
                    "SERVE token_env={name} enabled（API 需要 Authorization: Bearer <token>）"
                );
                Some(token)
            }
            _ => {
                eprintln!("ec-sim-esc: --serve-token-env {name} 未设置/为空：控制面不鉴权（现状）");
                None
            }
        },
        None => None,
    };

    if dump_dict {
        return dump_dict_and_exit(manifest_path.as_deref(), esi_path.as_deref(), dump_json);
    }

    let mut live_serve: Option<String> = None;
    if let Some(addr) = serve_addr.as_deref() {
        if uds_path.is_some() || tap_name.is_some() {
            live_serve = Some(addr.to_string());
        } else {
            return serve_http(
                manifest_path.as_deref(),
                esi_path.as_deref(),
                addr,
                web_root.as_deref(),
                serve_token.as_deref(),
            );
        }
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
            live_serve,
            web_root.clone(),
            serve_write,
            serve_token.clone(),
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
            live_serve,
            web_root.clone(),
            serve_write,
            serve_token.clone(),
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
            live_serve,
            web_root.clone(),
            serve_write,
            serve_token,
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
    serve: Option<String>,
    web_root: Option<String>,
    serve_write: bool,
    serve_token: Option<String>,
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

    let http_manifest = manifest.clone();
    let http_esi = esi.clone();
    let responders = match build_responders(
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
    let responders = std::sync::Arc::new(std::sync::Mutex::new(responders));
    if let Some(addr) = serve {
        let shared = std::sync::Arc::clone(&responders);
        let root = web_root.clone();
        std::thread::spawn(move || {
            serve_live_http(
                addr,
                http_manifest,
                http_esi,
                shared,
                root,
                serve_write,
                serve_token,
            );
        });
    }
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
        let reply = {
            let mut guard = responders.lock().unwrap();
            for responder in guard.iter_mut() {
                responder.tick(now_ms());
            }
            esc_node::handle_frame_chain(&mut guard, &buf[..len], DEFAULT_SIM_MAC)
        };
        let Some(reply) = reply else {
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
        {
            let guard = responders.lock().unwrap();
            maybe_print_stats(&guard[0], verbose);
        }
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
    serve: Option<String>,
    web_root: Option<String>,
    serve_write: bool,
    serve_token: Option<String>,
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

    let http_manifest = manifest.clone();
    let http_esi = esi.clone();
    let responders = match build_responders(
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
    let responders = std::sync::Arc::new(std::sync::Mutex::new(responders));
    if let Some(addr) = serve {
        let shared = std::sync::Arc::clone(&responders);
        let root = web_root.clone();
        std::thread::spawn(move || {
            serve_live_http(
                addr,
                http_manifest,
                http_esi,
                shared,
                root,
                serve_write,
                serve_token,
            );
        });
    }
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
        let reply = {
            let mut guard = responders.lock().unwrap();
            for responder in guard.iter_mut() {
                responder.tick(now_ms());
            }
            esc_node::handle_frame_chain(&mut guard, &buf[..len], mac)
        };
        let Some(reply) = reply else {
            continue;
        };
        if let Err(err) = tap.send(&reply) {
            eprintln!("ec-sim-esc: 写回 TAP 失败: {err}");
            return std::process::ExitCode::from(6);
        }
        {
            let guard = responders.lock().unwrap();
            maybe_print_stats(&guard[0], verbose);
        }
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
    _serve: Option<String>,
    _web_root: Option<String>,
    _serve_write: bool,
    _serve_token: Option<String>,
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
    _serve: Option<String>,
    _web_root: Option<String>,
    _serve_write: bool,
    _serve_token: Option<String>,
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
