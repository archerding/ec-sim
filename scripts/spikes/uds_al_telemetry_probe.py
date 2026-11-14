#!/usr/bin/env python3
"""AL 遥测 + SII 并发读 + 看门狗恢复预回归（EC-Sim `--uds` 合成主站）。

场景（全部在 EC-Sim 自己的 UDS 上，不依赖 TAP/root）：
1. 按主站顺序配置邮箱 SM 与过程数据 SM，INIT→PREOP→SAFEOP→OP；
2. **SII 并发读**：过程数据（SM2/LWR）持续活跃时，反复读 SII word 0x0008（Vendor ID），
   断言每次读都成功（无 busy/error 位）且值稳定，AL 保持 OP、无错误；
3. **总线丢帧恢复**：停止过程数据 > 看门狗超时（10 ms），下一帧到达时触发 SM watchdog
   （AL Status error 位 + 0x001B、OP→SAFEOP）；主站 Error ACK 清错、重新请求 OP 成功；
4. **遥测口径**：通过 `--serve` 的 `/state`、`/metrics` 断言计数与最后事件：
   `stats.sii_read_commands` 增加、`sm_watchdog_triggers`/`al_errors`/`al_error_acks` 各 ≥1、
   `last_al_event.watchdog` 真假切换；并检查进程 stdout 中 `AL_EVENT` 行（由调用方脚本检查）。

用法：python3 uds_al_telemetry_probe.py <socket_path> [--http-base http://127.0.0.1:PORT]
退出码 0 = 全部断言通过；非 0 = 断言失败/连接失败。
"""
import json
import os
import struct
import sys
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import uds_master_probe as probe  # noqa: E402

SII_WORD_VENDOR = 0x0008
SII_VENDOR_ID = 0x0000_0EC5
WATCHDOG_TIMEOUT_S = 0.03  # > 10 ms 的看门狗超时窗口
SII_CONCURRENT_READS = 40


def http_json(base, path):
    with urllib.request.urlopen(base + path, timeout=3.0) as response:
        assert response.status == 200, (path, response.status)
        return json.loads(response.read().decode("utf-8"))


def http_text(base, path):
    with urllib.request.urlopen(base + path, timeout=3.0) as response:
        assert response.status == 200, (path, response.status)
        return response.read().decode("utf-8")


def live_state(base):
    state = http_json(base, "/state")
    live = state.get("live")
    probe.check(isinstance(live, dict), f"/state 缺少 live 字段：{state}")
    return live


def metric_value(text, name, index=0):
    prefix = f'{name}{{slave="{index}",'
    for line in text.splitlines():
        if line.startswith(prefix):
            return int(line.rsplit(" ", 1)[1])
    raise AssertionError(f"/metrics 缺少 {name}（slave={index}）")


def sii_read_word(master, word):
    """IgH fsm_sii 读序列：写 4 字节命令到 0x0502，再读 10 字节（数据在偏移 6..10）。"""
    command = struct.pack("<BBH", 0x80, 0x01, word)
    reply = master.transact([(probe.CMD["APWR"], 0, 0x0502, command)])
    probe.check(reply[0][4] == 1, f"APWR SII 命令 WKC {reply[0][4]} != 1")
    reply = master.transact([(probe.CMD["APRD"], 0, 0x0502, bytes(10))])
    probe.check(reply[0][4] == 1, f"APRD SII 结果 WKC {reply[0][4]} != 1")
    data = reply[0][3]
    status = struct.unpack("<H", data[:2])[0]
    probe.check(status & 0x8000 == 0, f"SII busy 位未清零：{status:#06x}")
    probe.check(status & 0x2000 == 0, f"SII error 位被置位：{status:#06x}")
    return struct.unpack("<I", data[6:10])[0]


def write_process_data(master, value):
    reply = master.transact([(probe.CMD["LWR"], 0x1000, 0, struct.pack("<I", value) + bytes(4))])
    probe.check(reply[0][4] == 1, f"LWR WKC {reply[0][4]} != 1")


