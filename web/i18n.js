// H2/DOD-09：EC-Sim Web 轻量 i18n（zh/en，无第三方依赖）。
// 约定：
//  - 页面静态文案用 `data-i18n="key"`（元素内保留中文 fallback 文本，保证旧断言/无 JS 时可用）；
//  - 动态渲染文案用 `window.ECSIM_I18N.t("key")`；
//  - testid/id 契约不随语言变化；切换只改文本与 <html lang>。
//  - 语言存 localStorage（ecsim-lang），默认 zh。
(function () {
  "use strict";
  const STORAGE_KEY = "ecsim-lang";
  const dict = {
    zh: {
      "app.title": "EC-Sim 状态视图",
      "nav.overview": "总览",
      "nav.slave": "从站",
      "nav.pdo": "PDO",
      "nav.dict": "字典",
      "nav.sdo": "SDO",
      "nav.evidence": "证据",
      "nav.about": "关于",
      "topbar.reconnect": "重新连接",
      "topbar.pause": "暂停",
      "topbar.resume": "继续",
      "topbar.about": "关于 / 边界",
      "topbar.lang": "EN",
      "section.load": "载入状态",
      "section.sdo": "SDO 浏览（只读 API）",
      "section.audit": "SDO 写审计（只读 API：GET /sdo-audit）",
      "section.evidence": "证据导出（只读；S-11 / V-06）",
      "section.about": "关于 / 边界（V-07）",
      "load.api": "API 地址",
      "load.fromApi": "从 API 载入",
      "load.live": "实时刷新",
      "load.slaveHint": "从站在上方“从站”处选择（0-based，0..N-1）；实时还可用 ec-sim-esc --serve 127.0.0.1:8090 --uds <sock> --manifest plugins/…（同进程提供 /pdo、/topology、/metrics、/events）。",
      "conn.idle": "未连接",
      "conn.connecting": "连接中…",
      "conn.liveSse": "LIVE · SSE",
      "conn.livePoll": "LIVE · 轮询",
      "conn.stale": "STALE（数据陈旧）",
      "conn.stopped": "未连接",
      "conn.paused": "已暂停",
      "conn.reconnecting": "重连中…",
      "notice.static": "静态快照模式：当前 state 没有 live 字段（来自 --dump-state 的静态快照或文件载入），不提供实时/离线数据刷新。需要实时视图请用 ec-sim-esc --serve <addr> --uds <sock>（或 --tap）启动同进程实时模式。",
      "banner.livePoll": "E-10：SSE 推送已中断，页面已降级为轮询（LIVE · 轮询）；数据可能落后 1 个刷新周期。恢复后自动回到 LIVE · SSE。",
      "banner.stopped": "E-10：连续 3 次请求失败，已停止自动刷新（未连接）。请确认服务已启动后点“重新连接”；当前显示的是最后一次数据，可能已过期。",
      "banner.stale": "数据陈旧（STALE）：超过 2 个刷新周期没有新数据；以下所有数值都带陈旧标记，请勿当作实时值。",
      "banner.paused": "已暂停：页面停止发请求，显示最后一次数据与时间戳；点“继续”会立即拉取一次。",
      "banner.reconnecting": "正在重连：已发起一次探测请求…",
      "section.overview": "总线总览（health）",
      "section.topology": "总线拓扑",
      "section.slaveIdentity": "身份与画像",
      "section.slaveLive": "实时状态（--serve 实时模式）",
      "section.sm": "Sync Manager",
      "section.rx": "RxPDO（主站 → 从站）",
      "section.tx": "TxPDO（从站 → 主站）",
      "section.dc": "分布式时钟（DC）",
      "section.dict": "对象字典",
      "section.pdoCurrent": "过程数据（PDO，只读快照）",
      "section.pdoDecoded": "PDO 具名解码（按映射顺序）",
      "section.pdoHistory": "PDO 历史",
      "section.metrics": "指标（/metrics）",
      "section.evidence": "证据导出（只读；S-11 / V-06）",
      "section.about": "关于 / 边界（V-07）",      "intro.dataSource": "数据源：ec-sim-esc --dump-state 生成的 JSON（状态/PDO/拓扑/指标只读，SDO 面板只读浏览，写入需显式开启且二次确认）。可直接把 JSON 文件拖入下方，或执行 ec-sim-esc --dump-state --manifest plugins/… > web/state.json 后用 python3 -m http.server 8080 --directory web 打开页面（页面会自动读取同目录 state.json）。",
      "ctx.slave": "从站：",
      "ctx.address": "站地址：",
      "ctx.al": "AL：",
      "ctx.profile": "画像：",
      "ctx.copy": "复制摘要",
      "ctx.export": "导出 JSON",
      "sdo.intro": "经 GET /sdo?index&sub&slave 从运行时对象字典读取（不经过 mailbox/CoE 往返）。需先在上方填写 API 地址（--serve 的地址）并载入；请求会带当前站号（B55 起后端校验 0-based）。",
      "sdo.index": "索引（hex，可省略 0x）",
      "sdo.sub": "子索引",
      "sdo.read": "读取",
      "sdo.writeEnable": "允许写入（需服务端 --serve-write，写入前二次确认）",
      "sdo.newValue": "新值（小端 hex，如 0f00）",
      "sdo.write": "写入…",
      "sdo.boundary": "写操作默认关闭：仅当服务端以 --serve-write 启动、且对象在 manifest 中声明 writable 时才会成功；身份/配置类对象（0x1000/0x1008/0x1009/0x1010/0x1011/0x1018/0x1C12/0x1C13）与长度不匹配会被服务端拒绝。本页无法探测写开关，最终以服务端响应为准；确认框显示对象名/位长/旧值/新值/站号。",
      "sdo.token": "token 模式：若服务端以 --serve-token-env 开启鉴权，浏览器 fetch 无法自动携带 Authorization 头，请经反向代理注入请求头，或改用 curl；本页不保存 token。",
      "audit.intro": "最近 64 条受控写事件（服务端内存）；实时模式下随刷新更新，写入成功后会立即重取。审计需实时模式（静态快照无写事件）。",
      "audit.refresh": "刷新审计",      "skipLink": "跳到主内容",
      "confirm.stale": "⚠️ 数据可能过期（STALE/PAUSED）：写入前请确认现场状态。",
      "confirm.title": "确认写入（二次确认）",
      "confirm.slave": "目标站：",
      "confirm.object": "对象：",
      "confirm.meta": "位长/访问：",
      "confirm.old": "旧值（小端 hex）：",
      "confirm.new": "新值（小端 hex，含字节数）：",
      "confirm.cancel": "取消（Esc）",
      "confirm.submit": "确认写入",
      "confirm.note": "该操作仅在服务端以 --serve-write 启动时生效；写入会记入 /sdo-audit。",      "footer.boundary": "EC-Sim 只读状态视图 · 快照语义（非实时 SLA）· 不宣称 EtherCAT 一致性/CTT/LinuxCNC 或任何认证；控制面默认无鉴权、仅回环/受信环境使用。⚠️ 观察值为读取时刻近似值。",
    },
    en: {
      "app.title": "EC-Sim Status View",
      "nav.overview": "Overview",
      "nav.slave": "Slave",
      "nav.pdo": "PDO",
      "nav.dict": "Dictionary",
      "nav.sdo": "SDO",
      "nav.evidence": "Evidence",
      "nav.about": "About",
      "topbar.reconnect": "Reconnect",
      "topbar.pause": "Pause",
      "topbar.resume": "Resume",
      "topbar.about": "About / Limits",
      "topbar.lang": "中文",
      "section.load": "Load State",
      "section.sdo": "SDO Browser (read-only)",
      "section.audit": "SDO Write Audit (GET /sdo-audit)",
      "section.evidence": "Evidence Export (S-11 / V-06)",
      "section.about": "About / Limits (V-07)",
      "load.api": "API address",
      "load.fromApi": "Load from API",
      "load.live": "Live refresh",
      "load.slaveHint": "Pick the slave above (0-based, 0..N-1); for live data run ec-sim-esc --serve 127.0.0.1:8090 --uds <sock> --manifest plugins/… (same process serves /pdo, /topology, /metrics, /events).",
      "conn.idle": "Disconnected",
      "conn.connecting": "Connecting…",
      "conn.liveSse": "LIVE · SSE",
      "conn.livePoll": "LIVE · polling",
      "conn.stale": "STALE (data outdated)",
      "conn.stopped": "Disconnected",
      "conn.paused": "Paused",
      "conn.reconnecting": "Reconnecting…",
      "notice.static": "Static snapshot mode: the loaded state has no live field (from --dump-state or file import); no realtime/offline refresh. For live data run ec-sim-esc --serve <addr> --uds <sock> (or --tap) to start the same-process live mode.",
      "banner.livePoll": "E-10: SSE stream lost; page degraded to polling (LIVE · polling). Data may lag by one refresh cycle; auto-returns to LIVE · SSE when restored.",
      "banner.stopped": "E-10: 3 failures — auto-refresh stopped (disconnected). Start the service and press Reconnect; values are the last snapshot and may be stale.",
      "banner.stale": "STALE: no new data for over 2 refresh cycles; all values carry stale marks — do not treat them as live.",
      "banner.paused": "Paused: no requests are sent; last values and timestamps are kept. Press Resume to fetch once immediately.",
      "banner.reconnecting": "Reconnecting: a probe request has been sent…",
      "section.overview": "Bus Overview (health)",
      "section.topology": "Bus Topology",
      "section.slaveIdentity": "Identity",
      "section.slaveLive": "Live State (--serve live mode)",
      "section.sm": "Sync Manager",
      "section.rx": "RxPDO (master → slave)",
      "section.tx": "TxPDO (slave → master)",
      "section.dc": "Distributed Clock",
      "section.dict": "Dictionary",
      "section.pdoCurrent": "Process Data (PDO, snapshot)",
      "section.pdoDecoded": "PDO Named Decode (mapping order)",
      "section.pdoHistory": "PDO History",
      "section.metrics": "Metrics (/metrics)",
      "section.evidence": "Evidence Export (S-11 / V-06)",
      "section.about": "About / Limits (V-07)",      "intro.dataSource": "Data source: JSON generated by ec-sim-esc --dump-state (read-only state/PDO/topology/metrics; SDO panel is read-only; writes require explicit opt-in and confirmation). Drop JSON files below, or run ec-sim-esc --dump-state --manifest plugins/… > web/state.json and serve with python3 -m http.server 8080 --directory web (the page loads state.json from the same directory).",
      "ctx.slave": "Slave:",
      "ctx.address": "Station:",
      "ctx.al": "AL:",
      "ctx.profile": "Profile:",
      "ctx.copy": "Copy summary",
      "ctx.export": "Export JSON",
      "sdo.intro": "Reads the runtime object dictionary via GET /sdo?index&sub&slave (no mailbox/CoE round-trip). Set the API address above and load first; requests carry the current slave (B55 validates 0-based).",
      "sdo.index": "Index (hex, 0x optional)",
      "sdo.sub": "Subindex",
      "sdo.read": "Read",
      "sdo.writeEnable": "Allow writes (server --serve-write; confirmed before writing)",
      "sdo.newValue": "New value (little-endian hex, e.g. 0f00)",
      "sdo.write": "Write…",
      "sdo.boundary": "Writes are off by default: only with --serve-write and manifest-writable objects succeed; identity/config objects (0x1000/0x1008/0x1009/0x1010/0x1011/0x1018/0x1C12/0x1C13) and length mismatches are rejected. Server responses are authoritative; the confirm dialog shows object, bits, old/new value and slave.",
      "sdo.token": "Token mode: with --serve-token-env the browser fetch cannot add Authorization automatically — use a reverse proxy or curl; this page never stores tokens.",
      "audit.intro": "Last 64 writes (in-memory), refreshed live and after writes; audit needs live mode (no writes in static snapshots).",
      "audit.refresh": "Refresh audit",      "skipLink": "Skip to content",
      "confirm.stale": "⚠️ Data may be stale (STALE/PAUSED): confirm the machine state before writing.",
      "confirm.title": "Confirm write",
      "confirm.slave": "Slave: ",
      "confirm.object": "Object: ",
      "confirm.meta": "Bits/access: ",
      "confirm.old": "Old (LE hex): ",
      "confirm.new": "New (LE hex, bytes): ",
      "confirm.cancel": "Cancel (Esc)",
      "confirm.submit": "Confirm write",
      "confirm.note": "Effective only with --serve-write; writes are recorded in /sdo-audit.",      "footer.boundary": "Read-only status view · snapshot semantics (no realtime SLA) · no EtherCAT conformance or any certification claimed · loopback/trusted use only. ⚠️ values approx.",
    },
  };

  // app.js 动态文案的"中文源串 → 英文"映射（tz 用法；键为中文源串，便于逐点迁移与校验）。
  const enByZh = {
    "索引": "Index", "子索引": "Sub", "名称": "Name", "类型": "Type", "位长": "Bits", "访问": "Access",
    "值": "Value", "字段": "Field", "方向": "Direction", "地址": "Address", "字节数": "Bytes",
    "u16 解码": "u16 decode", "PDO": "PDO", "SM": "SM", "条目数": "Entries", "映射": "Mapping",
    "站地址": "Station", "AL 状态": "AL state", "AL 状态码": "AL code", "收/发帧": "Frames in/out",
    "SM2（地址 hex）": "SM2 (addr/hex)", "SM3（地址 hex）": "SM3 (addr/hex)",
    "时间": "Time", "站（0-based）": "Slave (0-based)", "旧值（小端 hex）": "Old (LE hex)",
    "新值（小端 hex）": "New (LE hex)", "收帧": "Frames in", "发帧": "Frames out", "丢弃帧": "Dropped",
    "datagram 命中": "Datagram hits", "邮箱请求": "Mailbox req", "邮箱响应": "Mailbox resp",
    "DC 寄存器读": "DC reg reads", "DC 寄存器写": "DC reg writes", "DC 模拟时间（ns）": "DC sim time (ns)",
    "字典对象数": "Dict objects", "画像": "Profile", "对象": "objects", "条目": "entries",
    "已载入：": "Loaded: ", "解析失败：": "Parse failed: ", "实时请求失败": "Live request failed",
    "写入成功（站 ": "Write OK (slave ", "已取消写入。": "Write cancelled.", "写入中…": "Writing…",
    "读取中：": "Reading: ", "读取失败：": "Read failed: ", "正在重连：": "Reconnecting: ",
    "暂无写事件。": "No write events.", "审计为空：尚未发生受控写。": "Audit empty: no controlled writes yet.",
    "审计读取失败：": "Audit load failed: ", "需要实时模式（--serve + --uds/--tap）。": "Live mode required (--serve + --uds/--tap).",
    "请先填写 API 地址并载入。": "Enter the API address and load first.",
    "复制 SM2/SM3 hex": "Copy SM2/SM3 hex", "暂停/继续": "Pause/Resume", "复制旧值 hex": "Copy old hex",
    "查看 PDO 映射": "View PDO mapping", "复制身份": "Copy identity", "用 SDO 读取该对象": "Read object via SDO",
    "关闭（Esc）": "Close (Esc)", "清空筛选": "Clear filters", "复制摘要": "Copy summary",
    "复制版本信息": "Copy version info", "复制总览摘要": "Copy overview summary",
    "无审计数据（需实时模式且发生过受控写）。": "No audit data (live mode + writes required).",
    "访问=全部": "Access: all", "有值": "Has value", "无值": "No value", "导出预览（Markdown）": "Export preview (Markdown)",
    "复制 Markdown": "Copy Markdown", "下载 .md": "Download .md", "下载 .json": "Download .json",
    "证据导出（只读；S-11 / V-06）": "Evidence Export (S-11 / V-06)", "关于 / 边界（V-07）": "About / Limits (V-07)",
    "确认写入（二次确认）": "Confirm write", "目标站：": "Target slave: ", "对象：": "Object: ",
    "位长/访问：": "Bits/access: ", "旧值（小端 hex）：": "Old (LE hex): ", "新值（小端 hex，含字节数）：": "New (LE hex, bytes): ",
    "取消（Esc）": "Cancel (Esc)", "确认写入": "Confirm write", "该操作仅在服务端以 --serve-write 启动时生效；写入会记入 /sdo-audit。": "Effective only with --serve-write; writes are recorded in /sdo-audit.",
    "组件": "Component", "已验证": "Verified", "备注": "Notes", "其它主站/系统": "Other masters/systems", "未验证": "Unverified",
    "AL 状态色：": "AL color: ", "正常（PREOP/SAFEOP/OP）": "normal (PREOP/SAFEOP/OP)", "异常或 INIT": "fault or INIT",
    "导出预览": "Export preview", "边界（必读）": "Boundaries (read)", "身份与画像": "Identity", "AL / 实时": "AL / live",
    "PDO 当前": "PDO current", "PDO 历史": "PDO history", "拓扑": "Topology", "指标": "Metrics", "对象字典": "Dictionary", "SDO 审计": "SDO audit",
    "复制总览摘要": "Copy overview summary", "展开前 20 条采样（共": "Show first 20 samples (of ",    "跳到主内容": "Skip to main content",
    "数据源：": "Source: ", "数据时间：": "Data time: ",
    "当前站：": "Slave: ", "（0-based，0..N-1）": " (0-based, 0..N-1)",
    "未填写": "not set", "静态/未刷新": "static/not refreshed", "数据：": "Data: ",
    "无审计数据（需实时模式且发生过受控写）。": "No audit data (live mode + writes required).",
    "审计为空：尚未发生受控写。": "Audit empty: no controlled writes yet.",
    "暂无写事件。": "No write events.", "审计读取失败：": "Audit load failed: ",
    "共": "Total", "条（服务端最多 64 条）。": " entries (server keeps up to 64).",
    "健康检查：未检查（需实时模式）": "Health: not checked (live mode required)",
    "健康检查：失败（无法连接）": "Health: failed (cannot connect)",
    "组件": "Component", "已验证": "Verified", "备注": "Notes",
    "IgH 主站（ethercat）": "IgH master (ethercat)", "SOEM（夹具）": "SOEM (fixture)",
    "LinuxCNC + lcec": "LinuxCNC + lcec", "验收机/虚拟闭环": "test VM / virtual loop",
    "TAP/UDS 夹具": "TAP/UDS fixture", "V1/V2 虚拟闭环由 CNC 侧验收": "V1/V2 virtual loop judged by CNC side",
    "其它主站/系统": "Other masters/systems", "未验证": "Unverified", "不在本视图支持范围": "out of scope for this view",
    "token 只提示不输入：": "Token is shown as a hint only: ", "不宣称：": "Not claimed: ",
    "EC-Sim 只读状态视图 · 版本：ec-sim-web（与 ec-sim-esc 同仓发布）": "EC-Sim read-only status view · version: ec-sim-web (released with ec-sim-esc)",
    "可复制版本信息": "Copy version info",
    "版本：ecsim-evidence-v1 · 模式：": "Version: ecsim-evidence-v1 · mode: ",
    "来源：": "Source: ", "时间：": "Time: ", "站：": "Slave: ",
    "尚未载入数据：请选择 JSON 文件或填写 API 地址后点“从 API 载入”。": "No data loaded yet: choose JSON files or enter the API address and press Load from API.",
    "请选择 --dump-state / --dump-dict --json 生成的 JSON，或使用上方只读 API（--serve）。": "Choose a JSON produced by --dump-state / --dump-dict --json, or use the read-only API above (--serve).",
    "只读视图，不构成一致性/认证结论。": "Read-only view; not a conformance/certification conclusion.",
    "无结果：空字典或筛选条件过严。": "No results: empty dictionary or filters too narrow.",
    "该视图需要实时模式": "This view requires live mode",
    "⚠️ 当前数据可能过期（": "⚠️ Current data may be stale (",
    "写入前请确认现场状态。": "confirm the machine state before writing.",
    "⚠️ 数据可能过期（STALE/PAUSED）：写入前请确认现场状态。": "⚠️ Data may be stale (STALE/PAUSED): confirm the machine state before writing.",
  };

  function readLang() {
    // 支持 `?lang=en` 静态覆盖（便于截图/自动化验收；不写 localStorage）。
    try {
      const override = new URLSearchParams(window.location.search).get("lang");
      if (override === "en" || override === "zh") return override;
    } catch (error) {
      // 非标准宿主（无 URLSearchParams）时忽略
    }
    try {
      return localStorage.getItem(STORAGE_KEY) === "en" ? "en" : "zh";
    } catch (error) {
      return "zh";
    }
  }

  let lang = readLang();

  function t(key, params) {
    const raw = (dict[lang] && dict[lang][key]) || dict.zh[key] || key;
    if (!params) return raw;
    let text = raw;
    for (const [name, value] of Object.entries(params)) {
      text = text.split(`{${name}}`).join(String(value));
    }
    return text;
  }

  function updateToggle() {
    const button = document.getElementById("lang-toggle");
    if (!button) return;
    button.textContent = t("topbar.lang");
    button.setAttribute("aria-pressed", lang === "en" ? "true" : "false");
    button.setAttribute("title", lang === "en" ? "Switch to Chinese" : "切换到英文");
  }

  function applyStatic(root) {
    const scope = root || document;
    for (const node of scope.querySelectorAll("[data-i18n]")) {
      const key = node.getAttribute("data-i18n");
      const text = t(key);
      if (node.hasAttribute("data-i18n-html")) node.innerHTML = text;
      else node.textContent = text;
    }
    document.documentElement.lang = lang === "en" ? "en" : "zh-CN";
    updateToggle();
  }

  function setLang(next) {
    lang = next === "en" ? "en" : "zh";
    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch (error) {
      // 隐私模式：忽略持久化失败
    }
    applyStatic(document);
    window.dispatchEvent(new CustomEvent("ecsim-lang", { detail: { lang } }));
  }

  function text(zh) {
    return lang === "zh" ? zh : (enByZh[zh] || zh);
  }

  window.ECSIM_I18N = {
    t,
    text,
    applyStatic,
    setLang,
    keys: dict,
    get lang() {
      return lang;
    },
    toggle() {
      setLang(lang === "zh" ? "en" : "zh");
    },
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => applyStatic(document));
  } else {
    applyStatic(document);
  }
})();
