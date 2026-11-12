# 通用 CiA 402 CST 画像 — 验证记录（合成主站 + Tier0 行为）

> 本页只记录**本仓库自己的**验证（合成 UDS 主站 / 单元测试）。**不是**厂商一致性
> 或认证结论（R3）。

## 画像

- `plugins/generic-cia402-cst-servo/plugin.yaml`：5 字节 RxPDO/TxPDO
  （`0x6040`/`0x6060`/`0x6071` → `0x6041`/`0x6061`/`0x6077`），CoE SDO/SDO Info/CA，
  `dc.supported=false`。
- 行为：`--behavior cia402` 且 SM2/SM3 长度均为 5 时启用 CST Tier0。

## 验证命令

```bash
cargo build --release -p esc-node
bash scripts/spikes/cia402-cst-probe.sh
```

探针（`scripts/spikes/uds_master_probe.py --cia402-cst`）通过 UDS 线缆层发真实
EtherCAT 帧，按 `{0x0006,0x0007,0x000F,0x0000}` × 模式 10、目标转矩 300 驱动状态机，
断言读回的 5 字节 TxPDO：

| 步骤 | controlword | 预期 state/statusword | 预期实际转矩 |
| --- | --- | --- | --- |
| Shutdown | `0x0006` | Ready to switch on `0x0221` | 0 |
| Switch on | `0x0007` | Switched on `0x0223` | 0 |
| Enable operation | `0x000F` | Operation enabled `0x0237` | 300 |
| Disable voltage | `0x0000` | Switch on disabled `0x0240` | 0 |

## 结果（2026-11-13 02:08 班，Linux 验证机）

- `cia402-cst-probe.sh` → **EXIT=0**：`BEHAVIOR cia402 sm2_out=0x1100/5B sm3_in=0x1110/5B`；
  四步状态字/模式显示/实际转矩全部符合预期。
- 单元测试 `cia402_cst_mode_mirrors_target_torque` 覆盖同一序列（esc-node）。
- 本机 `cargo test --workspace` **199 passed**。

## ⚠️ 边界

- Tier0 简化：转矩只镜像、无转矩环/负载/限幅/单位换算；状态机时序（Ti）与故障码
  表未实现。真实伺服行为仍需硬件与厂商手册核对。
- 未在 IgH/SOEM 的真实 PDO 映射流程中验证该 5 字节布局（需要支持 CST 的主站应用）。
