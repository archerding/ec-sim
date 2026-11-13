#!/usr/bin/env bash
# task-66 C4 v2：4h 全程主站在线的控制面采样（供 systemd-run 使用，不依赖 SSH 会话）。
# 用法（VM，root）：
#   systemd-run --unit=ecsim-c4v2 --collect --property=Type=exec --scope /bin/bash /root/c4v2-run.sh
# 或直接：setsid nohup /root/c4v2-run.sh &
# 产物：/tmp/hmi-c4-v2/{samples.csv,summary.txt,last-state.json,sim.log,master.log,sse.log}
set -u

BIN="${BIN:-/opt/ecsim/work/ec-sim-team4/target/release/ec-sim-esc}"
WEBROOT="${WEBROOT:-/opt/ecsim/work/ec-sim-team4/web}"
MANIFEST="${MANIFEST:-/opt/ecsim/work/ec-sim-team4/plugins/cnc-csp-servo/plugin.yaml}"
TAP="${TAP:-escsim0}"
SOEM="${SOEM:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
PORT="${PORT:-18210}"
DURATION="${DURATION:-14400}"
SAMPLE_SECONDS="${SAMPLE_SECONDS:-60}"
OUT="${OUT:-/tmp/hmi-c4-v2}"
mkdir -p "$OUT"
# 单实例保护（防两个 unit 同时跑）
exec 9>/tmp/hmi-c4-v2.lock
if ! flock -n 9; then echo "C4V2 already running"; exit 9; fi
echo "C4V2 start $(date +%Is) bin=$(sha256sum "$BIN" | cut -d' ' -f1) tap=$TAP" >"$OUT/run.log"

cleanup() {
  kill "${SIM_PID:-0}" "${MASTER_PID:-0}" "${SSE_PID:-0}" 2>/dev/null
  wait 2>/dev/null
  ip link del "$TAP" 2>/dev/null
  # 清理证明（sim-expert 口径 ⑤）：TAP/进程均无残留
  {
    echo "cleanup_at=$(date +%Is)"
    ip link show "$TAP" 2>&1 || echo "tap_absent"
    pgrep -af "ec-sim-esc --tap $TAP" || echo "sim_absent"
    pgrep -af "simple_ng $TAP" || echo "master_absent"
  } >"$OUT/cleanup-proof.txt" 2>&1
}
trap cleanup EXIT

pkill -f "ec-sim-esc --tap $TAP" 2>/dev/null
pkill -f "simple_ng $TAP" 2>/dev/null
sleep 1
ip link del "$TAP" 2>/dev/null

"$BIN" --tap "$TAP" --slaves 8 --manifest "$MANIFEST" --serve "0.0.0.0:$PORT" --web-root "$WEBROOT" --serve-write >"$OUT/sim.log" 2>&1 &
SIM_PID=$!
ip link set "$TAP" up 2>/dev/null || true
ip link set "$TAP" promisc on 2>/dev/null || true
for _ in $(seq 1 100); do
  curl -fsS --max-time 1 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break
  sleep 0.2
done
curl -fsS --max-time 3 "http://127.0.0.1:$PORT/metrics?slave=0" >"$OUT/metrics-before.txt" 2>&1 || true
echo "WKC_NOTE: simple_ng WKC 提示为已知夹具现象（EL2008 probe 注释），不作为产品结论；只要 frames_in>0/AL 进 OP 即判主站流量已连通" >"$OUT/wkc-note.txt"

# 单一连续主站会话（task-78：禁止 4h 内重启 master）：
# 优先 /root/ecsim-master-long（长跑 SOEM 主站，一次会话跑满时长）；缺失则回退 SOEM 示例循环并标记 harness_restart。
HARNESS_MASTER="single_session"
if [ -x /root/ecsim-master-long ]; then
  MASTER_HOURS=$(awk -v s="$DURATION" 'BEGIN{printf "%.2f", (s+300)/3600}')
  "$ (/bin/true)" 2>/dev/null || true
  (
    /root/ecsim-master-long "$TAP" "$MASTER_HOURS" >>"$OUT/master.log" 2>&1
    echo "MASTER_LONG exit=$? $(date -Is)" >>"$OUT/master.log"
  ) &
  MASTER_PID=$!
else
  HARNESS_MASTER="soem_restart_loop"
  (
    while :; do
      "$SOEM" "$TAP" >>"$OUT/master.log" 2>&1 || true
      echo "MASTER_LOOP restart $(date -Is)" >>"$OUT/master.log"
      sleep 1
    done
  ) &
  MASTER_PID=$!
fi

# SSE 消费计数（真实 /events 推送；断开则记录并重连，统计重连次数）
(
  connects=0
  while :; do
    connects=$((connects + 1))
    echo "SSE_CONNECT $connects $(date +%Is)" >>"$OUT/sse.log"
    curl -fsSN --max-time 60 "http://127.0.0.1:$PORT/events?slave=0" 2>/dev/null | while IFS= read -r line; do
      case "$line" in data:*) echo "EVENT $(date +%Is) ${line:6:80}" >>"$OUT/sse.log" ;; esac
    done
    echo "SSE_DISCONNECT $connects $(date +%Is)" >>"$OUT/sse.log"
    sleep 1
  done
) &
SSE_PID=$!

