#!/usr/bin/env bash
# CNC 虚拟闭环对接（V1/V2 预回归）：**TAP 上 N 个 CiA 402 从站**（默认 N=2）。
#
# 目的：给 CNC 侧（LinuxCNC + linuxcnc-ethercat + IgH）一个可复现的「TAP + 多 CiA402 从站」
# EC-Sim 侧证据；画像默认 `plugins/cnc-csp-servo/plugin.yaml`
# （RxPDO 7B = 0x6040/0x6060/0x607A，TxPDO 11B = 0x6041/0x6061/0x6064/0x606C，
#  即 lcec `basic_cia402` 的 PP/CSP 期望；详见 docs/plan/CNC-V1-integration.md）。
#
# 断言：
#   1. SOEM `slaveinfo <tap>`：`N slaves found and configured`、`Calculated workcounter 3N`、
#      N 个 `Slave:` 块、每站 `Output size: 56bits` / `Input size: 88bits`；
#   2. SOEM `simple_ng <tap>`：`mapped <7N>O+<11N>I bytes`、`all slaves are now operational`；
#      若该 SOEM 构建的 simple_ng 打印 `WKC` 行（经典样例），则要求全部等于 `3N`（不允许坏 WKC）。
#   3. `--serve` 控制面（只读）：simple_ng 运行期间 `/topology` 有 N 条且全部 AL=0x08（OP）；
#      `/pdo?slave=0` 的 `decoded` 含 0x6040/0x6041/0x6064/0x606C 的具名值。
#
# 前置（EC-Sim 自己的验证机，root；**不要**用 CNC 的验证机）：
#   - `cargo build --release -p esc-node`
#   - 验证机本地 SOEM TAP 构建：/opt/ecsim/fixtures/soem/build/samples/{slaveinfo,simple_ng}
#     （可用 SOEM_SLAVEINFO / SOEM_SIMPLE_NG 覆盖）
#
# 用法：sudo ./tap-cia402-multi-slave-probe.sh          # 2 从站
#       sudo N=4 ./tap-cia402-multi-slave-probe.sh      # V2 形态（3 伺服 + IO 仍受"同一画像"限制）
# 边界：不验证 LinuxCNC/lcec 侧（CNC 自己的 VM）、不宣称一致性/CTT/认证。
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
N="${N:-2}"
MANIFEST="${MANIFEST:-plugins/cnc-csp-servo/plugin.yaml}"
PORT="${PORT:-18098}"
RX_BYTES=7
TX_BYTES=11
EXPECTED_WKC=$((N * 3))
EXPECTED_MAPPED="$((N * RX_BYTES))O+$((N * TX_BYTES))I"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
LOG=/tmp/ec-sim-esc-tap-cia402-multi.log
SLAVEINFO_OUT=/tmp/soem-tap-cia402-slaveinfo.out
SIMPLE_NG_OUT=/tmp/soem-tap-cia402-simple-ng.out

fail() {
  echo "FAIL: $1"
  exit 1
}

[ "$(id -u)" -eq 0 ] || fail "需要 root（TAP + AF_PACKET）"
[ -x "$BIN" ] || fail "找不到 $BIN（先 cargo build --release -p esc-node）"
[ -f "$MANIFEST" ] || fail "找不到画像 $MANIFEST"
for binary in "$SOEM_SLAVEINFO" "$SOEM_SIMPLE_NG"; do
  [ -x "$binary" ] || {
    echo "SKIP: 缺少验证机本地 SOEM TAP 构建 $binary"
    exit 2
  }
done

