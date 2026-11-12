#!/usr/bin/env bash
# B6/B11 验证：**模拟 DC 时钟**——SV660N 画像声明 DC（SII DC 类别），0x0910 系统时间
# 随帧前进（Tier0：帧驱动模拟时钟，无真实 SYNC0 脉冲/漂移补偿）。
#
# 前置：`cargo build --release -p esc-node`；python3；不需要 root（UDS 线缆层）。
# 用法：./dc-clock-probe.sh
set -euo pipefail

MANIFEST="${MANIFEST:-plugins/inovance-sv660n/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-dc-clock.sock}"
LOG=/tmp/ec-sim-dc-clock.log

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}
[ -f "$MANIFEST" ] || {
  echo "FAIL: 找不到 manifest $MANIFEST"
  exit 1
}

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK"
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  exit 1
}

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --dc-clock

echo "OK: 模拟 DC 时钟在 UDS 合成主站下前进（0x0910）"
