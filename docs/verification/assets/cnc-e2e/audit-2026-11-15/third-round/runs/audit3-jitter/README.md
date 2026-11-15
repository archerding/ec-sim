# E2E run `audit3-jitter`

- 结果：**PASS**
- 创建：2026-11-15T08:23:56+08:00
- commit：`unknown`
- 二进制 sha256：`66587fa0094a94190eb82fb7ec42bd7377a3a0ab61369fc191c1ce7ba2f30c1f`
- manifest sha256：`5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`
- 命令：`cnc-e2e-jitter-probe.sh (3x cnc-csp-servo, UDS, port 18138)`

## 文件

| 文件 | 字节 | sha256 |
| --- | --- | --- |
| `jitter/jitter.json` | 1434 | `af193fd09d936e66a722bf3408742cfc22f232adfa47dc9d333e5f74a2401cb8` |
| `jitter/probe.out` | 144 | `edd90c59404455534e16b277a261a00f8a7739fef2e9a05f81db9ad7d67b7367` |
| `jitter/server.log` | 1252 | `49e9832cbfe3e9aa8d2bfd3adf6f3e441ad8a9b23e82f030f2d6bbda5ecde9d3` |
| `jitter/timeline.log` | 318 | `b74e01a91bf1986a61e28a02db50b595224aeea407f1f018657a4620d2eb0802` |
| `c1_jitter_probe.py` | 8832 | `d40c438015641195ceb23a4bbed1952c8861750b74ed7d4ad3221d74debfe327` |
| `uds_master_probe.py` | 44133 | `3aaa05636e8406496c8fe9ebed14b46b8936bacfdd035848455b87f7b302998c` |

## 边界

- 本目录只含 SIM 侧证据；CNC 侧功能/HMI 证据不在本归档内。
- 结论以 run.json.result 为准；PENDING/INCOMPLETE 不得当 PASS。
- `sha256sums.txt` 可用于独立校验。
