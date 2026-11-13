#!/usr/bin/env bash
# task-41 / B60（GAP-14 回归防护）：**端到端 PDO 字节序探针**。
#
# 目标：把"`cnc-csp-servo` 的 TxPDO 顺序 = lcec `basic_cia402` 读方向"从**单测**升级到
# **端到端探针**（合成主站经 UDS 写 SM2 → 读 SM3 原始字节 → 按 manifest 顺序断言）。
#
# 断言：
#   1. manifest `plugins/cnc-csp-servo/plugin.yaml` 的 TxPDO（0x1A00）条目顺序 =
#      [0x6041 U16, 0x6064 S32, 0x606C S32, 0x6061 S8]，且 SM3（dir: in）长度 = 11；
#   2. 合成主站写 SM2（controlword 0x000F / mode 8 / target 1000）后，SM3 原始字节 =
#      **`37 06 e8 03 00 00 00 00 00 00 08`**（statusword 0x0637 | position 1000 | velocity 0 | mode 8）；
#   3. **负例判别**：按旧顺序 `[6041,6061,6064,606C]` 解读时 position 偏移为 3..7（≠1000）、
#      mode 在 offset2（≠8）——即本探针能区分新旧顺序（旧顺序代码下必须 FAIL）。
#
# 前置：Linux（UDS 合成主站，无需 root/TAP）；`cargo build --release -p esc-node`。
# 用法：./pdo-byteorder-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
SOCK="${SOCK:-/tmp/ecsim-pdo-byteorder.sock}"
MANIFEST="${MANIFEST:-plugins/cnc-csp-servo/plugin.yaml}"
LOG_DIR="${LOG_DIR:-/tmp/ecsim-pdo-byteorder}"
mkdir -p "$LOG_DIR"

os="$(uname -s 2>/dev/null || echo unknown)"
case "$os" in
  Linux*) ;;
  *)
    echo "SKIP: 需要 Linux（UDS 合成主站），当前 $os"
    exit 2
    ;;
esac
[ -x "$BIN" ] || { echo "FAIL: 找不到 $BIN"; exit 1; }
[ -f "$MANIFEST" ] || { echo "FAIL: 找不到 manifest $MANIFEST"; exit 1; }
command -v "$PY" >/dev/null 2>&1 || { echo "FAIL: 找不到 Python：$PY"; exit 1; }

