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
#   1) 默认模式（ESM 前置条件检查开启）：BRD/APRD/APWR 后 FPRD/DL Status 0x5610/一帧两
#      datagram 的 WKC 全部正确；邮箱 SM 未配置 → 拒绝 PREOP（0x0015）、过程数据 SM 未配置
#      → 拒绝 SAFEOP（0x0017），配置 SM 后 INIT→PREOP→SAFEOP→OP 成功；
#      SM 过程数据看门狗 10 ms → SAFEOP+ERROR 0x001B → ACK 清错；
#   2) `--esm-lax` 模式（关闭前置条件检查，兼容/负向对照）：不做拒绝断言，配置 SM 后迁移成功。
#   3) `--manifest` 声明式画像 + `--sdo-segmented`：EL2008 画像的 0x1008 设备名
#      （VISIBLE_STRING，42 字节）必须经 **segmented SDO upload** 被合成主站完整读出。
#
# 用法：./uds-loopback.sh
set -euo pipefail

SOCK="${SOCK:-/tmp/ecsim-uds.sock}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
LOG=/tmp/ec-sim-esc-uds.log

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" "$SOCK".lax "$SOCK".client* "$SOCK".lax.client* 2>/dev/null || true
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

# 第一轮：默认模式（ESM 前置条件检查开启，验证拒绝路径与放行路径）。
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

# 第二轮：--esm-lax 模式（关闭前置条件检查的兼容路径）。
STRICT_SOCK="$SOCK.lax"
"$BIN" --uds "$STRICT_SOCK" --esm-lax >"$LOG.lax" 2>&1 &
strict_pid=$!
if ! wait_for_socket "$STRICT_SOCK"; then
  echo "FAIL: 模拟器（--esm-lax）未创建 UDS socket $STRICT_SOCK"
  cat "$LOG.lax"
  exit 1
fi
"$PY" scripts/spikes/uds_master_probe.py "$STRICT_SOCK" --lax
kill "$strict_pid" 2>/dev/null || true
wait "$strict_pid" 2>/dev/null || true

# 第三轮：声明式 manifest 画像 + segmented SDO 上传（0x1008 设备名）。
MANIFEST_SOCK="$SOCK.manifest"
"$BIN" --uds "$MANIFEST_SOCK" --manifest plugins/beckhoff-el2008/plugin.yaml \
  >"$LOG.manifest" 2>&1 &
manifest_pid=$!
if ! wait_for_socket "$MANIFEST_SOCK"; then
  echo "FAIL: 模拟器（--manifest）未创建 UDS socket $MANIFEST_SOCK"
  cat "$LOG.manifest"
  exit 1
fi
"$PY" scripts/spikes/uds_master_probe.py "$MANIFEST_SOCK" --sdo-segmented \
  --expect-string="Beckhoff EL2008 compatible 8DO (SIMULATED)"
kill "$manifest_pid" 2>/dev/null || true
wait "$manifest_pid" 2>/dev/null || true
