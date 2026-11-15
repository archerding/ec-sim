# E2E run `reset-idem-20261115082347-r1`

- 结果：**RESET_OK**
- 创建：2026-11-15T08:23:47+08:00
- commit：`n/a`
- 二进制 sha256：`n/a`
- manifest sha256：`n/a`
- 命令：`cnc-e2e-reset-idempotency.sh round 1 (3x cnc-csp-servo, UDS, port 18137)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r1.json` | 775 | `355cd7fc14605b9c3020d406d598755e4a472c45fe25ac0f512a740e88f7e4ab` |
| `server-r1.log` | 1266 | `357b2d1b04c11cb54717c6fb5904eb5756952a25fdea558161b10f574a1cc2e4` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
