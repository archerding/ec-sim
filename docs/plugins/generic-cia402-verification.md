# 通用 CiA 402 伺服画像（SIMULATED）— 验证记录

- 插件：`plugins/generic-cia402-servo/`（manifest + facts.md + README.md）
- 身份：自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5E0` / Rev `0x00010000`
  （**不是**任何厂商的 VID/PID；名称带 `SIMULATED`）
- 验证日期：2026-11-12（23:12 班）
- 验证环境：专用 Linux 验证机（openEuler 24.03 / kernel 6.6，TAP + 未修改 SOEM / IgH 1.6.13）
- 复现：`sudo bash scripts/spikes/generic-cia402-probe.sh`

## 结果（脚本 EXIT=0）

### SOEM `slaveinfo`

```
 Output size: 56bits
 Input size: 56bits
 Man: 00000ec5 ID: 0000e5e0 Rev: 00010000
 MBX length wr: 128 rd: 128 MBX protocols : 04
```

### SOEM `slaveinfo -map`（CoE 读取 PDO 映射；摘录）

```
  [0x0000.0] 0x6040:0x00 0x10 UNSIGNED16   Controlword
  [0x0007.0] 0x6041:0x00 0x10 UNSIGNED16   Statusword
  [0x000A.0] 0x6064:0x00 0x20 INTEGER32    Position actual value
```

（PDO 结构：RxPDO `0x1600` = `0x6040` 16bit + `0x6060` 8bit + `0x607A` 32bit = 56 bit；
TxPDO `0x1A00` = `0x6041` 16bit + `0x6061` 8bit + `0x6064` 32bit = 56 bit。）

### SOEM `simple_ng`（零补丁，进 OP）

```
Sequential mapping of I/O... mapped 7O+7I bytes from 1 segments
Waiting for all slaves in safe operational... done
Setting operational state... all slaves are now operational
Iteration    1:   131 usec  WKC 3  O: 00 00 00 00 00 00 00  I: 00 00 00 00 00 00 00  T: 0
```

### IgH 扫描

```
0  0:0  PREOP  +  0x00000ec5:0x0000e5e0
```

（IgH 无 ecrt 应用时只配置到 PREOP。）

### IgH **OP + 运行时行为**（ecrt 应用夹具，验证机本地、源码不入库）

脚本：`scripts/spikes/igh-cia402-op-probe.sh`（`--behavior cia402` 启用 CiA 402 行为）：

```
--- 应用运行中的 ethercat slaves ---
0  0:0  OP  +  0x00000ec5:0x0000e5e0
RESULT al_states=0x08 slaves_responding=1
STATUS sw=0x0637 modes_display=8 pos=1000
OK: IgH 零补丁把 CiA 402 画像推入 OP，且 TxPDO 状态字/模式显示/实际位置由行为驱动（0x0637）
```

即：未修改 IgH 的 ecrt 应用把从站推入 **OP**；应用按控制字序列 `0x0006→0x0007→0x000F`
写入 RxPDO，读回的 7 字节 TxPDO 中状态字达到 `Operation enabled(0x0004)` +
`Target reached(0x0400)`，模式显示为 8（CSP），实际位置跟随目标 1000。

## 结论

- 声明式 manifest 生成的 SII（身份/邮箱/SM/PDO）被未修改的 SOEM 正确识别：
  56/56 位映射、CoE 邮箱、RxPDO/TxPDO 对象名可见，`simple_ng` 零补丁进入 **OP**。
- IgH 正确显示身份与 PREOP。
- **本画像不包含 CiA 402 行为**：状态机、运动控制、编码器反馈、DC 均未实现（见
  `plugins/generic-cia402-servo/facts.md` 的"明确不模拟"清单）。本记录只证明
  **画像/SII/双主站扫描与 OP 闭环**，不构成任何一致性测试结论。

## ⚠️ 待验证 / 后续

- CiA 402 行为的运动环/模式切换（CSP/CSV/CST）、Ti=0/1 时序、错误码 `0x603F` 触发：
  未实现（Tier0 简化，见 `cia402-state-machine.md`）。
- 具体厂商兼容画像（Inovance SV660N 等）：在通用画像上按公开手册补默认 PDO/对象。
- SDO 上传本画像设备名（`0x1008`，分段上传）在 IgH 侧尚未列入本脚本；SOEM 侧已验证
  （分段协议与 EL2008 用例相同，见 `sdo-segmented-upload-probe.sh`）。

## 故障注入（`--fault <hex>`，2026-11-13 01:03 班）

```bash
cargo build --release -p esc-node
bash scripts/spikes/cia402-fault-probe.sh   # 合成主站（UDS）
```

验证机 **EXIT=0**：

```
OK: CiA 402 故障注入（Fault 0x0208 → 复位 0x0240，0x603F=0x7500）通过
```

- 启动即 Fault（状态字 bit3 置位），控制字 bit7（fault reset）后回到 SwitchOnDisabled
  （Fault 位清除）；`0x603F` 错误码可回读为注入值 `0x7500`。
- ⚠️ 简化：错误码保留到上层再次写入（与真机清除时序不同）；仅 Tier0 行为、无故障历史。