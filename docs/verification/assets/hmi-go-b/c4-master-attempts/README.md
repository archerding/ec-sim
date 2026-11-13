# EC-Sim VM 主站 harness 尝试记录（说明性，不作证据）

2026-11-14 task-78：在 EC-Sim VM（内核 6.6.0-159 非 RT、无 IgH）尝试让数据面 `frames_in>0`/AL OP：
- SOEM `simple_ng` 单次会话（`timeout 5m`）：AL 曾全 OP，但 **~1 分钟 `Slave 5 lost` 后退出**（Result=0），非产品 FAIL；
- 自改 `ecsim_master_long.c`（长跑循环 + ESM 恢复）：编译成功（sha256 `ca306c63…`），但运行时 attach 打印适配器列表、`systemd-run` 单元 exit-code；受 SSH 间歇挂起与残留进程影响未能取得可信预检；
- Lead 裁决停止在 EC-Sim VM 折腾主站（A/B 均不做）：**C4 数据面标 NOT-JUDGED（harness/环境限制），由 C5 24h（CNC VM：IgH+lcec+EC-Sim 完整栈）承接**；EC-Sim VM 仅做浏览器侧 4h + DOD-07/08（不要求 OP）。
- 本目录文件仅为尝试说明，**不构成 PASS/FAIL 证据**。