//! manifest → 运行时对象字典（B9 子项 2：声明式插件画像）。
//!
//! 从 [`PluginManifest`] 生成 [`esc_core::obj_dict::ObjectDictionary`]，包含：
//! - 身份对象 `0x1018`（Vendor/Product/Revision/Serial，取自 manifest `identity`）；
//! - SM 通信类型对象 `0x1C00`（SM 数量 + 每个 SM 的类型 1/2/3/4）；
//! - PDO 分配对象 `0x1C12`/`0x1C13` 与 PDO 映射对象（取自 manifest `pdo`）；
//! - manifest `objects` 声明的用户对象（数值初值；字符串对象需要分段 SDO，暂不支持）。
//!
//! 生成前会执行 [`PluginManifest::validate`]；R2/R5 未通过时不生成（避免把占位身份
//! 装进模拟器）。

use std::collections::BTreeMap;

use crate::manifest::{ObjectConfig, PluginManifest};
use esc_core::obj_dict::{self, ObjectDictionary, RuntimeEntry, RuntimeObject};

/// SM 方向 → 0x1C00 通信类型（SOEM `ecx_readPDOmap`：1 = mailbox in、2 = mailbox out、
/// 3 = outputs、4 = inputs）。
fn sm_type_for_dir(dir: &str) -> Option<u8> {
    match dir {
        "mbox-in" => Some(1),
        "mbox-out" => Some(2),
        "out" => Some(3),
        "in" => Some(4),
        _ => None,
    }
}

/// 数据类型字符串 → (类型码, 默认位长)。
fn data_type_for(text: &str) -> Result<(u16, u16), String> {
    let upper = text.trim().to_ascii_uppercase();
    let (code, bits) = match upper.as_str() {
        "BOOL" | "BOOLEAN" => (obj_dict::data_type::BOOLEAN, 1),
        "INT8" | "INTEGER8" => (obj_dict::data_type::INTEGER8, 8),
        "INT16" | "INTEGER16" => (obj_dict::data_type::INTEGER16, 16),
        "INT32" | "INTEGER32" => (obj_dict::data_type::INTEGER32, 32),
        "UINT8" | "UNSIGNED8" => (obj_dict::data_type::UNSIGNED8, 8),
        "UINT16" | "UNSIGNED16" => (obj_dict::data_type::UNSIGNED16, 16),
        "UINT32" | "UNSIGNED32" => (obj_dict::data_type::UNSIGNED32, 32),
        other => {
            return Err(format!(
                "不支持的 dtype：{other}（当前支持 8/16/32 位整数）"
            ))
        }
    };
    Ok((code, bits))
}

/// 解析 manifest 对象初值为小端字节；未给初值时按位长补零。
fn entry_value(object: &ObjectConfig, bits: u16) -> Result<Vec<u8>, String> {
    let len = usize::from(bits.div_ceil(8));
    let Some(raw) = object.value.as_deref() else {
        return Ok(vec![0u8; len]);
    };
    let text = raw.trim().replace('_', "");
    let value = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => text.parse::<u64>(),
    }
    .map_err(|_| {
        format!(
            "对象 0x{:04X}:{:02X} 的 value 不是数值：{raw}",
            object.index, object.sub
        )
    })?;
    let mut bytes = value.to_le_bytes().to_vec();
    bytes.truncate(len);
    bytes.resize(len, 0);
    Ok(bytes)
}

/// 访问权限：manifest `access` → (权限位, 是否可写)。
fn access_for(access: &str) -> (u16, bool) {
    match access.trim().to_ascii_lowercase().as_str() {
        "rw" => (obj_dict::access::READ_WRITE_PREOP, true),
        _ => (obj_dict::access::READ_ONLY, false),
    }
}

fn entry(
    index: u16,
    subindex: u8,
    value: Vec<u8>,
    data_type: u16,
    bit_length: u16,
    access: u16,
    name: &str,
) -> RuntimeEntry {
    RuntimeEntry {
        index,
        subindex,
        value,
        data_type,
        bit_length,
        access,
        value_info: 0,
        name: name.to_string(),
        writable: access & 0x0038 != 0,
    }
}

