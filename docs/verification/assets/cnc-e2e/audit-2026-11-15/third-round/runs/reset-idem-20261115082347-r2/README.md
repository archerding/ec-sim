# E2E run `reset-idem-20261115082347-r2`

- 结果：**RESET_OK**
- 创建：2026-11-15T08:23:48+08:00
- commit：`n/a`
- 二进制 sha256：`n/a`
- manifest sha256：`n/a`
- 命令：`cnc-e2e-reset-idempotency.sh round 2 (3x cnc-csp-servo, UDS, port 18137)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r2.json` | 775 | `72d2dcf351025abe2860893081f518bd36aef16fa3f294d068eb22ed5f978516` |
| `server-r2.log` | 1266 | `65fd48c462bafc303ffb45de1898be2f1a91d5bfb233d3ec1a7f50b61f1fbd77` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
