# E2E run `reset-idem-20261115081720-r2`

- 结果：**RESET_OK**
- 创建：2026-11-15T08:17:22+08:00
- commit：`n/a`
- 二进制 sha256：`n/a`
- manifest sha256：`n/a`
- 命令：`cnc-e2e-reset-idempotency.sh round 2 (3x cnc-csp-servo, UDS, port 18137)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r2.json` | 775 | `21b741d78b75c0ca9de72bcd6454cc2e7a9c73f5116b0ed449fbd640b35de620` |
| `server-r2.log` | 1266 | `5c4f359239e262e2cd3f36f0b8d5079cb17143a82c400503249f0757fac51347` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
