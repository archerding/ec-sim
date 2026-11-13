# Gate B-2 / PV-18：验收探针聚合脚本（`run-acceptance-probes.sh`）证据（2026-11-13）

> 任务板 `task-18`（Owner `sim-impl`）。本文件是**脚本与 Runbook 证据**，不是认证结论。
> 对应 [EC-SIM ACCEPTANCE](../../../../cnc-station/docs/product/ECSIM-ACCEPTANCE.md) §3.2.2（B-2 行业协议探针组）
> 与 PV-18；ACCEPTANCE 属 sim-expert 维护，本文件只给"脚本 + 追溯表 + 本机运行记录"，跨仓文档改动由 sim-expert 采认。
>
> **边界（防"全绿=全覆盖"误读）**：B-2 的 37 项**不等于** ACC-01..27 全覆盖——ACC-17（HIL/物理安全链）
> 不纳入自动化；ACC-18 即 B-2 本身；ACC-19..21 属 Gate A；ACC-23..25 为跨项目联合验收（CNC 侧结论，不在本脚本）。
> 本脚本只覆盖"可自动化、脚本已存在"的探针（ACCEPTANCE §7.1 由 sim-expert 同步写明）。

## 1. 交付物

| 项 | 内容 |
| --- | --- |
| 脚本 | `scripts/spikes/run-acceptance-probes.sh`（零第三方依赖，bash；`--list`；`TIMEOUT_SECONDS`/`LOG_DIR`/`RUN_ROOT`/`BIN`/`PY` 可覆盖） |
| 注册表 | 脚本内单一清单 `PROBES`（`name|kind|acc`），**项数由脚本自算打印**（本轮 **37** 项），不做硬编码 |
| 输出格式 | `<name>|<kind>|<EXIT>|<PASS|FAIL|SKIP>|<秒>[|<skip_reason>]`；末尾 `== 汇总：N PASS / M FAIL / K SKIP（共 T）==` + `RESULT: PASS|FAIL` |
| SKIP 规则 | 必须带原因：`平台不适用（需要 Linux）` / `缺 root（TAP/内核模块/IgH 需要）` / `RUN_ROOT=0` / `探针 exit 2（缺夹具/可选依赖）：<末行>`；**禁止**把失败改名 SKIP |
| 日志 | 每项 `$LOG_DIR/<name>.log`（默认 `/tmp/ec-sim-acceptance`）；FAIL 行带日志路径与末行摘要 |
| 退出码 | `0` = 0 FAIL（允许有理由 SKIP）；`1` = 任一 FAIL（含超时 `124`、探针缺失） |

## 2. 与 `run-all-probes.sh` 的分工（Gate B-1 vs B-2）

| 脚本 | 覆盖 | 判定 |
| --- | --- | --- |
| `run-all-probes.sh`（B-1 观测面/控制面） | 控制面路由/SSE/指标/token/MCP/Web 静态回归等（注册表自算，当前 23 项） | `0 FAIL`；`PASS + SKIP = 注册表项数`；每个 SKIP 有理由 |
| **`run-acceptance-probes.sh`（B-2 行业协议验收，本文件）** | 发现/身份、SII、OD/PDO、SDO、AL/OP、WKC/多站、DC、Watchdog、控制面交叉、CiA402 Tier0（37 项，按 ACC 组织） | `RESULT: PASS`（0 FAIL）；SKIP 必须有原因；真机/HIL 项不在注册表 |
| 串联 | Gate B = 先 B-1 再 B-2（`bash scripts/spikes/run-all-probes.sh && bash scripts/spikes/run-acceptance-probes.sh`） | 两者都 PASS 才算 B 门完成 |

## 3. 本机静态自测（Windows，非 root；2026-11-13）

```text
$ bash scripts/spikes/run-acceptance-probes.sh --list
READY mcp-stdio-probe / serve-token-probe / invalid-slave-probe
SKIP  … (其余项，原因=平台不适用（需要 Linux）/缺 root)
== 注册表项数：37（由脚本自算）==

$ bash scripts/spikes/run-acceptance-probes.sh
…（数据行）…
== 汇总：3 PASS / 0 FAIL / 34 SKIP（共 37）==
RESULT: PASS
```

