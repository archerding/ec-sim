#!/usr/bin/env python3
"""EC-Sim CNC E2E 控制面契约探针（Linux，需 UDS + HTTP 同进程）。

验证 task-102/103 的最小闭环：
- `/state` 的 run_id / master / 逐从站字段
- `POST /fault` 的写门禁、confirm 二次确认、run_id 一致性、target 校验、幂等
- 故障注入/到期恢复与 `/events?since=` 事件（fault_injected / fault_cleared / link）
- `/probe` 的 tripped 设置/清除与事件
- 事件 seq 单调

由 `e2e-control-probe.sh` 启动 `ec-sim-esc --uds ... --serve ... --serve-write --run-id ...`
后调用；本脚本自带 keepalive 帧线程，保证 duration_ms 到期会被 tick 处理。

用法：
    python3 e2e_control_probe.py --http-base http://127.0.0.1:18133 \
        --socket /tmp/ecsim-e2e.sock --run-id e2e-probe-run --expect-slaves 3
"""

import argparse
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from uds_master_probe import CMD, SyntheticMaster  # noqa: E402

FAILURES = []

# 显式绕过环境代理：探针只访问本机 127.0.0.1（VM 上可能配置了全局代理）。
_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def check(condition, message):
    if condition:
        print(f"OK: {message}", flush=True)
    else:
        FAILURES.append(message)
        print(f"FAIL: {message}", flush=True)


def http_call(base, method, path, payload=None, timeout=5.0):
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(base + path, data=data, headers=headers, method=method)
    try:
        with _OPENER.open(request, timeout=timeout) as response:
            return response.status, response.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", "replace")


def post_json(base, path, payload):
    status, body = http_call(base, "POST", path, payload)
    try:
        return status, json.loads(body)
    except json.JSONDecodeError:
        return status, None


def get_json(base, path):
    status, body = http_call(base, "GET", path)
    try:
        return status, json.loads(body)
    except json.JSONDecodeError:
        return status, None


class Keepalive(threading.Thread):
    def __init__(self, socket_path, interval_ms=20):
        super().__init__(daemon=True)
        self.master = SyntheticMaster(socket_path, timeout=0.2)
        self.interval = interval_ms / 1000.0
        self.running = True
        self.frames = 0

    def run(self):
        while self.running:
            try:
                self.master.transact([(CMD["APRD"], 0, 0x0130, b"\x00\x00")])
                self.frames += 1
            except OSError:
                pass
            time.sleep(self.interval)

    def stop(self):
        self.running = False
        try:
            self.master.close()
        except OSError:
            pass


