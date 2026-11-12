# EC-Sim Web 只读视图（M2）

最小、零第三方依赖的只读页面（`index.html` + `app.js`），把 EC-Sim 的只读数据渲染为
身份 / SM / PDO（原始 + 具名解码 + Canvas 波形）/ 对象字典 / 总线拓扑（链式 SVG + 表）/
指标卡片。**只读**，不构成一致性或认证结论。

## 用法

```bash
# 方式 A：文件/静态服务 + dump JSON
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --dump-state --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/state.json
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --dump-dict --json --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/dict.json
python3 -m http.server 8080 --directory web   # 打开 http://127.0.0.1:8080/

# 方式 B：控制面只读 API（实时模式）+ 同源托管本页（推荐）
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8090 --slaves 3 --web-root web
# 直接打开 http://127.0.0.1:8090/ ，API 地址填 http://127.0.0.1:8090 后“从 API 载入”
```

页面支持：
- 选择/多选 dump JSON 文件（state-v1 / dict-v1 / pdo-v1 / pdo-history-v1 / topology-v1）；
- 填 API 地址后拉取 `/state`、`/dict`、`/pdo`、`/pdo?history=1`、`/topology`、`/metrics`；
- “从站 N”切换与拓扑节点/行点击切换（对应 API 的 `?slave=N`）；
- “实时刷新”开关：优先 SSE `GET /events`（状态推送）+ 2s 拉取 PDO/拓扑/指标；无 EventSource 时回退 1s 轮询；
- PDO Canvas 波形（SM2/SM3 前 2 字节）与 AL 状态色（INIT 橙 / PREOP 蓝 / SAFEOP 紫 / OP 绿）。

## 数据格式

schema 与字段说明见 **[docs/CONTROL-PLANE.md](../docs/CONTROL-PLANE.md)**：
`ec-sim/state-v1`、`ec-sim/dict-v1`、`ec-sim/pdo-v1`、`ec-sim/pdo-history-v1`、
`ec-sim/topology-v1` 与 `/metrics` 文本指标。

## 验证

- `node --check web/app.js`（语法）；
- 浏览器人工验收（本班起含链式 SVG 拓扑图；⚠️ 无自动化 DOM 测试）；
- 若需回归控制面契约，用 `scripts/spikes/*-probe.sh` 与 `control-plane-doc-check.py`。
