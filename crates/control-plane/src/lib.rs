//! 控制面（M2 起点）。
//!
//! 本模块先提供**只读快照 API 的最小骨架**：把 ESC 状态/对象字典序列化为稳定的 JSON
//! （`state-v1` / `dict-v1`），并提供极简路由表（`/state`、`/dict`），供后续 HTTP/JSON
//! API 与 `web/` 前端复用。当前不引入网络/序列化依赖，保持核心宽松许可与零依赖。
//!
//! 依赖方向：`control-plane` → `esc-core`。后续接入实时状态时，若需要依赖 `esc-node`，
//! 应把 CLI 二进制迁到 `cli` crate，避免 `esc-node` ↔ `control-plane` 循环依赖。

use std::fmt::Write as _;

use esc_core::obj_dict::ObjectDictionary;
use esc_core::sii::SiiSpec;

/// 只读路由表条目（M2 API 的占位；当前只有快照 JSON）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// 路径（如 `/state`）。
    pub path: &'static str,
    /// 返回的 JSON schema 名。
    pub schema: &'static str,
}

/// 已注册的只读路由。
pub const ROUTES: &[Route] = &[
    Route {
        path: "/state",
        schema: "ec-sim/state-v1",
    },
    Route {
        path: "/dict",
        schema: "ec-sim/dict-v1",
    },
];

/// 按路径查找路由；未注册返回 `None`。
#[must_use]
pub fn route(path: &str) -> Option<&'static Route> {
    ROUTES.iter().find(|route| route.path == path)
}

/// JSON 字符串转义（只处理常用控制字符；输出为机器可读 JSON）。
#[must_use]
pub fn json_escape(text: &str) -> String {
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

/// 生成 `state-v1` JSON：SII 画像的只读状态快照（身份/SM/PDO/DC 等）。
///
/// - `dict_objects`：manifest 画像的运行时字典对象数（无 manifest 时为 `None`）；
/// - `profile`：manifest 画像的 `(id, name)`（无 manifest 时为 `None`）。
#[must_use]
pub fn state_json(
    spec: &SiiSpec,
    dict_objects: Option<usize>,
    profile: Option<(&str, &str)>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"schema\": \"ec-sim/state-v1\",");
    let _ = writeln!(out, "  \"tool\": \"ec-sim-esc {}\",", crate::TOOL_VERSION);
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
        let tail = if index + 1 == spec.sync_managers.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            out,
            "    {{\"start\": \"0x{:04X}\", \"length\": {}, \"control\": \"0x{:02X}\", \"status\": {}, \"enable\": {}}}{tail}",
            sm.physical_start_address, sm.length, sm.control, sm.status, sm.enable
        );
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
    match profile {
        Some((id, name)) => {
            let _ = writeln!(
                out,
                "  \"profile\": {{\"id\": \"{}\", \"name\": \"{}\"}},",
                json_escape(id),
                json_escape(name)
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
    out
}

/// 生成 `dict-v1` JSON：运行时对象字典的只读快照（对象/条目/类型/访问/值 hex）。
#[must_use]
pub fn dict_json(dict: &ObjectDictionary) -> String {
    let entry_count: usize = dict
        .objects()
        .iter()
        .map(|object| usize::from(object.max_subindex) + 1)
        .sum();
    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"schema\": \"ec-sim/dict-v1\",");
    let _ = writeln!(out, "  \"tool\": \"ec-sim-esc {}\",", crate::TOOL_VERSION);
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
    out
}

/// 工具版本（随包版本）。
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 允许跨源读取（只读接口；默认允许任意源，便于本机 Web 页面直接 fetch）。
pub const CORS_ALLOW_ORIGIN: &str = "*";

/// 只读 HTTP 响应（手写，无外部依赖）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// 状态行（如 `HTTP/1.1 200 OK`）。
    pub status_line: &'static str,
    /// Content-Type。
    pub content_type: &'static str,
    /// 响应体。
    pub body: String,
}

fn json_response(status_line: &'static str, body: String) -> HttpResponse {
    HttpResponse {
        status_line,
        content_type: "application/json; charset=utf-8",
        body,
    }
}

/// 生成 404 JSON 响应（未知路径/越界从站等）。
#[must_use]
pub fn not_found_json(message: &str) -> HttpResponse {
    let body = format!(
        "{{\"error\": \"{}\", \"routes\": [\"/state\", \"/dict\"]}}",
        json_escape(message)
    );
    json_response("HTTP/1.1 404 Not Found", body)
}

/// 需要鉴权的控制面 API 路由（B41：可选静态 Bearer token）。
///
/// 静态资源（`/`、`/index.html`、`/app.js`、`/health`）不在列表内：它们用于加载 WebUI 外壳，
/// token 只保护会泄漏/改变 ESC 状态的 API；WebUI 在 token 模式下需要经反向代理注入请求头或改用 curl。
pub const AUTH_PROTECTED_ROUTES: [&str; 8] = [
    "/state",
    "/dict",
    "/pdo",
    "/sdo",
    "/sdo-audit",
    "/topology",
    "/metrics",
    "/events",
];

/// 判断路径（可带 query）是否属于需要鉴权的 API 路由。
#[must_use]
pub fn is_protected_route(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    AUTH_PROTECTED_ROUTES.contains(&path)
}

