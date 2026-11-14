#!/usr/bin/env bash
# B37：一键预回归——按平台/权限挑选可跑的探针，逐个执行并汇总 PASS/FAIL/SKIP。
#
# 用法：
#   ./run-all-probes.sh                 # 自动：非 Linux 只跑跨平台项；root 时含 TAP 项
#   ./run-all-probes.sh --list          # 只列出探针与跳过原因
#   BIN=target/release/ec-sim-esc PY=python3 ./run-all-probes.sh
#   RUN_TAP=0 ./run-all-probes.sh       # 即使 root 也不跑 TAP（如无 SOEM 夹具）
#
# 前置：cargo build --release -p esc-node（UDS 探针）；python3；root 仅 TAP 项需要。
# 退出码：0 = 无失败（允许 skip）；1 = 至少一个 FAIL。
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 1

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-120}"
RUN_TAP="${RUN_TAP:-auto}"
LOG_DIR="${LOG_DIR:-/tmp/ec-sim-probes}"

os="$(uname -s 2>/dev/null || echo unknown)"
case "$os" in
  Linux*) host_kind="linux" ;;
  *) host_kind="other" ;;
esac
is_root="no"
if [ "$(id -u 2>/dev/null || echo 1)" = "0" ]; then is_root="yes"; fi

# Windows Git Bash 的 `timeout` 是 Windows 自带命令；仅在有 GNU timeout 时启用超时。
has_timeout="no"
if timeout --version >/dev/null 2>&1; then has_timeout="yes"; fi

run_with_timeout() {
  if [ "$has_timeout" = "yes" ]; then
    timeout "$TIMEOUT_SECONDS" "$@"
  else
    "$@"
  fi
}

# name|kind|script|python?
PROBES="
doc-check|any|control-plane-doc-check.py|yes
mcp-tools|any|mcp-tools-check.py|yes
agent-skills|any|agent-skills-check.py|yes
http-state|any|http-state-probe.sh|no
live-state|linux|live-state-probe.sh|no
pdo-live|linux|pdo-live-probe.sh|no
pdo-history|linux|pdo-history-probe.sh|no
pdo-decode|linux|pdo-decode-probe.sh|no
topology|linux|topology-probe.sh|no
slave-select|linux|slave-select-probe.sh|no
metrics|linux|metrics-probe.sh|no
metrics-filter|linux|metrics-filter-probe.sh|no
serve-web|linux|serve-web-probe.sh|no
web-sdo-panel|any|web-sdo-panel-check.py|yes
events|linux|events-probe.sh|no
write-pdo|linux|write-pdo-probe.sh|no
sdo-read|linux|sdo-read-probe.sh|no
sdo-write|linux|sdo-write-probe.sh|no
serve-token|any|serve-token-probe.sh|no
mcp-stdio|any|mcp-stdio-probe.sh|no
tap-live|root|tap-live-state-probe.sh|no
tap-cia402|root|tap-cia402-multi-slave-probe.sh|no
invalid-slave|any|invalid-slave-probe.sh|no
al-telemetry|linux|al-telemetry-probe.sh|no
"

skip_reason() {
  local kind="$1"
  case "$kind" in
    any) echo "" ;;
    linux)
      [ "$host_kind" = "linux" ] && echo "" || echo "需要 Linux（UDS）"
      ;;
    root)
      if [ "$host_kind" != "linux" ]; then echo "需要 Linux（TAP）"; return; fi
      if [ "$is_root" != "yes" ]; then echo "需要 root（TAP）"; return; fi
      if [ "$RUN_TAP" = "0" ]; then echo "RUN_TAP=0"; return; fi
      echo ""
      ;;
    *) echo "未知类别" ;;
  esac
}

if [ "${1:-}" = "--list" ]; then
  echo "host=$host_kind root=$is_root BIN=$BIN PY=$PY"
  echo "$PROBES" | while IFS='|' read -r name kind script uses_py; do
    [ -z "$name" ] && continue
    reason="$(skip_reason "$kind")"
    if [ -n "$reason" ]; then
      echo "SKIP  $name  ($script) — $reason"
    else
      echo "READY $name  ($script)"
    fi
  done
  exit 0
fi

[ -x "$BIN" ] || [ "$host_kind" != "linux" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}
command -v "$PY" >/dev/null 2>&1 || {
  echo "FAIL: 找不到 Python：$PY"
  exit 1
}
mkdir -p "$LOG_DIR"

total=0
pass=0
fail=0
skip=0
failed_names=""

echo "== EC-Sim 预回归（host=$host_kind root=$is_root BIN=$BIN）=="
while IFS='|' read -r name kind script uses_py; do
  [ -z "$name" ] && continue
  total=$((total + 1))
  reason="$(skip_reason "$kind")"
  if [ -n "$reason" ]; then
    skip=$((skip + 1))
    echo "SKIP  $name — $reason"
    continue
  fi
  log="$LOG_DIR/$name.log"
  if [ "$uses_py" = "yes" ]; then
    if run_with_timeout "$PY" "scripts/spikes/$script" >"$log" 2>&1; then
      pass=$((pass + 1))
      echo "PASS  $name"
    else
      code=$?
      fail=$((fail + 1))
      failed_names="$failed_names $name"
      echo "FAIL  $name (exit=$code, log=$log)"
      tail -5 "$log" | sed 's/^/      /'
    fi
  else
    if BIN="$BIN" PY="$PY" run_with_timeout bash "scripts/spikes/$script" >"$log" 2>&1; then
      pass=$((pass + 1))
      echo "PASS  $name"
    else
      code=$?
      fail=$((fail + 1))
      failed_names="$failed_names $name"
      echo "FAIL  $name (exit=$code, log=$log)"
      tail -5 "$log" | sed 's/^/      /'
    fi
  fi
done <<EOF
$PROBES
EOF

echo
echo "== 汇总：$pass PASS / $fail FAIL / $skip SKIP（共 $total）=="
if [ "$fail" -gt 0 ]; then
  echo "FAILED:$failed_names"
  echo "RESULT: FAIL"
  exit 1
fi
echo "RESULT: PASS"
