#!/usr/bin/env bash
# B11 验证：**CiA 402 故障注入**（`ec-sim-esc --fault <hex>`）——从站启动即处于 Fault，
# 断言状态字 Fault bit、故障复位（控制字 bit7）清故障、`0x603F` 错误码可回读。
#
# 前置：`cargo build --release -p esc-node`；python3；不需要 root（UDS 线缆层）。
# 用法：./cia402-fault-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-servo/plugin.yaml}"
SOCK="${SOCK:-/tmp/ecsim-cia402-fault.sock}"
LOG=/tmp/ec-sim-cia402-fault.log

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
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" --behavior cia402 --fault 0x7500 >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  exit 1
}

grep -q "FAULT cia402 error_code=0x7500" "$LOG" || {
  cat "$LOG"
  echo "FAIL: 未打印故障注入信息"
}

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --cia402-fault

echo "OK: CiA 402 故障注入在合成主站下通过"
