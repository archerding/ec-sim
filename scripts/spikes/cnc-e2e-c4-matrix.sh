#!/usr/bin/env bash
# C4 用例×SIM 可观察量矩阵（task-103 C4）：10 类可执行覆盖 + 按 run 归档。
#
# 前置：Linux；`cargo build --release -p esc-node`；python3；不占 CNC VM。
# 用法：
#   EC_SIM_BIN=target/release/ec-sim-esc ./scripts/spikes/cnc-e2e-c4-matrix.sh
#   OUT_DIR=/tmp/c4 RUNS_ROOT=/tmp/runs ./scripts/spikes/cnc-e2e-c4-matrix.sh
#
# 10 类：program / jog / MDI / home / override / tool / offset / probe / alarm / link-loss。
# 说明：SIM 可观测等价路径（合成主站 + /state、TxPDO、/events、/metrics），
# 不代表 CNC 侧 HMI/桥功能验收；CNC 侧证据仍 PENDING。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${EC_SIM_BIN:-$ROOT/target/release/ec-sim-esc}"
MANIFEST="${MANIFEST:-$ROOT/plugins/cnc-csp-servo/plugin.yaml}"
SOCK="${SOCK:-/tmp/ecsim-c4-matrix.sock}"
PORT="${PORT:-18136}"
OUT_DIR="${OUT_DIR:-$ROOT/docs/verification/assets/cnc-e2e/c4-matrix}"
RUNS_ROOT="${RUNS_ROOT:-$ROOT/docs/verification/assets/cnc-e2e}"
RUN_ID="${RUN_ID:-c4-matrix-$(date +%Y%m%d%H%M%S)}"
PY="${PY:-python3}"
ARCHIVE="$ROOT/scripts/spikes/e2e_run_archive.py"

if [ "$(uname -s 2>/dev/null)" != "Linux" ]; then
  echo "SKIP: C4 矩阵探针需要 Linux（UDS）"
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
note "控制面就绪；开始 C4 10 类矩阵"

set -o pipefail
"$PY" "$ROOT/scripts/spikes/c4_matrix_probe.py" "$SOCK" \
  --http-base "http://127.0.0.1:$PORT" --out-dir "$OUT_DIR" 2>&1 | tee "$OUT_DIR/probe.out"
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
mkdir -p "$RUNS_ROOT"
if "$PY" "$ARCHIVE" create --out-root "$RUNS_ROOT" --run-id "$RUN_ID" \
  --meta "result=$RESULT" --meta "commit=$COMMIT" --meta "binary_sha256=$BIN_SHA" \
  --meta "manifest_sha256=$MANIFEST_SHA" \
  --meta "command=cnc-e2e-c4-matrix.sh (3x cnc-csp-servo, UDS, port $PORT)" \
  --artifact "$OUT_DIR" --artifact "$ROOT/scripts/spikes/c4_matrix_probe.py" \
  --artifact "$ROOT/scripts/spikes/uds_master_probe.py"; then
  note "归档完成；校验 sha256"
  "$PY" "$ARCHIVE" verify --dir "$RUNS_ROOT/runs/$RUN_ID" | tee -a "$OUT_DIR/timeline.log"
  VERIFY_EXIT=${PIPESTATUS[0]}
  if [ "$VERIFY_EXIT" -ne 0 ]; then
    fail "归档校验失败（$RUNS_ROOT/runs/$RUN_ID）"
  fi
else
  fail "归档创建失败"
fi

if [ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
  note "C4 10 类覆盖通过；证据：$OUT_DIR 与 $RUNS_ROOT/runs/$RUN_ID"
  echo "OK: ${RUN_ID} C4 10/10 类（SIM 可观测等价路径）"
else
  fail "C4 矩阵探针退出码 $PROBE_EXIT"
fi

echo "RESULT: $([ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ] && echo PASS || echo FAIL)"
[ "$PROBE_EXIT" -eq 0 ] && [ "$FAIL" -eq 0 ] && exit 0 || exit 1
