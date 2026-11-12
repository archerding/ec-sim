#!/usr/bin/env bash
# B11 预回归：**Inovance SV660N 兼容画像**（SIMULATED）在未修改 SOEM / IgH 下的
# 能力位 / PDO / 状态验证，重点复核画像是否如实反映公开事实：
#   - 设备**不响应 SDO Info** → SII 通用类别 `CoE details` bit1 必须为 0（`0x0D`）；
#   - 每方向一个 PDO、7 字节 Tier0 通用布局 → SOEM 看到 56/56 bits；
#   - **DC 不声明**（模拟器未实现 DC 同步）→ `Has DC: 0`（⚠️ 与真实 SV660N 默认开 DC 有差距）。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - SOEM `slaveinfo`、IgH `ethercat`（独立进程夹具）
#   - 以 root 运行
#
# 断言：
#   1. SOEM `slaveinfo`：`Man: 00000ec5 ID: 0000e5f1`、56/56 bits、`MBX protocols : 04`、
#      `CoE details: 0d`（bit1=0 → 不声明 SDO Info）、`Has DC: 0`；
#   2. IgH `ethercat slaves`：`PREOP` + 身份 `0x00000ec5:0x0000e5f1`。
#
# 用法：sudo ./sv660n-capability-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/inovance-sv660n/plugin.yaml}"
EXPECT_VID="${EXPECT_VID:-00000ec5}"
EXPECT_PID="${EXPECT_PID:-0000e5f1}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest $MANIFEST"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  sleep 0.3
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" >/tmp/ec-sim-sv660n.log 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

echo "=== SOEM slaveinfo（SV660N 兼容画像）==="
timeout 60 "$SOEM_SLAVEINFO" "$TAP" >/tmp/soem-sv660n.out 2>&1 || true
tr '\r' '\n' </tmp/soem-sv660n.out >/tmp/soem-sv660n.lines
grep -E "Man:|Output size|Input size|MBX protocols|CoE details|Has DC" /tmp/soem-sv660n.lines | head -10 || true
grep -q "Man: $EXPECT_VID ID: $EXPECT_PID" /tmp/soem-sv660n.lines || fail "SOEM 身份不是 $EXPECT_VID:$EXPECT_PID"
grep -q "Output size: 56bits" /tmp/soem-sv660n.lines || fail "SOEM 未识别 56 位输出（Tier0 7 字节）"
grep -q "Input size: 56bits" /tmp/soem-sv660n.lines || fail "SOEM 未识别 56 位输入（Tier0 7 字节）"
grep -qE "MBX protocols *: *04" /tmp/soem-sv660n.lines || fail "SOEM 未识别 CoE 邮箱"
grep -q "CoE details: 0d" /tmp/soem-sv660n.lines || fail "CoE details 不是 0x0D（应不声明 SDO Info，bit1=0）"
grep -q "Has DC: 0" /tmp/soem-sv660n.lines || fail "画像不应声明 DC（Has DC 应为 0）"

echo "=== IgH 扫描 ==="
dmesg -C 2>/dev/null || true
if ! lsmod | grep -q '^ec_master'; then
  modprobe ec_master main_devices="$MAC" 2>/dev/null
  modprobe ec_generic 2>/dev/null
else
  "$IGH_BIN" rescan 2>/dev/null || true
fi
sleep 2
"$IGH_BIN" slaves
"$IGH_BIN" slaves | grep -q "0x${EXPECT_VID}:0x${EXPECT_PID}" || fail "IgH 未看到 SV660N 画像身份"
"$IGH_BIN" slaves | grep -q "PREOP" || fail "IgH 未把从站配置到 PREOP"

echo "OK: SV660N 兼容画像能力位/PDO 复核通过（SOEM 56/56 bits、CoE details=0x0D 不声明 SDO Info、Has DC=0、MBX 04；IgH PREOP + 身份 0x${EXPECT_VID}:0x${EXPECT_PID}）"
