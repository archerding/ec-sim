# Spike 4：双主站冒烟 —— 未修改的 IgH 与 SOEM 都看到从站进入 OP

- **日期**：2026-11-12 19:56 班（B8 收尾；证据来自 19:26–19:53 各班的实现与实测）
- **结论**：✅ **成立（单从站、8 位数字 I/O 画像）**。
  - 未修改的 **SOEM**：`slaveinfo` 扫描到 1 个从站并映射 8 位 I/O；`simple_ng` 把它推进 **OP**，
    每轮过程数据 **WKC = 3**（1055 轮 0 次不符）。
  - 未修改的 **IgH 1.6.13**：`ethercat slaves` 显示 `OP`（由 ecrt 应用配置 domain 驱动），
    应用侧 `al_states=0x08`、domain `WKC = 3`；内核日志
    `Domain 0: Working counter changed to 3/3.`、`Slave states on main device: OP.`。
  - 全部结论**可复现**（脚本见 §4）；本页是"用例映射 / 预回归"证据，**不构成**一致性测试或
    CTT 认证结论。
- **与 Spike 3 的关系**：Spike 3 只到"扫描到 1 个从站（PREOP）"（B2b）；本 spike 覆盖 **B8**：
  双主站进入 OP + 过程数据交换 + 邮箱/SDO。

## 1. 方法

1. 在 Linux 验证机上运行本仓库的模拟器：`target/release/ec-sim-esc --tap escsim0 [--verbose]`
   （进程持有 TAP fd；由脚本设置 MAC/up）。
2. 模拟器提供：ESC 寄存器/AL 状态机、SII（字符串/通用/SM/TxPDO/RxPDO 类别、CRC-8）、
   SM/FMMU 寄存器与逻辑寻址、CoE 邮箱（SDO expedited upload + 配置对象 download）、
   最小对象字典（0x1018/0x1C00/0x1C12/0x1C13/0x1600/0x1A00）。
3. 两家主站均为**未修改**的构建，独立进程/模块（IgH 的 ecrt 应用夹具也只在验证机本地构建）。
4. 断言脚本（§4）逐项检查关键输出，任一不符即非 0 退出。

## 2. 证据：SOEM（未修改）

### 2.1 扫描 + 8 位 I/O 映射（`slaveinfo`）

```
1 slaves found and configured.
Output size: 8bits
Input size: 8bits
State: 4                       # SAFEOP
SM0 A:1000 L: 128 F:00010026 Type:1     # mailbox out
SM1 A:1080 L: 128 F:00010022 Type:2     # mailbox in
SM2 A:1100 L:   1 F:00010004 Type:3     # 8 位输出
SM3 A:1110 L:   1 F:00010000 Type:4     # 8 位输入
MBX length wr: 128 rd: 128 MBX protocols : 04   # CoE
```

### 2.2 过程数据进入 OP（`simple_ng`）

```
1 slaves found
Sequential mapping of I/O... mapped 1O+1I bytes from 1 segments
Waiting for all slaves in safe operational... done
Setting operational state... all slaves are now operational
Iteration    1:   117 usec  WKC 3  O: 00  I: 00  T: 0
...
```

- 1055 轮全部 `WKC 3`、0 次 `wrong`；应答器侧 `al=0x08`（OP）。
- WKC 3 = LRW 的写 FMMU +2、读 FMMU +1（IgH `master/datagram_pair.c` 与 SOEM
  `src/ec_main.c` 的期望一致）。

### 2.3 CoE PDO 映射（`slaveinfo -map`，经对象字典 SDO 读取）

```
PDO mapping according to CoE :
  SM2 outputs ... [0x0000.0] 0x7000:0x00 0x08
  SM3 inputs  ... [0x0001.0] 0x6000:0x00 0x08
```

### 2.4 邮箱数据链路

应答器统计可见 `mbox=24 mboxresp=22`（SDO 请求/响应计数），说明 SOEM 的 CoE 邮箱
（SM0/SM1 + SM1 Status bit3）数据链路真实可用。

## 3. 证据：IgH（未修改）

### 3.1 SDO 上传 / 下载（`ethercat` 工具，无需应用）

```
# 上传
ethercat upload -p 0 -t uint16 0x1c12 0x01   -> 0x1600 5632
ethercat upload -p 0 -t uint32 0x1600 0x01   -> 0x70000008 1879048200
ethercat upload -p 0 -t uint8  0x1c00 0x00   -> 0x04 4
# 下载（写配置对象）并回读
ethercat download -p 0 -t uint16 0x1c13 0x01 0x1a01   -> 成功（rc 0）
ethercat upload -p 0 -t uint16 0x1c13 0x01            -> 0x1a01 6657
# 只读 / 未知对象必须被拒绝
ethercat download -p 0 -t uint32 0x1018 0x01 0x1      -> Failed to download SDO: Input/output error
ethercat download -p 0 -t uint16 0x2000 0x00 0x1      -> Failed to download SDO: Input/output error
```

