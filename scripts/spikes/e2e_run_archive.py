#!/usr/bin/env python3
"""EC-Sim E2E 证据归档器（task-103 C3 / task-101 A7）。

把一次运行产出的证据按 `runs/<run_id>/` 归档，保证：
- run.json：run_id、commit、binary/manifest sha256、命令、结果、文件 sha256/大小；
- README.md：人读摘要（运行、结果、边界）；
- sha256sums.txt：可独立校验；
- verify：重算 sha256 并核对 run.json。

用法：
  python3 e2e_run_archive.py create --out-root <dir> --run-id <id> \\
      --meta result=PASS --meta commit=<sha> --meta binary_sha256=<sha> \\
      --meta manifest_sha256=<sha> --meta command='<cmd>' \\
      --artifact <file-or-dir> [--artifact ...]
  python3 e2e_run_archive.py verify --dir <out-root>/runs/<id>

退出码：0=PASS，1=FAIL/校验失败，2=参数错误。
"""
import argparse
import datetime as _dt
import hashlib
import json
import os
import shutil
import sys


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 16), b""):
            digest.update(chunk)
    return digest.hexdigest()


def collect_files(artifacts):
    files = []
    for artifact in artifacts:
        if os.path.isdir(artifact):
            for root, _dirs, names in os.walk(artifact):
                for name in sorted(names):
                    files.append(os.path.join(root, name))
        elif os.path.isfile(artifact):
            files.append(artifact)
        else:
            raise SystemExit(f"FAIL: artifact 不存在：{artifact}")
    unique = []
    seen = set()
    for path in files:
        absolute = os.path.abspath(path)
        if absolute not in seen:
            seen.add(absolute)
            unique.append(absolute)
    return unique


