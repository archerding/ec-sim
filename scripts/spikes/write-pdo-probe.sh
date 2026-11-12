#!/usr/bin/env bash
# B35 预回归：`POST /pdo` 写操作（默认关闭，需 `--serve-write`）：
#   - 默认启动（无 --serve-write）：`POST /pdo` → 405，且 `GET /pdo` 仍为 0；
#   - `--serve-write`：`POST /pdo` 写 0xBEEF → 200 JSON（written_hex=beef），`GET /pdo` 读到 beef；
#   - `?slave=1`（--slaves 2）只写第 2 站，`/pdo?slave=0` 仍为 0；
#   - `POST /state` → 404（只支持 POST /pdo）。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./write-pdo-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18106}"
SOCK="${SOCK:-/tmp/ecsim-write-pdo.sock}"
LOG=/tmp/ec-sim-write-pdo.log

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

start_server() {
  local mode="$1" slaves="$2"
  cleanup
  if [ "$mode" = "write" ]; then
    "$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --slaves "$slaves" --serve-write >"$LOG" 2>&1 &
  else
    "$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --slaves "$slaves" >"$LOG" 2>&1 &
  fi
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
}

start_server read 1
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]


def post(path, body):
    request = urllib.request.Request(base + path, data=body, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            return response.status, json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8")


def pdo(path=""):
    with urllib.request.urlopen(base + "/pdo" + path, timeout=3) as response:
        return json.loads(response.read().decode("utf-8"))


status, payload = post("/pdo", b"\xbe\xef")
assert status == 405, (status, payload)
assert pdo()["sm2"]["hex"] == "0000"
print("OK: 默认（无 --serve-write）POST /pdo → 405，PDO 未改变")
PY
cleanup

start_server write 2
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]


def post(path, body):
    request = urllib.request.Request(base + path, data=body, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            return response.status, json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8")


def pdo(path=""):
    with urllib.request.urlopen(base + "/pdo" + path, timeout=3) as response:
        return json.loads(response.read().decode("utf-8"))


status, payload = post("/pdo", b"\xbe\xef")
assert status == 200, (status, payload)
assert payload["ok"] is True and payload["written_hex"] == "beef", payload
assert pdo()["sm2"]["hex"] == "beef", pdo()["sm2"]

status, payload = post("/pdo?slave=1", b"\x12\x34")
assert status == 200 and payload["slave"] == 1, payload
assert pdo("?slave=1")["sm2"]["hex"] == "1234", pdo("?slave=1")["sm2"]
assert pdo("?slave=0")["sm2"]["hex"] == "beef", pdo("?slave=0")["sm2"]

status, payload = post("/state", b"x")
assert status == 404, (status, payload)
print("OK: --serve-write 下 POST /pdo 写入生效（slave 0=beef、slave 1=1234），POST /state 404")
PY

echo "OK: POST /pdo 写操作预回归通过"
