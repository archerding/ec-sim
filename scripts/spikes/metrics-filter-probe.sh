#!/usr/bin/env bash
# B30 预回归：`/metrics?slave=N` 按从站筛选：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port> --slaves 3`；
#   - `/metrics`：每个计数器 3 行（slave="0"/"1"/"2"）；
#   - `/metrics?slave=1`：每个计数器恰好 1 行且只含 slave="1"；
#   - `/metrics?slave=99`、`/metrics?slave=abc` → 404。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./metrics-filter-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18103}"
SOCK="${SOCK:-/tmp/ecsim-metrics-filter.sock}"
LOG=/tmp/ec-sim-metrics-filter.log

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}

EC_PID=""
cleanup() {
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
  rm -f "$SOCK" "$SOCK".client* 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --slaves 3 >"$LOG" 2>&1 &
EC_PID=$!
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || { cat "$LOG"; echo "FAIL: UDS socket 未就绪"; exit 1; }
for _ in $(seq 1 100); do
  "$PY" - "127.0.0.1:$PORT" <<'PY' >/dev/null 2>&1 && break
import socket
import sys
host, port = sys.argv[1].rsplit(":", 1)
sock = socket.create_connection((host, int(port)), timeout=0.2)
sock.close()
PY
  sleep 0.05
done

"$PY" - "127.0.0.1:$PORT" <<'PY'
import re
import sys
import urllib.error
import urllib.request

http = "http://" + sys.argv[1]


def metrics(path):
    with urllib.request.urlopen(http + path, timeout=2) as response:
        assert response.status == 200
        return response.read().decode("utf-8")


def count_frames_lines(body):
    return len(re.findall(r"^ecsim_frames_in_total\{", body, re.MULTILINE))


def status_of(path):
    try:
        metrics(path)
        return 200
    except urllib.error.HTTPError as error:
        return error.code


all_slaves = metrics("/metrics")
assert count_frames_lines(all_slaves) == 3, count_frames_lines(all_slaves)
assert 'slave="0"' in all_slaves and 'slave="1"' in all_slaves and 'slave="2"' in all_slaves

one = metrics("/metrics?slave=1")
assert count_frames_lines(one) == 1, count_frames_lines(one)
assert 'slave="1"' in one and 'slave="0"' not in one and 'slave="2"' not in one
assert one.count("# TYPE ecsim_frames_in_total counter") == 1

assert status_of("/metrics?slave=99") == 404
assert status_of("/metrics?slave=abc") == 404
print("OK: /metrics?slave=1 仅导出 1 站；全量 3 站；越界/非法 slave → 404")
PY

echo "OK: /metrics 按从站筛选预回归通过"
