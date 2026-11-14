// task-78：C4 浏览器侧 4h 采样器 v2（健壮版）。
// 关键：任何单次失败不退出；连续 3 次失败仅做"软恢复"（reload → 重开 Edge），继续到 deadline；
// 保留 timestamps 单调；单实例锁；JSONL 每行含 ts/heap/requests/timers_live/sse_*/errors/crashed/error?。
// 用法：node scripts/spikes/hmi-c4-browser-sampler.mjs <url> <out.jsonl> [duration=14400] [interval=60]
import { spawn } from "node:child_process";
import { appendFile, mkdir, open, unlink } from "node:fs/promises";
import { unlinkSync } from "node:fs";
import http from "node:http";
import path from "node:path";

const url = process.argv[2];
const outFile = process.argv[3];
const duration = Number(process.argv[4] || 14400);
const interval = Number(process.argv[5] || 60);
if (!url || !outFile) { console.error("usage: hmi-c4-browser-sampler.mjs <url> <out.jsonl> [duration] [interval]"); process.exit(2); }
const edge = process.env.EDGE_BIN || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
await mkdir(path.dirname(outFile), { recursive: true });

const lockPath = `${outFile}.lock`;
let lockHandle;
try { lockHandle = await open(lockPath, "wx"); } catch (error) { console.error("SAMPLER_LOCKED", lockPath); process.exit(9); }
process.on("exit", () => { try { if (lockHandle) lockHandle.close(); } catch (e) {} try { unlinkSync(lockPath); } catch (e) {} });

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const getJson = (port, pathname) => new Promise((resolve, reject) => {
  http.get({ host: "127.0.0.1", port, path: pathname }, (r) => { let b = ""; r.on("data", (c) => (b += c)); r.on("end", () => { try { resolve(JSON.parse(b)); } catch (e) { reject(e); } }); }).on("error", reject);
});
class Cdp {
  constructor(socket) { this.socket = socket; this.id = 1; this.pending = new Map(); this.closed = false; socket.addEventListener("message", (e) => { const m = JSON.parse(e.data); const p = this.pending.get(m.id); if (!p) return; this.pending.delete(m.id); if (m.error) p.reject(new Error(JSON.stringify(m.error))); else p.resolve(m.result); }); socket.addEventListener("close", () => { this.closed = true; }); }
  send(method, params = {}) { const id = this.id++; return new Promise((res, rej) => { if (this.closed) return rej(new Error("CDP_CLOSED")); this.pending.set(id, { resolve: res, reject: rej }); this.socket.send(JSON.stringify({ id, method, params })); }); }
}
async function evaluate(cdp, expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return r.result && r.result.value;
}

const inject = `(() => {
  window.__c4 = { fetch: 0, xhr: 0, intervalsCreated: 0, timeoutsCreated: 0, intervalsLive: 0, timeoutsLive: 0, esCreated: 0, esOpens: 0, esErrors: 0, messages: 0 };
  const c = window.__c4;
  if (window.fetch) { const orig = window.fetch.bind(window); window.fetch = function (...a) { c.fetch += 1; return orig(...a); }; }
  const XHR = window.XMLHttpRequest;
  if (XHR && XHR.prototype) { const open = XHR.prototype.open; XHR.prototype.open = function (...a) { c.xhr += 1; return open.apply(this, a); }; }
  const oi = window.setInterval.bind(window);
  window.setInterval = function (...a) { c.intervalsCreated += 1; c.intervalsLive += 1; return oi(...a); };
  const ci = window.clearInterval.bind(window);
  window.clearInterval = function (id) { c.intervalsLive = Math.max(0, c.intervalsLive - 1); return ci(id); };
  const ot = window.setTimeout.bind(window);
  window.setTimeout = function (...a) { c.timeoutsCreated += 1; c.timeoutsLive += 1; const fn = a[0]; if (typeof fn === "function") { a[0] = function (...x) { c.timeoutsLive = Math.max(0, c.timeoutsLive - 1); return fn.apply(this, x); }; } return ot(...a); };
  const ct = window.clearTimeout.bind(window);
  window.clearTimeout = function (id) { c.timeoutsLive = Math.max(0, c.timeoutsLive - 1); return ct(id); };
  if (window.EventSource) {
    const Orig = window.EventSource;
    const Wrapped = function (u, o) { const es = new Orig(u, o); c.esCreated += 1; es.addEventListener("open", () => { c.esOpens += 1; }); es.addEventListener("error", () => { c.esErrors += 1; }); es.addEventListener("message", () => { c.messages += 1; }); return es; };
    Wrapped.prototype = Orig.prototype; Object.setPrototypeOf(Wrapped, Orig);
    window.EventSource = Wrapped;
  }
})();`;

