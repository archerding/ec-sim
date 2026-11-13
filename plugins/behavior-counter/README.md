# behavior-counter — 行为插件参考画像（SIMULATED）

> 这是 **EC-Sim 自写示例**，不是厂商兼容设备：身份用我方自有 VID/PID，名称带
> `SIMULATED`，不含任何厂商 ESI/PDF/固件/代码（`docs/DEVICE-PLUGINS.md` R1–R5）。
> 用途只有一个：验证「声明式 manifest `behavior:` → WASM worker 线程 → SM3」链路。

## 语义（Tier0 示例，不代表任何真实设备行为）

| 方向 | SM | 地址 | 内容 |
|---|---|---|---|
| 主站 → 从站 | SM2（dir `out`） | `0x1100`，4 字节 | `u32` 小端：计数值 |
| 从站 → 主站 | SM3（dir `in`） | `0x1110`，4 字节 | `u32` 小端：**计数值 + 1** |

对象字典：`0x7000:01`（UINT32，rw，RxPDO）、`0x6000:01`（UINT32，ro，TxPDO）；
默认分配 `0x1C12`/`0x1C13` → `0x1600`/`0x1A00`；另有 `0x1008`/`0x1018` 身份对象。

行为逻辑全部在 [`behavior.wat`](behavior.wat)（**自写 WAT**，无第三方代码），插件只导入
宿主提供的 `env.read_pdo` / `env.write_pdo` / `env.log`，导出 `tick()`：

- PDO arena 布局 = `[SM2 | SM3]`，SM2 容量 4 → 插件写 `write_pdo(4..=7)` 即写 SM3；
- 首次 `tick` 写一行日志 `counter tick`，之后不再写（避免刷满 64 KiB 日志缓冲）；
- `tick` 由 worker 线程按 `tick_hz: 1000` 节流调用，**不进入实时环**；宿主帧循环只做
  arena `publish` 与 `poll_output`（`try_lock`，绝不等待插件）。

## 怎么跑

需要 `wasm-runtime` feature（可选依赖 wasmi/wat 要从 crates.io 拉取）：

```bash
cargo test -p esc-node --features wasm-runtime --test behavior_counter_plugin
# 或一键探针（本机可直接跑；无 cargo/网络时 exit 2 = SKIP）
bash scripts/spikes/wasm-behavior-probe.sh
```

Linux 上还可以直接用 CLI 帧循环（TAP/UDS 线缆层仅 Linux 可用）：

```bash
cargo run -p esc-node --features wasm-runtime -- --uds /tmp/ecsim.sock \
  --manifest plugins/behavior-counter/plugin.yaml
```

⚠️ CLI 的 UDS/TAP 端到端尚未在验证机跑过（验证机无 crates.io，wasm-runtime 依赖拉不下来）；
目前证据是上面那条集成测试：真实磁盘 manifest + WAT → `BehaviorBridge` → 合成主站帧
写 SM2 / 读 SM3。验证方法：在能访问 crates.io 的 Linux 机器上执行上面那条 `cargo run`
并配合 `scripts/spikes/uds_master_probe.py` 的帧序列。

## 许可

Apache-2.0 OR MIT（与仓库核心一致）。
