//! 声明式插件 manifest → ESC SII 镜像（B9 子项 1）。
//!
//! 把 [`PluginManifest`] 的身份 / 邮箱 / SM / PDO 声明转换为
//! [`esc_core::sii::SiiSpec`]，再由 `SiiImage::from_spec()` 生成可被 IgH/SOEM 解析的
//! SII 镜像（固定区 CRC、类别链、EEPROM 大小自动计算）。
//!
//! 尚未覆盖（B9 后续子项）：对象字典的运行时生成（目前 `esc-node` 仍用内置字典）、
//! FMMU 布局与 `Responder` 的按 manifest 装配。

use crate::manifest::{PdoConfig, PluginManifest};
use esc_core::sii::{SiiMailbox, SiiPdoEntry, SiiPdoSpec, SiiSpec, SiiSyncManager};

/// 自有厂商名（SII 字符串索引 1；R2：不使用厂商名/商标）。
pub const SIM_VENDOR_NAME: &str = "EC-Sim";

/// CoE 邮箱协议位（SII 固定区 word 0x001C，bit2 = CoE）。
pub const MAILBOX_PROTOCOL_COE: u16 = 0x0004;

/// 把 manifest 的 PDO 声明转换为 SII PDO 规格。
fn pdo_spec(pdo: &PdoConfig, sync_manager: u8) -> SiiPdoSpec {
    SiiPdoSpec {
        index: pdo.index,
        sync_manager,
        entries: pdo
            .entries
            .iter()
            .map(|entry| SiiPdoEntry {
                index: entry.index,
                subindex: entry.sub,
                bit_length: entry.bits,
            })
            .collect(),
    }
}

/// 在 SM 列表里找指定方向第一个通道（找不到返回 `fallback`）。
fn sm_index_for(manifest: &PluginManifest, dir: &str, fallback: u8) -> u8 {
    manifest
        .sm
        .iter()
        .find(|sm| sm.dir == dir)
        .map_or(fallback, |sm| sm.index)
}

