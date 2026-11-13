//! B44 W2 子项 4：仓库内**真实行为画像**的端到端守卫测试。
//!
//! 走完整链路（不复制 WAT 字符串、不做内存桩）：
//! `plugins/behavior-counter/plugin.yaml`（磁盘 manifest）
//!   → `PluginManifest::load` + `validate`（R2/R5 等红线）
//!   → `behavior.code` 相对 manifest 目录解析到 `behavior.wat`
//!   → `BehaviorBridge::from_manifest`（worker 线程里编译/实例化，宿主不 tick 插件）
//!   → 合成主站 EtherCAT 帧写 SM2（APWR 0x1100）/ 读 SM3（APRD 0x1110）
//!   → 插件输出（SM2 + 1）出现在 SM3。
//!
//! 仅在 `wasm-runtime` feature 下编译（该 feature 会引入 wasmi/wat）。
#![cfg(feature = "wasm-runtime")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use esc_node::behavior::BehaviorBridge;
use esc_node::{Responder, DEFAULT_SIM_MAC};
use plugin_host::manifest::PluginManifest;
use wire::{encode_frame, Command, DatagramData};

/// SM2/SM3 起始地址与长度必须与 `plugin.yaml` 一致。
const SM2_ADDRESS: u16 = 0x1100;
const SM3_ADDRESS: u16 = 0x1110;
const PDO_LEN: usize = 4;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 组装一帧（以太网头 + 一个 datagram）；源 MAC 用固定测试值。
fn request(datagrams: &[DatagramData]) -> Vec<u8> {
    let payload = encode_frame(datagrams).expect("编码 EtherCAT 负载");
    let mut frame = vec![0u8; wire::ETHERNET_HEADER_LEN];
    frame[0..6].copy_from_slice(&[0xFF; 6]); // 广播目的
    frame[6..12].copy_from_slice(&[0x01; 6]);
    wire::set_ethertype(&mut frame, wire::ETHERCAT_ETHERTYPE);
    frame.extend_from_slice(&payload);
    frame
}

/// 取应答帧里第一个 datagram 的 WKC 与数据（以太网头 14 + EtherCAT 头 2 + datagram 头 10）。
fn reply_datagram(reply: &[u8]) -> (u16, Vec<u8>) {
    let data_start = wire::ETHERNET_HEADER_LEN + wire::ECAT_HEADER_LEN + wire::DATAGRAM_HEADER_LEN;
    let len = reply.len() - data_start - 2;
    let wkc = u16::from_le_bytes([reply[data_start + len], reply[data_start + len + 1]]);
    (wkc, reply[data_start..data_start + len].to_vec())
}

#[test]
fn repository_behavior_profile_drives_sm3_through_frame_path() {
    let manifest_path = repo_root()
        .join("plugins")
        .join("behavior-counter")
        .join("plugin.yaml");
    let manifest = PluginManifest::load(&manifest_path)
        .unwrap_or_else(|error| panic!("加载 {} 失败：{error}", manifest_path.display()));
    assert!(
        manifest.validate().is_empty(),
        "behavior-counter manifest 未通过 R2/R5 校验：{:?}",
        manifest.validate()
    );
    assert!(
        manifest.behavior.is_some(),
        "behavior-counter 必须声明 behavior: 段"
    );

    // 桥参数从 manifest 的 SM 布局推导（而不是测试里另写一份地址）。
    let mut bridge = BehaviorBridge::from_manifest(&manifest, &manifest_path)
        .unwrap_or_else(|error| panic!("启动行为桥失败：{error}"))
        .expect("manifest 声明了 behavior，应返回 Some");
    assert_eq!(bridge.config().sm2_address, SM2_ADDRESS);
    assert_eq!(bridge.config().sm2_len, PDO_LEN);
    assert_eq!(bridge.config().sm3_address, SM3_ADDRESS);
    assert_eq!(bridge.config().sm3_len, PDO_LEN);

    let mut responder = Responder::default();
    // 主站写 SM2：u32 = 41。
    let write_frame = request(&[DatagramData::auto_increment(
        Command::Apwr,
        1,
        0,
        SM2_ADDRESS,
        vec![41, 0, 0, 0],
    )]);
    let write_reply = responder
        .handle_frame(&write_frame, DEFAULT_SIM_MAC)
        .expect("APWR 应被应答");
    let (write_wkc, _) = reply_datagram(&write_reply);
    assert_eq!(write_wkc, 1, "单从站 APWR 的 WKC 应为 1");

    let read_frame = request(&[DatagramData::auto_increment(
        Command::Aprd,
        2,
        0,
        SM3_ADDRESS,
        vec![0u8; PDO_LEN],
    )]);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut worst_frame = Duration::ZERO;
    let mut frames = 0u64;
    let mut observed = None;
    while Instant::now() < deadline {
        // 帧循环：处理主站帧（读 SM3）→ 帧末尾 publish/poll（插件输出回写 SM3）。
        let first = responder
            .handle_frame(&read_frame, DEFAULT_SIM_MAC)
            .expect("APRD 应被应答");
        let (read_wkc, data) = reply_datagram(&first);
        assert_eq!(read_wkc, 1, "单从站 APRD 的 WKC 应为 1");

        let started = Instant::now();
        bridge.on_frame(&mut responder);
        worst_frame = worst_frame.max(started.elapsed());
        frames += 1;

        if data == 42u32.to_le_bytes() {
            observed = Some(data);
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let observed = observed.unwrap_or_else(|| {
        panic!(
            "{} 帧（{:?}）内未见插件输出 42：stats={:?}",
            frames,
            worst_frame,
            bridge.stats()
        )
    });
    assert_eq!(observed, 42u32.to_le_bytes(), "SM3 应为 SM2 + 1");
    // 宿主帧循环每帧只做有界工作：不允许等待插件（阈值远大于 2ms 的 sleep 粒度）。
    assert!(
        worst_frame < Duration::from_millis(50),
        "单帧耗时 {worst_frame:?} 不应等待插件"
    );
    assert!(bridge.published_frames() > 0, "宿主应发布过 arena 快照");
    assert!(
        bridge.applied_generations() >= 1,
        "插件结果应至少写回 SM3 一次"
    );
    let stats = bridge.stats();
    assert!(stats.ticks >= 1, "插件应至少被 tick 一次：{stats:?}");
    assert_eq!(stats.out_of_fuel, 0, "{stats:?}");
    assert_eq!(stats.timeouts, 0, "{stats:?}");
    assert_eq!(stats.errors, 0, "{stats:?}");
    assert!(!stats.stopped, "{stats:?}");
    // 插件只读 SM2 快照，不得反过来改写主站输出段。
    assert_eq!(
        responder.read_process_data(SM2_ADDRESS, 2),
        vec![41, 0],
        "SM2 不应被插件改写"
    );
}
