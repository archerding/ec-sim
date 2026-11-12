#!/usr/bin/env bash
# B11 验证：各 CiA 402 画像通过 **SDO 上传**暴露的 `0x6502`（Supported drive modes）
# 与其 PDO 布局一致（7B=0x01C1、11B=0x03CD、5B=0x0410）；SV660N 用公开实测 0x03AD。
#
# 前置：`cargo build --release -p esc-node`；python3；不需要 root（UDS 线缆层）。
# 用法：./cia402-modes-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-cia402-modes.sock}"
LOG=/tmp/ec-sim-cia402-modes.log

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK"
}
trap cleanup EXIT

check_profile() {
  local manifest="$1"
  local expected="$2"
  cleanup
  [ -f "$manifest" ] || {
    echo "FAIL: 找不到 $manifest"
    exit 1
  }
  "$BIN" --uds "$SOCK" --manifest "$manifest" >"$LOG" 2>&1 &
  for _ in $(seq 1 100); do
    [ -S "$SOCK" ] && break
    sleep 0.05
  done
  [ -S "$SOCK" ] || {
    cat "$LOG"
    echo "FAIL: 模拟器未创建 UDS socket $SOCK"
    exit 1
  }
  "$PY" scripts/spikes/uds_master_probe.py "$SOCK" --expect-6502="$expected"
}

check_profile plugins/generic-cia402-servo/plugin.yaml 0x01C1
check_profile plugins/generic-cia402-csv-servo/plugin.yaml 0x03CD
check_profile plugins/generic-cia402-cst-servo/plugin.yaml 0x0410

echo "OK: 三个 CiA 402 画像的 0x6502 经 SDO 上传与布局一致（0x01C1 / 0x03CD / 0x0410）"
