#!/usr/bin/env bash
# B34 预回归：`GET /events`（SSE 推送 `/state` 快照）：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port> --slaves 2`；
#   - `/events?interval_ms=100`：Content-Type=text/event-stream，收到至少两条 `data: {...}`
#     事件，JSON 为 state-v1 且含 live；
#   - 两条事件之间合成主站发帧，后一条 `live.stats.frames_in` 应增大；
#   - 客户端断开后 `/state` 仍 200（SSE 连接不阻塞其他请求）。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./events-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18105}"
SOCK="${SOCK:-/tmp/ecsim-events.sock}"
LOG=/tmp/ec-sim-events.log

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
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --slaves 2 >"$LOG" 2>&1 &
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

"$PY" - "$SOCK" "127.0.0.1:$PORT" <<'PY'
import json
import sys
import time
import urllib.request

sys.path.insert(0, "scripts/spikes")
import uds_master_probe as probe

socket_path, http = sys.argv[1], "http://" + sys.argv[2]


def read_event(response):
    """读取下一条 `data: {...}` 事件（跳过注释/空行），返回解析后的 JSON。"""
    deadline = time.time() + 5.0
    while time.time() < deadline:
        line = response.readline().decode("utf-8").strip()
        if line.startswith("data: "):
            return json.loads(line[len("data: ") :])
    raise AssertionError("未收到 SSE data 事件")


with urllib.request.urlopen(http + "/events?interval_ms=100", timeout=5) as response:
    content_type = response.headers.get("Content-Type", "")
    assert content_type.startswith("text/event-stream"), content_type
    first = read_event(response)
    assert first["schema"] == "ec-sim/state-v1", first.get("schema")
    assert "live" in first
    before = first["live"]["stats"]["frames_in"]

    master = probe.SyntheticMaster(socket_path, timeout=2.0)
    try:
        for _ in range(2):
            master.transact([(probe.CMD["BRD"], 0, 0x0000, b"\x00\x00")])
    finally:
        master.close()

    latest = first
    deadline = time.time() + 5.0
    while time.time() < deadline:
        latest = read_event(response)
        if latest["live"]["stats"]["frames_in"] >= before + 2:
            break
    assert latest["live"]["stats"]["frames_in"] >= before + 2, (
        before,
        latest["live"]["stats"],
    )

# SSE 断开后其他 API 仍可用（accept 循环未被阻塞）。
with urllib.request.urlopen(http + "/state", timeout=3) as response:
    assert response.status == 200
    assert json.loads(response.read().decode("utf-8"))["schema"] == "ec-sim/state-v1"

print(
    "OK: /events SSE 推送 state-v1（frames_in %d -> %d），断开后 /state 仍 200"
    % (before, latest["live"]["stats"]["frames_in"])
)
PY

echo "OK: SSE /events 预回归通过"
