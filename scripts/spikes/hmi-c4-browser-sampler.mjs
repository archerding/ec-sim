// task-66 C4 v2 浏览器侧 4h 采样（Windows 侧，经 SSH 隧道；headless Edge + CDP）。
// 输出 JSONL：ts/heap/requests/timers/sse_connects/sse_reconnects/errors/crashed/sections
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
const child = spawn(edge, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${path.join(path.dirname(outFile), ".edge")}`, "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
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
  constructor(s) { this.s = s; this.id = 1; this.p = new Map(); s.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.method === "Runtime.exceptionThrown") this.errors = (this.errors || 0) + 1; const p = this.p.get(m.id); if (!p) return; this.p.delete(m.id); if (m.error) p.reject(new Error(JSON.stringify(m.error))); else p.resolve(m.result); }); this.errors = 0; }
  send(method, params = {}) { const id = this.id++; return new Promise((res, rej) => { this.p.set(id, { resolve: res, reject: rej }); this.s.send(JSON.stringify({ id, method, params })); }); }
}
async function evaluate(cdp, expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return r.result && r.result.value;
}
// 注入计数器：interval/timeout、EventSource（含 onerror/重连）、错误
const inject = `(() => {
  window.__c4 = { intervals: 0, timeouts: 0, esCreated: 0, esErrors: 0, messages: 0 };
  const setIntervalOrig = window.setInterval.bind(window);
  window.setInterval = function (...args) { window.__c4.intervals += 1; return setIntervalOrig(...args); };
  const setTimeoutOrig = window.setTimeout.bind(window);
  window.setTimeout = function (...args) { window.__c4.timeouts += 1; return setTimeoutOrig(...args); };
  if (window.EventSource) {
    const Orig = window.EventSource;
    const Wrapped = function (url, opts) { const es = new Orig(url, opts); window.__c4.esCreated += 1; es.addEventListener("error", () => { window.__c4.esErrors += 1; }); es.addEventListener("message", () => { window.__c4.messages += 1; }); return es; };
    Wrapped.prototype = Orig.prototype; Object.setPrototypeOf(Wrapped, Orig);
    window.EventSource = Wrapped;
  }
})();`;
try {
  const t = await target();
  const socket = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => { socket.addEventListener("open", res, { once: true }); socket.addEventListener("error", rej, { once: true }); });
  const cdp = new Cdp(socket);
  await cdp.send("Page.enable"); await cdp.send("Runtime.enable");
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: inject });
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  await cdp.send("Page.navigate", { url });
  await sleep(2000);
  // 载入 API + 勾选实时刷新（触发 SSE 长连接）
  await evaluate(cdp, `(async () => { const input = document.getElementById("api"); if (input) input.value = location.origin; document.getElementById("load-api")?.click(); await new Promise(r => setTimeout(r, 800)); const live = document.getElementById("live"); if (live && !live.checked) { live.click(); } })()`);
  const started = Date.now();
  while (Date.now() - started < duration * 1000) {
    const sample = await evaluate(cdp, `(() => {
      const c = window.__c4 || {};
      const resources = performance.getEntriesByType("resource");
      const memory = performance.memory ? performance.memory.usedJSHeapSize : null;
      return JSON.stringify({
        heap: memory,
        requests: resources.length,
        timers: (c.intervals || 0) + (c.timeouts || 0),
        sse_connects: c.esCreated || 0,
        sse_reconnects: c.esErrors || 0,
        sse_messages: c.messages || 0,
        sections: document.querySelectorAll("#app section").length,
        textLen: (document.body.innerText || "").length,
      });
    })()`);
    const row = { ts: new Date().toISOString(), ...JSON.parse(sample), errors: cdp.errors || 0, crashed: false };
    await appendFile(outFile, JSON.stringify(row) + "\n");
    console.log("SAMPLE", JSON.stringify(row));
    // 崩溃探测：页面不可评估时抛错，标记 crashed 后退出。
    await sleep(interval * 1000);
  }
  socket.close();
} catch (error) {
  await appendFile(outFile, JSON.stringify({ ts: new Date().toISOString(), crashed: true, error: String(error) }) + "\n");
  console.error("SAMPLER_ERROR", error);
  process.exitCode = 1;
} finally {
  child.kill("SIGTERM"); await sleep(200); child.kill("SIGKILL");
}