/// 控制面鉴权决策（B41：`--serve-token-env VAR` 读到的可选静态 token）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthDecision {
    /// 未配置 token（变量缺失/为空）：保持现状，不鉴权。
    Disabled,
    /// `Authorization` 头合法且 token 匹配。
    Allowed,
    /// 缺少或格式非法的 `Authorization: Bearer <token>` 头 → 401。
    MissingToken,
    /// Bearer token 不匹配 → 403。
    InvalidToken,
}

/// 取请求头中的 `Authorization: Bearer <token>`（头名与 scheme 大小写不敏感）。
#[must_use]
pub fn bearer_token(headers: &[(String, String)]) -> Option<&str> {
    headers.iter().find_map(|(name, value)| {
        if !name.eq_ignore_ascii_case("authorization") {
            return None;
        }
        let (scheme, token) = value.trim().split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return None;
        }
        let token = token.trim();
        if token.is_empty() {
            None
        } else {
            Some(token)
        }
    })
}

/// 鉴权判定：`expected` 为 `None` 或空串时维持现状（不鉴权）；否则要求 Bearer token 精确匹配。
#[must_use]
pub fn authorize(headers: &[(String, String)], expected: Option<&str>) -> AuthDecision {
    let Some(expected) = expected.filter(|token| !token.is_empty()) else {
        return AuthDecision::Disabled;
    };
    match bearer_token(headers) {
        None => AuthDecision::MissingToken,
        Some(token) if token == expected => AuthDecision::Allowed,
        Some(_) => AuthDecision::InvalidToken,
    }
}

/// 生成 401 响应（缺少/格式非法的 Authorization 头；调用方需补 `WWW-Authenticate: Bearer`）。
#[must_use]
pub fn unauthorized_json() -> HttpResponse {
    json_response(
        "HTTP/1.1 401 Unauthorized",
        "{\"error\": \"需要 Authorization: Bearer <token>\"}".to_string(),
    )
}

/// 生成 403 响应（Bearer token 不匹配）。
#[must_use]
pub fn forbidden_token_json() -> HttpResponse {
    json_response(
        "HTTP/1.1 403 Forbidden",
        "{\"error\": \"Bearer token 不匹配\"}".to_string(),
    )
}

/// 处理一个只读 GET 请求（供 `ec-sim-esc --serve` 使用；无状态、可单元测试）。
///
/// 路由：`/state`、`/dict`（无字典时 404）、`/` 与 `/health` 返回路由列表；其余 404。
#[must_use]
pub fn handle_get(path: &str, state: &str, dict: Option<&str>) -> HttpResponse {
    let path = path.split('?').next().unwrap_or(path);
    match path {
        "/state" => json_response("HTTP/1.1 200 OK", state.to_string()),
        "/dict" => match dict {
            Some(body) => json_response("HTTP/1.1 200 OK", body.to_string()),
            None => not_found_json("dict 不可用（未加载 manifest）"),
        },
        "/" | "/health" => json_response(
            "HTTP/1.1 200 OK",
            "{\"service\": \"ec-sim-control-plane\", \"routes\": [\"/state\", \"/dict\"]}"
                .to_string(),
        ),
        _ => not_found_json("unknown path"),
    }
}

/// 运行时实时状态（`--serve` 实时模式从 `Responder` 读取；只读快照，非实时承诺）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveState {
    /// 配置站地址。
    pub station_address: u16,
    /// AL Status（0x0130）。
    pub al_status: u8,
    /// AL Status Code（0x0134）。
    pub al_status_code: u16,
    /// 模拟 DC 系统时间（ns，帧驱动）。
    pub dc_time_ns: u64,
    /// 收帧数。
    pub frames_in: u64,
    /// 发帧数。
    pub frames_out: u64,
    /// 丢弃帧数。
    pub frames_dropped: u64,
    /// 命中 datagram 数。
    pub datagrams_hit: u64,
    /// 邮箱请求数。
    pub mailbox_requests: u64,
    /// 邮箱响应数。
    pub mailbox_responses: u64,
    /// DC 寄存器读次数。
    pub dc_reg_reads: u64,
    /// DC 寄存器写次数。
    pub dc_reg_writes: u64,
}

/// 生成带 `live` 字段的 state-v1 JSON（在 `state_json` 基础上插入运行时状态）。
#[must_use]
pub fn live_state_json(
    spec: &SiiSpec,
    dict_objects: Option<usize>,
    profile: Option<(&str, &str)>,
    live: &LiveState,
) -> String {
    let mut out = state_json(spec, dict_objects, profile);
    let insert_at = out.rfind('}').unwrap_or(out.len());
    let live_json = format!(
        ",\n  \"live\": {{\"station_address\": \"0x{:04X}\", \"al_status\": \"0x{:02X}\", \"al_status_code\": \"0x{:04X}\", \"dc_time_ns\": {}, \"stats\": {{\"frames_in\": {}, \"frames_out\": {}, \"frames_dropped\": {}, \"datagrams_hit\": {}, \"mailbox_requests\": {}, \"mailbox_responses\": {}, \"dc_reg_reads\": {}, \"dc_reg_writes\": {}}}}}\n",
        live.station_address,
        live.al_status,
        live.al_status_code,
        live.dc_time_ns,
        live.frames_in,
        live.frames_out,
        live.frames_dropped,
        live.datagrams_hit,
        live.mailbox_requests,
        live.mailbox_responses,
        live.dc_reg_reads,
        live.dc_reg_writes
    );
    out.insert_str(insert_at, &live_json);
    out
}