/// manifest → [`SiiSpec`]。
///
/// 邮箱参数取自 `sm` 中 `mbox-in`（接收）与 `mbox-out`（发送）两个通道；只有声明了
/// `mailbox.coe` 时才声明邮箱协议与 CoE 能力位。
pub fn sii_spec_from_manifest(manifest: &PluginManifest) -> Result<SiiSpec, String> {
    let mailbox = match (
        manifest.coe,
        sm_index_for(manifest, "mbox-in", 0xFF),
        sm_index_for(manifest, "mbox-out", 0xFF),
    ) {
        (Some(_), rx_index, tx_index) if rx_index != 0xFF && tx_index != 0xFF => {
            let rx = manifest
                .sm
                .iter()
                .find(|sm| sm.index == rx_index)
                .ok_or_else(|| "找不到 mbox-in 通道".to_string())?;
            let tx = manifest
                .sm
                .iter()
                .find(|sm| sm.index == tx_index)
                .ok_or_else(|| "找不到 mbox-out 通道".to_string())?;
            Some(SiiMailbox {
                rx_offset: rx.start,
                rx_size: rx.len,
                tx_offset: tx.start,
                tx_size: tx.len,
                protocols: MAILBOX_PROTOCOL_COE,
            })
        }
        (Some(_), _, _) => {
            return Err("声明了 mailbox.coe 但 sm 缺少 mbox-in / mbox-out 通道（R7）".to_string())
        }
        (None, _, _) => None,
    };

    let sync_managers: Vec<SiiSyncManager> = manifest
        .sm
        .iter()
        .map(|sm| {
            // 非法 dir 已由 PluginManifest::validate() 报告；此处按只读通道处理。
            let control: u8 = match sm.dir.as_str() {
                "mbox-in" => 0x26,
                "mbox-out" => 0x22,
                "out" => 0x04,
                _ => 0x00,
            };
            SiiSyncManager {
                physical_start_address: sm.start,
                length: sm.len,
                control,
                status: 0,
                enable: 0x01,
                pdi_control: 0,
            }
        })
        .collect();

    let tx_sm = sm_index_for(manifest, "in", 3);
    let rx_sm = sm_index_for(manifest, "out", 2);

    let mut coe_details = 0u8;
    if let Some(coe) = manifest.coe {
        if coe.sdo {
            coe_details |= 0x01;
        }
        if coe.sdo_info {
            coe_details |= 0x02;
        }
        if !manifest.rx_pdos.is_empty() || !manifest.tx_pdos.is_empty() {
            coe_details |= 0x04; // PDO assign
            coe_details |= 0x08; // PDO configuration
        }
        if coe.complete_access {
            coe_details |= 0x20;
        }
    }

    Ok(SiiSpec {
        vendor_id: manifest.identity.vendor_id,
        product_code: manifest.identity.product_code,
        revision: manifest.identity.revision,
        serial: 0,
        names: vec![manifest.name.clone(), SIM_VENDOR_NAME.to_string()],
        mailbox,
        sync_managers,
        tx_pdos: manifest
            .tx_pdos
            .iter()
            .map(|pdo| pdo_spec(pdo, tx_sm))
            .collect(),
        rx_pdos: manifest
            .rx_pdos
            .iter()
            .map(|pdo| pdo_spec(pdo, rx_sm))
            .collect(),
        coe_details,
        general_flags: 0,
        ebus_current: 0,
        // DC 类别：仅当 manifest 明确声明 `dc.supported: true` 且提供 `cycle_time0` 时写入。
        // 模拟器尚未实现 DC 同步；声明前需确认主站行为（AGENTS §6 / B6）。
        dc: if manifest.dc.supported {
            if manifest.dc.cycle_time0 == 0 {
                return Err("dc.supported=true 需要 dc.cycle_time0（ns）".to_string());
            }
            Some(esc_core::sii::SiiDc {
                cycle_time0: manifest.dc.cycle_time0,
                shift_time0: manifest.dc.shift_time0,
                shift_time1: manifest.dc.shift_time1,
                sync1_cycle_factor: manifest.dc.sync1_cycle_factor,
                assign_activate: manifest.dc.assign_activate,
                sync0_cycle_factor: manifest.dc.sync0_cycle_factor,
                name_index: 0,
                desc_index: 0,
            })
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use esc_core::sii::{category, SiiImage};

    const VALID_MANIFEST: &str = r#"
schema: ecsim.plugin/v1
id: com.ecsim.compat.sample-el2008
name: "8DO sample device (SIMULATED)"
version: 0.1.0
kind: device
license: Apache-2.0 OR MIT
pricing: {model: free, amount: 0, currency: CNY}

compatibility:
  brands: ["Sample"]
  models: ["SAMPLE-8DO"]
  disclaimer: "Independent re-implementation from public documentation. Not affiliated with, endorsed by, or certified by Sample."

identity:
  vendor_id: 0x00000EC5
  product_code: 0x0000E5D0
  revision: 0x00010000
  simulated: true

profile: generic-io
mailbox:
  coe: {sdo: true, sdo_info: true, complete_access: false}

sm:
  - {index: 0, dir: mbox-in,  start: 0x1000, len: 128}
  - {index: 1, dir: mbox-out, start: 0x1080, len: 128}
  - {index: 2, dir: out,      start: 0x1100, len: 1}
  - {index: 3, dir: in,       start: 0x1110, len: 1}

pdo:
  rx:
    - index: 0x1600
      entries:
        - {index: 0x7000, sub: 0, bits: 8}
  tx:
    - index: 0x1A00
      entries:
        - {index: 0x6000, sub: 0, bits: 8}

objects:
  - {index: 0x1018, sub: 1, name: Vendor ID, dtype: UINT32, access: ro}
  - {index: 0x7000, sub: 0, name: Output 0..7, dtype: UINT8, access: rw, pdo: rx}

dc: {supported: false, sync0: false}

sources:
  - {title: "Public device manual", url: "https://example.invalid/manual.pdf", accessed: "2026-11-12"}
"#;

    #[test]
    fn dc_supported_manifest_emits_dc_category() {
        let with_dc = VALID_MANIFEST.replace(
            "dc: {supported: false, sync0: false}",
            "dc:\n  supported: true\n  cycle_time0: 1000000\n  sync0_cycle_factor: 1\n  sync1_cycle_factor: -1\n  assign_activate: 0x0300",
        );
        let manifest = PluginManifest::parse(&with_dc).expect("parse");
        let spec = sii_spec_from_manifest(&manifest).expect("sii spec");
        let dc = spec.dc.expect("manifest 声明了 DC");
        assert_eq!(dc.cycle_time0, 1_000_000);
        assert_eq!(dc.assign_activate, 0x0300);
        let sii = esc_core::sii::SiiImage::from_spec(&spec);
        let bytes = sii
            .category_bytes(esc_core::sii::category::DC)
            .expect("DC 类别应存在");
        assert_eq!(bytes.len(), 24);

        // 声明 DC 但缺少 cycle_time0 → 生成失败。
        let missing = with_dc.replace("  cycle_time0: 1000000\n", "");
        let manifest = PluginManifest::parse(&missing).expect("parse");
        assert!(sii_spec_from_manifest(&manifest).is_err());
    }

    #[test]
    fn valid_manifest_passes_validation() {
        let manifest = PluginManifest::parse(VALID_MANIFEST).expect("parse");
        assert_eq!(manifest.schema, "ecsim.plugin/v1");
        assert_eq!(manifest.identity.vendor_id, 0x0EC5);
        assert_eq!(manifest.sm.len(), 4);
        assert_eq!(manifest.rx_pdos.len(), 1);
        assert_eq!(manifest.tx_pdos.len(), 1);
        assert!(manifest.validate().is_empty(), "{:?}", manifest.validate());
    }

    #[test]
    fn template_manifest_parses_but_fails_placeholder_checks() {
        let template = include_str!("../../../plugins/_template/plugin.yaml");
        let manifest = PluginManifest::parse(template).expect("模板 manifest 必须可解析");
        assert_eq!(manifest.kind, "device");
        assert_eq!(manifest.sm.len(), 4);
        // 模板里 vendor_id / product_code 是 TODO 占位值 → R2 校验必须报出来。
        let problems = manifest.validate();
        assert!(
            problems.iter().any(|problem| problem.contains("vendor_id")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("product_code")),
            "{problems:?}"
        );
    }

    #[test]
    fn generated_sii_matches_manifest_declarations() {
        let manifest = PluginManifest::parse(VALID_MANIFEST).expect("parse");
        let spec = sii_spec_from_manifest(&manifest).expect("generated spec");
        assert_eq!(spec.vendor_id, 0x0EC5);
        assert_eq!(spec.product_code, 0xE5D0);
        assert_eq!(spec.names[0], "8DO sample device (SIMULATED)");
        assert_eq!(
            spec.mailbox.expect("mailbox").protocols,
            MAILBOX_PROTOCOL_COE
        );

        let sii = SiiImage::from_spec(&spec);
        assert_eq!(sii.read_word(0x0008), 0x0EC5);
        assert_eq!(sii.read_word(0x000A), 0xE5D0);
        assert!(sii.verify_checksum());
        assert_eq!(sii.mailbox_protocols(), MAILBOX_PROTOCOL_COE);
        assert_eq!(sii.sync_manager_count(), 4);
        assert!(sii.declares_process_data_sync_managers());

        // SM 类别内容：SM0/SM1 = mailbox、SM2 输出、SM3 输入。
        let sm_bytes = sii
            .category_bytes(category::SYNC_MANAGER)
            .expect("SM category");
        let first_bytes: [u8; 8] = sm_bytes[..8].try_into().expect("8 bytes");
        let first = SiiSyncManager::from_bytes(&first_bytes);
        assert_eq!(first.physical_start_address, 0x1000);
        assert_eq!(first.length, 128);
        assert_eq!(first.control, 0x26);
        assert_eq!(first.enable, 0x01);

        // PDO 类别：RxPDO 0x1600 → SM2，映射 0x7000:0 8 位。
        let rx = sii
            .category_bytes(category::RX_PDO)
            .expect("RxPDO category");
        assert_eq!(u16::from_le_bytes([rx[0], rx[1]]), 0x1600);
        assert_eq!(rx[3], 2, "SM2");
        assert_eq!(u16::from_le_bytes([rx[8], rx[9]]), 0x7000);
        assert_eq!(rx[13], 8, "8 位映射");
        let tx = sii
            .category_bytes(category::TX_PDO)
            .expect("TxPDO category");
        assert_eq!(u16::from_le_bytes([tx[0], tx[1]]), 0x1A00);
        assert_eq!(tx[3], 3, "SM3");

        // 字符串类别含设备名与 SIMULATED 标识（R2）。
        let strings = sii
            .category_bytes(category::STRINGS)
            .expect("strings category");
        assert!(strings.windows(9).any(|window| window == b"SIMULATED"));
        // EEPROM 大小声明覆盖镜像。
        let declared = (usize::from(sii.read_word(esc_core::sii::SII_EEPROM_SIZE_WORD)) + 1) * 128;
        assert!(declared >= sii.len_words() * 2);
    }

    #[test]
    fn absolute_minimum_manifest_is_rejected() {
        let manifest = PluginManifest::parse(VALID_MANIFEST).expect("parse");
        let mut broken = manifest.clone();
        broken.identity.vendor_id = 0;
        broken.sources.clear();
        let problems = broken.validate();
        assert!(problems.iter().any(|p| p.contains("vendor_id")));
        assert!(problems.iter().any(|p| p.contains("sources")));
    }
}
