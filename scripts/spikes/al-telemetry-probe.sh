#!/usr/bin/env bash
# task-97 后续（C5 RCA 修复项）：AL 遥测 + SII 并发读 + 看门狗恢复预回归。
#
# 覆盖：
#   1. `--uds` 合成主站按 ESM 顺序进 OP（邮箱 SM → PREOP；过程数据 SM → SAFEOP → OP）；
#   2. 过程数据活跃时并发读 SII（word 0x0008 / Vendor ID），断言无 busy/error、AL 保持 OP；
#   3. 停过程数据 > SM watchdog 超时（10 ms）→ 下一帧触发 0x001B、OP→SAFEOP；
#   4. Error ACK + 重新请求 OP 后恢复，SII 仍可读；
#   5. 只读控制面 `/state`、`/metrics` 的新遥测字段：
#      `stats.al_transitions/al_errors/al_error_acks/sm_watchdog_triggers/sii_read_commands/sii_write_commands`、
#      `last_al_event`（watchdog 事件 from/to/code）、`ecsim_*_total` 计数器；
#   6. 进程 stderr 的 `AL_EVENT slave=... from=... to=... code=... watchdog=...` 运行期事件行
#      （看门狗事件与恢复事件各至少一条，且含 code=0x001B）。
#
# 前置：Linux（UDS 由 Unix domain socket 提供，天然 Linux）、cargo build --release -p esc-node；
#       无需 root、无 TAP，不占 CNC 验证机。
# 用法：./al-telemetry-probe.sh
# 环境：BIN=target/release/ec-sim-esc PY=python3 SOCK=... PORT=... LOG_DIR=...
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
SOCK="${SOCK:-/tmp/ecsim-al-telemetry.sock}"
PORT="${PORT:-18133}"
LOG_DIR="${LOG_DIR:-/tmp/ecsim-al-telemetry}"
mkdir -p "$LOG_DIR"

os="$(uname -s 2>/dev/null || echo unknown)"
is_linux="no"
[ "${os#Linux}" != "$os" ] && is_linux="yes"
[ -x "$BIN" ] || { echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"; exit 1; }
command -v "$PY" >/dev/null 2>&1 || { echo "FAIL: 找不到 Python：$PY"; exit 1; }

fail() {
  echo "FAIL: $1"
  exit 1
}

if [ "$is_linux" != "yes" ]; then
  echo "SKIP: AL 遥测探针需要 Linux（UDS），当前 $os"
  exit 2
fi

rm -f "$SOCK"
"$BIN" --uds "$SOCK" --slaves 1 --serve "127.0.0.1:$PORT" \
  >"$LOG_DIR/sim.log" 2>&1 &
SIM_PID=$!
cleanup() {
  kill "$SIM_PID" 2>/dev/null || true
  wait "$SIM_PID" 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 100); do [ -S "$SOCK" ] && break; sleep 0.05; done
[ -S "$SOCK" ] || { cat "$LOG_DIR/sim.log"; fail "模拟器未创建 UDS socket"; }

# 等控制面就绪（/health）。
ready="no"
for _ in $(seq 1 100); do
  if "$PY" - "127.0.0.1:$PORT" <<'PY' 2>/dev/null
import sys
import urllib.request

with urllib.request.urlopen("http://" + sys.argv[1] + "/health", timeout=0.5) as response:
    assert response.status == 200
PY
  then
    ready="yes"
    break
  fi
  sleep 0.05
done
[ "$ready" = "yes" ] || { cat "$LOG_DIR/sim.log"; fail "控制面 /health 未就绪"; }

"$PY" "$(dirname "$0")/uds_al_telemetry_probe.py" "$SOCK" \
  --http-base "http://127.0.0.1:$PORT"

# 运行期事件日志：看门狗触发（watchdog=1、code=0x001B）与恢复（watchdog=0）各至少一条。
event_lines="$(grep -c '^AL_EVENT ' "$LOG_DIR/sim.log" || true)"
[ "$event_lines" -ge 2 ] || {
  grep '^AL_EVENT ' "$LOG_DIR/sim.log" || true
  fail "AL_EVENT 事件行不足（$event_lines 条，期望 >= 2）"
}
grep '^AL_EVENT ' "$LOG_DIR/sim.log" | grep -q 'code=0x001B.*watchdog=true' || {
  grep '^AL_EVENT ' "$LOG_DIR/sim.log" || true
  fail "缺少看门狗触发事件行（code=0x001B + watchdog=true）"
}
grep '^AL_EVENT ' "$LOG_DIR/sim.log" | grep -q 'watchdog=false' || {
  grep '^AL_EVENT ' "$LOG_DIR/sim.log" || true
  fail "缺少恢复（Error ACK/重新 OP）事件行（watchdog=false）"
}

echo "OK: AL_EVENT 运行期事件 ${event_lines} 条（含 watchdog 触发与恢复）"
echo "OK: /state 与 /metrics 遥测字段完整（SII 计数、AL 迁移、看门狗、last_al_event）"
echo "OK: SII 并发读 + 看门狗 0x001B + Error ACK/恢复通过"
