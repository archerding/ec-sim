#!/usr/bin/env bash
# B27 预回归：`/topology` 多从站摘要：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port> --slaves 3`（默认 2B PDO）；
#   - 初始 `/topology`：count=3、每站 AL=0x01、站地址 0x0000、SM2/SM3 hex 全 0；
#   - 合成主站按链上位置配置站地址 1/2/3（ADP = -k mod 65536），写第 2 站 SM2=0xBEEF；
#   - 再取 `/topology`：3 个站地址依次 0x0001/0x0002/0x0003，第 2 站 SM2=beef、其余 0000。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./topology-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18100}"
SOCK="${SOCK:-/tmp/ecsim-topology.sock}"
LOG=/tmp/ec-sim-topology.log

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
import urllib.request

sys.path.insert(0, "scripts/spikes")
import uds_master_probe as probe

socket_path, http = sys.argv[1], "http://" + sys.argv[2]


def topology():
    with urllib.request.urlopen(http + "/topology", timeout=2) as response:
        assert response.status == 200
        return json.loads(response.read().decode("utf-8"))


def auto_adp(position):
    """链上第 position 个从站（0-based）的 APWR 地址：转发 position 次后 ADP=0。"""
    return (-position) & 0xFFFF


data = topology()
assert data["schema"] == "ec-sim/topology-v1", data["schema"]
assert data["count"] == 3, data["count"]
for slave in data["slaves"]:
    assert slave["al_status"] == "0x01", slave
    assert slave["station_address"] == "0x0000", slave
    assert slave["sm2"]["hex"] == "0000" and slave["sm3"]["hex"] == "0000", slave

master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    for position in range(3):
        reply = master.transact(
            [(probe.CMD["APWR"], auto_adp(position), 0x0010, struct.pack("<H", position + 1))]
        )
        probe.check(reply[0][4] == 1, f"APWR 站地址 {position + 1} WKC")
    # 只写第 2 个从站（position=1）的 SM2 首 2 字节。
    reply = master.transact([(probe.CMD["APWR"], auto_adp(1), 0x1100, b"\xbe\xef")])
    probe.check(reply[0][4] == 1, "APWR 第 2 站 SM2 WKC")

    deadline = time.time() + 3.0
    data = topology()
    while time.time() < deadline:
        data = topology()
        addresses = [slave["station_address"] for slave in data["slaves"]]
        if addresses == ["0x0001", "0x0002", "0x0003"] and data["slaves"][1]["sm2"]["hex"] == "beef":
            break
        time.sleep(0.05)
    addresses = [slave["station_address"] for slave in data["slaves"]]
    assert addresses == ["0x0001", "0x0002", "0x0003"], addresses
    assert data["slaves"][1]["sm2"]["hex"] == "beef", data["slaves"][1]
    assert data["slaves"][0]["sm2"]["hex"] == "0000", data["slaves"][0]
    assert data["slaves"][2]["sm2"]["hex"] == "0000", data["slaves"][2]
    print("OK: /topology 3 站（地址 1/2/3，第 2 站 SM2=beef，其余 0000）")
finally:
    master.close()
PY

echo "OK: /topology 多从站摘要预回归通过"
