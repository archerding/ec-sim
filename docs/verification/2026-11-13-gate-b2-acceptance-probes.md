# Gate B-2 / PV-18：验收探针聚合脚本（`run-acceptance-probes.sh`）证据（2026-11-13）

> 任务板 `task-18`（Owner `sim-impl`）。本文件是**脚本与 Runbook 证据**，不是认证结论。
> 对应 [EC-SIM ACCEPTANCE](../../../../cnc-station/docs/product/ECSIM-ACCEPTANCE.md) §3.2.2（B-2 行业协议探针组）
> 与 PV-18；ACCEPTANCE 属 sim-expert 维护，本文件只给"脚本 + 追溯表 + 本机运行记录"，跨仓文档改动由 sim-expert 采认。
>
> **边界（防"全绿=全覆盖"误读）**：B-2 的 36 项**不等于** ACC-01..27 全覆盖——ACC-17（HIL/物理安全链）
> 不纳入自动化；ACC-18 即 B-2 本身；ACC-19..21 属 Gate A；ACC-23..25 为跨项目联合验收（CNC 侧结论，不在本脚本）。
> 本脚本只覆盖"可自动化、脚本已存在"的探针（ACCEPTANCE §7.1 由 sim-expert 同步写明）。

## 1. 交付物

| 项 | 内容 |
| --- | --- |
| 脚本 | `scripts/spikes/run-acceptance-probes.sh`（零第三方依赖，bash；`--list`；`TIMEOUT_SECONDS`/`LOG_DIR`/`RUN_ROOT`/`BIN`/`PY` 可覆盖） |
| 注册表 | 脚本内单一清单 `PROBES`（`name|kind|acc`），**项数由脚本自算打印**（本轮 **36** 项），不做硬编码 |
| 输出格式 | `<name>|<kind>|<EXIT>|<PASS|FAIL|SKIP>|<秒>[|<skip_reason>]`；末尾 `== 汇总：N PASS / M FAIL / K SKIP（共 T）==` + `RESULT: PASS|FAIL` |
| SKIP 规则 | 必须带原因：`平台不适用（需要 Linux）` / `缺 root（TAP/内核模块/IgH 需要）` / `RUN_ROOT=0` / `探针 exit 2（缺夹具/可选依赖）：<末行>`；**禁止**把失败改名 SKIP |
| 日志 | 每项 `$LOG_DIR/<name>.log`（默认 `/tmp/ec-sim-acceptance`）；FAIL 行带日志路径与末行摘要 |
| 退出码 | `0` = 0 FAIL（允许有理由 SKIP）；`1` = 任一 FAIL（含超时 `124`、探针缺失） |

## 2. 与 `run-all-probes.sh` 的分工（Gate B-1 vs B-2）

| 脚本 | 覆盖 | 判定 |
| --- | --- | --- |
| `run-all-probes.sh`（B-1 观测面/控制面） | 控制面路由/SSE/指标/token/MCP/Web 静态回归等（注册表自算，当前 23 项） | `0 FAIL`；`PASS + SKIP = 注册表项数`；每个 SKIP 有理由 |
| **`run-acceptance-probes.sh`（B-2 行业协议验收，本文件）** | 发现/身份、SII、OD/PDO、SDO、AL/OP、WKC/多站、DC、Watchdog、控制面交叉、CiA402 Tier0（36 项，按 ACC 组织） | `RESULT: PASS`（0 FAIL）；SKIP 必须有原因；真机/HIL 项不在注册表 |
| 串联 | Gate B = 先 B-1 再 B-2（`bash scripts/spikes/run-all-probes.sh && bash scripts/spikes/run-acceptance-probes.sh`） | 两者都 PASS 才算 B 门完成 |

## 3. 本机静态自测（Windows，非 root；2026-11-13）

```text
$ bash scripts/spikes/run-acceptance-probes.sh --list
READY mcp-stdio-probe / serve-token-probe / invalid-slave-probe
SKIP  … (其余项，原因=平台不适用（需要 Linux）/缺 root)
== 注册表项数：36（由脚本自算）==

$ bash scripts/spikes/run-acceptance-probes.sh
…（数据行）…
== 汇总：3 PASS / 0 FAIL / 33 SKIP（共 36）==
RESULT: PASS
```

- 本机 3 个 `any` 类探针 **PASS**；其余 SKIP 均带平台/root 原因（符合 ACCEPTANCE §1.3）。
- 一个真实拦截案例：首次运行时 `invalid-slave` FAIL——原因是本机 `target/release/ec-sim-esc` 是**旧构建**
  （B55 修复前），重新 `cargo build --release` 后 PASS；说明脚本能抓到"证据二进制与源码不一致"。

## 4. VM 整轮证据（⚠️ 待补；一次作废轮次已记录）