### 3.2 从站进入 OP（ecrt 应用驱动 domain）

验证机本地最小 ecrt 应用（**源码不入库**，见 §5）：`ecrt_request_master(0)` → 建 domain →
按本仓库对象字典配置 PDO（sync2 输出 `0x1600`/`0x7000:00` 8bit、sync3 输入
`0x1A00`/`0x6000:00` 8bit）→ activate → 2 ms 周期 LRW。

应用运行期间：

```
# ethercat slaves
0  0:0  OP  +  0x00000ec5:0x0000e5c1

# 应用输出
RESULT al_states=0x08 slaves_responding=1 wc=3 out=00 in=00

# IgH 内核日志
EtherCAT 0: Domain 0: Working counter changed to 3/3.
EtherCAT 0: Slave states on main device: OP.
```

注意：单独运行 `ethercat slaves`（没有应用）时 IgH 会把从站显示为 `PREOP` —— 这是主站工具
行为（无应用的 master 不配置 domain/状态）；OP 在应用激活期间维持。

## 4. 可复现脚本（仓库内，`scripts/spikes/`）

| 脚本 | 断言 |
|---|---|
| `minimal-esc-responder.sh` | SOEM 扫描到 1 从站 + IgH `ethercat slaves` 可见 + 单从站拓扑计算无 `Failed to calculate bus topology` |
| `soem-process-data.sh` | SOEM `simple_ng`：`mapped 1O+1I` → **OP**、每轮 `WKC 3`、0 次 `wrong` |
| `soem-mailbox-probe.sh` | SII 声明 128B/CoE 邮箱、`mbox>0`、`mboxresp>0`、8 位 I/O 映射不回归 |
| `sdo-upload-probe.sh` | SOEM CoE 映射 `0x7000/0x6000`；IgH SDO 上传 3 对象；IgH SDO 下载可回读、只读/未知被拒 |
| `igh-op-probe.sh` | 应用运行期间 `ethercat slaves`=**OP**、应用 `al_states=0x08` + `wc=3`、应答器 `al=0x08` |

使用（验证机上，root，TAP 名可覆盖）：

```
TAP=escsim0 MAC=02:ec:51:4d:00:01 bash scripts/spikes/minimal-esc-responder.sh
TAP=escsim0 bash scripts/spikes/soem-process-data.sh
TAP=escsim0 MAC=02:ec:51:4d:00:01 bash scripts/spikes/sdo-upload-probe.sh
TAP=escsim0 MAC=02:ec:51:4d:00:01 bash scripts/spikes/igh-op-probe.sh
```

## 5. IgH ecrt 夹具的 Provisioning（不入库）

IgH 的 ecrt 应用链接 GPL 的 `libethercat`，按仓库许可红线**不进入本仓库**；验证机本地构建：

```bash
gcc -O2 -I/opt/ecsim/igh/include -o /opt/ecsim/work/igh-op/ecrt-op-probe \
    /opt/ecsim/work/igh-op/ecrt-op-probe.c \
    /opt/ecsim/igh/lib/libethercat.a -lpthread -lrt
```

夹具要点（4 个 sync/2 个 PDO，与仓库对象字典一致）已写在 `igh-op-probe.sh` 头部注释与
WORKLOG 19:53 班条目中；`igh-op-probe.sh` 在二进制缺失时以 exit 2 提示重建。

## 6. 已知限制 / ⚠️ 待验证

- **单从站**：两条主站证据都只有 1 个从站；多从站链/分支的拓扑、WKC、FMMU 组合未验证。
- **过程数据内容静态**：模拟器无 PDI/应用层，输出只写内存、输入恒 0；无 PDO 数据回环逻辑。
- **DC / SM watchdog / OP 下的 AL 错误恢复**未实现/未验证；SII 缺 FMMU 类别与 DC 类别。
- **SDO 不完整**：无 SDO Info、Complete Access、分段传输；`ethercat sdos` / `slaveinfo -sdo` 会超时。
  Download 只对配置对象开放且值不持久（进程内）。
- **无 GUI 截图**：验证机为无图形界面的远端环境，本页以脱敏文本输出作为证据；
  若需要截图，可在图形环境重跑 §4 脚本抓取。
- **非认证**：以上为"用例映射 / 预回归"，**不构成**一致性测试或 CTT 认证结论。
