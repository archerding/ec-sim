// task-66 C4 v2 浏览器侧 4h 采样（Windows 侧，经 SSH 隧道；headless Edge + CDP）。
// 修正（sim-expert 预审）：requests=fetch/XHR 计数器（避免 resource buffer 饱和）；timers_created/timers_live；
// heap 优先 performance.memory + CDP Performance 兜底；连续 3 次失败才判 crashed；sse_opens 与 sse_errors 分列。
// 用法：node scripts/spikes/hmi-c4-browser-sampler.mjs <url> <outJsonl> [durationSeconds=14400] [interval=60]
import { spawn } from "node:child_process";
import { appendFile, mkdir } from "node:fs/promises";
import http from "node:http";
import path from "node:path";

const url = process.argv[2];
const outFile = process.argv[3];
const duration = Number(process.argv[4] || 14400);
const interval = Number(process.argv[5] || 60);
if (!url || !outFile) { console.error("usage: hmi-c4-browser-sampler.mjs <url> <out.jsonl> [duration] [interval]"); process.exit(2); }
const edge = process.env.EDGE_BIN || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
await mkdir(path.dirname(outFile), { recursive: true });
const port = 9666;
const child = spawn(edge, ["--headless=new", "--enable-precise-memory-info", `--remote-debugging-port=${port}`, `--user-data-dir=${path.join(path.dirname(outFile), ".edge")}`, "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const getJson = (p) => new Promise((resolve, reject) => {
  http.get({ host: "127.0.0.1", port, path: p }, (r) => { let b = ""; r.on("data", (c) => (b += c)); r.on("end", () => { try { resolve(JSON.parse(b)); } catch (e) { reject(e); } }); }).on("error", reject);
});
async function target() {
  for (let i = 0; i < 120; i += 1) {
    try { const t = (await getJson("/json")).find((x) => x.type === "page"); if (t) return t; } catch (e) { /* retry */ }
    await sleep(150);
  }
  throw new Error("edge timeout");
}
class Cdp {
  constructor(s) { this.s = s; this.id = 1; this.p = new Map(); s.addEventListener("message", (e) => { const m = JSON.parse(e.data); const p = this.p.get(m.id); if (!p) return; this.p.delete(m.id); if (m.error) p.reject(new Error(JSON.stringify(m.error))); else p.resolve(m.result); }); }
  send(method, params = {}) { const id = this.id++; return new Promise((res, rej) => { this.p.set(id, { resolve: res, reject: rej }); this.s.send(JSON.stringify({ id, method, params })); }); }
}
async function evaluate(cdp, expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return r.result && r.result.value;
}

const inject = `(() => {
  window.__c4 = { fetch: 0, xhr: 0, intervalsCreated: 0, timeoutsCreated: 0, intervalsLive: 0, timeoutsLive: 0, esCreated: 0, esOpens: 0, esErrors: 0, messages: 0 };
  const c = window.__c4;
  if (window.fetch) {
    const orig = window.fetch.bind(window);
    window.fetch = function (...args) { c.fetch += 1; return orig(...args); };
  }
  const XHR = window.XMLHttpRequest;
  if (XHR && XHR.prototype) {
    const open = XHR.prototype.open;
    XHR.prototype.open = function (...args) { c.xhr += 1; return open.apply(this, args); };
  }
  const origSetInterval = window.setInterval.bind(window);
  window.setInterval = function (...args) { c.intervalsCreated += 1; c.intervalsLive += 1; return origSetInterval(...args); };
  const origClearInterval = window.clearInterval.bind(window);
  window.clearInterval = function (id) { c.intervalsLive = Math.max(0, c.intervalsLive - 1); return origClearInterval(id); };
  const origSetTimeout = window.setTimeout.bind(window);
  window.setTimeout = function (...args) { c.timeoutsCreated += 1; c.timeoutsLive += 1; const fn = args[0]; if (typeof fn === "function") { args[0] = function (...inner) { c.timeoutsLive = Math.max(0, c.timeoutsLive - 1); return fn.apply(this, inner); }; } return origSetTimeout(...args); };
  const origClearTimeout = window.clearTimeout.bind(window);
  window.clearTimeout = function (id) { c.timeoutsLive = Math.max(0, c.timeoutsLive - 1); return origClearTimeout(id); };
  if (window.EventSource) {
    const Orig = window.EventSource;
    const Wrapped = function (u, o) { const es = new Orig(u, o); c.esCreated += 1; es.addEventListener("open", () => { c.esOpens += 1; }); es.addEventListener("error", () => { c.esErrors += 1; }); es.addEventListener("message", () => { c.messages += 1; }); return es; };
    Wrapped.prototype = Orig.prototype; Object.setPrototypeOf(Wrapped, Orig);
    window.EventSource = Wrapped;
  }
})();`;

