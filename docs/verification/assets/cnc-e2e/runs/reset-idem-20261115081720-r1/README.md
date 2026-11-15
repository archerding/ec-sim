# E2E run `reset-idem-20261115081720-r1`

- 结果：**RESET_OK**
- 创建：2026-11-15T08:17:21+08:00
- commit：`n/a`
- 二进制 sha256：`n/a`
- manifest sha256：`n/a`
- 命令：`cnc-e2e-reset-idempotency.sh round 1 (3x cnc-csp-servo, UDS, port 18137)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r1.json` | 775 | `d89bd55f69ee4feeb3cb4ed4724b3c11c2130cd2a866c0a50c39838f1963d31e` |
| `server-r1.log` | 1266 | `5ea6db965dd4bf950cec039efb4d50aaf2e6888e0afd6b0b4dc9697c07cafd0c` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
