# EC-Sim Web 视图（M2 观测面 + M3 SDO 面板）

最小、零第三方依赖的 Web 页面（`index.html` + `app.js`），把 EC-Sim 的只读数据渲染为
身份 / SM / PDO（原始 + 具名解码 + Canvas 波形）/ 对象字典 / 总线拓扑（链式 SVG + 表）/
指标卡片，并提供 **SDO 面板**：只读浏览 `GET /sdo?index&sub`，以及在服务端
`--serve-write` 下经**二次确认**的受控写 `POST /sdo`。
**不构成**一致性或认证结论。

## 用法

```bash
# 方式 A：文件/静态服务 + dump JSON
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --dump-state --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/state.json
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --dump-dict --json --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/dict.json
python3 -m http.server 8080 --directory web   # 打开 http://127.0.0.1:8080/

# 方式 B：控制面 API（实时模式）+ 同源托管本页（推荐；SDO 面板需要此方式）
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8090 --slaves 3 --web-root web
# 直接打开 http://127.0.0.1:8090/ ，API 地址填 http://127.0.0.1:8090 后“从 API 载入”

# 方式 C：同上并显式开启受控写（SDO 面板写入才会成功）
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8090 --web-root web --serve-write
```

页面支持：
- 选择/多选 dump JSON 文件（state-v1 / dict-v1 / pdo-v1 / pdo-history-v1 / topology-v1）；
- 填 API 地址后拉取 `/state`、`/dict`、`/pdo`、`/pdo?history=1`、`/topology`、`/metrics`；
- “从站 N”切换与拓扑节点/行点击切换（对应 API 的 `?slave=N`）；
- “实时刷新”开关：优先 SSE `GET /events`（状态推送）+ 2s 拉取 PDO/拓扑/指标；无 EventSource 时回退 1s 轮询；
- PDO Canvas 波形（SM2/SM3 前 2 字节）与 AL 状态色（INIT 橙 / PREOP 蓝 / SAFEOP 紫 / OP 绿）。

### SDO 面板（B42 / M3-W6）

- **只读浏览**：填“索引（hex，可省略 0x）”与“子索引”后点“读取”，经 `GET /sdo?index&sub`
  从运行时对象字典读取（**不经过** mailbox/CoE 往返），显示对象名/条目名/类型/位长/访问/值
  （大端显示 + 原始小端 hex）。需先用“从 API 载入”连接 `--serve` 地址。
- **受控写**（默认关闭）：勾选“允许写入”→ 填新值（小端 hex，如 `0f00`）→ 点“写入…”→
  浏览器 `confirm()` 对话框**二次确认**后才发 `POST /sdo?index&sub`（请求体 = 原始小端字节）。
  服务端仍以 `--serve-write` + manifest `writable` 白名单 + 安全排除表 + 长度校验把关：
  未启用 → 405、非白名单/身份配置对象 → 403、未知 → 404、长度错 → 400；成功写入计入
  `GET /sdo-audit`（≤64 条）与 `/metrics` 的 `ecsim_sdo_writes_total`。页面无法探测写开关，
  因此写入是否生效**以服务端响应为准**。
- **token 模式**：若服务端以 `--serve-token-env` 开启鉴权，浏览器 `fetch` 无法自动携带
  `Authorization` 头；页面会提示 401，并建议经反向代理注入请求头或改用 `curl`。本页**不保存** token。

## 数据格式

schema 与字段说明见 **[docs/CONTROL-PLANE.md](../docs/CONTROL-PLANE.md)**：
`ec-sim/state-v1`、`ec-sim/dict-v1`、`ec-sim/pdo-v1`、`ec-sim/pdo-history-v1`、
`ec-sim/topology-v1`、`ec-sim/sdo-v1`、`ec-sim/sdo-audit-v1` 与 `/metrics` 文本指标。

## 验证

- `node --check web/app.js`（语法）；
- `python3 scripts/spikes/web-sdo-panel-check.py`：SDO 面板控件 id/逻辑标记/README 断言 +
  `node --check`（无 node 时 SKIP）；⚠️ 该检查是静态资源回归，**无 DOM 自动化点击**；
- 浏览器人工验收（本班起含 SDO 面板；⚠️ 无自动化 DOM 测试）；
- 若需回归控制面契约，用 `scripts/spikes/*-probe.sh` 与 `control-plane-doc-check.py`。
