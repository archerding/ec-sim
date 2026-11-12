#!/usr/bin/env bash
# B7 验证：**Complete Access（CA）SDO 上传/下载**——合成主站用命令 0x50 一次读回
# 0x1C12 / 0x1600 的全部子索引（按子索引自身长度拼接、不填充），再用命令 0x31
# 单帧 CA 下载 0x1600 并回读验证。
#
# 注意：每个模式启动**独立的模拟器实例**（UDS 套件的 SM watchdog 用例会改变 AL 状态，
# 复用实例会破坏下一模式的初始状态断言）。
#
# 第三个模式用 CSV 画像 manifest（0x2100 = 140 字节可写字符串）验证 **>112 字节的 CA 分段下载**。
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

SIM_PID=""
cleanup() {
  if [ -n "$SIM_PID" ]; then
    kill "$SIM_PID" 2>/dev/null || true
    wait "$SIM_PID" 2>/dev/null || true
  fi
  rm -f "$SOCK"
}
trap cleanup EXIT

run_mode() {
  mode="$1"
  manifest="${2:-}"
  rm -f "$SOCK"
  if [ -n "$manifest" ]; then
    "$BIN" --uds "$SOCK" --manifest "$manifest" >"$LOG" 2>&1 &
  else
    "$BIN" --uds "$SOCK" >"$LOG" 2>&1 &
  fi
  SIM_PID=$!
  for _ in $(seq 1 100); do
    [ -S "$SOCK" ] && break
    sleep 0.05
  done
  [ -S "$SOCK" ] || {
    cat "$LOG"
    echo "FAIL: 模拟器未创建 UDS socket $SOCK（模式 $mode）"
    exit 1
  }
  "$PY" scripts/spikes/uds_master_probe.py "$SOCK" "$mode"
  kill "$SIM_PID" 2>/dev/null || true
  wait "$SIM_PID" 2>/dev/null || true
  SIM_PID=""
  rm -f "$SOCK"
}

run_mode --sdo-ca
run_mode --sdo-ca-download
run_mode --sdo-ca-download-multi plugins/generic-cia402-csv-servo/plugin.yaml
run_mode --sdo-retransmit plugins/generic-cia402-csv-servo/plugin.yaml

echo "OK: Complete Access 上传/下载在合成主站下通过"
