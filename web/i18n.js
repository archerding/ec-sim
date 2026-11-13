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
      "footer.boundary": "EC-Sim 只读状态视图 · 快照语义（非实时 SLA）· 不宣称 EtherCAT 一致性/CTT/LinuxCNC 或任何认证；控制面默认无鉴权、仅回环/受信环境使用。⚠️ 观察值为读取时刻近似值。",
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
      "footer.boundary": "Read-only status view · snapshot semantics (no realtime SLA) · no EtherCAT conformance or any certification claimed · loopback/trusted use only. ⚠️ values approx.",
    },
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

  window.ECSIM_I18N = {
    t,
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
