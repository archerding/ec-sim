#!/usr/bin/env python3
"""ESC-over-UDS 预回归：用合成主站验证 `ec-sim-esc --uds <path>` 的线缆层。

实现说明（只使用 Python 标准库，无第三方依赖）：
- 线缆：`SOCK_DGRAM` Unix domain socket，每个报文 = 一帧以太网数据（保留帧边界）。
- 帧格式按 ETG.1000.4：Ethernet 头 14B + EtherCAT 帧头 2B + datagram（10B 头 + data + WKC 2B）。
- 断言：广播读 ESC 类型；APRD AL 状态；APWR 站地址后 FPRD 命中；APRD DL Status
  (0x0110 = 0x5610)；一帧两 datagram（M 位）各自 WKC +1；ESM 迁移与 SM 过程数据看门狗。
- 默认按模拟器默认行为（ESM 前置条件检查开启）验证：邮箱 SM 未配置时拒绝 PREOP
  （AL Status Code 0x0015）、过程数据 SM 未配置时拒绝 SAFEOP（AL Status Code 0x0017），
  配置 SM 后 INIT→PREOP→SAFEOP→OP 全部成功。
- 加 `--lax` 对应 `ec-sim-esc --uds ... --esm-lax`（关闭前置条件检查）：不做拒绝断言，
  同样先按主站顺序配置 SM 再迁移。
- 加 `--sdo-segmented`（可配 `--expect-string=<text>`）：额外读取 0x1008 字符串对象，
  >4 字节时按 CiA 301 segmented upload（initiate 0x41 + segment 0x60）拼接并断言内容
  （用于验证声明式 manifest 画像的设备名）。

用法：python3 uds_master_probe.py [socket_path] [--timeout 2.0] [--lax]
        [--sdo-segmented] [--sdo-segmented-download] [--sdo-ca] [--sdo-ca-download]
        [--sdo-ca-download-multi] [--expect-string=<text>] [--cia402]
退出码 0 = 全部断言通过。
"""
import os
import socket
import struct
import sys
import time

ETHERCAT_ETHERTYPE = 0x88A4
FRAME_TYPE_ECAT_COMMAND = 1