fn object(
    index: u16,
    max_subindex: u8,
    object_code: u8,
    data_type: u16,
    name: &str,
) -> RuntimeObject {
    RuntimeObject {
        index,
        max_subindex,
        object_code,
        data_type,
        name: name.to_string(),
    }
}

/// 从 manifest 生成运行时对象字典（会先执行 R2/R5 校验）。
pub fn dictionary_from_manifest(manifest: &PluginManifest) -> Result<ObjectDictionary, String> {
    let problems = manifest.validate();
    if !problems.is_empty() {
        return Err(format!(
            "manifest 校验未通过，不能生成对象字典：{}",
            problems.join("；")
        ));
    }

    let mut objects: Vec<RuntimeObject> = Vec::new();
    let mut entries: BTreeMap<(u16, u8), RuntimeEntry> = BTreeMap::new();
    {
        let mut push_entry = |entry: RuntimeEntry| {
            entries.insert((entry.index, entry.subindex), entry);
        };

        // 1) 身份对象 0x1018。
        objects.push(object(
            0x1018,
            4,
            obj_dict::object_code::RECORD,
            obj_dict::data_type::NONE,
            "Identity",
        ));
        push_entry(entry(
            0x1018,
            0,
            vec![4],
            obj_dict::data_type::UNSIGNED8,
            8,
            obj_dict::access::READ_ONLY,
            "Number of entries",
        ));
        for (subindex, name, value) in [
            (1u8, "Vendor ID", manifest.identity.vendor_id),
            (2u8, "Product code", manifest.identity.product_code),
            (3u8, "Revision number", manifest.identity.revision),
            (4u8, "Serial number", 0u32),
        ] {
            push_entry(entry(
                0x1018,
                subindex,
                value.to_le_bytes().to_vec(),
                obj_dict::data_type::UNSIGNED32,
                32,
                obj_dict::access::READ_ONLY,
                name,
            ));
        }

        // 2) SM 通信类型对象 0x1C00。
        if !manifest.sm.is_empty() {
            let mut by_index: Vec<_> = manifest.sm.iter().collect();
            by_index.sort_by_key(|sm| sm.index);
            objects.push(object(
                0x1C00,
                u8::try_from(by_index.len()).unwrap_or(u8::MAX),
                obj_dict::object_code::ARRAY,
                obj_dict::data_type::UNSIGNED8,
                "Sync manager communication type",
            ));
            push_entry(entry(
                0x1C00,
                0,
                vec![u8::try_from(by_index.len()).unwrap_or(u8::MAX)],
                obj_dict::data_type::UNSIGNED8,
                8,
                obj_dict::access::READ_ONLY,
                "Number of sync managers",
            ));
            for sm in by_index {
                let sm_type = sm_type_for_dir(&sm.dir)
                    .ok_or_else(|| format!("sm[{}].dir 非法：{}", sm.index, sm.dir))?;
                push_entry(entry(
                    0x1C00,
                    sm.index.saturating_add(1),
                    vec![sm_type],
                    obj_dict::data_type::UNSIGNED8,
                    8,
                    obj_dict::access::READ_ONLY,
                    &format!("SM{} type", sm.index),
                ));
            }
        }

        // 3) PDO 分配对象（0x1C12 = RxPDO、0x1C13 = TxPDO）。
        for (assign_index, pdos, name) in [
            (0x1C12u16, &manifest.rx_pdos, "RxPDO assign"),
            (0x1C13u16, &manifest.tx_pdos, "TxPDO assign"),
        ] {
            if pdos.is_empty() {
                continue;
            }
            objects.push(object(
                assign_index,
                u8::try_from(pdos.len()).unwrap_or(u8::MAX),
                obj_dict::object_code::ARRAY,
                obj_dict::data_type::UNSIGNED16,
                name,
            ));
            push_entry(entry(
                assign_index,
                0,
                vec![u8::try_from(pdos.len()).unwrap_or(u8::MAX)],
                obj_dict::data_type::UNSIGNED8,
                8,
                obj_dict::access::READ_WRITE_PREOP,
                "Number of assigned PDOs",
            ));
            for (offset, pdo) in pdos.iter().enumerate() {
                push_entry(entry(
                    assign_index,
                    u8::try_from(offset + 1).unwrap_or(u8::MAX),
                    pdo.index.to_le_bytes().to_vec(),
                    obj_dict::data_type::UNSIGNED16,
                    16,
                    obj_dict::access::READ_WRITE_PREOP,
                    name,
                ));
            }
        }

        // 4) PDO 映射对象（每个 PDO 一个数组对象）。
        for (pdos, name) in [
            (&manifest.rx_pdos, "RxPDO mapping"),
            (&manifest.tx_pdos, "TxPDO mapping"),
        ] {
            for pdo in pdos {
                let max_subindex = u8::try_from(pdo.entries.len()).unwrap_or(u8::MAX);
                if objects.iter().any(|object| object.index == pdo.index) {
                    continue;
                }
                objects.push(object(
                    pdo.index,
                    max_subindex,
                    obj_dict::object_code::ARRAY,
                    obj_dict::data_type::UNSIGNED32,
                    name,
                ));
                push_entry(entry(
                    pdo.index,
                    0,
                    vec![max_subindex],
                    obj_dict::data_type::UNSIGNED8,
                    8,
                    obj_dict::access::READ_WRITE_PREOP,
                    "Number of mapped objects",
                ));
                for (offset, mapping) in pdo.entries.iter().enumerate() {
                    let packed = (u32::from(mapping.index) << 16)
                        | (u32::from(mapping.sub) << 8)
                        | u32::from(mapping.bits);
                    push_entry(entry(
                        pdo.index,
                        u8::try_from(offset + 1).unwrap_or(u8::MAX),
                        packed.to_le_bytes().to_vec(),
                        obj_dict::data_type::UNSIGNED32,
                        32,
                        obj_dict::access::READ_WRITE_PREOP,
                        name,
                    ));
                }
            }
        }
    }

    // 5) manifest 用户对象（可覆盖上面生成条目的名称/访问权限）。
    for spec in &manifest.objects {
        let upper = spec.data_type.trim().to_ascii_uppercase();
        let string_kind = match upper.as_str() {
            "STRING" | "VISIBLE_STRING" => Some(obj_dict::data_type::VISIBLE_STRING),
            "OCTET_STRING" => Some(obj_dict::data_type::OCTET_STRING),
            _ => None,
        };
        if let Some(type_code) = string_kind {
            // 字符串对象：值必须是 manifest 里的字符串文本（长度 = 字节数 × 8）。
            // 需分段 SDO 上传（>4 字节），由 esc-node 的 upload segment 支持。
            let Some(text) = spec.value.as_deref() else {
                return Err(format!(
                    "对象 0x{:04X}:{:02X} 是字符串类型但没有 value",
                    spec.index, spec.sub
                ));
            };
            let bytes = text.as_bytes().to_vec();
            let bit_length = u16::try_from(bytes.len() * 8)
                .map_err(|_| format!("对象 0x{:04X}:{:02X} 字符串过长", spec.index, spec.sub))?;
            let (access, _) = access_for(&spec.access);
            let description = RuntimeEntry {
                index: spec.index,
                subindex: spec.sub,
                value: bytes,
                data_type: type_code,
                bit_length,
                access,
                value_info: 0,
                name: spec.name.clone(),
                writable: access & 0x0038 != 0,
            };
            if let Some(existing) = entries.get_mut(&(spec.index, spec.sub)) {
                existing.name = description.name;
                existing.access = description.access;
                existing.writable = description.writable;
            } else {
                entries.insert((spec.index, spec.sub), description);
            }
            if !objects.iter().any(|object| object.index == spec.index) {
                objects.push(object(
                    spec.index,
                    0,
                    obj_dict::object_code::VAR,
                    type_code,
                    &spec.name,
                ));
            }
            continue;
        }

        let (type_code, bits) = data_type_for(&spec.data_type)?;
        let (access, _) = access_for(&spec.access);
        let value = entry_value(spec, bits)?;
        let description = RuntimeEntry {
            index: spec.index,
            subindex: spec.sub,
            value,
            data_type: type_code,
            bit_length: bits,
            access,
            value_info: 0,
            name: spec.name.clone(),
            writable: access & 0x0038 != 0,
        };
        if let Some(existing) = entries.get_mut(&(spec.index, spec.sub)) {
            existing.name = description.name;
            existing.access = description.access;
            existing.writable = description.writable;
        } else {
            entries.insert((spec.index, spec.sub), description);
        }
        if let Some(existing) = objects.iter_mut().find(|object| object.index == spec.index) {
            existing.max_subindex = existing.max_subindex.max(spec.sub);
        } else {
            // 子索引 > 0 的用户对象按 ARRAY 处理（子索引 0 = 条目数，与 EL2008 等
            // 多通道端子一致）；只有子索引 0 的对象按 VAR 处理。
            let (object_code, max_subindex) = if spec.sub > 0 {
                (obj_dict::object_code::ARRAY, spec.sub)
            } else {
                (obj_dict::object_code::VAR, 0)
            };
            objects.push(object(
                spec.index,
                max_subindex,
                object_code,
                type_code,
                &spec.name,
            ));
        }
    }

    // 6) 数组对象补一条子索引 0（UINT8 条目数，CiA 301 约定）。主站读对象字典时会读
    //    0xNNNN:00；缺失会产生无谓的 SDO Abort 日志（IgH 会 WARN "SDO information error"）。
    let array_objects: Vec<(u16, u8)> = objects
        .iter()
        .filter(|object| object.object_code == obj_dict::object_code::ARRAY)
        .map(|object| (object.index, object.max_subindex))
        .collect();
    for (index, max_subindex) in array_objects {
        if entries.contains_key(&(index, 0)) {
            continue;
        }
        entries.insert(
            (index, 0),
            entry(
                index,
                0,
                vec![max_subindex],
                obj_dict::data_type::UNSIGNED8,
                8,
                obj_dict::access::READ_ONLY,
                "Number of entries",
            ),
        );
    }

    // 修正每个对象的 max_subindex（取条目表最大值，保证 SDO Info 与实际条目一致）。
    for object in &mut objects {
        if let Some(max_subindex) = entries
            .keys()
            .filter(|(index, _)| *index == object.index)
            .map(|(_, subindex)| *subindex)
            .max()
        {
            object.max_subindex = max_subindex;
        }
    }

    // 对象列表按索引排序（SDO Info 的 Get OD List 顺序更直观，且与真实设备习惯一致）。
    objects.sort_by_key(|object| object.index);

    Ok(ObjectDictionary::new(
        objects,
        entries.into_values().collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/example-8do.yaml");

    fn fixture_manifest() -> PluginManifest {
        PluginManifest::parse(FIXTURE).expect("fixture manifest 可解析")
    }

    #[test]
    fn generated_dictionary_matches_manifest() {
        let manifest = fixture_manifest();
        let dict = dictionary_from_manifest(&manifest).expect("generate");
        assert_eq!(
            dict.object_indices(),
            vec![0x1008, 0x1018, 0x1600, 0x1A00, 0x1C00, 0x1C12, 0x1C13, 0x2000]
        );

        // 身份对象取自 manifest（自有 VID/PID，R2）。
        assert_eq!(
            u32::from_le_bytes(
                dict.value(0x1018, 1)
                    .expect("vendor")
                    .try_into()
                    .expect("4B")
            ),
            manifest.identity.vendor_id
        );
        assert_eq!(
            u32::from_le_bytes(
                dict.value(0x1018, 2)
                    .expect("product")
                    .try_into()
                    .expect("4B")
            ),
            manifest.identity.product_code
        );

        // SM 类型：0x1C00:1=1(mbox-in)、:2=2(mbox-out)、:3=3(out)、:4=4(in)。
        for (subindex, expected) in [(1u8, 1u8), (2, 2), (3, 3), (4, 4)] {
            assert_eq!(
                dict.value(0x1C00, subindex).expect("sm type"),
                [expected],
                "0x1C00:{subindex:02X}"
            );
        }

        // PDO 分配与映射。
        assert_eq!(dict.value(0x1C12, 1), Some(&0x1600u16.to_le_bytes()[..]));
        assert_eq!(dict.value(0x1C13, 1), Some(&0x1A00u16.to_le_bytes()[..]));
        assert_eq!(
            dict.value(0x1600, 1),
            Some(&0x7000_0008u32.to_le_bytes()[..]),
            "RxPDO 映射 0x7000:00 8 位"
        );
        assert_eq!(
            dict.value(0x1A00, 1),
            Some(&0x6000_0008u32.to_le_bytes()[..])
        );

        // 用户对象 0x2000:00 的初值与只读属性。
        assert_eq!(dict.value(0x2000, 0), Some(&0x1234u16.to_le_bytes()[..]));
        assert!(!dict.entry(0x2000, 0).expect("entry").writable);
        // PDO 分配/映射对象可写（主站配置 PDO 时需要 SDO Download）。
        assert!(dict.is_writable(0x1C12));
        assert!(dict.is_writable(0x1600));
        // 未知对象不存在。
        assert!(dict.value(0x3000, 0).is_none());
    }

    #[test]
    fn invalid_manifest_is_rejected_before_generation() {
        let template = include_str!("../../../plugins/_template/plugin.yaml");
        let manifest = PluginManifest::parse(template).expect("模板可解析");
        let error = dictionary_from_manifest(&manifest).expect_err("占位 VID/PID 应被拒绝");
        assert!(error.contains("vendor_id"), "{error}");
    }

    #[test]
    fn string_objects_are_supported_with_segmented_lengths() {
        let mut manifest = fixture_manifest();
        manifest.objects.push(crate::manifest::ObjectConfig {
            index: 0x100A,
            sub: 0,
            name: "Device name".to_string(),
            data_type: "VISIBLE_STRING".to_string(),
            access: "ro".to_string(),
            pdo: None,
            value: Some("EC-Sim sample (SIMULATED)".to_string()),
        });
        let dict = dictionary_from_manifest(&manifest).expect("generate");
        let entry = dict.entry(0x100A, 0).expect("string entry");
        assert_eq!(
            entry.data_type,
            obj_dict::data_type::VISIBLE_STRING,
            "VISIBLE_STRING 类型码"
        );
        assert_eq!(
            usize::from(entry.bit_length),
            "EC-Sim sample (SIMULATED)".len() * 8
        );
        assert_eq!(entry.value, b"EC-Sim sample (SIMULATED)");
        assert_eq!(
            dict.object(0x100A).expect("object").object_code,
            obj_dict::object_code::VAR
        );
    }

    #[test]
    fn array_channel_objects_get_number_of_entries_subindex() {
        let mut manifest = fixture_manifest();
        manifest.objects.push(crate::manifest::ObjectConfig {
            index: 0x7100,
            sub: 1,
            name: "Output channel".to_string(),
            data_type: "BOOL".to_string(),
            access: "ro".to_string(),
            pdo: None,
            value: None,
        });
        let dict = dictionary_from_manifest(&manifest).expect("generate");
        let object = dict.object(0x7100).expect("array object");
        assert_eq!(object.object_code, obj_dict::object_code::ARRAY);
        assert_eq!(object.max_subindex, 1);
        // 数组对象的子索引 0 = 条目数（UINT8，CiA 301），主站字典读取依赖它。
        let count = dict.entry(0x7100, 0).expect("count entry");
        assert_eq!(count.data_type, obj_dict::data_type::UNSIGNED8);
        assert_eq!(count.value, vec![1u8]);
    }

    #[test]
    fn string_values_are_not_supported_yet() {
        let mut manifest = fixture_manifest();
        manifest.objects.push(crate::manifest::ObjectConfig {
            index: 0x2001,
            sub: 0,
            name: "String object".to_string(),
            data_type: "UINT8".to_string(),
            access: "ro".to_string(),
            pdo: None,
            value: Some("not-a-number".to_string()),
        });
        let error = dictionary_from_manifest(&manifest).expect_err("非数值初值应被拒绝");
        assert!(error.contains("value"), "{error}");
    }
}
