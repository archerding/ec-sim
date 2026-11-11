#!/usr/bin/env python3
"""Spike 1: TAP(L2) <-> AF_PACKET 双向承载 EtherType 0x88A4 帧的可行性验证。

目的：证明"用户态模拟器（持有 TAP fd）"与"主站（AF_PACKET 原始套接字）"
可以通过同一个 TAP netdev 双向交换 EtherCAT 帧，且帧内容逐字节一致。

映射关系（与真实 ESC 一致）：
  模拟器 write(tap)  -> 视为从站发出 -> AF_PACKET 收到（主站收帧）
  AF_PACKET send     -> 视为主站发出 -> 模拟器 read(tap) 收到

用法：需 root 或 CAP_NET_ADMIN+CAP_NET_RAW。
  python3 tap_ethernet_loopback.py [tap_name]
退出码 0 = 两个方向都通过。
"""
import fcntl
import os
import socket
import struct
import sys
import time

DEV = sys.argv[1] if len(sys.argv) > 1 else "tapspike0"

TUNSETIFF = 0x400454CA
IFF_TAP = 0x0002
IFF_NO_PI = 0x1000

SIOCSIFHWADDR = 0x8924
SIOCGIFFLAGS = 0x8913
SIOCSIFFLAGS = 0x8914
IFF_UP = 0x1
IFF_RUNNING = 0x40

ETH_P_ALL = 0x0003
ETH_P_ECAT = 0x88A4

# 说明用 MAC：本地管理位置位，避免与真实网卡冲突
SIM_MAC = bytes([0x02, 0xEC, 0x51, 0x4D, 0x00, 0x01])
MASTER_MAC = bytes([0x02, 0x4D, 0x41, 0x53, 0x00, 0x01])
BROADCAST = b"\xff" * 6


def create_tap(name):
    fd = os.open("/dev/net/tun", os.O_RDWR)
    ifr = struct.pack("16sH", name.encode(), IFF_TAP | IFF_NO_PI)
    res = fcntl.ioctl(fd, TUNSETIFF, ifr)
    actual = res[:16].split(b"\x00", 1)[0].decode()
    return fd, actual


def set_mac(name, mac):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        ifr = struct.pack("16sH14s8x", name.encode(), 1, mac)  # ARPHRD_ETHER=1
        fcntl.ioctl(s, SIOCSIFHWADDR, ifr)
    finally:
        s.close()


def get_flags(name):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        ifr = struct.pack("16sH38x", name.encode(), 0)
        res = fcntl.ioctl(s, SIOCGIFFLAGS, ifr)
        return struct.unpack("16sH", res[:18])[1]
    finally:
        s.close()


def set_up(name):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        flags = get_flags(name) | IFF_UP
        ifr = struct.pack("16sH38x", name.encode(), flags)
        fcntl.ioctl(s, SIOCSIFFLAGS, ifr)
    finally:
        s.close()


def eth_frame(dst, src, ethertype, payload):
    return dst + src + struct.pack("!H", ethertype) + payload


def disable_ipv6(name):
    """新 TAP 一旦 UP，内核会往里塞 IPv6 MLD/RS 报文；真实模拟器也必须按 EtherType 过滤，
    这里先关掉以减少噪声（双保险）。"""
    path = f"/proc/sys/net/ipv6/conf/{name}/disable_ipv6"
    try:
        with open(path, "w") as f:
            f.write("1")
        return True
    except OSError as exc:
        print(f"    (warn) 未能关闭 {name} 的 IPv6: {exc}")
        return False


def read_ecat_frame(fd, timeout=2.0):
    """从 TAP fd 读帧，跳过内核产生的非 0x88A4 帧（IPv6/ARP 等）。

    返回 (frame, skipped_count) 或 (None, skipped_count)。
    """
    import select

    deadline = time.time() + timeout
    skipped = 0
    while time.time() < deadline:
        r, _, _ = select.select([fd], [], [], max(0.0, deadline - time.time()))
        if not r:
            continue
        frame = os.read(fd, 65535)
        if len(frame) >= 14 and struct.unpack("!H", frame[12:14])[0] == ETH_P_ECAT:
            return frame, skipped
        skipped += 1
    return None, skipped


def main():
    fd, name = create_tap(DEV)
    print(f"[1] created TAP: {name} (fd={fd})")
    set_mac(name, SIM_MAC)
    disable_ipv6(name)
    set_up(name)
    time.sleep(0.2)
    flags = get_flags(name)
    print(f"[2] flags=0x{flags:04x} up={bool(flags & IFF_UP)} running={bool(flags & IFF_RUNNING)}")

    # 主站视角：绑定该 netdev 的 AF_PACKET 原始套接字
    pkt = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(ETH_P_ALL))
    pkt.bind((name, 0))
    pkt.settimeout(2.0)
    print(f"[3] AF_PACKET bound to {name} (ifindex={socket.if_nametoindex(name)})")

    payload = b"ECAT-SPIKE-1"
    frame = eth_frame(BROADCAST, SIM_MAC, ETH_P_ECAT, payload)

    # 方向 A：模拟器 -> 主站
    os.write(fd, frame)
    try:
        got = pkt.recv(65535)
    except socket.timeout:
        print("[4] FAIL 模拟器->主站: 超时未收到帧")
        cleanup(fd, name)
        return 2
    same = got[: len(frame)] == frame
    print(f"[4] 模拟器->主站: 收到 {len(got)} 字节, ethertype=0x{struct.unpack('!H', got[12:14])[0]:04X}, 内容一致={same}")
    if not same:
        print(f"    sent={frame.hex()}")
        print(f"    recv={got[:len(frame)].hex()}")

    # 方向 B：主站 -> 模拟器
    master_frame = eth_frame(BROADCAST, MASTER_MAC, ETH_P_ECAT, b"ECAT-SPIKE-2")
    pkt.send(master_frame)
    got2, skipped = read_ecat_frame(fd, timeout=2.0)
    ok2 = got2 is not None and got2[: len(master_frame)] == master_frame
    print(
        f"[5] 主站->模拟器: 收到 {None if got2 is None else len(got2)} 字节, "
        f"内容一致={ok2}, 跳过非 0x88A4 帧={skipped}"
    )
    if not ok2 and got2 is not None:
        print(f"    sent={master_frame.hex()}")
        print(f"    recv={got2.hex()}")

    cleanup(fd, name)
    ok = same and ok2
    print(f"[6] RESULT: {'PASS' if ok else 'FAIL'} (downlink={same}, uplink={ok2})")
    return 0 if ok else 3


def cleanup(fd, name):
    try:
        os.close(fd)
    except OSError:
        pass
    os.system(f"ip link del {name} 2>/dev/null")
    print(f"[7] cleaned up {name}")


if __name__ == "__main__":
    sys.exit(main())
