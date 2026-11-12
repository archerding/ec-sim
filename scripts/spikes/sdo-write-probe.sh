#!/usr/bin/env bash
# B40 预回归：`POST /sdo` 受控写（默认关闭）：
#   - 默认（无 --serve-write）：POST /sdo → 405，GET /sdo 值不变；
#   - --serve-write：写 0x6040:0 = 0x000F → 200（written_hex=0f00），GET /sdo 读到 0f00，
#     `/sdo-audit` 有 1 条记录，`/metrics` 的 ecsim_sdo_writes_total = 1；
#   - 安全排除对象 0x1018:1 → 403；未知 0x9999 → 404；长度不匹配（1 字节 vs 2 字节）→ 400；
#   - 静态模式（无 --uds）POST /sdo → 405（静态模式仅 GET）。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./sdo-write-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18108}"
SOCK="${SOCK:-/tmp/ecsim-sdo-write.sock}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-cst-servo/plugin.yaml}"
LOG=/tmp/ec-sim-sdo-write.log

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

# 1) 静态模式：POST 应 405。
cleanup
"$BIN" --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" >"$LOG" 2>&1 &
EC_PID=$!
wait_http "127.0.0.1:$PORT"
"$PY" - "127.0.0.1:$PORT" <<'PY'
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]
request = urllib.request.Request(
    base + "/sdo?index=0x6040&sub=0", data=b"\x0f\x00", method="POST"
)
try:
    urllib.request.urlopen(request, timeout=3)
    raise AssertionError("静态模式 POST 应 405")
except urllib.error.HTTPError as error:
    assert error.code == 405, error.code
print("OK: 静态模式 POST /sdo → 405")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 2) 实时模式（未开写开关）：POST 应 405。
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
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]


def post(body):
    request = urllib.request.Request(
        base + "/sdo?index=0x6040&sub=0", data=body, method="POST"
    )
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            return response.status, json.loads(response.read().decode())
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode()


status, _ = post(b"\x0f\x00")
assert status == 405, status
with urllib.request.urlopen(base + "/sdo?index=0x6040&sub=0", timeout=3) as response:
    data = json.loads(response.read().decode())
assert data["value_hex"] == "0000", data
print("OK: 未开 --serve-write 时 POST /sdo → 405，值未变")
PY
kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true

# 3) 实时模式 + --serve-write：写成功 + 审计 + 指标 + 拒绝矩阵。
PORT=$((PORT + 1))
cleanup
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" --serve-write >"$LOG" 2>&1 &
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
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]


def post(query, body):
    request = urllib.request.Request(base + "/sdo" + query, data=body, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            return response.status, json.loads(response.read().decode())
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode()


def get_json(path):
    with urllib.request.urlopen(base + path, timeout=3) as response:
        return json.loads(response.read().decode())


status, payload = post("?index=0x6040&sub=0", b"\x0f\x00")
assert status == 200 and payload["ok"] is True, payload
assert payload["written_hex"] == "0f00", payload
assert payload["old_hex"] == "0000", payload
assert get_json("/sdo?index=0x6040&sub=0")["value_hex"] == "0f00"

audit = get_json("/sdo-audit")
assert audit["schema"] == "ec-sim/sdo-audit-v1" and audit["count"] >= 1, audit
assert audit["events"][-1]["index"] == "0x6040", audit["events"][-1]

with urllib.request.urlopen(base + "/metrics", timeout=3) as response:
    metrics = response.read().decode()
assert "ecsim_sdo_writes_total 1" in metrics, metrics

assert post("?index=0x1018&sub=1", b"\x01\x02\x03\x04")[0] == 403
assert post("?index=0x9999&sub=0", b"\x00")[0] == 404
assert post("?index=0x6040&sub=0", b"\x00")[0] == 400
print("OK: --serve-write 写入生效（值/审计/指标/403/404/400）")
PY

echo "OK: SDO 受控写预回归通过"
