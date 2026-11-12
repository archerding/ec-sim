#!/usr/bin/env bash
# B6/B11 验证：**未修改 SOEM 的 DC 配置路径**——SOEM `simple_ng` 在 `ecx_configdc()` 阶段
# 会读系统时间（0x0910）、写 DC 激活/启动/周期寄存器（0x0980/0x0990/0x09A0 等）。
# 用 SV660N 画像（声明 DC）验证：
#   1. SOEM 输出 `Configuring distributed clock... done`、`mapped 7O+7I`、
#      `all slaves are now operational`（OP）、`WKC 3`；
#   2. 应答器 verbose 统计里 **dc_rd / dc_wr > 0**（证明 DC 寄存器确实被主站访问）；
#   3. IgH 身份/PREOP 不受影响（可选，见 sv660n-capability-probe.sh）。
#
# 前置：Linux TAP + root；`cargo build --release -p esc-node`；SOEM `simple_ng`。
# 用法：sudo ./soem-dc-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-plugins/inovance-sv660n/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
ESC_LOG=/tmp/ec-sim-esc-dc.log
NG_OUT=/tmp/soem-dc-ng.out

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest $MANIFEST"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  killall simple_ng 2>/dev/null || true
  sleep 0.5
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" >"$ESC_LOG" 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

echo "=== SOEM simple_ng（含 ecx_configdc）==="
timeout 30 "$SOEM_SIMPLE_NG" "$TAP" >"$NG_OUT" 2>&1 || true
tr '\r' '\n' <"$NG_OUT" >/tmp/soem-dc-ng.lines
grep -E "Configuring distributed clock|mapped|operational|WKC" /tmp/soem-dc-ng.lines | head -8 || true
grep -q "Configuring distributed clock... done" /tmp/soem-dc-ng.lines || fail "SOEM DC 配置未完成"
grep -q "mapped 7O+7I" /tmp/soem-dc-ng.lines || fail "SOEM 未映射 7O+7I（SV660N 画像）"
grep -q "all slaves are now operational" /tmp/soem-dc-ng.lines || fail "SOEM simple_ng 未进入 OP"
grep -qE "WKC 3" /tmp/soem-dc-ng.lines || fail "SOEM WKC 不是 3"

# 应答器统计：取最后一条 STATS，解析 dc_rd/dc_wr。
STATS_LINE=$(grep "STATS " "$ESC_LOG" | tail -1 || true)
[ -n "$STATS_LINE" ] || fail "应答器未输出 STATS（--verbose 未生效？）"
echo "STATS: $STATS_LINE"
DC_RD=$(printf '%s\n' "$STATS_LINE" | sed -n 's/.*dc_rd=\([0-9]*\).*/\1/p')
DC_WR=$(printf '%s\n' "$STATS_LINE" | sed -n 's/.*dc_wr=\([0-9]*\).*/\1/p')
[ -n "$DC_RD" ] && [ -n "$DC_WR" ] || fail "STATS 缺少 dc_rd/dc_wr 字段"
[ "$DC_RD" -gt 0 ] || fail "主站未读取 DC 系统时间（dc_rd=0）"
[ "$DC_WR" -gt 0 ] || fail "主站未写 DC 配置寄存器（dc_wr=0）"

echo "OK: SOEM DC 配置路径通过（OP、WKC 3；应答器 dc_rd=${DC_RD} dc_wr=${DC_WR}）"
