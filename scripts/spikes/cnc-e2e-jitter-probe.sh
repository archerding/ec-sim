#!/usr/bin/env bash
# C1：周期抖动来源与量化（task-103 C1）。
#
# 前置：Linux；`cargo build --release -p esc-node`；python3；不占 CNC VM。
# 用法：
#   EC_SIM_BIN=target/release/ec-sim-esc ./scripts/spikes/cnc-e2e-jitter-probe.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${EC_SIM_BIN:-$ROOT/target/release/ec-sim-esc}"
MANIFEST="${MANIFEST:-$ROOT/plugins/cnc-csp-servo/plugin.yaml}"
SOCK="${SOCK:-/tmp/ecsim-jitter.sock}"
PORT="${PORT:-18138}"
OUT_DIR="${OUT_DIR:-$ROOT/docs/verification/assets/cnc-e2e/jitter}"
RUNS_ROOT="${RUNS_ROOT:-$ROOT/docs/verification/assets/cnc-e2e}"
RUN_ID="${RUN_ID:-jitter-$(date +%Y%m%d%H%M%S)}"
SAMPLES="${SAMPLES:-200}"
PY="${PY:-python3}"
ARCHIVE="$ROOT/scripts/spikes/e2e_run_archive.py"

if [ "$(uname -s 2>/dev/null)" != "Linux" ]; then
  echo "SKIP: 抖动探针需要 Linux（UDS）"
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

note "启动 ec-sim-esc --uds（3×cnc-csp-servo，run_id=$RUN_ID）"
"$BIN" --uds "$SOCK" --slaves 3 --manifest "$MANIFEST" --behavior cia402 \
  --serve "127.0.0.1:$PORT" --serve-write --run-id "$RUN_ID" >"$OUT_DIR/server.log" 2>&1 &
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
  cat "$OUT_DIR/server.log" 2>/dev/null || true
  echo "FAIL: 控制面未就绪（127.0.0.1:$PORT/health）"
  exit 1
fi
note "控制面就绪；测量 $SAMPLES 个周期"

set -o pipefail
"$PY" "$ROOT/scripts/spikes/c1_jitter_probe.py" "$SOCK" \
  --http-base "http://127.0.0.1:$PORT" --samples "$SAMPLES" \
  --out-dir "$OUT_DIR" 2>&1 | tee "$OUT_DIR/probe.out"
PROBE_EXIT=${PIPESTATUS[0]}
set +o pipefail

BIN_SHA="$(sha256sum "$BIN" 2>/dev/null | awk '{print $1}')"
MANIFEST_SHA="$(sha256sum "$MANIFEST" 2>/dev/null | awk '{print $1}')"
COMMIT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
RESULT="FAIL"
if [ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
  RESULT="PASS"
fi

note "归档 runs/$RUN_ID（binary=$BIN_SHA manifest=$MANIFEST_SHA）"
if "$PY" "$ARCHIVE" create --out-root "$RUNS_ROOT" --run-id "$RUN_ID" \
  --meta "result=$RESULT" --meta "commit=$COMMIT" --meta "binary_sha256=$BIN_SHA" \
  --meta "manifest_sha256=$MANIFEST_SHA" --meta "samples=$SAMPLES" \
  --meta "command=cnc-e2e-jitter-probe.sh (3x cnc-csp-servo, UDS, port $PORT)" \
  --artifact "$OUT_DIR" --artifact "$ROOT/scripts/spikes/c1_jitter_probe.py" \
  --artifact "$ROOT/scripts/spikes/uds_master_probe.py"; then
  "$PY" "$ARCHIVE" verify --dir "$RUNS_ROOT/runs/$RUN_ID" | tee -a "$OUT_DIR/timeline.log"
  [ "${PIPESTATUS[0]}" -eq 0 ] || fail "归档校验失败"
else
  fail "归档创建失败"
fi

if [ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
  note "抖动量化完成；证据：$OUT_DIR 与 $RUNS_ROOT/runs/$RUN_ID"
  echo "OK: ${RUN_ID} jitter 量化（SIM 侧来源与分位数见 jitter.json）"
else
  fail "抖动探针退出码 $PROBE_EXIT"
fi

echo "RESULT: $([ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ] && echo PASS || echo FAIL)"
[ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ] && exit 0 || exit 1
