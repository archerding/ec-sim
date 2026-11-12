#!/usr/bin/env bash
# B11 验证：**IgH 侧 CiA 402 CSV（速度模式，11 字节 PDO）画像进 OP 并验证行为**。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - IgH 已安装（`ec_master`/`ec_generic`、`libethercat`、`ecrt.h`）
#   - 验证机本地 ecrt 夹具源码/二进制（**不入库**）：
#       /opt/ecsim/work/igh-op/ecrt-cia402-csv-op.c（脚本会在源码存在时构建）
#   - 以 root 运行
#
# 断言：
#   1. 应用运行中 `ethercat slaves`：`OP` + 身份 `0x00000ec5:0x0000e5e1`；
#   2. `RESULT al_states=0x08`；
#   3. `STATUS sw=... modes_display=9 pos=>100 vel_actual=5`：Operation enabled(bit2)、
#      模式显示 9（CSV）、位置累加（Tier0 每周期 +5）、实际速度 = 5；
#   4. 应答器侧 `al=0x08`。
#
# 用法：sudo ./igh-cia402-csv-op-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-csv-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
IGH_INCLUDE="${IGH_INCLUDE:-/opt/ecsim/igh/include}"
IGH_LIB="${IGH_LIB:-/opt/ecsim/igh/lib/libethercat.a}"
IGH_APP_SRC="${IGH_APP_SRC:-/opt/ecsim/work/igh-op/ecrt-cia402-csv-op.c}"
IGH_APP_BIN="${IGH_APP_BIN:-/opt/ecsim/work/igh-op/ecrt-cia402-csv-op}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"
ESC_LOG=/tmp/ec-sim-esc-igh-cia402-csv.log
APP_OUT=/tmp/igh-cia402-csv-op.out

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

echo "=== IgH ethercat slaves（应用前）==="
"$IGH_BIN" slaves

echo "=== ecrt 应用（CSV：写目标速度、读实际位置/速度）==="
timeout 60 "$IGH_APP_BIN" 2 >"$APP_OUT" 2>&1 &
APP_PID=$!
sleep 1
"$IGH_BIN" slaves | tee /tmp/igh-cia402-csv-slaves-op.txt || true
grep -qE "^0[[:space:]]+0:0[[:space:]]+OP" /tmp/igh-cia402-csv-slaves-op.txt \
  || fail "IgH 应用运行期间从站不是 OP"
wait "$APP_PID" || true
cat "$APP_OUT"
grep -q "RESULT al_states=0x08" "$APP_OUT" || fail "从站未进入 OP"
grep -q "modes_display=9" "$APP_OUT" || fail "模式显示不是 9（CSV）"
grep -q "vel_actual=5" "$APP_OUT" || fail "实际速度不是 5（行为未生效？）"
SW=$(grep -oE "sw=0x[0-9A-Fa-f]{4}" "$APP_OUT" | head -1 | cut -d= -f2)
POS=$(grep -oE "pos=[0-9]+" "$APP_OUT" | head -1 | cut -d= -f2)
[ -n "$SW" ] || fail "未读到状态字"
python3 - "$SW" "$POS" <<'PY' || fail "状态字缺 Operation enabled 或位置未累加"
import sys
sw = int(sys.argv[1], 16)
pos = int(sys.argv[2])
sys.exit(0 if (sw & 0x0004) and pos > 100 else 1)
PY
grep -q "al=0x08" "$ESC_LOG" || { tail -n 5 "$ESC_LOG"; fail "应答器侧未进入 OP"; }

echo "OK: IgH 零补丁把 CSV 画像推入 OP，且 11 字节 TxPDO（状态字/模式/位置/速度）由行为驱动（$SW, pos=$POS）"
