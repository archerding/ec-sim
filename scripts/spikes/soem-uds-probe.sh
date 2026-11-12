#!/usr/bin/env bash
# SOEM oshw UDS shim 预回归（B3）：SOEM 的收发包换成 UDS 后，经 Unix socket 与
# `ec-sim-esc --uds` 通信，跑通 `ec_config_init` + `ec_config_map` 并进入 OP。
#
# 前置（Linux 验证机）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - 验证机本地已构建 **SOEM UDS shim**（GPL 夹具，源码/补丁**不入库**）：
#       1) cp -a /opt/ecsim/fixtures/soem /opt/ecsim/work/soem-uds
#       2) 用本地补丁脚本把 oshw/linux/nicdrv.c 的 PF_PACKET 初始化替换为 AF_UNIX
#          SOCK_DGRAM 客户端（bind 自己的路径并 connect 到 --uds 路径；send/recv/poll 复用）
#       3) cmake -S . -B build-uds -DCMAKE_BUILD_TYPE=Release && cmake --build build-uds -j
#       （补丁要点见 docs/spikes/spike-5-esc-over-uds.md §4 与 WORKLOG 20:03 班）
#   - 无需 root（Unix domain socket）
#
# 断言：
#   1. UDS-shim SOEM `slaveinfo <sock>`：1 从站、8 位输入/输出（SII 类别经 UDS 生效）；
#   2. UDS-shim SOEM `simple_ng <sock>`：`mapped 1O+1I` → OP、每轮 WKC=3、0 次 wrong；
#   3. 模拟器 `--verbose` 日志出现 `RX uds`（帧确实走 UDS）。
#
# 用法：SOCK=/tmp/ecsim-uds.sock ./soem-uds-probe.sh
set -euo pipefail

SOCK="${SOCK:-/tmp/ecsim-uds.sock}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_UDS_SLAVEINFO="${SOEM_UDS_SLAVEINFO:-/opt/ecsim/work/soem-uds/build-uds/samples/slaveinfo/slaveinfo}"
SOEM_UDS_SIMPLE_NG="${SOEM_UDS_SIMPLE_NG:-/opt/ecsim/work/soem-uds/build-uds/samples/simple_ng/simple_ng}"
LOG=/tmp/ec-sim-esc-soem-uds.log
SLAVEINFO_OUT=/tmp/soem-uds-slaveinfo.out
SIMPLE_NG_OUT=/tmp/soem-uds-simple-ng.out
SIMPLE_NG_LINES=/tmp/soem-uds-simple-ng.lines

for binary in "$SOEM_UDS_SLAVEINFO" "$SOEM_UDS_SIMPLE_NG"; do
  if [ ! -x "$binary" ]; then
    echo "FAIL: 未找到 SOEM UDS shim 二进制 $binary（验证机本地构建，源码不入库；见脚本头部）"
    exit 2
  fi
done

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rm -f "$SOCK" "$SOCK".client* /tmp/soem-uds-client-*.sock 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --uds "$SOCK" --verbose >"$LOG" 2>&1 &
sleep 0.6
for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
[ -S "$SOCK" ] || { echo "FAIL: 模拟器未创建 UDS socket $SOCK"; cat "$LOG"; exit 1; }

# 1) ec_config_init：扫描 + SII 读取 + 站地址分配。
timeout 30 "$SOEM_UDS_SLAVEINFO" "$SOCK" >"$SLAVEINFO_OUT" 2>&1 || true
tr '\r' '\n' <"$SLAVEINFO_OUT" >"$SLAVEINFO_OUT.lines"
grep -q "1 slaves found and configured" "$SLAVEINFO_OUT.lines" \
  || { echo "FAIL: UDS-shim SOEM 未扫描到 1 个从站"; cat "$SLAVEINFO_OUT.lines"; exit 1; }
grep -q "Output size: 8bits" "$SLAVEINFO_OUT.lines" \
  || { echo "FAIL: UDS 下输出映射不是 8 位"; exit 1; }
grep -q "Input size: 8bits" "$SLAVEINFO_OUT.lines" \
  || { echo "FAIL: UDS 下输入映射不是 8 位"; exit 1; }

# 2) ec_config_init + ec_config_map + 过程数据：进入 OP 且 WKC=3。
timeout 8 "$SOEM_UDS_SIMPLE_NG" "$SOCK" >"$SIMPLE_NG_OUT" 2>&1 || true
tr '\r' '\n' <"$SIMPLE_NG_OUT" >"$SIMPLE_NG_LINES"
grep -q "mapped 1O+1I bytes" "$SIMPLE_NG_LINES" \
  || { echo "FAIL: UDS 下 I/O 映射不是 1O+1I"; head -n 8 "$SIMPLE_NG_LINES"; exit 1; }
grep -q "all slaves are now operational" "$SIMPLE_NG_LINES" \
  || { echo "FAIL: UDS 下未进入 OP"; head -n 12 "$SIMPLE_NG_LINES"; exit 1; }
WRONG=$(grep -c "wrong" "$SIMPLE_NG_LINES" || true)
[ "$WRONG" -eq 0 ] || { echo "FAIL: $WRONG 轮 WKC 不符"; grep "wrong" "$SIMPLE_NG_LINES" | head -n 3; exit 1; }
WKC_ROUNDS=$(grep -c "WKC 3" "$SIMPLE_NG_LINES" || true)
[ "$WKC_ROUNDS" -ge 100 ] || { echo "FAIL: WKC=3 只有 $WKC_ROUNDS 轮"; exit 1; }

# 3) 帧确实走 UDS。
grep -q "RX uds" "$LOG" \
  || { echo "FAIL: 模拟器日志没有 UDS 收帧记录"; tail -n 5 "$LOG"; exit 1; }

echo "OK: SOEM UDS shim 经 $SOCK 跑通 ec_config_init + ec_config_map（1 从站 / 1O+1I / OP / $WKC_ROUNDS 轮 WKC=3 / 0 次 wrong）"
