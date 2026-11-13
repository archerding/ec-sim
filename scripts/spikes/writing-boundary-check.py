#!/usr/bin/env python3
"""DOD-10（H2）：写作边界扫描（S-02 禁用词 + S-11 导出模板边界）。

目标：Web 资源里不得出现"已认证/一致性通过/实时 SLA/兼容所有"等**无依据承诺**；
否定句白名单内的表述（"不宣称…""不构成…""未验证…"）允许；导出模板必须含版本/模式/来源/时间/站/⚠️ 边界。

实现：对 `web/index.html`、`web/app.js` 做**行级**扫描：
- 禁用词命中且同一行没有否定标记（不/非/未/禁止/无/避免）→ FAIL；
- 页脚常驻边界声明断言；
- 导出模板（evidence-*）必须包含：版本、模式、来源、时间、站、⚠️ 未验证项、无 token/绝对路径占位。
无第三方依赖，可在任意平台运行。
"""

import pathlib
import re
import sys

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

ROOT = pathlib.Path(__file__).resolve().parents[2]
FILES = [ROOT / "web" / "index.html", ROOT / "web" / "app.js"]

# 禁用词/短语（大小写不敏感）。任何肯定式出现都视为无依据承诺。
FORBIDDEN = [
    "一致性测试通过",
    "已通过一致性",
    "一致性认证",
    "通过 ctt",
    "ctt 通过",
    "已认证",
    "认证通过",
    "保证实时",
    "实时性保证",
    "实时 sla",
    "兼容所有",
    "支持所有",
    "生产可用",
    "免维护",
]
# 否定/边界标记：同一行出现任一即视为"否定句白名单"。
NEGATIONS = ["不", "非", "未", "禁止", "无", "避免", "仅", "⚠️", "not"]


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def main():
    text_all = ""
    hits = []
    for path in FILES:
        if not path.is_file():
            fail(f"文件不存在：{path}")
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            lowered = line.lower()
            for word in FORBIDDEN:
                if word in lowered:
                    has_negation = any(mark in line for mark in NEGATIONS)
                    if not has_negation:
                        hits.append((path.name, number, line.strip()[:120], word))
        text_all += "\n" + path.read_text(encoding="utf-8")

    if hits:
        fail("出现无依据承诺（禁用词且无否定）：" + "; ".join(f"{name}:{line}: {snippet}" for name, line, snippet, _ in hits))

    index = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
    app = (ROOT / "web" / "app.js").read_text(encoding="utf-8")

    # 页脚常驻边界声明。
    footer = re.search(r'<footer[^>]*data-testid="shell-footer-note"[^>]*>(.*?)</footer>', index, re.S)
    if not footer:
        fail("缺少常驻页脚边界声明（shell-footer-note）")
    footer_text = footer.group(1)
    for marker in ("不宣称", "快照", "⚠️"):
        if marker not in footer_text:
            fail(f"页脚边界声明缺少：{marker}")

    # 导出模板边界（evidence-*；版本/模式/来源/时间/站/⚠️/脱敏）。
    for marker in ("ecsim-evidence-v1", "版本", "模式", "来源", "时间", "站", "⚠️", "边界"):
        if marker not in text_all:
            fail(f"导出/边界文本缺少：{marker}")
    # 脱敏：只拦"看起来像真实凭证/绝对路径"的字面量；占位符（<token>、xxx）与说明文字允许。
    suspicious = [
        r"Bearer\s+[A-Za-z0-9._~+/=-]{20,}",  # 真实长度 token 字面量
        r"/home/[a-z]",  # 绝对家目录路径
        r"root@[0-9]",  # 主机账号
    ]
    for pattern in suspicious:
        if re.search(pattern, text_all):
            fail(f"Web 资源疑似泄漏凭证/绝对路径：{pattern}")

    print(
        "OK: DOD-10 写作边界通过 — 禁用词 %d 类无肯定式命中；页脚常驻边界；导出模板含版本/模式/来源/时间/站/⚠️ 边界；无 token/绝对路径"
        % len(FORBIDDEN)
    )


if __name__ == "__main__":
    main()
