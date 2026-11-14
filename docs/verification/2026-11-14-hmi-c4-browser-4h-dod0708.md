# HMI C4 浏览器 4h + DOD-07/08 证据（2026-11-14）

> 状态：浏览器侧 4h **NOT-JUDGED（未收满）**；DOD-08 单独通过；DOD-07 未采。**不写 PASS**。
> 发布冻结、不宣称 v1.1/认证不变。

## 1. 结论

| 项 | 判定 | 依据 |
|---|---|---|
| C4 浏览器侧 4h 连续采样 | **NOT-JUDGED** | 三次尝试均未达 4h；attempt3 仅 33 行 / 32 分钟 |
| DOD-08（三分辨率 × 7 视图） | 已有证据 | 21 张截图 + `DOD-08-dom.json`（2026-11-13/14） |
| DOD-07 T-PERF-01..06 | **未采** | 浏览器 4h 未完成，不能替代 |

## 2. 尝试记录（同一台 EC-Sim VM、同一 `--serve` + SSH 隧道方案）

| 尝试 | 开始 | 结束 | 行数 | 失败表现 |
|---|---|---|---|---|
| attempt1 | 2026-11-14 04:0x | 约 04:5x | 5 | 早期 sampler 只写 5 行 |
| attempt2 | 2026-11-14 05:5x | 06:52 | 161（2h40m） | CDP `JSON.parse` 异常退出；Edge 未崩 |
| attempt3 | 2026-11-14 08:35:48 | **09:07:54** | **33（32 分钟）** | 进程静默消失；`sampler.err` 0 B；Edge 仍在（26 进程）；SSH 隧道 18210 消失 |

attempt3 参数：`START.txt` 记 `PID=50004 START=2026-11-14T08:35:48+08:00 ETA=2026-11-14T12:45:48+08:00 url=http://127.0.0.1:18210/index.html`。
末条采样（09:07:54）：`heap=1,702,136 requests=5776 timers_live=2 sse_connects=1 sse_opens=1 sse_errors=0 sse_messages=3840 sections=14 crashed=false`。

证据归档：`docs/verification/assets/hmi-go-b/c4-browser-4h/attempt3/`（`browser.jsonl` 7,863 B、`START.txt`、`sampler.out` 6,510 B、`sampler.err` 0 B）。

## 3. 失败分析（未定论）

- attempt2 为 CDP 解析异常；attempt3 为**静默退出**，无 stderr、无异常栈，页面侧 `crashed=false` / `sse_errors=0`。
- 两个共同点：**采样进程与 SSH 隧道均为父进程派生**；attempt3 结束时隧道已消失而 Edge 存活 → 页面/浏览器不是直接崩溃点，**父进程或隧道生命周期**是首要嫌疑。
- 因此该结果**不能**作为浏览器 4h 稳定性证据，也不能据此判 DOD-07。

## 4. 下一步（待 CNC HMI 线之后）

1. 改用 detached 采样：VM 内 `systemd-run`（或独立 watchdog）+ 隧道 keepalive/自动重连；采样端同时记录隧道/HTTP 可用性，连续 3 次失败才退出。
2. 目标 ≥230 行 / 4h（300s 间隔），并保留 START/END 时间与退出码。
3. 重试未满 4h 则 C4 浏览器侧继续 NOT-JUDGED；C4 数据面仍由 CNC VM 的 C5 24h 承担。
4. 不删除 attempt1–3 证据；不写未验证 PASS。

## 5. 环境清理

- EC-Sim VM `192.168.31.6`：已终止旧 `ec-sim-esc --tap escsim0 --slaves 8 --serve 0.0.0.0:18210`（PID 91518/91519）与 `hmi-c4-master-loop.sh`；`ip link del escsim0` 后确认 `TAP_GONE`。
- `vm-ecsim.lock` 由 Lead 释放（原 owner sim-impl，task-78）。
- EC-Sim HMI 设计线按用户指令暂停（task-89 HOLD），本证据不改变该状态。
