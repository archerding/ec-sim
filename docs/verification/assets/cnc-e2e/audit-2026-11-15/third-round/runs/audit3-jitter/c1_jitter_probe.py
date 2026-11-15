#!/usr/bin/env python3
"""C1：周期抖动来源与量化探针（SIM 侧）。

三路测量（全部来自 SIM 可观测面，非 CNC/IgH 侧）：
1. `master_interval`：合成 UDS 主站两次 `transact` 起始时刻的间隔（Python 睡眠
   粒度 + 主机调度 + 内核 socket 唤醒），代表主站侧发送抖动；
2. `state_latency`：`GET /state` 的 HTTP 往返耗时（serve 线程调度 + 序列化），
   代表采样/遥测抖动；
3. `fps_samples` / `wkc`：同一窗口内 `/state.master.fps` 与每周期回复 WKC 的值域。

WKC 来源说明：本探针的 WKC 来自 EC-Sim 对合成帧的回复（SIM 帧处理结果）；
内核/IgH 主站的 WKC 由 CNC 侧观测，本探针**不提供、不声称**。

输出 `<out-dir>/jitter.json`（schema `ec-sim/e2e-jitter-v1`）；
退出码 0=测量完成（不代表 CNC 验收），1=运行失败。
"""
import argparse
import json
import os
import statistics
import struct
import sys
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import uds_master_probe as probe  # noqa: E402

CMD = probe.CMD
CMD["BWR"] = 0x08


def http_json(base, path):
    started = time.monotonic()
    with urllib.request.urlopen(base.rstrip("/") + path, timeout=3.0) as response:
        payload = json.loads(response.read().decode("utf-8"))
    return payload, (time.monotonic() - started) * 1000.0


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
        last, _ = http_json(base, "/state")
        listed = slaves_of(last)
        if (
            (last.get("master") or {}).get("op_slaves") == slave_count
            and len(listed) == slave_count
            and all(entry.get("op") is True for entry in listed)
        ):
            return last
        time.sleep(0.02)
    raise SystemExit(f"FAIL: {timeout_s:.1f}s 内未达到 {slave_count}/{slave_count} OP")


def percentiles(samples):
    ordered = sorted(samples)
    if not ordered:
        return {}
    def pick(pct):
        index = min(len(ordered) - 1, max(0, int(round((pct / 100.0) * (len(ordered) - 1)))))
        return ordered[index]
    return {
        "count": len(ordered),
        "mean_ms": round(statistics.fmean(ordered), 4),
        "std_ms": round(statistics.pstdev(ordered), 4) if len(ordered) > 1 else 0.0,
        "min_ms": round(ordered[0], 4),
        "p50_ms": round(pick(50), 4),
        "p95_ms": round(pick(95), 4),
        "p99_ms": round(pick(99), 4),
        "max_ms": round(ordered[-1], 4),
    }


