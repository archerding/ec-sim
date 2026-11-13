# M4 / B52 验收记录：Agent Skills 只读技能文档与示例（2026-11-13）

> 任务板 `task-12`（Owner `sim-impl`）。本文件是**验收证据**，不是认证或功能保证。
> 验收对象：EC-Sim `main` `7d665da`（B52 提交，2026-11-13 12:37 班）；同窗口后续提交
> `182147d`/`8ba9b41`/`d3c99a1` 为 B54/B55/画像小改进，不改变 B52 交付物。
> 关联：[STATUS](../STATUS.md)、[M4 §2.2](../product/ECSIM-ROADMAP.md)（产品侧）、
> [ECSIM-ROADMAP](../../../cnc-station/docs/product/ECSIM-ROADMAP.md)（跨仓，只读引用）、
> [M3-W8 MCP 设计](plan/M3-W8-mcp-agent-bridge-stub.md)。

## 1. B52 交付物清单（逐项核对）

| # | B52 承诺 | 仓库中的实现 | 核对方式 | 结果 |
| --- | --- | --- | --- | --- |
| 1 | Agent Skills 目录与格式约定 | `docs/skills/README.md`（SKILL.md 格式：`name/description/version/mode=read-only/tools`、约 4 条校验约束、只读边界） | 人工读 + `agent-skills-check.py` | ✅ |
| 2 | 一个可用的诊断技能 | `docs/skills/ecsim-diagnose/SKILL.md`（9 个只读 MCP 工具：health/state/dict/topology/pdo/pdo_history/metrics/sdo_read/sdo_audit；步骤/判据/错误码/边界） | 同上 | ✅ |
| 3 | 示例会话（静态/实时） | `docs/skills/ecsim-diagnose/examples/{static,live}-session.jsonl`（每行合法 JSON-RPC 2.0；无 token/凭据） | 脚本逐行 JSON 解析 + `tools/call` 名在工具目录内 | ✅ |
| 4 | 静态校验脚本 | `scripts/spikes/agent-skills-check.py`（零第三方依赖；frontmatter、mode=read-only、tools ⊆ MCP 目录且无写工具、正文只读/快照边界、examples JSON-RPC、相对链接、无 token） | `python3 scripts/spikes/agent-skills-check.py` → **EXIT=0** | ✅ |
| 5 | 登记进整轮探针 | `scripts/spikes/run-all-probes.sh` 注册表 `agent-skills|any`（B52 时 20→21 项；B54/B55 后为 23 项） | `bash scripts/spikes/run-all-probes.sh --list` 输出含 `READY agent-skills` | ✅ |
| 6 | 只读边界（不引入写工具） | 工具目录 `docs/plan/m3-w8-mcp-tools-v1.json` 仍 9 个只读 GET；`POST /sdo`、`POST /pdo` 永不映射 | `mcp-tools-check.py` EXIT=0；`agent-skills-check.py` 无 write/post | ✅ |
| 7 | 不宣称认证 | `docs/skills/README.md` 与 SKILL.md 明示"不构成认证/功能保证"、快照语义 | 人工读 + 全文检索"认证/认证"表述 | ✅ |
| 8 | 文档同步（AGENTS/STATUS/README） | `AGENTS.md` §6 B52 ✅；`docs/STATUS.md`；`README.md` roadmap；`docs/VERIFICATION.md` 探针行 | `git show 7d665da --stat`（11 文件 +402 行） | ✅ |

**B52 提交自查**（`git show --stat 7d665da`）：11 文件、+402/−12，含 `docs/skills/README.md`、SKILL.md、
2 个示例、`agent-skills-check.py`、`run-all-probes.sh`、AGENTS/STATUS/VERIFICATION/README。无代码语义改动。

## 2. 验证命令与结果（2026-11-13 团队班复核）

```text
$ python3 scripts/spikes/agent-skills-check.py
ok   ... frontmatter / mode=read-only / tools ⊆ MCP 目录 / 无写工具 / 正文只读边界 /
     examples JSON-RPC 合法 / 相对链接可解析 / 无 token
OK: agent-skills（1 个技能、9 个工具引用、15 条示例行）
→ EXIT=0

$ bash scripts/spikes/run-all-probes.sh --list
READY agent-skills  (agent-skills-check.py)
...
（注册表当前 23 项：B52 后新增 B54 `tap-cia402`、B55 `invalid-slave`）

$ python scripts/spikes/mcp-tools-check.py
→ EXIT=0（9 个只读 GET 工具；0 个写工具）
```

