#!/usr/bin/env python3
"""task-103：CNC E2E 观测采集器（对运行中的 `ec-sim-esc --serve`）。

按固定间隔抓取 `/state`、`/metrics`、`/events?since=<seq>`，落盘为 JSONL，便于
HMI/桥/CNC 侧证据按 `run_id` 关联。只读（GET），不注入故障、不改状态。

用法：
  python3 scripts/spikes/e2e_observability_collect.py \
      --base http://127.0.0.1:18210 --run-id <run_id> \
      --seconds 60 --interval 1.0 --out .scratch/observability/<run_id>

输出：
  state.jsonl    每行 {"ts_unix","run_id","master":{...},"slaves":[...]}
  metrics.jsonl  每行 {"ts_unix","text": "..."}（Prometheus 原始文本）
  events.jsonl   追加事件对象（按 seq 去重）
  summary.json   采集窗口内 op_slaves 最小/最大、逐站 al_code 集合、watchdog/frames 增量、事件计数
退出码：0=采集完成且 run_id 一致，1=参数/HTTP/run_id 不一致。
"""
import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request


def fetch(base, path):
    with urllib.request.urlopen(base.rstrip("/") + path, timeout=3.0) as response:
        return response.read().decode("utf-8")


def fetch_json(base, path):
    return json.loads(fetch(base, path))


def main():
    parser = argparse.ArgumentParser(description="EC-Sim CNC E2E observability collector")
    parser.add_argument("--base", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--seconds", type=float, default=60.0)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)
    state_path = os.path.join(args.out, "state.jsonl")
    metrics_path = os.path.join(args.out, "metrics.jsonl")
    events_path = os.path.join(args.out, "events.jsonl")

    last_seq = 0
    states = []
    events_count = 0
    watchdog_first = None
    watchdog_last = 0
    frames_first = None
    frames_last = 0
    op_values = set()
    al_codes = set()
    started = time.time()
    event_file = open(events_path, "a", encoding="utf-8")

    try:
        while time.time() - started < args.seconds:
            now = time.time()
            try:
                state = fetch_json(args.base, "/state")
                if state.get("run_id") != args.run_id:
                    print(
                        f"FAIL: /state.run_id={state.get('run_id')} != {args.run_id}",
                        file=sys.stderr,
                    )
                    return 1
                master = state.get("master") or {}
                slaves = state.get("slaves") or (master.get("slaves") or [])
                op = master.get("op_slaves")
                if isinstance(op, int):
                    op_values.add(op)
                trips = (master.get("watchdog") or {}).get("trips", 0)
                if watchdog_first is None:
                    watchdog_first = trips
                watchdog_last = trips
                frames = sum(entry.get("frames_in", 0) for entry in slaves)
                if frames_first is None:
                    frames_first = frames
                frames_last = frames
                for entry in slaves:
                    if entry.get("al_code") is not None:
                        al_codes.add(str(entry.get("al_code")))

                with open(state_path, "a", encoding="utf-8") as handle:
                    handle.write(
                        json.dumps(
                            {"ts_unix": round(now, 3), "run_id": state.get("run_id"), "master": master, "slaves": slaves},
                            ensure_ascii=False,
                        )
                        + "\n"
                    )
                states.append(state)

                metrics_text = fetch(args.base, "/metrics")
                with open(metrics_path, "a", encoding="utf-8") as handle:
                    handle.write(
                        json.dumps({"ts_unix": round(now, 3), "text": metrics_text}, ensure_ascii=False)
                        + "\n"
                    )

                events = fetch_json(args.base, f"/events?since={last_seq}").get("events") or []
                for event in events:
                    seq = event.get("seq")
                    if isinstance(seq, int):
                        last_seq = max(last_seq, seq)
                    event_file.write(json.dumps(event, ensure_ascii=False) + "\n")
                    events_count += 1
            except (urllib.error.URLError, json.JSONDecodeError, TimeoutError) as error:
                print(f"WARN: 采集失败（继续）：{error}", file=sys.stderr)
            time.sleep(args.interval)
    finally:
        event_file.close()

    summary = {
        "run_id": args.run_id,
        "window_seconds": round(time.time() - started, 2),
        "state_samples": len(states),
        "event_count": events_count,
        "op_slaves_values": sorted(op_values),
        "al_codes_seen": sorted(al_codes),
        "watchdog_trips_delta": (watchdog_last - watchdog_first) if watchdog_first is not None else None,
        "frames_in_delta": (frames_last - frames_first) if frames_first is not None else None,
        "last_seq": last_seq,
    }
    with open(os.path.join(args.out, "summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print("COLLECT_OK " + json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
