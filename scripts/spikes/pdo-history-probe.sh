#!/usr/bin/env bash
# B25 预回归：`/pdo?history=1` 历史缓冲：
#   - 启动 `ec-sim-esc --uds <sock> --esm-lax --serve 127.0.0.1:<port>`（默认 SM2/SM3 各 2B）；
#   - 首次 `/pdo?history=1`：schema=pdo-history-v1、count>=1；
#   - 重复请求（无变化）不应增长 count（去重）；
#   - UDS APWR 改 SM2 两次（0xBEEF → 0x1234），每次后 `/pdo` 变化并 push；
#   - 最终 `/pdo?history=1`：count>=3、最后一条 sm2_hex 等于最新值、count<=256。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./pdo-history-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
PORT="${PORT:-18098}"
SOCK="${SOCK:-/tmp/ecsim-pdo-hist.sock}"
LOG=/tmp/ec-sim-pdo-hist.log

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p ec-sim-esc）"
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

socket_path, http = sys.argv[1], "http://" + sys.argv[2]


def get_json(path):
    with urllib.request.urlopen(http + path, timeout=2) as response:
        assert response.status == 200
        return json.loads(response.read().decode("utf-8"))


def history():
    return get_json("/pdo?history=1")


def pdo():
    return get_json("/pdo")


def wait_pdo_hex(expected):
    deadline = time.time() + 3.0
    latest = None
    while time.time() < deadline:
        latest = pdo()
        if latest["sm2"]["hex"] == expected:
            return latest
        time.sleep(0.05)
    raise AssertionError(f"SM2 未变为 {expected}: {latest}")


data = history()
assert data["schema"] == "ec-sim/pdo-history-v1", data["schema"]
count_first = data["count"]
assert count_first >= 1, data
count_repeat = history()["count"]
assert count_repeat == count_first, f"无变化时历史不应增长：{count_first} -> {count_repeat}"

master = probe.SyntheticMaster(socket_path, timeout=2.0)
try:
    reply = master.transact([(probe.CMD["APWR"], 0, 0x1100, b"\xbe\xef")])
    probe.check(reply[0][4] == 1, "APWR SM2 WKC")
    wait_pdo_hex("beef")
    history()  # 触发一次采样
    count_after_first = history()["count"]
    assert count_after_first == count_first + 1, (count_first, count_after_first)

    reply = master.transact([(probe.CMD["APWR"], 0, 0x1100, b"\x34\x12")])
    probe.check(reply[0][4] == 1, "APWR SM2 WKC（第二帧）")
    wait_pdo_hex("3412")
    data = history()
    assert data["count"] >= count_first + 2, data["count"]
    assert data["samples"][-1]["sm2_hex"] == "3412", data["samples"][-1]
    assert data["count"] <= 256, data["count"]
    print(
        "OK: /pdo?history=1 去重采样 %d 条，最后 SM2=%s（缓冲区上限 256）"
        % (data["count"], data["samples"][-1]["sm2_hex"])
    )
finally:
    master.close()
PY

echo "OK: /pdo 历史缓冲预回归通过"
