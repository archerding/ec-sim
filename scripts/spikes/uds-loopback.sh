#!/usr/bin/env bash
# ESC-over-UDS 线缆层预回归（B3 子项 1）：启动 `ec-sim-esc --uds`（Linux），
# 用 `uds_master_probe.py` 合成主站验证帧收发与 WKC。
#
# 前置（Linux 验证机上）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - python3（标准库即可）
#   - 无需 root（Unix domain socket）
#
# 断言：BRD/APRD/APWR 后 FPRD/DL Status 0x5610/一帧两 datagram 的 WKC 全部正确。
#
# 用法：./uds-loopback.sh
set -euo pipefail

SOCK="${SOCK:-/tmp/ecsim-uds.sock}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
LOG=/tmp/ec-sim-esc-uds.log

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" "$SOCK".client* 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" >"$LOG" 2>&1 &
ec_pid=$!

# 等 socket 文件就绪（最多 5 秒）。
for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
if [ ! -S "$SOCK" ]; then
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  cat "$LOG"
  exit 1
fi

"$PY" scripts/spikes/uds_master_probe.py "$SOCK"

kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true
