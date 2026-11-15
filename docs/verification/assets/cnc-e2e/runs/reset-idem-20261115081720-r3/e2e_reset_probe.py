#!/usr/bin/env python3
"""A4：reset/stack-down 幂等探针。

`wait` 模式：用合成 UDS 主站把 N 个同画像从站从 INIT 拉到 OP，记录从
`--start-ms`（调用方在启动进程时取的 epoch 毫秒）到 3/3 OP 的耗时，并把
`/state` 快照写到 `--out`。

`compare` 模式：比较 3 次运行的 REF 字段（从站数、AL 状态/码、PDO 布局、
op_slaves、watchdog trips），字段级 diff 写入 `--out`，全部相等才返回 0。

退出码：0=OK，1=FAIL，2=参数/环境错误。
"""
import argparse
import json
import os
import struct
import sys
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import uds_master_probe as probe  # noqa: E402

CMD = probe.CMD
CMD["BWR"] = 0x08


def http_json(base, path):
    with urllib.request.urlopen(base.rstrip("/") + path, timeout=3.0) as response:
        return json.loads(response.read().decode("utf-8"))


def slaves_of(state_json):
    return state_json.get("slaves") or (state_json.get("master") or {}).get("slaves") or []


def broadcast_write(master, address, data, expect_wkc, what):
    reply = master.transact([(CMD["BWR"], 0, address, data)])
    wkc = reply[0][4]
    if wkc != expect_wkc:
        raise SystemExit(f"FAIL: {what} WKC {wkc} != {expect_wkc}")
    return reply


def wait_all_op(base, slave_count, timeout_s):
    deadline = time.time() + timeout_s
    last = {}
    while time.time() < deadline:
        last = http_json(base, "/state")
        listed = slaves_of(last)
        if (
            (last.get("master") or {}).get("op_slaves") == slave_count
            and len(listed) == slave_count
            and all(entry.get("op") is True for entry in listed)
        ):
            return last
        time.sleep(0.02)
    raise SystemExit(f"FAIL: {timeout_s:.1f}s 内未达到 {slave_count}/{slave_count} OP：{last}")


def write_json(path, payload):
    if not path:
        return
    os.makedirs(os.path.dirname(os.path.abspath(path)) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2, sort_keys=True)
        handle.write("\n")


def ref_view(state_json):
    listed = sorted(slaves_of(state_json), key=lambda entry: entry.get("index", 0))
    master = state_json.get("master") or {}
    return {
        "slave_count": len(listed),
        "op_slaves": master.get("op_slaves"),
        "al_state": master.get("al_state"),
        "pdo": master.get("pdo"),
        "watchdog_trips": (master.get("watchdog") or {}).get("trips"),
        "slaves": [
            {
                "index": entry.get("index"),
                "station_address": entry.get("station_address"),
                "al_code": entry.get("al_code"),
                "state": entry.get("state"),
                "op": entry.get("op"),
            }
            for entry in listed
        ],
    }


