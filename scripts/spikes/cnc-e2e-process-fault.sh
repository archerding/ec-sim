#!/usr/bin/env bash
# task-102 F-PROC（进程级）：EC-Sim 进程崩溃/重启的可观察性与恢复自证。
#
# 说明：本脚本在 **EC-Sim 自己的验证机**上跑（Linux UDS），不需要 IgH/lcec/CNC VM。
# 它证明：
#   1. run_id=<A> 时 3/3 OP、/state 可读；
#   2. kill -9 后控制面不可达（连接失败），进程确实消失；
#   3. 以 run_id=<B> 重启后 3/3 OP 恢复、hold 样本继续、run_id 变化可区分现场。
# 注：F-PROC 的“过程数据确定性错误”分支由 fault_matrix_probe.py 的 pdo_corrupt 用例覆盖；
#     本脚本覆盖审计文档另一口径（EC-Sim 进程 kill/重启）。
#
# 用法：
#   bash scripts/spikes/cnc-e2e-process-fault.sh [--hold-seconds 8] [--port 18135]
# 证据：LOG_DIR（默认 .scratch/e2e-process-fault-<ts>）下 server-a/b.log、hold-a/b.out、
#       state-a/b.json、timeline.log、summary.txt，末尾 RESULT 行。
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT" || exit 1

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SKIP: F-PROC 进程故障需要 Linux（UDS 运行时）。"
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

