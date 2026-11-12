# Generic CiA 402 CSV servo（SIMULATED）— 验证记录

- 插件：`plugins/generic-cia402-csv-servo/`（11 字节 RxPDO/TxPDO，速度模式）
- 身份：自有 VID `0x00000EC5` / PID `0x0000E5E1`（**不使用**任何厂商 VID/PID；名称带 `SIMULATED`）
- 验证日期：2026-11-13（00:11 班）
- 验证方式：合成主站（`scripts/spikes/uds_master_probe.py --cia402-csv`，UDS，无需 root/内核模块）

## 结果（`bash scripts/spikes/cia402-csv-probe.sh` → EXIT=0）

```
BEHAVIOR cia402 sm2_out=0x1100/11B sm3_in=0x1110/11B
OK: CiA 402 CSV：速度模式位置累加/速度显示通过（11 字节 PDO）
OK: ESC-over-UDS 线缆层可用：BRD/APRD/APWR/FPRD/M 位 + SM watchdog（10 ms → SAFEOP+ERROR
    0x001B → ACK 清错）+ ESM 前置条件（0x0015/0x0017）均通过
OK: CiA 402 CSV（11 字节 PDO）行为在合成主站下通过
```

断言序列（每个报文触发一次行为 tick；写帧用上一控制字、读帧用新控制字）：

| 控制字 | 模式 | 读回状态字 | 读回位置 | 读回速度 |
| --- | --- | --- | --- | --- |
| `0x0006` | 9 | `0x0221` | 0 | 0 |
| `0x0007` | 9 | `0x0223` | 0 | 0 |
| `0x000F` | 9 | `0x0237` | 5 | 5 |
| `0x000F` | 9 | `0x0237` | 15 | 5 |
| `0x0000` | 9 | `0x0240` | 20 | 0 |

## 结论

- 11 字节 RxPDO/TxPDO 的解析/写回、CSP 与 CSV 模式门控、速度模式位置累加与速度显示
  均由行为正确驱动；UDS 原套件（含 SM watchdog 与 ESM 前置条件）继续通过。
- 单测覆盖：`cia402_csv_mode_accumulates_velocity`（直接 datagram 序列）。

## IgH 侧 OP + 运行时行为（ecrt 应用夹具，验证机本地、源码不入库）

脚本：`scripts/spikes/igh-cia402-csv-op-probe.sh`（应用按 `0x0006→0x0007→0x000F`、
mode=9、target velocity=5 写 11 字节 RxPDO）：

```
0  0:0  OP  +  0x00000ec5:0x0000e5e1
RESULT al_states=0x08 slaves_responding=1
STATUS sw=0x0237 modes_display=9 pos=2995 vel_actual=5
OK: IgH 零补丁把 CSV 画像推入 OP，且 11 字节 TxPDO（状态字/模式/位置/速度）由行为驱动（0x0237, pos=2995）
```

即未修改 IgH 的 ecrt 应用把 CSV 画像推入 **OP**，读回的 TxPDO 中状态字含
Operation enabled、模式显示 9、位置按速度累加（Tier0 每周期 +5）、实际速度 = 5。

## ⚠️ 限制（详见 facts.md）

- CSV 仅 Tier0 简化：位置按"每 tick 累加目标速度"模拟，**不是**真实单位/轨迹/加减速/限幅。
- 无模式切换时序（Ti=0/1）、编码器/限位/Homing、DC、厂商专有对象。
- 夹具源码在验证机本地（不入库，符合 GPL 边界）；脚本依赖该本地夹具存在。
- 本记录只证明模拟器行为与协议路径，不构成一致性测试结论（R3）。
