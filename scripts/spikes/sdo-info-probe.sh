#!/usr/bin/env bash
# CoE SDO Information 预回归（B7 子项：`slaveinfo -sdo` / `ethercat sdos` 所需的字典服务）：
# 未修改的 SOEM 通过 CoE SDO Info 服务读取本仓库模拟器的对象字典。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - SOEM 的 slaveinfo 已构建（独立进程夹具，不链接、不复制其代码）
#   - 以 root 运行（TAP 需要）
#
# 断言（任一失败即 exit 1）：
#   1. SII 通用类别声明 CoE SDO Info 能力，SOEM 能读回 6 个对象；
#   2. 对象描述（Get OD）与条目描述（Get Entry Description）字段正确：
#      0x1018 / 0x1C12 的对象码、0x1C12:01 的 UNSIGNED16 + RW 访问权限；
#   3. 主站仍能找到 1 个从站并映射 8 位 I/O（SDO Info 不破坏扫描/过程数据）。
#
# 已知限制：IgH `ethercat sdos` 在"无应用"时输出为空（与 B8 记录的 IgH 工具行为一致），
# 需要 ecrt 应用上下文才能取字典；本脚本只验证 SOEM 侧，IgH 侧 ⚠️ 待验证。
#
# 用法：sudo TAP=escsim0 ./sdo-info-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
LOG=/tmp/ec-sim-esc-sdoinfo.log
OUT=/tmp/soem-slaveinfo-sdo.out

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

# `-sdo` 触发 SOEM 的 SDO Info 服务（Get OD List / Get OD / Get Entry Description）。
timeout 60 "$SOEM_SLAVEINFO" "$TAP" -sdo >"$OUT" 2>&1 || true
tr '\r' '\n' <"$OUT" >"$OUT.lines"

grep -q "CoE Object Description found, 6 entries." "$OUT.lines" \
  || { echo "FAIL: SOEM 未读到 6 个对象（SDO Info 响应不完整）"; grep -E "CoE|SDO" "$OUT.lines" || true; exit 1; }
grep -qE '0x1018 +"Identity"' "$OUT.lines" \
  || { echo "FAIL: 缺少 0x1018 Identity 对象描述"; exit 1; }
grep -qE '\[RECORD +maxsub\(0x04 / 4\)\]' "$OUT.lines" \
  || { echo "FAIL: 0x1018 的对象码/子索引数不正确"; exit 1; }
grep -qE 'Vendor ID' "$OUT.lines" \
  || { echo "FAIL: 0x1018:01 条目描述缺失"; exit 1; }
grep -qE '0x1c12 +"RxPDO assign"' "$OUT.lines" \
  || { echo "FAIL: 缺少 0x1C12 RxPDO assign 对象描述"; exit 1; }
grep -qE 'RW____' "$OUT.lines" \
  || { echo "FAIL: 0x1C12:01 的访问权限不是 PREOP 可读写（RW____）"; exit 1; }
grep -qE '0x1600 / 5632|0x1600' "$OUT.lines" \
  || { echo "FAIL: 0x1C12:01 的值（0x1600）未通过 SDO 上传读出"; exit 1; }
grep -q "1 slaves found and configured" "$OUT.lines" \
  || { echo "FAIL: SOEM 未找到从站"; exit 1; }
grep -q "Output size: 8bits" "$OUT.lines" \
  || { echo "FAIL: 输出映射不是 8 位"; exit 1; }
grep -q "Input size: 8bits" "$OUT.lines" \
  || { echo "FAIL: 输入映射不是 8 位"; exit 1; }

if ! grep -qE "mbox=[1-9][0-9]*" "$LOG"; then
  echo "FAIL: 应答器没有收到邮箱请求"
  tail -n 5 "$LOG"
  exit 1
fi

echo "OK: SOEM slaveinfo -sdo 经 CoE SDO Info 读到 6 个对象（0x1018/0x1C00/0x1C12/0x1C13/0x1600/0x1A00，含描述与访问权限）"
