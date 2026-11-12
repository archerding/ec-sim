#!/usr/bin/env bash
# B12 预回归：ESC-over-UDS 上的**多从站链**——1 个 `ec-sim-esc --slaves N` 进程模拟 N 个 ESC，
# 合成主站验证：广播 WKC=N、逐站分配站地址、配置地址寻址只命中单站、分配后广播仍为 N。
#
# 前置（Linux 验证机上）：
#   - `cargo build --release -p esc-node`
#   - python3（标准库即可）；无需 root
#
# 用法：./uds-multi-slave-probe.sh
set -euo pipefail

SOCK="${SOCK:-/tmp/ecsim-multi.sock}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
N="${N:-8}"
LOG=/tmp/ec-sim-esc-multi.log

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" "$SOCK".client* 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --slaves "$N" >"$LOG" 2>&1 &
ec_pid=$!
for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
if [ ! -S "$SOCK" ]; then
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  cat "$LOG"
  exit 1
fi

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --multi-slave="$N"

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
echo "OK: 多从站链预回归通过（$N 站，UDS shim）"
