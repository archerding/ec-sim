#!/usr/bin/env bash
# B22 预回归：`--serve` 实时状态（与 UDS 同进程）：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port>`；
#   - GET /state 初始 `live.al_status = 0x01`（INIT）、`live` 字段存在；
#   - 通过 UDS 合成主站发 APWR AL Control 0x02（PREOP），轮询 /state，直到
#     `live.al_status` 低 4 位 = 0x04（PREOP）且 `live.stats.frames_in >= 1`。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./live-state-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18095}"
SOCK="${SOCK:-/tmp/ecsim-live.sock}"
LOG=/tmp/ec-sim-live.log

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
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" >"$LOG" 2>&1 &
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

socket_path, http_addr = sys.argv[1], "http://" + sys.argv[2]


def state():
    with urllib.request.urlopen(http_addr + "/state", timeout=2) as response:
        assert response.status == 200
        return json.loads(response.read().decode("utf-8"))


data = state()
assert data["schema"] == "ec-sim/state-v1"
assert "live" in data, "缺失 live 字段（是否未进入实时模式？）"
assert data["live"]["al_status"] == "0x01", data["live"]
assert data["live"]["stats"]["frames_in"] == 0

master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    reply = master.transact([(probe.CMD["APWR"], 0, 0x0120, b"\x02")])
    probe.check(reply[0][4] == 1, f"APWR AL Control WKC {reply[0][4]} != 1")

    deadline = time.time() + 3.0
    status = 0
    while time.time() < deadline:
        data = state()
        status = int(data["live"]["al_status"], 16)
        if status & 0x0F == 0x02:
            break
        time.sleep(0.05)
    assert status & 0x0F == 0x02, f"AL 未到 PREOP：{data['live']}"
    assert data["live"]["stats"]["frames_in"] >= 1, data["live"]["stats"]
    print(
        "OK: 实时 /state 反映 AL=0x%02x（PREOP）、frames_in=%d"
        % (status, data["live"]["stats"]["frames_in"])
    )
finally:
    master.close()
PY

echo "OK: --serve 实时状态预回归通过（UDS + HTTP 同进程）"
