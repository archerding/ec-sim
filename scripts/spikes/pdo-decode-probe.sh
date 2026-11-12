#!/usr/bin/env bash
# B26 预回归：`/pdo` 具名解码（按 SMS 映射 + 字典名/类型）：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port>
#     --manifest plugins/generic-cia402-cst-servo/plugin.yaml`（RxPDO/TxPDO 各 5 字节）；
#   - 合成主站 APWR 写 SM2 = cw(0x000F) + mode(8) + target torque(300)；
#     SM3 = sw(0x0237) + mode display(8) + torque actual(-100)；
#   - GET /pdo：`decoded` 数组应含具名对象：0x6040=15、0x6060=8、0x6071=300、
#     0x6041=0x0237、0x6061=8、0x6077=-100（有符号）。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./pdo-decode-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18099}"
SOCK="${SOCK:-/tmp/ecsim-pdo-decode.sock}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-cst-servo/plugin.yaml}"
LOG=/tmp/ec-sim-pdo-decode.log

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
"$BIN" --uds "$SOCK" --esm-lax --serve "127.0.0.1:$PORT" --manifest "$MANIFEST" >"$LOG" 2>&1 &
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


def pdo():
    with urllib.request.urlopen(http + "/pdo", timeout=2) as response:
        assert response.status == 200
        return json.loads(response.read().decode("utf-8"))


master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    # SM2: controlword 0x000F、mode 8、target torque 300。
    reply = master.transact(
        [(probe.CMD["APWR"], 0, 0x1100, struct.pack("<HbH", 0x000F, 8, 300))]
    )
    probe.check(reply[0][4] == 1, "APWR SM2 WKC")
    # SM3: statusword 0x0237、mode display 8、torque actual -100。
    reply = master.transact(
        [(probe.CMD["APWR"], 0, 0x1110, struct.pack("<HbH", 0x0237, 8, -100 & 0xFFFF))]
    )
    probe.check(reply[0][4] == 1, "APWR SM3 WKC")

    deadline = time.time() + 3.0
    decoded = {}
    while time.time() < deadline:
        data = pdo()
        if "decoded" not in data:
            time.sleep(0.05)
            continue
        decoded = {
            (entry["dir"], entry["index"]): entry for entry in data["decoded"]
        }
        if len(decoded) >= 6:
            break
        time.sleep(0.05)

    def value(direction, index):
        entry = decoded[(direction, index)]
        return entry["name"], int(entry["value"])

    assert value("rx", "0x6040")[1] == 15, decoded
    assert value("rx", "0x6060")[1] == 8, decoded
    assert value("rx", "0x6071")[1] == 300, decoded
    assert value("tx", "0x6041")[1] == 0x0237, decoded
    assert value("tx", "0x6061")[1] == 8, decoded
    assert value("tx", "0x6077")[1] == -100, decoded
    cw_name = value("rx", "0x6040")[0]
    torque_name = value("tx", "0x6077")[0]
    assert "Control" in cw_name or cw_name == "Controlword", cw_name
    assert "Torque" in torque_name or torque_name == "Torque actual value", torque_name
    print(
        "OK: PDO 具名解码 6 项（0x6040=15、0x6060=8、0x6071=300、0x6041=0x237、"
        "0x6061=8、0x6077=-100；名称=%s/%s）" % (cw_name, torque_name)
    )
finally:
    master.close()
PY

echo "OK: /pdo 具名解码预回归通过"
