#!/usr/bin/env bash
# CNC E2E 故障矩阵（task-102）：slave_drop/重连 + SDO 拒绝/恢复。
#
# 前置：Linux；`cargo build --release -p esc-node`；python3；不占 CNC VM。
# 用法：
#   EC_SIM_BIN=target/release/ec-sim-esc ./scripts/spikes/cnc-e2e-fault-matrix.sh
#   OUT_DIR=/tmp/fault-matrix ./scripts/spikes/cnc-e2e-fault-matrix.sh
#
# 断言（P0）：
#   1. 3×CiA402 CSP 到 3/3 OP；
#   2. POST /fault slave_drop target=1 duration=1500 → 目标站帧停、其他站继续、
#      fault_injected 事件；到期 → /fault/status 清空、帧恢复、fault_cleared；
#   3. POST /fault sdo_reject target=0 duration=1500 → SDO 0x1018:01 Abort 0x06010000；
#      到期后正常应答；事件成对；
#   4. 结束仍 3/3 OP；run_id 全程一致。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${EC_SIM_BIN:-$ROOT/target/release/ec-sim-esc}"
MANIFEST="${MANIFEST:-$ROOT/plugins/cnc-csp-servo/plugin.yaml}"
SOCK="${SOCK:-/tmp/ecsim-fault-matrix.sock}"
PORT="${PORT:-18134}"
OUT_DIR="${OUT_DIR:-$ROOT/docs/verification/assets/cnc-e2e/fault-matrix}"
RUN_ID="${RUN_ID:-fault-matrix-$(date +%Y%m%d%H%M%S)}"
PY="${PY:-python3}"

if [ "$(uname -s 2>/dev/null)" != "Linux" ]; then
  echo "SKIP: 故障矩阵需要 Linux（UDS/TAP）"
  exit 2
fi
for tool in "$BIN" "$PY" curl; do
  if ! command -v "$tool" >/dev/null 2>&1 && [ ! -x "$tool" ]; then
    echo "SKIP: 缺少 $tool"
    exit 2
  fi
done
[ -f "$MANIFEST" ] || { echo "SKIP: manifest 不存在 $MANIFEST"; exit 2; }
mkdir -p "$OUT_DIR"
rm -f "$SOCK"

FAIL=0
note() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$OUT_DIR/timeline.log"; }
fail() { echo "FAIL: $*"; FAIL=1; }

cleanup() {
  if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT

note "启动 ec-sim-esc --uds（3×cnc-csp-servo，run_id=$RUN_ID）"
"$BIN" --uds "$SOCK" --slaves 3 --manifest "$MANIFEST" --behavior cia402 \
  --serve "127.0.0.1:$PORT" --serve-write --run-id "$RUN_ID" \
  >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!

ready=0
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  cat "$OUT_DIR/server.log" 2>/dev/null || true
  echo "FAIL: 控制面未就绪（127.0.0.1:$PORT/health）"
  exit 1
fi
note "控制面就绪；开始故障矩阵"

set -o pipefail
"$PY" "$ROOT/scripts/spikes/fault_matrix_probe.py" "$SOCK" \
  --http-base "http://127.0.0.1:$PORT" --out-dir "$OUT_DIR" 2>&1 | tee "$OUT_DIR/probe.out"
PROBE_EXIT=${PIPESTATUS[0]}
set +o pipefail

if [ "$PROBE_EXIT" -eq 0 ]; then
  note "slave_drop 时间线：$OUT_DIR/slave-drop-timeline.json"
  note "sdo_reject 时间线：$OUT_DIR/sdo-reject-timeline.json"
  echo "OK: ${RUN_ID} 故障矩阵（slave_drop/重连、sdo_reject/恢复，3/3 OP）"
else
  fail "故障矩阵探针退出码 $PROBE_EXIT"
fi

echo "RESULT: $([ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ] && echo PASS || echo FAIL)"
[ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ] && exit 0 || exit 1