- 本机 3 个 `any` 类探针 **PASS**；其余 SKIP 均带平台/root 原因（符合 ACCEPTANCE §1.3）。
- 一个真实拦截案例：首次运行时 `invalid-slave` FAIL——原因是本机 `target/release/ec-sim-esc` 是**旧构建**
  （B55 修复前），重新 `cargo build --release` 后 PASS；说明脚本能抓到"证据二进制与源码不一致"。

## 4. VM 整轮证据（2026-11-13 轮次 B：PASS）

**轮次 A（13:16，作废）**：当时 `vm-ecsim.lock` 已被 sim-expert（task-22）持有且其 `ecrt_master_sdo_upload`
进程处于内核 D 状态、`ec_master` 被占用 → B-2 前 4 项报 `FAIL: IgH 未看到身份`，B-1 亦在冲突窗口内。
按 Lead 判定整轮作废、不采信（已停进程、未 reboot）。教训：VM 共用必须先校验锁 owner 并等对方释放信号。

**轮次 B（13:24–13:28，clean VM，单 runner，**PASS**）**：

- 前置自检：`pgrep ecrt-` 空、无 D 状态**用户进程**（只排除内核线程 `[kworker/...]`）、`lsmod` 无 `ec_*`、
  无 `escsim*` TAP → `PRECHECK_OK`；`vm-ecsim.lock` = sim-impl；未 reboot。
- 构建：`cargo build --release -p esc-node -p mcp-server`（源码 `30ef83b`，等价代码 `9aae5ff`）→
  `ec-sim-esc` sha256 `7ec957e8e59019811c8ab9ba8c068a1caff18f554348df7a96bfb900f609344c`（1,060,800 B）。
- 定向探针：
  - `invalid-slave-probe.sh`（静态 + UDS + **TAP** 三路径）**EXIT=0**（TAP 段：非法/越界 404 JSON，0/1 均 200）→
    ACC-27② TAP 段闭环；
  - `al-error-probe.sh`（B57）**EXIT=0**：`/state.live.al_status=0x11`、`al_status_code=0x0024`、
    `/metrics` 17/36；
  - `tap-cia402-multi-slave-probe.sh` N=2 **EXIT=0**（2 站 OP / workcounter 6 / 14O+22I / decoded 含 606C）；
    N=3 **EXIT=0**（3 站 OP / workcounter 9 / 21O+33I）→ V2 参数（`--slaves 3` 同画像克隆）已预回归。
- **B-1**：`run-all-probes.sh` → **`23 PASS / 0 FAIL / 0 SKIP（共 23）`**、`RESULT: PASS`（含 `tap-live`/
  `tap-cia402`/`invalid-slave`）。
- **B-2**：`run-acceptance-probes.sh`（`NOT_JUDGED` 7 个 `igh-*`）→ **`29 PASS / 0 FAIL / 7 SKIP（共 36）`**、
  `RESULT: PASS`；7 个 SKIP 原因均为
  `NOT-JUDGED（已知环境阻塞 PV-01/PV-14/H6，未执行；不计 PASS）`：`igh-op-probe`、`igh-cia402-op-probe`、
  `igh-cia402-csv-op-probe`、`igh-el2008-op-probe`、`igh-sv660n-op-probe`、`igh-cia402-cst-op-probe`、`igh-dc-op-probe`。
- 同轮修掉的**陈旧探针断言**（真实缺陷，非 SKIP 规避）：
  1. `soem-dc-probe.sh` 期望 `mapped 7O+7I` → SV660N 画像默认可变映射早已改为 **8B/22B**（手册核对），
     改为断言 `mapped 8O+22I`；修后 **EXIT=0**（OP、WKC 3、`dc_rd=5335 dc_wr=5337`）。
  2. `manifest-slave-probe.sh` 依赖 SOEM 英文文案（"Sample counter"，且该构建把 `r` 渲染成 `n`）与固定对象数
     （7→现 8 个对象）→ 改为按稳定内容断言（`0x1018/0x1c12/0x1c13/0x2000` + `0x1234 / 4660`）；修后 **EXIT=0**
     （SDO Info + OP/WKC3 + IgH 身份）。
