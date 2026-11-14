#!/usr/bin/env bash
# task-101：CNC E2E 固定场景（EC-Sim 侧自证）3×CiA402 CSP，UDS 合成主站保持 OP。
#
# 说明：本脚本在 **EC-Sim 自己的验证机**上跑，不需要 IgH/lcec/CNC VM。
# 它证明：同一 manifest 的 3 个从站在连续周期帧下可稳定 OP、PDO 布局 7B/11B、
# /state 遥测一致。CNC 侧 TAP+IgH+lcec 的端到端验收仍由 C5/CNC VM 承担。
#
# 用法：
#   bash scripts/spikes/cnc-e2e-fixed-scene.sh [--rounds 3] [--hold-seconds 20] [--port 18210]
# 证据：LOG_DIR（默认 ../.scratch/e2e-fixed-scene-<ts>）下每轮 server.log/hold.out/state.json，
#       末尾 summary.txt 与 RESULT 行。
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT" || exit 1

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SKIP: 固定场景需要 Linux（UDS/FD 运行时）。"
  exit 2
fi

BIN="${EC_SIM_BIN:-}"
if [[ -z "$BIN" ]]; then
  for candidate in target/release/ec-sim-esc target/debug/ec-sim-esc; do
    if [[ -x "$candidate" ]]; then BIN="$candidate"; break; fi
  done
fi
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
  echo "SKIP: 找不到 ec-sim-esc（用 EC_SIM_BIN 指定）。"
  exit 2
fi

ROUNDS=3
HOLD_SECONDS=20
PORT=18210
while [[ $# -gt 0 ]]; do
  case "$1" in
    --rounds) ROUNDS="$2"; shift 2 ;;
    --hold-seconds) HOLD_SECONDS="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    *) echo "未知参数：$1"; exit 2 ;;
  esac
done

LOG_DIR="${LOG_DIR:-$ROOT/.scratch/e2e-fixed-scene-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$LOG_DIR"
SUMMARY="$LOG_DIR/summary.txt"

MANIFEST="plugins/cnc-csp-servo/plugin.yaml"
MANIFEST_SHA="$(sha256sum "$MANIFEST" | awk '{print $1}')"
BIN_SHA="$(sha256sum "$BIN" | awk '{print $1}')"
{
  echo "stage=fixed-scene"
  echo "date=$(date -Is)"
  echo "binary=$BIN"
  echo "binary_sha256=$BIN_SHA"
  echo "manifest=$MANIFEST"
  echo "manifest_sha256=$MANIFEST_SHA"
  echo "slaves=3 behavior=cia402 layout=tx7B/rx11B"
  echo "rounds=$ROUNDS hold_seconds=$HOLD_SECONDS port=$PORT"
  echo "note=EC-Sim VM UDS synthetic master; CNC VM TAP+IgH+lcec E2E is separate"
} >"$SUMMARY"

PASS=0
declare -a ROUND_RESULTS=()
for round in $(seq 1 "$ROUNDS"); do
  round_dir="$LOG_DIR/round-$round"
  mkdir -p "$round_dir"
  sock="$round_dir/uds.sock"
  run_id="fixed-scene-$(date +%s)-r$round"
  echo "== 轮次 $round/$ROUNDS（run_id=$run_id）=="

  "$BIN" --uds "$sock" --slaves 3 \
    --manifest "$MANIFEST" --behavior cia402 \
    --serve "127.0.0.1:$PORT" --serve-write --run-id "$run_id" \
    >"$round_dir/server.log" 2>&1 &
  server_pid=$!

  ready=0
  for _ in $(seq 1 100); do
    if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then ready=1; break; fi
    sleep 0.1
  done
  round_ok=0
  if [[ "$ready" == "1" ]]; then
    timeout -k 5 $((HOLD_SECONDS + 30)) python3 scripts/spikes/uds_op_hold.py "$sock" \
      --http-base "http://127.0.0.1:$PORT" \
      --expect-slaves 3 --hold-seconds "$HOLD_SECONDS" \
      >"$round_dir/hold.out" 2>&1
    hold_exit=$?
    curl -fsS "http://127.0.0.1:$PORT/state" -o "$round_dir/state.json" 2>/dev/null || true
    if [[ "$hold_exit" == "0" ]] && grep -q '^HOLD_OK ' "$round_dir/hold.out"; then
      round_ok=1
    else
      echo "轮次 $round FAIL（hold_exit=$hold_exit）"
      tail -20 "$round_dir/hold.out" || true
    fi
  else
    echo "轮次 $round FAIL（服务未就绪）"
    sed -n '1,30p' "$round_dir/server.log" || true
  fi

  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true

  if [[ "$round_ok" == "1" ]]; then
    PASS=$((PASS + 1))
    ROUND_RESULTS+=("round-$round PASS")
    echo "round-$round PASS ($(grep '^HOLD_OK' "$round_dir/hold.out" | head -1))"
  else
    ROUND_RESULTS+=("round-$round FAIL")
    echo "round-$round FAIL"
  fi
  echo "${ROUND_RESULTS[-1]}" >>"$SUMMARY"
done

{
  echo "pass_rounds=$PASS/$ROUNDS"
  echo "expected=3/3 OP per round"
} >>"$SUMMARY"

if [[ "$PASS" == "$ROUNDS" ]]; then
  echo "RESULT: PASS（$PASS/$ROUNDS 轮 3/3 OP，binary_sha256=$BIN_SHA manifest_sha256=$MANIFEST_SHA）"
  echo "RESULT: PASS" >>"$SUMMARY"
  exit 0
fi
echo "RESULT: FAIL（$PASS/$ROUNDS）"
echo "RESULT: FAIL" >>"$SUMMARY"
exit 1
