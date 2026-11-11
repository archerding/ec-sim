# WORKLOG — EC-Sim 工作日志

> 约定：**最新条目在最上面**。每条含 `日期 / 做了什么 / 验证结果 / 卡点 / 下次开工第一件事`。
> 夜班 agent 每次唤醒先读这里的第一条。

---

## 2026-11-12（人工会话）— 文件策略放开，迁移收口，夜班恢复

**做了什么**
- 会话文件策略改为 `danger-full-access`：**D 盘可直接读写，无需逐次审批**（此前阻塞迁移的根因消失）。
- 定时任务「低峰期自动开工推进 EC-Sim」**恢复 active**，并改写 prompt：第一步强制 `Set-Location 'D:\tokengence.ai\ec-sim'`，明令禁止在旧工作区提交。
- 仍保留旧工作区副本（并行会话的 `<other-project>/` 有 2395 个文件在活跃写入，`<legacy-workspace>\.git` 被其共用）。

**验证结果**
- D 盘仓库写入成功；`git status` 干净；服务器 `root@<LINUX-HOST>` 连通、内核 6.6 正常。

**卡点**
- 无（迁移阻塞已解除）。唯一残留：旧副本尚未退役，待 `<other-project>` 会话结束后再处理。

**下次开工第一件事**
- **B1 Rust workspace 骨架**（或若想先出证据：**B2b 最小 ESC 应答器**，让 IgH/SOEM 看到 1 个从站）。

---
## 2026-11-12（人工会话）— 仓库迁移到 D 盘

**做了什么**
- 按用户要求把 EC-Sim 仓库迁到 `D:\tokengence.ai\ec-sim`：以 `git clone` 方式复制，3 个提交与 `DeepSeek Harness` 作者身份完整保留，远端配置已移除。
- 旧工作区 `<legacy-workspace>` 保留为历史副本（**并行会话的 `<other-project>/` 项目仍在其中活跃写入，未触碰**）。

**验证结果**
- 新仓库 `git log` 三提交一致；`git status` 干净；身份 `DeepSeek Harness <deepseek-harness@users.noreply.github.com>`。

**卡点**
- 本会话（以及定时任务创建的会话）**沙箱工作区根仍是 C 盘**，无法直接写 D 盘（每次写需审批）。夜班任务已暂停，待以 `D:\tokengence.ai\ec-sim` 为工作目录新建会话后重建。

**下次开工第一件事**
- 在新工作区会话里恢复定时任务；代码任务仍从 **B1（Rust workspace 骨架）** 或 **B2b（最小 ESC 应答器）** 开始。

---
## 2026-11-12（人工会话，非自动）— 服务器与线缆层验收到位

**做了什么**
- 接入专用 Linux 验证机 `root@<LINUX-HOST>`（Linux / kernel 6.x / 8 核 15G），登记见 [docs/LINUX-SERVER.md](docs/LINUX-SERVER.md)。
- 装 Rust 1.99.1、`kernel-devel`（与运行内核精确匹配，**无需升级内核/重启**）、gcc/cmake/autotools。
- 停用并禁用 <removed-workload>（现场快照留档）；原 podman 容器已停、数据卷保留；podman 保留备用。
- **Spike 1**（TAP 双向承载 0x88A4）→ PASS，证据 [docs/spikes/spike-1-tap-bidirectional.md](docs/spikes/spike-1-tap-bidirectional.md)。
- **Spike 2**（未修改的 SOEM / IgH 在 TAP 上）→ **两者都成立**：SOEM `ecx_init on tapsoem0 succeeded`；IgH `ec_generic` 绑定 TAP、`/dev/EtherCAT0` 出现、Tx 496 帧/秒，模拟器侧收到 BRD 帧。证据 [docs/spikes/spike-2-masters-on-tap.md](docs/spikes/spike-2-masters-on-tap.md)。
- 新增验证脚本：`scripts/spikes/tap_ethernet_loopback.py`、`tap_hold.py`、`sniff_ecat.py`。

**验证结果**
- 线缆层最大技术风险（TAP 能否被真实主站使用）**已关闭**，且双主站均"零修改"。
- IgH 1.6.13 与 SOEM 均已在服务器构建完成并通过绑定验证。

**卡点**
- 尚无从站应答，IgH 侧每帧 timeout（预期）。下一步要让主站看到 1 个从站（B2b）。
- GitHub 不可达 → IgH 走 gitlab、SOEM 走 gitee 镜像/ghproxy（已写入服务器文档）。

**下次开工第一件事**
- **B1：Rust workspace 骨架**；若夜班想先拿"主站看到 1 个从站"的证据，可先做 **B2b 最小 ESC 应答器**（Python 原型即可）。

---

## 2026-11-12（人工会话，非自动）

**做了什么**
- 完成技术选型讨论，产出 `ethercat-slave-simulator-tech-selection.md`（含 5 层拆分、线缆层三方案、四层插件模型、许可组合、路线图、8 条风险、6 个待拍板决策、附录事实核查）。
- 核实了 4 条一手事实（避免沿用网上错信息）：
  - SOEM 2.x 是 **GPLv3 或商业双许可**（非网传 "GPLv2+链接例外"）。
  - IgH 用户态库 `ecrt.h` 是 **LGPL-2.1**；内核模块 GPLv2-only。
  - IgH 1.6 官方 `libfakeethercat` 只到 **PDO 级共享内存**，官方原文承认"bus error cannot be simulated"。
  - ETG 会员**免费、无分级**；Vendor ID 免费；MainDevice license 免费但须签；ETG FAQ 明确 "EtherCAT technology itself is not Open Source"，纯软件从站仿真的地位需书面澄清。
- 建立仓库：`.gitignore`、`AGENTS.md`（提交身份/许可红线/夜班协议/backlog）、本文件。

**验证结果**
- 仓库已初始化，提交身份固定为 `DeepSeek Harness <deepseek-harness@users.noreply.github.com>`。
- 尚无代码，无测试可跑。

**卡点**
- 待用户拍板 D1–D6（见 tech-selection §11）。其中 **D6（是否联系 KickCAT 上游）** 与 **D3（Windows 是否一级支持）** 会改变 backlog 顺序。

**下次开工第一件事**
- **B1：Rust workspace 骨架**（若用户尚未拍板 D1–D6，按文档推荐值默认执行，并在提交信息里注明"按推荐默认值"）。

**自动化**
- 已创建每日定时任务「低峰期自动开工推进 EC-Sim」：**每天 01:00（Asia/Shanghai）** 触发，投递到固定「夜班」会话，按 `AGENTS.md` §5 协议执行；首次触发 2026-11-13 01:00。