HOLD_SECONDS=8
PORT=18135
while [[ $# -gt 0 ]]; do
  case "$1" in
    --hold-seconds) HOLD_SECONDS="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    *) echo "未知参数：$1"; exit 2 ;;
  esac
done

LOG_DIR="${LOG_DIR:-$ROOT/.scratch/e2e-process-fault-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$LOG_DIR"
SUMMARY="$LOG_DIR/summary.txt"
TIMELINE="$LOG_DIR/timeline.log"
SOCK="$LOG_DIR/uds.sock"
HTTP_BASE="http://127.0.0.1:$PORT"
MANIFEST="plugins/cnc-csp-servo/plugin.yaml"
MANIFEST_SHA="$(sha256sum "$MANIFEST" | awk '{print $1}')"
BIN_SHA="$(sha256sum "$BIN" | awk '{print $1}')"
SERVER_A_PID=""
SERVER_B_PID=""
HOLD_A_PID=""
HOLD_B_PID=""

timestamp() { date -Is; }
record() { echo "$(timestamp) $*" | tee -a "$TIMELINE"; }
cleanup() {
  for pid in "$HOLD_A_PID" "$HOLD_B_PID" "$SERVER_A_PID" "$SERVER_B_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then kill "$pid" 2>/dev/null || true; fi
  done
  rm -f "$SOCK" "$SOCK.client"* 2>/dev/null || true
}
trap cleanup EXIT

wait_health() {
  for _ in $(seq 1 100); do
    if curl -sf --max-time 1 "$HTTP_BASE/health" 2>/dev/null | grep -q 'ec-sim-control-plane'; then
      return 0
    fi
    sleep 0.05
  done
  return 1
}

state_json() { curl -sf --max-time 1 "$HTTP_BASE/state" 2>/dev/null; }

state_op_slaves() {
  state_json | python3 -c 'import json,sys; print(json.load(sys.stdin)["master"]["op_slaves"])' 2>/dev/null || echo ""
}

state_run_id() {
  state_json | python3 -c 'import json,sys; print(json.load(sys.stdin).get("run_id"))' 2>/dev/null || echo ""
}

wait_all_op() {
  local deadline=$((SECONDS + 8))
  while (( SECONDS < deadline )); do
    if [[ "$(state_op_slaves)" == "3" ]]; then return 0; fi
    sleep 0.1
  done
  return 1
}

{
  echo "stage=process-fault"
  echo "date=$(date -Is)"
  echo "binary=$BIN"
  echo "binary_sha256=$BIN_SHA"
  echo "manifest=$MANIFEST"
  echo "manifest_sha256=$MANIFEST_SHA"
  echo "slaves=3 behavior=cia402 hold_seconds=$HOLD_SECONDS port=$PORT"
  echo "note=EC-Sim process killed and restarted; run_id across restarts recorded"
} >"$SUMMARY"

fail() {
  record "FAIL: $*"
  echo "RESULT: FAIL（$*）" | tee -a "$SUMMARY"
  exit 1
}

RUN_A="proc-fault-a-$(date +%s)"
record "start server-a run_id=$RUN_A"
"$BIN" --uds "$SOCK" --slaves 3 --manifest "$MANIFEST" --behavior cia402 \
  --serve "127.0.0.1:$PORT" --serve-write --run-id "$RUN_A" \
  >"$LOG_DIR/server-a.log" 2>&1 &
SERVER_A_PID=$!
wait_health || fail "server-a 未就绪（see server-a.log）"
record "server-a pid=$SERVER_A_PID healthy"

python3 scripts/spikes/uds_op_hold.py "$SOCK" --http-base "$HTTP_BASE" \
  --expect-slaves 3 --hold-seconds "$HOLD_SECONDS" >"$LOG_DIR/hold-a.out" 2>&1 &
HOLD_A_PID=$!
wait_all_op || fail "阶段 A 未达到 3/3 OP"
state_json >"$LOG_DIR/state-a.json"
record "stage A 3/3 OP run_id=$(state_run_id)"

record "kill -9 server-a pid=$SERVER_A_PID"
kill -9 "$SERVER_A_PID" 2>/dev/null || true
wait "$SERVER_A_PID" 2>/dev/null || true
SERVER_A_PID=""
if [[ -n "$HOLD_A_PID" ]] && kill -0 "$HOLD_A_PID" 2>/dev/null; then kill "$HOLD_A_PID" 2>/dev/null || true; fi
wait "$HOLD_A_PID" 2>/dev/null || true
HOLD_A_PID=""

unreachable=0
for _ in $(seq 1 40); do
  if ! curl -sf --max-time 1 "$HTTP_BASE/state" >/dev/null 2>&1; then
    unreachable=1
    break
  fi
  sleep 0.05
done
[[ "$unreachable" == "1" ]] || fail "kill 后 /state 仍可达（控制面未停止）"
record "stage B 控制面不可达（连接失败）"

RUN_B="proc-fault-b-$(date +%s)"
record "start server-b run_id=$RUN_B"
"$BIN" --uds "$SOCK" --slaves 3 --manifest "$MANIFEST" --behavior cia402 \
  --serve "127.0.0.1:$PORT" --serve-write --run-id "$RUN_B" \
  >"$LOG_DIR/server-b.log" 2>&1 &
SERVER_B_PID=$!
wait_health || fail "server-b 未就绪（see server-b.log）"
record "server-b pid=$SERVER_B_PID healthy"

python3 scripts/spikes/uds_op_hold.py "$SOCK" --http-base "$HTTP_BASE" \
  --expect-slaves 3 --hold-seconds "$HOLD_SECONDS" >"$LOG_DIR/hold-b.out" 2>&1 &
HOLD_B_PID=$!
wait_all_op || fail "重启后未达到 3/3 OP"
state_json >"$LOG_DIR/state-b.json"
RUN_B_OBSERVED="$(state_run_id)"
record "stage C 3/3 OP run_id=$RUN_B_OBSERVED"

[[ "$RUN_B_OBSERVED" == "$RUN_B" ]] || fail "重启后 run_id 期望 $RUN_B 实际 $RUN_B_OBSERVED"
[[ "$RUN_A" != "$RUN_B_OBSERVED" ]] || fail "重启前后 run_id 未变化"

wait "$HOLD_B_PID" 2>/dev/null
hold_b_exit=$?
HOLD_B_PID=""
if [[ "$hold_b_exit" != "0" ]]; then
  fail "hold-b 退出码 $hold_b_exit（see hold-b.out）"
fi
if ! grep -q '^HOLD_OK ' "$LOG_DIR/hold-b.out"; then
  fail "重启后 hold-b 未输出 HOLD_OK（see hold-b.out）"
fi
record "stage C hold-b HOLD_OK"

{
  echo "run_a=$RUN_A"
  echo "run_b=$RUN_B"
  echo "state_a_op_slaves=3"
  echo "state_b_op_slaves=3"
  echo "control_plane_unreachable_after_kill=yes"
} >>"$SUMMARY"

echo "RESULT: PASS（进程 kill → 控制面不可达 → 重启 3/3 OP，run_id $RUN_A → $RUN_B；binary_sha256=$BIN_SHA manifest_sha256=$MANIFEST_SHA）" | tee -a "$SUMMARY"
exit 0
