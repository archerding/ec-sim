#!/usr/bin/env bash
# Spike 3 可复现脚本：最小 ESC 应答器 + 未修改的 SOEM / IgH 扫描。
#
# 前置（在 Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：cargo build --release -p esc-node
#   - SOEM 的 slaveinfo、IgH 的 ethercat 工具与 ec_master/ec_generic 模块已安装
#   - 以 root 运行（TAP + 模块加载需要）
#
# 用法：sudo TAP=escsim0 MAC=02:ec:51:4d:00:01 ./minimal-esc-responder.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
IGH_ETHERCAT="${IGH_ETHERCAT:-/opt/ecsim/igh/bin/ethercat}"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose >/tmp/ec-sim-esc.log 2>&1 &
sleep 0.6
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

echo "=== SOEM slaveinfo ==="
"$SOEM_SLAVEINFO" "$TAP" 2>&1 | sed -n '1,20p'

echo "=== IgH ethercat slaves ==="
modprobe ec_master main_devices="$MAC"
modprobe ec_generic
sleep 2
"$IGH_ETHERCAT" slaves

echo "=== responder stats ==="
cat /tmp/ec-sim-esc.log
