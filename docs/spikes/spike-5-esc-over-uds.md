# Spike 5：ESC-over-UDS 线缆层（B3 子项 1）

- **日期**：2026-11-12 19:59 班（低峰）
- **结论**：✅ **模拟器侧成立**：`ec-sim-esc --uds <path>` 在 Linux 上以 `SOCK_DGRAM` Unix domain
  socket 承载 EtherCAT 帧（每个报文 = 一帧），复用**同一** `esc_node::Responder` 内核；
  合成主站 probe 已断言 BRD/APRD/APWR/FPRD、DL Status `0x5610`、一帧多 datagram（M 位）WKC。
- **定位**：这是 AGENTS §1.3「线缆层可插拔：Linux TAP（旗舰）与 ESC-over-UDS shim（全平台/CI）
  共用同一个 ESC 内核」的 **UDS 半边**；SOEM `oshw` 的 UDS shim（B3 主体）下一步接入本 socket。
- **非认证**：预回归证据，不构成一致性测试/认证结论。

## 1. 协议与实现

- 线缆：`AF_UNIX` + `SOCK_DGRAM`，报文边界 = 以太网帧边界（无需自定长度前缀）。
- 服务端：`ec-sim-esc --uds <path>`（仅 Linux）——
  - 启动时清理陈旧 socket 文件并 `UnixDatagram::bind(path)`；
  - `recv_from` 收到帧 → `Responder::handle_frame(frame, DEFAULT_SIM_MAC)` → 若产生应答，
    `send_to(peer_path)` 回给发送方；
  - `--verbose` 下每 50 帧打印与 TAP 相同的统计行（含 `mbox/mboxresp`），并打印 `RX/DROP` 明细。
- 客户端：必须**绑定到文件系统路径**（abstract/匿名 socket 无法被 `send_to`，服务端会跳过）；
  每帧内容就是完整的以太网帧（14B Ethernet 头起，EtherType `0x88A4`）。
- 服务端对 MAC 不敏感（应答源 MAC 用占位值）；MAC/链路语义只对 TAP 路径有意义。

## 2. 复现

```bash
cargo build --release -p esc-node
SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/uds-loopback.sh
# OK: ESC-over-UDS 线缆层可用（/tmp/ecsim-uds.sock）：BRD/APRD/APWR/FPRD/M 位均通过
```

- `scripts/spikes/uds-loopback.sh`：启动 `ec-sim-esc --uds`、等 socket 就绪、调用 probe、清理。
- `scripts/spikes/uds_master_probe.py`：零依赖（标准库）合成主站，自行构造 EtherCAT 帧并断言：
  1. `BRD 0x0000` → WKC 1、ESC 类型 `0x01`；
  2. `APRD 0x0130` → WKC 1、AL 状态 INIT；
  3. `APWR 0x0010 = 0x1001` → WKC 1，随后 `FPRD 0x1001 0x0130` 命中（WKC 1）；
  4. `APRD 0x0110` → DL Status `0x5610`（单从站拓扑值）；
  5. 一帧两个 datagram（M 位）→ 两条应答各自 WKC +1。
- 说明：datagram 头按 ETG.1000.4 为 **10 字节**（cmd/idx/ADP/ADO/len-flags/**IRQ**），
  probe 首版漏了 IRQ 字段导致帧被拒，已修复并保留为回归点。

## 3. 验证结果（Linux 验证机）

```
$ SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/uds-loopback.sh
OK: ESC-over-UDS 线缆层可用（/tmp/ecsim-uds.sock）：BRD/APRD/APWR/FPRD/M 位均通过
SCRIPT_RC=0
```

- 模拟器侧日志（`--verbose`）可见 `READY uds=...`、`RX uds N bytes from "<client path>" (pathname)`，
  统计行与 TAP 路径一致（`frames_in/out/hits` 增长）。
- TAP 路径无回归：同一构建下 `minimal-esc-responder.sh`（SOEM/IgH 扫描）与
  `igh-op-probe.sh`（IgH OP）重跑通过；`cargo fmt/clippy/test` **100 passed**。

## 4. SOEM oshw UDS shim 联调（B3 主体，2026-11-12 20:03 班完成）

在验证机本地把 SOEM 2.0.0 复制到 `/opt/ecsim/work/soem-uds`（**GPL 夹具，源码/补丁不入库**），
只替换 `oshw/linux/nicdrv.c` 中 `ecx_setupnic()` 的套接字初始化部分：

- 原实现：`socket(PF_PACKET, SOCK_RAW, ETH_P_ECAT)` + ioctl 配置网卡 + `bind` 到接口；
- 替换为：`socket(AF_UNIX, SOCK_DGRAM, 0)`，客户端 bind 自己的路径
  （`/tmp/soem-uds-client-<pid>.sock`）并 `connect(ifname)` —— 此时 `ifname` 参数就是
  `ec-sim-esc --uds` 的 socket 路径；
- `send`/`recv`/`poll`/索引缓冲区逻辑保持 SOEM 原样（UDS datagram 同样保留帧边界）。

```bash
# 验证机本地（一次性）
cp -a /opt/ecsim/fixtures/soem /opt/ecsim/work/soem-uds
python3 patch_soem_uds.py /opt/ecsim/work/soem-uds/oshw/linux/nicdrv.c   # 补丁脚本在验证机上
cd /opt/ecsim/work/soem-uds
cmake -S . -B build-uds -DCMAKE_BUILD_TYPE=Release && cmake --build build-uds -j4

# 回归（本仓库脚本）
cargo build --release -p esc-node
SOCK=/tmp/ecsim-uds.sock bash scripts/spikes/soem-uds-probe.sh
# OK: SOEM UDS shim 经 /tmp/ecsim-uds.sock 跑通 ec_config_init + ec_config_map
#     （1 从站 / 1O+1I / OP / 1358 轮 WKC=3 / 0 次 wrong）
```

实测（未修改 SOEM 的协议逻辑，仅换收发包层；**无需 root/TAP**）：

- `slaveinfo /tmp/ecsim-uds.sock`：`1 slaves found and configured`、
  `Output size: 8bits`、`Input size: 8bits`、`State: 4`、SM/FMMU/MBX 行齐全；
- `simple_ng /tmp/ecsim-uds.sock`：`mapped 1O+1I bytes` → **all slaves are now operational**，
  每轮 `WKC 3`、0 次 `wrong`；
- 模拟器 `--verbose` 日志：`READY uds=...`、`RX uds N bytes from "<client path>"`；
- 结论：SOEM 的 `ec_config_init` + `ec_config_map` 与过程数据在 UDS 线缆层上全部跑通。

仍属于**独立进程夹具**（SOEM 为 GPLv3，不进仓库、不链接核心）；本仓库只保留调用脚本与文档。

## 5. 已知限制

- 仅 Linux（`std::os::unix`）；Windows/macOS 编译通过但运行时报错退出。
- 服务端按 `peer.as_pathname()` 回包：客户端必须 bind 文件系统路径（abstract socket 不支持）。
- 单进程内串行处理（无并发/多客户端隔离保证）；无 MAC 过滤、无链路状态语义、无 FCS。
- SOEM UDS shim 只在验证机本地（补丁/构建产物不入库）；`soem-uds-probe.sh` 缺二进制时 exit 2。
- 单从站、无 DC/watchdog；过程数据内容静态（无 PDI 回环）。
- ⚠️ 待验证：多客户端并发、非 root CI 环境下的完整链路（TAP 对照已由 B8 脚本覆盖）。
