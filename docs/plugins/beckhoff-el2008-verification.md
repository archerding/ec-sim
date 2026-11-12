# 验证记录：Beckhoff EL2008 兼容 8DO 插件（SIMULATED）

> 本记录是**预回归/扫描级验证**，不是一致性测试，也不构成任何认证（R3）。
> 环境相关路径与主机信息不入库（见本地 `.local/ops.md`）；下列命令为脱敏后的复现步骤。

## 对象

- 插件：`plugins/beckhoff-el2008/plugin.yaml`（声明式，自有 VID/PID `0x00000EC5`/`0x0000E5D2`，
  名称含 `SIMULATED`；EL2008 厂商 PID 仅作为兼容性事实记录在 `facts.md`，不用于身份）。
- 接口事实与来源：`plugins/beckhoff-el2008/facts.md`（R5 证据链，访问日期 2026-11-12）。
- 验证脚本：`scripts/spikes/beckhoff-el2008-probe.sh`。

## 环境

- 本仓库提交：`24dc7ca`（B9 完成）+ 本班插件变更（见 git log）。
- Linux 验证机：真实 TAP，root 权限；**未修改**的 SOEM（slaveinfo/simple_ng）与
  IgH 1.6.13（`ethercat` 工具）；IgH 模块加载方式见本地运维记录。
- 模拟器：`target/release/ec-sim-esc --tap escsim0 --manifest plugins/beckhoff-el2008/plugin.yaml`。

## 复现

```bash
# 在 Linux 验证机上、仓库根目录
cargo build --release -p esc-node
sudo bash scripts/spikes/beckhoff-el2008-probe.sh
```

## 结果摘要（关键输出）

SOEM `slaveinfo`（身份与进程映像）：

```
1 slaves found and configured.
Man: 00000ec5 ID: 0000e5d2 Rev: 00010000
Output size: 8bits
Input size: 0bits
```

SOEM `slaveinfo -map`（CoE PDO 映射）：

```
  SM2 outputs
     addr b   index: sub bitl data_type    name
  [0x0000.0] 0x7000:0x01 0x01 BOOLEAN      Output 1
  ...
  [0x0000.7] 0x7070:0x01 0x01 BOOLEAN      Output 8
```

SOEM `simple_ng`（过程数据/OP）：

```
Sequential mapping of I/O... mapped 1O+0I bytes from 1 segments
Waiting for all slaves in safe operational... done
Setting operational state... all slaves are now operational
Iteration    1:   111 usec  WKC 2 wrong (expected 3)
```

SOEM `slaveinfo -sdo`（对象字典，2026-11-12 21:45 班起含设备名，经**分段 SDO 上传**）：

```
CoE Object Description found, 20 entries.
0x1008      "Device name"                                 [VAR]
    0x00      "Device name"                                 [VISIBLE_STR(336) R_R_R_]
                "Beckhoff EL2008 compatible 8DO (SIMULATED)"
0x1c12      "RxPDO assign"     [ARRAY  maxsub(0x08 / 8)]
    0x01      "RxPDO assign"    [UNSIGNED16  RW____]  0x1600 / 5632
0x1600      "RxPDO mapping"    [ARRAY  maxsub(0x01 / 1)]
0x7000      "Output 1"         [ARRAY  maxsub(0x01 / 1)]
0x7070      "Output 8"         [ARRAY  maxsub(0x01 / 1)]
```

> 设备名对象 `0x1008:00`（VISIBLE_STRING，42 字节 > 4）必须通过 **segmented upload**
> 才能读出；SOEM 打印出完整字符串，说明分段上传实现与真实主站兼容。
> 复现：`sudo bash scripts/spikes/sdo-segmented-upload-probe.sh`
> （⚠️ IgH `ethercat upload` CLI 在无 ecrt 应用配置时无法确定 SDO 类型，
> 输出 `Failed to determine SDO entry data type`；IgH 侧分段上传 ⚠️ 待验证。）

IgH `ethercat slaves`（扫描/拓扑）：

```
0  0:0  PREOP  +  0x00000ec5:0x0000e5d2
```

IgH **OP**（outputs-only ecrt 夹具，源码在验证机本地、不入库；脚本
`scripts/spikes/igh-el2008-op-probe.sh`）：

```
0  0:0  OP  +  0x00000ec5:0x0000e5d2
RESULT al_states=0x08 slaves_responding=1 wc=1 out=00
OK: 未修改 IgH 经 outputs-only ecrt 应用把 EL2008 兼容 8DO 画像推入 OP
```

脚本最终输出：

```
OK: EL2008 兼容 8DO 画像端到端通过（SOEM：身份 00000ec5:0000e5d2 / 8 位输出 0 位输入 /
SM2 PDO 0x1600..0x1607 / OP WKC=2 / SDO Info 含 0x7000..0x7070；IgH：拓扑与身份）
```

默认内置画像回归（`minimal-esc-responder.sh` / `soem-process-data.sh` / `igh-op-probe.sh`，
默认与 strict 两种模式）本班重跑 **6/6 EXIT=0**，插件改动不影响既有画像。

## 已知限制 / ⚠️ 待核实

1. **SOEM `simple_ng` 的 expected WKC 比实际多 1**：SOEM 2.0 对"无输入且带邮箱"的从站会
   额外把 SM1 状态寄存器（`ECT_REG_SM1STAT` = 0x080D）映射成进程数据 FMMU，并
   `src/ec_config.c: if (!Ibytes) inputsWKC++` 把期望 WKC 记 +1。本模拟器只支持
   **DL 用户内存（0x1000–0xFFFF）** 的 FMMU 物理映射，未模拟寄存器空间映射；因此
   simple_ng 打印 `WKC 2 wrong (expected 3)`。**OP 与过程数据本身正常**。
   ⚠️ 待核实：ESC Sec.II / ETG.1000.4 是否允许 FMMU 物理映射寄存器空间（0x080D），
   以及真实从站在该场景下的 WKC；核实后再决定是否实现寄存器空间 FMMU（并复核内置画像
   的 expected WKC 是否随之变化）。
2. **SOEM `slaveinfo -sdo` 工具缺陷**：它对非 VAR 对象的 subindex 0 用 2 字节缓冲区读取
   （`slaveinfo.c` 把 UINT8 响应的 1 字节拷进 `uint16`，高字节未初始化），对含多个数组
   对象的字典会间歇或必然段错误；脚本对 -sdo 做 best-effort（最多 3 次），失败时打印
   说明并继续（SDO 路径已由 `slaveinfo -map` 的 CoE 读取覆盖：0x1C00/0x1C12/0x1600）。
   本画像已为数组对象补齐子索引 0（UINT8 条目数，CiA 301），以匹配真实设备的字典形状。
3. 电气特性（0.5 A 驱动、短路保护、开关时间）与 DC 均未模拟；见 `README.md` 的
   「已模拟/未模拟」清单。

## 结论

- 未修改的 SOEM 与 IgH 都能扫描到该声明式插件画像（身份/SII/PDO 映射与公开资料一致）；
- SOEM 可将其推入 OP 并交换 1 字节输出过程数据（WKC=2，差异原因见限制 1）；
- **不**声称该设备"通过一致性测试/认证"——这里只做用例映射与预回归（R3）。