def main():
    parser = argparse.ArgumentParser(description="C1 抖动来源与量化探针")
    parser.add_argument("socket", nargs="?", default="/tmp/ecsim-jitter.sock")
    parser.add_argument("--http-base", required=True)
    parser.add_argument("--expect-slaves", type=int, default=3)
    parser.add_argument("--samples", type=int, default=200)
    parser.add_argument("--interval-ms", type=float, default=20.0)
    parser.add_argument("--out-dir", required=True)
    args = parser.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    master = probe.SyntheticMaster(args.socket, timeout=2.0)
    try:
        mailbox_sms = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        broadcast_write(master, 0x0800, b"".join(struct.pack("<HHBBBB", *e) for e in mailbox_sms), args.expect_slaves, "SM0/SM1 配置")
        broadcast_write(master, 0x0120, bytes([0x02]), args.expect_slaves, "AL Control PREOP")
        process_sms = [(0x1100, 7, 0x04, 0, 1, 0), (0x1110, 11, 0x00, 0, 1, 0)]
        broadcast_write(master, 0x0810, b"".join(struct.pack("<HHBBBB", *e) for e in process_sms), args.expect_slaves, "SM2/SM3 配置")
        broadcast_write(master, 0x0120, bytes([0x04]), args.expect_slaves, "AL Control SAFEOP")
        broadcast_write(master, 0x0120, bytes([0x08]), args.expect_slaves, "AL Control OP")
        first_state = wait_all_op(args.http_base, args.expect_slaves, 3.0)
        first_frames = sum(entry.get("frames_in") or 0 for entry in slaves_of(first_state))
        window_start = time.monotonic()

        send_times = []
        intervals = []
        state_latencies = []
        fps_samples = []
        wkc_values = []
        run_id = first_state.get("run_id")
        for index in range(args.samples):
            started = time.monotonic()
            reply = broadcast_write(
                master,
                0x1100,
                struct.pack("<Hbi", 0x000F, 8, (index * 100) % 100000),
                args.expect_slaves,
                "SM2 process data",
            )
            wkc_values.append(reply[0][4])
            send_times.append(started)
            if index:
                intervals.append((started - send_times[-2]) * 1000.0)
            if index % 5 == 0:
                snapshot, latency = http_json(args.http_base, "/state")
                state_latencies.append(latency)
                fps = (snapshot.get("master") or {}).get("fps")
                if isinstance(fps, (int, float)):
                    fps_samples.append(float(fps))
            sleep_s = args.interval_ms / 1000.0 - (time.monotonic() - started)
            if sleep_s > 0:
                time.sleep(sleep_s)

        last_state, _ = http_json(args.http_base, "/state")
        last_frames = sum(entry.get("frames_in") or 0 for entry in slaves_of(last_state))
        window_seconds = max(1e-9, time.monotonic() - window_start)
        measured_fps = (last_frames - first_frames) / window_seconds

        payload = {
            "schema": "ec-sim/e2e-jitter-v1",
            "run_id": run_id,
            "target_interval_ms": args.interval_ms,
            "samples": args.samples,
            "sources": {
                "master_interval": "合成 UDS 主站 transact 起始间隔：Python sleep 粒度 + 主机调度 + socket 唤醒",
                "state_latency": "GET /state HTTP 往返：serve 线程调度 + JSON 序列化",
                "fps": "/state.master.fps 为粗估（serve 线程两次调用间帧差），不等于瞬时周期；本探针另给 frame_rate（窗口内 frames_in 增量/墙钟）",
                "wkc": "EC-Sim 帧回复中的 WKC（SIM 帧处理结果）；内核/IgH WKC 不由本探针提供",
            },
            "frame_rate": {
                "frames": last_frames - first_frames,
                "seconds": round(window_seconds, 6),
                "fps": round(measured_fps, 4),
                "per_slave_fps": round(measured_fps / max(1, args.expect_slaves), 4),
                "per_slave": args.expect_slaves,
            },
            "master_interval": percentiles(intervals),
            "state_latency": percentiles(state_latencies),
            "fps_samples": {
                "count": len(fps_samples),
                "min": round(min(fps_samples), 4) if fps_samples else None,
                "max": round(max(fps_samples), 4) if fps_samples else None,
                "mean": round(statistics.fmean(fps_samples), 4) if fps_samples else None,
            },
            "wkc": {
                "values": sorted(set(wkc_values)),
                "min": min(wkc_values) if wkc_values else None,
                "max": max(wkc_values) if wkc_values else None,
            },
            "note": "SIM 侧抖动量化；不代表 CNC VM/IgH 周期抖动，CNC 侧来源仍以 IgH/lcec 观测为准。",
        }
        with open(os.path.join(args.out_dir, "jitter.json"), "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2, sort_keys=True)
            handle.write("\n")
        interval = payload["master_interval"]
        latency = payload["state_latency"]
        print(
            "JITTER_OK samples=%d frame_rate=%.2ffps (%.2f/slave) interval_mean=%.3fms std=%.3fms "
            "p95=%.3fms p99=%.3fms state_p95=%.3fms wkc=%s"
            % (
                args.samples,
                payload["frame_rate"]["fps"],
                payload["frame_rate"]["per_slave_fps"],
                interval.get("mean_ms", 0.0),
                interval.get("std_ms", 0.0),
                interval.get("p95_ms", 0.0),
                interval.get("p99_ms", 0.0),
                latency.get("p95_ms", 0.0),
                payload["wkc"]["values"],
            )
        )
        return 0
    finally:
        master.close()


if __name__ == "__main__":
    sys.exit(main())
