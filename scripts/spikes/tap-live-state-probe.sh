#!/usr/bin/env bash
# B24 预回归：**TAP 实时 `--serve`**——TAP 线缆层与 HTTP 线程共享 `Arc<Mutex<Responder>>`：
#   - 启动 `ec-sim-esc --tap <tap> --esm-lax --serve 127.0.0.1:<port>`；
#   - GET /state 初始 `live.al_status = 0x01`（INIT）；
#   - 用未修改 SOEM 的 `simple_ng <tap>` 跑若干轮（会配置 SM/PDO 并进入 OP）；
#   - 轮询 /state：`live.al_status` 低 4 位 = 0x08（OP）、`live.stats.frames_in > 0`；
#   - GET /pdo 返回 pdo-v1 且 SM2/SM3 有地址。
#
# 前置：root（TAP + AF_PACKET）、`cargo build --release -p esc-node`、验证机本地 SOEM TAP 构建
#       （默认 /opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng，可用 SOEM_SIMPLE_NG 覆盖）。
# 用法：sudo ./tap-live-state-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
PORT="${PORT:-18097}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
LOG=/tmp/ec-sim-tap-live.log

fail() {
  echo "FAIL: $1"
  exit 1
}
[ "$(id -u)" -eq 0 ] || fail "需要 root（TAP + AF_PACKET）"
[ -x "$BIN" ] || fail "找不到 $BIN"
[ -x "$SOEM_SIMPLE_NG" ] || {
  echo "SKIP: 缺少验证机本地 SOEM TAP simple_ng（$SOEM_SIMPLE_NG）"
  exit 2
}

EC_PID=""
cleanup() {
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
ip tuntap add dev "$TAP" mode tap
ip link set "$TAP" address "$MAC"
"$BIN" --tap "$TAP" --esm-lax --serve "127.0.0.1:$PORT" >"$LOG" 2>&1 &
EC_PID=$!
sleep 0.8
ip link set "$TAP" up
sleep 0.3

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

# 初始状态断言 + 简单 SOEM 流程 + 轮询。
"$PY" - "127.0.0.1:$PORT" "$SOEM_SIMPLE_NG" "$TAP" <<'PY'
import json
import subprocess
import sys
import time
import urllib.request

http_addr = "http://" + sys.argv[1]
simple_ng, tap = sys.argv[2], sys.argv[3]


def get(path):
    with urllib.request.urlopen(http_addr + path, timeout=2) as response:
        return response.status, json.loads(response.read().decode("utf-8"))


status, state = get("/state")
assert status == 200 and state["schema"] == "ec-sim/state-v1"
assert state["live"]["al_status"] == "0x01", state["live"]

# simple_ng 会运行一段时间（timeout 杀掉）；期间轮询 /state 直到 OP。
process = subprocess.Popen(
    ["timeout", "6", "stdbuf", "-oL", simple_ng, tap],
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
try:
    deadline = time.time() + 8.0
    op = False
    while time.time() < deadline:
        _, state = get("/state")
        status = int(state["live"]["al_status"], 16)
        if status & 0x0F == 0x08:
            op = True
            break
        time.sleep(0.1)
    assert op, f"TAP 下未到 OP：{state['live']}"
    assert state["live"]["stats"]["frames_in"] > 0, state["live"]["stats"]
    status, pdo = get("/pdo")
    assert status == 200 and pdo["schema"] == "ec-sim/pdo-v1"
    assert pdo["sm2"]["address"].startswith("0x") and pdo["sm3"]["address"].startswith("0x")
    print(
        "OK: TAP 实时 /state AL=0x%02x（OP）、frames_in=%d；/pdo schema=%s"
        % (status, state["live"]["stats"]["frames_in"], pdo["schema"])
    )
finally:
    process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
PY

echo "OK: TAP 实时 --serve 预回归通过"
