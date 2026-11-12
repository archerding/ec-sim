#!/usr/bin/env bash
# B41 预回归：`--serve-token-env VAR` 可选静态 Bearer token（默认关闭 = 现状）。
#   - 无该参数 / 变量未设置 / 变量为空 → 维持现状（API 200，不鉴权）；
#   - 变量非空时，API 路由（/state,/dict,/pdo,/sdo,/sdo-audit,/topology,/metrics,/events）
#     需要 `Authorization: Bearer <token>`：
#       缺头/非 Bearer → 401（带 `WWW-Authenticate: Bearer`），token 错误 → 403，正确 → 200；
#   - 静态资源 `/`、`/app.js` 与 `/health` 不鉴权（浏览器可先加载外壳）；
#   - 实时模式（Linux/UDS）：POST /sdo、/sdo-audit、/metrics 同规则，鉴权通过后才做受控写；
#   - `--serve-token-env` 未与 `--serve` 同用 → 退出码 2。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./serve-token-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18120}"
TOKEN="${TOKEN:-ec-sim-probe-secret}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-cst-servo/plugin.yaml}"
SOCK="${SOCK:-/tmp/ecsim-serve-token.sock}"
LOG=/tmp/ec-sim-serve-token.log

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

# 1) 未配置 token：现状不变（/state 200）。
cleanup
"$BIN" --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" --web-root web >"$LOG" 2>&1 &
EC_PID=$!
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.request

with urllib.request.urlopen("http://" + sys.argv[1] + "/state", timeout=5) as response:
    assert response.status == 200
    assert json.loads(response.read().decode())["schema"] == "ec-sim/state-v1"
print("OK: 未配置 token 时 /state → 200（现状不变）")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 2) 变量未设置 / 为空：仍不鉴权，且启动日志明确提示。
for mode in unset empty; do
  PORT=$((PORT + 1))
  cleanup
  if [ "$mode" = "unset" ]; then
    unset EC_SIM_TOKEN_TEST
    "$BIN" --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" \
      --serve-token-env EC_SIM_TOKEN_TEST >"$LOG" 2>&1 &
  else
    EC_SIM_TOKEN_TEST="" "$BIN" --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" \
      --serve-token-env EC_SIM_TOKEN_TEST >"$LOG" 2>&1 &
  fi
  EC_PID=$!
  wait_http "127.0.0.1:$PORT"
  grep -q "不鉴权" "$LOG" || {
    cat "$LOG"
    echo "FAIL: $mode 模式的启动日志未提示不鉴权"
    exit 1
  }
  "$PY" - "127.0.0.1:$PORT" <<'PY'
import json
import sys
import urllib.request

with urllib.request.urlopen("http://" + sys.argv[1] + "/state", timeout=5) as response:
    assert response.status == 200
    assert json.loads(response.read().decode())["schema"] == "ec-sim/state-v1"
print("OK: token 变量为空/未设置时 /state → 200（现状不变）")
PY
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
done

# 3) token 非空：静态模式鉴权矩阵（401/403/200 + 静态资源豁免）。
PORT=$((PORT + 1))
cleanup
EC_SIM_TOKEN_TEST="$TOKEN" "$BIN" --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" \
  --web-root web --serve-token-env EC_SIM_TOKEN_TEST >"$LOG" 2>&1 &
EC_PID=$!
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" "$TOKEN" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]
token = sys.argv[2]


def request(path, auth=None, scheme="Bearer", method="GET", body=None):
    headers = {}
    if auth is not None:
        headers["Authorization"] = "%s %s" % (scheme, auth)
    req = urllib.request.Request(base + path, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=5) as response:
            return response.status, dict(response.headers), response.read().decode()
    except urllib.error.HTTPError as error:
        return error.code, dict(error.headers), error.read().decode()


status, headers, _ = request("/state")
assert status == 401, status
assert headers.get("WWW-Authenticate", "").lower().startswith("bearer"), headers
print("OK: 缺 Authorization → 401 + WWW-Authenticate: Bearer")

status, _, _ = request("/state?slave=0")
assert status == 401, status
print("OK: 带 query 的受保护路由同样 401")

status, _, body = request("/state", auth="wrong-token")
assert status == 403, (status, body)
print("OK: Bearer token 错误 → 403")

