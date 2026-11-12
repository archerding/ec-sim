#!/usr/bin/env bash
# B11 验证：**CiA 402 PV（轮廓速度模式 3，11 字节 PDO）**——合成主站按
# `<controlword, mode, target_position, target_velocity>` 写 RxPDO，读回 11 字节 TxPDO
# （状态字/模式显示/实际位置/实际速度），断言速度模式下位置每周期累加。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - python3（`scripts/spikes/uds_master_probe.py`）
#   - 不需要 root（UDS 线缆层）
#
# 用法：./cia402-csv-probe.sh
set -euo pipefail

MANIFEST="${MANIFEST:-plugins/generic-cia402-csv-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-cia402-pv.sock}"
LOG=/tmp/ec-sim-cia402-pv.log

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest $MANIFEST"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK"
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" --behavior cia402 >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  fail "模拟器未创建 UDS socket $SOCK"
}

grep -q "BEHAVIOR cia402 sm2_out=0x1100/11B sm3_in=0x1110/11B" "$LOG" || {
  cat "$LOG"
  fail "行为未按 11 字节 PDO 启用"
}

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --cia402-pv

echo "OK: CiA 402 CSV（11 字节 PDO）行为在合成主站下通过"
