#!/usr/bin/env bash
# SDO 上传预回归（B7 子项 2）：**两家未修改主站**读取本仓库模拟器的对象字典。
#
# 前置（Linux 验证机上，环境相关路径见本地 .local/ops.md，不入库）：
#   - 已构建本仓库：`cargo build --release -p esc-node`
#   - SOEM 的 slaveinfo、IgH 的 ethercat 工具与 ec_master/ec_generic 模块已安装
#   - 以 root 运行（TAP + 模块加载需要）
#
# 断言：
#   1. SOEM `slaveinfo <tap> -map` 打印 "PDO mapping according to CoE"，且包含
#      `0x7000:0x00 0x08`（SM2 输出）与 `0x6000:0x00 0x08`（SM3 输入）；
#   2. IgH `ethercat slaves` 看到 PREOP 从站；
#   3. IgH `ethercat upload` 读到 0x1C12:01 = 0x1600、0x1600:01 = 0x70000008、0x1C00:00 = 0x04。
#
# 用法：sudo TAP=escsim0 MAC=02:ec:51:4d:00:01 ./sdo-upload-probe.sh
set -euo pipefail

TAP="${TAP:-escsim0}"
MAC="${MAC:-02:ec:51:4d:00:01}"
BIN="${BIN:-target/release/ec-sim-esc}"
SOEM_SLAVEINFO="${SOEM_SLAVEINFO:-/opt/ecsim/fixtures/soem/build/samples/slaveinfo/slaveinfo}"
IGH_ETHERCAT="${IGH_ETHERCAT:-/opt/ecsim/igh/bin/ethercat}"
LOG=/tmp/ec-sim-esc-sdo.log
SOEM_OUT=/tmp/sdo-soem-map.out
SOEM_LINES=/tmp/sdo-soem-map.lines

cleanup() {
  killall ec-sim-esc 2>/dev/null || true
  rmmod ec_generic 2>/dev/null || true
  rmmod ec_master 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
"$BIN" --tap "$TAP" --verbose >"$LOG" 2>&1 &
sleep 0.6
ip link set "$TAP" address "$MAC"
ip link set "$TAP" up
sleep 0.3

# 1) SOEM：CoE PDO 映射（SDO upload 0x1C00/0x1C12/0x1C13/0x1600/0x1A00）。
timeout 60 "$SOEM_SLAVEINFO" "$TAP" -map >"$SOEM_OUT" 2>&1 || true
tr '\r' '\n' <"$SOEM_OUT" >"$SOEM_LINES"
grep -q "PDO mapping according to CoE" "$SOEM_LINES" \
  || { echo "FAIL: SOEM 未打印 CoE 映射段"; exit 1; }
grep -q "0x7000:0x00 0x08" "$SOEM_LINES" \
  || { echo "FAIL: SOEM 未读到输出 PDO 映射 0x7000:00（8 bit）"; exit 1; }
grep -q "0x6000:0x00 0x08" "$SOEM_LINES" \
  || { echo "FAIL: SOEM 未读到输入 PDO 映射 0x6000:00（8 bit）"; exit 1; }

# 2) IgH：SDO upload（主站工具在 idle 状态直接读对象字典，无需应用）。
modprobe ec_master main_devices="$MAC"
modprobe ec_generic
sleep 2
"$IGH_ETHERCAT" slaves >/tmp/sdo-igh-slaves.out 2>&1 || true
grep -q "PREOP" /tmp/sdo-igh-slaves.out \
  || { echo "FAIL: IgH 未看到 PREOP 从站"; cat /tmp/sdo-igh-slaves.out; exit 1; }

OUT_1C12=$("$IGH_ETHERCAT" upload -p 0 -t uint16 0x1c12 0x01 2>&1 | awk '{print $1}')
[ "$OUT_1C12" = "0x1600" ] \
  || { echo "FAIL: IgH upload 0x1C12:01 = '$OUT_1C12'（预期 0x1600）"; exit 1; }
OUT_1600=$("$IGH_ETHERCAT" upload -p 0 -t uint32 0x1600 0x01 2>&1 | awk '{print $1}')
[ "$OUT_1600" = "0x70000008" ] \
  || { echo "FAIL: IgH upload 0x1600:01 = '$OUT_1600'（预期 0x70000008）"; exit 1; }
OUT_1C00=$("$IGH_ETHERCAT" upload -p 0 -t uint8 0x1c00 0x00 2>&1 | awk '{print $1}')
[ "$OUT_1C00" = "0x04" ] \
  || { echo "FAIL: IgH upload 0x1C00:00 = '$OUT_1C00'（预期 0x04）"; exit 1; }

echo "OK: SOEM CoE 映射 0x7000/0x6000（8bit）；IgH SDO upload 0x1C12:01=0x1600、0x1600:01=0x70000008、0x1C00:00=0x04"
