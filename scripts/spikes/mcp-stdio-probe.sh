#!/usr/bin/env bash
# B47 / M3-W8：`ec-sim-mcp` stdio 桥接端到端探针。
# 起真实控制面（静态 `--serve` + manifest）→ 用 python 合成 MCP client 走 stdio JSON-RPC，
# 验证 9 个只读工具、读写边界与 `--serve-write` 下表不变。无 root、任意平台可跑。
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 1

BIN="${BIN:-target/release/ec-sim-esc}"
MCP_BIN="${MCP_BIN:-target/release/ec-sim-mcp}"
PY="${PY:-python3}"
MANIFEST="${MANIFEST:-plugins/generic-cia402-cst-servo/plugin.yaml}"

# Windows Git Bash 里常见 `python` 而没有 `python3`；可执行文件也可能带 `.exe`。
if ! command -v "$PY" >/dev/null 2>&1 && command -v python >/dev/null 2>&1; then
  PY="python"
fi
if [ -f "$BIN.exe" ]; then
  BIN="$BIN.exe"
fi
if [ -f "$MCP_BIN.exe" ]; then
  MCP_BIN="$MCP_BIN.exe"
fi

command -v "$PY" >/dev/null 2>&1 || {
  echo "SKIP: 需要 python3"
  exit 2
}
[ -f "$BIN" ] || {
  echo "SKIP: 缺少 $BIN（先 cargo build --release -p esc-node）"
  exit 2
}
if [ ! -f "$MCP_BIN" ]; then
  command -v cargo >/dev/null 2>&1 || {
    echo "SKIP: 缺少 $MCP_BIN 且无 cargo"
    exit 2
  }
  cargo build --release -q -p mcp-server || {
    echo "SKIP: 构建 ec-sim-mcp 失败"
    exit 2
  }
  [ -f "$MCP_BIN" ] || MCP_BIN="$MCP_BIN.exe"
fi

exec "$PY" scripts/spikes/mcp-stdio-probe.py --server "$BIN" --mcp "$MCP_BIN" --manifest "$MANIFEST"