EC_PID=""
cleanup() {
  kill "$EC_PID" 2>/dev/null || true
  wait "$EC_PID" 2>/dev/null || true
  killall ec-sim-esc 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
ip tuntap add dev "$TAP" mode tap
ip link set "$TAP" address "$MAC"
"$BIN" --tap "$TAP" --slaves "$N" --manifest "$MANIFEST" --behavior cia402 \
  --serve "127.0.0.1:$PORT" >"$LOG" 2>&1 &
EC_PID=$!
sleep 0.8
ip link set "$TAP" up
sleep 0.3
[ -d "/sys/class/net/$TAP" ] || { cat "$LOG"; fail "TAP $TAP 未就绪"; }
grep -q "READY tap=" "$LOG" || { cat "$LOG"; fail "模拟器未打印 READY"; }

echo "=== SOEM slaveinfo（TAP，$N 个 CiA402 从站）==="
"$SOEM_SLAVEINFO" "$TAP" >"$SLAVEINFO_OUT" 2>&1 || fail "slaveinfo 退出码非 0"
grep -q "$N slaves found and configured" "$SLAVEINFO_OUT" || {
  grep -E "slaves found" "$SLAVEINFO_OUT" || true
  fail "slaveinfo 未报告 $N 个从站"
}
grep -q "Calculated workcounter $EXPECTED_WKC" "$SLAVEINFO_OUT" \
  || fail "workcounter 不是 $EXPECTED_WKC"
slave_blocks=$(grep -c "^Slave:" "$SLAVEINFO_OUT" || true)
[ "$slave_blocks" -eq "$N" ] || fail "slaveinfo 从站块数 $slave_blocks != $N"
# 每站 Rx/Tx 位宽（SOEM slaveinfo 文本；56bits = 7B，88bits = 11B）。
out_bits=$(grep -c "Output size: 56bits" "$SLAVEINFO_OUT" || true)
in_bits=$(grep -c "Input size: 88bits" "$SLAVEINFO_OUT" || true)
[ "$out_bits" -ge "$N" ] || fail "slaveinfo 的 56bits 输出出现 $out_bits 次（期望 ≥$N）"
[ "$in_bits" -ge "$N" ] || fail "slaveinfo 的 88bits 输入出现 $in_bits 次（期望 ≥$N）"
echo "OK: slaveinfo $N 个从站（workcounter $EXPECTED_WKC、Rx 56bits / Tx 88bits）"

echo "=== simple_ng + 控制面（$N 从站，OP / PDO 具名值）==="
"$PY" - "127.0.0.1:$PORT" "$TAP" "$SOEM_SIMPLE_NG" "$N" "$EXPECTED_MAPPED" "$EXPECTED_WKC" <<'PY'
import json
import subprocess
import sys
import time
import urllib.request

http_addr = "http://" + sys.argv[1]
tap = sys.argv[2]
simple_ng = sys.argv[3]
slave_count = int(sys.argv[4])
expected_mapped = sys.argv[5]
expected_wkc = sys.argv[6]


def get(path):
    with urllib.request.urlopen(http_addr + path, timeout=3) as response:
        return response.status, json.loads(response.read().decode("utf-8"))


process = subprocess.Popen(
    ["timeout", "8", "stdbuf", "-oL", simple_ng, tap],
    stdout=open("/tmp/soem-tap-cia402-simple-ng.out", "wb"),
    stderr=subprocess.STDOUT,
)
try:
    deadline = time.time() + 10.0
    op_slaves = 0
    topology = None
    while time.time() < deadline:
        _, topology = get("/topology")
        op_slaves = sum(
            1 for slave in topology["slaves"]
            if int(slave["al_status"], 16) & 0x0F == 0x08
        )
        if op_slaves == slave_count:
            break
        time.sleep(0.1)
    assert topology is not None
    assert topology["count"] == slave_count, topology
    assert op_slaves == slave_count, (
        "TAP 下 OP 从站数 %d != %d：%s" % (op_slaves, slave_count, topology["slaves"])
    )
    # 逐站 PDO 具名解码：cnc-csp-servo 的 TxPDO 含 0x6041/0x6061/0x6064/0x606C。
    _, pdo = get("/pdo?slave=0")
    assert pdo["schema"] == "ec-sim/pdo-v1", pdo
    names = {entry["index"] for entry in pdo.get("decoded", [])}
    for index in ("0x6040", "0x6041", "0x6064", "0x606C"):
        assert index in names, "decoded 缺少 %s：%s" % (index, sorted(names))
    print(
        "OK: /topology %d/%d 站 OP；/pdo?slave=0 decoded 含 %s"
        % (op_slaves, slave_count, ",".join(sorted(names)))
    )
finally:
    process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
PY

grep -q "mapped ${EXPECTED_MAPPED} bytes" "$SIMPLE_NG_OUT" \
  || { tail -n 20 "$SIMPLE_NG_OUT" || true; fail "simple_ng 未映射 $EXPECTED_MAPPED bytes"; }
grep -q "all slaves are now operational" "$SIMPLE_NG_OUT" \
  || { tail -n 20 "$SIMPLE_NG_OUT" || true; fail "simple_ng 未报告全部从站 operational"; }
# 验证机的 SOEM 2.0 `simple_ng`（samples/simple_ng）只做一次建链、不打印周期性 WKC；
# 若换用打印 `usec WKC` 的经典样例，则额外要求 WKC 全对（两种构建都覆盖）。
if grep -q "WKC " "$SIMPLE_NG_OUT"; then
  grep -q "WKC $EXPECTED_WKC" "$SIMPLE_NG_OUT" || fail "simple_ng 未出现 WKC $EXPECTED_WKC"
  bad_wkc=$(grep -oE "WKC [0-9]+" "$SIMPLE_NG_OUT" | grep -vc "WKC $EXPECTED_WKC" || true)
  [ "$bad_wkc" -eq 0 ] || fail "simple_ng 有 $bad_wkc 轮 WKC 不是 $EXPECTED_WKC"
  echo "OK: simple_ng 映射 $EXPECTED_MAPPED bytes、WKC $EXPECTED_WKC（无坏 WKC）"
else
  echo "OK: simple_ng 映射 $EXPECTED_MAPPED bytes、全部从站 operational（该构建不打印 WKC；WKC 由 slaveinfo 的 Calculated workcounter $EXPECTED_WKC 断言）"
fi

kill "$EC_PID" 2>/dev/null || true
wait "$EC_PID" 2>/dev/null || true
echo "OK: TAP + $N 个 CiA402 从站（cnc-csp-servo）预回归通过"
