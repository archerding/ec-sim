# `/fault` 门禁与审计独立测试（2026-11-15）

被测：EC-Sim `30ce3e9` 二进制（sha256 `912564b4…`）。EC-Sim VM 自建 server，端口 18136/18137。

## 1. 未启用 `--serve-write`

```bash
ec-sim-esc --uds /tmp/audit-gate.sock --slaves 3 --manifest plugins/cnc-csp-servo/plugin.yaml \
  --behavior cia402 --serve 127.0.0.1:18136 --run-id audit-gate
curl -s -o g1.out -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -d '{"fault":"slave_drop","target":0,"duration_ms":300,"confirm":true}' \
  http://127.0.0.1:18136/fault
```

结果：`405`，body：`{"error": "写操作未启用（需要 --serve-write）"}`（原始 `g1.out`）。

## 2. 启用 `--serve-write` 但缺少 per-request 确认

```bash
ec-sim-esc ... --serve 127.0.0.1:18137 --serve-write --run-id audit-gate2
curl ... -d '{"fault":"slave_drop","target":0,"duration_ms":300}' .../fault
```

结果：`400`，body：`{"error": "故障注入需要 per-request 二次确认（confirm:true）"}`（`g2.out`）。

## 3. `confirm:true` + run_id

```bash
curl ... -d '{"fault":"slave_drop","target":0,"duration_ms":800,"confirm":true,"run_id":"audit-gate2"}' .../fault
```

结果：`200`，body 含 `ok:true`、`run_id":"audit-gate2"`、`fault":"slave_drop"`、`target":0`、
`duration_ms":800`、`applied_count":1`（`g3.out`）。

## 4. `/fault/status` 与 `/events`

- `/fault/status`：`{"schema":"ec-sim/fault-v1","run_id":"audit-gate2","applied_count":1,
  "active":[{"fault":"slave_drop","target":0,"remaining_ms":592,...}]}`
- `/events?since=0`：`fault_injected` details =
  `{"fault":"slave_drop","target":0,"duration_ms":800,"modeled":true,
  "before":{"al_status":"0x01","al_status_code":"0x0000"},
  "after":{"al_status":"0x01","al_status_code":"0x0000"},
  "confirm":true,"who":"http","run_id":"audit-gate2"}`

## 5. 观测口径说明

- 无主站周期帧时，本次 800 ms 故障在 1 s 后 `/fault/status` 仍列 `active` 且 `remaining_ms=0`：
  到期清理依赖帧/状态推进；故障矩阵在 keepalive 流量下 `wait_fault_cleared` 5 s 内清空并通过。
  该行为不影响本轮 B1/B5 判定，但应在产品文档中写明“到期清理需数据面推进”。
