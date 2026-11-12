#!/usr/bin/env bash
# IgH 应用预回归（B8）：未修改的 IgH 主站 + 最小 ecrt 应用，把模拟器从站推入 **OP**。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - IgH 已安装（`ec_master`/`ec_generic` 模块、`libethercat`、`ecrt.h`）
#   - 验证机本地的最小 ecrt 夹具已构建（**源码不入库**，避免把 GPL 链接代码带进仓库）：
#       gcc -O2 -I/opt/ecsim/igh/include -o /opt/ecsim/work/igh-op/ecrt-op-probe \
#           /opt/ecsim/work/igh-op/ecrt-op-probe.c \
#           /opt/ecsim/igh/lib/libethercat.a -lpthread -lrt
#     夹具要点见 docs/spikes/spike-3-minimal-esc-responder.md §3.4（按本仓库对象字典
#     0x1600/0x1A00 配置 domain，2 ms 周期交换 1 字节输入/输出）。
#   - 以 root 运行（TAP + 模块加载需要）
#
# 断言：
#   1. 应用运行期间 `ethercat slaves` 显示从站为 **OP**；
#   2. 应用输出 `RESULT al_states=0x08 ... wc=3`；
#   3. 应答器侧 `--verbose` 统计显示 `al=0x08`（OP）。
#
# 用法：sudo TAP=escsim0 MAC=02:ec:51:4d:00:01 ./igh-op-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
BIN="${BIN:-target/release/ec-sim-esc}"
IGH_ETHERCAT="${IGH_ETHERCAT:-/opt/ecsim/igh/bin/ethercat}"
IGH_APP_BIN="${IGH_APP_BIN:-/opt/ecsim/work/igh-op/ecrt-op-probe}"
ESC_LOG=/tmp/ec-sim-esc-igh-op.log
APP_OUT=/tmp/igh-op-app.out

[ -x "$IGH_APP_BIN" ] || {
  echo "FAIL: 未找到 ecrt 夹具 $IGH_APP_BIN（验证机本地构建，源码不入库；见脚本头部注释）"
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
"$BIN" --tap "$TAP" --verbose >"$ESC_LOG" 2>&1 &
sleep 0.6
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3
modprobe ec_master main_devices="$MAC"
modprobe ec_generic
sleep 2

# 应用跑 8 秒（2 ms 周期）；运行期间查看主站看到的从站状态。
"$IGH_APP_BIN" 8 >"$APP_OUT" 2>&1 &
APP_PID=$!
sleep 3
"$IGH_ETHERCAT" slaves >/tmp/igh-op-slaves.out 2>&1 || true
wait "$APP_PID" || true

grep -q "OP" /tmp/igh-op-slaves.out \
  || { echo "FAIL: 应用运行期间 IgH 未把从站显示为 OP"; cat /tmp/igh-op-slaves.out; exit 1; }
grep -q "RESULT al_states=0x08" "$APP_OUT" \
  || { echo "FAIL: 应用未确认 OP（期望 RESULT al_states=0x08）"; cat "$APP_OUT"; exit 1; }
grep -q "wc=3" "$APP_OUT" \
  || { echo "FAIL: domain WKC 不是 3"; cat "$APP_OUT"; exit 1; }
grep -q "al=0x08" "$ESC_LOG" \
  || { echo "FAIL: 应答器侧未进入 OP"; tail -n 5 "$ESC_LOG"; exit 1; }

echo "OK: 未修改 IgH 经 ecrt 应用把模拟器从站推入 OP（$(grep -o 'RESULT.*' "$APP_OUT")）"
