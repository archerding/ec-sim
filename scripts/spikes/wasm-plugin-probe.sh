#!/usr/bin/env bash
# B43 / M3-W1：WASM 行为插件宿主 spike 探针。
#
# 运行 `plugin-host` 的可选 feature `wasm-runtime`（wasmi 0.31，MIT）与示例插件，
# 断言宿主契约（read_pdo/write_pdo/log）与调用统计。首次运行需要从 crates.io
# 拉取可选依赖；离线环境返回 exit 2（SKIP），不进入 run-all-probes 门禁。
set -u

cd "$(dirname "$0")/../.." || exit 2

if ! command -v cargo >/dev/null 2>&1; then
    echo "SKIP: cargo 不可用"
    exit 2
fi

OUT="$(cargo run --quiet -p plugin-host --features wasm-runtime --example wasm_plugin_demo 2>&1)"
RC=$?

if [ "$RC" -ne 0 ]; then
    if printf '%s' "$OUT" | grep -qiE 'failed to download|no matching package|network|registry'; then
        echo "SKIP: 可选依赖无法下载（wasm-runtime 需要 crates.io）"
        exit 2
    fi
    printf '%s\n' "$OUT"
    echo "FAIL: wasm_plugin_demo 退出码 $RC"
    exit 1
fi

printf '%s\n' "$OUT"

fail() {
    echo "FAIL: $1"
    exit 1
}

printf '%s' "$OUT" | grep -q '^wasm-plugin-demo: pdo_after=29 00 00 00 2a 00 00 00$' \
    || fail "pdo_after 不符合 read_pdo(0)=0x29 → write_pdo(4)=0x2a 的契约"
printf '%s' "$OUT" | grep -q '^wasm-plugin-demo: log="tick"$' \
    || fail "插件 log() 未被宿主捕获"
printf '%s' "$OUT" | grep -q '^wasm-plugin-demo: log_lines=1 calls=1$' \
    || fail "首次 tick 的调用统计不符"
printf '%s' "$OUT" | grep -q '^wasm-plugin-demo: invariants=ok$' \
    || fail "1 万次 tick 后不变式不成立"
printf '%s' "$OUT" | grep -q '^wasm-plugin-demo: OK$' \
    || fail "示例未打印 OK"

echo "OK: WASM 插件宿主（wasmi 0.31，feature wasm-runtime）契约与调用路径全部通过"
