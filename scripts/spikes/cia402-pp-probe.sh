#!/usr/bin/env bash
# B11 验证：**CiA 402 PP（轮廓位置模式 1，7 字节 PDO）**——合成主站按
# `<controlword, mode, target torque>` 写 RxPDO，读回 7 字节 TxPDO
# （状态字/模式显示/实际转矩），断言 Operation enabled 时实际转矩镜像目标、禁用后归零。
#
# 前置：`cargo build --release -p esc-node`；python3；不需要 root（UDS 线缆层）。
# 用法：./cia402-cst-probe.sh
set -euo pipefail

MANIFEST="${MANIFEST:-plugins/generic-cia402-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-cia402-pp.sock}"
LOG=/tmp/ec-sim-cia402-pp.log

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
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" --behavior cia402 >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  exit 1
}

grep -q "BEHAVIOR cia402 sm2_out=0x1100/7B sm3_in=0x1110/7B" "$LOG" || {
  cat "$LOG"
  echo "FAIL: 行为未按 7 字节 PDO 启用"
}

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --cia402-pp

echo "OK: CiA 402 CST（7 字节 PDO）行为在合成主站下通过"
