#!/usr/bin/env python3
"""ESC-over-UDS 预回归：用合成主站验证 `ec-sim-esc --uds <path>` 的线缆层。

实现说明（只使用 Python 标准库，无第三方依赖）：
- 线缆：`SOCK_DGRAM` Unix domain socket，每个报文 = 一帧以太网数据（保留帧边界）。
- 帧格式按 ETG.1000.4：Ethernet 头 14B + EtherCAT 帧头 2B + datagram（10B 头 + data + WKC 2B）。
- 断言：广播读 ESC 类型；APRD AL 状态；APWR 站地址后 FPRD 命中；APRD DL Status
  (0x0110 = 0x5610)；一帧两 datagram（M 位）各自 WKC +1。

用法：python3 uds_master_probe.py [socket_path] [--timeout 2.0]
退出码 0 = 全部断言通过。
"""
import os
import socket
import struct
import sys

ETHERCAT_ETHERTYPE = 0x88A4
FRAME_TYPE_ECAT_COMMAND = 1

CMD = {
    "APRD": 0x01,
    "APWR": 0x02,
    "FPRD": 0x04,
    "BRD": 0x07,
}

DEFAULT_SOCKET = "/tmp/ecsim-uds.sock"
EXPECTED_DL_STATUS = 0x5610  # esc-node DL_STATUS_INITIAL（单从站、单链路）


def build_frame(datagrams):
    """datagrams: [(command, adp, ado, data_bytes), ...] -> Ethernet 帧字节。"""
    body = b""
    last = len(datagrams) - 1
    for index, (command, adp, ado, data) in enumerate(datagrams):
        more = 0x8000 if index != last else 0
        # datagram 头 10 字节：cmd(1) idx(1) ADP(2) ADO(2) len/flags(2) IRQ(2)。
        body += struct.pack("<BBHHHH", command, index, adp, ado, (len(data) & 0x07FF) | more, 0)
        body += data + b"\x00\x00"  # WKC 由从站累加
    payload = struct.pack("<H", ((FRAME_TYPE_ECAT_COMMAND & 0x0F) << 12) | len(body)) + body
    ethernet = (
        b"\xff" * 6  # 广播目的
        + b"\x02\x00\x00\x00\x00\x01"  # 合成主站源 MAC
        + struct.pack(">H", ETHERCAT_ETHERTYPE)
    )
    return ethernet + payload


def parse_reply(frame):
    """解析应答帧，返回 [(command, adp, ado, data, wkc), ...]。"""
    payload = frame[14:]
    declared = struct.unpack("<H", payload[:2])[0] & 0x07FF
    body = payload[2 : 2 + declared]
    out = []
    offset = 0
    while offset + 12 <= len(body):
        command = body[offset]
        adp, ado, flags = struct.unpack("<HHH", body[offset + 2 : offset + 8])  # IRQ 在 +8..+10 忽略
        length = flags & 0x07FF
        data_start = offset + 10
        data = body[data_start : data_start + length]
        wkc = struct.unpack("<H", body[data_start + length : data_start + length + 2])[0]
        out.append((command, adp, ado, data, wkc))
        offset = data_start + length + 2
        if not flags & 0x8000:
            break
    return out


class SyntheticMaster:
    def __init__(self, socket_path, timeout=2.0):
        self.socket_path = socket_path
        self.client_path = f"{socket_path}.client{os.getpid()}"
        try:
            os.unlink(self.client_path)
        except FileNotFoundError:
            pass
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
        self.sock.bind(self.client_path)
        self.sock.connect(socket_path)
        self.timeout = timeout

    def transact(self, datagrams):
        self.sock.settimeout(self.timeout)
        self.sock.send(build_frame(datagrams))
        return parse_reply(self.sock.recv(2048))

    def close(self):
        self.sock.close()
        try:
            os.unlink(self.client_path)
        except FileNotFoundError:
            pass


def check(condition, message):
    if not condition:
        raise SystemExit(f"FAIL: {message}")


def main():
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    socket_path = arguments[0] if arguments else DEFAULT_SOCKET
    timeout = 2.0
    for arg in sys.argv[1:]:
        if arg.startswith("--timeout="):
            timeout = float(arg.split("=", 1)[1])

    master = SyntheticMaster(socket_path, timeout)
    try:
        # 1) 广播读 ESC 类型寄存器 0x0000（BRD，2 字节）。
        reply = master.transact([(CMD["BRD"], 0, 0x0000, b"\x00\x00")])
        check(len(reply) == 1, f"BRD 应答 datagram 数 {len(reply)} != 1")
        check(reply[0][4] == 1, f"BRD WKC {reply[0][4]} != 1")
        check(reply[0][3][0] == 0x01, f"ESC 类型 {reply[0][3][0]:#04x} != 0x01")

        # 2) APRD AL Status（0x0130）：INIT=0x01。
        reply = master.transact([(CMD["APRD"], 0, 0x0130, b"\x00\x00")])
        check(reply[0][4] == 1, f"APRD AL Status WKC {reply[0][4]} != 1")
        check(reply[0][3][0] & 0x0F == 0x01, f"AL 状态 {reply[0][3][0]:#04x} 不是 INIT")

        # 3) APWR 配置站地址 0x1001，然后 FPRD 命中。
        reply = master.transact([(CMD["APWR"], 0, 0x0010, b"\x01\x10")])
        check(reply[0][4] == 1, f"APWR 站地址 WKC {reply[0][4]} != 1")
        reply = master.transact([(CMD["FPRD"], 0x1001, 0x0130, b"\x00\x00")])
        check(reply[0][4] == 1, f"FPRD(0x1001) WKC {reply[0][4]} != 1")

        # 4) APRD DL Status（0x0110）必须是单从站正确值 0x5610。
        reply = master.transact([(CMD["APRD"], 0, 0x0110, b"\x00\x00")])
        dl_status = struct.unpack("<H", reply[0][3][:2])[0]
        check(dl_status == EXPECTED_DL_STATUS, f"DL Status {dl_status:#06x} != {EXPECTED_DL_STATUS:#06x}")

        # 5) 一帧两 datagram（M 位）：广播读 0x0000 与 AL Status，各自 WKC +1。
        reply = master.transact(
            [
                (CMD["BRD"], 0, 0x0000, b"\x00\x00"),
                (CMD["BRD"], 0, 0x0130, b"\x00\x00"),
            ]
        )
        check(len(reply) == 2, f"M 位应答 datagram 数 {len(reply)} != 2")
        check(reply[0][4] == 1 and reply[1][4] == 1, "多 datagram 帧 WKC 不是各 +1")
    finally:
        master.close()

    print(f"OK: ESC-over-UDS 线缆层可用（{socket_path}）：BRD/APRD/APWR/FPRD/M 位均通过")


if __name__ == "__main__":
    main()
