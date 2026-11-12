#!/usr/bin/env bash
# 过程数据预回归（P0）：未修改的 SOEM `simple_ng` ↔ 本仓库 `esc-sim-esc` 做 8 位 PDO 交换。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - SOEM 的 simple_ng 已构建（独立进程夹具，不链接、不复制其代码）
#   - 以 root 运行（TAP 需要）
#
# 断言（任一失败即 exit 1）：
#   1. SOEM 找到 1 个从站；
#   2. 映射为 1 字节输出 + 1 字节输入（来自 SII SM/PDO 类别）；
#   3. 从 SAFEOP 进入 OPERATIONAL；
#   4. 之后每轮过程数据 WKC = 3（= 输出 FMMU +2、输入 FMMU +1），无 "wrong"。
#
# 用法：sudo TAP=escsim0 ./soem-process-data.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
ROUNDS_SEC="${ROUNDS_SEC:-6}"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" >/tmp/ec-sim-esc-processdata.log 2>&1 &
sleep 0.6
ip link set "$TAP" up
sleep 0.3

# simple_ng 用 \r 刷新同一行，先归一化成行再断言。
timeout "$ROUNDS_SEC" "$SOEM_SIMPLE_NG" "$TAP" >/tmp/soem-simple-ng.out 2>&1 || true
tr '\r' '\n' </tmp/soem-simple-ng.out >/tmp/soem-simple-ng.lines

grep -q "1 slaves found" /tmp/soem-simple-ng.lines \
  || { echo "FAIL: SOEM 未找到从站"; exit 1; }
grep -q "mapped 1O+1I bytes" /tmp/soem-simple-ng.lines \
  || { echo "FAIL: I/O 映射不是 1O+1I（SII SM/PDO 类别未被主站采用）"; exit 1; }
grep -q "all slaves are now operational" /tmp/soem-simple-ng.lines \
  || { echo "FAIL: 从站未进入 OP"; exit 1; }

WRONG=$(grep -c "wrong" /tmp/soem-simple-ng.lines || true)
if [ "$WRONG" -ne 0 ]; then
  echo "FAIL: $WRONG 轮 WKC 与主站期望不符"
  grep "wrong" /tmp/soem-simple-ng.lines | head -n 3
  exit 1
fi

WKC_ROUNDS=$(grep -c "WKC 3" /tmp/soem-simple-ng.lines || true)
if [ "$WKC_ROUNDS" -lt 100 ]; then
  echo "FAIL: WKC=3 的轮次只有 $WKC_ROUNDS（预期 >=100）"
  head -n 12 /tmp/soem-simple-ng.lines
  exit 1
fi

echo "OK: SOEM simple_ng 进入 OP，$WKC_ROUNDS 轮 WKC=3，无 WKC 不符"
