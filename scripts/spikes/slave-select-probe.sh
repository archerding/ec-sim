#!/usr/bin/env bash
# B28 预回归：逐从站查看 `/state?slave=N`、`/pdo?slave=N`：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port> --slaves 3`；
#   - 按链上位置配置地址 1/2/3，只写第 2 站（N=1）SM2=0xBEEF；
#   - `/state?slave=1` 的 live.station_address=0x0002；`/pdo?slave=1` sm2=beef；
#     `/pdo?slave=0` sm2=0000（默认 N=0 同旧行为）；
#   - `/state?slave=99`、`/pdo?slave=abc` 返回 404。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./slave-select-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18101}"
SOCK="${SOCK:-/tmp/ecsim-slave-select.sock}"
LOG=/tmp/ec-sim-slave-select.log

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
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --slaves 3 >"$LOG" 2>&1 &
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
import struct
import sys
import time
import urllib.error
import urllib.request

sys.path.insert(0, "scripts/spikes")
import uds_master_probe as probe

socket_path, http = sys.argv[1], "http://" + sys.argv[2]


def get_json(path):
    with urllib.request.urlopen(http + path, timeout=2) as response:
        assert response.status == 200, (path, response.status)
        return json.loads(response.read().decode("utf-8"))


def status_of(path):
    try:
        get_json(path)
        return 200
    except urllib.error.HTTPError as error:
        return error.code


def auto_adp(position):
    return (-position) & 0xFFFF


master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    for position in range(3):
        reply = master.transact(
            [(probe.CMD["APWR"], auto_adp(position), 0x0010, struct.pack("<H", position + 1))]
        )
        probe.check(reply[0][4] == 1, f"APWR 站地址 {position + 1} WKC")
    reply = master.transact([(probe.CMD["APWR"], auto_adp(1), 0x1100, b"\xbe\xef")])
    probe.check(reply[0][4] == 1, "APWR 第 2 站 SM2 WKC")

    deadline = time.time() + 3.0
    state = get_json("/state?slave=1")
    while time.time() < deadline and state["live"]["station_address"] != "0x0002":
        state = get_json("/state?slave=1")
        time.sleep(0.05)
    assert state["live"]["station_address"] == "0x0002", state["live"]

    pdo1 = get_json("/pdo?slave=1")
    assert pdo1["sm2"]["hex"] == "beef", pdo1["sm2"]
    pdo0 = get_json("/pdo?slave=0")
    assert pdo0["sm2"]["hex"] == "0000", pdo0["sm2"]
    default_pdo = get_json("/pdo")
    assert default_pdo["sm2"]["hex"] == "0000", default_pdo["sm2"]

    state_default = get_json("/state")
    assert state_default["live"]["station_address"] == "0x0001", state_default["live"]

    assert status_of("/state?slave=99") == 404
    assert status_of("/pdo?slave=99") == 404
    assert status_of("/pdo?slave=abc") == 404

    print(
        "OK: 逐站查看（N=1 站地址 0x0002/SM2=beef；N=0 SM2=0000；越界/非法 slave → 404）"
    )
finally:
    master.close()
PY

echo "OK: /state?slave=N 与 /pdo?slave=N 预回归通过"
