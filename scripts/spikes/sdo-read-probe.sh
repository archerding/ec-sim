#!/usr/bin/env bash
# B39 预回归：`GET /sdo?index&sub` 从运行时对象字典只读浏览（不经过 mailbox）：
#   - 静态模式（`--serve --manifest generic-cia402-cst-servo`）：
#       `?index=0x6040&sub=0` → 200 sdo-v1（Controlword/0x0006/16/rw）；
#       `?index=0x6060`（默认 sub=0）→ 200；缺 index / index 非法 / 未知对象 / 未知子索引 → 404；
#   - 实时模式（--uds）：同一对象仍返回 200 sdo-v1。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./sdo-read-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18107}"
SOCK="${SOCK:-/tmp/ecsim-sdo-read.sock}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-cst-servo/plugin.yaml}"
LOG=/tmp/ec-sim-sdo-read.log

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

wait_http() {
  local addr="$1"
  for _ in $(seq 1 100); do
    "$PY" - "$addr" <<'PY' >/dev/null 2>&1 && return 0
import socket
import sys
host, port = sys.argv[1].rsplit(":", 1)
sock = socket.create_connection((host, int(port)), timeout=0.2)
sock.close()
PY
    sleep 0.05
  done
  cat "$LOG" || true
  echo "FAIL: HTTP $addr 未就绪"
  exit 1
}

# 1) 静态模式。
cleanup
"$BIN" --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" >"$LOG" 2>&1 &
EC_PID=$!
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]


def get(path):
    with urllib.request.urlopen(base + path, timeout=3) as response:
        return response.status, json.loads(response.read().decode("utf-8"))


def status_of(path):
    try:
        get(path)
        return 200
    except urllib.error.HTTPError as error:
        return error.code


status, data = get("/sdo?index=0x6040&sub=0")
assert status == 200 and data["schema"] == "ec-sim/sdo-v1", data
assert data["index"] == "0x6040" and data["sub"] == 0
assert data["name"] == "Controlword", data["name"]
assert data["type"] == "0x0006" and data["bits"] == 16 and data["access"] == "rw", data

status, data = get("/sdo?index=0x6060")
assert status == 200 and data["name"] == "Modes of operation", data

assert status_of("/sdo") == 404
assert status_of("/sdo?index=abc") == 404
assert status_of("/sdo?index=0x9999") == 404
assert status_of("/sdo?index=0x6040&sub=9") == 404
print("OK: 静态模式 /sdo 已知对象 200、未知/非法 404")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 2) 实时模式。
PORT=$((PORT + 1))
cleanup
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" >"$LOG" 2>&1 &
EC_PID=$!
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || { cat "$LOG"; echo "FAIL: UDS socket 未就绪"; exit 1; }
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.request

base = "http://" + sys.argv[1]
with urllib.request.urlopen(base + "/sdo?index=0x6041&sub=0", timeout=3) as response:
    data = json.loads(response.read().decode("utf-8"))
assert data["schema"] == "ec-sim/sdo-v1" and data["index"] == "0x6041", data
print("OK: 实时模式 /sdo 返回 sdo-v1（0x6041）")
PY

echo "OK: SDO 只读浏览预回归通过"
