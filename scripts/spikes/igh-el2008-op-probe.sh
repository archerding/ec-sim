#!/usr/bin/env bash
# B10 缺口补齐：IgH 应用预回归——未修改的 IgH + **outputs-only** ecrt 夹具，把
# "Beckhoff EL2008 兼容 8DO（SIMULATED）"声明式画像推入 **OP**。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - IgH 已安装（`ec_master`/`ec_generic`、`libethercat`、`ecrt.h`）
#   - 验证机本地夹具源码/二进制（**不入库**，避免把 GPL 链接代码带进仓库）：
#     `/opt/ecsim/work/igh-op/ecrt-el2008-op.c`（outputs-only：8 个 1 bit 输出对象）
#     脚本会在源码存在时用下面的命令构建（与既有 `ecrt-op-probe` 同一方式）：
#       gcc -O2 -I/opt/ecsim/igh/include -o /opt/ecsim/work/igh-op/ecrt-el2008-op \
#           /opt/ecsim/work/igh-op/ecrt-el2008-op.c \
#           /opt/ecsim/igh/lib/libethercat.a -lpthread -lrt
#   - 以 root 运行（TAP + 模块加载需要）
#
# 断言：
#   1. 应用运行期间 `ethercat slaves` 显示从站为 **OP** 且身份为 manifest 的 VID/PID；
#   2. 应用输出 `RESULT al_states=0x08`（OP）且退出码 0；
#   3. 应答器侧 `--verbose` 统计显示 `al=0x08`；
#   4. IgH 主站日志没有拓扑计算错误。
#
# 用法：sudo TAP=escsim0 MAC=02:ec:51:4d:00:01 ./igh-el2008-op-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/beckhoff-el2008/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
IGH_ETHERCAT="${IGH_ETHERCAT:-/opt/ecsim/igh/bin/ethercat}"
IGH_INCLUDE="${IGH_INCLUDE:-/opt/ecsim/igh/include}"
IGH_LIB="${IGH_LIB:-/opt/ecsim/igh/lib/libethercat.a}"
IGH_APP_SRC="${IGH_APP_SRC:-/opt/ecsim/work/igh-op/ecrt-el2008-op.c}"
IGH_APP_BIN="${IGH_APP_BIN:-/opt/ecsim/work/igh-op/ecrt-el2008-op}"
ESC_LOG=/tmp/ec-sim-esc-el2008-igh-op.log
APP_OUT=/tmp/el2008-igh-app.out

[ -f "$MANIFEST" ] || { echo "FAIL: 找不到 manifest $MANIFEST"; exit 2; }
if [ -f "$IGH_APP_SRC" ]; then
  gcc -O2 -I"$IGH_INCLUDE" -o "$IGH_APP_BIN" "$IGH_APP_SRC" "$IGH_LIB" -lpthread -lrt
fi
[ -x "$IGH_APP_BIN" ] || {
  echo "FAIL: 未找到 outputs-only ecrt 夹具 $IGH_APP_BIN（验证机本地构建，源码不入库）"
  exit 2
}

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" >"$ESC_LOG" 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3
dmesg -C 2>/dev/null || true
modprobe ec_master main_devices="$MAC"
modprobe ec_generic
sleep 2

"$IGH_APP_BIN" 8 >"$APP_OUT" 2>&1 &
APP_PID=$!
sleep 3
"$IGH_ETHERCAT" slaves >/tmp/el2008-igh-slaves.out 2>&1 || true
wait "$APP_PID"
APP_EXIT=$?

cat /tmp/el2008-igh-slaves.out
cat "$APP_OUT"

grep -q "OP" /tmp/el2008-igh-slaves.out \
  || { echo "FAIL: 应用运行期间 IgH 未把从站显示为 OP"; exit 1; }
grep -q "0x00000ec5:0x0000e5d2" /tmp/el2008-igh-slaves.out \
  || { echo "FAIL: IgH 显示的从站身份不是 EL2008 manifest 的 VID/PID"; exit 1; }
[ "$APP_EXIT" -eq 0 ] || { echo "FAIL: ecrt 应用退出码 $APP_EXIT（期望 0）"; exit 1; }
grep -q "RESULT al_states=0x08" "$APP_OUT" \
  || { echo "FAIL: 应用未确认 OP（期望 RESULT al_states=0x08）"; exit 1; }
grep -q "al=0x08" "$ESC_LOG" \
  || { echo "FAIL: 应答器侧未进入 OP"; tail -n 5 "$ESC_LOG"; exit 1; }
if dmesg | grep -q "Failed to calculate bus topology"; then
  echo "FAIL: IgH 拓扑计算失败"
  exit 1
fi

echo "OK: 未修改 IgH 经 outputs-only ecrt 应用把 EL2008 兼容 8DO 画像推入 OP（$(grep -o 'RESULT.*' "$APP_OUT")）"
