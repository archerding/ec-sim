# 控制面只读 API（`ec-sim-esc --serve`）

> 状态：M2 观测面 + M3 写路径。控制面**默认只读**（状态/快照/指标）；`--serve-write` 显式开启
> `POST /pdo`、`POST /sdo` 受控写（§5），`--serve-token-env` 可选静态 Bearer token（§6）。
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

# 可选静态 token：变量非空时 API 需要 Authorization: Bearer <token>（空/未设置 = 现状）。
EC_SIM_TOKEN='<token>' target/release/ec-sim-esc --uds /tmp/ecsim.sock --esm-lax \
  --serve 127.0.0.1:8091 --serve-token-env EC_SIM_TOKEN
```

- 绑定地址由用户显式给出（建议 `127.0.0.1`）；服务不监听其他地址、不做 TLS。
  鉴权默认关闭；需要时用 `--serve-token-env <环境变量名>` 开启（§6）。
- `--web-root <dir>`：同源托管 `web/` 白名单文件（`/` 与 `/index.html` → `index.html`，`/app.js` → `app.js`），
  UI 与 API 同端口，无需另起静态服务器；未给出或目录不存在时回退为纯 API（`/` 返回自述 JSON）。
- 读路由只接受 `GET`；写路由只接受 `POST` 且需 `--serve-write`（静态模式非 GET → 405，见 §5）。
- 所有路由均带 `Access-Control-Allow-Origin: *` 与 `Cache-Control: no-store`，便于本机 WebUI 直接 `fetch`。
- **验收/调试注入开关**（不影响控制面路由，默认关闭）：
  - `--fault <hex>`（需 `--behavior cia402`）：注入 **CiA402 Fault**（statusword bit3、`0x603F`=码）；
  - `--al-error <hex>`（task-34 / AC-E-203）：启动即置 **AL Status error 位 + AL Status Code**（写
    `0x0130`/`0x0134`），**不改变** AL 状态、可由主站 Error ACK 清除；只接受已知码
    （`0x0011/0x0012/0x0013/0x0015/0x0016/0x0017/0x0018/0x0019/0x001B/0x0024/0x0025/0x002B`），
    未知码/0 **拒启动（exit 2）**。观测面：`/state.live.al_status`（如 `0x11` = INIT|error）与
    `al_status_code`；`/metrics` 的 `ecsim_al_status`/`ecsim_al_status_code`。探针：
    `scripts/spikes/al-error-probe.sh`（Linux/UDS；本机仅跑参数校验）。

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
| `GET /dict` | 全部 | `ec-sim/dict-v1` | 运行时对象字典（无 manifest 时 404）；支持 `?slave=N`（校验站号，字典当前对同画像各站相同） |
| `GET /pdo` | 实时 | `ec-sim/pdo-v1` | SM2/SM3 原始字节 + `decoded` 具名解码 |
| `GET /sdo?index=0x6040&sub=0` | 全部 | `ec-sim/sdo-v1` | 运行时对象字典只读浏览（不经过 mailbox；未知对象 404）；支持 `?slave=N` |
| `POST /sdo?index=0x6040&sub=0` | 实时（需 `--serve-write`） | （内联 JSON） | 受控写对象值（白名单/长度/审计；默认 405、非白名单 403、未知 404、长度错 400）；支持 `?slave=N` |
| `GET /sdo-audit` | 实时 | `ec-sim/sdo-audit-v1` | 最近 64 条 SDO 写审计事件；`?slave=N` 按站过滤（缺省 = 站 0） |
| `GET /pdo?history=1` | 实时 | `ec-sim/pdo-history-v1` | 按变化去重的 PDO 时间序列（每站 256 条） |
| `GET /topology` | 实时 | `ec-sim/topology-v1` | 链上所有从站摘要（**全局路由**，不接受 `?slave`） |
| `GET /metrics` | 实时 | Prometheus 文本 | 每站计数器/状态，`text/plain; version=0.0.4` |
| `GET /state?slave=N` | 实时 | `ec-sim/state-v1` | 指定从站（默认 0；越界/非法 → 404） |
| `GET /pdo?slave=N`（可叠加 `history=1`） | 实时 | 同上 | 指定从站的 PDO/历史（各站独立缓冲） |
| `GET /metrics?slave=N` | 实时 | Prometheus 文本 | 仅导出指定从站（默认全量） |
| `GET /events` | 实时 | `text/event-stream` | SSE 推送 state-v1（默认 500ms，可 `?interval_ms=100..5000`；支持 `?slave=N`） |
| `POST /pdo` | 实时（需 `--serve-write`） | （内联 JSON） | 写 SM2 原始字节（调试用；默认 405，见 §5）；支持 `?slave=N` |
| 其他路径 | 全部 | `{"error": ..., "routes": [...]}` | 404 |

### 2.1 站号语义（`?slave=`，ACC-27②）

- **0-based**：`slave=0` 是链上第一个从站，合法范围 `0..=N-1`（`N` = `/topology` 的 `count`）。
  缺省 = 站 0（保持既有默认行为）。
- **统一校验，不静默回退**：按站路由（`/state`、`/pdo`、`/metrics`、`/events`、`/sdo`、`/dict`、
  `/sdo-audit`，含 `POST /sdo`、`POST /pdo`）对 非数字（`abc`）、负数（`-1`）、空串、越界（`>= N`）
  一律返回 **HTTP 404 + `application/json`**（`{"error": ...}`）；**不会**回退到站 0 或返回他站数据。
- **静态模式**（无 `--uds`/`--tap`）只有站 0 的快照：`slave=0`/缺省 → 200；其它 → 404。
- **全局路由**（`/topology`、`/`、`/health`）不接受 `?slave`：带了 → 404（不静默忽略）。
- 历史背景（P1 缺陷）：修复前 `/sdo`、`/dict` 对非法/越界 slave 返回 200 + 站 0 数据，
  静态 `/sdo` 直接忽略参数 → 诊断可能被误导；修复见 `scripts/spikes/invalid-slave-probe.sh`
  （静态/UDS/TAP 三路径矩阵）与 `control-plane::validate_slave_param` 单测。

> 开启 token（§6）后，上表除 `/`、`/index.html`、`/app.js`、`/health` 外的**全部 API 路由**
> （含 `POST /pdo`、`POST /sdo`）都要求 `Authorization: Bearer <token>`；静态资源不鉴权，
> 便于浏览器先加载 WebUI 外壳。

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

### 4.5 `ec-sim/sdo-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` | 元信息 |
| `index` / `sub` / `object_name` / `name` | 对象索引、子索引、对象名、条目名 |
| `type` / `bits` / `access` | 数据类型、位长、`ro`/`rw` |
| `value_hex` | 当前值（原始小端字节） |

> 只读、来自运行时对象字典（`/dict` 的内存视图），**不发起** CoE SDO 邮箱往返；写路径见 M3 范围。

### 4.6 `ec-sim/sdo-audit-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` / `count` | 元信息（count ≤ 64） |
| `events[]` | `ts_ms` / `slave` / `index` / `sub` / `old_hex` / `new_hex` |

### 4.7 `ec-sim/topology-v1`

| 字段 | 说明 |
| --- | --- |
| `schema` / `tool` / `count` | 元信息 |
| `slaves[]` | `index` / `station_address` / `al_status` / `al_status_code` / `frames_in` / `frames_out` / `sm2{address,hex}` / `sm3{address,hex}` |

### 4.8 `/metrics`（Prometheus 文本）

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

## 5. 写操作（默认关闭）

- 只有显式加 **`--serve-write`** 才启用 `POST /pdo` 与 `POST /sdo`（其余 POST 返回 404；未启用时返回 405 JSON）。
- `POST /sdo` 额外规则：只允许 manifest 声明 `writable: true` 的对象；身份/配置类对象
  （`0x1000/0x1008/0x1009/0x1010/0x1011/0x1018/0x1C12/0x1C13`）一律 403；值长度必须等于对象位长，
  否则 400；每次成功写入追加到 `/sdo-audit`（≤64 条）并使 `/metrics` 的
  `ecsim_sdo_writes_total` 递增。
- 请求体 = 原始 SM2 字节（长度截断到 SM2 映射长度，≤1024 字节），`?slave=N` 选择从站；
  服务端 `write_process_data` 只写对应 DL 用户内存，**不改变** ESM/SDO/邮箱语义。
- 响应：`{"ok": true, "slave": N, "sm2_address": "0x1100", "written_hex": "beef"}`。
- 用途：自动化测试/调试；默认关闭，生产/共享环境不要开启。写操作仍**无鉴权**，只应在回环或受信环境使用。

## 6. 鉴权（可选静态 token，默认关闭）

- 启动参数 **`--serve-token-env <VAR>`**（可与 `--serve-write` 叠加）：从环境变量 `VAR` 读取 token；
  变量**未设置或为空** = 不鉴权（维持现状，启动时在 stderr 提示）。
- 变量非空时：受保护路由需要 `Authorization: Bearer <token>`；头名与 `Bearer` scheme 大小写不敏感，
  token 本身**精确匹配**。缺失/非 Bearer → `401`（响应带 `WWW-Authenticate: Bearer realm="ec-sim"`）；
  token 错误 → `403`。静态资源与 `/health` 不鉴权。
- 受保护路由：`/state`、`/dict`、`/pdo`、`/sdo`、`/sdo-audit`、`/topology`、`/metrics`、`/events`，
  含 `POST /sdo`、`POST /pdo` 写路由与全部 `?slave=N`/`?history=1` 变体。
- 这是**静态 token**，不是完整认证：无用户体系/轮换/登录审计，token 比较非恒定时间；
  仅适合本机/受信网络。公网访问仍应经反向代理做 TLS 与认证（ADR 0002 D4）。
- ⚠️ 开启 token 后浏览器 WebUI 的 `fetch` 无法自带 `Authorization` 头：需经反向代理注入请求头，
  或直接用 `curl -H "Authorization: Bearer <token>"`；静态外壳可正常加载。

## 7. 安全边界（务必遵守）

- **只读**：没有任何写路由；不会触发 SDO 写/PDO 写/ESM 迁移。
- **静态文件白名单**：`--web-root` 只映射上面三个路径，不拼接用户路径、无目录遍历风险；
  路径不匹配时回退到 API 路由/404。
- **默认无鉴权、无 TLS**：`Access-Control-Allow-Origin: *` 仅适合本机/受信网络调试；
  需要时用 `--serve-token-env`（§6）加静态 token，但**不要暴露到公网**。
- 默认示例都绑定 `127.0.0.1`；如需远程访问，应自行加反向代理/鉴权，并注意泄漏面。
- `--serve` 与 `--uds`/`--tap` 同进程时只增加一个 HTTP 线程；实时路径仍由线缆层主循环驱动，HTTP 请求仅加锁读快照（不改变实时语义）。
- 快照是**观测值**：`/state` 的 `live`、`/pdo`、`/metrics` 均为读取时刻的近似值，不构成实时性/一致性保证。
- 写操作（`POST /pdo`、`POST /sdo`）默认关闭；启用 `--serve-write` 后仍无鉴权，除非同时开启 token（§6）。

## 8. 与 WebUI 的关系

`web/` 是零依赖页面：可载入 `--dump-state` / `--dump-dict --json` 生成的 JSON 文件，也可用 `--web-root web`
同源托管（`http://127.0.0.1:<port>/`），或另起静态服务器后填 API 地址，从本控制面拉取
`/state`、`/dict`、`/pdo`、`/pdo?history=1`、`/topology`、`/metrics`；支持“从站 N”切换、拓扑行点击切换、1s 实时轮询与 PDO Canvas 波形。

