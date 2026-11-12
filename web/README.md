# EC-Sim Web 只读状态视图（M2 起点）

最小、零依赖的只读页面：把 `ec-sim-esc --dump-state` 输出的 JSON 渲染为
身份 / Sync Manager / PDO 映射 / DC 摘要。**不是**完整 WebUI（无实时波形、无编辑），
也不构成一致性或认证结论。

## 用法

```bash
# 1) 生成状态 JSON（任选一个画像；不带 --manifest 为内置画像）
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --dump-state --manifest plugins/generic-cia402-cst-servo/plugin.yaml > web/state.json

# 2) 静态服务（Python 标准库即可）
python3 -m http.server 8080 --directory web
# 打开 http://127.0.0.1:8080/
```

也可以直接双击 `web/index.html` 打开，再选择 JSON 文件载入（`file://` 下不会自动读取
`state.json`，页面会提示选择文件）。

## 数据格式

`state-v1`：见 `ec-sim-esc --dump-state` 输出与 `docs/DEVICE-PLUGINS.md`。
字段：`identity`、`names`、`mailbox_declared`、`coe_details`、`general_flags`、
`ebus_current_ma`、`sync_managers[]`、`pdo_tx[]`/`pdo_rx[]`、`dc`（可空）、
`profile`（manifest 时）、`dict_objects`（manifest 时）。

## 后续（M2）

- 实时拓扑/PDO 波形/SDO 编辑器需要控制面与 WebSocket/HTTP API（见路线图 M2，未实现）。
