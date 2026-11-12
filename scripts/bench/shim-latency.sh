#!/usr/bin/env bash
# M1 退出项（部分）：**shim 延迟基准**——启动 `ec-sim-esc --uds`（可多从站），
# 用 `scripts/bench/uds-latency.py` 顺序发 APRD 帧，输出 p50/p99/p999。
#
# 说明：这是**同机回归基准**，不是实时性承诺；绝对值随 CPU/调度/隔离配置变化。
# 可用 SIM_CPU / PROBE_CPU 绑定 CPU，SIM_RT / PROBE_RT 指定 chrt FIFO 优先级：
#   SIM_CPU=2 PROBE_CPU=3 PROBE_RT=80 ./shim-latency.sh
#   N=8 SIM_CPU=2 SIM_RT=50 PROBE_CPU=3 PROBE_RT=80 ./shim-latency.sh
#
# 用法：./shim-latency.sh        # 默认 1 从站、2000 帧、不绑核不 RT
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
N="${N:-1}"
FRAMES="${FRAMES:-2000}"
SOCK="${SOCK:-/tmp/ecsim-latency.sock}"
SIM_CPU="${SIM_CPU:-}"
PROBE_CPU="${PROBE_CPU:-}"
SIM_RT="${SIM_RT:-0}"     # 非 0 = chrt -f <优先级>
PROBE_RT="${PROBE_RT:-0}"
LOG=/tmp/ec-sim-esc-latency.log

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
sim_cmd=("$BIN" --uds "$SOCK" --slaves "$N")
if [ -n "$SIM_CPU" ]; then
  sim_cmd=(taskset -c "$SIM_CPU" "${sim_cmd[@]}")
fi
if [ "$SIM_RT" != "0" ]; then
  sim_cmd=(chrt -f "$SIM_RT" "${sim_cmd[@]}")
fi
"${sim_cmd[@]}" >"$LOG" 2>&1 &
ec_pid=$!
for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
[ -S "$SOCK" ] || { cat "$LOG"; echo "FAIL: 模拟器未创建 UDS socket $SOCK"; exit 1; }

probe_cmd=("$PY" scripts/bench/uds-latency.py "$SOCK" "$FRAMES")
if [ -n "$PROBE_CPU" ]; then
  probe_cmd=(taskset -c "$PROBE_CPU" "${probe_cmd[@]}")
fi
if [ "$PROBE_RT" != "0" ]; then
  probe_cmd=(chrt -f "$PROBE_RT" "${probe_cmd[@]}")
fi
"${probe_cmd[@]}"

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
echo "OK: shim 延迟基准完成（N=$N frames=$FRAMES sim_cpu=${SIM_CPU:-any} sim_rt=$SIM_RT probe_cpu=${PROBE_CPU:-any} probe_rt=$PROBE_RT）"