/// 过程数据（PDO）只读快照（SM2 输出 / SM3 输入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdoSnapshot {
    /// 配置站地址。
    pub station_address: u16,
    /// AL Status（0x0130）。
    pub al_status: u8,
    /// SM2（RxPDO）物理地址。
    pub sm2_address: u16,
    /// SM2 原始字节。
    pub sm2_bytes: Vec<u8>,
    /// SM3（TxPDO）物理地址。
    pub sm3_address: u16,
    /// SM3 原始字节。
    pub sm3_bytes: Vec<u8>,
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 生成 `pdo-v1` JSON：SM2/SM3 过程数据原始字节（含地址/长度/hex）。
#[must_use]
pub fn pdo_json(pdo: &PdoSnapshot) -> String {
    format!(
        "{{\n  \"schema\": \"ec-sim/pdo-v1\",\n  \"tool\": \"ec-sim-esc {}\",\n  \"station_address\": \"0x{:04X}\",\n  \"al_status\": \"0x{:02X}\",\n  \"sm2\": {{\"address\": \"0x{:04X}\", \"bytes\": {}, \"hex\": \"{}\"}},\n  \"sm3\": {{\"address\": \"0x{:04X}\", \"bytes\": {}, \"hex\": \"{}\"}}\n}}",
        TOOL_VERSION,
        pdo.station_address,
        pdo.al_status,
        pdo.sm2_address,
        pdo.sm2_bytes.len(),
        hex_bytes(&pdo.sm2_bytes),
        pdo.sm3_address,
        pdo.sm3_bytes.len(),
        hex_bytes(&pdo.sm3_bytes)
    )
}

/// 生成 `pdo-history-v1` JSON：PDO 时间序列（每次 `/pdo` 请求按变化去重追加，最多 256 条）。
#[must_use]
pub fn pdo_history_json(samples: &[PdoSnapshot]) -> String {
    let mut out = format!(
        "{{\n  \"schema\": \"ec-sim/pdo-history-v1\",\n  \"tool\": \"ec-sim-esc {}\",\n  \"count\": {},\n  \"samples\": [\n",
        TOOL_VERSION,
        samples.len()
    );
    for (index, sample) in samples.iter().enumerate() {
        let tail = if index + 1 == samples.len() { "" } else { "," };
        let _ = writeln!(
            out,
            "    {{\"sm2_hex\": \"{}\", \"sm3_hex\": \"{}\", \"al_status\": \"0x{:02X}\"}}{tail}",
            hex_bytes(&sample.sm2_bytes),
            hex_bytes(&sample.sm3_bytes),
            sample.al_status
        );
    }
    out.push_str("  ]\n}");
    out
}

/// PDO 解码条目（按映射顺序把原始字节解释为具名值；只读）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdoDecodedEntry {
    /// `"rx"`（SM2）或 `"tx"`（SM3）。
    pub direction: &'static str,
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub sub: u8,
    /// 位长度。
    pub bits: u8,
    /// 对象名（字典有则用字典名，否则 `0xIIII:SS`）。
    pub name: String,
    /// 解码值（有符号类型按符号扩展）。
    pub value: i64,
}

fn read_bits(bytes: &[u8], bit_offset: usize, bits: usize) -> Option<u64> {
    if bits == 0 || bits > 32 || bit_offset % 8 != 0 {
        return None;
    }
    let start = bit_offset / 8;
    let need = bits.div_ceil(8);
    if start + need > bytes.len() {
        return None;
    }
    let mut raw = 0u64;
    for index in 0..need {
        raw |= u64::from(bytes[start + index]) << (8 * index);
    }
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    Some(raw & mask)
}

fn sign_extend(raw: u64, bits: usize) -> i64 {
    if bits == 0 || bits >= 64 {
        return raw as i64;
    }
    let shift = 64 - bits;
    ((raw << shift) as i64) >> shift
}

/// 按 SII 的 TxPDO/RxPDO 映射顺序解码 SM2/SM3 原始字节为具名对象值。
///
/// 只支持字节对齐、位长 ≤32 的条目；超出项跳过（Tier0 简化）。
#[must_use]
pub fn decode_pdo(
    spec: &SiiSpec,
    dict: Option<&ObjectDictionary>,
    sm2: &[u8],
    sm3: &[u8],
) -> Vec<PdoDecodedEntry> {
    use esc_core::obj_dict::data_type;

    let mut decoded = Vec::new();
    for (direction, pdos, bytes) in [("rx", &spec.rx_pdos, sm2), ("tx", &spec.tx_pdos, sm3)] {
        let mut bit_offset = 0usize;
        for pdo in pdos {
            for mapping in &pdo.entries {
                let bits = usize::from(mapping.bit_length);
                if let Some(raw) = read_bits(bytes, bit_offset, bits) {
                    let dict_entry =
                        dict.and_then(|dict| dict.entry(mapping.index, mapping.subindex));
                    let (name, signed) = match dict_entry {
                        Some(entry) => (
                            entry.name.clone(),
                            matches!(
                                entry.data_type,
                                data_type::INTEGER8 | data_type::INTEGER16 | data_type::INTEGER32
                            ),
                        ),
                        None => (
                            format!("0x{:04X}:{:02X}", mapping.index, mapping.subindex),
                            false,
                        ),
                    };
                    let value = if signed {
                        sign_extend(raw, bits)
                    } else {
                        raw as i64
                    };
                    decoded.push(PdoDecodedEntry {
                        direction,
                        index: mapping.index,
                        sub: mapping.subindex,
                        bits: mapping.bit_length,
                        name,
                        value,
                    });
                }
                bit_offset += bits;
            }
        }
    }
    decoded
}

