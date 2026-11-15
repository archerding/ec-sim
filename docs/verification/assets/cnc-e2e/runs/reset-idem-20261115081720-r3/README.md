# E2E run `reset-idem-20261115081720-r3`

- 结果：**PASS**
- 创建：2026-11-15T08:21:30+08:00
- commit：`7a919cf`
- 二进制 sha256：`66587fa0094a94190eb82fb7ec42bd7377a3a0ab61369fc191c1ce7ba2f30c1f`
- manifest sha256：`5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`
- 命令：`cnc-e2e-reset-idempotency.sh round 3 (3x cnc-csp-servo, UDS)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r3.json` | 775 | `b991e686686912998bed188eab15cb9d27c57136cb348277449e2f5316f63064` |
| `server-r3.log` | 1266 | `98b98c0c393c3fb901b9265c7007a7703ba649abee8c7de5856cc01ad6ce3f9e` |
| `e2e_reset_probe.py` | 7741 | `0386aff38c7b220fae958979fb108b3f672c7ab189b0188a68ab46c11df1d470` |
| `uds_master_probe.py` | 44133 | `3aaa05636e8406496c8fe9ebed14b46b8936bacfdd035848455b87f7b302998c` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
