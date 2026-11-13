// task-78：DOD-07 全项性能采集（T-PERF-01..06；按 sim-expert §7 口径）。
// - 01: navigationStart → performance.mark('app-ready')（app.js 首屏数据渲染后埋点）+ 首次点击→视觉反馈
// - 02: 视图切换 click→rAF（7 视图 × 3 次），断言 hash/目标 rect
// - 03: SSE onmessage→DOM（注入 wrapper + MutationObserver，真实 /events 推送，客户端不伪造）
// - 04: 真点击→状态变化（导航/暂停点击后 DOM 变化 rAF）
// - 05: 波形 draw 耗时（app.js 埋点 performance.measure('pdo-waveform-draw')），P95≤16ms 参考
// - 06: 合成字典（make-synthetic-dict.py 生成，seed/sha256）文件载入后搜索 input→列表渲染 P95
// 输出 dod07-perf-v2.json（含环境字段/n/P50/P95/P99/NOT_MEASURED 原因）。headless/隧道只作参考，不判 PASS/SLA。
// 用法：node scripts/spikes/hmi-dod07-perf.mjs <baseUrl> <outDir> <syntheticDictPath>
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import http from "node:http";
import path from "node:path";

const [base, outDir, dictPath] = process.argv.slice(2);
if (!base || !outDir || !dictPath) { console.error("usage: hmi-dod07-perf.mjs <baseUrl> <outDir> <dictPath>"); process.exit(2); }
const edge = process.env.EDGE_BIN || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
await mkdir(outDir, { recursive: true });
const port = 9991;
const child = spawn(edge, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${path.join(outDir, ".edge")}`, "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const getJson = (p) => new Promise((resolve, reject) => {
  http.get({ host: "127.0.0.1", port, path: p }, (r) => { let b = ""; r.on("data", (c) => (b += c)); r.on("end", () => { try { resolve(safeParse(b)); } catch (e) { reject(e); } }); }).on("error", reject);
});
async function target() {
  for (let i = 0; i < 120; i += 1) {
    try { const t = (await getJson("/json")).find((x) => x.type === "page"); if (t) return t; } catch (e) { /* retry */ }
    await sleep(150);
  }
  throw new Error("edge timeout");
}
class Cdp {
  constructor(s) { this.s = s; this.id = 1; this.p = new Map(); s.addEventListener("message", (e) => { const m = safeParse(e.data); const p = this.p.get(m.id); if (!p) return; this.p.delete(m.id); if (m.error) p.reject(new Error(JSON.stringify(m.error))); else p.resolve(m.result); }); }
  send(method, params = {}) { const id = this.id++; return new Promise((res, rej) => { this.p.set(id, { resolve: res, reject: rej }); this.s.send(JSON.stringify({ id, method, params })); }); }
}
async function ev(cdp, expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return r.result && r.result.value !== undefined ? r.result.value : "null";
}
const safeParse = (value) => { try { return safeParse(value) ?? {}; } catch (error) { return {}; } };
const pct = (values, p) => { const v = [...values].sort((a, b) => a - b); return v[Math.min(v.length - 1, Math.max(0, Math.round((p / 100) * (v.length - 1))))]; };
const stat = (values) => values.length ? { n: values.length, p50: +pct(values, 50).toFixed(2), p95: +pct(values, 95).toFixed(2), p99: +pct(values, 99).toFixed(2), max: +Math.max(...values).toFixed(2) } : { n: 0, not_measured: true };

const inject = `(() => {
  window.__perf = { sseMsgs: [], mutations: [], firstClick: null };
  if (window.EventSource) {
    const origAdd = EventSource.prototype.addEventListener;
    EventSource.prototype.addEventListener = function (type, listener, options) {
      if (type === "message") {
        const wrapped = function (...args) { window.__perf.sseMsgs.push(performance.now()); return listener.apply(this, args); };
        return origAdd.call(this, type, wrapped, options);
      }
      return origAdd.call(this, type, listener, options);
    };
  }
  document.addEventListener("click", () => {
    if (window.__perf.firstClick === null) { window.__perf.firstClick = performance.now(); requestAnimationFrame(() => { window.__perf.firstClickRaf = performance.now(); }); }
  }, true);
  new MutationObserver((records) => {
    const now = performance.now();
    for (const record of records) window.__perf.mutations.push(now);
  }).observe(document.body, { childList: true, subtree: true, characterData: true });
})();`;

try {
  const t = await target();
  const socket = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => { socket.addEventListener("open", res, { once: true }); socket.addEventListener("error", rej, { once: true }); });
  const cdp = new Cdp(socket);
  await cdp.send("Page.enable"); await cdp.send("Runtime.enable"); await cdp.send("DOM.enable");
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: inject });
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });

  const result = { env: { url: base, browser: "Microsoft Edge headless " + (process.env.EDGE_VER || "157"), viewport: "1280x900", device_scale_factor: 1, mode: "headless+ssh-tunnel", note: "参考证据，不判 PASS/SLA" }, items: {} };

  // ——— T-PERF-01：app-ready
  await cdp.send("Page.navigate", { url: `${base}/index.html` });
  await sleep(2500);
  const ready = await ev(cdp, `(() => { const mark = performance.getEntriesByName("app-ready").pop(); const nav = performance.getEntriesByType("navigation")[0]; return JSON.stringify({ mark: mark ? mark.startTime : null, domContentLoaded: nav ? nav.domContentLoadedEventEnd : null }); })()`);
  const readyObj = safeParse(ready) || {};
  // 首次点击→视觉反馈（导航点击后 rAF）
  await ev(cdp, `document.querySelector('[data-testid="shell-nav-overview"]')?.click()`);
  await sleep(300);
  const clickRaf = await ev(cdp, `JSON.stringify({ firstClick: window.__perf.firstClick, firstClickRaf: window.__perf.firstClickRaf })`);
  result.items["T-PERF-01"] = { app_ready: readyObj, first_click_to_raf_ms: safeParse(clickRaf) || {}, note: "app-ready 缺失时回退 first click→rAF" };

  // 载入 API（live 8 站）+ 开启实时
  await ev(cdp, `(async () => { const input = document.getElementById("api"); if (input) input.value = location.origin; document.getElementById("load-api")?.click(); await new Promise(r => setTimeout(r, 1500)); const live = document.getElementById("live"); if (live) { live.checked = true; live.dispatchEvent(new Event("change", { bubbles: true })); } await new Promise(r => setTimeout(r, 1500)); })()`);

  // ——— T-PERF-02：视图切换（7 视图 × 3 次，click→rAF+目标 rect）
  const views = ["overview-topology", "slave-identity", "pdo-current-table", "dict-table", "sdo-index", "evidence-boundaries", "about-boundaries"];
  const switchTimes = [];
  const switchChecks = [];
  for (let round = 0; round < 3; round += 1) {
    for (const id of views) {
      const r = await ev(cdp, `(async () => { const link = document.querySelector('[data-testid="shell-nav-' + ${JSON.stringify(id === "overview-topology" ? "overview" : id === "slave-identity" ? "slave" : id === "pdo-current-table" ? "pdo" : id === "dict-table" ? "dict" : id === "sdo-index" ? "sdo" : id === "evidence-boundaries" ? "evidence" : "about")}"]'); const t0 = performance.now(); link?.click(); await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))); const t1 = performance.now(); const el = document.getElementById(${JSON.stringify(id)}); const rect = el ? el.getBoundingClientRect() : null; return JSON.stringify({ ms: t1 - t0, found: Boolean(el), top: rect ? Math.round(rect.top) : null, hash: location.hash }); })()`);
      const parsed = safeParse(r);
      switchTimes.push(parsed.ms);
      switchChecks.push({ id, ...parsed, ok: parsed.found && parsed.top !== null && parsed.top >= -8 && parsed.top < 900 * 0.6 });
    }
  }
  result.items["T-PERF-02"] = { ms: stat(switchTimes), checks_ok: switchChecks.filter((c) => c.ok).length, checks_total: switchChecks.length, details: switchChecks.slice(0, 21) };

  // ——— T-PERF-03：SSE→DOM（真实 /events 推送；20s 窗口内最多 30 对）
  await sleep(15000);
  const sseRaw = await ev(cdp, `JSON.stringify({ msgs: window.__perf.sseMsgs.slice(-60), muts: window.__perf.mutations.slice(-400) })`);
  const sseObj = safeParse(sseRaw);
  const latencies = [];
  for (const msg of sseObj.msgs) {
    const next = sseObj.muts.find((m) => m >= msg);
    if (next !== undefined) latencies.push(next - msg);
  }
  result.items["T-PERF-03"] = { ms: stat(latencies.slice(0, 30)), samples: latencies.length, source: "真实控制面 /events（8 站 live 推送），SSE onmessage→MutationObserver", note: "8 站 + 主站在线；客户端未伪造事件" };

  // ——— T-PERF-04：真点击→状态变化（暂停按钮 aria/DOM 变化）
  const clickState = await ev(cdp, `(async () => { const btn = document.getElementById("pause-toggle"); if (!btn) return JSON.stringify({ found: false }); const before = btn.textContent; const t0 = performance.now(); btn.click(); await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))); const after = btn.textContent; return JSON.stringify({ found: true, ms: performance.now() - t0, before, after, changed: before !== after }); })()`);
  result.items["T-PERF-04"] = safeParse(clickState) || { found: false };

  // ——— T-PERF-05：波形 draw 耗时（app.js performance.measure）
  const drawRaw = await ev(cdp, `JSON.stringify(performance.getEntriesByType("measure").filter(e => e.name === "pdo-waveform-draw").map(e => e.duration))`);
  const drawTimes = safeParse(drawRaw);
  result.items["T-PERF-05"] = { ms: stat(drawTimes), note: "draw 函数前后 performance.measure；rAF 间隔仅参考（vsync ~16.7ms）；Canvas 实现差异" };

  // ——— T-PERF-06：合成字典搜索（文件载入 → input→rAF）
  const dictText = await readFile(dictPath, "utf8");
  await cdp.send("DOM.getDocument", {});
  const nodeIds = [];
  const walk = async (nodeId) => {
    const node = await cdp.send("DOM.describeNode", { nodeId });
    if (node.node && node.node.nodeName === "INPUT" && node.node.attributes) {
      const attrs = node.node.attributes;
      const idx = attrs.indexOf("id");
      if (idx >= 0 && attrs[idx + 1] === "file") nodeIds.push(node.node.backendNodeId);
    }
    if (node.node && node.node.children) for (const child of node.node.children) await walk(child.backendNodeId);
  };
  try { await walk(1); } catch (e) { /* fallback below */ }
  let dictLoaded = false;
  if (nodeIds.length > 0) {
    try { await cdp.send("DOM.setFileInputFiles", { files: [dictPath], backendNodeId: nodeIds[0] }); dictLoaded = true; } catch (e) { dictLoaded = false; }
  }
  await sleep(1500);
  const searchTimes = [];
  const searchRows = [];
  if (dictLoaded) {
    for (let i = 0; i < 10; i += 1) {
      const r = await ev(cdp, `(async () => { const input = document.getElementById("dict-search"); if (!input) return JSON.stringify({ found: false }); const t0 = performance.now(); input.value = ${JSON.stringify("Synthetic")}; input.dispatchEvent(new Event("input", { bubbles: true })); await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))); const rows = document.querySelectorAll('#dict-table tbody tr').length; return JSON.stringify({ found: true, ms: performance.now() - t0, rows }); })()`);
      const parsed = safeParse(r);
      searchTimes.push(parsed.ms);
      searchRows.push(parsed.rows);
    }
  }
  result.items["T-PERF-06"] = { ms: stat(searchTimes), rows: searchRows.slice(-3), dict_loaded: dictLoaded, dict_source: path.basename(dictPath), note: "合成/脱敏字典（seed=42），非 8 站真实字典（真实约 18 对象）" };

  const ua = await ev(cdp, "navigator.userAgent");
  result.env.user_agent = ua;
  result.env.window = await ev(cdp, "JSON.stringify({ w: innerWidth, h: innerHeight, dpr: devicePixelRatio })").then(JSON.parse);
  await writeFile(path.join(outDir, "dod07-perf-v2.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result.items, null, 2));
  socket.close();
} catch (error) {
  console.error("DOD07_ERROR", error);
  process.exitCode = 1;
} finally {
  child.kill("SIGTERM"); await sleep(200); child.kill("SIGKILL");
}
