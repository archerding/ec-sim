// H3 取证：DOD-08 21 张截图（3 分辨率 × 7 视图）+ DOD-07 性能 5 轮采样（8 站实时，走 SSH 隧道）。
// 用法：node scripts/spikes/hmi-dod07-08-capture.mjs http://127.0.0.1:18210 <outDir>
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import http from "node:http";
import path from "node:path";

const base = process.argv[2];
const out = process.argv[3];
if (!base || !out) { console.error("usage: hmi-dod07-08-capture.mjs <base-url> <outDir>"); process.exit(2); }
const edge = process.env.EDGE_BIN || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
const views = [
  ["overview", "overview-topology"], ["slave", "slave-identity"], ["pdo", "pdo-current-table"],
  ["dict", "dict-table"], ["sdo", "sdo-index"], ["evidence", "evidence-boundaries"], ["about", "about-boundaries"],
];
const resolutions = [
  { label: "1024x768", width: 1024, height: 768, scale: 1 },
  { label: "1920x1080", width: 1920, height: 1080, scale: 1 },
  { label: "200pct", width: 1024, height: 768, scale: 2 },
];
await mkdir(out, { recursive: true });
const port = 9555;
const child = spawn(edge, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${path.join(out, ".edge")}`, "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
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
  return r.result.value;
}
async function loadPage(cdp, width, height, scale) {
  await cdp.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
  await cdp.send("Page.navigate", { url: `${base}/index.html` });
  await sleep(1800);
  await evaluate(cdp, `(async () => { const input = document.getElementById("api"); if (input) input.value = location.origin; document.getElementById("load-api")?.click(); await new Promise(r => setTimeout(r, 1200)); })()`);
}
try {
  const t = await target();
  const socket = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => { socket.addEventListener("open", res, { once: true }); socket.addEventListener("error", rej, { once: true }); });
  const cdp = new Cdp(socket);
  await cdp.send("Page.enable"); await cdp.send("Runtime.enable");

  // DOD-08：21 张（3 分辨率 × 7 视图）
  for (const res of resolutions) {
    await loadPage(cdp, res.width, res.height, res.scale);
    for (const [view, targetId] of views) {
      const scrolled = await evaluate(cdp, `(async () => { const el = document.getElementById(${JSON.stringify(targetId)}); if (!el) return {found:false, scrollY: window.scrollY}; el.scrollIntoView({block:"start"}); await new Promise(r => requestAnimationFrame(r)); await new Promise(r => requestAnimationFrame(r)); return {found:true, scrollY: window.scrollY, top: Math.round(el.getBoundingClientRect().top), overflowX: document.scrollingElement.scrollWidth - window.innerWidth}; })()`);
      const shot = await cdp.send("Page.captureScreenshot", { format: "png" });
      const file = path.join(out, `${res.label}-${view}.png`);
      await writeFile(file, Buffer.from(shot.data, "base64"));
      console.log(`OK DOD-08 ${res.label} ${view} found=${scrolled.found} scrollY=${scrolled.scrollY} top=${scrolled.top} overflowX=${scrolled.overflowX}`);
    }
  }

  // DOD-07：1280×900，5 轮（FCP/导航时序/API 载入到首渲染/导航点击到滚动）
  const perf = [];
  for (let round = 1; round <= 5; round += 1) {
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
    await cdp.send("Page.navigate", { url: `${base}/index.html` });
    await sleep(1500);
    const result = await evaluate(cdp, `(async () => {
      const nav = performance.getEntriesByType("navigation")[0];
      const paint = performance.getEntriesByType("paint");
      const t0 = performance.now();
      const input = document.getElementById("api"); if (input) input.value = location.origin;
      document.getElementById("load-api")?.click();
      const deadline = performance.now() + 5000;
      while (!document.querySelector('[data-testid="overview-topology"]') && performance.now() < deadline) await new Promise(r => setTimeout(r, 20));
      const apiToFirstRender = performance.now() - t0;
      const t1 = performance.now();
      document.querySelector('[data-testid="shell-nav-evidence"]')?.click();
      await new Promise(r => setTimeout(r, 150));
      const navClickToScroll = performance.now() - t1;
      const scrollY = window.scrollY;
      return {
        fcp: paint.find(p => p.name === "first-contentful-paint")?.startTime ?? null,
        domContentLoaded: nav ? nav.domContentLoadedEventEnd : null,
        load: nav ? nav.loadEventEnd : null,
        apiToFirstRender, navClickToScroll, scrollY,
        appSections: document.querySelectorAll("#app section").length,
        overflowX: document.scrollingElement.scrollWidth - window.innerWidth,
      };
    })()`);
    perf.push({ round, ...result });
    console.log(`OK DOD-07 round ${round}`, JSON.stringify(result));
  }
  await writeFile(path.join(out, "dod07-perf.json"), JSON.stringify(perf, null, 2));
  await writeFile(path.join(out, "dod08-layout.json"), JSON.stringify({ resolutions, views }, null, 2));
  console.log("DONE:", out);
  socket.close();
} finally {
  child.kill("SIGTERM"); await sleep(200); child.kill("SIGKILL");
}
