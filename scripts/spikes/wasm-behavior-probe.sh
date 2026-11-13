#!/usr/bin/env bash
# B44 W2 子项 4：仓库内**真实行为画像**（plugins/behavior-counter）端到端探针。
#
# 验证链路（全部真实文件，不做内存桩）：
#   plugins/behavior-counter/plugin.yaml + behavior.wat
#     → PluginManifest::load/validate → BehaviorBridge::from_manifest（worker 线程）
#     → 合成主站 EtherCAT 帧 APWR 0x1100 / APRD 0x1110 → SM3 出现 SM2 + 1。
#
# 依赖可选 feature `wasm-runtime`（wasmi/wat 首次要从 crates.io 拉取）：
# 离线/无 cargo 时 exit 2（SKIP），不进入 run-all-probes 门禁。
set -u

cd "$(dirname "$0")/../.." || exit 2

if ! command -v cargo >/dev/null 2>&1; then
    echo "SKIP: cargo 不可用"
    exit 2
fi

fail() {
    echo "FAIL: $1"
    exit 1
}

for file in \
    plugins/behavior-counter/plugin.yaml \
    plugins/behavior-counter/behavior.wat \
    crates/esc-node/tests/behavior_counter_plugin.rs
do
    [ -f "$file" ] || fail "缺少行为画像文件 $file"
done

# 1) 端到端测试：磁盘 manifest + WAT → BehaviorBridge → 帧路径 → SM3。
OUT="$(cargo test -p esc-node --features wasm-runtime --test behavior_counter_plugin 2>&1)"
RC=$?
if [ "$RC" -ne 0 ]; then
    if printf '%s' "$OUT" | grep -qiE 'failed to download|no matching package|network|registry|Could not resolve'; then
        echo "SKIP: 可选依赖无法下载（wasm-runtime 需要 crates.io）"
        exit 2
    fi
    printf '%s\n' "$OUT"
    fail "behavior_counter_plugin 退出码 $RC"
fi
printf '%s\n' "$OUT"
printf '%s' "$OUT" | grep -q 'test repository_behavior_profile_drives_sm3_through_frame_path ... ok' \
    || fail "端到端用例未执行/未通过"
printf '%s' "$OUT" | grep -q 'test result: ok. 1 passed' \
    || fail "端到端用例统计不符"

# 2) manifest 守卫（默认 feature，无需 crates.io）：新画像必须能被解析、通过 R2/R5 校验、
#    生成合法 SII 与对象字典，且 PDO 都有映射对象。
OUT2="$(cargo test -p plugin-host --test plugin_manifests 2>&1)"
RC2=$?
if [ "$RC2" -ne 0 ]; then
    printf '%s\n' "$OUT2"
    fail "plugin_manifests 退出码 $RC2"
fi
printf '%s' "$OUT2" | grep -q 'test repository_plugin_manifests_are_valid_and_generate_profiles ... ok' \
    || fail "仓库画像守卫未通过"

echo "OK: plugins/behavior-counter（manifest behavior: + 自写 WAT）在帧路径上产生 SM3 = SM2 + 1"
