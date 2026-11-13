# M3 范围与退出条件（草案，2026-11-13）

> 目的：在 M2 观测面【已完成】之上界定 **M3「插件宿主 + 写路径 + 可运维性」**的范围、工作流与
> 退出条件，防止控制面继续无约束扩张。**不是承诺**；未验证项标 ⚠️。
> 关联：[ADR 0002](../adr/0002-m3-plugin-host-and-write-paths.md)、[M2 退出清单](M2-exit-checklist.md)、
> [STATUS](../STATUS.md)、[CONTROL-PLANE](../CONTROL-PLANE.md)。

## 1. 目标（M3）

1. **行为插件宿主**：WASM 沙箱 + PDO arena，插件不进入实时环（ADR 0002 D1/D2）。
2. **SDO 写路径**：显式开关 + manifest 白名单 + 审计；先只读浏览，再受控写入（D3）。
3. **可运维性**：可选静态 token、审计日志接口、`/metrics` 扩展写计数（D3/D4）。
4. **M3 回归**：`run-all-probes.sh` 覆盖新增探针；文档/代码一致性检查继续为 CI 门禁。

## 2. 非目标（M3 不做）

- 一致性测试/CTT 认证宣称；硬实时保证；多租户/RBAC 完整方案；进程内 TLS。
- 厂商 ESI/PDF/固件入库；GPL/LGPL 代码进核心。
- 32 从站 @1 kHz 的样例级达成（仅做调研与瓶颈记录，见 M1 清单）。

## 3. 工作流与验收

| # | 工作流 | 关键交付 | 验收（可独立验证） | 状态 |
| --- | --- | --- | --- | --- |
| W1 | WASM 插件宿主 spike | wasmi/wasmtime 选型对比（启动/体积/许可/MSRV）；最小 `read_pdo/write_pdo/log` 宿主 | spike 文档 + 可运行示例；不改变现有实时环 | ✅ 已完成（B43：选型 **wasmi 0.31.2**（MIT/Apache-2.0，兼容 workspace MSRV 1.75）；`plugin-host` 默认关闭的 `wasm-runtime` feature + WAT 夹具 + `wasm_plugin_demo` + `wasm-plugin-probe.sh`；7 个 feature 单测；release 实例化 605µs / tick 179.5ns）；[spike-6](../spikes/spike-6-wasm-host.md) |
| W2 | PDO arena 同步 | 双缓冲/generation 或 seqlock；插件崩溃隔离 | 单测 + 压测（无撕裂、无实时环阻塞） | ✅ 已完成（B44：子项 1 `plugin-host::arena` 双缓冲/seqlock + generation（零撕裂/零分配）；子项 2 `wasm_worker` 独占线程 + fuel/超时/内存上限；子项 3 `plugin-host::behavior` + `esc_node::behavior::BehaviorBridge` 已接 `esc-node` UDS/TAP 帧循环，manifest `behavior:` 已支持；子项 4 仓库内真实画像 `plugins/behavior-counter/` + `wasm-behavior-probe.sh` 端到端）；增量项：panic 重启/多实例配额由 B51 完成、**多实例接线由 B54 完成**（`BehaviorBridges` 每站一实例 + 共享 `BehaviorQuota` + `--behavior-max-instances`）；Linux CLI 端到端仍待 crates.io |
| W3 | SDO 只读浏览 | `GET /sdo?index&sub` 从运行时字典读值（不 mailbox） | 探针：已知对象值/名称/类型；未知 404 | ✅ 已完成（B39：`sdo-read-probe.sh`；sdo-v1 schema） |
| W4 | SDO 受控写 | `POST /sdo`（`--serve-write` + manifest `writable` 白名单 + 审计） | 探针：默认拒绝、白名单成功、非白名单 403、审计记录 | ✅ 已完成（B40：`sdo-write-probe.sh`；`/sdo-audit`、`ecsim_sdo_writes_total`） |
| W5 | 可选 token | `--serve-token-env VAR`（`Authorization: Bearer`）；空 = 现状 | 探针：带/不带 token 的 200/401 行为 | ✅ 已完成（B41：`serve-token-probe.sh`；401/403/200、静态资源豁免、写路径同保护） |
| W6 | Web SDO 面板 | 只读浏览；写入需二次确认且仅当写开关启用 | `node --check` + 人工验收（⚠️ 无 DOM 测试） | ✅ 实现完成（B42：`web-sdo-panel-check.py` 静态资源回归 + `serve-web-probe.sh` 同源资源断言；`GET /sdo` 浏览 + `--serve-write` 勾选/confirm 二次确认写入）；⚠️ 浏览器人工点击验收待补 |
| W7 | 32 从站 @1 kHz 调研 | 瓶颈记录与可行路径（SOEM 线程/内核旁路） | 文档结论；不承诺达成 | ✅ 已完成（B45：[M3-W7 瓶颈调研](M3-W7-32slave-1khz-bottleneck.md)；修正 `simple_ng` 口径——32 站链处理 1.7 µs、TAP 往返 167 µs，未测 RT@p99 → 建议 B46） |
| W8 | MCP/Agent 桥接 stub | 工具清单与只读端点映射（不含写） | 设计文档；无外部依赖 | ✅ 已完成（B45 设计/目录/静态校验 + B47 实现：`crates/mcp-server` 的 `ec-sim-mcp`（JSON-RPC 2.0 stdio、9 个只读工具、零第三方依赖、只发 GET）+ `mcp-stdio-probe.sh` 端到端 18 项断言（本机/验证机 EXIT=0）；见 [M3-W8 文档](M3-W8-mcp-agent-bridge-stub.md)） |

