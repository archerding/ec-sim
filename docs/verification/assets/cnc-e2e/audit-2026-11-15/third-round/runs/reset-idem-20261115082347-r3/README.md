# E2E run `reset-idem-20261115082347-r3`

- 结果：**RESET_OK**
- 创建：2026-11-15T08:23:48+08:00
- commit：`n/a`
- 二进制 sha256：`n/a`
- manifest sha256：`n/a`
- 命令：`cnc-e2e-reset-idempotency.sh round 3 (3x cnc-csp-servo, UDS, port 18137)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r3.json` | 775 | `67ad797d14ba7264ba299ad5e237253de2bc7904f812e5e57f093b120f7828ee` |
| `server-r3.log` | 1266 | `612ac58a66de5c314c27f5776d18d25b0e5661326a0cb6936b0a38e7b4b2afda` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
