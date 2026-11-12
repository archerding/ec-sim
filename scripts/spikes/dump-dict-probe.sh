#!/usr/bin/env bash
# B19 预回归：`ec-sim-esc --dump-dict --json` 输出必须是合法 dict-v1 JSON，且关键对象
# 与 manifest 一致（0x6502 支持模式、0x6040 控制字）；同时校验旧的纯文本 dump 仍可用。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./dump-dict-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
OUT=/tmp/ec-sim-dict

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}

check_profile() {
  local manifest="$1"
  local expect_6502="$2"
  local json="$3"
  "$BIN" --dump-dict --json --manifest "$manifest" >"$json" 2>/dev/null || {
    echo "FAIL: --dump-dict --json 失败（$manifest）"
    exit 1
  }
  "$PY" - "$json" "$expect_6502" <<'PY'
import json
import sys

dict_data = json.load(open(sys.argv[1], encoding="utf-8"))
expect_6502 = sys.argv[2]
assert dict_data["schema"] == "ec-sim/dict-v1", dict_data["schema"]
assert dict_data["object_count"] > 0
assert dict_data["entry_count"] > 0
objects = {obj["index"]: obj for obj in dict_data["objects"]}
assert "0x6502" in objects, "缺少 0x6502"
mode = objects["0x6502"]["entries"][0]
assert mode["value_hex"] == expect_6502, (mode["value_hex"], expect_6502)
control = objects["0x6040"]["entries"][0]
assert control["access"] == "rw" and control["bits"] == 16, control
print("OK: dict-v1（%s 对象 / %s 条目，0x6502=%s、0x6040=rw/16）" % (
    dict_data["object_count"], dict_data["entry_count"], mode["value_hex"]))
PY
}

check_profile plugins/generic-cia402-cst-servo/plugin.yaml 10040000 "$OUT-cst.json"   # 0x0410 小端
check_profile plugins/inovance-sv660n/plugin.yaml ad030000 "$OUT-sv660n.json"          # 0x3AD 小端

# 纯文本 dump 仍工作（向后兼容）。
"$BIN" --dump-dict --manifest plugins/generic-cia402-cst-servo/plugin.yaml >"$OUT-text.log" 2>&1
grep -q "dictionary dump: objects=" "$OUT-text.log" || {
  echo "FAIL: 纯文本 --dump-dict 输出缺失"
  exit 1
}

echo "OK: --dump-dict 预回归通过（dict-v1 JSON 2 个画像 + 纯文本向后兼容）"
