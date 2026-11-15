#!/usr/bin/env python3
"""C4 用例×SIM 可观察量矩阵探针（task-103 C4）。

覆盖 10 类 CNC E2E 用例域，每类给出 SIM 侧可观察量与采集证据：
program / jog / MDI / home / override / tool / offset / probe / alarm / link-loss。

说明：这是 **SIM 可观测等价路径**（合成主站驱动 3×cnc-csp-servo，读 /state、TxPDO、
/events、/metrics），不代表 CNC 侧 HMI/桥的功能验收；CNC 侧证据仍 PENDING（D1–D3）。

前提：`ec-sim-esc --uds <sock> --slaves 3 --manifest plugins/cnc-csp-servo/plugin.yaml
--behavior cia402 --serve 127.0.0.1:<port> --serve-write --run-id <id>` 已启动。

输出：`--out-dir/<class>/{stimulus.json,samples.json,state-before.json,state-after.json,
events.jsonl,result.json}` + `c4-matrix-summary.json`。

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

CMD = dict(probe.CMD)
CMD["BWR"] = 0x08

STATUS_TARGET_REACHED = 0x0400
STATUS_HOMING_ATTAINED = 0x1000

TXPDO_LEN = 11


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


def sdo_download_i32(master, index, subindex, value):
    service = struct.pack("<HBHBi", 0x02 << 12, 0x23, index, subindex, value)
    return probe.mailbox_exchange(master, service)


def read_tx_pdo(master, length=TXPDO_LEN):
    reply = master.transact([(CMD["APRD"], 0, 0x1110, bytes(length))])
    if reply[0][4] != 1:
        raise SystemExit(f"FAIL: APRD 0x1110 WKC {reply[0][4]} != 1")
    return reply[0][3][:length]


def decode_tx(raw):
    statusword = struct.unpack("<H", raw[0:2])[0]
    position = struct.unpack("<i", raw[2:6])[0]
    velocity = struct.unpack("<i", raw[6:10])[0]
    mode = struct.unpack("<b", raw[10:11])[0]
    return {"statusword": statusword, "position": position, "velocity": velocity, "mode": mode}


def events_since(base, since):
    status, body = http_json(base, f"/events?since={since}")
    if status != 200:
        raise SystemExit(f"FAIL: /events?since={since} -> {status} {body}")
    return body.get("events") or []


def write_json(path, payload):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)
        handle.write("\n")


class Stimulus:
    """keepalive 线程按当前命令持续广播写 SM2，主线程可随时改命令。"""

    def __init__(self, master, log):
        self.master = master
        self.log = log
        self.lock = threading.Lock()
        self.cw = 0x0000
        self.mode = 8
        self.target = 0
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)

    def set(self, cw=None, mode=None, target=None):
        with self.lock:
            if cw is not None:
                self.cw = cw
            if mode is not None:
                self.mode = mode
            if target is not None:
                self.target = target

    def _run(self):
        while not self.stop.is_set():
            with self.lock:
                cw, mode, target = self.cw, self.mode, self.target
            try:
                self.log.transact([(CMD["BWR"], 0, 0x1100, struct.pack("<Hbi", cw, mode, target))])
            except Exception:
                pass
            time.sleep(0.02)

    def start(self):
        self.thread.start()
        time.sleep(0.15)
        return self

    def stop_thread(self):
        self.stop.set()
        time.sleep(0.05)


def sample(master, extra=None):
    raw = read_tx_pdo(master)
    decoded = decode_tx(raw)
    decoded["tx_hex"] = raw.hex()
    decoded["t_ms"] = int(time.time() * 1000)
    if extra:
        decoded.update(extra)
    return decoded


def wait_position(master, target, timeout_s=2.0, tolerance=0):
    deadline = time.time() + timeout_s
    last = {}
    while time.time() < deadline:
        last = sample(master)
        if abs(last["position"] - target) <= tolerance:
            return last
        time.sleep(0.02)
    raise SystemExit(f"FAIL: {timeout_s:.1f}s 内位置未到 {target}（last={last}）")


def capture_events(base, since):
    events = events_since(base, since)
    return events


def run_class(name, base, run_id, class_dir, stimulus, since, body):
    if not class_dir:
        return 0 if all(item.get("ok") for item in body.get("assertions") or []) else 1
    os.makedirs(class_dir, exist_ok=True)
    before_state = body["state"]
    samples = body.get("samples") or []
    after_state = http_json(base, "/state")[1]
    events = capture_events(base, since)
    ok = all(item.get("ok") for item in body.get("assertions") or [])
    write_json(os.path.join(class_dir, "stimulus.json"), body.get("stimulus") or {})
    write_json(os.path.join(class_dir, "samples.json"), samples)
    write_json(os.path.join(class_dir, "state-before.json"), before_state)
    write_json(os.path.join(class_dir, "state-after.json"), after_state)
    with open(os.path.join(class_dir, "events.jsonl"), "w", encoding="utf-8") as handle:
        for event in events:
            handle.write(json.dumps(event, ensure_ascii=False) + "\n")
    result = {
        "class": name,
        "run_id": run_id,
        "ok": ok,
        "assertions": body.get("assertions") or [],
        "notes": body.get("notes") or [],
    }
    write_json(os.path.join(class_dir, "result.json"), result)
    for assertion in result["assertions"]:
        print(f"{'OK  ' if assertion.get('ok') else 'FAIL'} [{name}] {assertion.get('name')} {assertion.get('detail') or ''}")
    return 0 if ok else 1


def main():
    parser = argparse.ArgumentParser(description="C4 用例×SIM 观测矩阵探针")
    parser.add_argument("socket_path", nargs="?", default="/tmp/ecsim-c4-matrix.sock")
    parser.add_argument("--http-base", required=True)
    parser.add_argument("--out-dir", default="")
    args = parser.parse_args()
    out_dir = args.out_dir or ""
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    master = probe.SyntheticMaster(args.socket_path, timeout=2.0)
    lock = threading.Lock()
    original = master.transact

    def locked(datagrams):
        with lock:
            return original(datagrams)

    master.transact = locked
    failures = 0
    try:
        sm_mailbox = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        broadcast_write(master, 0x0800, b"".join(struct.pack("<HHBBBB", *e) for e in sm_mailbox), 3)
        broadcast_write(master, 0x0120, bytes([0x02]), 3)
        sm_process = [(0x1100, 7, 0x04, 0, 1, 0), (0x1110, 11, 0x00, 0, 1, 0)]
        broadcast_write(master, 0x0810, b"".join(struct.pack("<HHBBBB", *e) for e in sm_process), 3)
        broadcast_write(master, 0x0120, bytes([0x04]), 3)
        broadcast_write(master, 0x0120, bytes([0x08]), 3)
        state = wait_all_op(args.http_base, 3, 3.0)
        run_id = state.get("run_id")
        print(f"RUN_ID {run_id}")

        stimulus = Stimulus(master, master).start()
        stimulus.set(cw=0x0006, mode=8, target=0)
        time.sleep(0.1)
        stimulus.set(cw=0x0007)
        time.sleep(0.1)
        stimulus.set(cw=0x000F)
        time.sleep(0.1)
        enabled = sample(master)
        print(f"ENABLED {enabled}")

        since = max((event.get("seq", 0) for event in http_json(args.http_base, "/events?since=0")[1].get("events") or []), default=0)

        def mark():
            nonlocal since
            current = max((event.get("seq", 0) for event in http_json(args.http_base, "/events?since=0")[1].get("events") or []), default=0)
            since = current
            return current

        def assertions(condition_pairs):
            return [{"name": name, "ok": bool(ok), "detail": detail} for name, ok, detail in condition_pairs]

        # ---- program：程序等价（连续轨迹点）----
        mark()
        stimulus.set(cw=0x000F, mode=8, target=0)
        time.sleep(0.1)
        points = [100, 200, 300, 400, 500]
        samples = []
        for point in points:
            stimulus.set(target=point)
            time.sleep(0.08)
            samples.append(sample(master, {"target": point}))
        following = all(item["position"] == point for item, point in zip(samples, points))
        reached = all(item["statusword"] & STATUS_TARGET_REACHED for item in samples)
        failures += run_class(
            "program", args.http_base, run_id, os.path.join(out_dir, "program") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "连续程序轨迹点（G-Code 直线段等价）", "points": points, "mode": 8},
                "samples": samples,
                "assertions": assertions([
                    ("程序轨迹点逐点跟随", following, f"{[(s['position'], p) for s, p in zip(samples, points)]}"),
                    ("每个点 Target reached", reached, f"{[hex(s['statusword']) for s in samples]}"),
                ]),
                "notes": ["CNC 侧程序装载/循环启动证据 PENDING（D1/D3）"],
            },
        )

        # ---- jog：增量点动 ----
        mark()
        jog_start = samples[-1]["position"]
        jog_samples = []
        for _ in range(5):
            stimulus.set(target=stimulus.target + 100)
            time.sleep(0.08)
            jog_samples.append(sample(master))
        jog_ok = all(
            later["position"] - earlier["position"] == 100
            for earlier, later in zip([{"position": jog_start}] + jog_samples[:-1], jog_samples)
        )
        failures += run_class(
            "jog", args.http_base, run_id, os.path.join(out_dir, "jog") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "增量 jog：每 80ms 目标 +100 脉冲", "start": jog_start, "steps": 5, "step": 100},
                "samples": jog_samples,
                "assertions": assertions([("jog 每次 +100 跟随", jog_ok, f"{[s['position'] for s in jog_samples]}")]),
                "notes": ["CNC 侧 jog 键/HAL 证据 PENDING；SIM 观察值为 RxPDO 0x607A → TxPDO 0x6064"],
            },
        )

        # ---- MDI：单条绝对移动 ----
        mark()
        stimulus.set(target=5000)
        mdi = wait_position(master, 5000)
        mdi_ok = mdi["position"] == 5000 and bool(mdi["statusword"] & STATUS_TARGET_REACHED)
        failures += run_class(
            "mdi", args.http_base, run_id, os.path.join(out_dir, "mdi") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "单条 MDI 绝对移动（G0 X5000 等价）", "target": 5000},
                "samples": [mdi],
                "assertions": assertions([("MDI 单目标到位", mdi_ok, f"{mdi}")]),
                "notes": ["CNC 侧 MDI 通道证据 PENDING；SIM 侧只验证 PDO 目标/实际一致"],
            },
        )

        # ---- home：回零（HM 模式 6，bit4 → 位置归零 + Homing attained）----
        mark()
        stimulus.set(mode=6, cw=0x001F, target=0)
        time.sleep(0.2)
        home = sample(master)
        home_ok = home["position"] == 0 and bool(home["statusword"] & STATUS_HOMING_ATTAINED) and home["mode"] == 6
        stimulus.set(mode=8, cw=0x000F, target=0)
        time.sleep(0.1)
        failures += run_class(
            "home", args.http_base, run_id, os.path.join(out_dir, "home") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "CiA402 HM 模式 6，控制字 bit4=1（Tier0 归零）", "mode": 6, "controlword": "0x000F"},
                "samples": [home],
                "assertions": assertions([("回零：位置 0 + Homing attained(bit12)", home_ok, f"{home}")]),
                "notes": ["Tier0 不模拟寻零开关/过程；CNC 侧 HOME ALL 证据 PENDING"],
            },
        )

        # ---- override：轨迹速率 100% → 50%（等效 feed override 对目标增量速率）----
        mark()
        stimulus.set(mode=8, cw=0x000F, target=0)
        time.sleep(0.1)
        rate_samples = []
        rate_start = sample(master)
        stimulus.set(target=0)
        time.sleep(0.1)
        full_start = sample(master)
        for step in range(1, 11):
            stimulus.set(target=step * 200)
            time.sleep(0.1)
        full_end = sample(master)
        full_delta = full_end["position"] - full_start["position"]
        half_start = sample(master)
        for step in range(1, 11):
            stimulus.set(target=full_end["position"] + step * 100)
            time.sleep(0.1)
        half_end = sample(master)
        half_delta = half_end["position"] - half_start["position"]
        ratio = (half_delta / full_delta) if full_delta else 0.0
        override_ok = full_delta == 2000 and half_delta == 1000 and 0.4 <= ratio <= 0.6
        rate_samples = [rate_start, full_start, full_end, half_start, half_end]
        failures += run_class(
            "override", args.http_base, run_id, os.path.join(out_dir, "override") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "轨迹目标增量速率 100% vs 50%（feed override 等效）", "full_step": 200, "half_step": 100, "period_ms": 100},
                "samples": rate_samples,
                "assertions": assertions([("100% 速率增量 2000", full_delta == 2000, f"{full_delta}"), ("50% 速率增量 1000", half_delta == 1000, f"{half_delta}"), ("速率比 ≈0.5", 0.4 <= ratio <= 0.6, f"{ratio:.3f}")]),
                "notes": ["CNC 侧 feed override 旋钮/桥命令证据 PENDING；SIM 侧以目标增量速率等价观察"],
            },
        )

        # ---- tool：SDO 参数装载/读回（0x60FF 参数面；等效刀具参数装载）----
        mark()
        download = sdo_download_i32(master, 0x60FF, 0, 750)
        download_ok = len(download) >= 4 and download[2] == 0x60
        upload = sdo_upload(master, 0x60FF, 0)
        value = struct.unpack("<i", upload[6:10])[0] if len(upload) >= 10 else None
        tool_ok = download_ok and upload[2] == 0x43 and value == 750
        audit = http_json(args.http_base, "/sdo-audit")
        failures += run_class(
            "tool", args.http_base, run_id, os.path.join(out_dir, "tool") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "刀具/工艺参数装载走 SDO 参数面：写 0x60FF=750 再读回", "index": "0x60FF", "value": 750},
                "samples": [],
                "assertions": assertions([
                    ("SDO 下载 0x60FF 成功", download_ok, download.hex()),
                    ("SDO 上传读回 750", tool_ok, f"cmd={upload[2]:#04x} value={value}"),
                    ("/sdo-audit 可用", audit[0] == 200, f"status={audit[0]}"),
                ]),
                "notes": ["CNC 侧刀具表 T/M6/TOUCH PLATE 证据 PENDING；SIM 侧仅证明参数面 SDO 可写可读"],
            },
        )

        # ---- offset：工件坐标偏移 → 目标位置平移 ----
        mark()
        stimulus.set(target=1000)
        time.sleep(0.15)
        base_pos = sample(master)
        stimulus.set(target=3500)  # +2500 偏移
        time.sleep(0.15)
        shifted = sample(master)
        stimulus.set(target=3000)  # -500 再偏移
        time.sleep(0.15)
        shifted_back = sample(master)
        offset_ok = base_pos["position"] == 1000 and shifted["position"] == 3500 and shifted_back["position"] == 3000
        failures += run_class(
            "offset", args.http_base, run_id, os.path.join(out_dir, "offset") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "工件坐标系偏移：目标 0x607A 平移 +2500/-500，观察 0x6064", "positions": [1000, 3500, 3000]},
                "samples": [base_pos, shifted, shifted_back],
                "assertions": assertions([("目标位置平移后实际位置一致", offset_ok, f"{[s['position'] for s in [base_pos, shifted, shifted_back]]}")]),
                "notes": ["CNC 侧 OFFSETS 写/生效证据 PENDING；SIM 侧只观察 PDO 平移"],
            },
        )

        # ---- probe：探针 tripped 置位/清除 ----
        mark()
        status, body = http_json(args.http_base, "/probe", "POST", {"tripped": True, "target": 0, "confirm": True, "run_id": run_id})
        probe_set_ok = status == 200 and body.get("ok") is True
        probe_state = http_json(args.http_base, "/probe")[1]
        tripped = (probe_state.get("probe") or [{}])[0].get("tripped") if probe_state.get("probe") else None
        status2, body2 = http_json(args.http_base, "/probe", "POST", {"tripped": False, "target": 0, "confirm": True, "run_id": run_id})
        probe_clear_ok = status2 == 200 and body2.get("ok") is True
        events = capture_events(args.http_base, since)
        probe_events = [event for event in events if event.get("type") == "probe_state"]
        probe_ok = probe_set_ok and tripped is True and probe_clear_ok and len(probe_events) >= 2
        failures += run_class(
            "probe", args.http_base, run_id, os.path.join(out_dir, "probe") if out_dir else "",
            stimulus, since,
            {
                "state": http_json(args.http_base, "/state")[1],
                "stimulus": {"equivalence": "POST /probe tripped=true→false（探针触发信号等价）", "target": 0},
                "samples": [],
                "assertions": assertions([
                    ("probe 置位 200", probe_set_ok, f"{status}"),
                    ("/probe 读回 tripped=true", tripped is True, f"{tripped}"),
                    ("probe 清除 200", probe_clear_ok, f"{status2}"),
                    ("probe_state 事件成对", len(probe_events) >= 2, f"events={len(probe_events)}"),
                ]),
                "notes": ["CNC 侧 touch plate/probe 流程证据 PENDING；SIM 侧只覆盖 tripped 信号与事件"],
            },
        )

        # ---- alarm：运行期 AL 错误注入 + 恢复（P1 能力，用于矩阵覆盖）----
        mark()
        status, body = http_json(args.http_base, "/fault", "POST", {"fault": "al_error", "target": 0, "duration_ms": 900, "confirm": True, "run_id": run_id})
        alarm_inject_ok = status == 200 and body.get("ok") is True
        time.sleep(0.2)
        during = http_json(args.http_base, "/state")[1]
        slave0 = slaves_of(during)[0] if slaves_of(during) else {}
        alarm_ok = (
            alarm_inject_ok
            and slave0.get("al_code") == "0x0011"
            and (int(str(slave0.get("state", "0x0")), 0) & 0x10) != 0
        )
        deadline = time.time() + 5.0
        recovered = False
        while time.time() < deadline:
            current = http_json(args.http_base, "/state")[1]
            listed = slaves_of(current)
            if listed and listed[0].get("al_code") == "0x0000" and listed[0].get("op") is True:
                recovered = True
                break
            time.sleep(0.1)
        events = capture_events(args.http_base, since)
        alarm_events = [event for event in events if (event.get("details") or {}).get("fault") == "al_error"]
        alarm_ok = alarm_ok and recovered and len(alarm_events) >= 1
        failures += run_class(
            "alarm", args.http_base, run_id, os.path.join(out_dir, "alarm") if out_dir else "",
            stimulus, since,
            {
                "state": during,
                "stimulus": {"equivalence": "运行期 AL 错误注入 al_error target=0 900ms（P1 能力；复用 B57/遥测）", "target": 0},
                "samples": [{"during": during, "recovered": recovered}],
                "assertions": assertions([("注入 200 且 al_code=0x0011/state error bit", alarm_ok, f"{slave0}"), ("到期恢复 0x0000/op=true", recovered, ""), ("al_error 事件", len(alarm_events) >= 1, f"events={len(alarm_events)}")]),
                "notes": ["CNC 侧报警/ack_alarm 证据 PENDING；AL 运行期注入为 P1，不作 task-102 P0 判据"],
            },
        )

        # ---- link-loss：/fault link_loss 期间帧停更、恢复后续帧 ----
        mark()
        status, body = http_json(args.http_base, "/fault", "POST", {"fault": "link_loss", "target": None, "duration_ms": 600, "confirm": True, "run_id": run_id})
        link_inject_ok = status == 200 and body.get("ok") is True
        time.sleep(0.2)
        first = http_json(args.http_base, "/state")[1]
        time.sleep(0.2)
        second = http_json(args.http_base, "/state")[1]
        frames_a = {entry["index"]: entry.get("frames_in") for entry in slaves_of(first)}
        frames_b = {entry["index"]: entry.get("frames_in") for entry in slaves_of(second)}
        frozen = frames_a == frames_b
        deadline = time.time() + 5.0
        link_cleared = False
        while time.time() < deadline:
            _, status_body = http_json(args.http_base, "/fault/status")
            if not any(item.get("fault") == "link_loss" for item in (status_body.get("active") or [])):
                link_cleared = True
                break
            time.sleep(0.1)
        time.sleep(0.25)
        third = http_json(args.http_base, "/state")[1]
        frames_c = {entry["index"]: entry.get("frames_in") for entry in slaves_of(third)}
        resumed = all(frames_c[index] > frames_b[index] for index in frames_b)
        events = capture_events(args.http_base, since)
        link_events = [event for event in events if event.get("type") == "link"]
        link_ok = link_inject_ok and frozen and link_cleared and resumed and len(link_events) >= 2
        failures += run_class(
            "link-loss", args.http_base, run_id, os.path.join(out_dir, "link-loss") if out_dir else "",
            stimulus, since,
            {
                "state": third,
                "stimulus": {"equivalence": "POST /fault link_loss 600ms（帧停更）；控制面仍 200", "target": None},
                "samples": [{"frames_a": frames_a, "frames_b": frames_b, "frames_c": frames_c}],
                "assertions": assertions([
                    ("注入 200", link_inject_ok, f"{status}"),
                    ("期间 frames 停更", frozen, f"{frames_a} -> {frames_b}"),
                    ("到期清理", link_cleared, ""),
                    ("恢复后 frames 增长", resumed, f"{frames_b} -> {frames_c}"),
                    ("link down/up 事件", len(link_events) >= 2, f"events={len(link_events)}"),
                ]),
                "notes": ["CNC 侧 STALE/LOST 证据 PENDING；SIM 侧覆盖帧停更/恢复与 link 事件"],
            },
        )

        summary = {"run_id": run_id, "classes": 10, "failed_checks": failures, "result": "PASS" if failures == 0 else "FAIL"}
        if out_dir:
            write_json(os.path.join(out_dir, "c4-matrix-summary.json"), summary)
        print(f"RESULT: {summary['result']}（10 类，{failures} 个失败）")
        return 0 if failures == 0 else 1
    finally:
        try:
            stimulus.stop_thread()
        except Exception:
            pass
        master.close()


if __name__ == "__main__":
    sys.exit(main())
