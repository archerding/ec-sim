#!/usr/bin/env python3
"""E2E 故障矩阵探针（task-102）：slave_drop/重连、SDO 拒绝/恢复。

前提：`ec-sim-esc --uds <sock> --slaves 3 --manifest plugins/cnc-csp-servo/plugin.yaml
--behavior cia402 --serve 127.0.0.1:<port> --serve-write --run-id <id>` 已启动。

覆盖（P0）：
1. `slave_drop`（target=1，1.5 s）：注入 → `/state.slaves[1].frames_dropped` 增长、
   `fault_injected` 事件；到期 → 帧恢复、`fault_cleared`、`/fault/status` 清空。
2. `sdo_reject`（target=0，1.5 s）：注入后 SDO upload 0x1018:01 返回 Abort
   0x06010000；到期后同一请求正常返回；事件成对。
3. `pdo_corrupt`（target=0，1.5 s）：注入后 TxPDO 逐字节 XOR 0xA5（确定性、不累积）；
   审计含 `pdo.before_hex`/`pdo.xor`；到期后恢复原值；事件成对。

退出码：0=PASS，1=FAIL，2=环境错误。
"""
import argparse
import json
import os
import struct
import sys
import threading
import time
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import uds_master_probe as probe  # noqa: E402

CMD = probe.CMD
CMD["BWR"] = 0x08  # 广播写；BRD（0x07）是广播读
SDO_ABORT_UNSUPPORTED_ACCESS = 0x0601_0000


def http_json(base, path, method="GET", payload=None):
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(base.rstrip("/") + path, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=3.0) as response:
            return response.status, json.loads(response.read().decode("utf-8") or "{}")
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", "replace")
        try:
            return error.code, json.loads(body or "{}")
        except json.JSONDecodeError:
            return error.code, {"raw": body}


def slaves_of(state):
    return state.get("slaves") or (state.get("master") or {}).get("slaves") or []


def broadcast_write(master, address, data, expect_wkc=None):
    reply = master.transact([(CMD["BWR"], 0, address, data)])
    wkc = reply[0][4]
    if expect_wkc is not None and wkc != expect_wkc:
        raise SystemExit(f"FAIL: BWR 0x{address:04X} WKC {wkc} != {expect_wkc}")
    return wkc


def wait_all_op(base, slave_count, timeout_s):
    deadline = time.time() + timeout_s
    last = {}
    while time.time() < deadline:
        _, last = http_json(base, "/state")
        listed = slaves_of(last)
        if (
            (last.get("master") or {}).get("op_slaves") == slave_count
            and len(listed) == slave_count
            and all(entry.get("op") is True for entry in listed)
        ):
            return last
        time.sleep(0.05)
    raise SystemExit(f"FAIL: {timeout_s:.1f}s 内未达到 {slave_count}/{slave_count} OP：{last}")


def sdo_upload(master, index, subindex):
    service = struct.pack("<HBHB", 0x02 << 12, 0x40, index, subindex)
    return probe.mailbox_exchange(master, service)


def read_tx_pdo(master, length=11):
    """APRD 读自动增量首站（slave 0）的 TxPDO（SM3 @ 0x1110）。"""
    reply = master.transact([(CMD["APRD"], 0, 0x1110, bytes(length))])
    if reply[0][4] != 1:
        raise SystemExit(f"FAIL: APRD 0x1110 WKC {reply[0][4]} != 1")
    return reply[0][3][:length]


def wait_fault_cleared(base, kind, timeout_s):
    deadline = time.time() + timeout_s
    status = {}
    while time.time() < deadline:
        _, status = http_json(base, "/fault/status")
        active = status.get("active") or []
        if not any(item.get("fault") == kind for item in active):
            return status
        time.sleep(0.1)
    raise SystemExit(f"FAIL: {timeout_s:.1f}s 内 {kind} 未到期清理：{status}")


def events_since(base, since):
    status, body = http_json(base, f"/events?since={since}")
    if status != 200:
        raise SystemExit(f"FAIL: /events?since={since} -> {status} {body}")
    return body.get("events") or []


def check(name, condition, detail=""):
    if condition:
        print(f"OK   {name}")
        return 0
    print(f"FAIL {name} {detail}")
    return 1


