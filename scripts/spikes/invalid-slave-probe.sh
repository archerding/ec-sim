#!/usr/bin/env bash
# ACC-27②（P1 缺陷）回归：控制面 `?slave=` 参数**统一校验**。
#
# 缺陷背景（sim-expert 2026-11-13）：`/sdo`、`/dict` 对非法/越界 slave **静默回退到站 0**
# 并返回 200；静态模式 `/sdo` 直接忽略 `slave` —— 诊断会被误导。
# 修复后契约：**凡"按站"的路由都校验 `?slave=`（0-based，取值 0..=N-1）**：
#   - 非数字（abc）、负数（-1）、空串、越界（>= N）→ HTTP **404** + `application/json`；
#   - 合法边界 0 与 N-1 → 200 且返回对应从站数据；
#   - 缺省 = 站 0（保持既有默认行为）。
# 覆盖：静态模式 / 实时 UDS / 实时 TAP（root 时）三路径；路由 /state /pdo /metrics /events /
#       /sdo /dict /sdo-audit + POST /sdo /pdo。
#
# 断言矩阵（每条路由）：
#   {非数字, 负数, 越界} → 404；{0, N-1} → 200（除个别路由的语义注记）。
# `/sdo-audit`：全局审计原语，`?slave=N` 表示按站过滤（非法仍 404、合法仍 200）。
# `/dict`：字典对当前 N 个同画像从站相同，但仍校验 `?slave`（非法 404、合法 200）。
#
# 前置：`cargo build --release -p esc-node`；python3；TAP 段需 root（无 root 打印 SKIP，不算失败）。
# 用法：./invalid-slave-probe.sh            # 静态 + UDS；root 时再跑 TAP
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MANIFEST="${MANIFEST:-$ROOT_DIR/plugins/generic-cia402-servo/plugin.yaml}"
STATIC_PORT="${STATIC_PORT:-18110}"
LIVE_PORT="${LIVE_PORT:-18111}"
TAP_PORT="${TAP_PORT:-18112}"
UDS_SOCK="${UDS_SOCK:-/tmp/ecsim-invalid-slave.sock}"
TAP="${TAP:-escsim9}"
N="${N:-3}"
LOG_DIR="${LOG_DIR:-/tmp/ecsim-invalid-slave}"
mkdir -p "$LOG_DIR"

os="$(uname -s 2>/dev/null || echo unknown)"
is_linux="no"
[ "${os#Linux}" != "$os" ] && is_linux="yes"
is_root="no"
[ "$(id -u 2>/dev/null || echo 1)" = "0" ] && is_root="yes"
[ -x "$BIN" ] || { echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"; exit 1; }
command -v "$PY" >/dev/null 2>&1 || { echo "FAIL: 找不到 Python：$PY"; exit 1; }

PIDS=""
cleanup() {
  for pid in $PIDS; do kill "$pid" 2>/dev/null || true; done
  sleep 0.2
  for pid in $PIDS; do kill -9 "$pid" 2>/dev/null || true; done
  if [ "$is_linux" = "yes" ]; then ip link del "$TAP" 2>/dev/null || true; fi
  rm -f "$UDS_SOCK" 2>/dev/null || true
}
trap cleanup EXIT

wait_http() {
  local port="$1"
  local waited=0
  while [ "$waited" -lt 100 ]; do
    if "$PY" - "$port" <<'PY' >/dev/null 2>&1
import socket, sys
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=0.2)
s.close()
PY
    then
      return 0
    fi
    sleep 0.05
    waited=$((waited + 1))
  done
  return 1
}

# ---------------------------------------------------------------------------
# 路径 1：静态模式（无 UDS/TAP；只有站 0 的快照）
# ---------------------------------------------------------------------------
echo "=== 路径 1/3：静态模式（--serve，无 live）==="
"$BIN" --serve "127.0.0.1:$STATIC_PORT" --manifest "$MANIFEST" >"$LOG_DIR/static.log" 2>&1 &
PIDS="$PIDS $!"
wait_http "$STATIC_PORT" || { cat "$LOG_DIR/static.log"; echo "FAIL: 静态 --serve 未就绪"; exit 1; }

"$PY" - "127.0.0.1:$STATIC_PORT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]


def status(path):
    try:
        with urllib.request.urlopen(base + path, timeout=3) as response:
            return response.status, response.headers.get("Content-Type", ""), response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers.get("Content-Type", ""), error.read()


failures = []


def expect(path, expected, label):
    code, ctype, body = status(path)
    if code != expected:
        failures.append("%s %s → %d（期望 %d）" % (label, path, code, expected))
    if code == 404 and "application/json" not in ctype:
        failures.append("%s %s → 404 但 Content-Type=%r" % (label, path, ctype))
    if code == 404 and b'"error"' not in body:
        failures.append("%s %s → 404 体不是 JSON error" % (label, path))


