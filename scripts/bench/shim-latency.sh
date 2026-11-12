#!/usr/bin/env bash
# M1 退出项（部分）：**shim 延迟基准**——启动 `ec-sim-esc --uds`（可多从站），
# 用 `scripts/bench/uds-latency.py` 顺序发 APRD 帧，输出 p50/p99/p999。
#
# 说明：这是**同机回归基准**，不是实时性承诺；绝对值随 CPU/调度变化。M1 目标
# p99 < 100 µs 需要隔离 CPU + 实时调度，本轮只记录非隔离环境的实测值。
#
# 用法：./shim-latency.sh        # 默认 1 从站、2000 帧
#       N=8 FRAMES=2000 ./shim-latency.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
N="${N:-1}"
FRAMES="${FRAMES:-2000}"
SOCK="${SOCK:-/tmp/ecsim-latency.sock}"
LOG=/tmp/ec-sim-esc-latency.log

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --slaves "$N" >"$LOG" 2>&1 &
ec_pid=$!
for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
[ -S "$SOCK" ] || { cat "$LOG"; echo "FAIL: 模拟器未创建 UDS socket $SOCK"; exit 1; }

"$PY" scripts/bench/uds-latency.py "$SOCK" "$FRAMES"

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
echo "OK: shim 延迟基准完成（N=$N frames=$FRAMES；非隔离 CPU，数值仅作回归对比）"
