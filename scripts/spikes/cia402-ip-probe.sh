#!/usr/bin/env bash
# B11 验证：**CiA 402 状态机行为**（`--behavior cia402`）——合成主站写 SM2 控制字
# （`0x6040`）、读 SM3 状态字（`0x6041`），断言标准状态转换序列。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - python3（合成主站 `scripts/spikes/uds_master_probe.py`）
#   - 不需要 root（UDS 线缆层）
#
# 断言序列（控制字 → 期望状态字）：
#   0x0000 → 0x0240（Switch on disabled）
#   0x0006 → 0x0221（Shutdown → Ready to switch on）
#   0x0007 → 0x0223（Switch on → Switched on）
#   0x000F → 0x0237（Enable operation → Operation enabled）
#   0x0000 → 0x0240（Disable voltage → Switch on disabled）
#
# 用法：./cia402-behavior-probe.sh
set -euo pipefail

MANIFEST="${MANIFEST:-plugins/generic-cia402-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-cia402-ip.sock}"
LOG=/tmp/ec-sim-cia402-behavior.log

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest $MANIFEST"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK"
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" --behavior cia402 >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  fail "模拟器未创建 UDS socket $SOCK"
}

grep -q "BEHAVIOR cia402" "$LOG" || {
  cat "$LOG"
  fail "模拟器未报告 BEHAVIOR cia402"
}
head -3 "$LOG" || true

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --cia402-ip

echo "OK: CiA 402 行为（Tier0）在合成主站下通过：控制字→状态字序列符合预期"
