# 控制面只读 API（`ec-sim-esc --serve`）

> 状态：M2 起步。当前控制面**只读**：只暴露状态/快照/指标，不提供任何写操作（SDO 写、PDO 激励、ESM 迁移等仍走主站或 UDS 合成主站）。
> 实现：`crates/control-plane`（JSON 序列化 + 路由判定）+ `crates/esc-node/src/bin/ec_sim_esc.rs`（std `TcpListener` HTTP 服务）。
> 依赖：**零外部依赖**（无 HTTP/序列化 crate），许可保持 Apache-2.0 OR MIT。

## 1. 启动方式与模式

```bash
# 静态快照模式：启动时生成一次 state/dict，只提供 /state、/dict（无 /pdo、/topology、/metrics）。
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --serve 127.0.0.1:8090 --manifest plugins/generic-cia402-cst-servo/plugin.yaml

# 实时模式（UDS）：与合成主站同进程，/state 含 live、/pdo、/topology、/metrics。
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8091 --slaves 3

# 实时模式（TAP，需 root）：与 SOEM/IgH 的 TAP 夹具同进程。
sudo target/release/ec-sim-esc --tap escsim0 --serve 127.0.0.1:8092 --slaves 1
```

- 绑定地址由用户显式给出（建议 `127.0.0.1`）；服务不监听其他地址、不做 TLS/鉴权。
- `--web-root <dir>`：同源托管 `web/` 白名单文件（`/` 与 `/index.html` → `index.html`，`/app.js` → `app.js`），
  UI 与 API 同端口，无需另起静态服务器；未给出或目录不存在时回退为纯 API（`/` 返回自述 JSON）。
- 服务只接受 `GET`；其他方法按 404/忽略处理（实现为路径匹配，未做方法校验）。
- 所有路由均带 `Access-Control-Allow-Origin: *` 与 `Cache-Control: no-store`，便于本机 WebUI 直接 `fetch`。

示例（同源托管 WebUI，推荐）：

```bash
cargo run -q -p esc-node --bin ec-sim-esc -- \
  --uds /tmp/ecsim.sock --esm-lax --serve 127.0.0.1:8091 --slaves 3 --web-root web
# 浏览器打开 http://127.0.0.1:8091/ ，页面内 API 地址填 http://127.0.0.1:8091 后“从 API 载入”
```

## 2. 路由总表

| 路径 | 模式 | schema | 说明 |
| --- | --- | --- | --- |
| `GET /` | 全部 | HTML 或内联 JSON | `--web-root` 有效时返回 `index.html`；否则服务自述 JSON + 路由列表 |
| `GET /index.html` | 全部 | HTML | 同上（显式 index） |
| `GET /app.js` | 全部 | JavaScript | Web 前端脚本（仅在 `--web-root` 有效时） |
| `GET /health` | 全部 | （内联 JSON） | 同上（健康检查） |
| `GET /state` | 全部 | `ec-sim/state-v1` | 只读状态快照；实时模式额外含 `live` 字段 |
| `GET /dict` | 全部 | `ec-sim/dict-v1` | 运行时对象字典（无 manifest 时 404） |
| `GET /pdo` | 实时 | `ec-sim/pdo-v1` | SM2/SM3 原始字节 + `decoded` 具名解码 |
| `GET /pdo?history=1` | 实时 | `ec-sim/pdo-history-v1` | 按变化去重的 PDO 时间序列（每站 256 条） |
| `GET /topology` | 实时 | `ec-sim/topology-v1` | 链上所有从站摘要 |
| `GET /metrics` | 实时 | Prometheus 文本 | 每站计数器/状态，`text/plain; version=0.0.4` |
| `GET /state?slave=N` | 实时 | `ec-sim/state-v1` | 指定从站（默认 0；越界/非法 → 404） |
| `GET /pdo?slave=N`（可叠加 `history=1`） | 实时 | 同上 | 指定从站的 PDO/历史（各站独立缓冲） |
| `GET /metrics?slave=N` | 实时 | Prometheus 文本 | 仅导出指定从站（默认全量） |
| 其他路径 | 全部 | `{"error": ..., "routes": [...]}` | 404 |

## 3. 示例

```bash
curl -s http://127.0.0.1:8091/state | head -c 400
curl -s http://127.0.0.1:8091/state?slave=1
curl -s http://127.0.0.1:8091/dict | head -c 300
curl -s http://127.0.0.1:8091/pdo
curl -s 'http://127.0.0.1:8091/pdo?slave=1&history=1'
curl -s http://127.0.0.1:8091/topology
curl -s http://127.0.0.1:8091/metrics
curl -s 'http://127.0.0.1:8091/metrics?slave=1'
curl -si http://127.0.0.1:8091/nope   # 404 JSON
```

## 4. Schema 摘要

