# Generic CiA 402 CSV servo（SIMULATED）— 11 字节 PDO / 速度模式

B11 的 CSV（Cyclic Synchronous Velocity）画像：在通用 CiA 402 基础上扩展 11 字节
RxPDO/TxPDO，验证速度模式下的运行行为。

- 身份：自有 Vendor ID `0x00000EC5` / Product Code `0x0000E5E1`（R2），名称带 `SIMULATED`。
- SII/PDO：
  - RxPDO `0x1600`（11 字节）：`0x6040` controlword(16) + `0x6060` modes(8) +
    `0x607A` target position(32) + `0x60FF` target velocity(32)；
  - TxPDO `0x1A00`（11 字节）：`0x6041` statusword(16) + `0x6061` modes display(8) +
    `0x6064` position actual(32) + `0x606C` velocity actual(32)。
- 行为（`--behavior cia402`，Tier0 简化）：
  - CSP（模式 8）：位置跟随目标位置并置 Target reached（同通用画像）；
  - **CSV（模式 9）**：Operation enabled 时位置每个周期**累加目标速度**，`0x606C`
    镜像目标速度；禁用后速度显示归零、位置保持。
  - ⚠️ 模拟器把"速度"简化为**每 tick 的增量**（无真实单位/轨迹/加减速/限幅），
    不是真实伺服运动模型。
- 验证：`scripts/spikes/cia402-csv-probe.sh`（合成主站，UDS，无需 root），断言
  11 字节 PDO 的状态字/模式显示/位置累加/速度显示。

事实来源：CiA 402 公开 profile（对象 0x6040/0x6041/0x6060/0x6061/0x6064/0x606C/0x607A/
0x60FF）；R1 未入库任何厂商 ESI/PDF/固件。⚠️ 本画像不声称与任何厂商设备一致或认证（R3）。
