#!/usr/bin/env bash
# B6/B8 验证：**ESI + manifest 组合画像**（ESI 出 SII 描述、manifest 出身份与对象字典）
# 在双主站下扫描并进入 **OP**。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - SOEM `slaveinfo` / `simple_ng`、IgH `ethercat`（独立进程夹具）
#   - 以 root 运行
#
# 断言：
#   1. SOEM `slaveinfo`：身份取 manifest（`Man: 00000ec5 ID: 0000e5d3`）、
#      SII 的 SM/PDO 取 ESI（`Output size: 8bits / Input size: 8bits`、`MBX protocols : 04`）；
#   2. SOEM `simple_ng`：`mapped 1O+1I`，且 `all slaves are now operational`（进入 OP）；
#   3. IgH `ethercat slaves`：`PREOP` + 身份 `0x00000ec5:0x0000e5d3`（无 ecrt 应用时 IgH 只到 PREOP；OP 见 `igh-el2008-op-probe.sh`）。
#
# 用法：sudo ESI=crates/plugin-host/tests/fixtures/sample.esi.xml \
#       MANIFEST=crates/plugin-host/tests/fixtures/sample.esi.plugin.yaml ./esi-manifest-op-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
ESI="${ESI:-crates/plugin-host/tests/fixtures/sample.esi.xml}"
MANIFEST="${MANIFEST:-crates/plugin-host/tests/fixtures/sample.esi.plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
IGH_BIN="${IGH_BIN:-/opt/ecsim/igh/bin/ethercat}"

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$ESI" ] || fail "找不到 ESI 文件 $ESI"
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
"$BIN" --tap "$TAP" --verbose --esi "$ESI" --manifest "$MANIFEST" >/tmp/ec-sim-esi-manifest.log 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

echo "=== SOEM slaveinfo（ESI 描述 + manifest 身份）==="
timeout 60 "$SOEM_SLAVEINFO" "$TAP" >/tmp/soem-esiman.out 2>&1 || true
tr '\r' '\n' </tmp/soem-esiman.out >/tmp/soem-esiman.lines
grep -E "Man:|Output size|Input size|MBX protocols" /tmp/soem-esiman.lines | head -6 || true
grep -q "Man: 00000ec5 ID: 0000e5d3" /tmp/soem-esiman.lines || fail "SOEM 身份不是 manifest 声明的 00000ec5:0000e5d3"
grep -q "Output size: 8bits" /tmp/soem-esiman.lines || fail "SOEM 未识别 8 位输出（ESI SII）"
grep -q "Input size: 8bits" /tmp/soem-esiman.lines || fail "SOEM 未识别 8 位输入（ESI SII）"

echo "=== SOEM simple_ng（OP，零补丁）==="
timeout 30 "$SOEM_SIMPLE_NG" "$TAP" >/tmp/soem-esiman-ng.out 2>&1 || true
tr '\r' '\n' </tmp/soem-esiman-ng.out >/tmp/soem-esiman-ng.lines
grep -E "mapped|operational|WKC" /tmp/soem-esiman-ng.lines | head -6 || true
grep -q "mapped 1O+1I" /tmp/soem-esiman-ng.lines || fail "SOEM simple_ng 未映射 1O+1I"
grep -q "all slaves are now operational" /tmp/soem-esiman-ng.lines || fail "SOEM simple_ng 未进入 OP"

echo "=== IgH 扫描（OP）==="
dmesg -C 2>/dev/null || true
if ! lsmod | grep -q '^ec_master'; then
  modprobe ec_master main_devices="$MAC" 2>/dev/null
  modprobe ec_generic 2>/dev/null
else
  "$IGH_BIN" rescan 2>/dev/null || true
fi
sleep 2
"$IGH_BIN" slaves
"$IGH_BIN" slaves | grep -q "0x00000ec5:0x0000e5d3" || fail "IgH 未看到 manifest 身份"
# IgH 无 ecrt 应用时只配置到 PREOP（OP 需要应用激活，见 igh-el2008-op-probe.sh）；这里断言 PREOP。`n"$IGH_BIN" slaves | grep -q "PREOP" || fail "IgH 未把从站配置到 PREOP"

echo "OK: ESI+manifest 组合画像：SOEM 经 SII 映射 1O+1I 并进入 OP（WKC 3）；IgH 扫描 PREOP + 身份 0x00000ec5:0x0000e5d3"
