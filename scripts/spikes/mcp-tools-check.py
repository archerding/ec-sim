#!/usr/bin/env python3
"""B45 / M3-W8：校验 MCP/Agent 桥接 stub 的工具清单与只读端点映射。

检查项（无第三方依赖，任意平台可跑）：
1. `docs/plan/m3-w8-mcp-tools-v1.json` 结构合法：只读、每个工具都是 GET、输入 schema 最小合法；
2. 目录里的每个 endpoint 都出现在 `docs/CONTROL-PLANE.md` 的路由总表中，
   且文档中**所有 GET 路由**要么被某个工具映射、要么被显式排除（附理由）；
3. 文档中的查询参数变体（slave/history/index/sub）都被对应工具声明，不凭空发明参数；
4. 不存在任何写工具（POST/PUT/PATCH/DELETE 一律失败；POST 只能出现在 excluded 并附理由）；
5. 设计文档存在、引用目录文件并逐一提到工具名，明确"只读/不含写"；
6. M3 范围把 W8 标为完成、`run-all-probes.sh` 与 `VERIFICATION.md` 已登记本检查。

退出码：0 = 全部通过；1 = 至少一项失败。
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
CATALOG = ROOT / "docs" / "plan" / "m3-w8-mcp-tools-v1.json"
DESIGN = ROOT / "docs" / "plan" / "M3-W8-mcp-agent-bridge-stub.md"
CONTROL = ROOT / "docs" / "CONTROL-PLANE.md"
M3 = ROOT / "docs" / "plan" / "M3-scope.md"
RUN_ALL = ROOT / "scripts" / "spikes" / "run-all-probes.sh"
VERIFICATION = ROOT / "docs" / "VERIFICATION.md"

WRITE_METHODS = {"POST", "PUT", "PATCH", "DELETE"}
ALLOWED_METHODS = {"GET"}
PROP_TYPES = {"integer", "string", "boolean", "number"}
TOOL_NAME_RE = re.compile(r"^ecsim_[a-z0-9_]+$")


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


for required in (CATALOG, DESIGN, CONTROL, M3, RUN_ALL, VERIFICATION):
    if not required.is_file():
        fail(f"缺少必需文件：{required}")

catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
design_text = DESIGN.read_text(encoding="utf-8")
control_text = CONTROL.read_text(encoding="utf-8")
m3_text = M3.read_text(encoding="utf-8")
run_all_text = RUN_ALL.read_text(encoding="utf-8")
verif_text = VERIFICATION.read_text(encoding="utf-8")

# ---- 1. 目录顶层结构 ----
if catalog.get("schema") != "ecsim/mcp-tools-v1":
    fail("catalog.schema 必须是 ecsim/mcp-tools-v1")
if catalog.get("read_only") is not True:
    fail("catalog.read_only 必须为 true")
if not isinstance(catalog.get("protocol"), str) or not catalog["protocol"].strip():
    fail("catalog.protocol 必须是非空字符串")
tools = catalog.get("tools")
excluded = catalog.get("excluded")
if not isinstance(tools, list) or not tools:
    fail("catalog.tools 必须是非空数组")
if not isinstance(excluded, list):
    fail("catalog.excluded 必须是数组（即使为空）")

# ---- 2. 解析 CONTROL-PLANE.md 路由总表（只取 `GET ...` 单元格）----
doc_routes = {}  # base path -> set(query param names)
for line in control_text.splitlines():
    if not line.startswith("| `GET "):
        continue
    match = re.match(r"\|\s*`GET\s+([^\s`]+)", line)
    if not match:
        continue
    target = match.group(1)
    base, _, query = target.partition("?")
    params = set()
    if query:
        for part in query.split("&"):
            name = part.split("=", 1)[0]
            if name:
                params.add(name)
    doc_routes.setdefault(base, set()).update(params)

if not doc_routes:
    fail("未能从 docs/CONTROL-PLANE.md 解析出任何 GET 路由（路由总表格式变了？）")

# ---- 3. 校验每个工具 ----
names = set()
mapped = set()
query_union = {base: set() for base in doc_routes}
write_tools = []

for index, tool in enumerate(tools):
    where = f"tools[{index}]"
    if not isinstance(tool, dict):
        fail(f"{where} 必须是对象")
    name = tool.get("name")
    if not isinstance(name, str) or not TOOL_NAME_RE.match(name):
        fail(f"{where}.name 非法：{name!r}（应为 ecsim_<snake_case>）")
    if name in names:
        fail(f"工具名重复：{name}")
    names.add(name)

    description = tool.get("description")
    if not isinstance(description, str) or len(description) < 10:
        fail(f"{name}.description 必须是至少 10 字的说明")

    http = tool.get("http")
    if not isinstance(http, dict):
        fail(f"{name}.http 必须是对象")
    method = http.get("method")
    if method in WRITE_METHODS:
        write_tools.append(name)
    if method not in ALLOWED_METHODS:
        fail(f"{name}.http.method 必须是 GET（当前 {method!r}）；本 stub 不含写")
    endpoint = http.get("endpoint")
    if endpoint not in doc_routes:
        fail(f"{name}.http.endpoint={endpoint!r} 不在 CONTROL-PLANE.md 的 GET 路由总表中")
    mapped.add(endpoint)

    queries = tool.get("queries")
    if not isinstance(queries, list):
        fail(f"{name}.queries 必须是数组")
    for query in queries:
        if query not in doc_routes[endpoint]:
            fail(f"{name}.queries 声明了文档未列出的参数 {query!r}（endpoint {endpoint}）")
        query_union[endpoint].add(query)

    mode = tool.get("mode")
    if mode not in ("any", "live"):
        fail(f"{name}.mode 必须是 any 或 live（当前 {mode!r}）")
    if not isinstance(tool.get("response_schema"), str) or not tool["response_schema"].strip():
        fail(f"{name}.response_schema 必须是非空字符串")

    schema = tool.get("input_schema")
    if not isinstance(schema, dict) or schema.get("type") != "object":
        fail(f"{name}.input_schema 必须是 type=object 的 JSON Schema")
    if schema.get("additionalProperties") is not False:
        fail(f"{name}.input_schema 必须 additionalProperties=false")
    props = schema.get("properties")
    if not isinstance(props, dict):
        fail(f"{name}.input_schema.properties 必须是对象")
    required = schema.get("required", [])
    if not isinstance(required, list):
        fail(f"{name}.input_schema.required 必须是数组")
    for key in required:
        if key not in props:
            fail(f"{name}.input_schema.required 含未定义属性 {key!r}")
    for key, prop in props.items():
        if not isinstance(prop, dict) or prop.get("type") not in PROP_TYPES:
            fail(f"{name}.input_schema.properties[{key!r}] 类型必须是 {sorted(PROP_TYPES)}")
        if "pattern" in prop:
            try:
                re.compile(prop["pattern"])
            except re.error as error:
                fail(f"{name}.input_schema.properties[{key!r}].pattern 非法：{error}")
        minimum = prop.get("minimum")
        maximum = prop.get("maximum")
        if isinstance(minimum, (int, float)) and isinstance(maximum, (int, float)):
            if minimum > maximum:
                fail(f"{name}.input_schema.properties[{key!r}] minimum > maximum")

if write_tools:
    fail(f"发现写工具（禁止）：{', '.join(write_tools)}")

# 文档里的查询参数变体必须被映射工具声明；工具也不得漏掉文档变体。
for base, params in sorted(doc_routes.items()):
    if base in mapped:
        missing = params - query_union[base]
        if missing:
            fail(f"路由 {base} 的文档参数 {sorted(missing)} 未被任何工具声明")

# ---- 4. 排除项：GET 只能用于未映射路由，POST 只能作为显式写排除 ----
excluded_get = set()
for index, item in enumerate(excluded):
    where = f"excluded[{index}]"
    if not isinstance(item, dict):
        fail(f"{where} 必须是对象")
    method = item.get("method")
    endpoint = item.get("endpoint")
    reason = item.get("reason")
    if not isinstance(reason, str) or len(reason) < 5:
        fail(f"{where}.reason 必须说明排除理由")
    if method == "GET":
        if endpoint not in doc_routes:
            fail(f"{where}.endpoint={endpoint!r} 不在文档 GET 路由总表中")
        if endpoint in mapped:
            fail(f"{where}: 路由 {endpoint} 既被工具映射又被排除")
        excluded_get.add(endpoint)
    elif method in WRITE_METHODS:
        if not reason.startswith(("写", "写路径")) and "写" not in reason:
            fail(f"{where}: 写端点的排除理由必须说明是写路径")
    else:
        fail(f"{where}.method={method!r} 非法")

# ---- 5. 覆盖完整性：文档里每条 GET 路由都要有归宿 ----
missing_routes = sorted(set(doc_routes) - mapped - excluded_get)
if missing_routes:
    fail(f"以下 GET 路由既未映射也未排除：{missing_routes}")

# ---- 6. 设计文档与其它文档登记 ----
if "m3-w8-mcp-tools-v1.json" not in design_text:
    fail("设计文档未引用 m3-w8-mcp-tools-v1.json")
if "只读" not in design_text or "不含写" not in design_text:
    fail("设计文档必须明确声明只读/不含写")
for name in sorted(names):
    if name not in design_text:
        fail(f"设计文档未提到工具 {name}")

if "W8" not in m3_text or "M3-W8-mcp-agent-bridge-stub.md" not in m3_text:
    fail("docs/plan/M3-scope.md 未链接 W8 设计文档")
for line in m3_text.splitlines():
    if line.startswith("| W8 "):
        if "❌" in line or "未开始" in line:
            fail("docs/plan/M3-scope.md 的 W8 仍标记为未开始")
        break
else:
    fail("docs/plan/M3-scope.md 缺少 W8 行")

if "mcp-tools" not in run_all_text:
    fail("run-all-probes.sh 未登记 mcp-tools 检查")
if "mcp-tools-check.py" not in verif_text:
    fail("docs/VERIFICATION.md 未登记 mcp-tools-check.py")

print(
    "OK: MCP stub 工具映射一致（%d 个只读工具、%d 个文档路由、%d 个排除项）"
    % (len(tools), len(doc_routes), len(excluded))
)