**SDO 面板（B42 / M3-W6）**：页内可按 `index`/`sub` 调用 `GET /sdo` 只读浏览（对象名/类型/位长/访问/值 hex）；
写入默认关闭——需先勾选“允许写入”，并在 `confirm()` **二次确认**后才发 `POST /sdo`，服务端仍以
`--serve-write`、manifest 白名单与安全排除表把关（405/403/404/400）。页面无法探测写开关，以服务端响应为准。
token 模式下 `fetch` 不能自动带 `Authorization`，页面提示经反向代理注入或改用 `curl`（§6）。

## 9. 验证

| 检查 | 方式 |
| --- | --- |
| 路由/JSON schema/鉴权判定 | `cargo test -p control-plane`（22 个单测，含 `validate_slave_param` 合法/非法矩阵） |
| 站号校验（ACC-27②，静态/UDS/TAP 三路径） | `scripts/spikes/invalid-slave-probe.sh` |
| AL Status 错误注入（`--al-error`，task-34/AC-E-203） | `scripts/spikes/al-error-probe.sh`（Linux/UDS；本机仅参数校验段） |
| 静态服务 | `scripts/spikes/http-state-probe.sh` |
| 同源托管 web/ | `scripts/spikes/serve-web-probe.sh` |
| SSE 推送 | `scripts/spikes/events-probe.sh` |
| SDO 只读浏览 | `scripts/spikes/sdo-read-probe.sh` |
| SDO 受控写/审计 | `scripts/spikes/sdo-write-probe.sh` |
| 可选 token 鉴权矩阵 | `scripts/spikes/serve-token-probe.sh` |
| Web SDO 面板（静态资源回归，无 DOM 自动化） | `scripts/spikes/web-sdo-panel-check.py` |
| 写操作（默认关闭） | `scripts/spikes/write-pdo-probe.sh` |
| 实时状态/PDO/历史/拓扑/逐站/指标 | `live-state-probe.sh`、`pdo-live-probe.sh`、`pdo-history-probe.sh`、`pdo-decode-probe.sh`、`topology-probe.sh`、`slave-select-probe.sh`、`metrics-probe.sh`、`metrics-filter-probe.sh`（Linux/UDS） |
| TAP 实时 | `scripts/spikes/tap-live-state-probe.sh`（root，验证机） |
| 文档与代码路由一致 | `python3 scripts/spikes/control-plane-doc-check.py` |
| M2 退出条件 | [plan/M2-exit-checklist.md](plan/M2-exit-checklist.md) |
