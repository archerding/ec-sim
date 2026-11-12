#!/usr/bin/env bash
# B11 验证：**IgH 侧 CiA 402 画像进 OP 并验证运行时行为**（`--behavior cia402`）。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - IgH 已安装（`ec_master`/`ec_generic`、`libethercat`、`ecrt.h`）
#   - 验证机本地 ecrt 夹具源码/二进制（**不入库**）：
#       /opt/ecsim/work/igh-op/ecrt-cia402-op.c（脚本会在源码存在时构建）
#   - 以 root 运行
#
# 断言：
#   1. `ethercat slaves`：`OP` + 身份 `0x00000ec5:0x0000e5e0`；
#   2. 应用 `RESULT al_states=0x08`；
#   3. `STATUS sw=0x... modes_display=8 pos=1000`，且状态字含 Operation enabled(bit2) 与
#      Target reached(bit10) —— 即 7 字节 TxPDO 由 CiA 402 行为驱动；
#   4. 应答器侧 `al=0x08`。
#
# 用法：sudo ./igh-cia402-op-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
IGH_INCLUDE="${IGH_INCLUDE:-/opt/ecsim/igh/include}"
IGH_LIB="${IGH_LIB:-/opt/ecsim/igh/lib/libethercat.a}"
IGH_APP_SRC="${IGH_APP_SRC:-/opt/ecsim/work/igh-op/ecrt-cia402-op.c}"
IGH_APP_BIN="${IGH_APP_BIN:-/opt/ecsim/work/igh-op/ecrt-cia402-op}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"
ESC_LOG=/tmp/ec-sim-esc-igh-cia402.log
APP_OUT=/tmp/igh-cia402-op.out

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest $MANIFEST"
if [ -f "$IGH_APP_SRC" ]; then
  gcc -O2 -I"$IGH_INCLUDE" -o "$IGH_APP_BIN" "$IGH_APP_SRC" "$IGH_LIB" -lpthread -lrt
fi
[ -x "$IGH_APP_BIN" ] || fail "未找到 ecrt 夹具 $IGH_APP_BIN（验证机本地构建，源码不入库）"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  pkill -f ecrt- 2>/dev/null || true
  sleep 0.5
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" --behavior cia402 >"$ESC_LOG" 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3
dmesg -C 2>/dev/null || true
modprobe ec_master main_devices="$MAC" 2>/dev/null
modprobe ec_generic 2>/dev/null
sleep 2

echo "=== IgH ethercat slaves ==="
"$IGH_BIN" slaves
"$IGH_BIN" slaves | grep -q "0x00000ec5:0x0000e5e0" || fail "IgH 未看到 CiA 402 画像身份"

echo "=== ecrt 应用（写控制字序列 / 读状态字）==="
timeout 60 "$IGH_APP_BIN" 2 >"$APP_OUT" 2>&1 &
APP_PID=$!
sleep 1
echo "--- 应用运行中的 ethercat slaves ---"
"$IGH_BIN" slaves | tee /tmp/igh-cia402-slaves-op.txt || true
grep -qE "^0[[:space:]]+0:0[[:space:]]+OP" /tmp/igh-cia402-slaves-op.txt \
  || fail "IgH 应用运行期间从站不是 OP"
wait "$APP_PID" || true
cat "$APP_OUT"
grep -q "RESULT al_states=0x08" "$APP_OUT" || fail "从站未进入 OP"
grep -q "modes_display=8" "$APP_OUT" || fail "TxPDO 模式显示不是 8（行为未生效？）"
grep -q "pos=1000" "$APP_OUT" || fail "TxPDO 实际位置不是 1000（行为未生效？）"
SW=$(grep -oE "sw=0x[0-9A-Fa-f]{4}" "$APP_OUT" | head -1 | cut -d= -f2)
[ -n "$SW" ] || fail "未读到状态字"
python3 - "$SW" <<'PY' || fail "状态字缺少 Operation enabled(0x0004) 或 Target reached(0x0400)"
import sys
sw = int(sys.argv[1], 16)
sys.exit(0 if (sw & 0x0004) and (sw & 0x0400) else 1)
PY
grep -q "al=0x08" "$ESC_LOG" || { tail -n 5 "$ESC_LOG"; fail "应答器侧未进入 OP"; }

echo "OK: IgH 零补丁把 CiA 402 画像推入 OP，且 TxPDO 状态字/模式显示/实际位置由行为驱动（$SW）"
