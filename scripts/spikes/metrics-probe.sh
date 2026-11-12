#!/usr/bin/env bash
# B29 预回归：`/metrics` Prometheus 文本指标：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port> --slaves 2`；
#   - `GET /metrics`：200、`Content-Type: text/plain`、含 `# TYPE ecsim_frames_in_total counter`，
#     每站一行（slave="0"、slave="1"）；
#   - 合成主站发若干净帧后，两站 `ecsim_frames_in_total` 单调增加。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./metrics-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18102}"
SOCK="${SOCK:-/tmp/ecsim-metrics.sock}"
LOG=/tmp/ec-sim-metrics.log

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
import re
import sys
import time
import urllib.request

sys.path.insert(0, "scripts/spikes")
import uds_master_probe as probe

socket_path, http = sys.argv[1], "http://" + sys.argv[2]


def metrics():
    with urllib.request.urlopen(http + "/metrics", timeout=2) as response:
        assert response.status == 200
        content_type = response.headers.get("Content-Type", "")
        body = response.read().decode("utf-8")
    assert content_type.startswith("text/plain"), content_type
    return body


def frames_in(body):
    values = {}
    for line in body.splitlines():
        match = re.match(r'ecsim_frames_in_total\{slave="(\d+)",station_address="(0x[0-9A-F]{4})"\} (\d+)$', line)
        if match:
            values[int(match.group(1))] = int(match.group(3))
    return values


body = metrics()
assert "# TYPE ecsim_frames_in_total counter" in body, body[:200]
assert "# HELP ecsim_frames_in_total" in body
assert "# TYPE ecsim_al_status gauge" in body
before = frames_in(body)
assert set(before) == {0, 1}, before

master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    for _ in range(3):
        master.transact([(probe.CMD["BRD"], 0, 0x0000, b"\x00\x00")])
    deadline = time.time() + 3.0
    after = before
    while time.time() < deadline:
        after = frames_in(metrics())
        if after.get(0, 0) >= before[0] + 3 and after.get(1, 0) >= before[1] + 3:
            break
        time.sleep(0.05)
    assert after[0] >= before[0] + 3, (before, after)
    assert after[1] >= before[1] + 3, (before, after)
    print(
        "OK: /metrics Prometheus 文本两站计数单调（frames_in %s -> %s）"
        % (before, after)
    )
finally:
    master.close()
PY

echo "OK: /metrics 预回归通过"