rm -f "$SOCK"
"$BIN" --uds "$SOCK" --manifest "$MANIFEST" --behavior cia402 >"$LOG_DIR/sim.log" 2>&1 &
SIM_PID=$!
cleanup() {
  kill "$SIM_PID" 2>/dev/null || true
  wait "$SIM_PID" 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT
for _ in $(seq 1 100); do [ -S "$SOCK" ] && break; sleep 0.05; done
[ -S "$SOCK" ] || { cat "$LOG_DIR/sim.log"; echo "FAIL: 模拟器未创建 UDS socket"; exit 1; }
grep -q "BEHAVIOR cia402" "$LOG_DIR/sim.log" \
  || { cat "$LOG_DIR/sim.log"; echo "FAIL: 启动日志缺少 BEHAVIOR cia402 行"; exit 1; }
grep -q "tx_layout=LcecCsp" "$LOG_DIR/sim.log" \
  || { cat "$LOG_DIR/sim.log"; echo "FAIL: 未选择 LcecCsp 布局（manifest Tx 顺序不符 GAP-14）"; exit 1; }

"$PY" - "$SOCK" "$MANIFEST" <<'PY'
import re
import struct
import sys

sys.path.insert(0, "scripts/spikes")
from uds_master_probe import CMD, SyntheticMaster  # noqa: E402

sock_path, manifest_path = sys.argv[1], sys.argv[2]

# 1) manifest 顺序/长度断言（零依赖：按文本抽取 tx 条目与 in SM 长度）。
text = open(manifest_path, encoding="utf-8").read()
tx_block = text.split("tx:", 1)[1] if "tx:" in text else ""
# 只取映射条目行（`{index: 0x...., sub: N, bits: M}`），排除 `- index: 0x1A00` 这行 PDO 头。
tx_indices = [
    int(value, 16)
    for value in re.findall(r"\{index:\s*0x([0-9A-Fa-f]{4}),\s*sub:", tx_block)
]
expected_order = [0x6041, 0x6064, 0x606C, 0x6061]
assert tx_indices[:4] == expected_order, (
    "manifest TxPDO 顺序 %s != lcec 读方向 %s" % ([hex(i) for i in tx_indices[:4]], [hex(i) for i in expected_order])
)
in_len = None
for block in re.findall(r"\{[^}]*dir:\s*in[^}]*\}", text):
    match = re.search(r"len:\s*(\d+)", block)
    if match:
        in_len = int(match.group(1))
assert in_len == 11, "SM3（dir: in）长度应为 11B，实际 %r" % (in_len,)

# 2) 合成主站：写 SM2 → 读 SM3 原始字节。
master = SyntheticMaster(sock_path)
try:
    def write_rx(controlword, mode, target):
        data = struct.pack("<HBi", controlword, mode & 0xFF, target)
        reply = master.transact([(CMD["APWR"], 0, 0x1100, data)])
        assert reply[0][4] == 1, "APWR SM2 WKC %d != 1" % reply[0][4]

    def read_tx():
        reply = master.transact([(CMD["APRD"], 0, 0x1110, b"\x00" * 11)])
        assert reply[0][4] == 1, "APRD SM3 WKC %d != 1" % reply[0][4]
        return reply[0][3]

    write_rx(0x0006, 8, 1000)  # Shutdown → Ready
    write_rx(0x0007, 8, 1000)  # Switch on
    write_rx(0x000F, 8, 1000)  # Enable operation（CSP：位置跟随 target）
    tx = b""
    deadline = 20
    for _ in range(deadline):
        # 行为在每帧 tick 时生效：多发几帧确保 controlword 已生效。
        tx = read_tx()
        if tx == bytes.fromhex("3706e80300000000000008"):
            break
    expected = bytes.fromhex("3706e80300000000000008")
    assert tx == expected, "SM3 字节序列 %s != %s" % (tx.hex(), expected.hex())

    # 3) 负例判别（旧顺序会 FAIL）：旧布局把 mode 放 offset2、position 放 3..7。
    statusword = struct.unpack("<H", tx[0:2])[0]
    position_new = struct.unpack("<i", tx[2:6])[0]
    velocity_new = struct.unpack("<i", tx[6:10])[0]
    mode_new = tx[10]
    assert statusword == 0x0637, "statusword %#06x != 0x0637" % statusword
    assert position_new == 1000, "position(LcecCsp 偏移 2..6) %d != 1000" % position_new
    assert velocity_new == 0, "velocity %d != 0" % velocity_new
    assert mode_new == 8, "mode display %d != 8" % mode_new
    # 旧顺序：mode 在 offset2、position 在 3..7 → 同一字节序列下这两个断言都会失败。
    assert tx[2] != 8, "旧顺序判别失败：offset2 恰为 8（新旧顺序无法区分）"
    assert struct.unpack("<i", tx[3:7])[0] != 1000, "旧顺序判别失败：offset3..7 恰为 1000"
finally:
    master.close()

print("OK: TxPDO 字节序端到端通过 —— SM3=%s（statusword=0x0637, pos=1000, vel=0, mode=8）" % tx.hex())
print("OK: manifest Tx 顺序 [0x6041,0x6064,0x606C,0x6061]、SM3 11B；旧顺序判别成立（offset2/3..7 不匹配）")
PY

echo "OK: GAP-14 PDO 字节序探针通过（cnc-csp-servo / LcecCsp）"
