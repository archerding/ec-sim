#!/usr/bin/env bash
# B33 预回归：`--serve --web-root <dir>` 同源托管 web/ 静态文件：
#   - 静态模式（无 --uds/--tap）：`GET /` → text/html（含 EC-Sim），`GET /app.js` → JS
#     （含 renderTopologyGraph），`GET /state` 仍为 JSON，未知路径 404；
#   - 实时模式（--uds）：同样托管 web/，且 `/state` 含 live 字段（API 不受影响）；
#   - `--web-root` 指向不存在目录时：`GET /` 404，但 `/state` 仍 200。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./serve-web-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18104}"
SOCK="${SOCK:-/tmp/ecsim-serve-web.sock}"
LOG=/tmp/ec-sim-serve-web.log

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

# 1) 静态模式：web/ 同源 + API。
cleanup
"$BIN" --serve "127.0.0.1:$PORT" --web-root web >"$LOG" 2>&1 &
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
        return response.status, response.headers.get("Content-Type", ""), response.read().decode("utf-8")

status, ctype, body = get("/")
assert status == 200 and ctype.startswith("text/html"), (status, ctype)
assert "EC-Sim" in body, body[:80]

status, ctype, body = get("/app.js")
assert status == 200 and "javascript" in ctype, (status, ctype)
assert "renderTopologyGraph" in body

status, ctype, body = get("/state")
assert status == 200 and ctype.startswith("application/json"), (status, ctype)
assert json.loads(body)["schema"] == "ec-sim/state-v1"

try:
    get("/nope")
    raise AssertionError("未知路径应 404")
except urllib.error.HTTPError as error:
    assert error.code == 404
print("OK: 静态模式 GET / 与 /app.js 返回 web 文件，/state 仍为 JSON，未知路径 404")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 2) 实时模式：同源 web + live API。
PORT=$((PORT + 1))
cleanup
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --web-root web >"$LOG" 2>&1 &
EC_PID=$!
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.request

base = "http://" + sys.argv[1]

with urllib.request.urlopen(base + "/", timeout=3) as response:
    assert response.status == 200 and "EC-Sim" in response.read().decode("utf-8")
with urllib.request.urlopen(base + "/state", timeout=3) as response:
    state = json.loads(response.read().decode("utf-8"))
assert state["schema"] == "ec-sim/state-v1" and "live" in state
print("OK: 实时模式托管 / 且 /state 仍含 live")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 3) 不存在的 web-root：静态 404，API 仍 200。
PORT=$((PORT + 1))
cleanup
"$BIN" --serve "127.0.0.1:$PORT" --web-root /nonexistent-ecsim-web >"$LOG" 2>&1 &
EC_PID=$!
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]

with urllib.request.urlopen(base + "/", timeout=3) as response:
    ctype = response.headers.get("Content-Type", "")
    payload = json.loads(response.read().decode("utf-8"))
assert ctype.startswith("application/json"), ctype
assert payload.get("service") == "ec-sim-control-plane", payload

with urllib.request.urlopen(base + "/state", timeout=3) as response:
    assert json.loads(response.read().decode("utf-8"))["schema"] == "ec-sim/state-v1"
print("OK: web-root 不存在时 GET / 回退为 API 自述 JSON，/state 仍 200")
PY

echo "OK: --serve --web-root 同源托管预回归通过"