**轮次 A（2026-11-13 13:16，作废）**：本人在 EC-Sim VM 上启动整轮，但当时 `vm-ecsim.lock` 已被
sim-expert（task-22）持有且其 `ecrt_master_sdo_upload` 进程处于内核 D 状态、`ec_master` 被占用 →
B-2 前 4 项 `esi-sii-scan`/`sv660n-capability`/`manifest-slave`/`esi-manifest-op` 全报
`FAIL: IgH 未看到身份`，B-1 亦在冲突窗口内。**按 Lead 判定整轮作废、不采信**（已停止进程、未 reboot）。
教训：VM 共用必须**先校验锁 owner 并等对方释放信号**（同名锁同秒覆盖导致误判），单 runner 才可信。

**轮次 B（待跑，clean VM）**：

```bash
# EC-Sim 验证机（root，自有 VM；单 runner，先持锁 D:\tokengence.ai\.local\vm-ecsim.lock）
cargo build --release -p esc-node -p mcp-server
bash scripts/spikes/run-all-probes.sh          # B-1（0 FAIL；PASS+SKIP=注册表项数）
bash scripts/spikes/run-acceptance-probes.sh   # B-2（36 项；0 FAIL，SKIP 均带原因）
# 若验证机仍处 ecrt SDO 已知阻塞（PV-01/PV-14/H6），可对相关项显式标 NOT-JUDGED（不计 PASS）：
NOT_JUDGED="igh-cia402-op,igh-cia402-csv-op,igh-cia402-cst-op,igh-sv660n-op,igh-dc-op,sdo-upload-probe" \
  bash scripts/spikes/run-acceptance-probes.sh
```

> `NOT_JUDGED` 参数：逗号分隔探针名 → 运行时标 `SKIP` 且原因写
> `NOT-JUDGED（已知环境阻塞 PV-01/PV-14/H6，未执行；不计 PASS）`；这是**如实标注**，不是"用 SKIP 刷绿"
> （ACCEPTANCE §1.2 的 `NOT-JUDGED` 语义）。仅在验证机确有已知 D 状态阻塞时使用，并在证据中写明原因。

## 5. ACC ↔ 探针追溯表（本脚本注册表，36 项）

| ACC | 探针（kind） |
| --- | --- |
| ACC-01/02 发现与身份 | `minimal-esc-responder`(root)、`soem-multi-slave-probe`(linux)、`uds-multi-slave-probe`(linux)、`tap-multi-slave-probe`(root) |
| ACC-03/04 SII / 邮箱 | `esi-sii-scan-probe`(root)、`soem-mailbox-probe`(root) |
| ACC-05/06 能力位 / DC 声明 | `sv660n-capability-probe`(root) |
| ACC-07 对象字典 / PDO 映射 | `manifest-slave-probe`、`esi-manifest-op-probe`、`beckhoff-el2008-probe`、`generic-cia402-probe`（均 root）、`cia402-modes-probe`(linux)、`tap-cia402-multi-slave-probe`(root，EC-Sim 侧 V1/V2 布局证据) |
| ACC-08 SDO 上传 | `sdo-upload-probe`(root) |
| ACC-09 SDO 分段/下载 | `sdo-segmented-upload-probe`(root)、`cia402-sdo-download-probe`(linux) |
| ACC-10 SDO Info / CA | `sdo-info-probe`(root)、`sdo-ca-probe`(linux) |
| ACC-11..13 AL / OP（双主站） | `soem-process-data`(root)、`igh-op-probe`、`igh-cia402-op-probe`、`igh-cia402-csv-op-probe`、`igh-el2008-op-probe`、`igh-sv660n-op-probe`、`igh-cia402-cst-op-probe`（均 root）、`uds-loopback`(linux)、`al-error-probe`(linux，AL 错误注入/AC-E-203 交叉) |
| ACC-14 WKC / 多站 | `tap-multi-slave-probe`(root，8/32 站)、`soem-process-data`(root，1055 轮) |
| ACC-15 DC 配置路径 | `soem-dc-probe`(root)、`igh-dc-op-probe`(root)、`dc-clock-probe`(linux) |
| ACC-16 Watchdog | `uds-loopback`(linux，10 ms→SAFEOP+0x001B→ACK) |
| ACC-26 CiA402 Tier0 | `cia402-modes-probe`(linux，`0x6502` 与布局) + B-1 的 `cia402-*`/`mcp-stdio` 交叉 |
| ACC-27 控制面安全/只读 | `serve-token-probe`(any)、`sdo-write-probe`(linux)、`sdo-read-probe`(linux)、`topology-probe`(linux)、`mcp-stdio-probe`(any)、**`invalid-slave-probe`(any，ACC-27② 修复回归)** |

## 6. 边界 / ⚠️

- 注册表只含"可自动化、脚本已存在"的探针；**真机/HIL 项**（物理急停/安全门/真实伺服同步精度）不在其中，
  报告按 ACCEPTANCE §1.2 标 `NOT-JUDGED`，不计入 PASS 分子。
- `kind` 只表达平台/权限要求；ACC 列仅用于证据归档与追溯，不改变探针行为。
- 不宣称一致性/CTT/认证；结果为"预回归"。
