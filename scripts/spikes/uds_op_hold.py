#!/usr/bin/env python3
"""UDS 合成主站：把 N 个同画像从站拉到 OP 并保持一段时间的周期过程数据。

用途：task-101 固定场景（3×CiA402 CSP）在 EC-Sim VM 上做「无 IgH/无 lcec」的
EC-Sim 侧自证：证明从站状态机、SM 配置、PDO 布局与 `/state` 遥测在连续周期帧下
可稳定保持 OP。**不替代** CNC VM 上 IgH+lcec 的端到端验收。

帧用法与 `uds_master_probe.py` 一致（SOCK_DGRAM，一报文一帧）。因为 3 个从站
使用同一 manifest，SM/控制字/PDO 写入用广播写（BWR）即可同时应用；逐站状态统一
从 `--serve` 的 `/state` 读取，避免广播读的「最后一个从站覆盖」歧义。

退出码：0=PASS（保持期内每站 OP、WKC 正常），1=FAIL，2=参数/环境错误。
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
CMD["BWR"] = 0x08  # 广播写；BRD（0x07）是广播读，不能用于配置/控制写


def http_json(base, path):
    with urllib.request.urlopen(base.rstrip("/") + path, timeout=2.0) as response:
        return json.loads(response.read().decode("utf-8"))


def state(base):
    return http_json(base, "/state")


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
        last = state(base)
        listed = slaves_of(last)
        if (
            (last.get("master") or {}).get("op_slaves") == slave_count
            and len(listed) == slave_count
            and all(entry.get("op") is True for entry in listed)
        ):
            return last
        time.sleep(0.05)
    raise SystemExit(f"FAIL: {timeout_s:.1f}s 内未达到 {slave_count}/{slave_count} OP：{last}")


def main():
    parser = argparse.ArgumentParser(description="UDS OP hold master")
    parser.add_argument("socket_path", nargs="?", default="/tmp/ecsim-fixed-scene.sock")
    parser.add_argument("--http-base", required=True)
    parser.add_argument("--expect-slaves", type=int, default=3)
    parser.add_argument("--hold-seconds", type=float, default=20.0)
    parser.add_argument("--interval-ms", type=float, default=20.0)
    parser.add_argument("--timeout", type=float, default=2.0)
    args = parser.parse_args()

    expect = args.expect_slaves
    master = probe.SyntheticMaster(args.socket_path, timeout=args.timeout)
    samples = 0
    try:
        # 1) 邮箱 SM（INIT 下可写）→ PREOP。
        mailbox_sms = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        broadcast_write(
            master,
            0x0800,
            b"".join(struct.pack("<HHBBBB", *entry) for entry in mailbox_sms),
            expect,
            "SM0/SM1 配置",
        )
        broadcast_write(master, 0x0120, bytes([0x02]), expect, "AL Control PREOP")
        preop_state = state(args.http_base)
        listed = slaves_of(preop_state)
        if len(listed) != expect or any(
            (int(str(entry.get("state", "0x0")), 0) & 0x0F) != 0x02 for entry in listed
        ):
            raise SystemExit(f"FAIL: PREOP 未生效：{listed}")

        # 2) 过程数据 SM（cnc-csp-servo：SM2 7B out、SM3 11B in）→ SAFEOP → OP。
        process_sms = [(0x1100, 7, 0x04, 0, 1, 0), (0x1110, 11, 0x00, 0, 1, 0)]
        broadcast_write(
            master,
            0x0810,
            b"".join(struct.pack("<HHBBBB", *entry) for entry in process_sms),
            expect,
            "SM2/SM3 配置",
        )
        broadcast_write(master, 0x0120, bytes([0x04]), expect, "AL Control SAFEOP")
        broadcast_write(master, 0x0120, bytes([0x08]), expect, "AL Control OP")
        op_state = wait_all_op(args.http_base, expect, 3.0)

        master_pdo = (op_state.get("master") or {}).get("pdo") or {}
        if master_pdo.get("tx") != 7 or master_pdo.get("rx") != 11:
            raise SystemExit(f"FAIL: PDO 布局不是 7B/11B：{master_pdo}")

        started = time.monotonic()
        next_report = started + 5.0
        target = 0
        while time.monotonic() - started < args.hold_seconds:
            payload = struct.pack("<Hbi", 0x000F, 8, target)
            broadcast_write(master, 0x1100, payload, expect, "SM2 process data")
            samples += 1
            target = (target + 100) % 100000
            if time.monotonic() >= next_report:
                snap = state(args.http_base)
                listed = slaves_of(snap)
                if (snap.get("master") or {}).get("op_slaves") != expect or any(
                    entry.get("op") is not True for entry in listed
                ):
                    raise SystemExit(f"FAIL: 保持期内掉出 OP：{snap}")
                if ((snap.get("master") or {}).get("watchdog") or {}).get("trips", 0) != 0:
                    raise SystemExit(f"FAIL: 非预期看门狗触发：{snap}")
                next_report += 5.0
            time.sleep(args.interval_ms / 1000.0)

        final = state(args.http_base)
        listed = slaves_of(final)
        codes = {entry.get("al_code") for entry in listed}
        ok = (
            (final.get("master") or {}).get("op_slaves") == expect
            and len(listed) == expect
            and all(entry.get("op") is True for entry in listed)
            and len(codes) == 1
            and next(iter(codes)) == "0x0000"
        )
        print(
            "HOLD_OK samples=%d slaves=%d run_id=%s al_state=%s"
            % (samples, expect, final.get("run_id"), (final.get("master") or {}).get("al_state"))
        )
        if not ok:
            raise SystemExit(f"FAIL: 结束时状态不满足 OP/AL 一致：{final}")
    finally:
        master.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