echo "ts,rss_kb,fds,threads,state_s,pdo_s,al_status,frames_in,datagrams_hit,sse_events,sse_connects,http_fail" >"$OUT/samples.csv"
start=$(date +%s)
http_fail=0
while :; do
  elapsed=$(( $(date +%s) - start ))
  [ "$elapsed" -ge "$DURATION" ] && break
  state_s=$(curl -fsS --max-time 3 -o /dev/null -w '%{time_total}' "http://127.0.0.1:$PORT/state?slave=0" 2>>"$OUT/curl-errors.log") || { state_s=0; http_fail=$((http_fail + 1)); }
  pdo_s=$(curl -fsS --max-time 3 -o /dev/null -w '%{time_total}' "http://127.0.0.1:$PORT/pdo?slave=0" 2>>"$OUT/curl-errors.log") || { pdo_s=0; http_fail=$((http_fail + 1)); }
  metrics=$(curl -fsS --max-time 3 "http://127.0.0.1:$PORT/metrics?slave=0" 2>>"$OUT/curl-errors.log") || { metrics=""; http_fail=$((http_fail + 1)); }
  al=$(printf '%s\n' "$metrics" | awk '/^ecsim_al_status\{/ {print $2; exit}')
  frames=$(printf '%s\n' "$metrics" | awk '/^ecsim_frames_in_total\{/ {print $2; exit}')
  hits=$(printf '%s\n' "$metrics" | awk '/^ecsim_datagrams_hit_total\{/ {print $2; exit}')
  rss=$(awk '/VmRSS/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")
  fds=$(ls "/proc/$SIM_PID/fd" 2>/dev/null | wc -l | tr -d ' ')
  threads=$(awk '/Threads/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")
  events=$(grep -c '^EVENT ' "$OUT/sse.log" 2>/dev/null || true); events=${events:-0}
  connects=$(grep -c '^SSE_CONNECT ' "$OUT/sse.log" 2>/dev/null || true); connects=${connects:-0}
  echo "$(date -Is),${rss:-},${fds:-},${threads:-},${state_s},${pdo_s},${al:-},${frames:-},${hits:-},${events},${connects},${http_fail}" >>"$OUT/samples.csv"
  sleep "$SAMPLE_SECONDS"
done

curl -fsS --max-time 3 "http://127.0.0.1:$PORT/state?slave=0" >"$OUT/last-state.json" 2>"$OUT/last-state.err" || true
curl -fsS --max-time 3 "http://127.0.0.1:$PORT/metrics?slave=0" >"$OUT/metrics-after.txt" 2>&1 || true
curl -sS -o /dev/null -w 'last_state_code=%{http_code} time_total=%{time_total}\n' --max-time 3 "http://127.0.0.1:$PORT/state?slave=0" >"$OUT/last-state-curl.txt" 2>&1 || true
{
  echo "duration_seconds=$(( $(date +%s) - start ))"
  echo "http_fail_total=$http_fail"
  echo "frames_in_last=$(printf '%s\n' "$metrics" | awk '/^ecsim_frames_in_total\{/ {print $2; exit}')"
  echo "al_status_last=$(printf '%s\n' "$metrics" | awk '/^ecsim_al_status\{/ {print $2; exit}')"
  echo "sse_events_total=$(grep -c '^EVENT ' "$OUT/sse.log" 2>/dev/null || true)"
  echo "sse_connects_total=$(grep -c '^SSE_CONNECT ' "$OUT/sse.log" 2>/dev/null || true)"
  echo "samples_rows=$(wc -l <"$OUT/samples.csv")"
  echo "final_rss_kb=$(awk '/VmRSS/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")"
  echo "final_fds=$(ls "/proc/$SIM_PID/fd" 2>/dev/null | wc -l | tr -d ' ')"
  echo "final_threads=$(awk '/Threads/ {print $2}' "/proc/$SIM_PID/status" 2>/dev/null || echo "")"
  echo "last_state_curl=$(cat "$OUT/last-state-curl.txt" 2>/dev/null)"
  echo "master_mode=$HARNESS_MASTER"
} >"$OUT/summary.txt"
{
  python3 - "$OUT/samples.csv" >"$OUT/al-summary.txt" 2>&1 <<'PY'
import csv, sys
rows = [r for r in csv.DictReader(open(sys.argv[1], encoding="utf-8")) if r.get("al_status")]
vals = [int(float(r["al_status"])) for r in rows]
seconds = len(vals)
counts = {}
for v in vals: counts[v] = counts.get(v, 0) + 1
segments = []
for v in vals:
    if segments and segments[-1][0] == v: segments[-1][1] += 1
    else: segments.append([v, 1])
op = [v for v in vals if v & 0x0F == 8]
longest = max((n for v, n in segments if v & 0x0F == 8), default=0)
print("samples=%d" % seconds)
print("distribution=%s" % {hex(k): f"{v}/{seconds}" for k, v in sorted(counts.items())})
print("segments=%d" % len(segments))
print("op_samples=%d op_pct=%.2f%%" % (len(op), 100.0 * len(op) / seconds if seconds else 0))
print("longest_op_segment=%d" % longest)
print("op_ge_99pct=%s" % (len(op) >= 0.99 * seconds))
PY
  echo "al_summary_written=$(wc -l <"$OUT/al-summary.txt" 2>/dev/null || echo 0)"
} >>"$OUT/summary.txt" 2>&1cat "$OUT/summary.txt"
