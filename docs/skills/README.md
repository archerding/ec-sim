# Agent Skills（EC-Sim，只读）— 索引与格式（B52 / M4）

> 本目录把 EC-Sim 的 **9 个只读 MCP 工具**封装成 Agent 可加载的"技能"（SKILL.md + 示例请求），
> 用于**诊断/排障/生成观测摘要**。**不包含任何写工具**，不宣称一致性/认证（`AGENTS.md` §4）。
> 机器可读工具目录：[`docs/plan/m3-w8-mcp-tools-v1.json`](../plan/m3-w8-mcp-tools-v1.json)（单一事实来源）；
> 静态校验：`python3 scripts/spikes/agent-skills-check.py`（已登记进 `run-all-probes.sh` 的 `agent-skills`）。

## 技能清单

| 技能 | 用途 | 工具（只读） | 实时要求 |
|---|---|---|---|
| [`ecsim-diagnose`](ecsim-diagnose/SKILL.md) | 单段/单站诊断：健康 → 状态 → 拓扑 → PDO → SDO → 指标 → 审计；输出可归档摘要 | `ecsim_health`、`ecsim_state`、`ecsim_dict`、`ecsim_topology`、`ecsim_pdo`、`ecsim_pdo_history`、`ecsim_metrics`、`ecsim_sdo_read`、`ecsim_sdo_audit` | 部分工具仅实时模式（见技能内标注） |

## SKILL.md 格式（本仓库约定）

```markdown
---
name: <kebab-case，且与目录名一致>
description: <一句话说明何时使用（≥20 字符）>
version: <x.y>
mode: read-only
tools:
  - ecsim_<工具名>
---

# 指令正文（步骤、判据、错误码、边界）
```

约束（由 `agent-skills-check.py` 校验）：

1. `mode` 必须是 `read-only`；`tools` 必须全部来自 MCP 工具目录，且**不得**出现 `write` / `post` 类工具；
2. 正文必须写明**只读边界**与**快照语义**（观测值不构成实时/一致性保证）；
3. `examples/*.jsonl` 每行必须是合法 JSON-RPC 2.0；`tools/call` 的工具名必须在目录内；
4. 相对链接必须可解析；示例中不得出现 token/凭据。

## 只读边界

- MCP 桥接层只有 GET 工具；`POST /sdo`、`POST /pdo` **永不映射**（即使控制面 `--serve-write` 已开启）。
- 技能里任何"写入/改配置"诉求都应转为：提示用户用控制面显式写开关 + 人为确认；本目录不提供写示例。
- 控制面默认无鉴权、仅回环/受信环境；token 模式需由 MCP `--token-env` 从环境变量注入（技能不保存 token）。

## 验证

```bash
python3 scripts/spikes/agent-skills-check.py            # 静态校验（任意平台，无 root）
bash scripts/spikes/run-all-probes.sh --list | grep agent-skills
```

> 校验结果只代表"文档/示例与工具目录一致"，**不构成**认证或功能保证。
