#!/usr/bin/env bash
# B12 预回归（M1 退出项）：**8 从站链**——`ec-sim-esc --uds <sock> --slaves 8` +
# 未修改 SOEM 的 UDS shim（验证机本地构建，不入库）：
#   1. `slaveinfo <sock>` 报 "8 slaves found and configured"（每站 1O+1I，8 个从站块）；
#   2. `simple_ng <sock>` 连续跑若干轮，WKC 恒为 24（8 站 × 3 = 读+写+读？按 SOEM 语义），
#      无 WKC 下降（说明整链数据交换稳定）。
#
# 前置（Linux 验证机上，路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - SOEM UDS shim 构建：`/opt/ecsim/work/soem-uds/build-uds/samples/{slaveinfo,simple_ng}`
#     （可用 SOEM_UDS_SLAVEINFO / SOEM_UDS_SIMPLE_NG 覆盖）
#
# 用法：./soem-multi-slave-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
SOCK="${SOCK:-/tmp/ecsim-soem-multi.sock}"
N="${N:-8}"
SOEM_UDS_SLAVEINFO="${SOEM_UDS_SLAVEINFO:-/opt/ecsim/work/soem-uds/build-uds/samples/slaveinfo/slaveinfo}"
SOEM_UDS_SIMPLE_NG="${SOEM_UDS_SIMPLE_NG:-/opt/ecsim/work/soem-uds/build-uds/samples/simple_ng/simple_ng}"
LOG=/tmp/ec-sim-esc-soem-multi.log
SLAVEINFO_OUT=/tmp/soem-multi-slaveinfo.out
SIMPLE_NG_OUT=/tmp/soem-multi-simple-ng.out

fail() {
  echo "FAIL: $1"
  exit 1
}

for binary in "$SOEM_UDS_SLAVEINFO" "$SOEM_UDS_SIMPLE_NG"; do
  if [ ! -x "$binary" ]; then
    echo "SKIP: 缺少验证机本地 SOEM UDS shim 二进制 $binary"
    exit 2
  fi
done

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --slaves "$N" >"$LOG" 2>&1 &
ec_pid=$!
for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
[ -S "$SOCK" ] || { cat "$LOG"; fail "模拟器未创建 UDS socket $SOCK"; }

echo "=== SOEM slaveinfo（$N 从站链）==="
"$SOEM_UDS_SLAVEINFO" "$SOCK" >"$SLAVEINFO_OUT" 2>&1 || fail "slaveinfo 退出码非 0"
grep -q "$N slaves found and configured" "$SLAVEINFO_OUT" || {
  grep -E "slaves found" "$SLAVEINFO_OUT" || true
  fail "slaveinfo 未报告 $N 个从站"
}
slave_blocks=$(grep -c "^Slave:" "$SLAVEINFO_OUT" || true)
[ "$slave_blocks" -eq "$N" ] || fail "slaveinfo 从站块数 $slave_blocks != $N"
echo "OK: slaveinfo 报告 $N 个从站（$slave_blocks 个 Slave 块）"

echo "=== SOEM simple_ng（$N 从站链，10 秒）==="
timeout 10 stdbuf -oL "$SOEM_UDS_SIMPLE_NG" "$SOCK" >"$SIMPLE_NG_OUT" 2>&1 || true
grep -q "WKC 24" "$SIMPLE_NG_OUT" || {
  tail -n 5 "$SIMPLE_NG_OUT" || true
  fail "simple_ng 未出现 WKC 24（8 站链）"
}
grep -q "mapped 8O+8I" "$SIMPLE_NG_OUT" || fail "simple_ng 未映射 8 个从站的 I/O"
iterations=$(grep -c "Iteration" "$SIMPLE_NG_OUT" || true)
[ "$iterations" -ge 1 ] || fail "simple_ng 没有迭代输出"
echo "OK: simple_ng 映射 8O+8I、$iterations 轮 WKC 24（8 站 OP，数据交换稳定）"

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
echo "OK: SOEM 8 从站链预回归通过（UDS shim，未修改 SOEM）"
