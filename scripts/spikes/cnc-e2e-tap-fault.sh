#!/usr/bin/env bash
# CNC E2E TAP down/up 故障（task-102 P0）：真实链路断开 → 主站丢站 → 恢复重连。
#
# 前置（EC-Sim 自己的验证机，root；**不要**用 CNC 的验证机）：
#   - EC-Sim 二进制：EC_SIM_BIN（默认 target/release/ec-sim-esc）
#   - SOEM TAP 构建：/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng
#     （可用 SOEM_SIMPLE_NG 覆盖）
#
# 用法： sudo ./scripts/spikes/cnc-e2e-tap-fault.sh
#        OUT_DIR=/tmp/tap-fault sudo -E ./scripts/spikes/cnc-e2e-tap-fault.sh
#
# 断言：
#   1. 3×cnc-csp-servo 经 SOEM simple_ng 到 3/3 OP；
#   2. `ip link set <tap> down` 后：控制面仍 200，三个从站 frames_in 全部停止增长；
#      主站侧日志/退出码记录（可能报 lost/timeout，不强制）；
#   3. `ip link set <tap> up` 后第二个 simple_ng 重新完成 ESM 配置并打印
#      `all slaves are now operational`，/state 回到 3/3 OP、frames_in 恢复增长。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${EC_SIM_BIN:-$ROOT/target/release/ec-sim-esc}"
MANIFEST="${MANIFEST:-$ROOT/plugins/cnc-csp-servo/plugin.yaml}"
SOEM_SIMPLE_NG="${SOEM_SIMPLE_NG:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
TAP="${TAP:-ecsimfault0}"
PORT="${PORT:-18135}"
OUT_DIR="${OUT_DIR:-$ROOT/docs/verification/assets/cnc-e2e/tap-fault}"
RUN_ID="${RUN_ID:-tap-fault-$(date +%Y%m%d%H%M%S)}"
PY="${PY:-python3}"
URL="http://127.0.0.1:$PORT/state"

if [ "$(id -u)" -ne 0 ]; then
  echo "SKIP: TAP down/up 需要 root"
  exit 2
fi
[ "$(uname -s 2>/dev/null)" = "Linux" ] || { echo "SKIP: 需要 Linux TAP"; exit 2; }
[ -x "$SOEM_SIMPLE_NG" ] || { echo "SKIP: 缺少 SOEM simple_ng（$SOEM_SIMPLE_NG）"; exit 2; }
[ -x "$BIN" ] || { echo "SKIP: 缺少 EC_SIM_BIN（$BIN）"; exit 2; }
[ -f "$MANIFEST" ] || { echo "SKIP: manifest 不存在 $MANIFEST"; exit 2; }
mkdir -p "$OUT_DIR"

FAIL=0
note() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$OUT_DIR/timeline.log"; }
fail() { echo "FAIL: $*"; FAIL=1; }

cleanup() {
  [ -n "${MASTER2_PID:-}" ] && kill "$MASTER2_PID" 2>/dev/null || true
  [ -n "${MASTER1_PID:-}" ] && kill "$MASTER1_PID" 2>/dev/null || true
  if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  ip link set "$TAP" down 2>/dev/null || true
  ip link del "$TAP" 2>/dev/null || true
}
trap cleanup EXIT

op_wait() {
  # $1=timeout 秒；成功（3/3 OP）返回 0 并写 /state 到 $2。
  "$PY" - "$URL" "$1" "$2" <<'PY'
import json, sys, time, urllib.request
url, timeout, out = sys.argv[1], float(sys.argv[2]), sys.argv[3]
deadline = time.time() + timeout
last = None
while time.time() < deadline:
    try:
        with urllib.request.urlopen(url, timeout=1.5) as response:
            last = json.loads(response.read().decode("utf-8"))
        master = last.get("master") or {}
        if master.get("op_slaves") == master.get("slave_count") == 3:
            if out:
                with open(out, "w", encoding="utf-8") as handle:
                    json.dump(last, handle, ensure_ascii=False, indent=2)
            sys.exit(0)
    except Exception:
        pass
    time.sleep(0.1)
sys.stderr.write(f"last state={json.dumps(last, ensure_ascii=False)[:400]}\n")
sys.exit(1)
PY
}

frames_of() {
  # 读取 $1 文件，输出 index:frames_in（空格分隔）。
  "$PY" - "$1" <<'PY'
import json, sys
state = json.load(open(sys.argv[1], encoding="utf-8"))
print(" ".join(f"{entry['index']}:{entry['frames_in']}" for entry in (state.get("slaves") or [])))
PY
}

ip link del "$TAP" 2>/dev/null || true
note "启动 ec-sim-esc --tap $TAP（3×cnc-csp-servo，run_id=$RUN_ID）"
"$BIN" --tap "$TAP" --slaves 3 --manifest "$MANIFEST" --behavior cia402 \
  --serve "127.0.0.1:$PORT" --run-id "$RUN_ID" >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!
ready=0
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then ready=1; break; fi
  kill -0 "$SERVER_PID" 2>/dev/null || break
  sleep 0.1
