#!/usr/bin/env bash
# B13 预回归（M1 退出项）：**TAP 上 N 从站链**（默认 N=8；B45/M3-W7 用 `N=32`）——
# `ec-sim-esc --tap <tap> --slaves N` + 未修改 SOEM（TAP 版，验证机本地构建，不入库）：
#   1. `slaveinfo <tap>`：`N slaves found and configured`、`Calculated workcounter N*3`、
#      N 个 `Slave:` 块；
#   2. `simple_ng <tap>`：`mapped NO+NI`、WKC N*3 稳定；记录每轮**往返时间**（usec）。
#      ⚠️ `simple_ng` 样例迭代间有 fixed 5ms sleep，iterations/T 不是控制环频率（B45 修正）。
#
# 前置：root、TAP 权限、验证机本地 SOEM TAP 构建：
#   /opt/ecsim/fixtures/soem/build/samples/{slaveinfo,simple_ng}
#   （可用 SOEM_SLAVEINFO / SOEM_SIMPLE_NG 覆盖）
#
# 用法：sudo ./tap-multi-slave-probe.sh            # 8 从站
#       sudo N=32 ./tap-multi-slave-probe.sh       # 32 从站（M3-W7 瓶颈记录）
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
N="${N:-8}"
EXPECTED_WKC=$((N * 3))
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
# 可选：SIM_CPU/SIM_RT 绑核/实时（如 SIM_CPU=2 SIM_RT=50），SOEM_CPU/SOEM_RT 同理。
SIM_CPU="${SIM_CPU:-}"
SIM_RT="${SIM_RT:-0}"
SOEM_CPU="${SOEM_CPU:-}"
SOEM_RT="${SOEM_RT:-0}"
sim_cmd=("$BIN" --tap "$TAP" --slaves "$N")
if [ -n "$SIM_CPU" ]; then sim_cmd=(taskset -c "$SIM_CPU" "${sim_cmd[@]}"); fi
if [ "$SIM_RT" != "0" ]; then sim_cmd=(chrt -f "$SIM_RT" "${sim_cmd[@]}"); fi
"${sim_cmd[@]}" >"$LOG" 2>&1 &
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
grep -q "Calculated workcounter $EXPECTED_WKC" "$SLAVEINFO_OUT" || fail "TAP workcounter 不是 $EXPECTED_WKC"
slave_blocks=$(grep -c "^Slave:" "$SLAVEINFO_OUT" || true)
[ "$slave_blocks" -eq "$N" ] || fail "TAP slaveinfo 从站块数 $slave_blocks != $N"
echo "OK: TAP slaveinfo 报告 $N 个从站（workcounter $EXPECTED_WKC）"

echo "=== SOEM simple_ng（TAP，$N 从站链，8 秒）==="
soem_simple_cmd=(stdbuf -oL "$SOEM_SIMPLE_NG" "$TAP")
if [ -n "$SOEM_CPU" ]; then soem_simple_cmd=(taskset -c "$SOEM_CPU" "${soem_simple_cmd[@]}"); fi
if [ "$SOEM_RT" != "0" ]; then soem_simple_cmd=(chrt -f "$SOEM_RT" "${soem_simple_cmd[@]}"); fi
timeout 8 "${soem_simple_cmd[@]}" >"$SIMPLE_NG_OUT" 2>&1 || true; sleep 0.3
grep -q "mapped ${N}O+${N}I" "$SIMPLE_NG_OUT" || fail "simple_ng 未映射 ${N}O+${N}I"
grep -q "WKC $EXPECTED_WKC" "$SIMPLE_NG_OUT" || fail "simple_ng 未出现 WKC $EXPECTED_WKC"

# ⚠️ 语义修正（B45/M3-W7）：`simple_ng` 样例每次迭代固定 `osal_usleep(5000)`，
# 因此 `iterations / T` 只是**样例上限**（≈160–200 轮/s），不能当作控制环频率。
# 主站侧真正可比的指标是每轮**往返时间**：输出行 `<usec> usec  WKC <n>`。
usec_list=$(grep -oE "[0-9]+ usec  WKC $EXPECTED_WKC" "$SIMPLE_NG_OUT" | awk '{print $1}' || true)
bad_wkc=$(grep -oE "WKC [0-9]+" "$SIMPLE_NG_OUT" | grep -vc "WKC $EXPECTED_WKC" || true)
[ -n "$usec_list" ] || fail "simple_ng 未打印可解析的往返时间（usec WKC）"
rt=$(printf '%s\n' "$usec_list" | awk '
  { n++; sum += $1; if (n == 1 || $1 < min) min = $1; if ($1 > max) max = $1 }
  END { printf "n=%d min=%d avg=%.0f max=%d", n, min, sum / n, max }')
[ "$bad_wkc" -eq 0 ] || fail "simple_ng 有 $bad_wkc 轮 WKC 不是 $EXPECTED_WKC"
echo "OK: simple_ng 映射 ${N}O+${N}I、WKC $EXPECTED_WKC、往返时间 $rt usec"

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
echo "OK: TAP ${N} 从站链预回归通过（未修改 SOEM，往返 $rt usec）"