fn decoded_json(decoded: &[PdoDecodedEntry]) -> String {
    let mut out = String::new();
    for (index, entry) in decoded.iter().enumerate() {
        let tail = if index + 1 == decoded.len() { "" } else { "," };
        let _ = write!(
            out,
            "\n    {{\"dir\": \"{}\", \"index\": \"0x{:04X}\", \"sub\": {}, \"bits\": {}, \"name\": \"{}\", \"value\": {}}}{tail}",
            entry.direction,
            entry.index,
            entry.sub,
            entry.bits,
            json_escape(&entry.name),
            entry.value
        );
    }
    out
}

/// 生成带 `decoded` 数组的 pdo-v1 JSON（在 `pdo_json` 基础上追加）。
#[must_use]
pub fn pdo_json_decoded(pdo: &PdoSnapshot, decoded: &[PdoDecodedEntry]) -> String {
    let base = pdo_json(pdo);
    let insert_at = base.rfind('}').unwrap_or(base.len());
    let insert = format!(",\n  \"decoded\": [{}]\n", decoded_json(decoded));
    let mut out = base;
    out.insert_str(insert_at, &insert);
    out
}

/// 多从站拓扑条目（每站只读摘要）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyEntry {
    /// 链上序号（0 = 首个从站）。
    pub index: usize,
    /// 配置站地址。
    pub station_address: u16,
    /// AL Status。
    pub al_status: u8,
    /// AL Status Code。
    pub al_status_code: u16,
    /// 收帧数。
    pub frames_in: u64,
    /// 发帧数。
    pub frames_out: u64,
    /// SM2 物理地址。
    pub sm2_address: u16,
    /// SM2 当前字节 hex。
    pub sm2_hex: String,
    /// SM3 物理地址。
    pub sm3_address: u16,
    /// SM3 当前字节 hex。
    pub sm3_hex: String,
}

/// 生成 `topology-v1` JSON：链上所有从站摘要。
#[must_use]
pub fn topology_json(slaves: &[TopologyEntry]) -> String {
    let mut out = format!(
        "{{\n  \"schema\": \"ec-sim/topology-v1\",\n  \"tool\": \"ec-sim-esc {}\",\n  \"count\": {},\n  \"slaves\": [\n",
        TOOL_VERSION,
        slaves.len()
    );
    for (position, slave) in slaves.iter().enumerate() {
        let tail = if position + 1 == slaves.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            out,
            "    {{\"index\": {}, \"station_address\": \"0x{:04X}\", \"al_status\": \"0x{:02X}\", \"al_status_code\": \"0x{:04X}\", \"frames_in\": {}, \"frames_out\": {}, \"sm2\": {{\"address\": \"0x{:04X}\", \"hex\": \"{}\"}}, \"sm3\": {{\"address\": \"0x{:04X}\", \"hex\": \"{}\"}}}}{tail}",
            slave.index,
            slave.station_address,
            slave.al_status,
            slave.al_status_code,
            slave.frames_in,
            slave.frames_out,
            slave.sm2_address,
            slave.sm2_hex,
            slave.sm3_address,
            slave.sm3_hex
        );
    }
    out.push_str("  ]\n}");
    out
}

/// 单从站指标（`/metrics` 文本导出用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSlave {
    /// 链上序号。
    pub index: usize,
    /// 配置站地址。
    pub station_address: u16,
    /// AL Status。
    pub al_status: u8,
    /// AL Status Code。
    pub al_status_code: u16,
    /// 收帧数。
    pub frames_in: u64,
    /// 发帧数。
    pub frames_out: u64,
    /// 丢弃帧数。
    pub frames_dropped: u64,
    /// datagram 命中数。
    pub datagrams_hit: u64,
    /// 邮箱请求数。
    pub mailbox_requests: u64,
    /// 邮箱响应数。
    pub mailbox_responses: u64,
    /// DC 寄存器读次数。
    pub dc_reg_reads: u64,
    /// DC 寄存器写次数。
    pub dc_reg_writes: u64,
    /// 模拟 DC 时间（ns）。
    pub dc_time_ns: u64,
}

/// 计数器取值函数。
type MetricGetter = fn(&MetricsSlave) -> u64;

/// 计数器描述：`(名称, 说明, 取值函数)`。
type MetricSpec = (&'static str, &'static str, MetricGetter);