let cdp;
let consecutiveFailures = 0;
try {
  const t = await target();
  const socket = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => { socket.addEventListener("open", res, { once: true }); socket.addEventListener("error", rej, { once: true }); });
  cdp = new Cdp(socket);
  await cdp.send("Page.enable"); await cdp.send("Runtime.enable"); await cdp.send("Performance.enable");
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: inject });
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  await cdp.send("Page.navigate", { url });
  await sleep(2000);
  // 确保实时模式：勾选 #live 并派发 change（触发 SSE EventSource 创建），等待连接建立。
  await evaluate(cdp, `(async () => { const input = document.getElementById("api"); if (input) input.value = location.origin; document.getElementById("load-api")?.click(); await new Promise(r => setTimeout(r, 1200)); const live = document.getElementById("live"); if (live) { live.checked = true; live.dispatchEvent(new Event("change", { bubbles: true })); } await new Promise(r => setTimeout(r, 1500)); return document.getElementById("conn-badge")?.dataset?.mode || null; })()`);

  const started = Date.now();
  while (Date.now() - started < duration * 1000) {
    try {
      const sample = JSON.parse(await evaluate(cdp, `(() => {
        const c = window.__c4 || {};
        const resources = performance.getEntriesByType("resource");
        return JSON.stringify({
          heap: performance.memory ? performance.memory.usedJSHeapSize : null,
          requests: (c.fetch || 0) + (c.xhr || 0),
          resource_entries: resources.length,
          timers_created: (c.intervalsCreated || 0) + (c.timeoutsCreated || 0),
          timers_live: (c.intervalsLive || 0) + (c.timeoutsLive || 0),
          sse_connects: c.esCreated || 0,
          sse_opens: c.esOpens || 0,
          sse_errors: c.esErrors || 0,
          sse_messages: c.messages || 0,
          sections: document.querySelectorAll("#app section").length,
          text_len: (document.body.innerText || "").length,
        });
      })()`));
      if (sample.heap == null) {
        try {
          const metrics = await cdp.send("Performance.getMetrics");
          const heap = metrics.metrics.find((m) => m.name === "JSHeapUsedSize");
          sample.heap = heap ? heap.value : null;
        } catch (e) { sample.heap = null; }
      }
      consecutiveFailures = 0;
      const row = { ts: new Date().toISOString(), ...sample, crashed: false };
      await appendFile(outFile, JSON.stringify(row) + "\n");
      console.log("SAMPLE", JSON.stringify(row));
    } catch (error) {
      consecutiveFailures += 1;
      const row = { ts: new Date().toISOString(), retry: true, failures: consecutiveFailures, error: String(error), crashed: consecutiveFailures >= 3 };
      await appendFile(outFile, JSON.stringify(row) + "\n");
      console.error("SAMPLE_RETRY", JSON.stringify(row));
      if (consecutiveFailures >= 3) {
        console.error("CRASHED_AFTER_3_FAILURES");
        process.exitCode = 1;
        break;
      }
    }
    await sleep(interval * 1000);
  }
  socket.close();
} catch (error) {
  await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), crashed: true, error: String(error) }) + "\n");
  console.error("SAMPLER_FATAL", error);
  process.exitCode = 1;
} finally {
  child.kill("SIGTERM"); await sleep(200); child.kill("SIGKILL");
}