def main():
    socket_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/ecsim-uds.sock"
    http_base = None
    args = sys.argv[2:]
    while args:
        arg = args.pop(0)
        if arg == "--http-base":
            http_base = args.pop(0).rstrip("/")
        else:
            raise SystemExit(f"未知参数：{arg}")

    master = probe.SyntheticMaster(socket_path, timeout=2.0)
    try:
        # 1) 配置邮箱 SM → PREOP；配置过程数据 SM → SAFEOP → OP。
        mailbox_sms = [(0x1000, 128, 0x26, 0, 1, 0), (0x1080, 128, 0x22, 0, 1, 0)]
        probe.write_sync_managers(master, 0x0800, mailbox_sms)
        master.transact([(probe.CMD["APWR"], 0, 0x0120, bytes([0x02]))])
        probe.check(probe.al_status(master) & 0x0F == 0x02, "配置邮箱 SM 后未进入 PREOP")

        process_sms = [(0x1100, 8, 0x04, 0, 1, 0), (0x1110, 8, 0x00, 0, 1, 0)]
        probe.write_sync_managers(master, 0x0810, process_sms)
        for control in (0x04, 0x08):
            master.transact([(probe.CMD["APWR"], 0, 0x0120, bytes([control]))])
        status = probe.al_status(master)
        probe.check(status & 0x0F == 0x08 and status & 0x10 == 0,
                    f"ESM 未进入 OP：AL Status {status:#04x}")

        # 2) 开启 SM2 看门狗（10 ms）并写一次过程数据。
        master.transact([(probe.CMD["APWR"], 0, 0x0400, struct.pack("<H", 2500))])
        master.transact([(probe.CMD["APWR"], 0, 0x0420, struct.pack("<H", 100))])
        sm2 = struct.pack("<HHBBBB", 0x1100, 8, 0x44, 0x00, 0x01, 0x00)
        master.transact([(probe.CMD["APWR"], 0, 0x0810, sm2)])
        fmmu = struct.pack("<IHBBHBBB", 0x1000, 8, 0, 7, 0x1100, 0, 0x02, 0x01) + bytes(3)
        master.transact([(probe.CMD["APWR"], 0, 0x0600, fmmu)])
        write_process_data(master, 1)

        if http_base:
            before = live_state(http_base)
            probe.check(before["al_status"] == "0x08", f"OP 前 /state al_status={before['al_status']}")
            before_sii = before["stats"]["sii_read_commands"]

        # 3) 过程数据活跃时并发读 SII（每轮先写 PDO，再读 SII word 0x0008）。
        for round_index in range(SII_CONCURRENT_READS):
            write_process_data(master, round_index)
            vendor = sii_read_word(master, SII_WORD_VENDOR)
            probe.check(vendor == SII_VENDOR_ID,
                        f"第 {round_index} 轮 SII Vendor ID {vendor:#010x} != {SII_VENDOR_ID:#010x}")
            if round_index % 10 == 0:
                status = probe.al_status(master)
                probe.check(status & 0x0F == 0x08 and status & 0x10 == 0,
                            f"SII 并发读期间 AL 非 OP/有错：{status:#04x}")
        write_process_data(master, 0xFFFF)

        if http_base:
            after_sii = live_state(http_base)
            stats = after_sii["stats"]
            probe.check(stats["sii_read_commands"] >= before_sii + SII_CONCURRENT_READS,
                        f"SII 读计数未累计：{before_sii} -> {stats['sii_read_commands']}")
            probe.check(after_sii["last_al_event"] is None,
                        f"SII 并发期间不应产生 AL 事件：{after_sii['last_al_event']}")
            probe.check(stats["sm_watchdog_triggers"] == 0,
                        f"SII 并发期间不应触发看门狗：{stats['sm_watchdog_triggers']}")

        # 4) 停止过程数据 > 10 ms → 下一帧触发 SM watchdog（OP→SAFEOP + 0x001B）。
        time.sleep(WATCHDOG_TIMEOUT_S)
        status = probe.al_status(master)
        probe.check(status & 0x10 == 0x10, f"看门狗未置 error 位：AL Status {status:#04x}")
        probe.check(status & 0x0F == 0x04, f"看门狗触发后未回到 SAFEOP：{status:#04x}")
        probe.check(probe.al_status_code(master) == 0x001B,
                    f"AL Status Code 不是 0x001B：{probe.al_status_code(master):#06x}")
        if http_base:
            watchdog_state = live_state(http_base)
            probe.check(watchdog_state["stats"]["sm_watchdog_triggers"] >= 1,
                        "看门狗触发计数未增加")
            probe.check(watchdog_state["stats"]["al_errors"] >= 1, "AL error 计数未增加")
            event = watchdog_state["last_al_event"]
            probe.check(isinstance(event, dict) and event["watchdog"] is True,
                        f"last_al_event 未标记看门狗：{event}")
            probe.check(event["to"] == "0x14", f"last_al_event.to={event['to']} != 0x14")
            probe.check(event["code"] == "0x001B", f"last_al_event.code={event['code']} != 0x001B")

        # 5) Error ACK 清错，重新请求 OP，确认恢复。
        master.transact([(probe.CMD["APWR"], 0, 0x0120, bytes([0x14]))])
        status = probe.al_status(master)
        probe.check(status & 0x10 == 0, f"Error ACK 后 error 位未清除：{status:#04x}")
        master.transact([(probe.CMD["APWR"], 0, 0x0120, bytes([0x08]))])
        status = probe.al_status(master)
        probe.check(status & 0x0F == 0x08 and status & 0x10 == 0,
                    f"重新请求 OP 失败：AL Status {status:#04x}")
        write_process_data(master, 7)
        probe.check(sii_read_word(master, SII_WORD_VENDOR) == SII_VENDOR_ID,
                    "恢复 OP 后 SII 读失败")
        if http_base:
            recovered = live_state(http_base)
            probe.check(recovered["al_status"] == "0x08", f"恢复后 /state al_status={recovered['al_status']}")
            probe.check(recovered["stats"]["al_error_acks"] >= 1, "AL error ack 计数未增加")
            probe.check(recovered["last_al_event"]["watchdog"] is False,
                        "恢复后 last_al_event 应不再是看门狗事件")
            metrics = http_text(http_base, "/metrics")
            probe.check(metric_value(metrics, "ecsim_sm_watchdog_triggers_total") >= 1,
                        "/metrics 缺少看门狗触发计数")
            probe.check(metric_value(metrics, "ecsim_sii_read_commands_total") >= SII_CONCURRENT_READS,
                        "/metrics 缺少 SII 读计数")
    finally:
        master.close()

    print(f"OK: SII 并发读 {SII_CONCURRENT_READS} 轮 + 看门狗 0x001B + Error ACK/恢复 OP 通过")


if __name__ == "__main__":
    main()
