#!/usr/bin/env python3
"""B42 预回归：Web SDO 面板静态检查（无 DOM 自动化，见 M3 范围 W6）。

检查内容：
1. `web/index.html` 含 SDO 面板的全部控件 id（读索引/子索引/读取/结果/写开关/写输入/写按钮），
   以及 token 模式提示（`--serve-token-env`、`Authorization`、反向代理/curl）与“二次确认”说明；
2. `web/app.js` 含面板函数与关键安全行为：只读 `GET /sdo`、`renderSdoResult`、
   写入前的二次确认（`window.confirm`）、405/401/403/404 错误提示、`POST` 用小端字节体；
3. `web/README.md` 记录面板用法与“⚠️ 无 DOM 自动化测试”限制；
4. 若本机有 `node`：`node --check web/app.js` 语法通过；没有 node 时打印 SKIP（不判失败）。

说明：本检查是**静态资源回归**，不模拟 DOM、不点击页面；真实交互仍需浏览器人工验收。
无第三方依赖，可在任意平台运行。

用法：python3 scripts/spikes/web-sdo-panel-check.py
"""

import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
INDEX = ROOT / "web" / "index.html"
APP = ROOT / "web" / "app.js"
README = ROOT / "web" / "README.md"

IDS = [
    "sdo-index",
    "sdo-sub",
    "sdo-read",
    "sdo-status",
    "sdo-result",
    "sdo-write-enable",
    "sdo-write-fields",
    "sdo-write-value",
    "sdo-write",
    "sdo-write-status",
]

APP_MARKERS = [
    "readSdo",
    "writeSdo",
    "renderSdoResult",
    "sdoFailureHint",
    "parseSdoIndex",
    "hexToBytes",
    "window.confirm",
    "Uint8Array",
    "ec-sim/sdo-v1",
    "405",
    "401",
    "403",
    "404",
]


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


for path in (INDEX, APP, README):
    if not path.is_file():
        fail(f"文件不存在：{path}")

index_text = INDEX.read_text(encoding="utf-8")
app_text = APP.read_text(encoding="utf-8")
readme_text = README.read_text(encoding="utf-8")

# 1) 面板控件与提示。
for element_id in IDS:
    if f'id="{element_id}"' not in index_text:
        fail(f"index.html 缺少 SDO 面板控件 id={element_id}")
for hint in ("--serve-token-env", "Authorization", "反向代理", "curl", "二次确认", "--serve-write"):
    if hint not in index_text:
        fail(f"index.html 缺少 SDO 面板提示：{hint}")

# 2) 面板逻辑与安全行为。
for marker in APP_MARKERS:
    if marker not in app_text:
        fail(f"app.js 缺少 SDO 面板实现标记：{marker}")
if "method: \"POST\"" not in app_text:
    fail("app.js 未见 POST /sdo 写入请求")
if "/sdo${parsed.query}" not in app_text:
    fail("app.js 未见 /sdo 请求路径拼接")
if "cache: \"no-store\"" not in app_text:
    fail("app.js 未见 no-store 读取（避免缓存旧值）")

# 3) README 记录面板与限制。
for marker in ("SDO", "node --check", "无 DOM"):
    if marker not in readme_text:
        fail(f"web/README.md 缺少 SDO 面板说明：{marker}")

# 4) node --check（可选）。
node = shutil.which("node")
if node is None:
    print("SKIP: 未安装 node，跳过 node --check（静态断言已通过）")
else:
    result = subprocess.run(
        [node, "--check", str(APP)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(f"node --check 失败：{result.stderr.strip() or result.stdout.strip()}")
    print("OK: node --check web/app.js 语法通过")

print("OK: Web SDO 面板静态检查通过（%d 个控件、%d 个逻辑标记；无 DOM 自动化）" % (len(IDS), len(APP_MARKERS)))