/// 生成 Prometheus 文本指标（每站带 `slave` / `station` 标签；只读、无外部依赖）。
#[must_use]
pub fn metrics_text(slaves: &[MetricsSlave]) -> String {
    let labels = |slave: &MetricsSlave| {
        format!(
            "slave=\"{}\",station_address=\"0x{:04X}\"",
            slave.index, slave.station_address
        )
    };
    let mut out = String::new();
    let counters: [MetricSpec; 9] = [
        (
            "ecsim_frames_in_total",
            "收到的 EtherCAT 帧总数",
            |slave| slave.frames_in,
        ),
        (
            "ecsim_frames_out_total",
            "发出的 EtherCAT 帧总数",
            |slave| slave.frames_out,
        ),
        ("ecsim_frames_dropped_total", "丢弃的帧总数", |slave| {
            slave.frames_dropped
        }),
        (
            "ecsim_datagrams_hit_total",
            "命中的 datagram 总数",
            |slave| slave.datagrams_hit,
        ),
        (
            "ecsim_mailbox_requests_total",
            "邮箱请求总数",
            |slave| slave.mailbox_requests,
        ),
        (
            "ecsim_mailbox_responses_total",
            "邮箱响应总数",
            |slave| slave.mailbox_responses,
        ),
        (
            "ecsim_dc_reg_reads_total",
            "DC 寄存器读次数",
            |slave| slave.dc_reg_reads,
        ),
        (
            "ecsim_dc_reg_writes_total",
            "DC 寄存器写次数",
            |slave| slave.dc_reg_writes,
        ),
        (
            "ecsim_dc_time_ns",
            "模拟 DC 系统时间（ns，帧驱动）",
            |slave| slave.dc_time_ns,
        ),
    ];
    for (name, help, get) in counters {
        let kind = if name.ends_with("_total") {
            "counter"
        } else {
            "gauge"
        };
        let _ = writeln!(out, "# HELP {name} {help}");
        let _ = writeln!(out, "# TYPE {name} {kind}");
        for slave in slaves {
            let _ = writeln!(out, "{name}{{{}}} {}", labels(slave), get(slave));
        }
    }
    let _ = writeln!(out, "# HELP ecsim_al_status 当前 AL 状态（低 4 位）");
    let _ = writeln!(out, "# TYPE ecsim_al_status gauge");
    let _ = writeln!(out, "# HELP ecsim_al_status_code AL Status Code");
    let _ = writeln!(out, "# TYPE ecsim_al_status_code gauge");
    for slave in slaves {
        let _ = writeln!(
            out,
            "ecsim_al_status{{{}}} {}",
            labels(slave),
            slave.al_status
        );
        let _ = writeln!(
            out,
            "ecsim_al_status_code{{{}}} {}",
            labels(slave),
            slave.al_status_code
        );
    }
    out
}

