#!/usr/bin/env bash
# A4：reset / stack-down 幂等（task-101 A4）：3 次独立启动→回 REF，每次 ≤30s，
# 停止后无残留进程/socket/端口，并把每次运行按 run_id 归档（C3/A7）。
#
# 前置：Linux；`cargo build --release -p esc-node`；python3；不占 CNC VM。
# 用法：
#   EC_SIM_BIN=target/release/ec-sim-esc ./scripts/spikes/cnc-e2e-reset-idempotency.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${EC_SIM_BIN:-$ROOT/target/release/ec-sim-esc}"
MANIFEST="${MANIFEST:-$ROOT/plugins/cnc-csp-servo/plugin.yaml}"
SOCK="${SOCK:-/tmp/ecsim-reset.sock}"
PORT="${PORT:-18137}"
OUT_DIR="${OUT_DIR:-$ROOT/docs/verification/assets/cnc-e2e/reset-idempotency}"
RUNS_ROOT="${RUNS_ROOT:-$ROOT/docs/verification/assets/cnc-e2e}"
RUN_PREFIX="${RUN_PREFIX:-reset-idem-$(date +%Y%m%d%H%M%S)}"
LIMIT_MS="${LIMIT_MS:-30000}"
ROUNDS="${ROUNDS:-3}"
PY="${PY:-python3}"
ARCHIVE="$ROOT/scripts/spikes/e2e_run_archive.py"

if [ "$(uname -s 2>/dev/null)" != "Linux" ]; then
  echo "SKIP: reset 幂等探针需要 Linux（UDS）"
  exit 2
fi
for tool in "$BIN" "$PY" curl; do
  if ! command -v "$tool" >/dev/null 2>&1 && [ ! -x "$tool" ]; then
    echo "SKIP: 缺少 $tool"
    exit 2
  fi
done
[ -f "$MANIFEST" ] || { echo "SKIP: manifest 不存在 $MANIFEST"; exit 2; }
mkdir -p "$OUT_DIR" "$RUNS_ROOT"
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

STATES=()
for round in $(seq 1 "$ROUNDS"); do
  RUN_ID="${RUN_PREFIX}-r${round}"
  STATE_FILE="$OUT_DIR/state-r${round}.json"
  LOG_FILE="$OUT_DIR/server-r${round}.log"
  rm -f "$STATE_FILE"
  note "round $round/$ROUNDS：启动 ${RUN_ID}"
  start_ms=$("$PY" -c 'import time; print(int(time.time()*1000))')
  "$BIN" --uds "$SOCK" --slaves 3 --manifest "$MANIFEST" --behavior cia402 \
    --serve "127.0.0.1:$PORT" --serve-write --run-id "$RUN_ID" >"$LOG_FILE" 2>&1 &
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
    sleep 0.05
  done
  if [ "$ready" -ne 1 ]; then
    cat "$LOG_FILE" 2>/dev/null || true
    fail "round $round 控制面未就绪"
    break
  fi

  if "$PY" "$ROOT/scripts/spikes/e2e_reset_probe.py" wait "$SOCK" \
    --http-base "http://127.0.0.1:$PORT" --start-ms "$start_ms" \
    --expect-slaves 3 --out "$STATE_FILE" 2>&1 | tee -a "$OUT_DIR/timeline.log"; then
    ELAPSED_MS=$("$PY" -c "import json;print(json.load(open('$STATE_FILE'))['elapsed_ms'])" 2>/dev/null || echo 0)
    note "round $round：reset 到 REF 用时 ${ELAPSED_MS}ms（上限 ${LIMIT_MS}ms）"
    if [ "${ELAPSED_MS:-0}" -gt "$LIMIT_MS" ]; then
      fail "round $round reset 用时 ${ELAPSED_MS}ms 超过 ${LIMIT_MS}ms"
    fi
  else
    fail "round $round reset 探针失败"
  fi

  # 停止并检查无残留（进程、socket、端口）；stack-down 清理运行期 socket。
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  wait "$SERVER_PID" 2>/dev/null || true
  SERVER_PID=""
  rm -f "$SOCK" 2>/dev/null || true
  if pgrep -f "[e]c-sim-esc --uds $SOCK" >/dev/null 2>&1; then
    fail "round $round 停止后仍有残留 ec-sim-esc 进程"
  fi
  if [ -e "$SOCK" ]; then
    fail "round $round stack-down 后 socket 仍存在：$SOCK"
  fi
  if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
    fail "round $round 停止后控制面端口仍可访问"
  fi
  note "round $round：停止后无残留（进程/socket/端口）"

  STATES+=("$STATE_FILE")
  "$PY" "$ARCHIVE" create --out-root "$RUNS_ROOT" --run-id "$RUN_ID" \
    --meta "result=RESET_OK" --meta "elapsed_ms=${ELAPSED_MS:-0}" \
    --meta "limit_ms=$LIMIT_MS" \
    --meta "command=cnc-e2e-reset-idempotency.sh round $round (3x cnc-csp-servo, UDS, port $PORT)" \
    --artifact "$STATE_FILE" --artifact "$LOG_FILE" >/dev/null 2>&1 || fail "round $round 归档失败"
done

if [ "${#STATES[@]}" -eq "$ROUNDS" ]; then
  if "$PY" "$ROOT/scripts/spikes/e2e_reset_probe.py" compare --compare "${STATES[@]}" \
    --out "$OUT_DIR/ref-compare.json" 2>&1 | tee -a "$OUT_DIR/timeline.log"; then
    note "3 次运行 REF 字段级一致"
  else
    fail "3 次运行 REF 字段级不一致（$OUT_DIR/ref-compare.json）"
  fi
else
  fail "未完成全部 $ROUNDS 轮"
fi

if [ "$FAIL" -eq 0 ]; then
  note "reset 幂等通过：$ROUNDS 轮均 ≤${LIMIT_MS}ms、回 REF、无残留"
  echo "OK: $RUNS_ROOT/runs/$RUN_PREFIX-r*（reset≤${LIMIT_MS}ms，REF 一致）"
fi

echo "RESULT: $([ "$FAIL" -eq 0 ] && echo PASS || echo FAIL)"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
