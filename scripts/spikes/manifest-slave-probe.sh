#!/usr/bin/env bash
# B9 端到端预回归：声明式插件 manifest 画像（`ec-sim-esc --manifest`）在未修改主站上
# 被扫描到，并跑通 SII / 过程数据 / SDO Info。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - SOEM 的 slaveinfo / simple_ng、IgH 的 ethercat 工具与模块已安装
#   - 以 root 运行（TAP + 模块加载需要）
#
# 断言（任一失败即 exit 1）：
#   1. SOEM 扫描到 manifest 声明的身份（VID/PID/Revision）与 8 位 I/O；
#   2. SOEM `slaveinfo -sdo` 读到 manifest 的 7 个对象（含用户对象 0x2000 = 0x1234）；
#   3. SOEM `simple_ng` 进入 OP，过程数据 WKC=3 且无 WKC 不符；
#   4. IgH `ethercat slaves` 看到同一身份且拓扑计算无报错。
#
# 已知限制：SOEM `slaveinfo -sdo` 对非 VAR 对象的 subindex 0 用 2 字节缓冲区读取
# （`slaveinfo.c` 把 UINT8 响应的 1 字节拷进 uint16，高字节未初始化），**偶发段错误**，
# 属 SOEM 工具夹具自身缺陷；本脚本对 -sdo 做最多 3 次重试。
#
# 用法：sudo TAP=escsim0 ./manifest-slave-probe.sh
#      （MANIFEST 可指定其它 manifest，默认用仓库内的 8DO 样例）
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
MANIFEST="${MANIFEST:-crates/plugin-host/tests/fixtures/example-8do.yaml}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
IGH_ETHERCAT="${IGH_ETHERCAT:-/opt/ecsim/igh/bin/ethercat}"

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "FAIL: $1"
  exit 1
}

[ -f "$MANIFEST" ] || fail "找不到 manifest：$MANIFEST"

cleanup
"$BIN" --tap "$TAP" --verbose --manifest "$MANIFEST" >/tmp/ec-sim-manifest.log 2>&1 &
sleep 0.7
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

grep -q "MANIFEST id=com.ecsim.compat.example-8do" /tmp/ec-sim-manifest.log \
  || { echo "FAIL: 模拟器未按 manifest 启动"; cat /tmp/ec-sim-manifest.log; exit 1; }

echo "=== SOEM slaveinfo（身份 + SII PDO）==="
timeout 60 "$SOEM_SLAVEINFO" "$TAP" >/tmp/soem-manifest.out 2>&1 || true
tr '\r' '\n' </tmp/soem-manifest.out >/tmp/soem-manifest.lines
grep -E "Man:|Output size|Input size" /tmp/soem-manifest.lines | head -4 || true
grep -q "Man: 00000ec5 ID: 0000e5d1" /tmp/soem-manifest.lines || fail "身份不是 manifest 的 VID/PID"
grep -q "1 slaves found and configured" /tmp/soem-manifest.lines || fail "SOEM 未找到从站"
grep -q "Output size: 8bits" /tmp/soem-manifest.lines || fail "输出不是 8 位"
grep -q "Input size: 8bits" /tmp/soem-manifest.lines || fail "输入不是 8 位"

echo "=== SOEM slaveinfo -sdo（最多 3 次重试，见脚本头注释）==="
sdo_ok=0
for attempt in 1 2 3; do
  timeout 60 "$SOEM_SLAVEINFO" "$TAP" -sdo >"/tmp/soem-manifest-sdo.$attempt.out" 2>&1 || true
  tr '\r' '\n' <"/tmp/soem-manifest-sdo.$attempt.out" >/tmp/soem-manifest-sdo.lines
  if grep -q "CoE Object Description found, 7 entries." /tmp/soem-manifest-sdo.lines \
     && grep -q "Sample counter" /tmp/soem-manifest-sdo.lines \
     && grep -q "0x1234 / 4660" /tmp/soem-manifest-sdo.lines; then
    sdo_ok=1
    break
  fi
  sleep 0.3
done
[ "$sdo_ok" -eq 1 ] || fail "SDO Info 未读到 manifest 对象（3 次重试均失败）"
grep -E "0x1018|0x1c12|0x2000|Sample counter|0x1234" /tmp/soem-manifest-sdo.lines | head -8 || true

echo "=== SOEM simple_ng（process data / OP）==="
timeout 6 "$SOEM_SIMPLE_NG" "$TAP" >/tmp/soem-manifest-op.out 2>&1 || true
tr '\r' '\n' </tmp/soem-manifest-op.out >/tmp/soem-manifest-op.lines
grep -q "1 slaves found" /tmp/soem-manifest-op.lines || fail "simple_ng 未找到从站"
grep -q "mapped 1O+1I bytes" /tmp/soem-manifest-op.lines || fail "I/O 映射不是 1O+1I"
grep -q "all slaves are now operational" /tmp/soem-manifest-op.lines || fail "未进入 OP"
WKC_ROUNDS=$(grep -c "WKC 3" /tmp/soem-manifest-op.lines || true)
[ "$WKC_ROUNDS" -ge 100 ] || fail "WKC=3 轮次只有 $WKC_ROUNDS（预期 >=100）"
[ "$(grep -c "wrong" /tmp/soem-manifest-op.lines || true)" -eq 0 ] || fail "过程数据存在 WKC 不符"

echo "=== IgH 扫描 ==="
dmesg -C 2>/dev/null || true
modprobe ec_master main_devices="$MAC" 2>/dev/null
modprobe ec_generic 2>/dev/null
sleep 2
"$IGH_ETHERCAT" slaves
if dmesg | grep -q "Failed to calculate bus topology"; then
  fail "IgH 拓扑计算失败"
fi
"$IGH_ETHERCAT" slaves | grep -q "0x00000ec5:0x0000e5d1" || fail "IgH 未看到 manifest 的从站身份"

echo "OK: 声明式 manifest 画像端到端通过（SOEM：身份/SII-PDO/OP-WKC3/SDO Info 7 对象含 0x2000=0x1234；IgH：拓扑与身份）"
