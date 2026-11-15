# E2E run `reset-idem-20261115081720-r1`

- 结果：**PASS**
- 创建：2026-11-15T08:21:29+08:00
- commit：`7a919cf`
- 二进制 sha256：`66587fa0094a94190eb82fb7ec42bd7377a3a0ab61369fc191c1ce7ba2f30c1f`
- manifest sha256：`5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`
- 命令：`cnc-e2e-reset-idempotency.sh round 1 (3x cnc-csp-servo, UDS)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `state-r1.json` | 775 | `d89bd55f69ee4feeb3cb4ed4724b3c11c2130cd2a866c0a50c39838f1963d31e` |
| `server-r1.log` | 1266 | `5ea6db965dd4bf950cec039efb4d50aaf2e6888e0afd6b0b4dc9697c07cafd0c` |
| `e2e_reset_probe.py` | 7741 | `0386aff38c7b220fae958979fb108b3f672c7ab189b0188a68ab46c11df1d470` |
| `uds_master_probe.py` | 44133 | `3aaa05636e8406496c8fe9ebed14b46b8936bacfdd035848455b87f7b302998c` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