CMD = {
    "APRD": 0x01,
    "APWR": 0x02,
    "FPRD": 0x04,
    "BRD": 0x07,
    "LWR": 0x0B,
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


def al_status(master):
    """APRD AL Status（0x0130）→ 1 字节。"""
    reply = master.transact([(CMD["APRD"], 0, 0x0130, b"\x00\x00")])
    check(reply[0][4] == 1, f"APRD AL Status WKC {reply[0][4]} != 1")
    return reply[0][3][0]


def al_status_code(master):
    """APRD AL Status Code（0x0134）→ u16。"""
    reply = master.transact([(CMD["APRD"], 0, 0x0134, b"\x00\x00")])
    check(reply[0][4] == 1, f"APRD AL Status Code WKC {reply[0][4]} != 1")
    return struct.unpack("<H", reply[0][3][:2])[0]


def write_sync_managers(master, address, entries):
    """APWR 写一段 SM 寄存器块；entries = [(start, length, control, status, activate, pdi), ...]。"""
    data = b"".join(struct.pack("<HHBBBB", *entry) for entry in entries)
    reply = master.transact([(CMD["APWR"], 0, address, data)])
    check(reply[0][4] == 1, f"APWR SM@{address:#06x} WKC {reply[0][4]} != 1")


def mailbox_exchange(master, service):
    """把一条 CoE 服务数据写入 SM0（0x1000），轮询 SM1 full 位后读回响应服务数据。

    邮箱头按 IgH/SOEM 约定：length = 服务数据长度（不含 6 字节头）、address、priority、
    type（CoE = 0x03）；full 位在 SM1 Status（0x080D）bit3。
    """
    message = bytearray(128)
    message[0:2] = struct.pack("<H", len(service))
    message[2:4] = struct.pack("<H", 0x1001)
    message[4] = 0x00
    message[5] = 0x03  # CoE
    message[6 : 6 + len(service)] = service
    reply = master.transact([(CMD["APWR"], 0, 0x1000, bytes(message))])
    check(reply[0][4] == 1, f"APWR SM0 mailbox WKC {reply[0][4]} != 1")

    full = False
    for _ in range(100):
        status = master.transact([(CMD["APRD"], 0, 0x080D, b"\x00")])[0][3][0]
        if status & 0x08:
            full = True
            break
        time.sleep(0.01)
    check(full, "SM1 mailbox full 位未置位（从站未响应）")

    raw = master.transact([(CMD["APRD"], 0, 0x1080, bytes(128))])[0][3]
    length = struct.unpack("<H", raw[0:2])[0]
    check(0 < length <= 122, f"邮箱响应长度 {length} 非法")
    return bytes(raw[6 : 6 + length])


def sdo_upload_ca(master, index):
    """Complete Access 上传（命令 0x50）：一次读回对象全部子索引数据。"""
    service = struct.pack("<HBHB", 0x02 << 12, 0x50, index, 0x00)
    response = mailbox_exchange(master, service)
    check(len(response) >= 10, f"CA 上传 0x{index:04X} 响应过短：{len(response)}")
    check(response[2] == 0x41, f"CA 上传 0x{index:04X} 响应命令 {response[2]:#04x} != 0x41")
    got_index = struct.unpack("<H", response[3:5])[0]
    check(got_index == index, f"CA 响应索引 0x{got_index:04X} != 0x{index:04X}")
    check(response[5] == 0x00, f"CA 响应子索引 {response[5]} != 0")
    size = struct.unpack("<I", response[6:10])[0]
    check(size == len(response) - 10, f"CA 数据长度 {size} != 实际 {len(response) - 10}")
    return response[10 : 10 + size]


def sdo_download_ca(master, index, data):
    """Complete Access 下载（命令 0x31，单帧）：数据按子索引长度拆分写入。"""
    service = struct.pack("<HBHBI", 0x02 << 12, 0x31, index, 0x00, len(data)) + data
    response = mailbox_exchange(master, service)
    check(response[2] == 0x60, f"CA 下载 0x{index:04X} 响应命令 {response[2]:#04x} != 0x60")
    got_index = struct.unpack("<H", response[3:5])[0]
    check(got_index == index, f"CA 下载响应索引 0x{got_index:04X} != 0x{index:04X}")
    check(response[5] == 0x00, f"CA 下载响应子索引 {response[5]} != 0")


def sdo_upload_ca_full(master, index):
    """CA 上传并在数据超出首帧时继续 segment 上传（返回完整数据）。"""
    service = struct.pack("<HBHB", 0x02 << 12, 0x50, index, 0x00)
    response = mailbox_exchange(master, service)
    check(len(response) >= 10, f"CA 上传 0x{index:04X} 响应过短：{len(response)}")
    check(response[2] == 0x41, f"CA 上传 0x{index:04X} 响应命令 {response[2]:#04x} != 0x41")
    check(struct.unpack("<H", response[3:5])[0] == index, "CA 响应索引回显不符")
    check(response[5] == 0x00, "CA 响应子索引不是 0")
    total = struct.unpack("<I", response[6:10])[0]
    data = bytearray(response[10:])
    toggle = 0
    guard = 0
    while len(data) < total:
        guard += 1
        check(guard <= 64, "CA 分段上传段数超限")
        segment_request = (
            b"\x00\x20"
            + bytes([0x60 | (toggle << 4)])
            + struct.pack("<H", index)
            + bytes([0x00, 0x00])
        )
        response = mailbox_exchange(master, segment_request)
        check(len(response) == 10, f"CA segment 响应长度 {len(response)} != 10")
        command = response[2]
        check((command & 0xE0) == 0x00, f"CA segment 响应命令 {command:#04x} 非法")
        check(((command >> 4) & 0x01) == toggle, "CA segment 响应 toggle 未回显")
        unused = (command >> 1) & 0x07
        data += response[3 : 3 + (7 - unused)]
        toggle ^= 1
        if command & 0x01:
            break
    return bytes(data[:total])


def sdo_download_ca_multi(master, index, data):
    """CA 分段下载（命令 0x31 首帧 + 0x20|toggle<<4|unused<<1|last 多段）。"""
    first = data[:112]
    service = struct.pack("<HBHBI", 0x02 << 12, 0x31, index, 0x00, len(data)) + first
    response = mailbox_exchange(master, service)
    check(response[2] == 0x60, f"CA 下载 initiate 响应 {response[2]:#04x} != 0x60")
    rest = data[112:]
    offset = 0
    toggle = 0
    while offset < len(rest):
        chunk = rest[offset : offset + 7]
        offset += len(chunk)
        unused = 7 - len(chunk)
        last = 1 if offset == len(rest) else 0
        service = (
            struct.pack("<H", 0x02 << 12)
            + bytes([0x20 | (toggle << 4) | (unused << 1) | last])
            + chunk
        )
        response = mailbox_exchange(master, service)
        check(
            response[2] & 0xE0 == 0x20,
            f"CA segment 下载响应命令 {response[2]:#04x} 非法",
        )
        check(
            ((response[2] >> 4) & 0x01) == toggle,
            "CA segment 下载响应 toggle 未回显",
        )
        toggle ^= 1


def sdo_upload_segmented(master, index, subindex):
    """用 SDO 上传读取对象；>4 字节对象自动走 segmented upload（主站侧实现）。"""
    # initiate upload request：CoE 服务 0x2<<12、Command 0x40、Index、SubIndex。
    request = b"\x00\x20" + bytes([0x40]) + struct.pack("<H", index) + bytes([subindex])
    response = mailbox_exchange(master, request)
    check(len(response) >= 6, f"SDO 响应过短：{len(response)}")
    check(response[0:2] == b"\x00\x30", "CoE 服务类型不是 SDO 响应")
    command = response[2]
    check(command & 0x40, "SDO 命令不是 upload response")
    check(struct.unpack("<H", response[3:5])[0] == index, "SDO 索引回显不符")
    check(response[5] == subindex, "SDO 子索引回显不符")

    if command & 0x02:  # expedited
        size = (4 - ((command >> 2) & 0x03)) if command & 0x01 else 4
        return bytes(response[6 : 6 + size])

    # normal upload：+6 u32 总长度，+10 首段数据；不足则发 segment 请求。
    total = struct.unpack("<I", response[6:10])[0]
    data = bytearray(response[10:])
    toggle = 0
    guard = 0
    while len(data) < total:
        guard += 1
        check(guard <= 64, "分段上传段数超限")
        segment_request = (
            b"\x00\x20"
            + bytes([0x60 | (toggle << 4)])
            + struct.pack("<H", index)
            + bytes([subindex, 0x00])
        )
        response = mailbox_exchange(master, segment_request)
        check(len(response) == 10, f"segment 响应长度 {len(response)} != 10")
        check(response[0:2] == b"\x00\x30", "segment 响应 CoE 类型不符")
        command = response[2]
        check((command & 0xE0) == 0x00, f"segment 响应命令 {command:#04x} 非法")
        check(((command >> 4) & 0x01) == toggle, "segment 响应 toggle 未回显")
        unused = (command >> 1) & 0x07
        last = command & 0x01
        data += response[3 : 3 + (7 - unused)]
        toggle ^= 1
        if last:
            break
    check(len(data) == total, f"分段上传长度 {len(data)} != {total}")
    return bytes(data)


def main():
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    socket_path = arguments[0] if arguments else DEFAULT_SOCKET
    timeout = 2.0
    lax = False
    sdo_segmented = False
    sdo_segmented_download = False
    expect_string = None
    cia402 = False
    cia402_csv = False
    sdo_ca = False
    sdo_ca_download = False
    sdo_ca_download_multi = False
    for arg in sys.argv[1:]:
        if arg.startswith("--timeout="):
            timeout = float(arg.split("=", 1)[1])
        # `--lax` 对应 `ec-sim-esc --esm-lax`（关闭前置条件检查）；`--strict` 是默认行为，保留兼容。
        if arg == "--lax":
            lax = True
        # `--sdo-segmented`：额外验证 >4 字节对象（0x1008 设备名）的 segmented SDO 上传。
        if arg == "--sdo-segmented":
            sdo_segmented = True
        if arg == "--sdo-segmented-download":
            sdo_segmented_download = True
        if arg == "--sdo-ca-download-multi":
            sdo_ca_download_multi = True
        if arg == "--sdo-ca-download":
            sdo_ca_download = True
        if arg == "--sdo-ca":
            sdo_ca = True
        if arg == "--cia402-csv":
            cia402_csv = True
        if arg == "--cia402":
            cia402 = True
        if arg.startswith("--expect-string="):
            expect_string = arg.split("=", 1)[1]

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

        # 5b) 可选：segmented SDO 上传（>4 字节字符串对象，如 manifest 的 0x1008 设备名）。
        #     主站侧按 CiA 301 发送 initiate + segment 请求，断言拼出的字符串与 manifest 一致。
        if sdo_segmented:
            value = sdo_upload_segmented(master, 0x1008, 0x00)
            text = value.decode("utf-8", errors="replace")
            if expect_string is not None:
                check(text == expect_string, f"SDO 分段读取 {text!r} != 期望 {expect_string!r}")
            print(f"OK: segmented SDO 读到 0x1008 = {text!r}（{len(value)} 字节）")

        # 5d) 可选：segmented download（>4 字节对象，写后回读）。
        if sdo_segmented_download:
            payload = b"EC-Sim SDO download!"
            first = payload[:8]
            service = (
                b"\x00\x20\x21"
                + struct.pack("<H", 0x2100)
                + b"\x00"
                + struct.pack("<I", len(payload))
                + first
            )
            response = mailbox_exchange(master, service)
            check(
                len(response) >= 6 and response[2] == 0x60,
                f"download initiate 响应 {response[2]:#04x} != 0x60",
            )
            offset = len(first)
            toggle = 0
            while offset < len(payload):
                remaining = len(payload) - offset
                take = min(7, remaining)
                last = offset + take >= len(payload)
                unused = 7 - take
                chunk = payload[offset : offset + take].ljust(7, b"\x00")
                command = 0x20 | (toggle << 4) | (unused << 1) | (1 if last else 0)
                service = b"\x00\x20" + bytes([command]) + chunk
                response = mailbox_exchange(master, service)
                check(
                    len(response) >= 3 and (response[2] & 0xE0) == 0x20,
                    "segment download 响应命令不是 0x20",
                )
                check(((response[2] >> 4) & 1) == toggle, "segment download toggle 未回显")
                offset += take
                toggle ^= 1
            value = sdo_upload_segmented(master, 0x2100, 0x00)
            check(value == payload, f"SDO 回读 {value!r} != {payload!r}")
            print("OK: segmented SDO download 写入 0x2100 并回读一致（20 字节）")

        # 5f) 可选：Complete Access 上传（命令 0x50，一次读回对象全部子索引）。
        if sdo_ca:
            data = sdo_upload_ca(master, 0x1C12)
            check(
                data == b"\x01\x00\x16",
                f"CA 0x1C12 数据 {data.hex()} != 01 00 16（计数 1 + 0x1600）",
            )
            data = sdo_upload_ca(master, 0x1600)
            check(
                data == b"\x01\x08\x00\x00\x70",
                f"CA 0x1600 数据 {data.hex()} != 01 08 00 00 70（计数 1 + 0x70000008）",
            )
            print("OK: Complete Access 上传（0x1C12/0x1600 全部子索引一次性读回）通过")

        # 5h) 可选：Complete Access **分段**下载（>首帧 112 字节；需 manifest 提供足够大的
        # 可写对象，如 generic-cia402-csv-servo 的 0x2100 = 140 字节）。
        if sdo_ca_download_multi:
            payload = bytes(0x41 + (i % 26) for i in range(140))
            sdo_download_ca_multi(master, 0x2100, payload)
            readback = sdo_upload_ca_full(master, 0x2100)
            check(
                readback == payload,
                f"CA 分段下载回读不一致（{len(readback)} 字节）",
            )
            print("OK: Complete Access 分段下载（140 字节对象多段写入并回读一致）通过")

        # 5g) 可选：Complete Access 下载（命令 0x31，单帧写全部子索引并回读）。
        if sdo_ca_download:
            sdo_download_ca(master, 0x1600, b"\x01\x08\x00\x00\x70")
            data = sdo_upload_ca(master, 0x1600)
            check(
                data == b"\x01\x08\x00\x00\x70",
                f"CA 下载后回读 {data.hex()} != 01 08 00 00 70",
            )
            print("OK: Complete Access 下载（0x1600 写入全部子索引并回读一致）通过")

        # 5e) 可选：CiA 402 CSV（11 字节 PDO，速度模式，Tier0 累加）。
        if cia402_csv:
            sequence = [
                (0x0006, 9, 0, 5, 0x0221, 0, 0),
                (0x0007, 9, 0, 5, 0x0223, 0, 0),
                # 每个报文都会让行为 tick 一次：写帧 tick 用上一控制字、读帧 tick 用新控制字，
                # 因此读到的位置按"写帧 + 读帧"各累加一次。
                (0x000F, 9, 0, 5, 0x0237, 5, 5),
                (0x000F, 9, 0, 5, 0x0237, 15, 5),
                (0x0000, 9, 0, 5, 0x0240, 20, 0),
            ]
            for controlword, mode, target, velocity, expected_sw, expected_pos, expected_vel in sequence:
                master.transact(
                    [(CMD["APWR"], 0, 0x1100, struct.pack("<Hbii", controlword, mode, target, velocity))]
                )
                reply = master.transact([(CMD["APRD"], 0, 0x1110, bytes(11))])
                sw, modes_display, position, velocity_actual = struct.unpack(
                    "<Hbii", reply[0][3][:11]
                )
                check(sw == expected_sw, f"CiA402-CSV sw={sw:#06x} != {expected_sw:#06x}")
                check(modes_display == mode, f"CiA402-CSV 模式显示 {modes_display} != {mode}")
                check(
                    position == expected_pos,
                    f"CiA402-CSV 位置 {position} != {expected_pos}",
                )
                check(
                    velocity_actual == expected_vel,
                    f"CiA402-CSV 速度显示 {velocity_actual} != {expected_vel}",
                )
            print("OK: CiA 402 CSV：速度模式位置累加/速度显示通过（11 字节 PDO）")

        # 5c) 可选：CiA 402 状态机行为（完整 7 字节 RxPDO/TxPDO，Tier0）。
        if cia402:
            # (controlword, mode, target, 期望状态字, 期望实际位置)
            sequence = [
                (0x0000, 8, 1000, 0x0240, 0),      # Switch on disabled，位置 0
                (0x0006, 8, 1000, 0x0221, 0),      # Shutdown → Ready
                (0x0007, 8, 1000, 0x0223, 0),      # Switch on → Switched on
                (0x000F, 8, 1000, 0x0237 | 0x0400, 1000),  # Enable op → Operation enabled + Target reached
                (0x000F, 9, 2000, 0x0237, 1000),       # 切 CSV：位置冻结、无 Target reached
                (0x0000, 8, 1000, 0x0240, 1000),   # Disable voltage，位置保持
            ]
            for controlword, mode, target, expected_sw, expected_pos in sequence:
                rx = struct.pack("<Hbi", controlword, mode, target)
                master.transact([(CMD["APWR"], 0, 0x1100, rx)])
                reply = master.transact([(CMD["APRD"], 0, 0x1110, bytes(7))])
                status, modes_display, position = struct.unpack("<Hbi", reply[0][3][:7])
                check(
                    status == expected_sw,
                    f"CiA402 cw={controlword:#06x} → sw={status:#06x}，期望 {expected_sw:#06x}",
                )
                check(
                    modes_display == mode,
                    f"CiA402 模式显示 {modes_display} != 模式 {mode}",
                )
                check(
                    position == expected_pos,
                    f"CiA402 实际位置 {position} != 期望 {expected_pos}",
                )
            print("OK: CiA 402 行为：完整 7 字节 RxPDO/TxPDO 序列通过（状态字/模式显示/实际位置）")

        # 6) ESM 迁移 + SM 过程数据看门狗（B5 子项）。
        #    默认（模拟器默认开启前置条件检查）：先验证拒绝路径（SM 未配置 → 0x0015 / 0x0017），
        #    再按主站顺序配置 SM 并放行 INIT→PREOP→SAFEOP→OP；
        #    --lax（对应 `ec-sim-esc --esm-lax`）：不做拒绝断言，同样先配置 SM 再迁移。
        mailbox_sms = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        # 过程数据 SM 先以 control 0x04（写方向、无看门狗）配置，看门狗位留给后面的 SM2 重写。
        process_data_sms = [(0x1100, 8, 0x04, 0, 1, 0), (0x1110, 8, 0x00, 0, 1, 0)]
        if not lax:
            # a) 邮箱 SM 未配置 → 拒绝 PREOP：error 位 + AL Status Code 0x0015，状态保持 INIT。
            master.transact([(CMD["APWR"], 0, 0x0120, bytes([0x02]))])
            status = al_status(master)
            check(status & 0x10, f"邮箱 SM 缺失时 AL Status {status:#04x} 未置 error 位")
            check(
                status & 0x0F == 0x01,
                f"前置条件失败后状态 {status & 0x0F:#04x} 不是 INIT",
            )
            check(
                al_status_code(master) == 0x0015,
                "AL Status Code 不是 0x0015（邮箱配置无效）",
            )
            # 主站按 IgH fsm_change.c 的做法 ACK（写当前状态 + ACK 位）清错。
            master.transact([(CMD["APWR"], 0, 0x0120, bytes([0x11]))])
            check(al_status(master) & 0x10 == 0, "ACK 后 error 位未清除")
        # b) 配置邮箱 SM0/SM1 → PREOP 被接受。
        write_sync_managers(master, 0x0800, mailbox_sms)
        reply = master.transact([(CMD["APWR"], 0, 0x0120, bytes([0x02]))])
        check(reply[0][4] == 1, f"AL control 0x02 WKC {reply[0][4]} != 1")
        status = al_status(master)
        check(status & 0x0F == 0x02, f"配置邮箱 SM 后状态 {status:#04x} 不是 PREOP")
        check(status & 0x10 == 0, f"配置正确时不应置 error 位（{status:#04x}）")
        if not lax:
            # c) 过程数据 SM 未配置 → 拒绝 SAFEOP：error 位 + AL Status Code 0x0017。
            master.transact([(CMD["APWR"], 0, 0x0120, bytes([0x04]))])
            status = al_status(master)
            check(status & 0x10, f"过程数据 SM 缺失时 AL Status {status:#04x} 未置 error 位")
            check(
                status & 0x0F == 0x02,
                f"前置条件失败后状态 {status & 0x0F:#04x} 不是 PREOP",
            )
            check(
                al_status_code(master) == 0x0017,
                "AL Status Code 不是 0x0017（SM 配置无效）",
            )
            master.transact([(CMD["APWR"], 0, 0x0120, bytes([0x14]))])
            check(al_status(master) & 0x10 == 0, "ACK 后 error 位未清除")
        # d) 配置过程数据 SM2/SM3 → SAFEOP → OP。
        write_sync_managers(master, 0x0810, process_data_sms)
        for control in (0x04, 0x08):
            reply = master.transact([(CMD["APWR"], 0, 0x0120, bytes([control]))])
            check(reply[0][4] == 1, f"AL control {control:#04x} WKC 不是 1")
        status = al_status(master)
        check(
            status & 0x0F == 0x08 and status & 0x10 == 0,
            f"最终状态 {status:#04x} 不是 OP（无错误）",
        )
        # divider = 2500（40 ns 单位）、intervals = 100 → 10 ms。
        master.transact([(CMD["APWR"], 0, 0x0400, struct.pack("<H", 2500))])
        master.transact([(CMD["APWR"], 0, 0x0420, struct.pack("<H", 100))])
        # SM2：0x1100 / 8 字节 / control 0x44（写方向 + watchdog bit6）/ activate。
        sm2 = struct.pack("<HHBBBB", 0x1100, 8, 0x44, 0x00, 0x01, 0x00)
        master.transact([(CMD["APWR"], 0, 0x0810, sm2)])
        # FMMU0：逻辑 0x1000 → 物理 0x1100，8 字节，type=0x02（写），activate。
        fmmu = struct.pack("<IHBBHBBB", 0x1000, 8, 0, 7, 0x1100, 0, 0x02, 0x01) + b"\x00\x00\x00"
        master.transact([(CMD["APWR"], 0, 0x0600, fmmu)])
        # 写一次过程数据（LWR 逻辑 0x1000），重置看门狗；LWR 的 WKC 应为 1。
        reply = master.transact([(CMD["LWR"], 0x1000, 0, b"\x00" * 8)])
        check(reply[0][4] == 1, f"LWR WKC {reply[0][4]} != 1")
        # 等 > 10 ms 再发一帧（帧驱动时钟推进），读 AL Status / Code。
        time.sleep(0.02)
        reply = master.transact([(CMD["APRD"], 0, 0x0130, b"\x00\x00")])
        status = reply[0][3][0]
        check(status & 0x10, f"看门狗未置 error 位（AL Status {status:#04x}）")
        check(status & 0x0F == 0x04, f"看门狗触发后状态 {status & 0x0F:#04x} 不是 SAFEOP")
        reply = master.transact([(CMD["APRD"], 0, 0x0134, b"\x00\x00")])
        code = struct.unpack("<H", reply[0][3][:2])[0]
        check(code == 0x001B, f"AL Status Code {code:#06x} != 0x001B")
        # Error ACK 清错（SAFEOP|ACK）。
        master.transact([(CMD["APWR"], 0, 0x0120, bytes([0x12]))])
        reply = master.transact([(CMD["APRD"], 0, 0x0130, b"\x00\x00")])
        status = reply[0][3][0]
        check(status & 0x10 == 0, "Error ACK 后 error 位未清除")
    finally:
        master.close()

    mode = "（--lax：跳过前置条件拒绝断言）" if lax else " + ESM 前置条件（0x0015/0x0017）"
    print(
        f"OK: ESC-over-UDS 线缆层可用（{socket_path}）：BRD/APRD/APWR/FPRD/M 位 + "
        f"SM watchdog（10 ms → SAFEOP+ERROR 0x001B → ACK 清错）{mode}均通过"
    )


if __name__ == "__main__":
    main()
