# B62：运行期 AL 遥测 + SII 并发/看门狗恢复探针（2026-11-14）

> 目的：补齐 C5 24h 失败 RCA 暴露的**观测缺口**——长跑失败时不知道"哪个从站、何时、因何"离开 OP。
> 本项只增加遥测与探针，**不改变**协议语义、ESM 行为或 24h 判据；不构成一致性/认证结论。
> 关联：[C5 失败 RCA](2026-11-14-c5-fail-rca.md)、`docs/CONTROL-PLANE.md` §4.1/§4.8。

## 1. 变更

### `esc-node`

- `Stats` 新增 6 个计数器：`al_transitions` / `al_errors` / `al_error_acks` /
  `sm_watchdog_triggers` / `sii_read_commands` / `sii_write_commands`。
- 新增 `AlEvent { from, to, from_code, code, watchdog, at_ms }`：
  - `Responder::last_al_event()`——只读 peek，供 `/state` 展示，**不会被日志消费清空**；
  - `Responder::poll_al_event()`——取出即清，供运行期日志逐条打印。
- `sync_al_state()`、`trigger_watchdog()`、`force_al_error()` 统一记账；SII 计数从
  `EepromInterface::read_commands()/write_commands()` 镜像（0x0500 窗口读写均计）。

### `ec-sim-esc`

- UDS/TAP 两个帧循环在每帧处理后打印（stderr）：

```
AL_EVENT slave=0 from=0x08 to=0x14 from_code=0x0000 code=0x001B watchdog=true at_ms=1794642943815
```

- `/state` 的 `live.last_al_event` 与 `live.stats` 6 字段、SSE 增量、`/metrics` 6 个
  `ecsim_*_total` 同步暴露；`/metrics` 计数器数组 9→15。

### 探针

- 新增 `scripts/spikes/uds_al_telemetry_probe.py`（纯标准库，复用 `uds_master_probe` 合成主站）
  与 `scripts/spikes/al-telemetry-probe.sh`（启动 `--uds` + `--serve`，抓 `AL_EVENT` 日志）。
- 覆盖：ESM 进 OP → **过程数据活跃时 SII 并发读 40 轮**（每轮先写 SM2，再读 SII word
  `0x0008`=Vendor ID `0x0EC5`；断言无 busy/error、AL 保持 OP）→ 停 PDO 30 ms 触发 **SM watchdog**
  （`0x001B`、OP→SAFEOP）→ **Error ACK** → 重新请求 OP 恢复；并核对 `/state` 计数、
  `last_al_event`、`/metrics` 6 个计数器与 6 条 `AL_EVENT`。
- 登记 B-1（`al-telemetry|linux`），注册表 **24 项**。

## 2. 验证（EC-Sim 自己的验证机；未触碰 CNC 验证机）

- 来源：commit `36d6dc7`（`git archive HEAD` 传到 EC-Sim 验证机，`cargo build --release -p esc-node`）。
- 本机：`cargo fmt --all -- --check` 0；`cargo clippy --workspace --all-targets -- -D warnings`
  0 warning；`cargo test --workspace` **265 passed / 0 failed**（较上轮 +1：SII 计数镜像单测）。
- 验证机（Linux / openEuler 24.03 SP4，EC-Sim 自有 VM，无 CNC 栈）：

```
BIN=target/release/ec-sim-esc LOG_DIR=/tmp/b62-probes bash scripts/spikes/run-all-probes.sh
== 汇总：24 PASS / 0 FAIL / 0 SKIP（共 24）==
RESULT: PASS
```

新探针单独运行输出（`PROBE_EXIT=0`）：

```
OK: SII 并发读 40 轮 + 看门狗 0x001B + Error ACK/恢复 OP 通过
OK: AL_EVENT 运行期事件 6 条（含 watchdog 触发与恢复）
OK: /state 与 /metrics 遥测字段完整（SII 计数、AL 迁移、看门狗、last_al_event）
OK: SII 并发读 + 看门狗 0x001B + Error ACK/恢复通过
```

`AL_EVENT` 实录（同一进程，ESM 配置 + 看门狗 + ACK + 恢复）：

```
AL_EVENT slave=0 from=0x01 to=0x02 from_code=0x0000 code=0x0000 watchdog=false at_ms=1794642943767
AL_EVENT slave=0 from=0x02 to=0x04 from_code=0x0000 code=0x0000 watchdog=false at_ms=1794642943767
AL_EVENT slave=0 from=0x04 to=0x08 from_code=0x0000 code=0x0000 watchdog=false at_ms=1794642943767
AL_EVENT slave=0 from=0x08 to=0x14 from_code=0x0000 code=0x001B watchdog=true  at_ms=1794642943815
AL_EVENT slave=0 from=0x14 to=0x04 from_code=0x001B code=0x0000 watchdog=false at_ms=1794642943816
AL_EVENT slave=0 from=0x04 to=0x08 from_code=0x0000 code=0x0000 watchdog=false at_ms=1794642943816
```

## 3. 验证中发现并修掉的问题

| 现象 | 根因 | 修复 |
|---|---|---|
| `/state.last_al_event` 恒为 `null`，而日志已有事件 | 帧循环的 `log_al_events` 用 `poll_al_event` 消费了唯一的事件槽 | 拆成持久 `last_al_event`（`/state` 只读）+ `pending_al_event`（日志 `poll` 消费）；commit `0bb5818` |
| 探针 grep `watchdog=true` 失败（日志为 `watchdog=1`） | 日志把 bool 转成 `u8` | 改为打印 `true`/`false`，与文档/断言一致；commit `36d6dc7` |

## 4. 边界

- 遥测在**现有二进制**上即可用；C5 本轮按 Lead 决定沿用既有部署二进制（不含本遥测），
  下一轮再部署含遥测版本。
- 本探针只证明 EC-Sim 侧计数/事件/恢复路径可用；**不**证明 C5 slave1 在 14:40 的精确
  AL 码（该结论仍为 NOT-JUDGED，需下一轮 24h 用 `/state`/`/metrics` 取证）。
- 未修改 `--serve` 鉴权、只读/写路径、ESM 前置条件或 24h 判据。