let edgeChild = null;
let cdp = null;
let socket = null;
let consecutiveFailures = 0;

async function launchEdge() {
  if (edgeChild) { try { edgeChild.kill("SIGKILL"); } catch (e) {} edgeChild = null; }
  if (socket) { try { socket.close(); } catch (e) {} socket = null; }
  const port = 9700 + Math.floor(Math.random() * 200);
  const profile = path.join(path.dirname(outFile), `.edge-${port}-${Date.now()}`);
  edgeChild = spawn(edge, ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
  for (let i = 0; i < 100; i += 1) {
    try {
      const targets = await getJson(port, "/json");
      const page = targets.find((t) => t.type === "page");
      if (page) {
        socket = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((res, rej) => { socket.addEventListener("open", res, { once: true }); socket.addEventListener("error", rej, { once: true }); });
        cdp = new Cdp(socket);
        await cdp.send("Page.enable"); await cdp.send("Runtime.enable"); await cdp.send("Performance.enable");
        await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: inject });
        await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
        return true;
      }
    } catch (error) { /* retry */ }
    await sleep(150);
  }
  return false;
}

async function openPage() {
  await cdp.send("Page.navigate", { url });
  await sleep(2000);
  try {
    await evaluate(cdp, `(async () => { const input = document.getElementById("api"); if (input) input.value = location.origin; document.getElementById("load-api")?.click(); await new Promise(r => setTimeout(r, 1200)); const live = document.getElementById("live"); if (live) { live.checked = true; live.dispatchEvent(new Event("change", { bubbles: true })); } await new Promise(r => setTimeout(r, 1500)); })()`);
  } catch (error) {
    await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), error: `open_page: ${String(error)}`, crashed: false, retry: true }) + "\n");
  }
}

async function recover() {
  try { if (cdp) await cdp.send("Page.reload"); await sleep(1500); await openPage(); return; } catch (error) { /* fallthrough */ }
  if (await launchEdge()) await openPage();
}

try {
  if (!(await launchEdge())) { await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), error: "edge_launch_failed", crashed: true }) + "\n"); process.exit(1); }
  await openPage();
  const started = Date.now();
  while (Date.now() - started < duration * 1000) {
    try {
      const sample = JSON.parse(await evaluate(cdp, `(() => {
        const c = window.__c4 || {};
        return JSON.stringify({
          heap: performance.memory ? performance.memory.usedJSHeapSize : null,
          requests: (c.fetch || 0) + (c.xhr || 0),
          resource_entries: performance.getEntriesByType("resource").length,
          timers_created: (c.intervalsCreated || 0) + (c.timeoutsCreated || 0),
          timers_live: (c.intervalsLive || 0) + (c.timeoutsLive || 0),
          sse_connects: c.esCreated || 0, sse_opens: c.esOpens || 0, sse_errors: c.esErrors || 0, sse_messages: c.messages || 0,
          sections: document.querySelectorAll("#app section").length,
          text_len: (document.body.innerText || "").length,
        });
      })()`));
      if (sample.heap == null) {
        try { const metrics = await cdp.send("Performance.getMetrics"); const heap = metrics.metrics.find((m) => m.name === "JSHeapUsedSize"); if (heap) sample.heap = heap.value; } catch (error) { /* ignore */ }
      }
      consecutiveFailures = 0;
      await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), ...sample, crashed: false }) + "\n");
      console.log("SAMPLE", JSON.stringify(sample));
    } catch (error) {
      consecutiveFailures += 1;
      await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), retry: true, failures: consecutiveFailures, error: String(error), crashed: false }) + "\n");
      if (consecutiveFailures >= 3) {
        await recover();
        consecutiveFailures = 0;
      }
    }
    await sleep(interval * 1000);
  }
  await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), finished: true, duration_seconds: Math.round((Date.now() - started) / 1000) }) + "\n");
} catch (error) {
  await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), fatal: true, error: String(error), crashed: true }) + "\n");
  process.exitCode = 1;
} finally {
  try { if (socket) socket.close(); } catch (e) {}
  try { if (edgeChild) edgeChild.kill("SIGKILL"); } catch (e) {}
}
