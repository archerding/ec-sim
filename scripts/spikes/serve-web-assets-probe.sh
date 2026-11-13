#!/usr/bin/env bash
# G-18（P0）回归：`ec-sim-esc --serve` 产品路径下 Web 静态资源必须齐全。
#
# 背景：sim-ux en 走查发现 `--serve` 下 `/i18n.js` 404（static_web_response 白名单漏放行），
#       导致 window.ECSIM_I18N undefined、?lang=en/#lang-toggle 静默失效。本探针：
#   1. 静态模式：GET / 200；解析 index.html 的 `<script src>`/`<link href>`，逐个断言 200 且 JS 为
#      javascript Content-Type；
#   2. 断言 `/i18n.js` 200 + 内容含 `ECSIM_I18N`（防止再次漏放行）；
#   3. 断言未知资源 404（白名单语义，不误吞不存在文件）；
#   4. Linux 下再用 `--uds` 实时同源起一份，重复 1–3（实时 serve 与静态 serve 共用 static_web_response）。
#
# 前置：`cargo build --release -p esc-node`；python3（标准库）。无需 root。
# 用法：BIN=target/release/ec-sim-esc ./serve-web-assets-probe.sh
set -euo pipefail

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
PORT="${PORT:-18180}"
MANIFEST="${MANIFEST:-plugins/cnc-csp-servo/plugin.yaml}"
WEBROOT="${WEBROOT:-web}"
LOG_DIR="${LOG_DIR:-/tmp/ecsim-serve-assets}"
mkdir -p "$LOG_DIR"

[ -x "$BIN" ] || { echo "FAIL: 找不到 $BIN（先 cargo build --release -p esc-node）"; exit 1; }
[ -f "$WEBROOT/index.html" ] || { echo "FAIL: 找不到 $WEBROOT/index.html"; exit 1; }
command -v "$PY" >/dev/null 2>&1 || { echo "FAIL: 找不到 Python：$PY"; exit 1; }

EC_PID=""
cleanup() {
  for pid in ${EC_PID:-} ${LIVE_PID:-}; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
  rm -f "${SOCK:-}" "${SOCK:-}".client* 2>/dev/null || true
}
trap cleanup EXIT

wait_http() {
  local base="$1"
  for _ in $(seq 1 60); do
    if "$PY" - "$base" <<'PY' 2>/dev/null
import sys, urllib.request
try:
    with urllib.request.urlopen(sys.argv[1] + "/", timeout=1) as response:
        sys.exit(0 if response.status == 200 else 1)
except Exception:
    sys.exit(1)
PY
    then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

check_served_web() {
  local base="$1"
  "$PY" - "$base" <<'PY'
import re
import sys
import urllib.error
import urllib.request

base = sys.argv[1]


def get(path):
    try:
        with urllib.request.urlopen(base + path, timeout=5) as response:
            return response.status, response.headers.get("Content-Type", ""), response.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, "", ""


status, content_type, html = get("/")
assert status == 200, f"GET / -> {status}"
assert "text/html" in content_type, f"GET / content-type {content_type}"
scripts = re.findall(r'<script[^>]+src="([^"]+)"', html)
links = re.findall(r'<link[^>]+href="([^"]+)"', html)
assets = [item for item in scripts + links if not item.startswith(("http:", "https:"))]
assert "i18n.js" in assets, f"index.html 未引用 i18n.js：{assets}"
for asset in assets:
    path = "/" + asset.lstrip("./")
    status, content_type, body = get(path)
    assert status == 200, f"{path} -> {status}"
    assert body.strip(), f"{path} 内容为空"
    if path.endswith(".js"):
        assert "javascript" in content_type, f"{path} content-type {content_type}"
status, content_type, body = get("/i18n.js")
assert status == 200, f"/i18n.js -> {status}"
assert "javascript" in content_type, f"/i18n.js content-type {content_type}"
assert "ECSIM_I18N" in body, "/i18n.js 内容缺少 ECSIM_I18N（G-18 回归）"
status, _, _ = get("/missing-asset-does-not-exist.js")
assert status == 404, f"未知资源应 404，实际 {status}"
print(f"OK: assets {assets} all 200; /i18n.js has ECSIM_I18N; unknown asset 404")
PY
}

echo "== 静态 serve =="
"$BIN" --serve "127.0.0.1:$PORT" --web-root "$WEBROOT" --manifest "$MANIFEST" >"$LOG_DIR/static.log" 2>&1 &
EC_PID=$!
if ! wait_http "http://127.0.0.1:$PORT"; then
  cat "$LOG_DIR/static.log"
  echo "FAIL: 静态 --serve 未就绪"
  exit 1
fi
check_served_web "http://127.0.0.1:$PORT"
echo "OK: 静态 --serve 资源齐全（G-18 主路径）"

if [ "$(uname -s 2>/dev/null || echo unknown)" = "Linux" ]; then
  echo "== 实时 serve（--uds 同源）=="
  SOCK="${SOCK:-/tmp/ecsim-serve-assets.sock}"
  rm -f "$SOCK"
  LIVE_PORT=$((PORT + 1))
  "$BIN" --uds "$SOCK" --serve "127.0.0.1:$LIVE_PORT" --web-root "$WEBROOT" --manifest "$MANIFEST" >"$LOG_DIR/live.log" 2>&1 &
  LIVE_PID=$!
  for _ in $(seq 1 100); do [ -S "$SOCK" ] && break; sleep 0.05; done
  if ! wait_http "http://127.0.0.1:$LIVE_PORT"; then
    cat "$LOG_DIR/live.log"
    echo "FAIL: 实时 --serve 未就绪"
    exit 1
  fi
  check_served_web "http://127.0.0.1:$LIVE_PORT"
  echo "OK: 实时 --serve 同源资源齐全"
else
  echo "SKIP: 非 Linux 平台跳过 --uds 实时段（静态段已覆盖 G-18）"
fi

echo "OK: serve-web-assets 探针通过（--serve 下 /i18n.js 200 + ECSIM_I18N + 资源清单全 200）"
