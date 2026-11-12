#!/usr/bin/env bash
# B7 验证：**分段 SDO 下载**（segmented download）——合成主站向可写字符串对象
# 0x2100 写入 20 字节，再经 SDO（分段）上传回读并比对。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - `cargo build --release -p esc-node`
#   - python3（`scripts/spikes/uds_master_probe.py`）
#   - 不需要 root（UDS 线缆层）
#
# 用法：./cia402-sdo-download-probe.sh
set -euo pipefail

MANIFEST="${MANIFEST:-plugins/generic-cia402-servo/plugin.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
SOCK="${SOCK:-/tmp/ecsim-cia402-dl.sock}"
LOG=/tmp/ec-sim-cia402-dl.log

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
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" >"$LOG" 2>&1 &
for _ in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  sleep 0.05
done
[ -S "$SOCK" ] || {
  cat "$LOG"
  fail "模拟器未创建 UDS socket $SOCK"
}

"$PY" scripts/spikes/uds_master_probe.py "$SOCK" --sdo-segmented-download

echo "OK: 分段 SDO 下载（>4 字节对象）经合成主站写入并回读一致"
