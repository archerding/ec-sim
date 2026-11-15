# E2E run `jitter-20261115081722`

- 结果：**PASS**
- 创建：2026-11-15T08:21:29+08:00
- commit：`7a919cf`
- 二进制 sha256：`66587fa0094a94190eb82fb7ec42bd7377a3a0ab61369fc191c1ce7ba2f30c1f`
- manifest sha256：`5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`
- 命令：`cnc-e2e-jitter-probe.sh (3x cnc-csp-servo, UDS, samples=200)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `jitter/jitter.json` | 1452 | `1d75ad647d7f7fd5e81109a4b4c62763b7ad0fe11d01ee14e2041080fc80df60` |
| `jitter/probe.out` | 144 | `18aa142c4d6f0b6242f95715d34204377802c74329b3180f1f96a14565124962` |
| `jitter/server.log` | 1260 | `50194cee6989ef40e21ff01ad09b77622dbf86a705ae06ce7b52a2939cdbf6d4` |
| `jitter/timeline.log` | 2212 | `6a815b57b6bb51bec1ba460ecabc64ea1fc46f77991109eb78ba22b104c21c49` |
| `c1_jitter_probe.py` | 8832 | `d40c438015641195ceb23a4bbed1952c8861750b74ed7d4ad3221d74debfe327` |
| `uds_master_probe.py` | 44133 | `3aaa05636e8406496c8fe9ebed14b46b8936bacfdd035848455b87f7b302998c` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
