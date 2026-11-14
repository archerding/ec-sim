#!/usr/bin/env bash
# EC-Sim CNC E2E 控制面契约探针（Linux；需要 UDS + HTTP 同进程）。
#
# 覆盖 task-102/103 的最小闭环：
#   /state run_id+master+逐从站、POST /fault 门禁/confirm/run_id/target/幂等、
#   fault_injected/fault_cleared/link 事件、duration 到期恢复、
#   /probe tripped 与事件、/events?since 过滤、无写权限时 /fault=405。
#
# 用法：
#   EC_SIM_BIN=/path/to/ec-sim-esc bash scripts/spikes/e2e-control-probe.sh
#   # 缺省在 target/{release,debug}/ec-sim-esc 中查找
#
# 退出码：0=PASS，1=FAIL，2=SKIP（非 Linux 或找不到二进制）。
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT" || exit 1

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SKIP: e2e-control-probe 需要 Linux（UDS/TAP 运行时）。"
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

WORK="$(mktemp -d "${TMPDIR:-/tmp}/ecsim-e2e.XXXXXX")"
SOCK="$WORK/ecsim-e2e.sock"
SOCK_NOWRITE="$WORK/ecsim-e2e-nowrite.sock"
PORT="${E2E_HTTP_PORT:-18133}"
PORT_NOWRITE=$((PORT + 1))
RUN_ID="e2e-probe-run"
SERVER_PID=""
NOWRITE_PID=""

cleanup() {
  [[ -n "$NOWRITE_PID" ]] && kill "$NOWRITE_PID" 2>/dev/null || true
  [[ -n "$SERVER_PID" ]] && kill "$SERVER_PID" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

echo "== 启动 ec-sim-esc（--serve-write --run-id $RUN_ID）=="
"$BIN" --uds "$SOCK" --slaves 3 \
  --manifest plugins/cnc-csp-servo/plugin.yaml --behavior cia402 \
  --serve "127.0.0.1:$PORT" --serve-write --run-id "$RUN_ID" --web-root web \
  >"$WORK/server.log" 2>&1 &
SERVER_PID=$!

ready=0
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.1
done
if [[ "$ready" != "1" ]]; then
  echo "FAIL: 服务 10s 内未就绪（$PORT）"
  sed -n '1,40p' "$WORK/server.log"
  exit 1
fi
echo "OK: /health 就绪（$PORT）"

python3 scripts/spikes/e2e_control_probe.py \
  --http-base "http://127.0.0.1:$PORT" \
  --socket "$SOCK" \
  --run-id "$RUN_ID" \
  --expect-slaves 3
PROBE_EXIT=$?

# 无 --serve-write 时 POST /fault 必须 405（写门禁）。
echo "== 无写门禁实例（应 405）=="
"$BIN" --uds "$SOCK_NOWRITE" --slaves 1 \
  --serve "127.0.0.1:$PORT_NOWRITE" --run-id "$RUN_ID" \
  >"$WORK/nowrite.log" 2>&1 &
NOWRITE_PID=$!
ready=0
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:$PORT_NOWRITE/health" >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.1
done
if [[ "$ready" == "1" ]]; then
  code="$(curl -sS -o "$WORK/nowrite-fault.json" -w '%{http_code}' \
    -X POST -H 'Content-Type: application/json' \
    -d '{"fault":"link_loss","duration_ms":10,"confirm":true}' \
    "http://127.0.0.1:$PORT_NOWRITE/fault" || true)"
  if [[ "$code" == "405" ]]; then
    echo "OK: 无 --serve-write 时 POST /fault == 405"
  else
    echo "FAIL: 无 --serve-write 时 POST /fault == $code（期望 405）"
    PROBE_EXIT=1
  fi
else
  echo "FAIL: 无写门禁实例未就绪"
  PROBE_EXIT=1
fi

echo "== 服务日志（AL_EVENT 行）=="
grep -c '^AL_EVENT' "$WORK/server.log" 2>/dev/null || true

if [[ "$PROBE_EXIT" -ne 0 ]]; then
  echo "RESULT: FAIL（控制面探针）"
  exit 1
fi
echo "RESULT: PASS"
exit 0