def create(args):
    run_dir = os.path.join(args.out_root, "runs", args.run_id)
    if os.path.isdir(run_dir) and os.listdir(run_dir):
        raise SystemExit(f"FAIL: 目标已存在且非空：{run_dir}")
    os.makedirs(run_dir, exist_ok=True)
    meta = {}
    for item in args.meta:
        key, sep, value = item.partition("=")
        if not sep or not key:
            raise SystemExit(f"FAIL: --meta 需要 key=value：{item}")
        meta[key] = value
    meta.setdefault("result", "PENDING")
    meta.setdefault("command", "")
    meta.setdefault("commit", "")
    meta.setdefault("binary_sha256", "")
    meta.setdefault("manifest_sha256", "")
    files = collect_files(args.artifact)
    if not files:
        raise SystemExit("FAIL: 至少需要一个 --artifact")

    entries = []
    for source in files:
        name = os.path.basename(source)
        target_name = name
        suffix = 1
        while os.path.exists(os.path.join(run_dir, target_name)):
            target_name = f"{name}.{suffix}"
            suffix += 1
        target = os.path.join(run_dir, target_name)
        shutil.copy2(source, target)
        entries.append({"file": target_name, "bytes": os.path.getsize(target), "sha256": sha256_file(target)})

    run = {
        "schema": "ec-sim/e2e-run-v1",
        "run_id": args.run_id,
        "created_at": _dt.datetime.now(_dt.timezone.utc).astimezone().isoformat(timespec="seconds"),
        "result": meta["result"],
        "command": meta["command"],
        "commit": meta["commit"],
        "binary_sha256": meta["binary_sha256"],
        "manifest_sha256": meta["manifest_sha256"],
        "meta": {key: value for key, value in meta.items() if key not in {"result", "command", "commit", "binary_sha256", "manifest_sha256"}},
        "files": entries,
    }
    with open(os.path.join(run_dir, "run.json"), "w", encoding="utf-8") as handle:
        json.dump(run, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    with open(os.path.join(run_dir, "sha256sums.txt"), "w", encoding="utf-8") as handle:
        for entry in entries:
            handle.write(f"{entry['sha256']}  {entry['file']}\n")
        handle.write(f"{sha256_file(os.path.join(run_dir, 'run.json'))}  run.json\n")
    with open(os.path.join(run_dir, "README.md"), "w", encoding="utf-8") as handle:
        handle.write(f"# E2E run `{args.run_id}`\n\n")
        handle.write(f"- 结果：**{meta['result']}**\n")
        handle.write(f"- 创建：{run['created_at']}\n")
        handle.write(f"- commit：`{meta['commit'] or 'n/a'}`\n")
        handle.write(f"- 二进制 sha256：`{meta['binary_sha256'] or 'n/a'}`\n")
        handle.write(f"- manifest sha256：`{meta['manifest_sha256'] or 'n/a'}`\n")
        handle.write(f"- 命令：`{meta['command'] or 'n/a'}`\n\n")
        handle.write("## 文件\n\n| 文件 | 字节 | sha256 |\n| --- | --- | --- |\n")
        for entry in entries:
            handle.write(f"| `{entry['file']}` | {entry['bytes']} | `{entry['sha256']}` |\n")
        handle.write("\n## 边界\n\n")
        handle.write("- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。\n")
        handle.write("- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。\n")
        handle.write("- `sha256sums.txt` 可用于独立校验。\n")
    print(f"ARCHIVED {run_dir} files={len(entries)} result={meta['result']}")
    return 0


def verify(args):
    run_dir = args.dir
    run_path = os.path.join(run_dir, "run.json")
    if not os.path.isfile(run_path):
        raise SystemExit(f"FAIL: 缺少 run.json：{run_dir}")
    with open(run_path, encoding="utf-8") as handle:
        run = json.load(handle)
    failures = 0
    if run.get("schema") != "ec-sim/e2e-run-v1":
        print(f"FAIL schema {run.get('schema')}")
        failures += 1
    if not run.get("run_id"):
        print("FAIL 缺 run_id")
        failures += 1
    listed = set()
    for entry in run.get("files") or []:
        path = os.path.join(run_dir, entry["file"])
        listed.add(entry["file"])
        if not os.path.isfile(path):
            print(f"FAIL 缺文件 {entry['file']}")
            failures += 1
            continue
        digest = sha256_file(path)
        if digest != entry["sha256"]:
            print(f"FAIL hash {entry['file']}: {digest} != {entry['sha256']}")
            failures += 1
    for name in os.listdir(run_dir):
        if name in {"run.json", "sha256sums.txt", "README.md"}:
            continue
        if name not in listed:
            print(f"FAIL 未登记文件 {name}")
            failures += 1
    sums_path = os.path.join(run_dir, "sha256sums.txt")
    if os.path.isfile(sums_path):
        with open(sums_path, encoding="utf-8") as handle:
            for line in handle:
                parts = line.strip().split("  ", 1)
                if len(parts) != 2:
                    print(f"FAIL sha256sums 行非法：{line!r}")
                    failures += 1
                    continue
                digest, name = parts
                path = os.path.join(run_dir, name)
                if not os.path.isfile(path):
                    print(f"FAIL sha256sums 缺文件 {name}")
                    failures += 1
                elif sha256_file(path) != digest:
                    print(f"FAIL sha256sums hash {name}")
                    failures += 1
    else:
        print("FAIL 缺 sha256sums.txt")
        failures += 1
    if failures:
        print(f"RESULT: FAIL（{failures} 个失败）")
        return 1
    print(f"RESULT: PASS（{len(listed)} 个文件校验通过）")
    return 0


def main():
    parser = argparse.ArgumentParser(description="EC-Sim E2E evidence run archiver")
    sub = parser.add_subparsers(dest="command", required=True)
    create_parser = sub.add_parser("create")
    create_parser.add_argument("--out-root", required=True)
    create_parser.add_argument("--run-id", required=True)
    create_parser.add_argument("--meta", action="append", default=[])
    create_parser.add_argument("--artifact", action="append", default=[])
    create_parser.set_defaults(func=create)
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--dir", required=True)
    verify_parser.set_defaults(func=verify)
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
