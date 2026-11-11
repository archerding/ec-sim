#!/usr/bin/env python3
"""抓取某个 netdev 上的 EtherType 0x88A4 帧（用于验证主站确实通过 TAP 发出了 EtherCAT 帧）。

用法：python3 sniff_ecat.py <iface> <seconds>
输出：每帧时间戳 + 方向 + 长度 + 前 32 字节 hex；最后一行 SUMMARY。
"""
import socket
import struct
import sys
import time

ETH_P_ALL = 0x0003
ETH_P_ECAT = 0x88A4


def main():
    iface = sys.argv[1]
    seconds = float(sys.argv[2]) if len(sys.argv) > 2 else 10.0

    s = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(ETH_P_ALL))
    s.bind((iface, 0))
    s.settimeout(0.5)

    start = time.time()
    frames = []
    print(f"[sniffer] listening on {iface} for {seconds}s ...", flush=True)
    while time.time() - start < seconds:
        try:
            pkt, addr = s.recvfrom(65535)
        except socket.timeout:
            continue
        if len(pkt) < 14:
            continue
        etype = struct.unpack("!H", pkt[12:14])[0]
        if etype != ETH_P_ECAT:
            continue
        pkttype = addr[2] if len(addr) > 2 else -1
        direction = {4: "OUT(host->dev)", 0: "IN(dev->host)"}.get(pkttype, f"pkttype={pkttype}")
        # EtherCAT 头 2 字节：长度 + 类型；随后第一条 datagram 的 cmd/idx
        ec_len = struct.unpack("<H", pkt[14:16])[0] if len(pkt) >= 16 else 0
        cmd = pkt[16] if len(pkt) >= 17 else 0
        frames.append((time.time() - start, direction, len(pkt), ec_len, cmd, pkt[:32].hex()))
        print(
            f"  +{time.time() - start:6.3f}s {direction:14s} len={len(pkt):4d} "
            f"ec_len={ec_len:4d} cmd=0x{cmd:02X} head={pkt[:24].hex()}",
            flush=True,
        )

    print(f"[sniffer] SUMMARY frames={len(frames)} iface={iface} window={seconds}s")
    if frames:
        cmds = sorted({f[4] for f in frames})
        print(f"[sniffer] cmds seen: {['0x%02X' % c for c in cmds]}")
    return 0 if frames else 1


if __name__ == "__main__":
    sys.exit(main())
