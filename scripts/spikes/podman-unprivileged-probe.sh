#!/usr/bin/env bash
# B14 预回归（M1 退出项）：**无特权容器**跑通 ESC-over-UDS shim——
# 用 podman 构建 `docker/Dockerfile.prebuilt`（预构建二进制的离线变体）并运行
# `ec-sim-esc --uds /sock/ecsim.sock --slaves 8`，容器**不带 --privileged / --network=host**；
# 宿主机通过共享卷中的 UDS 文件跑合成主站（`uds_master_probe.py --multi-slave=8`）。
#
# 前置：podman（root 或 rootless 均可）、已 `cargo build --release -p esc-node`、python3。
# 无 registry 网络的机器可设 BASE_IMAGE 指向本地缓存的 openeuler/debian 镜像。
#
# 用法：./podman-unprivileged-probe.sh
set -euo pipefail

PODMAN="${PODMAN:-podman}"
IMAGE="${IMAGE:-ecsim-uds:prebuilt}"
BASE_IMAGE="${BASE_IMAGE:-docker.io/openeuler/openeuler:24.03-lts-sp4}"
BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
N="${N:-8}"
WORK="${WORK:-/tmp/ecsim-podman}"
CONTAINER="ecsim-uds-probe-$$"
LOG=/tmp/ec-sim-podman.log

fail() {
  echo "FAIL: $1"
  exit 1
}

command -v "$PODMAN" >/dev/null 2>&1 || { echo "SKIP: 未安装 podman"; exit 2; }
[ -x "$BIN" ] || fail "找不到已构建的 $BIN（先 cargo build --release -p esc-node）"

cleanup() {
  "$PODMAN" rm -f "$CONTAINER" >/dev/null 2>&1 || true
  rm -rf "$WORK" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
mkdir -p "$WORK"
"$PODMAN" build -f docker/Dockerfile.prebuilt --build-arg "BASE_IMAGE=$BASE_IMAGE" \
  -t "$IMAGE" . >"$LOG" 2>&1 || {
  tail -n 5 "$LOG" || true
  fail "podman build 失败（见 $LOG）"
}

"$PODMAN" run -d --name "$CONTAINER" --rm \
  --security-opt label=disable \
  -v "$WORK:$WORK" \
  "$IMAGE" --uds "$WORK/ecsim.sock" --slaves "$N" >>"$LOG" 2>&1 || fail "podman run 失败（见 $LOG）"

for _ in $(seq 1 100); do
  [ -S "$WORK/ecsim.sock" ] && break
  sleep 0.1
done
[ -S "$WORK/ecsim.sock" ] || {
  "$PODMAN" logs "$CONTAINER" 2>/dev/null | tail -n 5 || true
  fail "容器内未创建 UDS socket（$WORK/ecsim.sock）"
}
echo "OK: 容器内启动 ec-sim-esc（无 --privileged），UDS socket 已就绪"

"$PY" scripts/spikes/uds_master_probe.py "$WORK/ecsim.sock" --multi-slave="$N" \
  || fail "容器内从站的合成主站预回归失败"

"$PODMAN" stop "$CONTAINER" >/dev/null 2>&1 || true
echo "OK: 无特权容器预回归通过（$N 从站，UDS shim，podman 无 --privileged/--network=host）"
