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

用法：python3 uds_master_probe.py [socket_path] [--timeout 2.0] [--lax]
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


def main():
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    socket_path = arguments[0] if arguments else DEFAULT_SOCKET
    timeout = 2.0
    lax = False
    for arg in sys.argv[1:]:
        if arg.startswith("--timeout="):
            timeout = float(arg.split("=", 1)[1])
        # `--lax` 对应 `ec-sim-esc --esm-lax`（关闭前置条件检查）；`--strict` 是默认行为，保留兼容。
        if arg == "--lax":
            lax = True

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
