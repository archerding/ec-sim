#!/usr/bin/env python3
"""ESC-over-UDS shim 延迟基准：顺序发 APRD 帧，测 UDS 往返延迟（主站→从站→主站）。

用法：python3 scripts/bench/uds-latency.py <socket> [frames]
输出：`LATENCY frames=<n> min=..us p50=..us p90=..us p99=..us p999=..us max=..us`

说明：
- 计时包含 UDS 发送/接收、模拟器处理与调度开销；**不是**真实总线/实时性承诺。
- 验证机默认非隔离 CPU；用于回归对比（同机前后对比），绝对数值随环境波动。
- 需要从站已按 `--uds <socket>` 启动（本脚本不负责启动）。
"""
import os
import statistics
import sys
import time

SPIKES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "spikes")
sys.path.insert(0, os.path.abspath(SPIKES))

import uds_master_probe as probe  # noqa: E402


def main() -> int:
    if len(sys.argv) < 2:
        print("用法：uds-latency.py <socket> [frames]", file=sys.stderr)
        return 2
    socket_path = sys.argv[1]
    frames = int(sys.argv[2]) if len(sys.argv) > 2 else 2000
    master = probe.SyntheticMaster(socket_path, timeout=2.0)
    samples = []
    try:
        # 预热（建立路径/缓存）
        for _ in range(50):
            master.transact([(probe.CMD["APRD"], 0, 0x0130, b"\x00\x00")])
        for _ in range(frames):
            started = time.perf_counter_ns()
            reply = master.transact([(probe.CMD["APRD"], 0, 0x0130, b"\x00\x00")])
            elapsed = (time.perf_counter_ns() - started) / 1000.0
            probe.check(reply[0][4] == 1, "APRD WKC 不是 1")
            samples.append(elapsed)
    finally:
        master.close()

    samples.sort()

    def pct(q: float) -> float:
        index = min(len(samples) - 1, max(0, int(round(q * (len(samples) - 1)))))
        return samples[index]

    print(
        "LATENCY frames={} min={:.1f}us p50={:.1f}us p90={:.1f}us p99={:.1f}us "
        "p999={:.1f}us max={:.1f}us mean={:.1f}us".format(
            len(samples),
            samples[0],
            statistics.median(samples),
            pct(0.90),
            pct(0.99),
            pct(0.999),
            samples[-1],
            statistics.fmean(samples),
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