# 静态快照只有站 0（count=1）：合法仅 slave=0；其它一律 404（不得静默忽略）。
for path in [
    "/state?slave=0",
    "/dict?slave=0",
    "/sdo?index=0x6040&sub=0&slave=0",
]:
    expect(path, 200, "静态合法")
for raw in ["abc", "-1", "", "1", "99999999999999999999"]:
    for route in ["/state", "/dict", "/sdo?index=0x6040&sub=0"]:
        sep = "&" if "?" in route else "?"
        expect("%s%sslave=%s" % (route, sep, raw), 404, "静态非法")
# 缺省 = 站 0，保持 200。
expect("/state", 200, "静态缺省")
expect("/dict", 200, "静态缺省")

if failures:
    print("静态模式失败 %d 条：" % len(failures))
    for line in failures:
        print("  - " + line)
    sys.exit(1)
print("OK: 静态模式 — 合法 0/缺省 200；非数字/负数/空串/越界 404 JSON（/state /dict /sdo）")
PY

for pid in $PIDS; do kill "$pid" 2>/dev/null || true; done
PIDS=""
sleep 0.2

# ---------------------------------------------------------------------------
# 路径 2：实时 UDS 模式（N 个从站；覆盖全部"按站"路由 + 写路由）
# ---------------------------------------------------------------------------
if [ "$is_linux" != "yes" ]; then
  echo "SKIP: 实时 UDS / TAP 段需要 Linux（当前 $os）；静态段已通过"
  echo "OK: invalid-slave 探针（静态段）通过"
  exit 0
fi

echo "=== 路径 2/3：实时 UDS 模式（--slaves $N，含 POST 写路由）==="
rm -f "$UDS_SOCK"
"$BIN" --uds "$UDS_SOCK" --slaves "$N" --esm-lax --serve "127.0.0.1:$LIVE_PORT" \
  --manifest "$MANIFEST" --serve-write >"$LOG_DIR/live.log" 2>&1 &
PIDS="$PIDS $!"
for _ in $(seq 1 100); do [ -S "$UDS_SOCK" ] && break; sleep 0.05; done
wait_http "$LIVE_PORT" || { cat "$LOG_DIR/live.log"; echo "FAIL: 实时 --serve 未就绪"; exit 1; }

"$PY" - "127.0.0.1:$LIVE_PORT" "$N" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]
count = int(sys.argv[2])
last = count - 1
failures = []


def request(path, method="GET", body=None):
    req = urllib.request.Request(base + path, method=method, data=body)
    try:
        with urllib.request.urlopen(req, timeout=3) as response:
            code = response.status
            ctype = response.headers.get("Content-Type", "")
            # `/events` 是 SSE（长连接）：只取状态行/头，不读正文，避免阻塞。
            payload = b"" if path.startswith("/events") else response.read()
            return code, ctype, payload
    except urllib.error.HTTPError as error:
        return error.code, error.headers.get("Content-Type", ""), error.read()


def check(path, expected, method="GET", body=None):
    code, ctype, payload = request(path, method, body)
    if code != expected:
        failures.append("%s %s → %d（期望 %d）" % (method, path, code, expected))
    if code == 404:
        if "application/json" not in ctype:
            failures.append("%s %s → 404 但 Content-Type=%r" % (method, path, ctype))
        if b'"error"' not in payload:
            failures.append("%s %s → 404 体不是 JSON error" % (method, path))
    return code, payload


read_routes = [
    "/state",
    "/pdo",
    "/pdo?history=1",
    "/metrics",
    "/sdo?index=0x6040&sub=0",
    "/dict",
    "/sdo-audit",
    "/events",  # SSE：只取状态行（404 时不进入事件流）
]
invalid_values = ["abc", "-1", "", str(count), str(count + 7), "99999999999999999999"]
valid_values = [0, last]

for route in read_routes:
    sep = "&" if "?" in route else "?"
    for raw in invalid_values:
        check("%s%sslave=%s" % (route, sep, raw), 404)
    for index in valid_values:
        check("%s%sslave=%d" % (route, sep, index), 200)
    # 缺省 = 站 0（/metrics 缺省为全量，200）
    check(route, 200)

# 写路由：POST /sdo、POST /pdo 也必须校验 slave（开启 --serve-write）。
# 合法（slave=0 / N-1）→ 200（0x6040 为 UINT16、2 字节值合法；SM2 写入 2 字节合法）。
write_cases = [
    ("/sdo?index=0x6040&sub=0", b"\x06\x00"),
    ("/pdo", b"\x06\x00"),
]
for route, body in write_cases:
    sep = "&" if "?" in route else "?"
    for raw in invalid_values:
        check("%s%sslave=%s" % (route, sep, raw), 404, "POST", body)
    for index in valid_values:
        check("%s%sslave=%d" % (route, sep, index), 200, "POST", body)
    check(route, 200, "POST", body)

