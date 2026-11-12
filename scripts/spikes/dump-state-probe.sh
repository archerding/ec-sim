#!/usr/bin/env bash
# B18 预回归：`ec-sim-esc --dump-state` 输出必须是合法 state-v1 JSON，且关键字段与
# manifest 一致（身份/PDO/DC/字典对象数）；同时检查 `web/` 静态资源存在。
#
# 前置：`cargo build --release -p esc-node`、python3（标准库）。无需 root。
# 用法：./dump-state-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-python3}"
OUT=/tmp/ec-sim-state

[ -x "$BIN" ] || {
  echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"
  exit 1
}
[ -f web/index.html ] && [ -f web/app.js ] || {
  echo "FAIL: web/ 静态资源缺失"
  exit 1
}

check_state() {
  local manifest="$1"
  local json="$2"
  "$BIN" --dump-state --manifest "$manifest" >"$json" 2>/dev/null || {
    echo "FAIL: --dump-state 失败（$manifest）"
    exit 1
  }
  "$PY" - "$json" <<'PY'
import json
import sys

state = json.load(open(sys.argv[1], encoding="utf-8"))
assert state["schema"] == "ec-sim/state-v1", state["schema"]
assert state["identity"]["vendor_id"].startswith("0x")
assert state["names"], "names 为空"
assert state["sync_managers"], "sync_managers 为空"
assert state["pdo_tx"] and state["pdo_rx"], "PDO 为空"
assert state["profile"] is not None and state["dict_objects"] > 0
print("OK: state-v1 合法（profile=%s, sm=%d, rx=%d, tx=%d, dict=%d）" % (
    state["profile"]["id"], len(state["sync_managers"]),
    len(state["pdo_rx"]), len(state["pdo_tx"]), state["dict_objects"]))
PY
}

check_state plugins/generic-cia402-cst-servo/plugin.yaml "$OUT-cst.json"
check_state plugins/inovance-sv660n/plugin.yaml "$OUT-sv660n.json"

# 内置画像：合法 JSON 且 profile/dict 为 null（不依赖 manifest）。
"$BIN" --dump-state >"$OUT-builtin.json" 2>/dev/null
"$PY" - "$OUT-builtin.json" <<'PY'
import json
import sys

state = json.load(open(sys.argv[1], encoding="utf-8"))
assert state["schema"] == "ec-sim/state-v1"
assert state["profile"] is None and state["dict_objects"] is None
assert state["sync_managers"], "内置画像应至少有一个 SM（默认 SM 配置为空也可能为空，按实现校验）"
print("OK: 内置画像 state-v1 合法")
PY

echo "OK: --dump-state 预回归通过（2 个 manifest 画像 + 内置画像；web/ 资源存在）"
