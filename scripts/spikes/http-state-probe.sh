#!/usr/bin/env bash
# B21 预回归：`ec-sim-esc --serve` 只读 HTTP JSON API（std TcpListener）：
#   - 有 manifest：GET /state → 200 state-v1（CORS、Content-Type）；GET /dict → 200 dict-v1；
#   - GET /health → 200；未知路径 → 404（带 routes 列表）；
#   - 无 manifest：GET /dict → 404（dict 不可用），/state 的 profile 为 null。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./http-state-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18089}"
LOG=/tmp/ec-sim-http-probe.log

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}

cleanup() {
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
  rm -f /tmp/ec-sim-http-*.log
}
EC_PID=""
trap cleanup EXIT

run_server() {
  local addr="$1"
  shift
  "$BIN" --serve "$addr" "$@" >/tmp/ec-sim-http-$PORT.log 2>&1 &
  EC_PID=$!
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
  cat /tmp/ec-sim-http-$PORT.log || true
  echo "FAIL: --serve $addr 未就绪"
  exit 1
}

check_with_manifest() {
  run_server "127.0.0.1:$PORT" --manifest plugins/generic-cia402-cst-servo/plugin.yaml
  "$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]

def get(path):
    with urllib.request.urlopen(base + path, timeout=5) as response:
        return response.status, dict(response.headers), response.read().decode("utf-8")

status, headers, body = get("/state")
state = json.loads(body)
assert status == 200 and state["schema"] == "ec-sim/state-v1"
assert headers.get("Content-Type", "").startswith("application/json")
assert headers.get("Access-Control-Allow-Origin") == "*"
assert state["profile"] and state["profile"]["id"].endswith("cia402-cst-servo")
assert headers.get("Cache-Control") == "no-store"

status, _, body = get("/dict?x=1")
dict_data = json.loads(body)
assert status == 200 and dict_data["schema"] == "ec-sim/dict-v1"
assert dict_data["object_count"] > 0

status, _, body = get("/health")
assert status == 200 and "ec-sim-control-plane" in body

try:
    get("/nope")
    raise AssertionError("未知路径应返回 404")
except urllib.error.HTTPError as error:
    body = error.read().decode("utf-8")
    assert error.code == 404 and "/state" in body
print("OK: HTTP API（manifest 画像）：/state、/dict、/health 200，未知路径 404，CORS/缓存头正确")
PY
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
  EC_PID=""
}

check_without_manifest() {
  PORT=$((PORT + 1))
  run_server "127.0.0.1:$PORT"
  "$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]

def get(path):
    with urllib.request.urlopen(base + path, timeout=5) as response:
        return response.status, response.read().decode("utf-8")

status, body = get("/state")
state = json.loads(body)
assert status == 200 and state["profile"] is None

try:
    get("/dict")
    raise AssertionError("无 manifest 时 /dict 应返回 404")
except urllib.error.HTTPError as error:
    assert error.code == 404
print("OK: HTTP API（内置画像）：/state 200（profile=null）、/dict 404")
PY
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
  EC_PID=""
}

check_with_manifest
check_without_manifest
echo "OK: --serve 只读 HTTP API 预回归通过"