# 合法写必须真的写进对应从站（站 1 与站 0 不互相污染）：用 POST /pdo 写 0x1100。
for index in (0, last):
    check("/pdo?slave=%d" % index, 200, "POST", b"\xEF\xBE")
for index in (0, last):
    code, payload = check("/pdo?slave=%d" % index, 200)
    try:
        pdo = json.loads(payload.decode("utf-8"))
    except Exception as error:  # noqa: BLE001 - 探针输出可读性优先
        failures.append("/pdo?slave=%d 不是 JSON：%s" % (index, error))
        continue
    if pdo.get("sm2", {}).get("hex", "")[:4] != "efbe":
        failures.append(
            "/pdo?slave=%d 写入未落到该站：sm2.hex=%r"
            % (index, pdo.get("sm2", {}).get("hex"))
        )

if failures:
    print("实时 UDS 模式失败 %d 条：" % len(failures))
    for line in failures:
        print("  - " + line)
    sys.exit(1)
print(
    "OK: 实时 UDS — %d 条按站路由 × {非数字/负数/空串/越界} 全 404；0 与 N-1 全 200；"
    "POST /sdo /pdo 同校验且写入按站隔离" % len(read_routes)
)
PY

for pid in $PIDS; do kill "$pid" 2>/dev/null || true; done
PIDS=""
sleep 0.2

# ---------------------------------------------------------------------------
# 路径 3：实时 TAP 模式（root 时；验证 TAP 路径同一契约）
# ---------------------------------------------------------------------------
if [ "$is_root" != "yes" ] || [ "${SKIP_TAP:-0}" = "1" ]; then
  echo "SKIP: TAP 段需要 root 且未设置 SKIP_TAP=1（当前 root=$is_root SKIP_TAP=${SKIP_TAP:-0}）；静态 + UDS 段已通过"
  echo "OK: invalid-slave 探针（静态 + UDS 段）通过"
  exit 0
fi

echo "=== 路径 3/3：实时 TAP 模式（--slaves 2）==="
cleanup
ip tuntap add dev "$TAP" mode tap
ip link set "$TAP" address 02:ec:51:4d:00:09 2>/dev/null || true
ip link set "$TAP" up
"$BIN" --tap "$TAP" --slaves 2 --esm-lax --serve "127.0.0.1:$TAP_PORT" \
  --manifest "$MANIFEST" >"$LOG_DIR/tap.log" 2>&1 &
PIDS="$PIDS $!"
wait_http "$TAP_PORT" || { cat "$LOG_DIR/tap.log"; echo "FAIL: TAP --serve 未就绪"; exit 1; }

"$PY" - "127.0.0.1:$TAP_PORT" <<'PY'
import sys
import urllib.error
import urllib.request

base = "http://" + sys.argv[1]
failures = []


def status(path):
    try:
        with urllib.request.urlopen(base + path, timeout=3) as response:
            return response.status, response.headers.get("Content-Type", ""), response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers.get("Content-Type", ""), error.read()


for route in ["/state", "/pdo"]:
    for raw in ["abc", "-1", "2", "7"]:
        code, ctype, payload = status("%s?slave=%s" % (route, raw))
        if code != 404:
            failures.append("%s?slave=%s → %d（期望 404）" % (route, raw, code))
        elif "application/json" not in ctype:
            failures.append("%s?slave=%s → 404 但 Content-Type=%r" % (route, raw, ctype))
    for index in (0, 1):
        code, _, _ = status("%s?slave=%d" % (route, index))
        if code != 200:
            failures.append("%s?slave=%d → %d（期望 200）" % (route, index, code))

# /topology 是全局路由：带任何 ?slave（合法或非法）都必须显式 404（不静默忽略）。
for raw in ["abc", "-1", "0", "1", "2", "7"]:
    code, ctype, payload = status("/topology?slave=%s" % raw)
    if code != 404:
        failures.append("/topology?slave=%s → %d（全局路由期望 404）" % (raw, code))
    elif b'"error"' not in payload:
        failures.append("/topology?slave=%s → 404 体不是 JSON error" % raw)
code, _, _ = status("/topology")
if code != 200:
    failures.append("/topology（无 slave）→ %d（期望 200）" % code)

if failures:
    print("TAP 模式失败 %d 条：" % len(failures))
    for line in failures:
        print("  - " + line)
    sys.exit(1)
print("OK: 实时 TAP — 非法/越界 404 JSON；0 与 1 均 200")
PY

for pid in $PIDS; do kill "$pid" 2>/dev/null || true; done
PIDS=""

echo "OK: invalid-slave 探针三路径全通过（静态 / UDS / TAP；0-based 契约）"
