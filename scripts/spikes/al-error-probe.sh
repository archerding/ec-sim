#!/usr/bin/env bash
# task-34（AC-E-203 支持）：`--al-error <code>` AL Status 错误注入预回归。
#
# 语义（实现见 `esc-node::Responder::force_al_error`）：
#   - 启动即把本进程模拟的**全部**从站置 AL Status error 位 + AL Status Code（写 0x0130/0x0134），
#     **不改变**当前 AL 状态；
#   - 主站仍可按 ESM 语义用 AL Control 的 Error ACK 清错；
#   - 只接受已知 AL Status Code（`esc_core::al_status::is_known`）；未知码/0 → **拒启动（exit 2）**；
#   - 默认关闭，不影响既有双主站/探针行为。
#
# 断言：
#   1. 任何平台：`ec-sim-esc --al-error 0x00FF` 拒启动（exit 2，stderr 给出可用码）——参数校验；
#   2. Linux（UDS 实时 + 只读控制面）：`--al-error 0x0024` 启动后
#      `/state.live.al_status == "0x11"`（INIT 0x01 | error 0x10）、`al_status_code == "0x0024"`；
#      `/metrics` 中 `ecsim_al_status{...} 17`、`ecsim_al_status_code{...} 36`（十进制）。
#
# 前置：Linux（UDS），`cargo build --release -p esc-node`；无需 root、无 TAP。
# 用法：./al-error-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
SOCK="${SOCK:-/tmp/ecsim-al-error.sock}"
PORT="${PORT:-18131}"
CODE="${CODE:-0x0024}"
LOG_DIR="${LOG_DIR:-/tmp/ecsim-al-error}"
mkdir -p "$LOG_DIR"

os="$(uname -s 2>/dev/null || echo unknown)"
is_linux="no"
[ "${os#Linux}" != "$os" ] && is_linux="yes"
[ -x "$BIN" ] || { echo "FAIL: 找不到 $BIN"; exit 1; }
command -v "$PY" >/dev/null 2>&1 || { echo "FAIL: 找不到 Python：$PY"; exit 1; }

fail() {
  echo "FAIL: $1"
  exit 1
}

# 1) 参数校验（跨平台）：非法码必须拒启动。
set +e
out="$("$BIN" --al-error 0x00FF 2>&1)"
code=$?
set -e
[ "$code" -eq 2 ] || {
  echo "$out" | tail -n 3
  fail "非法 --al-error 0x00FF 应 exit 2，实际 $code"
}
echo "$out" | grep -q "拒绝未知" || fail "非法码提示缺少原因：$out"
echo "OK: 非法 AL Status Code 拒启动（exit 2）"

if [ "$is_linux" != "yes" ]; then
  echo "SKIP: 实时注入观测段需要 Linux（UDS + 控制面，当前 $os）"
  exit 2
fi

rm -f "$SOCK"
"$BIN" --uds "$SOCK" --al-error "$CODE" --serve "127.0.0.1:$PORT" \
  >"$LOG_DIR/sim.log" 2>&1 &
SIM_PID=$!
cleanup() {
  kill "$SIM_PID" 2>/dev/null || true
  wait "$SIM_PID" 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 100); do [ -S "$SOCK" ] && break; sleep 0.05; done
[ -S "$SOCK" ] || { cat "$LOG_DIR/sim.log"; fail "模拟器未创建 UDS socket"; }
grep -q "AL_ERROR inject code=0x$(
  echo "${CODE#0x}" | tr 'a-f' 'A-F'
)" "$LOG_DIR/sim.log" || {
  # 大小写不敏感兜底
  grep -qi "AL_ERROR inject code=$CODE" "$LOG_DIR/sim.log" || {
    cat "$LOG_DIR/sim.log"
    fail "启动日志缺少 AL_ERROR inject 行"
  }
}

"$PY" - "127.0.0.1:$PORT" "$CODE" <<'PY'
import json
import sys
import time
import urllib.request

base = "http://" + sys.argv[1]
code = sys.argv[2]
expected_code = int(code, 16)


def get(path, raw=False):
    with urllib.request.urlopen(base + path, timeout=3) as response:
        body = response.read().decode("utf-8")
        return body if raw else json.loads(body)


deadline = time.time() + 5.0
last_error = None
while time.time() < deadline:
    try:
        state = get("/state")
        break
    except Exception as error:  # noqa: BLE001 - 探针自愈：等服务就绪
        last_error = error
        time.sleep(0.1)
else:
    raise SystemExit(f"控制面未就绪：{last_error}")

live = state.get("live")
assert live is not None, f"期望实时 /state 含 live：{state}"
assert live["al_status"] == "0x11", f"AL Status 应为 INIT(0x01)|error(0x10)=0x11：{live}"
assert int(live["al_status_code"], 16) == expected_code, (
    f"AL Status Code 应为 {code}：{live}"
)

metrics = get("/metrics", raw=True)
value = expected_code
assert f"ecsim_al_status{{" in metrics, metrics
assert " 17" in metrics, f"ecsim_al_status 应为 17（0x11）：{metrics}"
assert f"ecsim_al_status_code{{" in metrics and f"}} {value}" in metrics, (
    f"ecsim_al_status_code 应为 {value}（{code}）：{metrics}"
)
print(
    "OK: 实时注入可见 — /state.live.al_status=0x11、al_status_code=%s；/metrics al_status=17、al_status_code=%d"
    % (code, value)
)
PY

echo "OK: --al-error 注入预回归通过（code=$CODE；默认关闭、非法码拒启动）"