- 日志（EC-Sim VM `/tmp`）：`/tmp/sim-impl-vm-round/{run-all,run-acceptance,invalid-slave,al-error,tap-cia402-n2,tap-cia402-n3}.log`、
  `/tmp/sim-impl-vm-round-console.log`、`/tmp/b2-clean.log`、`/tmp/recheck-{dc,manifest}.log`。

```bash
# clean VM（root，单 runner，先持锁）：
cargo build --release -p esc-node -p mcp-server
bash scripts/spikes/run-all-probes.sh          # B-1：23 PASS / 0 FAIL / 0 SKIP
NOT_JUDGED="igh-op-probe,igh-cia402-op-probe,igh-cia402-csv-op-probe,igh-el2008-op-probe,igh-sv660n-op-probe,igh-cia402-cst-op-probe,igh-dc-op-probe" \
  bash scripts/spikes/run-acceptance-probes.sh # B-2：29 PASS / 0 FAIL / 7 SKIP(NOT-JUDGED)
```

> `NOT_JUDGED` 参数：逗号分隔探针名 → 运行时标 `SKIP` 且原因写
> `NOT-JUDGED（已知环境阻塞 PV-01/PV-14/H6，未执行；不计 PASS）`；这是**如实标注**，不是"用 SKIP 刷绿"
> （ACCEPTANCE §1.2 的 `NOT-JUDGED` 语义）。仅在验证机确有已知 D 状态阻塞时使用，并在证据中写明原因。

**轮次 C（同 VM，新探针接入后，**PASS**）**：`pdo-byteorder-probe.sh`（task-41 / GAP-14 端到端字节序）加入注册表
（36 → **37**，脚本自算）后整轮复跑：

- `pdo-byteorder-probe.sh` **EXIT=0**：合成主站写 SM2（controlword `0x000F` / mode 8 / target 1000）→
  SM3 原始字节 **`3706e80300000000000008`**（statusword 0x0637 | pos 1000 | vel 0 | mode 8）；
  同探针断言 manifest Tx 顺序 `[0x6041,0x6064,0x606C,0x6061]` 与 SM3 11B，并做**旧顺序负例判别**
  （旧布局 mode 在 offset2、position 在 3..7 → 同字节序列下两断言均不成立）。
- **B-2**：`== 汇总：30 PASS / 0 FAIL / 7 SKIP（共 37）==`、`RESULT: PASS`（`pdo-byteorder-probe` 为新增 PASS；
  其余同轮次 B，7 项 `igh-*` 仍 `NOT_JUDGED`）。
- 日志：VM `/tmp/b2-37.log`；归档副本见 CNC 仓库 `docs/verification/assets/ecsim-round/`（Lead 指定路径）。

## 5. ACC ↔ 探针追溯表（本脚本注册表，37 项）

| ACC | 探针（kind） |
| --- | --- |
| ACC-01/02 发现与身份 | `minimal-esc-responder`(root)、`soem-multi-slave-probe`(linux)、`uds-multi-slave-probe`(linux)、`tap-multi-slave-probe`(root) |
| ACC-03/04 SII / 邮箱 | `esi-sii-scan-probe`(root)、`soem-mailbox-probe`(root) |
| ACC-05/06 能力位 / DC 声明 | `sv660n-capability-probe`(root) |
| ACC-07 对象字典 / PDO 映射 | `manifest-slave-probe`、`esi-manifest-op-probe`、`beckhoff-el2008-probe`、`generic-cia402-probe`（均 root）、`cia402-modes-probe`(linux)、`tap-cia402-multi-slave-probe`(root，EC-Sim 侧 V1/V2 布局证据)、**`pdo-byteorder-probe`(linux，GAP-14 TxPDO 字节序端到端 + 旧顺序负例判别)** |
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
