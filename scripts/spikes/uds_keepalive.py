#!/usr/bin/env python3
"""周期性向 EC-Sim UDS 发 APRD 帧，保持帧循环 tick。

运行期故障（POST /fault）的到期清理只在下一帧到达时执行；没有真实主站时用本脚本
每 20ms 发一帧，让 `duration_ms` 到期、watchdog/AL 事件得以产生。仅供探针使用。

用法：
    python3 uds_keepalive.py /tmp/ecsim-uds.sock [--interval-ms 20] [--seconds 10]

退出码：0 = 正常结束（收到 SIGTERM/SIGINT 或到达 --seconds）；2 = 帧全部失败。
"""

import argparse
import os
import signal
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from uds_master_probe import CMD, SyntheticMaster  # noqa: E402

RUNNING = True


def _stop(_signum, _frame):
    global RUNNING
    RUNNING = False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("socket", nargs="?", default="/tmp/ecsim-uds.sock")
    parser.add_argument("--interval-ms", type=int, default=20)
    parser.add_argument("--seconds", type=float, default=0.0, help="0 = 收到 SIGTERM 前一直发")
    parser.add_argument("--timeout", type=float, default=0.3)
    args = parser.parse_args()

    signal.signal(signal.SIGTERM, _stop)
    signal.signal(signal.SIGINT, _stop)

    master = SyntheticMaster(args.socket, timeout=args.timeout)
    interval = max(args.interval_ms, 1) / 1000.0
    deadline = time.monotonic() + args.seconds if args.seconds > 0 else None
    sent = 0
    failed = 0
    try:
        while RUNNING and (deadline is None or time.monotonic() < deadline):
            try:
                master.transact([(CMD["APRD"], 0, 0x0130, b"\x00\x00")])
                sent += 1
            except OSError:
                failed += 1
            time.sleep(interval)
    finally:
        master.close()

    print(f"KEEPALIVE_OK frames={sent} failed={failed}", flush=True)
    return 0 if sent > 0 else 2


if __name__ == "__main__":
    sys.exit(main())
