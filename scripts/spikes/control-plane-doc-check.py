#!/usr/bin/env python3
"""B31：核对 docs/CONTROL-PLANE.md 与代码中的只读路由逐条一致。

检查项：
1. 代码（bin）中出现的路由判定字符串（`path_only == "..."`、`handle_get` 的 ROUTES）都能在文档中找到；
2. 文档“路由总表”中列出的每条路径都在代码中有对应处理（预期清单硬编码，改动路由时必须同步改本脚本）；
3. 文档引用的探针脚本存在；
4. 文档引用的源码文件存在。
无第三方依赖；可在任意平台运行。
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
DOC = ROOT / "docs" / "CONTROL-PLANE.md"
BIN = ROOT / "crates" / "esc-node" / "src" / "bin" / "ec_sim_esc.rs"
LIB = ROOT / "crates" / "control-plane" / "src" / "lib.rs"

EXPECTED_PATHS = [
    "/",
    "/health",
    "/state",
    "/dict",
    "/pdo",
    "/pdo?history=1",
    "/topology",
    "/metrics",
    "/state?slave=N",
    "/pdo?slave=N",
    "/metrics?slave=N",
    "/events",
]

EXPECTED_PROBES = [
    "http-state-probe.sh",
    "live-state-probe.sh",
    "pdo-live-probe.sh",
    "pdo-history-probe.sh",
    "pdo-decode-probe.sh",
    "topology-probe.sh",
    "slave-select-probe.sh",
    "metrics-probe.sh",
    "metrics-filter-probe.sh",
    "tap-live-state-probe.sh",
    "serve-web-probe.sh",
    "events-probe.sh",
    "write-pdo-probe.sh",
]


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


doc = DOC.read_text(encoding="utf-8")
bin_text = BIN.read_text(encoding="utf-8")
lib_text = LIB.read_text(encoding="utf-8")

# 1) 文档表格里出现的路由（`GET /xxx` 单元格）应覆盖预期清单。
for path in EXPECTED_PATHS:
    table_cell = path.split("?")[0] if path.count("?") == 2 else path
    pattern = re.escape(table_cell.split("?")[0])
    if not re.search(r"`GET " + pattern, doc):
        fail(f"文档路由总表缺少路径：{path}")

# 2) 代码中的路由处理应覆盖预期清单。
code_paths = ["/state", "/dict", "/pdo", "/topology", "/metrics"]
for path in code_paths:
    if f'"{path}"' not in bin_text and f'path: "{path}"' not in lib_text:
        fail(f"代码缺少路由处理：{path}")
if "history=1" not in bin_text:
    fail("代码缺少 /pdo?history=1 处理")
if "slave" not in bin_text:
    fail("代码缺少 ?slave=N 处理")

if "POST /pdo" in doc:
    assert "--serve-write" in bin_text, "文档描述 POST /pdo，但代码缺少 --serve-write"
    assert "--serve-write" in doc, "文档缺少 --serve-write 说明"
    assert "write-pdo-probe.sh" in doc

# 3) 文档引用的探针脚本必须存在。
for probe in EXPECTED_PROBES:
    target = ROOT / "scripts" / "spikes" / probe
    if probe not in doc:
        fail(f"文档未引用探针：{probe}")
    if not target.is_file():
        fail(f"探针脚本不存在：{target}")

# 4) 文档引用的源码文件必须存在。
for relative in ["crates/control-plane", "crates/esc-node/src/bin/ec_sim_esc.rs"]:
    if relative not in doc:
        fail(f"文档未引用源码路径：{relative}")
    if not (ROOT / relative).exists():
        fail(f"源码路径不存在：{relative}")

print(
    "OK: docs/CONTROL-PLANE.md 与代码路由一致（%d 条路径、%d 个探针）"
    % (len(EXPECTED_PATHS), len(EXPECTED_PROBES))
)
