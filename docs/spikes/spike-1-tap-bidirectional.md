# Spike 1：TAP 能否双向承载 EtherType 0x88A4 帧

- **日期**：2026-11-12
- **假设**：用户态进程持有 L2 TAP 设备（`IFF_TAP|IFF_NO_PI`）时，可以与主站侧的 AF_PACKET 原始套接字双向交换 EtherCAT 帧。
- **结论**：✅ **成立**。两个方向逐字节一致。
- **脚本**：[scripts/spikes/tap_ethernet_loopback.py](../../scripts/spikes/tap_ethernet_loopback.py)
- **环境**：Linux / kernel 6.x / root（见 [docs/LINUX-SERVER.md](../LINUX-SERVER.md)）

## 方法

1. `open("/dev/net/tun")` + `ioctl(TUNSETIFF, IFF_TAP|IFF_NO_PI)` 建 TAP；设 MAC、关闭 IPv6、`IFF_UP`。
2. 同一 netdev 上开 `AF_PACKET/SOCK_RAW`（ETH_P_ALL）并 bind，模拟"主站"。
3. 方向 A（模拟器→主站）：`write(tap_fd)` 写入自定义 0x88A4 帧，主站 socket 应收到同样的帧。
4. 方向 B（主站→模拟器）：主站 `send()` 写入 0x88A4 帧，模拟器 `read(tap_fd)` 应收到。

## 结果（原始输出）

```
[1] created TAP: tapspike0 (fd=3)
[2] flags=0x1043 up=True running=True
[3] AF_PACKET bound to tapspike0 (ifindex=88)
[4] 模拟器->主站: 收到 26 字节, ethertype=0x88A4, 内容一致=True
[5] 主站->模拟器: 收到 26 字节, 内容一致=True, 跳过非 0x88A4 帧=0
[6] RESULT: PASS (downlink=True, uplink=True)
```

## 结论与影响

- TAP 是 **L2**，原生透传任意 EtherType，`0x88A4` 不需要内核协议处理器 → **网络级方案成立**。
- 工程注意点（写进实现）：
  1. 必须 `IFF_NO_PI`，否则每帧前面会多 4 字节。
  2. TAP 一旦 UP，内核会自己发 IPv6 MLD/RS 等帧 → **模拟器读侧必须按 EtherType 过滤**（第一次跑就是因为读到 MLD 帧误判失败）。
  3. TAP 设备在无进程持有 fd 时是 `NO-CARRIER`，发往它的帧会被丢弃（见 Spike 2 教训）。
