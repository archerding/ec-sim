#!/usr/bin/env bash
# B23 预回归：`--serve` 实时 PDO 快照 `/pdo`（UDS + HTTP 同进程）：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port>`（无 manifest → 默认
#     SM2=0x1100/2B、SM3=0x1110/2B）；
#   - 初始 `/pdo`：schema=pdo-v1、sm2/sm3 hex 全 0、AL=0x01；
#   - 合成主站 APWR 写 0x1100=0xBEEF、0x1110=0x1234，轮询 `/pdo`，断言 hex 随写入变化。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./pdo-live-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18096}"
SOCK="${SOCK:-/tmp/ecsim-pdo.sock}"
LOG=/tmp/ec-sim-pdo.log

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


def pdo():
    with urllib.request.urlopen(http_addr + "/pdo", timeout=2) as response:
        assert response.status == 200
        return json.loads(response.read().decode("utf-8"))


data = pdo()
assert data["schema"] == "ec-sim/pdo-v1", data["schema"]
assert data["sm2"]["address"] == "0x1100" and data["sm2"]["hex"] == "0000", data["sm2"]
assert data["sm3"]["address"] == "0x1110" and data["sm3"]["hex"] == "0000", data["sm3"]
assert data["al_status"] == "0x01", data["al_status"]

master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    reply = master.transact([(probe.CMD["APWR"], 0, 0x1100, b"\xbe\xef")])
    probe.check(reply[0][4] == 1, "APWR SM2 WKC")
    reply = master.transact([(probe.CMD["APWR"], 0, 0x1110, b"\x12\x34")])
    probe.check(reply[0][4] == 1, "APWR SM3 WKC")

    deadline = time.time() + 3.0
    updated = None
    while time.time() < deadline:
        updated = pdo()
        if updated["sm2"]["hex"] == "beef" and updated["sm3"]["hex"] == "1234":
            break
        time.sleep(0.05)
    assert updated["sm2"]["hex"] == "beef", updated["sm2"]
    assert updated["sm3"]["hex"] == "1234", updated["sm3"]
    print("OK: /pdo 实时反映 SM2=beef、SM3=1234（只读快照）")
finally:
    master.close()
PY

echo "OK: --serve 实时 PDO 快照预回归通过"