done
[ "$ready" -eq 1 ] || { cat "$OUT_DIR/server.log"; echo "FAIL: 控制面未就绪"; exit 1; }
ip link set "$TAP" up || { echo "FAIL: ip link set $TAP up"; exit 1; }
note "TAP $TAP up：$(ip -details link show "$TAP" | head -n 1)"
ip -details link show "$TAP" >"$OUT_DIR/ip-link-up.txt" 2>&1

note "主站 #1（simple_ng）建链"
timeout 30 "$SOEM_SIMPLE_NG" "$TAP" >"$OUT_DIR/master1.log" 2>&1 &
MASTER1_PID=$!
if op_wait 8 "$OUT_DIR/state-before-down.json"; then
  echo "OK: 断开前 3/3 OP"
  note "断开前 frames_in=$(frames_of "$OUT_DIR/state-before-down.json")"
else
  fail "断开前未达到 3/3 OP"
  tail -n 20 "$OUT_DIR/master1.log" || true
fi

note "ip link set $TAP down（模拟链路中断）"
ip link set "$TAP" down
ip -details link show "$TAP" >"$OUT_DIR/ip-link-down.txt" 2>&1
sleep 0.6
curl -fsS "$URL" -o "$OUT_DIR/state-during-down-1.json" || fail "TAP down 期间控制面不可达"
sleep 0.6
curl -fsS "$URL" -o "$OUT_DIR/state-during-down-2.json" || fail "TAP down 期间控制面不可达"
if [ -s "$OUT_DIR/state-during-down-1.json" ] && [ -s "$OUT_DIR/state-during-down-2.json" ]; then
  f1="$(frames_of "$OUT_DIR/state-during-down-1.json")"
  f2="$(frames_of "$OUT_DIR/state-during-down-2.json")"
  note "down 期间 frames_in：$f1 -> $f2"
  if [ "$f1" = "$f2" ]; then
    echo "OK: TAP down 后全部从站 frames_in 停止增长（$f1）"
  else
    fail "TAP down 后 frames_in 仍在增长：$f1 -> $f2"
  fi
fi
# 主站 #1 的观察：允许仍在跑（timeout 124）或报错退出；记录即可。
sleep 2
if kill -0 "$MASTER1_PID" 2>/dev/null; then
  note "主站 #1 在链路断开 2s 后仍存活（开始链路恢复）"
  kill "$MASTER1_PID" 2>/dev/null || true
  wait "$MASTER1_PID" 2>/dev/null || true
else
  wait "$MASTER1_PID" 2>/dev/null
  note "主站 #1 已退出（exit=$?，链路断开被主站观测到）"
fi
grep -iE "lost|timeout|error|WKC|not.*operational" "$OUT_DIR/master1.log" >"$OUT_DIR/master1-fault-lines.log" 2>/dev/null || true
note "主站 #1 故障相关行数：$(wc -l <"$OUT_DIR/master1-fault-lines.log" 2>/dev/null || echo 0)"

note "ip link set $TAP up（链路恢复）"
ip link set "$TAP" up
sleep 1

note "主站 #2（simple_ng）重新建链"
timeout 20 "$SOEM_SIMPLE_NG" "$TAP" >"$OUT_DIR/master2.log" 2>&1 &
MASTER2_PID=$!
RECONNECTED=0
for _ in $(seq 1 120); do
  if grep -q "all slaves are now operational" "$OUT_DIR/master2.log" 2>/dev/null; then
    RECONNECTED=1
    break
  fi
  kill -0 "$MASTER2_PID" 2>/dev/null || break
  sleep 0.1
done
if [ "$RECONNECTED" -eq 1 ]; then
  echo "OK: 主站 #2 重新完成 ESM 配置（all slaves are now operational）"
else
  fail "主站 #2 未在 12s 内重新 operational"
  tail -n 30 "$OUT_DIR/master2.log" || true
fi
if op_wait 5 "$OUT_DIR/state-after-up.json"; then
  echo "OK: 恢复后 3/3 OP"
else
  fail "恢复后未达到 3/3 OP"
fi
sleep 0.5
curl -fsS "$URL" -o "$OUT_DIR/state-after-up-2.json" 2>/dev/null || true
if [ -s "$OUT_DIR/state-after-up.json" ] && [ -s "$OUT_DIR/state-after-up-2.json" ]; then
  a="$(frames_of "$OUT_DIR/state-after-up.json")"
  b="$(frames_of "$OUT_DIR/state-after-up-2.json")"
  note "恢复后 frames_in：$a -> $b"
  if [ "$a" != "$b" ]; then
    echo "OK: 链路恢复后 frames_in 恢复增长"
  else
    fail "链路恢复后 frames_in 未恢复：$a -> $b"
  fi
fi
kill "$MASTER2_PID" 2>/dev/null || true
wait "$MASTER2_PID" 2>/dev/null || true

echo "RESULT: $([ "$FAIL" -eq 0 ] && echo PASS || echo FAIL)"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