def wait_http(base, timeout=15.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            status, body = http_call(base, "GET", "/health", timeout=1.0)
            if status == 200 and "ec-sim-control-plane" in body:
                return True
        except OSError:
            pass
        time.sleep(0.1)
    return False


def wait_fault_cleared(base, timeout=5.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        status, body = get_json(base, "/fault/status")
        if status == 200 and body is not None and not body.get("active"):
            return body
        time.sleep(0.05)
    status, body = get_json(base, "/fault/status")
    return body


def events_since(base, since):
    status, body = get_json(base, f"/events?since={since}")
    check(status == 200, f"/events?since={since} 返回 200（实际 {status}）")
    if not isinstance(body, dict):
        return []
    events = body.get("events") or []
    return events


def event_obj(raw):
    return raw if isinstance(raw, dict) else json.loads(raw)


def state_slaves(state):
    """逐从站列表：优先顶层 `slaves`，兼容旧版嵌套在 `master.slaves`。"""
    if not isinstance(state, dict):
        return []
    return state.get("slaves") or (state.get("master") or {}).get("slaves") or []


def event_types(events):
    kinds = []
    for raw in events:
        try:
            event = raw if isinstance(raw, dict) else json.loads(raw)
            kinds.append(event.get("type"))
        except (json.JSONDecodeError, AttributeError):
            kinds.append(None)
    return kinds


def event_seqs(events):
    seqs = []
    for raw in events:
        try:
            event = raw if isinstance(raw, dict) else json.loads(raw)
            seqs.append(event.get("seq"))
        except (json.JSONDecodeError, AttributeError):
            seqs.append(None)
    return seqs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--http-base", default="http://127.0.0.1:18133")
    parser.add_argument("--socket", default="/tmp/ecsim-e2e.sock")
    parser.add_argument("--run-id", default="e2e-probe-run")
    parser.add_argument("--expect-slaves", type=int, default=3)
    args = parser.parse_args()
    base = args.http_base.rstrip("/")

    if not wait_http(base):
        print("FAIL: /health 在 15s 内未就绪", flush=True)
        return 1

    keepalive = Keepalive(args.socket)
    keepalive.start()
    time.sleep(0.2)

    try:
        # 1) /state 结构与 run_id
        status, state = get_json(base, "/state")
        check(status == 200 and isinstance(state, dict), "/state 返回 200 JSON")
        if isinstance(state, dict):
            check(state.get("run_id") == args.run_id, f"/state.run_id == {args.run_id}")
            master = state.get("master") or {}
            check(master.get("slave_count") == args.expect_slaves, "master.slave_count 正确")
            check("al_state" in master, "master.al_state 存在")
            check("watchdog" in master and "trips" in master["watchdog"], "master.watchdog.trips 存在")
            pdo = master.get("pdo") or {}
            for key in ("tx", "rx", "miss", "wkc_errors"):
                check(key in pdo, f"master.pdo.{key} 存在")
            slaves = state_slaves(state)
            check(len(slaves) == args.expect_slaves, "slaves 数量正确")
            if slaves:
                for key in (
                    "index",
                    "station_address",
                    "al_code",
                    "state",
                    "op",
                    "frames_in",
                    "frames_out",
                    "frames_dropped",
                    "datagrams_hit",
                    "wkc_errors",
                ):
                    check(key in slaves[0], f"slave[0].{key} 存在")

        # 2) /fault 门禁与校验
        status, _ = post_json(base, "/fault", {"fault": "link_loss", "duration_ms": 400})
        check(status == 400, f"缺 confirm → 400（实际 {status}）")

        status, _ = post_json(
            base,
            "/fault",
            {"fault": "link_loss", "duration_ms": 400, "confirm": True, "run_id": "wrong-run"},
        )
        check(status == 409, f"run_id 不匹配 → 409（实际 {status}）")

        status, _ = post_json(
            base, "/fault", {"fault": "bogus", "duration_ms": 10, "confirm": True}
        )
        check(status == 400, f"未知 fault → 400（实际 {status}）")

        status, _ = post_json(
            base,
            "/fault",
            {"fault": "al_error", "target": 99, "duration_ms": 10, "confirm": True},
        )
        check(status == 400, f"target 越界 → 400（实际 {status}）")

        # 3) link_loss 注入 → 状态/事件 → 幂等 → 到期恢复
        status, body = post_json(
            base,
            "/fault",
            {"fault": "link_loss", "duration_ms": 600, "confirm": True, "run_id": args.run_id},
        )
        check(status == 200 and isinstance(body, dict) and body.get("ok") is True, "link_loss 注入 200")

        status, fault_status = get_json(base, "/fault/status")
        active = (fault_status or {}).get("active") or []
        check(status == 200 and len(active) == 1, "/fault/status 显示 1 个 active fault")
        if active:
            check(active[0].get("fault") == "link_loss", "active fault == link_loss")
            check(active[0].get("remaining_ms", 0) > 0, "remaining_ms > 0")
            check((fault_status or {}).get("run_id") == args.run_id, "/fault/status.run_id 一致")

        status, body = post_json(
            base,
            "/fault",
            {"fault": "link_loss", "duration_ms": 600, "confirm": True, "run_id": args.run_id},
        )
        check(
            status == 200 and isinstance(body, dict) and body.get("idempotent") is True,
            "重复 link_loss 幂等（idempotent=true）",
        )

        events = events_since(base, 0)
        kinds = event_types(events)
        check("fault_injected" in kinds, "事件含 fault_injected")
        check("link" in kinds, "事件含 link（down）")
        seqs = event_seqs(events)
        check(
            all(isinstance(a, int) and isinstance(b, int) and a < b for a, b in zip(seqs, seqs[1:])),
            "事件 seq 严格单调递增",
        )
        injected = next(
            (event_obj(raw) for raw in events if event_obj(raw).get("type") == "fault_injected"),
            {},
        )
        check(
            injected.get("run_id") == args.run_id,
            "fault_injected.run_id == --run-id",
        )
        check(
            "\"who\": \"http\"" in json.dumps(injected.get("details") or {})
            or (injected.get("details") or {}).get("who") == "http",
            "fault_injected 审计含 who=http",
        )
        check(
            isinstance(injected.get("details"), dict)
            and "before" in injected["details"]
            and "after" in injected["details"],
            "fault_injected 审计含 before/after",
        )

        before = len(events)
        wait_fault_cleared(base, 5.0)
        cleared_events = events_since(base, 0)
        new_kinds = event_types(cleared_events[before:])
        check("fault_cleared" in new_kinds, "到期后事件含 fault_cleared")
        check("link" in new_kinds, "到期后事件含 link（up）")

        status, fault_status = get_json(base, "/fault/status")
        check(status == 200 and not (fault_status or {}).get("active"), "到期后 /fault/status 无 active")

        # 4) al_error 单站注入（P1）→ /state AL 码 → 到期清除
        status, body = post_json(
            base,
            "/fault",
            {
                "fault": "al_error",
                "target": 1,
                "code": "0x0011",
                "duration_ms": 700,
                "confirm": True,
                "run_id": args.run_id,
            },
        )
        check(status == 200 and isinstance(body, dict) and body.get("ok") is True, "al_error 注入 200")

        status, state = get_json(base, "/state")
        slaves = state_slaves(state)
        target_slave = slaves[1] if len(slaves) > 1 else {}
        check(
            target_slave.get("al_code") == "0x0011",
            f"注入后 slave[1].al_code == 0x0011（实际 {target_slave.get('al_code')}）",
        )
        check(target_slave.get("op") is False, "注入后 slave[1].op == false")
        status_value = int(str(target_slave.get("state", "0x0")), 0)
        check(bool(status_value & 0x10), "注入后 state error 位有效")

        wait_fault_cleared(base, 5.0)
        status, state = get_json(base, "/state")
        slaves = state_slaves(state)
        target_slave = slaves[1] if len(slaves) > 1 else {}
        check(
            target_slave.get("al_code") == "0x0000",
            f"恢复后 slave[1].al_code == 0x0000（实际 {target_slave.get('al_code')}）",
        )

        # 5) /probe tripped 设置/清除 + 事件
        status, _ = post_json(base, "/probe", {"tripped": True, "target": 2})
        check(status == 400, f"probe 缺 confirm → 400（实际 {status}）")

        status, _ = post_json(
            base,
            "/probe",
            {"tripped": True, "target": 2, "confirm": True, "run_id": "wrong-run"},
        )
        check(status == 409, f"probe run_id 不匹配 → 409（实际 {status}）")

        status, body = post_json(
            base,
            "/probe",
            {"tripped": True, "target": 2, "confirm": True, "run_id": args.run_id},
        )
        check(status == 200 and isinstance(body, dict) and body.get("tripped") is True, "probe 置位 200")

        status, probe = get_json(base, "/probe")
        entries = (probe or {}).get("probe") or []
        tripped = {entry.get("target"): entry.get("tripped") for entry in entries}
        check(status == 200 and tripped.get(2) is True, "/probe 显示 target=2 tripped=true")
        check((probe or {}).get("run_id") == args.run_id, "/probe.run_id 一致")

        status, body = post_json(
            base,
            "/probe",
            {"tripped": False, "target": 2, "confirm": True, "run_id": args.run_id},
        )
        check(status == 200 and isinstance(body, dict) and body.get("tripped") is False, "probe 清除 200")

        events = events_since(base, 0)
        probe_events = [
            event_obj(raw) for raw in events if event_obj(raw).get("type") == "probe_state"
        ]
        check(len(probe_events) >= 2, "probe_state 事件记录置位+清除")
        check(
            all(event.get("run_id") == args.run_id for event in probe_events),
            "probe_state 事件 run_id 一致",
        )

        # 6) 事件环 since 过滤
        status, state = get_json(base, "/state")
        status, all_events = get_json(base, "/events?since=0")
        all_list = (all_events or {}).get("events") or []
        last_seq = event_seqs(all_list)[-1] if all_list else 0
        check(isinstance(last_seq, int), "已产生 seq")
        status, tail = get_json(base, f"/events?since={last_seq}")
        tail_list = (tail or {}).get("events") or []
        check(
            all(event_obj(raw).get("seq") > last_seq for raw in tail_list),
            "/events?since 只返回更大 seq",
        )
    finally:
        keepalive.stop()

    print(f"KEEPALIVE frames={keepalive.frames}", flush=True)
    if FAILURES:
        print(f"RESULT: FAIL ({len(FAILURES)} 项)", flush=True)
        return 1
    print("RESULT: PASS", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
