# 第三轮独立复跑证据（2026-11-15，`33bf2fb`）

- 来源：EC-Sim 验证机（openEuler 24.03 SP4，root）`/tmp/audit3/`；由行业专家（sim-expert）独立复跑，未触碰 CNC 验证机。
- 被测 commit：`33bf2fb`（`c47a747` gap 实现、`f016506` 归档保留子目录、`8050f1a`/`33bf2fb` 归档字节稳定）。
- binary sha256 `66587fa0094a94190eb82fb7ec42bd7377a3a0ab61369fc191c1ce7ba2f30c1f`（1,157,928 B）；manifest sha256 `5f96adc131c19f1448b29f8cd30577e20a12e72ed7fd8ae4ffd2d80f3dcd45e9`；部署目录 `/root/ecsim-gap`，脚本已按上述 commit 逐字节核对（审计时 VM 内 `e2e_run_archive.py` 曾为旧版 `1e833592…`，已替换为 commit 版 `8b714d76…`）。
- 复跑命令与结果见 `docs/verification/2026-11-15-cnc-e2e-support-audit.md` §13；关键值：C4 10/10（0 失败，归档 66 文件）、C1 200 样本 p95 20.681ms/std 0.170/`/state` p95 0.658ms/48.94 fps/站/WKC 3、A4 reset 44/43/42ms/REF 6 字段一致/无残留、B-1 32 PASS/0 FAIL/0 SKIP；仓内 5 个 `runs/<run_id>` 归档 verify 5/5 PASS。
- 本目录内容：`c4/`（10 类 `result.json`/`events.jsonl` 与 summary/probe.out）、`jitter/`（`jitter.json`/probe.out）、`reset/`（`ref-compare.json`/timeline）、`b1/`（32 项日志）、`runs/`（5 个归档，含 `run.json`/`sha256sums.txt`）及四份 `*-run.log`。
- 说明：`b1/round-*/uds.sock` 为运行期 Unix socket，scp 不复制非常规文件，故未在本目录；不影响结果判定。`sha256sums.txt` 覆盖本目录 226 个文件（不含 README/sha256sums 本身）。
- 边界：本轮为 EC-Sim 侧自证（UDS 合成主站/TAP），不代表 CNC VM 上 lcec/HMI/LinuxCNC 行为；C4 的 tool/offset/home 是 SIM 可观测等价路径；不宣称 EtherCAT 一致性/CTT/LinuxCNC 认证。