### 4.1 `ec-sim/state-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` | 固定 schema 名与工具版本 |
| `identity` | `vendor_id` / `product_code` / `revision` / `serial`（`0x` 前缀大写 hex 字符串） |
| `names` | 器件名（SII 字符串） |
| `mailbox_declared` / `coe_details` / `general_flags` / `ebus_current_ma` | SII 邮箱/CoE/标志/电流 |
| `sync_managers[]` | `start` / `length` / `control` / `status` / `enable` |
| `pdo_tx[]` / `pdo_rx[]` | `index` / `sync_manager` / `entries[]`（`index`/`sub`/`bits`） |
| `dc` | 有则 `cycle_time0_ns` 等；无则 `null` |
| `profile` | manifest 画像 `{id,name}`；内置画像 `null` |
| `dict_objects` | 运行时字典对象数；无 manifest `null` |
| `live`（仅实时模式） | `station_address` / `al_status` / `al_status_code` / `dc_time_ns` / `stats{frames_in,frames_out,frames_dropped,datagrams_hit,mailbox_requests,mailbox_responses,dc_reg_reads,dc_reg_writes}` |

### 4.2 `ec-sim/dict-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` / `object_count` / `entry_count` | 元信息 |
| `objects[]` | `index` / `max_sub` / `code` / `name` / `entries[]` |
| `entries[]` | `sub` / `type` / `bits` / `access`（`ro`/`rw`）/ `name` / `value_hex`（原始小端字节） |

### 4.3 `ec-sim/pdo-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` / `station_address` / `al_status` | 元信息 |
| `sm2` / `sm3` | `address` / `bytes` / `hex`（原始小端字节） |
| `decoded[]` | 按映射顺序：`dir`（`rx`/`tx`）/ `index` / `sub` / `bits` / `name` / `value`（有符号类型已符号扩展） |

### 4.4 `ec-sim/pdo-history-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` / `count` | 元信息 |
| `samples[]` | `sm2_hex` / `sm3_hex` / `al_status`（按 PDO 变化去重、每站最多 256 条） |

### 4.5 `ec-sim/topology-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` / `count` | 元信息 |
| `slaves[]` | `index` / `station_address` / `al_status` / `al_status_code` / `frames_in` / `frames_out` / `sm2{address,hex}` / `sm3{address,hex}` |

### 4.6 `/metrics`（Prometheus 文本）

| 指标 | 类型 | 说明 |
| --- | --- | --- |
| `ecsim_frames_in_total` | counter | 收帧数 |
| `ecsim_frames_out_total` | counter | 发帧数 |
| `ecsim_frames_dropped_total` | counter | 丢弃帧数 |
| `ecsim_datagrams_hit_total` | counter | datagram 命中数 |
| `ecsim_mailbox_requests_total` | counter | 邮箱请求数 |
| `ecsim_mailbox_responses_total` | counter | 邮箱响应数 |
| `ecsim_dc_reg_reads_total` | counter | DC 寄存器读次数 |
| `ecsim_dc_reg_writes_total` | counter | DC 寄存器写次数 |
| `ecsim_dc_time_ns` | gauge | 模拟 DC 系统时间（ns，帧驱动） |
| `ecsim_al_status` | gauge | AL 状态（低 4 位） |
| `ecsim_al_status_code` | gauge | AL Status Code |

所有指标带标签 `slave="<index>"`、`station_address="0x...."`。

## 5. 安全边界（务必遵守）

- **只读**：没有任何写路由；不会触发 SDO 写/PDO 写/ESM 迁移。
- **静态文件白名单**：`--web-root` 只映射上面三个路径，不拼接用户路径、无目录遍历风险；
  路径不匹配时回退到 API 路由/404。
- **无鉴权、无 TLS**：`Access-Control-Allow-Origin: *` 仅适合本机/受信网络调试；**不要暴露到公网**。
- 默认示例都绑定 `127.0.0.1`；如需远程访问，应自行加反向代理/鉴权，并注意泄漏面。
- `--serve` 与 `--uds`/`--tap` 同进程时只增加一个 HTTP 线程；实时路径仍由线缆层主循环驱动，HTTP 请求仅加锁读快照（不改变实时语义）。
- 快照是**观测值**：`/state` 的 `live`、`/pdo`、`/metrics` 均为读取时刻的近似值，不构成实时性/一致性保证。

## 6. 与 WebUI 的关系

`web/` 是零依赖只读页面：可载入 `--dump-state` / `--dump-dict --json` 生成的 JSON 文件，也可用 `--web-root web`
同源托管（`http://127.0.0.1:<port>/`），或另起静态服务器后填 API 地址，从本控制面拉取
`/state`、`/dict`、`/pdo`、`/pdo?history=1`、`/topology`、`/metrics`；支持“从站 N”切换、拓扑行点击切换、1s 实时轮询与 PDO Canvas 波形。

## 7. 验证

| 检查 | 方式 |
| --- | --- |
| 路由/JSON schema | `cargo test -p control-plane`（13 个单测） |
| 静态服务 | `scripts/spikes/http-state-probe.sh` |
| 同源托管 web/ | `scripts/spikes/serve-web-probe.sh` |
| 实时状态/PDO/历史/拓扑/逐站/指标 | `live-state-probe.sh`、`pdo-live-probe.sh`、`pdo-history-probe.sh`、`pdo-decode-probe.sh`、`topology-probe.sh`、`slave-select-probe.sh`、`metrics-probe.sh`、`metrics-filter-probe.sh`（Linux/UDS） |
| TAP 实时 | `scripts/spikes/tap-live-state-probe.sh`（root，验证机） |
| 文档与代码路由一致 | `python3 scripts/spikes/control-plane-doc-check.py` |
