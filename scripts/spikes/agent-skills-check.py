#!/usr/bin/env python3
"""B52 / M4：Agent Skills 静态校验（零第三方依赖）。

校验 `docs/skills/` 下的技能文档与示例是否与 MCP 只读工具目录一致：
1. 每个技能目录有 `SKILL.md`，YAML frontmatter 含 name/description/version/mode/tools；
2. `mode` 必须是 `read-only`；`tools` 必须全部来自 `docs/plan/m3-w8-mcp-tools-v1.json`，
   且不得出现 write/post 类工具；
3. 正文必须写明只读边界、快照语义与稳定错误码（AUTH_REQUIRED / MODE_UNAVAILABLE）；
4. `examples/*.jsonl` 每行是合法 JSON-RPC 2.0；`tools/call` 的工具名必须在目录内；
   示例中不得出现 token/凭据；
5. Markdown 相对链接可解析。

退出码：0 = 通过；1 = 失败。任何平台可跑，无 root、无网络。
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SKILLS_DIR = ROOT / "docs" / "skills"
CATALOG_PATH = ROOT / "docs" / "plan" / "m3-w8-mcp-tools-v1.json"

NAME_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
LINK_RE = re.compile(r"\]\(([^)\s]+)\)")
ALLOWED_METHODS = {
    "initialize",
    "notifications/initialized",
    "ping",
    "tools/list",
    "tools/call",
}

failures = []


def check(condition, message):
    if condition:
        print(f"ok   {message}")
    else:
        print(f"FAIL {message}")
        failures.append(message)


def load_catalog():
    data = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
    return {tool["name"] for tool in data["tools"]}


def parse_frontmatter(text):
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return None, text
    end = None
    for index in range(1, len(lines)):
        if lines[index].strip() == "---":
            end = index
            break
    if end is None:
        return None, text
    meta = {}
    tools = []
    current = None
    for line in lines[1:end]:
        if not line.strip():
            continue
        item = re.match(r"^\s+-\s+(.+)$", line)
        if item and current == "tools":
            tools.append(item.group(1).strip().strip('"').strip("'"))
            continue
        if ":" in line:
            key, value = line.split(":", 1)
            key, value = key.strip(), value.strip().strip('"').strip("'")
            current = key
            if key == "tools" and value:
                tools.append(value)
            elif key:
                meta[key] = value
    meta["tools"] = tools
    return meta, "\n".join(lines[end + 1 :])


def check_links(path, text):
    for match in LINK_RE.finditer(text):
        target = match.group(1)
        if target.startswith(("http://", "https://", "#", "mailto:")):
            continue
        resolved = (path.parent / target).resolve()
        if not resolved.exists():
            check(False, f"{path.relative_to(ROOT)} 链接不存在：{target}")


def check_examples(skill_dir, catalog_names):
    example_dir = skill_dir / "examples"
    if not example_dir.is_dir():
        check(False, f"{skill_dir.name} 缺 examples/ 目录")
        return 0
    files = sorted(example_dir.glob("*.jsonl"))
    check(len(files) >= 1, f"{skill_dir.name} 至少 1 个示例 .jsonl")
    total_lines = 0
    for path in files:
        lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
        check(len(lines) >= 2, f"{path.relative_to(ROOT)} 至少 2 行")
        for number, line in enumerate(lines, start=1):
            try:
                payload = json.loads(line)
            except json.JSONDecodeError as error:
                check(False, f"{path.relative_to(ROOT)}:{number} 非法 JSON：{error}")
                continue
            total_lines += 1
            check(payload.get("jsonrpc") == "2.0", f"{path.name}:{number} jsonrpc=2.0")
            method = payload.get("method")
            check(method in ALLOWED_METHODS, f"{path.name}:{number} 方法在白名单：{method}")
            if method == "tools/call":
                name = (payload.get("params") or {}).get("name")
                check(name in catalog_names, f"{path.name}:{number} 工具在目录内：{name}")
            text_lower = line.lower()
            check(
                "token" not in text_lower and "bearer" not in text_lower,
                f"{path.name}:{number} 示例不含 token/凭据",
            )
    return total_lines


def main():
    check(SKILLS_DIR.is_dir(), "docs/skills/ 存在")
    check(CATALOG_PATH.is_file(), "MCP 工具目录存在")
    if not SKILLS_DIR.is_dir() or not CATALOG_PATH.is_file():
        return 1
    catalog_names = load_catalog()
    check(len(catalog_names) == 9, f"工具目录恰好 9 个只读工具（实际 {len(catalog_names)}）")

    readme = SKILLS_DIR / "README.md"
    check(readme.is_file(), "docs/skills/README.md 存在")
    if readme.is_file():
        readme_text = readme.read_text(encoding="utf-8")
        check("只读" in readme_text, "README 声明只读边界")
        check("不构成" in readme_text, "README 声明不构成认证/保证")
        check("ecsim-diagnose" in readme_text, "README 索引列出 ecsim-diagnose")
        check_links(readme, readme_text)

    skills = [path for path in SKILLS_DIR.iterdir() if path.is_dir()]
    check(len(skills) >= 1, "至少 1 个技能目录")
    tool_refs = 0
    example_lines = 0
    for skill_dir in sorted(skills):
        skill_file = skill_dir / "SKILL.md"
        if not skill_file.is_file():
            check(False, f"{skill_dir.name} 缺 SKILL.md")
            continue
        text = skill_file.read_text(encoding="utf-8")
        meta, body = parse_frontmatter(text)
        if meta is None:
            check(False, f"{skill_dir.name}/SKILL.md 缺 YAML frontmatter")
            continue
        name = meta.get("name", "")
        check(NAME_RE.match(name) is not None, f"{skill_dir.name} name 为 kebab-case：{name}")
        check(name == skill_dir.name, f"{skill_dir.name} name 与目录名一致")
        check(len(meta.get("description", "")) >= 20, f"{skill_dir.name} description ≥20 字符")
        check(bool(meta.get("version")), f"{skill_dir.name} 有 version")
        check(meta.get("mode") == "read-only", f"{skill_dir.name} mode=read-only")
        tools = meta.get("tools", [])
        check(len(tools) >= 5, f"{skill_dir.name} tools ≥5（实际 {len(tools)}）")
        for tool in tools:
            check(tool in catalog_names, f"{skill_dir.name} 工具在目录内：{tool}")
            check("write" not in tool.lower() and "post" not in tool.lower(),
                  f"{skill_dir.name} 工具非写：{tool}")
        tool_refs += len(tools)
        check("只读" in body, f"{skill_dir.name} 正文声明只读边界")
        check("不构成" in body, f"{skill_dir.name} 正文声明快照/认证边界")
        check("AUTH_REQUIRED" in body, f"{skill_dir.name} 含错误码 AUTH_REQUIRED")
        check("MODE_UNAVAILABLE" in body, f"{skill_dir.name} 含错误码 MODE_UNAVAILABLE")
        check_links(skill_file, text)
        example_lines += check_examples(skill_dir, catalog_names)

    check(example_lines >= 6, f"示例总行数 ≥6（实际 {example_lines}）")
    if failures:
        print(f"FAIL: agent-skills 校验 {len(failures)} 项失败")
        return 1
    print(f"OK: agent-skills（{len(skills)} 个技能、{tool_refs} 个工具引用、{example_lines} 行示例）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
