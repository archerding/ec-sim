# Spike 2：未修改的 SOEM 与 IgH 能否在 TAP 上工作

- **日期**：2026-11-12
- **假设**：SOEM（AF_PACKET 端口）与 IgH（`ec_generic` 驱动）都能绑定 L2 TAP 设备，并向"模拟器侧"发出真实 EtherCAT 帧。
- **结论**：✅ **两者都成立**。这是本项目**最大的技术风险项**，现已关闭。
- **证据脚本**：[tap_hold.py](../../scripts/spikes/tap_hold.py)（模拟器角色最小原型）、[sniff_ecat.py](../../scripts/spikes/sniff_ecat.py)

## 关键教训（先说这个）

**TAP 必须有用户态进程持有 fd**，否则为 `NO-CARRIER`，主站发往它的帧在内核里被丢弃。
第一次实验（无持有者）现象是：`ecx_init` 成功、但抓包 0 帧 —— 差点误判为"SOEM 不支持 TAP"。
"谁持有 TAP fd，谁就是从站" —— 这也正是模拟器进程的天然角色。

## A. SOEM（未修改源码，gitee 镜像构建）

```
SOEM (Simple Open EtherCAT Master)
Slaveinfo
Starting slaveinfo
ecx_init on tapsoem0 succeeded.        ← 绑定 TAP 成功
No slaves found!                        ← 符合预期（尚无从站应答）
End slaveinfo, close socket
```

模拟器侧（持有者）收到的帧：

```
HOLDER_FRAME #1 len=29 cmd=0x08 head=ffffffffffff01010101010188a40d100801000003010100
HOLDER_FRAME #4 len=30 cmd=0x08 head=ffffffffffff01010101010188a40e100802000020010200
HOLDER_FRAME #10 len=30 cmd=0x07 head=ffffffffffff01010101010188a40e100704000000000200
```

观察：SOEM 的源 MAC 固定为 `01:01:01:01:01:01`；帧命令为 BWR(0x08)/BRD(0x07)，与 SOEM 初始化流程（`BWR 0x0012`、`BWR 0x0120`、`BRD 0x0000` 数从站）一致 → **帧级行为与预期完全吻合**。

## B. IgH 1.6.13（`ec_generic`，未修改源码）

```
EtherCAT: Master driver 1.6.13
EtherCAT: 1 master waiting for devices.
ec_generic: Binding socket to interface 95 (tapigh0).
EtherCAT: Accepting 02:EC:51:4D:00:01 as main device for master 0.
EtherCAT 0: Starting EtherCAT-IDLE thread.
EtherCAT 0: Link state of ecm0 changed to UP.
EtherCAT WARNING 0: 496 datagrams TIMED OUT!      ← 预期：没有从站应答
```

`/dev/EtherCAT0` 出现，`ethercat master` 输出：

```
Master0
  Phase: Idle
  Ethernet devices:
    Main: 02:ec:51:4d:00:01 (attached)
      Link: UP
      Tx frames:   1004
      Tx rate [1/s]:    496
```

模拟器侧收到 IgH 的真实帧（CMD=0x07 BRD，源 MAC 为 TAP 的 MAC）：

```
HOLDER_FRAME #1 len=60 cmd=0x07 head=ffffffffffff02ec514d000188a40e100700000030010200
```

## 结论与影响

1. **网络级方案（Linux TAP）在 IgH 与 SOEM 上都成立**，且不需要改主站一行代码 —— "未修改的主站开箱即用"这个核心卖点有了硬证据。
2. IgH 侧待补的是**从站应答**（否则每帧 timeout）；这正是 M1 的第一目标：让主站看到 ≥1 个从站。
3. 运行注意：IgH 模块加载顺序（先 `ec_master main_devices=<MAC>` 再 `ec_generic`）、`make modules`、参数只在首次加载生效 —— 详见本地 `.local/ops.md`。

## 下一步

- 实现最小 ESC 应答器：响应 `BRD 0x0000`（Type/Rev 等）、`APWR 0x0010`（站地址）、`APRD 0x0130`（AL Status）→ 目标：IgH `ethercat slaves` 与 SOEM `slaveinfo` 都显示 **1 个从站**。