def wait_mode(args):
    started = args.start_ms / 1000.0
    if started <= 0:
        raise SystemExit("FAIL: --start-ms 必须为正的 epoch 毫秒")
    master = probe.SyntheticMaster(args.socket, timeout=args.timeout)
    try:
        mailbox_sms = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        broadcast_write(
            master,
            0x0800,
            b"".join(struct.pack("<HHBBBB", *entry) for entry in mailbox_sms),
            args.expect_slaves,
            "SM0/SM1 配置",
        )
        broadcast_write(master, 0x0120, bytes([0x02]), args.expect_slaves, "AL Control PREOP")
        process_sms = [(0x1100, 7, 0x04, 0, 1, 0), (0x1110, 11, 0x00, 0, 1, 0)]
        broadcast_write(
            master,
            0x0810,
            b"".join(struct.pack("<HHBBBB", *entry) for entry in process_sms),
            args.expect_slaves,
            "SM2/SM3 配置",
        )
        broadcast_write(master, 0x0120, bytes([0x04]), args.expect_slaves, "AL Control SAFEOP")
        broadcast_write(master, 0x0120, bytes([0x08]), args.expect_slaves, "AL Control OP")

        snapshot = wait_all_op(args.http_base, args.expect_slaves, args.timeout_s)
        elapsed_ms = int(time.time() * 1000 - args.start_ms)

        deadline = time.monotonic() + args.hold_seconds
        while time.monotonic() < deadline:
            broadcast_write(
                master,
                0x1100,
                struct.pack("<Hbi", 0x000F, 8, 0),
                args.expect_slaves,
                "SM2 process data",
            )
            time.sleep(args.interval_ms / 1000.0)

        final = http_json(args.http_base, "/state")
        view = ref_view(final)
        ok = (
            view["slave_count"] == args.expect_slaves
            and view["op_slaves"] == args.expect_slaves
            and view["watchdog_trips"] == 0
            and all(entry["op"] is True for entry in view["slaves"])
            and all(entry["al_code"] == "0x0000" for entry in view["slaves"])
            and (view["pdo"] or {}).get("tx") == 7
            and (view["pdo"] or {}).get("rx") == 11
        )
        payload = {
            "schema": "ec-sim/e2e-reset-v1",
            "run_id": final.get("run_id"),
            "elapsed_ms": elapsed_ms,
            "ok": ok,
            "ref": view,
        }
        write_json(args.out, payload)
        if not ok:
            raise SystemExit(f"FAIL: reset 后 REF 不满足：{json.dumps(view, ensure_ascii=False)}")
        print(
            "RESET_OK elapsed_ms=%d run_id=%s op_slaves=%s al_state=%s"
            % (elapsed_ms, final.get("run_id"), view["op_slaves"], view["al_state"])
        )
        return 0
    finally:
        master.close()


def compare_mode(args):
    states = []
    for path in args.compare:
        with open(path, "r", encoding="utf-8") as handle:
            states.append(ref_view(json.load(handle)))
    baseline = states[0]
    diffs = []
    for index, candidate in enumerate(states[1:], start=2):
        for key in ("slave_count", "op_slaves", "al_state", "pdo", "watchdog_trips", "slaves"):
            if candidate.get(key) != baseline.get(key):
                diffs.append({"run": index, "field": key, "expected": baseline.get(key), "actual": candidate.get(key)})
    payload = {
        "schema": "ec-sim/e2e-ref-compare-v1",
        "runs": len(states),
        "equal": not diffs,
        "diffs": diffs,
        "ref": baseline,
    }
    write_json(args.out, payload)
    if diffs:
        print(f"FAIL: REF 字段级 diff {len(diffs)} 项：{json.dumps(diffs, ensure_ascii=False)}")
        return 1
    print(f"REF_EQUAL runs={len(states)} fields=6")
    return 0


def main():
    parser = argparse.ArgumentParser(description="E2E reset 幂等探针")
    sub = parser.add_subparsers(dest="mode", required=True)

    wait = sub.add_parser("wait", help="拉起从站到 OP 并记录耗时")
    wait.add_argument("socket", nargs="?", default="/tmp/ecsim-reset.sock")
    wait.add_argument("--http-base", required=True)
    wait.add_argument("--start-ms", type=int, required=True)
    wait.add_argument("--expect-slaves", type=int, default=3)
    wait.add_argument("--timeout-s", type=float, default=30.0)
    wait.add_argument("--hold-seconds", type=float, default=0.5)
    wait.add_argument("--interval-ms", type=float, default=20.0)
    wait.add_argument("--timeout", type=float, default=2.0)
    wait.add_argument("--out")

    compare = sub.add_parser("compare", help="比较多次运行的 REF 字段")
    compare.add_argument("--compare", nargs="+", required=True)
    compare.add_argument("--out")

    args = parser.parse_args()
    if args.mode == "wait":
        return wait_mode(args)
    return compare_mode(args)


if __name__ == "__main__":
    sys.exit(main())