## 4. 风险与依赖

- **wasm 运行时许可/体积**：需逐项核对（wasmi MIT；wasmtime Apache-2.0 WITH LLVM-exception）并评估构建时间。
- **写路径安全**：任何写操作都需要文档化的威胁模型；默认关闭，CI 中不启用。
- **性能**：arena 同步与插件调用不得进入实时环关键路径；用现有 `chain_latency`/`uds_latency` 守门。
- **CI**：Linux clippy 需在 CI 覆盖（验证机未装 clippy），新增探针加入 `run-all-probes.sh`。

## 5. M3 退出条件（核对结果）

> 逐条核对见 **[M3 退出清单](M3-exit-checklist.md)**（2026-11-13，HEAD `d7089da`）。

| # | 条件 | 状态 |
| --- | --- | --- |
| 1 | W1–W5 完成且各有探针/单测；W6 人工验收通过；W7/W8 至少产出文档 | 🟡 部分（W6 真人点击待补；其余达成） |
| 2 | 默认配置保持**只读**（写/token/插件均需显式开启） | ✅ |
| 3 | `run-all-probes.sh` 在 Linux root 下全绿（含新增探针）；`control-plane-doc-check.py` 覆盖新路由 | ✅（20 PASS / 0 FAIL / 0 SKIP） |
| 4 | 无 GPL/LGPL 代码进核心；无未声明依赖；许可与 NOTICE 复核 | ✅（默认 0 第三方；可选 25 个宽松许可） |
| 5 | 文档：ADR 0002 更新为 Accepted、CONTROL-PLANE/STATUS/M3 清单互链 | ✅ |

## 6. 下一步（按优先级）

1. ✅ W1 spike 已完成（B43，wasmi 0.31.2 + 最小宿主，见 [spike-6](../spikes/spike-6-wasm-host.md)）。
2. ✅ W2 arena 同步（W1 依赖）**已完成**（B44）：双缓冲/seqlock + generation（`plugin-host::arena`，`crates/plugin-host/tests/arena_no_alloc.rs` 零分配守卫 + 2 万次发布并发压测零撕裂）；子项 2 **插件独占 worker 线程 + fuel/超时 + 内存上限**（`plugin-host::wasm_worker`：`ArenaSnapshot` 投递 + `recv_timeout` 有界等待 + `StoreLimits`；死循环由 fuel 终止、宿主永不 join 卡住的插件）；子项 3 **实例生命周期接 `esc-node` 非实时线程**（`plugin-host::behavior` + `esc_node::behavior::BehaviorBridge` + manifest `behavior:`；UDS/TAP 帧循环只 publish/poll）；子项 4 **仓库内真实行为画像 + 端到端探针**（`plugins/behavior-counter/`：自写 WAT + manifest `behavior:`，`crates/esc-node/tests/behavior_counter_plugin.rs` 经合成主站帧验证 SM3 = SM2 + 1，探针 `wasm-behavior-probe.sh`；`wasm-runtime` 下 plugin-host 53 / esc-node 73 单测 + 6 集成测试）。增量项（不在 W2 原范围）：插件 panic 重启与多实例配额、Linux CLI UDS/TAP 端到端（需 crates.io）。
3. ✅ W7 32 从站调研已完成（B45：[M3-W7 瓶颈调研](M3-W7-32slave-1khz-bottleneck.md)）。
4. ✅ W8 MCP/Agent 桥接已完成（B45 设计/目录/静态校验；B47 实现 `crates/mcp-server` + `mcp-stdio-probe.sh`）；
   **B48 M3 退出核对已完成（2026-11-13）**：验证机整轮 **20 PASS**、本机 256 单测、许可核对，
   ADR 0002 → Accepted；见 [M3 退出清单](M3-exit-checklist.md)（M3 = 退出候选，**W6 真人点击验收待补**）。
   下一步：B44 剩余增量（插件 panic 重启/多实例配额）与 M4（测试报告 + CI 接入 + Agent Skills）。