/// 读取对象的默认值 hex（字典当前值，不含控制面覆盖值）。
#[must_use]
pub fn sdo_value_hex(dict: &ObjectDictionary, index: u16, sub: u8) -> Option<String> {
    dict.entry(index, sub).map(|entry| {
        entry
            .value
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
}

/// 生成 `sdo-v1` JSON：从运行时对象字典读取单个条目（不经过 EtherCAT mailbox）。
///
/// 未知对象/子索引返回 `None`。
#[must_use]
pub fn sdo_json(dict: &ObjectDictionary, index: u16, sub: u8) -> Option<String> {
    sdo_json_with_value(dict, index, sub, None)
}

/// 同 [`sdo_json`]，但可用 `value` 覆盖当前值（控制面写入后的运行时覆盖值）。
#[must_use]
pub fn sdo_json_with_value(
    dict: &ObjectDictionary,
    index: u16,
    sub: u8,
    value: Option<&[u8]>,
) -> Option<String> {
    let entry = dict.entry(index, sub)?;
    let object_name = dict
        .objects()
        .iter()
        .find(|object| object.index == index)
        .map(|object| object.name.as_str())
        .unwrap_or("");
    let value_bytes = value.unwrap_or(&entry.value);
    let value_hex: String = value_bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Some(format!(
        "{{\n  \"schema\": \"ec-sim/sdo-v1\",\n  \"tool\": \"ec-sim-esc {}\",\n  \"index\": \"0x{:04X}\",\n  \"sub\": {},\n  \"object_name\": \"{}\",\n  \"name\": \"{}\",\n  \"type\": \"0x{:04X}\",\n  \"bits\": {},\n  \"access\": \"{}\",\n  \"value_hex\": \"{}\"\n}}",
        TOOL_VERSION,
        index,
        sub,
        json_escape(object_name),
        json_escape(&entry.name),
        entry.data_type,
        entry.bit_length,
        if entry.writable { "rw" } else { "ro" },
        value_hex
    ))
}

/// 生成 `sdo-audit-v1` JSON：控制面 SDO 写审计事件（`events` 为已序列化的 JSON 对象字符串）。
#[must_use]
pub fn sdo_audit_json(events: &[String]) -> String {
    let mut out = format!(
        "{{\n  \"schema\": \"ec-sim/sdo-audit-v1\",\n  \"tool\": \"ec-sim-esc {}\",\n  \"count\": {},\n  \"events\": [\n",
        TOOL_VERSION,
        events.len()
    );
    for (index, event) in events.iter().enumerate() {
        let tail = if index + 1 == events.len() { "" } else { "," };
        let _ = writeln!(out, "    {event}{tail}");
    }
    out.push_str("  ]\n}");
    out
}

/// 同 [`metrics_text`]，附带控制面 SDO 写计数。
#[must_use]
pub fn metrics_text_with_sdo(slaves: &[MetricsSlave], sdo_writes: u64) -> String {
    let mut out = metrics_text(slaves);
    let _ = writeln!(out, "# HELP ecsim_sdo_writes_total 控制面 SDO 写操作次数");
    let _ = writeln!(out, "# TYPE ecsim_sdo_writes_total counter");
    let _ = writeln!(out, "ecsim_sdo_writes_total {sdo_writes}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use esc_core::sii::simulated_spec;

    #[test]
    fn http_get_routes_state_dict_and_404() {
        let state = "{\"schema\": \"ec-sim/state-v1\"}";
        let dict = "{\"schema\": \"ec-sim/dict-v1\"}";
        let ok_state = handle_get("/state", state, Some(dict));
        assert_eq!(ok_state.status_line, "HTTP/1.1 200 OK");
        assert_eq!(ok_state.body, state);
        assert_eq!(ok_state.content_type, "application/json; charset=utf-8");

        let ok_dict = handle_get("/dict?x=1", state, Some(dict));
        assert_eq!(ok_dict.status_line, "HTTP/1.1 200 OK");
        assert_eq!(ok_dict.body, dict);

        let missing_dict = handle_get("/dict", state, None);
        assert_eq!(missing_dict.status_line, "HTTP/1.1 404 Not Found");
        assert!(missing_dict
            .body
            .contains("\"routes\": [\"/state\", \"/dict\"]"));

        let unknown = handle_get("/nope", state, Some(dict));
        assert_eq!(unknown.status_line, "HTTP/1.1 404 Not Found");

        let health = handle_get("/health", state, Some(dict));
        assert_eq!(health.status_line, "HTTP/1.1 200 OK");
        assert!(health.body.contains("ec-sim-control-plane"));
    }
    #[test]
    fn live_state_json_injects_runtime_state() {
        let live = LiveState {
            station_address: 0x1001,
            al_status: 0x04,
            al_status_code: 0x0000,
            dc_time_ns: 1234567,
            frames_in: 42,
            frames_out: 41,
            frames_dropped: 1,
            datagrams_hit: 84,
            mailbox_requests: 2,
            mailbox_responses: 2,
            dc_reg_reads: 3,
            dc_reg_writes: 4,
        };
        let json = live_state_json(&simulated_spec(), Some(15), Some(("p", "P")), &live);
        assert!(json.contains("\"schema\": \"ec-sim/state-v1\""));
        assert!(json.contains("\"live\": {"));
        assert!(json.contains("\"station_address\": \"0x1001\""));
        assert!(json.contains("\"al_status\": \"0x04\""));
        assert!(json.contains("\"dc_time_ns\": 1234567"));
        assert!(json.contains("\"frames_in\": 42"));
        assert!(json.contains("\"dc_reg_writes\": 4"));
        assert!(json.trim_end().ends_with('}'));
    }
    #[test]
    fn pdo_json_serializes_sm2_and_sm3() {
        let pdo = PdoSnapshot {
            station_address: 0x1001,
            al_status: 0x02,
            sm2_address: 0x1100,
            sm2_bytes: vec![0xde, 0xad, 0xbe, 0xef],
            sm3_address: 0x1110,
            sm3_bytes: vec![0x00, 0x01],
        };
        let json = pdo_json(&pdo);
        assert!(json.contains("\"schema\": \"ec-sim/pdo-v1\""));
        assert!(json
            .contains("\"sm2\": {\"address\": \"0x1100\", \"bytes\": 4, \"hex\": \"deadbeef\"}"));
        assert!(
            json.contains("\"sm3\": {\"address\": \"0x1110\", \"bytes\": 2, \"hex\": \"0001\"}")
        );
        assert!(json.contains("\"al_status\": \"0x02\""));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn pdo_history_json_lists_samples() {
        let samples = vec![
            PdoSnapshot {
                station_address: 0x1001,
                al_status: 0x01,
                sm2_address: 0x1100,
                sm2_bytes: vec![0x00, 0x00],
                sm3_address: 0x1110,
                sm3_bytes: vec![0x00, 0x00],
            },
            PdoSnapshot {
                station_address: 0x1001,
                al_status: 0x08,
                sm2_address: 0x1100,
                sm2_bytes: vec![0xbe, 0xef],
                sm3_address: 0x1110,
                sm3_bytes: vec![0x12, 0x34],
            },
        ];
        let json = pdo_history_json(&samples);
        assert!(json.contains("\"schema\": \"ec-sim/pdo-history-v1\""));
        assert!(json.contains("\"count\": 2"));
        assert!(json.contains("\"sm2_hex\": \"0000\""));
        assert!(json.contains("\"sm2_hex\": \"beef\", \"sm3_hex\": \"1234\""));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn decode_pdo_reads_simulated_mapping_in_order() {
        use esc_core::sii::simulated_spec;
        let spec = simulated_spec();
        let dict = ObjectDictionary::simulated();
        let sm2 = vec![0x01; 16];
        let sm3 = vec![0x02; 16];
        let decoded = decode_pdo(&spec, Some(&dict), &sm2, &sm3);
        let expected = spec
            .rx_pdos
            .iter()
            .map(|pdo| pdo.entries.len())
            .sum::<usize>()
            + spec
                .tx_pdos
                .iter()
                .map(|pdo| pdo.entries.len())
                .sum::<usize>();
        assert_eq!(decoded.len(), expected);
        assert!(decoded.iter().any(|entry| entry.direction == "rx"));
        assert!(decoded.iter().any(|entry| entry.direction == "tx"));
        // 首条映射值 = 0x01（8 位无符号）。
        let first = &decoded[0];
        assert_eq!(first.value, 1);
        assert!(first.name.contains("0x") || !first.name.is_empty());
    }

    #[test]
    fn sign_extend_and_read_bits_handle_signed_values() {
        assert_eq!(sign_extend(0xFFFF, 16), -1);
        assert_eq!(sign_extend(0x8000, 16), i16::MIN as i64);
        assert_eq!(sign_extend(0x7F, 8), 127);
        assert_eq!(read_bits(&[0x34, 0x12], 0, 16), Some(0x1234));
        assert_eq!(read_bits(&[0x34, 0x12], 8, 16), None);
        assert_eq!(read_bits(&[0x01], 0, 8), Some(1));
    }

    #[test]
    fn pdo_json_decoded_appends_named_values() {
        let pdo = PdoSnapshot {
            station_address: 0x1001,
            al_status: 0x02,
            sm2_address: 0x1100,
            sm2_bytes: vec![0x0F, 0x00],
            sm3_address: 0x1110,
            sm3_bytes: vec![0x37, 0x02],
        };
        let decoded = vec![PdoDecodedEntry {
            direction: "rx",
            index: 0x6040,
            sub: 0,
            bits: 16,
            name: "Controlword".to_string(),
            value: 15,
        }];
        let json = pdo_json_decoded(&pdo, &decoded);
        assert!(json.contains("\"schema\": \"ec-sim/pdo-v1\""));
        assert!(json.contains("\"decoded\": ["));
        assert!(json.contains("\"index\": \"0x6040\""));
        assert!(json.contains("\"value\": 15"));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn topology_json_lists_all_slaves() {
        let entry = |index: usize, station: u16| TopologyEntry {
            index,
            station_address: station,
            al_status: 0x01,
            al_status_code: 0,
            frames_in: 10 + index as u64,
            frames_out: 9,
            sm2_address: 0x1100,
            sm2_hex: "0000".to_string(),
            sm3_address: 0x1110,
            sm3_hex: "0000".to_string(),
        };
        let json = topology_json(&[entry(0, 1), entry(1, 2), entry(2, 3)]);
        assert!(json.contains("\"schema\": \"ec-sim/topology-v1\""));
        assert!(json.contains("\"count\": 3"));
        assert!(json.contains("\"station_address\": \"0x0001\""));
        assert!(json.contains("\"station_address\": \"0x0002\""));
        assert!(json.contains("\"station_address\": \"0x0003\""));
        assert!(json.contains("\"index\": 2"));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn metrics_text_has_help_type_and_labels() {
        let slave = |index: usize, station: u16| MetricsSlave {
            index,
            station_address: station,
            al_status: 0x08,
            al_status_code: 0,
            frames_in: 10 + index as u64,
            frames_out: 9,
            frames_dropped: 1,
            datagrams_hit: 20,
            mailbox_requests: 2,
            mailbox_responses: 2,
            dc_reg_reads: 3,
            dc_reg_writes: 4,
            dc_time_ns: 1234,
        };
        let text = metrics_text(&[slave(0, 1), slave(1, 2)]);
        assert!(text.contains("# TYPE ecsim_frames_in_total counter"));
        assert!(text.contains("# HELP ecsim_frames_in_total"));
        assert!(text.contains("ecsim_frames_in_total{slave=\"0\",station_address=\"0x0001\"} 10"));
        assert!(text.contains("ecsim_frames_in_total{slave=\"1\",station_address=\"0x0002\"} 11"));
        assert!(text.contains("ecsim_al_status{slave=\"0\",station_address=\"0x0001\"} 8"));
        assert!(text.contains("ecsim_dc_time_ns{"));
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("ecsim_frames_in_total{"))
                .count(),
            2
        );
    }

    #[test]
    fn sdo_json_reads_known_object_and_rejects_unknown() {
        let dict = ObjectDictionary::simulated();
        let json = sdo_json(&dict, 0x1018, 1).expect("0x1018:01 应存在");
        assert!(json.contains("\"schema\": \"ec-sim/sdo-v1\""));
        assert!(json.contains("\"index\": \"0x1018\""));
        assert!(json.contains("\"sub\": 1"));
        assert!(json.contains("\"name\": \"Vendor ID\""));
        assert!(json.contains("\"access\": \"ro\""));
        assert!(json.contains("\"value_hex\": \""));
        assert!(sdo_json(&dict, 0xFFFF, 0).is_none());
        assert!(sdo_json(&dict, 0x1018, 9).is_none());
    }

    #[test]
    fn sdo_audit_and_sdo_write_metric_serialize() {
        let events = vec![
            "{\"index\": \"0x6040\", \"sub\": 0, \"old_hex\": \"0000\", \"new_hex\": \"0f00\"}"
                .to_string(),
        ];
        let json = sdo_audit_json(&events);
        assert!(json.contains("\"schema\": \"ec-sim/sdo-audit-v1\""));
        assert!(json.contains("\"count\": 1"));
        assert!(json.contains("\"new_hex\": \"0f00\""));
        assert!(json.trim_end().ends_with('}'));

        let text = metrics_text_with_sdo(
            &[MetricsSlave {
                index: 0,
                station_address: 1,
                al_status: 8,
                al_status_code: 0,
                frames_in: 1,
                frames_out: 1,
                frames_dropped: 0,
                datagrams_hit: 1,
                mailbox_requests: 0,
                mailbox_responses: 0,
                dc_reg_reads: 0,
                dc_reg_writes: 0,
                dc_time_ns: 0,
            }],
            3,
        );
        assert!(text.contains("# TYPE ecsim_sdo_writes_total counter"));
        assert!(text.contains("ecsim_sdo_writes_total 3"));
    }

    #[test]
    fn routes_are_registered_and_dispatch() {
        assert_eq!(
            route("/state").map(|route| route.schema),
            Some("ec-sim/state-v1")
        );
        assert_eq!(
            route("/dict").map(|route| route.schema),
            Some("ec-sim/dict-v1")
        );
        assert!(route("/nope").is_none());
        assert_eq!(ROUTES.len(), 2);
    }

    #[test]
    fn state_json_contains_identity_sm_pdo_and_profile() {
        let spec = simulated_spec();
        let json = state_json(&spec, Some(15), Some(("test.profile", "Test (SIMULATED)")));
        assert!(json.contains("\"schema\": \"ec-sim/state-v1\""));
        assert!(json.contains(&format!("\"vendor_id\": \"0x{:08X}\"", spec.vendor_id)));
        assert!(json.contains("\"sync_managers\": ["));
        assert!(json.contains("\"pdo_tx\": ["));
        assert!(json.contains("\"pdo_rx\": ["));
        assert!(json.contains("\"profile\": {\"id\": \"test.profile\""));
        assert!(json.contains("\"dict_objects\": 15"));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn state_json_handles_builtin_without_profile_or_dict() {
        let json = state_json(&simulated_spec(), None, None);
        assert!(json.contains("\"profile\": null"));
        assert!(json.contains("\"dict_objects\": null"));
    }

    #[test]
    fn dict_json_matches_object_dictionary_entries() {
        let dict = ObjectDictionary::simulated();
        let json = dict_json(&dict);
        assert!(json.contains("\"schema\": \"ec-sim/dict-v1\""));
        assert!(json.contains(&format!("\"object_count\": {},", dict.objects().len())));
        for object in dict.objects() {
            assert!(
                json.contains(&format!("\"index\": \"0x{:04X}\"", object.index)),
                "缺少对象 0x{:04X}",
                object.index
            );
        }
        // 值以原始小端字节 hex 表示。
        let entry = dict.entry(0x1018, 1).expect("0x1018:01");
        let value_hex: String = entry
            .value
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert!(json.contains(&format!("\"value_hex\": \"{value_hex}\"")));
    }

    #[test]
    fn auth_protected_routes_match_api_only() {
        for path in [
            "/state",
            "/state?slave=1",
            "/dict",
            "/pdo?history=1",
            "/sdo?index=0x6040&sub=0",
            "/sdo-audit",
            "/topology",
            "/metrics?slave=0",
            "/events",
        ] {
            assert!(is_protected_route(path), "应保护：{path}");
        }
        for path in ["/", "/index.html", "/app.js", "/health", "/nope"] {
            assert!(!is_protected_route(path), "不应保护：{path}");
        }
    }

    #[test]
    fn auth_disabled_without_configured_token() {
        assert_eq!(authorize(&[], None), AuthDecision::Disabled);
        assert_eq!(authorize(&[], Some("")), AuthDecision::Disabled);
        let header = vec![("Authorization".to_string(), "Bearer secret".to_string())];
        assert_eq!(authorize(&header, None), AuthDecision::Disabled);
    }

    #[test]
    fn auth_missing_and_invalid_credentials() {
        let expected = Some("secret");
        assert_eq!(authorize(&[], expected), AuthDecision::MissingToken);
        let basic = vec![("Authorization".to_string(), "Basic c2VjcmV0".to_string())];
        assert_eq!(authorize(&basic, expected), AuthDecision::MissingToken);
        let empty = vec![("Authorization".to_string(), "Bearer   ".to_string())];
        assert_eq!(authorize(&empty, expected), AuthDecision::MissingToken);
        let wrong = vec![("Authorization".to_string(), "Bearer guess".to_string())];
        assert_eq!(authorize(&wrong, expected), AuthDecision::InvalidToken);
    }

    #[test]
    fn auth_allows_matching_bearer_case_insensitive_names() {
        let header = vec![("authorization".to_string(), "bearer secret".to_string())];
        assert_eq!(authorize(&header, Some("secret")), AuthDecision::Allowed);
        // token 本身大小写敏感。
        let wrong_case = vec![("Authorization".to_string(), "Bearer SECRET".to_string())];
        assert_eq!(
            authorize(&wrong_case, Some("secret")),
            AuthDecision::InvalidToken
        );
        // 头列表里混入无关请求头时仍能找到 Authorization。
        let mixed = vec![
            ("Host".to_string(), "127.0.0.1".to_string()),
            ("Authorization".to_string(), "Bearer secret".to_string()),
        ];
        assert_eq!(authorize(&mixed, Some("secret")), AuthDecision::Allowed);
    }

    #[test]
    fn auth_error_responses_have_401_and_403_status() {
        let unauthorized = unauthorized_json();
        assert_eq!(unauthorized.status_line, "HTTP/1.1 401 Unauthorized");
        assert!(unauthorized.body.contains("Authorization: Bearer"));
        let forbidden = forbidden_token_json();
        assert_eq!(forbidden.status_line, "HTTP/1.1 403 Forbidden");
        assert!(forbidden.body.contains("token"));
    }
}