status, _, _ = request("/state", auth=token, scheme="Basic")
assert status == 401, status
print("OK: 非 Bearer scheme → 401")

status, _, body = request("/state", auth=token)
assert status == 200, (status, body)
assert json.loads(body)["schema"] == "ec-sim/state-v1", body

status, _, body = request("/dict", auth=token)
assert status == 200, (status, body)
assert json.loads(body)["schema"] == "ec-sim/dict-v1", body

status, _, body = request("/sdo?index=0x6040&sub=0", auth=token)
assert status == 200, (status, body)
assert json.loads(body)["schema"] == "ec-sim/sdo-v1", body
print("OK: 正确 token → /state、/dict、/sdo 均 200")

for path in ("/", "/app.js", "/health"):
    status, _, _ = request(path)
    assert status == 200, (path, status)
print("OK: 静态资源 / 与 /app.js、/health 不鉴权（浏览器可先加载外壳）")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 4) 实时模式（Linux/UDS）：写路径/审计/指标同样受 token 保护。
if [ "$(uname -s 2>/dev/null || echo unknown)" != "Linux" ]; then
  echo "SKIP: 实时模式 token 矩阵（需要 Linux/UDS）"
  # 5) 参数约束（跨平台）。
  set +e
  "$BIN" --serve-token-env EC_SIM_TOKEN_TEST >/dev/null 2>&1
  code=$?
  set -e
  [ "$code" = "2" ] || {
    echo "FAIL: --serve-token-env 未与 --serve 同用时应退出码 2（实际 $code）"
    exit 1
  }
  echo "OK: --serve-token-env 缺少 --serve → 退出码 2"
  echo "OK: serve-token 静态鉴权矩阵通过（实时模式跳过：需要 Linux/UDS）"
  exit 0
fi

PORT=$((PORT + 1))
cleanup
EC_SIM_TOKEN_TEST="$TOKEN" "$BIN" --uds "$SOCK" --esm-lax \
  --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" --serve-write \
  --serve-token-env EC_SIM_TOKEN_TEST >"$LOG" 2>&1 &
EC_PID=$!
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  echo "FAIL: UDS socket 未就绪"
  exit 1
}
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" "$TOKEN" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]
token = sys.argv[2]


def request(path, auth=None, scheme="Bearer", method="GET", body=None):
    headers = {}
    if auth is not None:
        headers["Authorization"] = "%s %s" % (scheme, auth)
    req = urllib.request.Request(base + path, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=5) as response:
            return response.status, response.read().decode()
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode()


def get_json(path, auth=token):
    status, body = request(path, auth=auth)
    assert status == 200, (path, status, body)
    return json.loads(body)


# 写路径：无 token 必须 401，且不产生写入。
status, body = request("/sdo?index=0x6040&sub=0", method="POST", body=b"\x0f\x00")
assert status == 401, (status, body)
assert get_json("/sdo?index=0x6040&sub=0")["value_hex"] == "0000"

status, body = request(
    "/sdo?index=0x6040&sub=0", auth=token, method="POST", body=b"\x0f\x00"
)
assert status == 200, (status, body)
assert json.loads(body)["written_hex"] == "0f00", body
assert get_json("/sdo?index=0x6040&sub=0")["value_hex"] == "0f00"

# 审计/指标：无 token 401，带 token 200 且计数可见。
assert request("/sdo-audit")[0] == 401
assert get_json("/sdo-audit")["count"] >= 1
assert request("/metrics")[0] == 401
status, metrics = request("/metrics", auth=token)
assert status == 200 and "ecsim_sdo_writes_total 1" in metrics, (status, metrics)

# SSE：无 token 401（带 token 的流式成功不在本探针覆盖，避免挂起）。
assert request("/events")[0] == 401
assert get_json("/state")["live"] is not None
print("OK: 实时模式 token 保护 POST /sdo、/sdo-audit、/metrics、/events，鉴权后写生效")
PY

# 5) 参数约束。
set +e
"$BIN" --serve-token-env EC_SIM_TOKEN_TEST >/dev/null 2>&1
code=$?
set -e
[ "$code" = "2" ] || {
  echo "FAIL: --serve-token-env 未与 --serve 同用时应退出码 2（实际 $code）"
  exit 1
}
echo "OK: --serve-token-env 缺少 --serve → 退出码 2"
echo "OK: serve-token 预回归通过（静态 + 实时）"
