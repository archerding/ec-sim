#!/usr/bin/env bash
# 邮箱数据链路预回归（B7 子项 1）：未修改的 SOEM 通过 CoE 邮箱向本仓库模拟器发 SDO 请求，
# 模拟器回 SDO 响应（`0x1C00` 等对象来自最小对象字典）。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - SOEM 的 slaveinfo 已构建（独立进程夹具，不链接、不复制其代码）
#   - 以 root 运行（TAP 需要）
#
# 断言（任一失败即 exit 1）：
#   1. SII 声明 mailbox：`MBX length wr: 128 rd: 128 MBX protocols : 04`（04 = CoE）；
#   2. 主站仍能找到 1 个从站并映射 8 位 I/O（邮箱启用不破坏扫描/过程数据）；
#   3. 应答器统计出现 `mbox=N`（N > 0，即真实收到了 SDO 邮箱请求）；
#   4. 应答器统计出现 `mboxresp=N`（N > 0，即回了 SDO 响应）。
#
# 用法：sudo TAP=escsim0 ./soem-mailbox-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
LOG=/tmp/ec-sim-esc-mailbox.log
OUT=/tmp/soem-slaveinfo-mailbox.out

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose >"$LOG" 2>&1 &
sleep 0.6
ip link set "$TAP" up
sleep 0.3

# -map 会触发 SOEM 的 CoE SDO 读取（0x1C00 等）；对字典内对象回 expedited upload，字典外回 SDO Abort。
timeout 60 "$SOEM_SLAVEINFO" "$TAP" -map >"$OUT" 2>&1 || true
tr '\r' '\n' <"$OUT" >"$OUT.lines"

grep -q "MBX length wr: 128 rd: 128 MBX protocols : 04" "$OUT.lines" \
  || { echo "FAIL: SII 未按 128 字节 + CoE 声明 mailbox"; grep -E "MBX" "$OUT.lines" || true; exit 1; }
grep -q "1 slaves found and configured" "$OUT.lines" \
  || { echo "FAIL: SOEM 未找到从站"; exit 1; }
grep -q "Output size: 8bits" "$OUT.lines" \
  || { echo "FAIL: 输出映射不是 8 位（SII PDO 类别未生效）"; exit 1; }
grep -q "Input size: 8bits" "$OUT.lines" \
  || { echo "FAIL: 输入映射不是 8 位（SII PDO 类别未生效）"; exit 1; }

if ! grep -qE "mbox=[1-9][0-9]*" "$LOG"; then
  echo "FAIL: 应答器没有收到邮箱请求（SDO 未发出/未被识别）"
  tail -n 5 "$LOG"
  exit 1
fi
if ! grep -qE "mboxresp=[1-9][0-9]*" "$LOG"; then
  echo "FAIL: 应答器没有回 SDO 响应"
  tail -n 5 "$LOG"
  exit 1
fi

echo "OK: SOEM 通过 CoE 邮箱发出 SDO 请求并收到 SDO 响应（$(grep -aoE 'mbox=[0-9]+ mboxresp=[0-9]+' "$LOG" | tail -n1)）"
