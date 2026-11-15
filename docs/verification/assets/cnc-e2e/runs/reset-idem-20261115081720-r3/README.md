# E2E run `reset-idem-20261115081720-r3`

- 结果：**RESET_OK**
- 创建：2026-11-15T08:17:22+08:00
- commit：`n/a`
- 二进制 sha256：`n/a`
- manifest sha256：`n/a`
- 命令：`cnc-e2e-reset-idempotency.sh round 3 (3x cnc-csp-servo, UDS, port 18137)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r3.json` | 775 | `b991e686686912998bed188eab15cb9d27c57136cb348277449e2f5316f63064` |
| `server-r3.log` | 1266 | `98b98c0c393c3fb901b9265c7007a7703ba649abee8c7de5856cc01ad6ce3f9e` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
