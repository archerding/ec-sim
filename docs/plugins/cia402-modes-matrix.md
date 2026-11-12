# CiA 402 Tier0 模式矩阵（EC-Sim）

> 本页描述 **EC-Sim 模拟器**的 CiA 402 Tier0 行为支持矩阵，**不是**厂商设备能力表。
> 所有模式都是简化实现（见每行"边界"），不能据此判断真实伺服行为，也不构成一致性/认证结论。

## 支持矩阵

| 模式 | 名称 | PDO 布局 | 行为（Tier0） | 验证脚本 | 边界 |
| --- | --- | --- | --- | --- | --- |
| 1 | PP 轮廓位置 | 7 字节 | bit4（new set-point）时位置跳到目标，置 Target reached + Set-point acknowledge | `cia402-pp-probe.sh` | 瞬时到位，无轨迹/加减速/限幅 |
| 2 | VL 速度 | 11 字节 | 位置每周期累加目标速度，速度显示目标速度；禁用归零 | `cia402-pv-probe.sh`（含 VL） | 无加减速/单位换算 |
| 3 | PV 轮廓速度 | 11 字节 | 同 VL | `cia402-pv-probe.sh` | 同 VL |
| 4 | TQ 转矩 | 5 字节 | 实际转矩镜像目标转矩；禁用归零 | `cia402-tq-probe.sh` | 无转矩环/限幅/单位换算 |
| 6 | HM 回零 | 7 字节 | enabled 且 bit4=1 时位置归零并置 Homing attained；清 bit4 后不再置位 | `cia402-homing-probe.sh` | 无寻零过程/开关信号/0x6098 |
| 7 | IP 插补位置 | 7 字节 | 复用 CSP 位置跟随语义 | `cia402-ip-probe.sh` | 无插补缓冲/时间片 |
| 8 | CSP 循环同步位置 | 7 字节 | enabled 时位置跟随目标，置 Target reached | `cia402-behavior-probe.sh` | 非真实轨迹/动态 |
| 9 | CSV 循环同步速度 | 11 字节 | 位置每周期累加目标速度，速度显示目标速度；禁用归零 | `cia402-csv-probe.sh` | 无加减速/单位换算 |
| 10 | CST 循环同步转矩 | 5 字节 | 实际转矩镜像目标转矩；禁用归零 | `cia402-cst-probe.sh` | 无转矩环/限幅/单位换算 |

状态机骨架：`0x6040` 控制字 → `0x6041` 状态字的 CiA 402 状态转换（INIT 由 ESC 状态机负责；
本矩阵覆盖 Operation enabled 之后的模式相关行为）；故障注入 `--fault 0xNNNN` 与复位见
`cia402-fault-probe.sh`。

## `0x6502`（Supported drive modes）与布局

| 画像 | PDO 字节数 | 声明值 | 位含义 |
| --- | --- | --- | --- |
| `generic-cia402-servo` | 7 | `0x01C1` | pp(1)/hm(6)/ip(7)/csp(8) |
| `generic-cia402-csv-servo` | 11 | `0x03CD` | pp(1)/vl(2)/pv(3)/hm(6)/ip(7)/csp(8)/csv(9) |
| `generic-cia402-cst-servo` | 5 | `0x0410` | tq(4)/cst(10) |
| `inovance-sv660n`（兼容画像） | 7（Tier0 通用） | `0x03AD` | 公开实测厂商默认支持模式（不随布局变化） |

守卫测试：`crates/plugin-host/tests/plugin_manifests.rs`；端到端 SDO 验证：
`scripts/spikes/cia402-modes-probe.sh`。

## 相关文档

- 状态机与错误注入细节：[docs/plugins/cia402-state-machine.md](cia402-state-machine.md)
- 各画像验证记录：`docs/plugins/generic-cia402-*-verification.md`
- 验证脚本索引：[docs/VERIFICATION.md](../VERIFICATION.md)
