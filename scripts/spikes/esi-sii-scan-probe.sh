#!/usr/bin/env bash
# B6 验证：**ESI XML → SII 镜像**在双主站（未修改 SOEM / IgH）下的扫描闭环。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`（`--esi` 入口）
#   - SOEM `slaveinfo`、IgH `ethercat` 已构建/安装（独立进程夹具）
#   - 以 root 运行（TAP + 模块加载需要）
#
# 断言（ESI 生成画像：自有 VID/PID、SM0/SM1 邮箱、SM2 输出 1 字节、SM3 输入 1 字节）：
#   1. SOEM `slaveinfo`：身份 `Man: 00000ec5 ID: 0000e5d3 Rev: 00010000`、
#      `Output size: 8bits / Input size: 8bits`、`MBX protocols : 04`；
#   2. IgH `ethercat slaves`：`PREOP` + 身份 `0x00000ec5:0x0000e5d3`。
#
# 用法：sudo ESI=scripts/spikes/fixtures/sample.esi.xml ./esi-sii-scan-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
ESI="${ESI:-crates/plugin-host/tests/fixtures/sample.esi.xml}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$ESI" ] || fail "找不到 ESI 文件 $ESI"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  # 验证机本地 ecrt 夹具可能残留并持有 ec_master 引用（会让 rmmod 失败、主站保留旧从站身份）。
  pkill -f ecrt- 2>/dev/null || true
  sleep 0.5
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose --esi "$ESI" >/tmp/ec-sim-esi.log 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

echo "=== SOEM slaveinfo（ESI 生成画像）==="
timeout 60 "$SOEM_SLAVEINFO" "$TAP" >/tmp/soem-esi.out 2>&1 || true
tr '\r' '\n' </tmp/soem-esi.out >/tmp/soem-esi.lines
grep -E "Man:|Output size|Input size|MBX protocols|Identity" /tmp/soem-esi.lines | head -8 || true
grep -q "Man: 00000ec5 ID: 0000e5d3" /tmp/soem-esi.lines || fail "SOEM 身份与 ESI 不一致"
grep -q "Output size: 8bits" /tmp/soem-esi.lines || fail "SOEM 未识别 8 位输出"
grep -q "Input size: 8bits" /tmp/soem-esi.lines || fail "SOEM 未识别 8 位输入"
grep -qE "MBX protocols *: *04" /tmp/soem-esi.lines || fail "SOEM 未识别 CoE 邮箱"

echo "=== IgH 扫描（ESI 生成画像）==="
dmesg -C 2>/dev/null || true
if ! lsmod | grep -q '^ec_master'; then
  modprobe ec_master main_devices="$MAC" 2>/dev/null
  modprobe ec_generic 2>/dev/null
else
  # 模块已被前一次运行加载（rmmod 可能因引用失败）：让主站重新扫描总线。
  "$IGH_BIN" rescan 2>/dev/null || true
fi
sleep 2
"$IGH_BIN" slaves
"$IGH_BIN" slaves | grep -q "0x00000ec5:0x0000e5d3" || fail "IgH 未看到 ESI 身份"
"$IGH_BIN" slaves | grep -q "PREOP" || fail "IgH 未把从站配置到 PREOP"

echo "OK: ESI XML → SII 画像在未修改 SOEM/IgH 下扫描通过（身份 0x00000ec5:0x0000e5d3、8+8 bits、CoE 邮箱）"