def main():
    parser = argparse.ArgumentParser(description="CNC E2E fault matrix probe")
    parser.add_argument("socket_path", nargs="?", default="/tmp/ecsim-fault-matrix.sock")
    parser.add_argument("--http-base", required=True)
    parser.add_argument("--out-dir", default="")
    args = parser.parse_args()
    failures = 0
    out_dir = args.out_dir
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    master = probe.SyntheticMaster(args.socket_path, timeout=2.0)
    # keepalive 线程与主线程共用同一 socket：所有 transact 串行化。
    transact_lock = threading.Lock()
    original_transact = master.transact

    def locked_transact(datagrams):
        with transact_lock:
            return original_transact(datagrams)

    master.transact = locked_transact
    stop = threading.Event()

    def keepalive():
        # 固定 target=0 使 CSP 的 TxPDO（状态字/实际位置等）保持稳定，便于 pdo_corrupt 逐字节断言；
        # slave_drop / sdo_reject 只依赖帧计数与邮箱，不受 target 变化影响。
        while not stop.is_set():
            try:
                broadcast_write(master, 0x1100, struct.pack("<Hbi", 0x000F, 8, 0))
            except Exception:
                pass
            time.sleep(0.02)

    try:
        # ESM：INIT → PREOP → SAFEOP → OP（3 站同画像，广播写）。
        sm_mailbox = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        broadcast_write(master, 0x0800, b"".join(struct.pack("<HHBBBB", *e) for e in sm_mailbox), 3)
        broadcast_write(master, 0x0120, bytes([0x02]), 3)
        sm_process = [(0x1100, 7, 0x04, 0, 1, 0), (0x1110, 11, 0x00, 0, 1, 0)]
        broadcast_write(master, 0x0810, b"".join(struct.pack("<HHBBBB", *e) for e in sm_process), 3)
        broadcast_write(master, 0x0120, bytes([0x04]), 3)
        broadcast_write(master, 0x0120, bytes([0x08]), 3)
        op_state = wait_all_op(args.http_base, 3, 3.0)
        run_id = op_state.get("run_id")
        failures += check("初始 3/3 OP", len(slaves_of(op_state)) == 3 and run_id, f"state={op_state}")
        print(f"RUN_ID {run_id}")

        thread = threading.Thread(target=keepalive, daemon=True)
        thread.start()
        time.sleep(0.3)
        _, seq_before = http_json(args.http_base, "/events?since=0")
        since = max((event.get("seq", 0) for event in (seq_before.get("events") or [])), default=0)

        # ---- 1) slave_drop / 重连 ----
        _, before = http_json(args.http_base, "/state")
        dropped_before = {entry["index"]: entry.get("frames_dropped", 0) for entry in slaves_of(before)}
        status, body = http_json(
            args.http_base,
            "/fault",
            "POST",
            {"fault": "slave_drop", "target": 1, "duration_ms": 1500, "confirm": True, "run_id": run_id},
        )
        failures += check("slave_drop 注入 200", status == 200 and body.get("ok") is True, f"{status} {body}")
        time.sleep(0.35)
        _, during = http_json(args.http_base, "/state")
        frames_during = {entry["index"]: entry["frames_in"] for entry in slaves_of(during)}
        dropped_during = {entry["index"]: entry.get("frames_dropped", 0) for entry in slaves_of(during)}
        time.sleep(0.35)
        _, during2 = http_json(args.http_base, "/state")
        frames_during2 = {entry["index"]: entry["frames_in"] for entry in slaves_of(during2)}
        dropped_during2 = {entry["index"]: entry.get("frames_dropped", 0) for entry in slaves_of(during2)}
        # 掉线从站仍物理收帧（环网旁路），但每帧计 frames_dropped；其他站正常处理。
        dropped_growing = dropped_during2[1] > dropped_during[1]
        others_moving = frames_during2[0] > frames_during[0] and frames_during2[2] > frames_during[2]
        failures += check(
            "slave_drop 期间目标站帧计为丢弃",
            dropped_growing,
            f"dropped {dropped_during} -> {dropped_during2}（注入前 {dropped_before}）",
        )
        failures += check("slave_drop 期间其他站仍收帧", others_moving, f"{frames_during} -> {frames_during2}")
        events = events_since(args.http_base, since)
        inject_event = [event for event in events if event.get("type") == "fault_injected"]
        failures += check("fault_injected 事件", len(inject_event) == 1, f"events={events}")
        cleared = wait_fault_cleared(args.http_base, "slave_drop", 5.0)
        failures += check("slave_drop 到期清理", cleared.get("active") == [], f"{cleared}")
        time.sleep(0.35)
        _, after = http_json(args.http_base, "/state")
        frames_after = {entry["index"]: entry["frames_in"] for entry in slaves_of(after)}
        failures += check("slave_drop 恢复后帧增长", frames_after[1] > frames_during2[1], f"{frames_during2} -> {frames_after}")
        events = events_since(args.http_base, since)
        failures += check(
            "fault_cleared 事件",
            any(event.get("type") == "fault_cleared" and (event.get("details") or {}).get("fault") == "slave_drop" for event in events),
            f"events={events}",
        )
        if out_dir:
            with open(os.path.join(out_dir, "slave-drop-timeline.json"), "w", encoding="utf-8") as handle:
                json.dump({"before": before, "during": during, "during2": during2, "after": after, "events": events}, handle, ensure_ascii=False, indent=2)

        # ---- 2) sdo_reject / 恢复 ----
        response = sdo_upload(master, 0x1018, 0x01)
        failures += check("SDO 注入前正常应答", len(response) >= 9 and response[2] == 0x43, f"response={response.hex()}")
        status, body = http_json(
            args.http_base,
            "/fault",
            "POST",
            {"fault": "sdo_reject", "target": 0, "duration_ms": 1500, "confirm": True, "run_id": run_id},
        )
        failures += check("sdo_reject 注入 200", status == 200 and body.get("ok") is True, f"{status} {body}")
        response = sdo_upload(master, 0x1018, 0x01)
        abort_code = struct.unpack("<I", response[6:10])[0] if len(response) >= 10 else None
        failures += check(
            "注入后 SDO 返回 Abort 0x06010000",
            len(response) >= 10 and response[2] == 0x80 and abort_code == SDO_ABORT_UNSUPPORTED_ACCESS,
            f"response={response.hex()}",
        )
        cleared = wait_fault_cleared(args.http_base, "sdo_reject", 5.0)
        failures += check("sdo_reject 到期清理", cleared.get("active") == [], f"{cleared}")
        response = sdo_upload(master, 0x1018, 0x01)
        failures += check("恢复后 SDO 正常应答", len(response) >= 9 and response[2] == 0x43, f"response={response.hex()}")
        events = events_since(args.http_base, since)
        kinds = [(event.get("type"), (event.get("details") or {}).get("fault")) for event in events]
        failures += check(
            "sdo_reject 事件成对",
            ("fault_injected", "sdo_reject") in kinds and ("fault_cleared", "sdo_reject") in kinds,
            f"kinds={kinds}",
        )
        if out_dir:
            with open(os.path.join(out_dir, "sdo-reject-timeline.json"), "w", encoding="utf-8") as handle:
                json.dump({"events": events, "abort_code": abort_code}, handle, ensure_ascii=False, indent=2)

        # ---- 3) pdo_corrupt / 恢复（F-PROC：过程数据确定性错误）----
        normal = read_tx_pdo(master)
        failures += check("pdo_corrupt 注入前 TxPDO 可读", len(normal) == 11, f"tx={normal.hex()}")
        status, body = http_json(
            args.http_base,
            "/fault",
            "POST",
            {"fault": "pdo_corrupt", "target": 0, "duration_ms": 1500, "confirm": True, "run_id": run_id},
        )
        failures += check("pdo_corrupt 注入 200", status == 200 and body.get("ok") is True, f"{status} {body}")
        audit = body.get("audit") or body
        pdo_audit = audit.get("pdo") or {}
        failures += check(
            "pdo_corrupt 审计含前值与 XOR",
            pdo_audit.get("before_hex") == normal.hex() and pdo_audit.get("xor") in ("a5", "0xa5"),
            f"pdo_audit={pdo_audit} normal={normal.hex()}",
        )
        time.sleep(0.2)
        corrupted = read_tx_pdo(master)
        expected = bytes(byte ^ 0xA5 for byte in normal)
        failures += check(
            "pdo_corrupt 期间 TxPDO 逐字节 XOR 0xA5",
            corrupted == expected,
            f"normal={normal.hex()} corrupted={corrupted.hex()} expected={expected.hex()}",
        )
        time.sleep(0.3)
        corrupted_again = read_tx_pdo(master)
        failures += check("pdo_corrupt 结果确定（不累积翻转）", corrupted_again == corrupted, f"{corrupted.hex()} -> {corrupted_again.hex()}")
        cleared = wait_fault_cleared(args.http_base, "pdo_corrupt", 5.0)
        failures += check("pdo_corrupt 到期清理", cleared.get("active") == [], f"{cleared}")
        restored = read_tx_pdo(master)
        failures += check("pdo_corrupt 恢复后 TxPDO 还原", restored == normal, f"normal={normal.hex()} restored={restored.hex()}")
        events = events_since(args.http_base, since)
        kinds = [(event.get("type"), (event.get("details") or {}).get("fault")) for event in events]
        failures += check(
            "pdo_corrupt 事件成对",
            ("fault_injected", "pdo_corrupt") in kinds and ("fault_cleared", "pdo_corrupt") in kinds,
            f"kinds={kinds}",
        )
        if out_dir:
            with open(os.path.join(out_dir, "pdo-corrupt-timeline.json"), "w", encoding="utf-8") as handle:
                json.dump(
                    {
                        "normal_hex": normal.hex(),
                        "corrupted_hex": corrupted.hex(),
                        "restored_hex": restored.hex(),
                        "audit": body,
                        "events": events,
                    },
                    handle,
                    ensure_ascii=False,
                    indent=2,
                )

        _, final = http_json(args.http_base, "/state")
        failures += check("矩阵结束 3/3 OP", (final.get("master") or {}).get("op_slaves") == 3, f"master={final.get('master')}")
    finally:
        stop.set()
        time.sleep(0.05)
        master.close()

    print(f"RESULT: {'PASS' if failures == 0 else 'FAIL'}（{failures} 个失败）")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
