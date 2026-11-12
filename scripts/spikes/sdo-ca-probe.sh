#!/usr/bin/env bash
# B7 验证：**Complete Access（CA）SDO 上传**——合成主站用命令 0x50 一次读回
# 0x1C12 / 0x1600 的全部子索引，断言数据按子索引自身长度拼接（不填充）。
#
# 前置：`cargo build --release -p esc-node`；python3；不需要 root（UDS 线缆层）。
# 用法：./sdo-ca-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-sdo-ca.sock}"
LOG=/tmp/ec-sim-sdo-ca.log

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK"
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  exit 1
}

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --sdo-ca

echo "OK: Complete Access 上传在合成主站下通过"
