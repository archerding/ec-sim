#!/usr/bin/env python3
"""DOD-01（H2）：EC-Sim Web 契约校验（16 个 `#id` + 87 固定 testid + 9 参数化模板 + 4 通用态槽位）。

来源：[ECSIM-HMI-READINESS] §1.1/§1.2（跨仓设计基线，只读引用）。本脚本不引入第三方依赖。

检查项（对应 READINESS §1.3）：
1. **数量**：§1.1 的 16 个 `#id` 全在 `web/index.html`；§1.2 的 87 个固定 `data-testid` 全在
   `web/index.html` 或 `web/app.js`；9 个参数化 testid 各有模板使用点；4 个通用态槽位有模板占位。
2. **唯一性**：固定 testid 在单文件内无重复；固定 id 无重复。
3. **命名**：testid 全小写 kebab，前缀 ∈ {overview,slave,pdo,dict,sdo,evidence,about,shell}；
   不得用 testid 取代 `#id` 契约。
4. `--list`：打印注册表与当前缺失项（按组分类），供 H2 逐批补齐。

退出码：0 = 契约全绿；1 = 有缺失/重复/命名违规（缺失项分组打印）。
说明：本检查是**静态契约**检查，不等价于 DOM 运行时验证（H3 自动化负责）。
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
INDEX = ROOT / "web" / "index.html"
APP = ROOT / "web" / "app.js"

# READINESS §1.1：16 个保留 #id（冻结契约，不得删除/改名）。
IDS = [
    "file",
    "api",
    "load-api",
    "slave-index",
    "live",
    "message",
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

# READINESS §1.2：87 个固定 data-testid（按组）。
FIXED_TESTIDS = {
    "外壳": [
        "shell-topbar",
        "shell-topbar-mode-badge",
        "shell-topbar-source",
        "shell-topbar-pause",
        "shell-topbar-reconnect",
        "shell-topbar-about",
        "shell-skip-link",
        "shell-context",
        "shell-context-slave-select",
        "shell-context-address",
        "shell-context-al-chip",
        "shell-context-profile",
        "shell-context-datetime",
        "shell-context-copy-summary",
        "shell-context-export",
        "shell-toast",
        "shell-footer-note",
    ],
    "V-01 总线总览": [
        "overview-health",
        "overview-problems-table",
        "overview-problems-empty",
        "overview-topology",
        "overview-slaves-table",
        "overview-copy-summary",
        "overview-pause-toggle",
    ],
    "V-02 从站详情": [
        "slave-identity",
        "slave-sii",
        "slave-live",
        "slave-sm-table",
        "slave-pdo-rx",
        "slave-pdo-tx",
        "slave-dc",
        "slave-profile",
        "slave-goto-pdo",
        "slave-copy-identity",
    ],
    "V-03 过程数据": [
        "pdo-current-table",
        "pdo-decoded-table",
        "pdo-waveform-canvas",
        "pdo-waveform-summary",
        "pdo-waveform-table",
        "pdo-sample-info",
        "pdo-pause-toggle",
        "pdo-copy-hex",
    ],
    "V-04 对象字典": [
        "dict-search",
        "dict-filter-access",
        "dict-filter-value",
        "dict-table",
        "dict-drawer",
        "dict-drawer-goto-sdo",
        "dict-meta",
        "dict-empty",
    ],
    "V-05 SDO 调试": [
        "sdo-index-input",
        "sdo-index-error",
        "sdo-sub-input",
        "sdo-sub-error",
        "sdo-read",
        "sdo-status",
        "sdo-result",
        "sdo-value-hex",
        "sdo-value-copy",
        "sdo-write-enable",
        "sdo-write-fields",
        "sdo-write-value-input",
        "sdo-write-value-error",
        "sdo-write-button",
        "sdo-confirm-dialog",
        "sdo-confirm-slave",
        "sdo-confirm-old-value",
        "sdo-confirm-new-value",
        "sdo-confirm-submit",
        "sdo-confirm-cancel",
        "sdo-write-status",
        "sdo-audit-table",
        "sdo-audit-empty",
        "sdo-boundary-note",
    ],
    "V-06 证据导出": [
        "evidence-checklist",
        "evidence-meta",
        "evidence-preview",
        "evidence-copy",
        "evidence-download-md",
        "evidence-download-json",
        "evidence-boundaries",
    ],
    "V-07 关于/边界": [
        "about-version",
        "about-health",
        "about-support-matrix",
        "about-boundaries",
        "about-token",
        "about-copy-version",
    ],
}

# READINESS §1.2：9 个参数化 testid（模板名 + 静态前缀）。
PARAM_TESTIDS = {
    "shell-nav-<view>": "shell-nav-",
    "shell-banner-<level>": "shell-banner-",
    "overview-topology-node-<i>": "overview-topology-node-",
    "overview-slaves-row-<i>": "overview-slaves-row-",
    "overview-metric-<name>": "overview-metric-",
    "pdo-hex-<key>": "pdo-hex-",
    "dict-row-<index>-<sub>": "dict-row-",
    "sdo-audit-row-<n>": "sdo-audit-row-",
    "evidence-item-<key>": "evidence-item-",
}

# READINESS §1.2：4 个通用态槽位模板（按视图展开）。
GENERIC_SLOTS = ["<view>-empty", "<view>-loading", "<view>-error-state", "<view>-stale-badge"]

VIEW_PREFIXES = {"overview", "slave", "pdo", "dict", "sdo", "evidence", "about", "shell"}
TESTID_NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9<>|]+)*$")
TESTID_LITERAL_RE = re.compile(r'data-testid="([^"]+)"')


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def main():
    for path in (INDEX, APP):
        if not path.is_file():
            fail(f"文件不存在：{path}")
    index_text = INDEX.read_text(encoding="utf-8")
    app_text = APP.read_text(encoding="utf-8")
    combined = index_text + "\n" + app_text

    list_mode = "--list" in sys.argv[1:]

    # 1) #id 契约（16 个冻结 id）。
    missing_ids = [element_id for element_id in IDS if f'id="{element_id}"' not in index_text]
    duplicate_ids = sorted(
        {
            element_id
            for element_id in IDS
            if len(re.findall(r'(?<![-\w])id="' + re.escape(element_id) + r'"', index_text)) > 1
        }
    )

    # 2) 固定 testid 存在性（html/app.js 任意一处即可，但同一文件内不得重复）。
    present = {}
    missing = {}
    duplicates = {}
    for group, names in FIXED_TESTIDS.items():
        missing[group] = []
        for name in names:
            count_index = index_text.count(f'data-testid="{name}"')
            count_app = app_text.count(f'data-testid="{name}"')
            # 存在性：HTML 属性，或在 JS 中以字符串/template 传给 sectionT/el/setAttribute。
            quoted = f'"{name}"' in combined or f"'{name}'" in combined
            if count_index + count_app == 0 and not quoted:
                missing[group].append(name)
            else:
                present[name] = count_index + count_app
                if count_index > 1 or count_app > 1:
                    duplicates[name] = count_index + count_app

    # 3) 参数化模板/通用槽位：代码中必须有模板使用点（静态前缀或 ${view}- 形式）。
    missing_params = [name for name, prefix in PARAM_TESTIDS.items() if prefix not in combined]
    missing_slots = [
        slot for slot in GENERIC_SLOTS if "${view}" not in combined and slot.split("-", 1)[1] not in combined
    ]

    # 4) 命名检查：所有已出现的字面 testid 必须小写 kebab 且前缀合法。
    literal_bad = []
    for literal in TESTID_LITERAL_RE.findall(combined):
        if "<" in literal or "${" in literal:
            continue  # 模板化属性在运行时展开；此处只查字面值
        if not TESTID_NAME_RE.match(literal):
            literal_bad.append((literal, "命名不合 ^[a-z0-9]+(-[a-z0-9]+)*$"))
            continue
        prefix = literal.split("-", 1)[0]
        if prefix not in VIEW_PREFIXES and prefix not in {"shell"}:
            literal_bad.append((literal, f"前缀 {prefix!r} 不在允许集合"))

    total_fixed = sum(len(names) for names in FIXED_TESTIDS.values())
    found = total_fixed - sum(len(names) for names in missing.values())

    if list_mode:
        print(f"# 契约注册表：16 id + {total_fixed} 固定 testid + {len(PARAM_TESTIDS)} 参数化 + {len(GENERIC_SLOTS)} 通用槽位")
        for group, names in FIXED_TESTIDS.items():
            print(f"[{group}] {len(names) - len(missing[group])}/{len(names)}")
            for name in names:
                mark = "OK  " if name not in missing[group] else "MISS"
                print(f"  {mark} {name}")
        print("# 参数化模板:", "OK" if not missing_params else f"MISS {missing_params}")
        print("# 通用槽位:", "OK" if not missing_slots else f"MISS {missing_slots}")
        print(f"# 覆盖：{found}/{total_fixed} 固定 testid；#id 缺失 {len(missing_ids)}")
        return 0

    problems = []
    if missing_ids:
        problems.append(f"缺少 {len(missing_ids)} 个冻结 #id：{missing_ids}")
    if duplicate_ids:
        problems.append(f"#id 重复：{duplicate_ids}")
    for group, names in missing.items():
        if names:
            problems.append(f"{group} 缺少 testid：{names}")
    if duplicates:
        problems.append(f"testid 重复：{duplicates}")
    if missing_params:
        problems.append(f"缺少参数化 testid 模板：{missing_params}")
    if missing_slots:
        problems.append(f"缺少通用态槽位模板：{missing_slots}")
    if literal_bad:
        problems.append(f"命名违规：{literal_bad}")

    if problems:
        print(f"FAIL: DOD-01 契约未全绿（固定 testid {found}/{total_fixed}，16 id 缺失 {len(missing_ids)}）")
        for problem in problems:
            print(f"  - {problem}")
        print("提示：用 --list 查看逐项缺口；未实现视图的 testid 随对应组件补齐（H2 分批）。")
        sys.exit(1)

    print(
        "OK: DOD-01 契约全绿 — 16 id + %d 固定 testid + %d 参数化模板 + %d 通用槽位；命名/唯一性通过（静态）"
        % (total_fixed, len(PARAM_TESTIDS), len(GENERIC_SLOTS))
    )


if __name__ == "__main__":
    main()
