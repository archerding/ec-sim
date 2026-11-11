#!/usr/bin/env python3
"""TAP 持有者（模拟器角色最小原型）：创建并持有 TAP fd，使 carrier up，并记录收到的 EtherCAT 帧。

没有进程持有时，TAP 是 NO-CARRIER，内核会丢弃发送到它的帧——所以"谁持有 fd"就是"谁扮演从站"。
本脚本用于阶段性验证：主站（SOEM/IgH）发出的帧能否到达"从站侧"。

用法：python3 tap_hold.py <iface> <seconds>
"""
import fcntl
import os
import select
import signal
import struct
import sys
import time

TUNSETIFF = 0x400454CA
IFF_TAP = 0x0002
IFF_NO_PI = 0x1000
SIOCSIFHWADDR = 0x8924
SIOCGIFFLAGS = 0x8913
SIOCSIFFLAGS = 0x8914
IFF_UP = 0x1
IFF_RUNNING = 0x40
ETH_P_ECAT = 0x88A4

DEV = sys.argv[1] if len(sys.argv) > 1 else "taphold0"
SECONDS = float(sys.argv[2]) if len(sys.argv) > 2 else 15.0
SIM_MAC = bytes([0x02, 0xEC, 0x51, 0x4D, 0x00, 0x01])

running = True


def on_signal(signum, frame):
    global running
    running = False


def create_tap(name):
    fd = os.open("/dev/net/tun", os.O_RDWR)
    ifr = struct.pack("16sH", name.encode(), IFF_TAP | IFF_NO_PI)
    fcntl.ioctl(fd, TUNSETIFF, ifr)
    return fd


def set_mac(name, mac):
    s = socket_socket()
    try:
        fcntl.ioctl(s, SIOCSIFHWADDR, struct.pack("16sH14s8x", name.encode(), 1, mac))
    finally:
        s.close()


def socket_socket():
    import socket

    return socket.socket(socket.AF_INET, socket.SOCK_DGRAM)


def get_flags(name):
    s = socket_socket()
    try:
        res = fcntl.ioctl(s, SIOCGIFFLAGS, struct.pack("16sH38x", name.encode(), 0))
        return struct.unpack("16sH", res[:18])[1]
    finally:
        s.close()


def set_up(name):
    s = socket_socket()
    try:
        flags = get_flags(name) | IFF_UP
        fcntl.ioctl(s, SIOCSIFFLAGS, struct.pack("16sH38x", name.encode(), flags))
    finally:
        s.close()


def main():
    global running
    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

    fd = create_tap(DEV)
    set_mac(DEV, SIM_MAC)
    try:
        with open(f"/proc/sys/net/ipv6/conf/{DEV}/disable_ipv6", "w") as f:
            f.write("1")
    except OSError:
        pass
    set_up(DEV)
    time.sleep(0.2)
    flags = get_flags(DEV)
    print(
        f"HOLDER_READY iface={DEV} fd={fd} up={bool(flags & IFF_UP)} "
        f"running={bool(flags & IFF_RUNNING)}",
        flush=True,
    )

    deadline = time.time() + SECONDS
    frames = 0
    skipped = 0
    while running and time.time() < deadline:
        r, _, _ = select.select([fd], [], [], 0.2)
        if not r:
            continue
        pkt = os.read(fd, 65535)
        if len(pkt) < 14 or struct.unpack("!H", pkt[12:14])[0] != ETH_P_ECAT:
            skipped += 1
            continue
        frames += 1
        cmd = pkt[16] if len(pkt) >= 18 else 0xFF
        print(f"HOLDER_FRAME #{frames} len={len(pkt)} cmd=0x{cmd:02X} head={pkt[:24].hex()}", flush=True)

    print(f"HOLDER_SUMMARY frames={frames} skipped_non_ecat={skipped} iface={DEV}", flush=True)
    os.close(fd)
    return 0 if frames > 0 else 1


if __name__ == "__main__":
    sys.exit(main())
