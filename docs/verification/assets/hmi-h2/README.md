# H2 headless 采集资产（DOD-07/DOD-08 参考证据）

> 采集人：sim-impl（2026-11-13）；命令脚本：`scripts/spikes/hmi-browser-probe.mjs`（headless Edge + CDP，无第三方依赖）。

## 方法与边界

- 被测页面：EC-Sim `web/`（静态模式：把 `--dump-state` / `--dump-dict --json` 写入页目录并由
  `python -m http.server` 同源托管；无 live/UDS）。
- 浏览器：Microsoft Edge **157.0.4341.46**，`--headless=new`，CDP `Emulation.setDeviceMetricsOverride`。
- 视口/缩放：1024×768、1280×900、1920×1080、1024×768@125%（deviceScaleFactor=1.25，近似 125% 缩放）。
- 每个视口等待 2.5 s 后截图并用 `Runtime.evaluate` 采样：可见交互元素 `getBoundingClientRect`、
  横向溢出（`scrollWidth - innerWidth`）、Paint Timing、Navigation Timing、资源耗时。
- **⚠️ 边界**：headless 与真实浏览器存在字体/合成/节流差异；本目录只作**布局/触控/计时的参考证据**，
  **不等于** sim-ux 真人走查 PASS；8 站实时（TAP+UDS）性能与真实交互延迟仍需 VM/浏览器窗口。
- 环境：本机 Windows，未占用 EC-Sim VM、未动 `vm-ecsim.lock`、未 reboot。

## 结果摘要（详见 `metrics.json`）

| 视口 | 渲染 section | 可见交互元素 | <44px | 横向溢出 | FCP（P50 单次） |
| --- | --- | --- | --- | --- | --- |
| 1024×768 | 7 | 43 | **0** | 无（-15 = 滚动条） | 112 ms |
| 1280×900 | 7 | 43 | **0** | 无 | 32 ms |
| 1920×1080 | 7 | 43 | **0** | 无 | 56 ms |
| 1024×768@125 | 7 | 43 | **0** | 无 | 44 ms |

- 截图：`1024x768.png`、`1280x900.png`、`1920x1080.png`、`1024x768@125.png`。
- 首屏 paint 均在 1 s 预算内（静态模式、本机；**不是** 8 站实时结论）。

## 未覆盖（不得记 PASS）

- 7 个视图的逐视图截图/抽屉、1024 下导航折叠的人工确认；
- 8 站实时环境（`--slaves 8` + TAP/UDS）的首屏/交互/Timer 指标；
- 真实浏览器（非 headless）的视觉走查与对比度取色。

## DOD-09 i18n 截图（en 模式）

- `en-1024x768.png`、`en-1280x900.png`、`en-1920x1080.png`、`en-1024x768@125.png` + `en-metrics.json`：
  通过 `?lang=en` 静态覆盖（不写 localStorage）在 headless Edge 下采集；为 batch 1（静态外壳 i18n）证据。
- 结果：四视口渲染正常、无横向溢出、可见交互仍全 ≥44px；动态文案（横幅/表头/字段/错误提示）i18n 迁移在
  batch 2 完成后再补最终 en 截图。
- 边界：headless 参考证据，不替代 sim-ux 真人走查。
