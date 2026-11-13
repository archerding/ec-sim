#!/usr/bin/env python3
"""EC-Sim 测试报告生成器（B49，M4）。

一次运行生成 **JSON + Markdown** 两份报告，包含：
- 环境：OS/架构、Python、rustc/cargo、Git HEAD 与 dirty 状态；
- 检查：`cargo fmt/clippy/test/doc`（可跳过）与 `run-all-probes.sh`（可跳过/复用日志）；
- 结果：每条检查的退出码、耗时、日志路径；测试用例 passed/failed 计数；探针 PASS/FAIL/SKIP 明细；
- 证据链接：[docs/VERIFICATION.md](../../docs/VERIFICATION.md) 等仓库内相对链接。

设计约束（与 M3-W8/M4 一致）：
- **零第三方依赖**（仅标准库）、跨平台（Windows / Linux）；
- 不宣称一致性/CTT/认证；报告只记录"仓库用例映射/预回归"结果；
- 报告默认写到 `target/test-reports/`（已被 `.gitignore` 忽略，不入库）；
- 失败时退出码 1（便于 CI/本地门禁），成功（允许 SKIP）退出码 0。

用法：
    python3 scripts/spikes/gen-test-report.py
    python3 scripts/spikes/gen-test-report.py --skip-cargo --skip-probes \
        --probes-log /tmp/ec-sim-probes.log --out target/test-reports

报告文件名固定为 `report.json` / `report.md`，便于 CI artifact 上传与对比。
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

SCHEMA = "ecsim/test-report-v1"
TOOL = "scripts/spikes/gen-test-report.py"
ROOT = Path(__file__).resolve().parents[2]

TEST_LINE = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed")
PROBE_LINE = re.compile(r"^(PASS|FAIL|SKIP)\s+([A-Za-z0-9_.-]+)")
PROBE_SUMMARY = re.compile(r"汇总：(\d+) PASS / (\d+) FAIL / (\d+) SKIP")
PROBE_RESULT = re.compile(r"RESULT: (PASS|FAIL)")


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_command(cmd: list[str], *, timeout: int, log_path: Path) -> dict:
    """运行命令并写日志；返回 exit/duration/log 记录（不抛异常）。"""
    started = time.time()
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
        output = proc.stdout or ""
        code = proc.returncode
    except FileNotFoundError:
        output = f"命令不存在：{cmd[0]}\n"
        code = 127
    except subprocess.TimeoutExpired:
        output = f"命令超时（{timeout} s）：{' '.join(cmd)}\n"
        code = 124
    except OSError as exc:  # pragma: no cover - 平台差异兜底
        output = f"命令执行失败：{exc}\n"
        code = 126
    duration = round(time.time() - started, 2)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(output, encoding="utf-8")
    return {
        "cmd": " ".join(cmd),
        "exit": code,
        "duration_s": duration,
        "log": str(log_path.relative_to(ROOT)),
        "output": output,
    }


def git_info() -> dict:
    try:
        head = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=str(ROOT), capture_output=True, text=True, timeout=15,
        )
        status = subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=str(ROOT), capture_output=True, text=True, timeout=15,
        )
        dirty_files = [line for line in (status.stdout or "").splitlines() if line.strip()]
        return {
            "head": (head.stdout or "").strip() if head.returncode == 0 else None,
            "dirty": bool(dirty_files),
            "dirty_count": len(dirty_files),
        }
    except (OSError, subprocess.SubprocessError):
        return {"head": None, "dirty": None, "dirty_count": None}


def tool_version(cmd: list[str]) -> str | None:
    try:
        proc = subprocess.run(
            cmd, cwd=str(ROOT), capture_output=True, text=True,
            timeout=20, encoding="utf-8", errors="replace",
        )
        if proc.returncode != 0:
            return None
        return (proc.stdout or proc.stderr or "").strip().splitlines()[0]
    except (OSError, subprocess.SubprocessError):
        return None


def parse_cargo_test(output: str) -> dict:
    passed = failed = 0
    for match in TEST_LINE.finditer(output):
        passed += int(match.group(2))
        failed += int(match.group(3))
    return {
        "passed": passed,
        "failed": failed,
        "suites": len(TEST_LINE.findall(output)),
        "has_failure_marker": "FAILED" in output or "error[" in output,
    }


def parse_probes(output: str) -> dict:
    probes: list[dict] = []
    for line in output.splitlines():
        match = PROBE_LINE.match(line.strip())
        if match:
            probes.append({"name": match.group(2), "status": match.group(1).lower()})
    summary_match = PROBE_SUMMARY.search(output)
    result_match = PROBE_RESULT.search(output)
    summary = None
    if summary_match:
        summary = {
            "pass": int(summary_match.group(1)),
            "fail": int(summary_match.group(2)),
            "skip": int(summary_match.group(3)),
        }
    result = result_match.group(1).lower() if result_match else None
    status = "pass" if result == "pass" else ("fail" if summary and summary["fail"] > 0 else "unknown")
    if result is None and probes:
        status = "fail" if any(p["status"] == "fail" for p in probes) else "pass"
    return {"probes": probes, "summary": summary, "result": result, "status": status}


def build_checks(args, out_dir: Path) -> list[dict]:
    checks: list[dict] = []
    if not args.skip_cargo:
        steps = [
            ("cargo_fmt", ["cargo", "fmt", "--all", "--", "--check"]),
            ("cargo_clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
            ("cargo_test", ["cargo", "test", "--workspace"]),
            ("cargo_doc", ["cargo", "doc", "--workspace", "--no-deps"]),
        ]
        for name, cmd in steps:
            record = run_command(cmd, timeout=args.timeout, log_path=out_dir / f"{name}.log")
            check = {"name": name, "status": "pass" if record["exit"] == 0 else "fail",
                     "exit": record["exit"], "duration_s": record["duration_s"], "log": record["log"]}
            if name == "cargo_test":
                check["tests"] = parse_cargo_test(record["output"])
            checks.append(check)
    if not args.skip_probes:
        if args.probes_log:
            log_path = Path(args.probes_log)
            if log_path.is_file():
                output = log_path.read_text(encoding="utf-8", errors="replace")
                exit_code = 0
                duration = 0.0
                copied_log = out_dir / "probes.log"
                copied_log.parent.mkdir(parents=True, exist_ok=True)
                copied_log.write_text(output, encoding="utf-8")
                log_ref = str(copied_log.relative_to(ROOT))
            else:
                output = f"探针日志不存在：{log_path}\n"
                exit_code = 2
                duration = 0.0
                log_ref = None
        else:
            bash = shutil.which("bash")
            if bash is None:
                checks.append({"name": "probes", "status": "skip", "exit": None,
                               "duration_s": 0.0, "log": None,
                               "reason": "未找到 bash（Windows 可用 Git Bash；Linux 自带）"})
                return checks
            record = run_command([bash, "scripts/spikes/run-all-probes.sh"],
                                 timeout=args.timeout, log_path=out_dir / "probes.log")
            output, exit_code, duration, log_ref = record["output"], record["exit"], record["duration_s"], record["log"]
        parsed = parse_probes(output)
        status = parsed["status"]
        if status == "unknown" and exit_code == 0:
            status = "pass"
        elif status == "unknown":
            status = "fail"
        checks.append({
            "name": "probes", "status": status, "exit": exit_code, "duration_s": duration,
            "log": log_ref, "probes": parsed["probes"], "summary": parsed["summary"],
            "result": parsed["result"],
        })
    return checks


def build_environment() -> dict:
    return {
        "os": platform.system(),
        "os_release": platform.release(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "rustc": tool_version(["rustc", "--version"]),
        "cargo": tool_version(["cargo", "--version"]),
        "ci": os.environ.get("CI", "") == "true" or os.environ.get("GITHUB_ACTIONS", "") == "true",
    }


def markdown_report(report: dict) -> str:
    env = report["environment"]
    git = report["git"]
    checks = report["checks"]
    test_check = next((c for c in checks if c["name"] == "cargo_test"), None)
    probe_check = next((c for c in checks if c["name"] == "probes"), None)
    lines = [
        f"# EC-Sim 测试报告（{report['generated_at']}）",
        "",
        f"> 生成器：`{TOOL}`（schema `{SCHEMA}`）。本报告是**仓库用例映射 / 预回归**记录，",
        "> **不构成** EtherCAT 一致性测试（CTT）或任何认证结论（`AGENTS.md` §4）。",
        "",
        "## 环境",
        "",
        "| 项 | 值 |",
        "| --- | --- |",
        f"| HEAD | `{git.get('head') or '-'}`（dirty={git.get('dirty')}，{git.get('dirty_count')} 个变更） |",
        f"| OS | {env['os']} {env['os_release']}（{env['machine']}） |",
        f"| Python | {env['python']} |",
        f"| rustc | {env['rustc'] or '-'} |",
        f"| cargo | {env['cargo'] or '-'} |",
        f"| CI | {env['ci']} |",
        "",
        "## 汇总",
        "",
        "| 项 | 结果 |",
        "| --- | --- |",
    ]
    if test_check:
        tests = test_check.get("tests", {})
        lines.append(f"| 单测 | {tests.get('passed', 0)} passed / {tests.get('failed', 0)} failed |")
    if probe_check:
        summary = probe_check.get("summary") or {}
        lines.append(f"| 探针 | {summary.get('pass', '-')} PASS / {summary.get('fail', '-')} FAIL / {summary.get('skip', '-')} SKIP |")
    lines.append(f"| 总体 | **{report['status'].upper()}** |")
    lines += ["", "## 检查明细", "", "| 检查 | 状态 | 耗时 (s) | 退出码 | 日志 |", "| --- | --- | --- | --- | --- |"]
    for check in checks:
        lines.append(
            f"| `{check['name']}` | {check['status']} | {check.get('duration_s', '-')} | "
            f"{check.get('exit', '-')} | `{check.get('log') or '-'}` |"
        )
    if probe_check and probe_check.get("probes"):
        lines += ["", "## 探针明细", "", "| 探针 | 结果 |", "| --- | --- |"]
        for probe in probe_check["probes"]:
            lines.append(f"| `{probe['name']}` | {probe['status']} |")
    lines += [
        "",
        "## 证据与复现",
        "",
        "- 证据索引：[docs/VERIFICATION.md](../../docs/VERIFICATION.md)",
        "- 项目状态：[docs/STATUS.md](../../docs/STATUS.md)",
        "- 复现：`python3 scripts/spikes/gen-test-report.py`（本机）/ CI `docs-and-probes` job（Linux 非 root UDS 探针）",
        "",
        "> 报告中的 SKIP 表示该环境不适用（例如非 root 跳过 TAP、Windows 跳过 UDS 探针），不代表失败。",
        "",
    ]
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="生成 EC-Sim JSON/Markdown 测试报告（零第三方依赖）")
    parser.add_argument("--out", default="target/test-reports", help="输出目录（默认 target/test-reports，已被 gitignore）")
    parser.add_argument("--skip-cargo", action="store_true", help="跳过 cargo fmt/clippy/test/doc")
    parser.add_argument("--skip-probes", action="store_true", help="跳过 run-all-probes.sh")
    parser.add_argument("--probes-log", default=None, help="复用已有探针日志（隐含不重跑探针）")
    parser.add_argument("--timeout", type=int, default=1800, help="单条命令超时秒数（默认 1800）")
    parser.add_argument("--version", action="version", version=f"{TOOL} (schema {SCHEMA})")
    args = parser.parse_args(argv)

    out_dir = (ROOT / args.out).resolve() if not Path(args.out).is_absolute() else Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    checks = build_checks(args, out_dir)
    statuses = [check["status"] for check in checks]
    status = "fail" if any(item in ("fail", "unknown") for item in statuses) else "pass"

    report = {
        "schema": SCHEMA,
        "generated_at": now_iso(),
        "tool": TOOL,
        "status": status,
        "git": git_info(),
        "environment": build_environment(),
        "checks": checks,
        "evidence": {
            "verification": "docs/VERIFICATION.md",
            "status": "docs/STATUS.md",
            "m3_exit_checklist": "docs/plan/M3-exit-checklist.md",
        },
    }
    # 分离内部字段（output 仅用于解析，不写入报告）
    for check in report["checks"]:
        check.pop("output", None)

    (out_dir / "report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / "report.md").write_text(markdown_report(report), encoding="utf-8")

    print(f"report: {out_dir / 'report.md'}")
    print(f"report: {out_dir / 'report.json'}")
    print(f"status: {status.upper()}")
    for check in report["checks"]:
        print(f"  {check['name']}: {check['status']} (exit={check.get('exit')})")
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
