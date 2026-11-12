# plugins/ — 设备与协议插件

> 规则与流程见 **[docs/DEVICE-PLUGINS.md](../docs/DEVICE-PLUGINS.md)**。开始前必须读完，尤其 R1–R5 红线。

## 目录布局

```
plugins/
├── _template/           # 新插件从这里复制
│   └── plugin.yaml
├── catalog.yaml         # 设备研究清单（planned / researching / ready / blocked）
├── <plugin-id>/         # 每个插件一个目录
│   ├── plugin.yaml      # manifest（入库）
│   ├── facts.md         # 从公开文档提取的接口事实（入库，仅数据+来源链接）
│   ├── README.md        # 已模拟 / 未模拟 / 免责声明（入库）
│   └── code/            # 行为插件（闭源插件不入库；仅入库接口与测试）
└── README.md
```

## 两类插件

| 类型 | 是否需要写代码 | 典型例子 | 商业形态 |
|---|---|---|---|
| **声明式设备插件** | 不需要（填 manifest + facts） | 8DI/8DO 端子、编码器端子、简单网关 | 可免费，可低价付费 |
| **行为插件** | 需要（WASM 状态机） | CiA402 伺服、步进驱动、厂商私有协议 | 付费主力 |

## 提示

- 先做**一个能跑通闭环的样品**（建议 Beckhoff EL2008 兼容 8DO），再做伺服；
- 每个插件都要能在 IgH 与 SOEM 上被扫描到（这是本项目的验收线）；
- 命名为 `com.ecsim.compat.<brand>-<model>`（自有域名前缀，不用厂商域名）。
