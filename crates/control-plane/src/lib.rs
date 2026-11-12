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

fn not_found(message: &str) -> HttpResponse {
    let body = format!(
        "{{\"error\": \"{}\", \"routes\": [\"/state\", \"/dict\"]}}",
        json_escape(message)
    );
    json_response("HTTP/1.1 404 Not Found", body)
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
            None => not_found("dict 不可用（未加载 manifest）"),
        },
        "/" | "/health" => json_response(
            "HTTP/1.1 200 OK",
            "{\"service\": \"ec-sim-control-plane\", \"routes\": [\"/state\", \"/dict\"]}"
                .to_string(),
        ),
        _ => not_found("unknown path"),
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
}