本机（Windows，非 root）可跑的 any 类项同样通过（`doc-check`/`mcp-tools`/`agent-skills`/
`web-sdo-panel`/`serve-token`/`mcp-stdio`/`invalid-slave` 静态段）。

## 3. CI 观察窗（GitHub Actions `main` 最近 5 次 run）

观察时间：2026-11-13 13:0x（UTC+8）；查询：GitHub REST `actions/runs?branch=main&per_page=5`。

| run | commit | 状态 | 结论 | 说明 |
| --- | --- | --- | --- | --- |
| **153** | `d3c99a1`（B54 画像小改进） | completed | **success** | 4 jobs（`test (ubuntu/windows/macos-latest)` + `docs-and-probes`） |
| 152 | `8ba9b41`（B55 P1 修复） | completed | **cancelled** | **预期**：`ci.yml` 有 `concurrency: cancel-in-progress: true`，紧接着的 153 push 取代它（非失败、非 flaky） |
| 151 | `182147d`（B54） | completed | **success** | 同 4 jobs |
| 150 | `7d665da`（B52） | completed | **success** | 同 4 jobs |
| 149 | `10b990a`（B51） | completed | **success** | 同 4 jobs |

**判定口径**：workflow 的 `test` job 是 3 OS 矩阵（`fail-fast: false`）+ `docs-and-probes`（ubuntu 非 root
文档一致性/探针/报告 artifact）。任一 job 失败 ⇒ run 结论为 failure；故 run **success ⇒ 4/4 job 绿**。
局限（⚠️）：当前 GitHub fine-grained token 只有 Contents/Administration 权限，`/actions/runs/{id}/jobs`
返回 404，**无法逐个 job 列名**；本记录以 run 级结论 + workflow 结构判定（可在有 Actions:read 权限的
token 或网页上复核）。观察窗内 **0 个未解释失败**；1 次 cancelled 已定位为并发取代（预期行为）。

> 结论：**M4 的 CI 观察窗（"连续 10 次绿"起步阶段）当前 4 连绿 + 1 次预期取消**，无 flaky 迹象。
> flaky 统计（30 天 <2%）仍是 ⬜（需更长窗口，见 ROADMAP §2.2 第 5 项）。

## 4. M4 退出映射（与 sim-pm 的 ECSIM-ROADMAP §2.2 对齐）

| M4 项 | 状态 | 证据 |
| --- | --- | --- |
| 1 只读 MCP 正式化 + 覆盖矩阵（B47/B50） | ✅ 既有 | `mcp-stdio-probe` 31 断言；本窗口 149/150/151/153 CI 绿 |
| 2 Agent Skills（**B52，本验收对象**） | ✅ **验收通过**（本文件） | 静态校验 EXIT=0 + 整轮注册 + 只读边界 |
| 3 可归档测试报告（B49） | ✅ 既有 | `gen-test-report.py`；CI artifact |
| 4 CI 接入（`docs-and-probes`） | 🟡→**观察窗进行中** | 本轮 4 绿 + 1 预期取消；job 级明细需 Actions:read token 复核 |
| 5 flaky 统计（30 天 <2%） | ⬜ | 需更长窗口；阶段门槛"连续 10 次绿"当前 4/10 |
| 6 M4 范围定版/文档同步（区别 M5） | ⬜（产品侧 sim-pm） | ROADMAP §2.2 第 6 项 |

**结论（EC-Sim 实现侧）**：B52 验收 **PASS**；M4 达到"实现与验收证据齐全、CI 稳定观察中"的状态，
**不宣称 M4 关闭**（flaky 窗口与产品侧文档定版未完成，owner 分别为观察窗与 sim-pm）。

## 5. ⚠️ 未覆盖 / 边界

- 技能的**真实 Agent 宿主加载与执行**未自动化：仅有静态校验 + `mcp-stdio-probe`（工具端到端）；
  SKILL.md 的实际提示词行为属人工/后续评估。
- CI job 级明细无法用当前 token 拉取（仅 run 级结论）；建议有 Actions:read 权限时补一次逐 job 记录。
- 全部技能与示例只覆盖**只读**；写路径（`--serve-write`）刻意不映射。
- 不宣称 EtherCAT 一致性 / CTT / 任何认证；不承诺实时性。
