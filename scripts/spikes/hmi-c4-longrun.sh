#!/usr/bin/env bash
# H3/C4：8 站实时 4h 长稳采集（EC-Sim VM；TAP 需要 root）。
# 采集：RSS/fd/线程数、/state 与 /pdo 延迟、/metrics 计数、SSE 事件数/重连、HTTP 非 200 次数；
#      每 60 s 一条 CSV；结束输出 summary。失败/异常如实记录到 log/CSV（不伪造 PASS）。
# 用法：sudo DURATION=14400 ./hmi-c4-longrun.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
TAP="${TAP:-escsim0}"
PORT="${PORT:-18210}"
SLAVES="${SLAVES:-8}"
WEBROOT="${WEBROOT:-web}"
DURATION="${DURATION:-14400}" # 4h
OUT="${OUT:-/tmp/hmi-c4}"
SAMPLE_SECONDS="${SAMPLE_SECONDS:-60}"
mkdir -p "$OUT"

"$BIN" --version >"$OUT/version.txt" 2>&1 || true
sha256sum "$BIN" >>"$OUT/version.txt" 2>&1 || true

cleanup() {
  for pid in ${SIM_PID:-} ${SSE_PID:-}; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

ip link del "$TAP" 2>/dev/null || true
"$BIN" --tap "$TAP" --slaves "$SLAVES" --serve "0.0.0.0:$PORT" --web-root "$WEBROOT" --serve-write \
  >"$OUT/sim.log" 2>&1 &
SIM_PID=$!

ip link set "$TAP" up 2>/dev/null || true
ip link set "$TAP" promisc on 2>/dev/null || true
for _ in $(seq 1 100); do
  if curl -fsS --max-time 1 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then break; fi
  sleep 0.2
done
curl -fsS --max-time 2 "http://127.0.0.1:$PORT/state?slave=0" >"$OUT/state-initial.json" || true

# SSE 消费者：持续读 /events，统计消息数与异常（重连由浏览器/客户端负责，这里只消费不重连）。
python3 - "$PORT" >"$OUT/sse.log" 2>&1 <<'PY' &
import sys
import time
import urllib.request

port = sys.argv[1]
url = f"http://127.0.0.1:{port}/events?slave=0"
started = time.time()
count = 0
try:
    with urllib.request.urlopen(url, timeout=30) as response:
        while True:
            line = response.readline()
            if not line:
                break
            if line.startswith(b"data:"):
                count += 1
                print(f"{time.time():.3f} event {count}", flush=True)
except Exception as error:  # 断开/超时如实记录
    print(f"{time.time():.3f} SSE_ERROR {error}", flush=True)
PY
SSE_PID=$!

echo "ts,rss_kb,fds,threads,state_ms,pdo_ms,al_status,frames_in,mailbox_req,sse_events,http_fail" >"$OUT/samples.csv"
start=$(date +%s)
http_fail=0
while :; do
  now=$(date +%s)
  elapsed=$((now - start))
  [ "$elapsed" -ge "$DURATION" ] && break
  state_metrics=$(curl -fsS --max-time 3 -o /dev/null -w '%{time_total}' "http://127.0.0.1:$PORT/state?slave=0" 2>>"$OUT/curl-errors.log") || { state_metrics=0; http_fail=$((http_fail + 1)); }
  pdo_metrics=$(curl -fsS --max-time 3 -o /dev/null -w '%{time_total}' "http://127.0.0.1:$PORT/pdo?slave=0" 2>>"$OUT/curl-errors.log") || { pdo_metrics=0; http_fail=$((http_fail + 1)); }
  metrics=$(curl -fsS --max-time 3 "http://127.0.0.1:$PORT/metrics?slave=0" 2>>"$OUT/curl-errors.log") || { metrics=""; http_fail=$((http_fail + 1)); }
  al=$(printf '%s\n' "$metrics" | awk '/^ecsim_al_status\{/ {print $2; exit}')
  frames=$(printf '%s\n' "$metrics" | awk '/^ecsim_frames_in_total\{/ {print $2; exit}')
  mbox=$(printf '%s\n' "$metrics" | awk '/^ecsim_mailbox_requests_total\{/ {print $2; exit}')
  rss=$(awk '/VmRSS/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")
  fds=$(ls "/proc/$SIM_PID/fd" 2>/dev/null | wc -l)
  threads=$(awk '/Threads/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")
  sse_events=$(grep -c " event " "$OUT/sse.log" 2>/dev/null || true)
  sse_events=${sse_events:-0}
  echo "$(date -Is),${rss:-},${fds:-},${threads:-},${state_metrics},${pdo_metrics},${al:-},${frames:-},${mbox:-},${sse_events},${http_fail}" >>"$OUT/samples.csv"
  sleep "$SAMPLE_SECONDS"
done

curl -fsS --max-time 3 "http://127.0.0.1:$PORT/state?slave=0" >"$OUT/last-state.json" 2>"$OUT/last-state.err" || true
{
  echo "duration_seconds=$(( $(date +%s) - start ))"
  echo "http_fail_total=$http_fail"
  echo "sse_events_total=$(grep -c ' event ' "$OUT/sse.log" 2>/dev/null || echo 0)"
  echo "sse_errors=$(grep -c 'SSE_ERROR' "$OUT/sse.log" 2>/dev/null || echo 0)"
  echo "samples_rows=$(wc -l <"$OUT/samples.csv")"
  echo "final_rss_kb=$(awk '/VmRSS/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")"
  echo "final_fds=$(ls "/proc/$SIM_PID/fd" 2>/dev/null | wc -l)"
  echo "final_threads=$(awk '/Threads/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")"
} >"$OUT/summary.txt"
cat "$OUT/summary.txt"
