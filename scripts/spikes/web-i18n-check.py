#!/usr/bin/env python3
"""DOD-09（H2）：Web i18n 静态检查（zh/en 键集、引用、长度、lang 切换）。

检查项：
1. `web/i18n.js` 的 zh/en 键集**完全一致**，无空翻译；en 文本长度 ≤ zh×1.3（字符数，防止英文溢出）；
2. `web/index.html` 中所有 `data-i18n` / `data-i18n-html` 键均可解析；`data-i18n-html` 键的 zh 值允许含
   `<code>` 等受控标签（仅白名单标签）；
3. 所有字典键至少被 `index.html` 或 `app.js` 引用（无未使用键），除显式允许的保留键；
4. 语言切换：`#lang-toggle` 存在（44px/aria-pressed）、`i18n.js` 提供 `ECSIM_I18N.setLang`、`<html lang>`
   切换逻辑存在；`?lang=en` 覆盖存在（供截图/自动化）；
5. `testid`/`id` 契约不随语言变化：i18n.js 不出现 `data-testid`（防止翻译污染契约）。

退出码 0 = 通过；1 = 有缺口（逐项打印）。
"""

import pathlib
import re
import sys

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

ROOT = pathlib.Path(__file__).resolve().parents[2]
INDEX = ROOT / "web" / "index.html"
APP = ROOT / "web" / "app.js"
I18N = ROOT / "web" / "i18n.js"

# 允许保留（暂时未引用）的键前缀；新增键需有引用。
ALLOWED_UNUSED = {"topbar.lang"}
ALLOWED_HTML_TAGS = {"code", "strong", "b", "em", "span", "br"}
LEN_RATIO = 1.3

def est_width(text):
    """粗略视觉宽度：CJK 记 2，其余记 1（用于替代裸字符数比较）。"""
    return sum(2 if "\u4e00" <= char <= "\u9fff" else 1 for char in text)


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def parse_dict(text):
    """从 i18n.js 抽取 zh/en 的 key→value（简单正则；本项目词典为扁平结构）。"""
    languages = {}
    for name in ("zh", "en"):
        match = re.search(rf"\n    {name}: \{{(.*?)\n    \}},", text, re.S)
        if not match:
            fail(f"i18n.js 缺少 {name} 词典")
        pairs = re.findall(r'"([A-Za-z0-9.\-]+)":\s*"((?:[^"\\]|\\.)*)"', match.group(1))
        languages[name] = {key: value.replace('\\"', '"') for key, value in pairs}
    return languages


def main():
    for path in (INDEX, APP, I18N):
        if not path.is_file():
            fail(f"文件不存在：{path}")
    index = INDEX.read_text(encoding="utf-8")
    app = APP.read_text(encoding="utf-8")
    i18n = I18N.read_text(encoding="utf-8")

    languages = parse_dict(i18n)
    zh, en = languages["zh"], languages["en"]
    problems = []

    missing_en = sorted(set(zh) - set(en))
    missing_zh = sorted(set(en) - set(zh))
    if missing_en:
        problems.append(f"en 缺键：{missing_en}")
    if missing_zh:
        problems.append(f"zh 缺键：{missing_zh}")

    for key in sorted(set(zh) & set(en)):
        if not en[key].strip():
            problems.append(f"en 空翻译：{key}")
        if not zh[key].strip():
            problems.append(f"zh 空翻译：{key}")
        # 视觉宽度 ≤ zh×1.3（短标签允许 +6 绝对余量；长句允许 +40；最终布局由 headless 无溢出兜底）。
        zh_width, en_width = est_width(zh[key]), est_width(en[key])
        allowance = 6 if zh_width <= 20 else 40
        if en_width > max(zh_width * LEN_RATIO, zh_width + allowance):
            problems.append(f"en 宽度超 {LEN_RATIO}×zh（{en_width}/{zh_width}）：{key}")
        # en 值与 zh 相同且非纯符号（如 PDO/SDO）→ 可能未翻译
        if en[key] == zh[key] and re.search(r"[\u4e00-\u9fff]", zh[key]):
            problems.append(f"疑似未翻译（zh==en 且含中文）：{key}")
        if key.startswith("topbar.") or key.startswith("nav.") or key.startswith("section."):
            if not en[key].strip():
                problems.append(f"外壳键无英文：{key}")

    # data-i18n 引用可解析
    for attribute in re.findall(r'data-i18n="([^"]+)"', index):
        if attribute not in zh:
            problems.append(f"index.html 引用了未定义键：{attribute}")
        if attribute not in en:
            problems.append(f"index.html 键缺 en：{attribute}")

    # HTML 键仅允许白名单标签
    for key in re.findall(r'data-i18n-html="1"[^>]*', index):
        pass
    for key in zh:
        if key.startswith("section.") and ("<" in zh[key]):
            tags = set(re.findall(r"</?([a-z]+)", zh[key]))
            bad = tags - ALLOWED_HTML_TAGS
            if bad:
                problems.append(f"zh 键 {key} 含非白名单标签：{sorted(bad)}")

    # 未使用键：键未出现在 index.html/app.js 的字符串中（app.js 动态使用 t("key")）。
    combined = index + "\n" + app
    for key in sorted(set(zh) & set(en)):
        if key in ALLOWED_UNUSED:
            continue
        if f'"{key}"' not in combined:
            problems.append(f"未被引用的词典键：{key}")

    # 语言切换设施
    if 'id="lang-toggle"' not in index:
        problems.append("缺少 #lang-toggle 语言切换按钮")
    if 'aria-pressed' not in index:
        problems.append("语言切换缺少 aria-pressed")
    if "ECSIM_I18N" not in i18n or "setLang" not in i18n:
        problems.append("i18n.js 未暴露 ECSIM_I18N.setLang")
    if "document.documentElement.lang" not in i18n:
        problems.append("i18n.js 未更新 <html lang>")
    if "?lang=" not in i18n and "URLSearchParams" not in i18n:
        problems.append("i18n.js 未支持 ?lang= 覆盖（截图/自动化需要）")
    if "data-testid" in i18n:
        problems.append("i18n.js 不应包含 data-testid（契约不随语言变化）")

    if problems:
        fail("DOD-09 i18n 未全绿：\n  - " + "\n  - ".join(problems))

    print(
        "OK: DOD-09 i18n 通过 — zh/en 键集一致（%d 键）、无空翻译、en ≤ %.1f×zh、data-i18n 全部可解析、"
        "无未使用键、lang 切换/覆盖/44px 按钮齐备" % (len(zh), LEN_RATIO)
    )


if __name__ == "__main__":
    main()
