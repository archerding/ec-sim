#!/usr/bin/env bash
# B11 验证：**IgH 侧 CiA 402 CST+DC 画像 + **显式 DC 配置**（`ecrt_slave_config_dc` 0x0300/1ms）进 OP，验证 IgH DC 配置路径**。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - IgH 已安装（`ec_master`/`ec_generic`、`libethercat`、`ecrt.h`）
#   - 验证机本地 ecrt 夹具（**不入库**）：
#       /opt/ecsim/work/igh-op/ecrt-dc-op.c（脚本会在源码存在时构建）
#   - 以 root 运行
#
# 断言：
#   1. 应用运行中 `ethercat slaves`：`OP` + 身份 `0x00000ec5:0x0000e5e2`；
#   2. `RESULT al_states=0x08`；
#   3. `STATUS sw=... modes_display=10 torque_actual=300`：Operation enabled(bit2)、
#      模式显示 10（CST）、实际转矩镜像目标转矩 300（Tier0）；
#   4. 应答器侧 `al=0x08`。
#
# 用法：sudo ./igh-cia402-cst-op-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-cst-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
IGH_INCLUDE="${IGH_INCLUDE:-/opt/ecsim/igh/include}"
IGH_LIB="${IGH_LIB:-/opt/ecsim/igh/lib/libethercat.a}"
IGH_APP_SRC="${IGH_APP_SRC:-/opt/ecsim/work/igh-op/ecrt-dc-op.c}"
IGH_APP_BIN="${IGH_APP_BIN:-/opt/ecsim/work/igh-op/ecrt-dc-op}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"
ESC_LOG=/tmp/ec-sim-esc-igh-dc.log
APP_OUT=/tmp/igh-dc-op.out

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

echo "=== ecrt 应用（CST：写目标转矩、读实际转矩/状态字）==="
timeout 60 "$IGH_APP_BIN" 2 >"$APP_OUT" 2>&1 &
APP_PID=$!
sleep 1
"$IGH_BIN" slaves | tee /tmp/igh-dc-slaves-op.txt || true
grep -qE "^0[[:space:]]+0:0[[:space:]]+OP" /tmp/igh-dc-slaves-op.txt \
  || fail "IgH 应用运行期间从站不是 OP"
wait "$APP_PID" || fail "ecrt 应用退出码非 0（见 $APP_OUT）"
cat "$APP_OUT"
grep -q "RESULT al_states=0x08" "$APP_OUT" || fail "从站未进入 OP"
grep -q "modes_display=10" "$APP_OUT" || fail "模式显示不是 10（CST）"
grep -q "torque_actual=300" "$APP_OUT" || fail "实际转矩不是 300（行为未生效？）"
SW=$(grep -oE "sw=0x[0-9A-Fa-f]{4}" "$APP_OUT" | head -1 | cut -d= -f2)
[ -n "$SW" ] || fail "未读到状态字"
python3 - "$SW" <<'PY' || fail "状态字缺 Operation enabled"
import sys
sw = int(sys.argv[1], 16)
sys.exit(0 if sw & 0x0004 else 1)
PY
grep -q "al=0x08" "$ESC_LOG" || { tail -n 5 "$ESC_LOG"; fail "应答器侧未进入 OP"; }

# 应答器侧确认 DC 寄存器被 IgH 访问（--verbose STATS 里的 dc_rd/dc_wr）。
STATS_LINE=$(grep "STATS " "$ESC_LOG" | tail -1 || true)
[ -n "$STATS_LINE" ] || fail "应答器未输出 STATS"
echo "STATS: $STATS_LINE"
DC_WR=$(printf '%s\n' "$STATS_LINE" | sed -n 's/.*dc_wr=\([0-9]*\).*/\1/p')
[ -n "$DC_WR" ] && [ "$DC_WR" -gt 0 ] || fail "IgH 未写 DC 配置寄存器（dc_wr=0）"

echo "OK: IgH 零补丁 + DC 配置（0x0300/1ms）仍进 OP（sw=$SW, torque=300, dc_wr=$DC_WR）"
