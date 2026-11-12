#!/usr/bin/env bash
# ESC-over-UDS 线缆层预回归（B3 子项 1）：启动 `ec-sim-esc --uds`（Linux），
# 用 `uds_master_probe.py` 合成主站验证帧收发与 WKC。
#
# 前置（Linux 验证机上）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - python3（标准库即可）
#   - 无需 root（Unix domain socket）
#
# 断言：
#   1) 默认模式：BRD/APRD/APWR 后 FPRD/DL Status 0x5610/一帧两 datagram 的 WKC 全部正确，
#      SM 过程数据看门狗 10 ms → SAFEOP+ERROR 0x001B → ACK 清错；
#   2) `--esm-strict` 模式：邮箱 SM 未配置时拒绝 PREOP（0x0015）、过程数据 SM 未配置时
#      拒绝 SAFEOP（0x0017），配置 SM 后 INIT→PREOP→SAFEOP→OP 全部成功。
#
# 用法：./uds-loopback.sh
set -euo pipefail

SOCK="${SOCK:-/tmp/ecsim-uds.sock}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
LOG=/tmp/ec-sim-esc-uds.log

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" "$SOCK".strict "$SOCK".client* "$SOCK".strict.client* 2>/dev/null || true
}
trap cleanup EXIT

wait_for_socket() {
  local path="$1"
  for _ in $(seq 1 50); do
    [ -S "$path" ] && return 0
    sleep 0.1
  done
  return 1
}

cleanup

# 第一轮：默认模式（与两家未修改主站的既有行为一致，不做 ESM 前置条件检查）。
"$BIN" --uds "$SOCK" >"$LOG" 2>&1 &
ec_pid=$!
if ! wait_for_socket "$SOCK"; then
  echo "FAIL: 模拟器未创建 UDS socket $SOCK"
  cat "$LOG"
  exit 1
fi
"$PY" scripts/spikes/uds_master_probe.py "$SOCK"
kill "$ec_pid" 2>/dev/null || true
wait "$ec_pid" 2>/dev/null || true

# 第二轮：--esm-strict 模式（验证 ESM 前置条件拒绝路径与放行路径）。
STRICT_SOCK="$SOCK.strict"
"$BIN" --uds "$STRICT_SOCK" --esm-strict >"$LOG.strict" 2>&1 &
strict_pid=$!
if ! wait_for_socket "$STRICT_SOCK"; then
  echo "FAIL: 模拟器（--esm-strict）未创建 UDS socket $STRICT_SOCK"
  cat "$LOG.strict"
  exit 1
fi
"$PY" scripts/spikes/uds_master_probe.py "$STRICT_SOCK" --strict
kill "$strict_pid" 2>/dev/null || true
wait "$strict_pid" 2>/dev/null || true
