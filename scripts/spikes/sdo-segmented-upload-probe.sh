#!/usr/bin/env bash
# CoE 分段 SDO 上传预回归（B7 子项）：>4 字节对象（VISIBLE_STRING 设备名 0x1008:00）
# 必须通过 **segmented upload** 才能被主站完整读出。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - SOEM 的 slaveinfo 已构建（独立进程夹具）
#   - manifest 里声明了 >4 字节字符串对象（默认用 EL2008 插件画像的 0x1008 设备名）
#   - 以 root 运行（TAP 需要）
#
# 断言：
#   1. SOEM `slaveinfo -sdo` 读到 0x1008 "Device name"，数据类型 VISIBLE_STR、位长 >32；
#   2. 读出的值等于 manifest 中声明的字符串（SOEM 通过分段上传拼接，证明分段实现正确）。
#
# 已知限制：IgH `ethercat upload` CLI 在无 ecrt 应用配置时无法确定 SDO 数据类型
# （`Failed to determine SDO entry data type`），本脚本只验证 SOEM；IgH 侧分段上传
# ⚠️ 待验证（需要先激活的 ecrt 应用上下文）。
#
# 用法：sudo TAP=escsim0 MANIFEST=plugins/beckhoff-el2008/plugin.yaml ./sdo-segmented-upload-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MANIFEST="${MANIFEST:-plugins/beckhoff-el2008/plugin.yaml}"
EXPECTED_INDEX="${EXPECTED_INDEX:-0x1008}"
EXPECTED_NAME="${EXPECTED_NAME:-Device name}"
EXPECTED_VALUE="${EXPECTED_VALUE:-Beckhoff EL2008 compatible 8DO (SIMULATED)}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest：$MANIFEST"

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" >/tmp/ec-sim-sdo-seg.log 2>&1 &
sleep 0.7
ip link set "$TAP" up
sleep 0.3

# SOEM slaveinfo -sdo 对 UINT8 子索引 0 有已知缺陷（2 字节读取），做最多 3 次重试。
sdo_ok=0
for attempt in 1 2 3; do
  timeout 60 "$SOEM_SLAVEINFO" "$TAP" -sdo >"/tmp/soem-sdo-seg.$attempt.out" 2>&1 || true
  tr '\r' '\n' <"/tmp/soem-sdo-seg.$attempt.out" >/tmp/soem-sdo-seg.lines
  if grep -q "$EXPECTED_INDEX" /tmp/soem-sdo-seg.lines \
     && grep -q "$EXPECTED_NAME" /tmp/soem-sdo-seg.lines \
     && grep -q "VISIBLE_STR" /tmp/soem-sdo-seg.lines; then
    sdo_ok=1
    break
  fi
  sleep 0.3
done
[ "$sdo_ok" -eq 1 ] || {
  echo "FAIL: SOEM -sdo 未读到 $EXPECTED_INDEX 的字符串对象（3 次重试均失败）"
  tail -n 20 /tmp/soem-sdo-seg.lines 2>/dev/null || true
  exit 1
}

grep -E "0x1008|Device name|VISIBLE_STR" /tmp/soem-sdo-seg.lines | head -5 || true
grep -qF "$EXPECTED_VALUE" /tmp/soem-sdo-seg.lines \
  || fail "设备名字符串内容与 manifest 不一致（期望：$EXPECTED_VALUE）"

if ! grep -qE "mbox=[1-9][0-9]*" /tmp/ec-sim-sdo-seg.log; then
  echo "FAIL: 应答器没有收到邮箱请求"
  tail -n 5 /tmp/ec-sim-sdo-seg.log
  exit 1
fi

echo "OK: SOEM 经 segmented SDO upload 完整读到 $EXPECTED_INDEX（$EXPECTED_NAME，$EXPECTED_VALUE）"
