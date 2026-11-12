#!/usr/bin/env bash
# B11 预回归：**通用 CiA 402 伺服声明式画像**在双主站（未修改 SOEM / IgH）下的
# 扫描 / PDO 映射 / 进 OP 验证。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - SOEM `slaveinfo` / `simple_ng`、IgH `ethercat`（独立进程夹具）
#   - 以 root 运行
#
# 断言（画像：SM2 7 字节 / SM3 7 字节，RxPDO 0x1600、TxPDO 0x1A00）：
#   1. SOEM `slaveinfo`：身份 `Man: 00000ec5 ID: 0000e5e0`、`Output size: 56bits /
#      Input size: 56bits`、`MBX protocols : 04`；
#   2. SOEM `slaveinfo -map`：`0x6040`（RxPDO）与 `0x6041`（TxPDO）映射可见；
#   3. SOEM `simple_ng`：`mapped 7O+7I` 且 `all slaves are now operational`（OP）；
#   4. IgH `ethercat slaves`：`PREOP` + 身份 `0x00000ec5:0x0000e5e0`
#      （无 ecrt 应用时 IgH 只配置到 PREOP）。
#
# 用法：sudo ./generic-cia402-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest $MANIFEST"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  killall simple_ng 2>/dev/null || true
  pkill -f ecrt- 2>/dev/null || true
  sleep 0.5
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" >/tmp/ec-sim-cia402.log 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

echo "=== SOEM slaveinfo（CiA 402 通用画像）==="
timeout 60 "$SOEM_SLAVEINFO" "$TAP" >/tmp/soem-cia402.out 2>&1 || true
tr '\r' '\n' </tmp/soem-cia402.out >/tmp/soem-cia402.lines
grep -E "Man:|Output size|Input size|MBX protocols" /tmp/soem-cia402.lines | head -6 || true
grep -q "Man: 00000ec5 ID: 0000e5e0" /tmp/soem-cia402.lines || fail "SOEM 身份不是 00000ec5:0000e5e0"
grep -q "Output size: 56bits" /tmp/soem-cia402.lines || fail "SOEM 未识别 56 位输出（0x1600）"
grep -q "Input size: 56bits" /tmp/soem-cia402.lines || fail "SOEM 未识别 56 位输入（0x1A00）"
grep -qE "MBX protocols *: *04" /tmp/soem-cia402.lines || fail "SOEM 未识别 CoE 邮箱"

echo "=== SOEM slaveinfo -map（PDO 映射）==="
timeout 60 "$SOEM_SLAVEINFO" "$TAP" -map >/tmp/soem-cia402-map.out 2>&1 || true
tr '\r' '\n' </tmp/soem-cia402-map.out >/tmp/soem-cia402-map.lines
grep -E "0x6040|0x6041|0x607a|0x6064" /tmp/soem-cia402-map.lines | head -8 || true
grep -q "0x6040" /tmp/soem-cia402-map.lines || fail "SOEM -map 未看到 RxPDO 0x6040"
grep -q "0x6041" /tmp/soem-cia402-map.lines || fail "SOEM -map 未看到 TxPDO 0x6041"

echo "=== SOEM simple_ng（OP，零补丁）==="
timeout 30 "$SOEM_SIMPLE_NG" "$TAP" >/tmp/soem-cia402-ng.out 2>&1 || true
tr '\r' '\n' </tmp/soem-cia402-ng.out >/tmp/soem-cia402-ng.lines
grep -E "mapped|operational|WKC" /tmp/soem-cia402-ng.lines | head -6 || true
grep -q "mapped 7O+7I" /tmp/soem-cia402-ng.lines || fail "SOEM simple_ng 未映射 7O+7I"
grep -q "all slaves are now operational" /tmp/soem-cia402-ng.lines || fail "SOEM simple_ng 未进入 OP"

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
"$IGH_BIN" slaves | grep -q "0x00000ec5:0x0000e5e0" || fail "IgH 未看到 CiA 402 画像身份"
"$IGH_BIN" slaves | grep -q "PREOP" || fail "IgH 未把从站配置到 PREOP"

echo "OK: 通用 CiA 402 画像双主站通过（SOEM 56/56 bits、0x6040/0x6041 映射、7O+7I 进 OP；IgH PREOP + 身份 0x00000ec5:0x0000e5e0）"
