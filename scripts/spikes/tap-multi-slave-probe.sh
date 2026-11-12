#!/usr/bin/env bash
# B13 预回归（M1 退出项）：**TAP 上 8 从站链**——`ec-sim-esc --tap <tap> --slaves 8` +
# 未修改 SOEM（TAP 版，验证机本地构建，不入库）：
#   1. `slaveinfo <tap>`：`8 slaves found and configured`、`Calculated workcounter 24`、
#      8 个 `Slave:` 块；
#   2. `simple_ng <tap>`：`mapped 8O+8I`、WKC 24 稳定；并从输出估算**循环频率**
#      （iterations / 最后 T 时间，simple_ng 名义 1 ms 周期 → 约 1 kHz；实测值随环境）。
#
# 前置：root、TAP 权限、验证机本地 SOEM TAP 构建：
#   /opt/ecsim/fixtures/soem/build/samples/{slaveinfo,simple_ng}
#   （可用 SOEM_SLAVEINFO / SOEM_SIMPLE_NG 覆盖）
#
# 用法：sudo ./tap-multi-slave-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
N="${N:-8}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
LOG=/tmp/ec-sim-esc-tap-multi.log
SLAVEINFO_OUT=/tmp/soem-tap-multi-slaveinfo.out
SIMPLE_NG_OUT=/tmp/soem-tap-multi-simple-ng.out

fail() {
  echo "FAIL: $1"
  exit 1
}

[ "$(id -u)" -eq 0 ] || fail "需要 root（TAP + AF_PACKET）"
for binary in "$SOEM_SLAVEINFO" "$SOEM_SIMPLE_NG"; do
  [ -x "$binary" ] || {
    echo "SKIP: 缺少验证机本地 SOEM TAP 构建 $binary"
    exit 2
  }
done

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
ip tuntap add dev "$TAP" mode tap
ip link set "$TAP" address "$MAC"
"$BIN" --tap "$TAP" --slaves "$N" >"$LOG" 2>&1 &
ec_pid=$!
sleep 0.8
ip link set "$TAP" up
sleep 0.3
[ -d "/sys/class/net/$TAP" ] || { cat "$LOG"; fail "TAP $TAP 未就绪"; }

echo "=== SOEM slaveinfo（TAP，$N 从站链）==="
"$SOEM_SLAVEINFO" "$TAP" >"$SLAVEINFO_OUT" 2>&1 || fail "slaveinfo 退出码非 0"
grep -q "$N slaves found and configured" "$SLAVEINFO_OUT" || {
  grep -E "slaves found" "$SLAVEINFO_OUT" || true
  fail "TAP slaveinfo 未报告 $N 个从站"
}
grep -q "Calculated workcounter 24" "$SLAVEINFO_OUT" || fail "TAP workcounter 不是 24"
slave_blocks=$(grep -c "^Slave:" "$SLAVEINFO_OUT" || true)
[ "$slave_blocks" -eq "$N" ] || fail "TAP slaveinfo 从站块数 $slave_blocks != $N"
echo "OK: TAP slaveinfo 报告 $N 个从站（workcounter 24）"

echo "=== SOEM simple_ng（TAP，$N 从站链，8 秒）==="
timeout 8 stdbuf -oL "$SOEM_SIMPLE_NG" "$TAP" >"$SIMPLE_NG_OUT" 2>&1 || true
grep -q "mapped 8O+8I" "$SIMPLE_NG_OUT" || fail "simple_ng 未映射 8O+8I"
grep -q "WKC 24" "$SIMPLE_NG_OUT" || fail "simple_ng 未出现 WKC 24"
iterations=$(grep -o "Iteration" "$SIMPLE_NG_OUT" | wc -l || true)
[ "$iterations" -ge 1 ] || fail "simple_ng 没有迭代输出"
last_t=$(grep -oE "T: [0-9]+" "$SIMPLE_NG_OUT" | tail -1 | awk '{print $2}' || true)
if [ -n "$last_t" ]; then
  rate=$(awk -v i="$iterations" -v t="$last_t" 'BEGIN { printf "%.0f", i * 1000000.0 / t }')
  echo "OK: simple_ng 映射 8O+8I、$iterations 轮 WKC 24、估算循环频率 ≈ ${rate} Hz（名义 1 kHz，非隔离 CPU）"
  if [ "$rate" -lt 1000 ]; then
    echo "⚠️ 实测 ${rate} Hz < M1 目标 1 kHz（非隔离 CPU/未实时调度；记录为环境限制）"
  fi
  [ "$rate" -ge 200 ] || fail "8 从站循环频率过低：${rate} Hz"
else
  rate="unknown"
  echo "OK: simple_ng 映射 8O+8I、$iterations 轮 WKC 24（T 时间戳不可解析，跳过频率估算）"
fi

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
echo "OK: TAP 8 从站链预回归通过（未修改 SOEM，循环频率 ≈ ${rate} Hz）"
