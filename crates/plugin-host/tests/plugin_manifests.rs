//! 仓库内所有设备插件 manifest 的守卫测试（B10 起）。
//!
//! 遍历 `plugins/<id>/plugin.yaml`（跳过 `_template`），要求每个 manifest：
//! 1. 可被 `PluginManifest::load()` 解析；
//! 2. 通过 `validate()`（R2 自有 VID/PID + SIMULATED 名称、R5 sources 等）；
//! 3. 能生成合法 SII 镜像（CRC 校验通过）与运行时对象字典；
//! 4. manifest 中声明的每个 PDO 都在生成的字典里有映射对象。

use std::path::{Path, PathBuf};

use esc_core::sii::SiiImage;
use plugin_host::dict_gen::dictionary_from_manifest;
use plugin_host::manifest::PluginManifest;
use plugin_host::sii_gen::sii_spec_from_manifest;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn plugin_manifest_paths() -> Vec<PathBuf> {
    let plugins_dir = repo_root().join("plugins");
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(&plugins_dir)
        .unwrap_or_else(|error| panic!("读取 {} 失败：{error}", plugins_dir.display()));
    for entry in entries {
        let entry = entry.expect("plugins 目录项");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name.starts_with('_') {
            continue; // `_template` 含占位值，不参与校验
        }
        let manifest_path = path.join("plugin.yaml");
        if manifest_path.is_file() {
            paths.push(manifest_path);
        }
    }
    paths.sort();
    paths
}

fn display(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

#[test]
fn repository_plugin_manifests_are_valid_and_generate_profiles() {
    let manifests = plugin_manifest_paths();
    assert!(
        !manifests.is_empty(),
        "plugins/ 下应至少有一个设备插件 manifest"
    );

    for manifest_path in manifests {
        let name = display(&manifest_path);
        let manifest = PluginManifest::load(&manifest_path)
            .unwrap_or_else(|error| panic!("{name} 解析失败：{error}"));
        let problems = manifest.validate();
        assert!(problems.is_empty(), "{name} 校验未通过：{problems:?}");

        let spec = sii_spec_from_manifest(&manifest)
            .unwrap_or_else(|error| panic!("{name} 生成 SII 规格失败：{error}"));
        if let Some(coe) = manifest.coe {
            let expected_enable = u8::from(coe.sdo);
            assert_eq!(
                spec.coe_details & 0x01,
                expected_enable,
                "{name} 的 CoE details bit0（enable SDO）与 manifest 不一致"
            );
            let expected_info = u8::from(coe.sdo_info) << 1;
            assert_eq!(
                spec.coe_details & 0x02,
                expected_info,
                "{name} 的 CoE details bit1（SDO Info）与 manifest 不一致"
            );
            let expected_ca = u8::from(coe.complete_access) << 5;
            assert_eq!(
                spec.coe_details & 0x20,
                expected_ca,
                "{name} 的 CoE details bit5（Complete Access）与 manifest 不一致"
            );
        }
        let sii = SiiImage::from_spec(&spec);
        assert!(sii.verify_checksum(), "{name} 生成的 SII CRC 不一致");
        assert_eq!(
            sii.read_word(esc_core::sii::SII_VERSION_WORD),
            esc_core::sii::SIM_SII_VERSION
        );

        let dict = dictionary_from_manifest(&manifest)
            .unwrap_or_else(|error| panic!("{name} 生成对象字典失败：{error}"));
        assert!(!dict.objects().is_empty(), "{name} 生成的对象字典为空");
        for pdo in manifest.rx_pdos.iter().chain(manifest.tx_pdos.iter()) {
            assert!(
                dict.object(pdo.index).is_some(),
                "{name} 的 PDO {:#06X} 没有对应映射对象",
                pdo.index
            );
        }
    }
}

/// CiA 402 画像的 `0x6502`（Supported drive modes）必须与它实际可支持的 Tier0 模式一致：
/// - 7 字节布局：pp(1)/hm(6)/ip(7)/csp(8) → 0x01C1；
/// - 11 字节布局：pp(1)/vl(2)/pv(3)/hm(6)/ip(7)/csp(8)/csv(9) → 0x03CD；
/// - 5 字节布局：tq(4)/cst(10) → 0x0410；
/// - SV660N 兼容画像使用公开实测值 0x03AD（厂商默认支持模式，不随本仓库 PDO 布局变化）。
#[test]
fn cia402_profiles_declare_supported_modes_matching_layout() {
    let cases = [
        ("generic-cia402-servo", 0x01C1u32),
        ("generic-cia402-csv-servo", 0x03CD),
        ("generic-cia402-cst-servo", 0x0410),
        ("inovance-sv660n", 0x03AD),
    ];
    for (id, expected) in cases {
        let path = repo_root().join("plugins").join(id).join("plugin.yaml");
        let manifest = PluginManifest::load(path.to_str().expect("utf8 path"))
            .unwrap_or_else(|error| panic!("加载 {id} 失败：{error}"));
        let dict = dictionary_from_manifest(&manifest)
            .unwrap_or_else(|error| panic!("{id} 生成对象字典失败：{error}"));
        let entry = dict
            .entry(0x6502, 0)
            .unwrap_or_else(|| panic!("{id} 缺少 0x6502"));
        assert_eq!(entry.value.len(), 4, "{id} 的 0x6502 应为 UINT32");
        let value = u32::from_le_bytes([
            entry.value[0],
            entry.value[1],
            entry.value[2],
            entry.value[3],
        ]);
        assert_eq!(value, expected, "{id} 的 0x6502 与布局不一致");
    }
}
