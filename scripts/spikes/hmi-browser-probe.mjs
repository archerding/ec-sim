// H2（DOD-07/DOD-08）浏览器度量/截图采集：headless Edge + CDP（无第三方依赖，Node ≥ 22 原生 WebSocket）。
// 注意：headless 与真实浏览器存在差异（字体/合成/节流），结果只作**布局/触控/计时参考**，
// 不替代 sim-ux 真人走查（证据中必须注明）。
//
// 用法：node scripts/spikes/hmi-browser-probe.mjs <url> <outDir> [edgePath]
// 输出：<outDir>/*.png + <outDir>/metrics.json（每个尺寸一份）
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import http from "node:http";
import path from "node:path";

const url = process.argv[2];
const outDir = process.argv[3];
const edgeBin = process.argv[4] || process.env.EDGE_BIN || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
if (!url || !outDir) {
  console.error("usage: node hmi-browser-probe.mjs <url> <outDir> [edgePath]");
  process.exit(2);
}

const SIZES = [
  { label: "1024x768", width: 1024, height: 768, scale: 1 },
  { label: "1280x900", width: 1280, height: 900, scale: 1 },
  { label: "1920x1080", width: 1920, height: 1080, scale: 1 },
  { label: "1024x768@125", width: 1024, height: 768, scale: 1.25 },
  { label: "512x384@200", width: 512, height: 384, scale: 2 },
];

const port = 9333;
const profile = path.join(outDir, ".edge-profile");
await mkdir(outDir, { recursive: true });

const child = spawn(
  edgeBin,
  [
    "--headless=new",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    "about:blank",
  ],
  { stdio: "ignore" },
);

function httpGetJson(pathname) {
  return new Promise((resolve, reject) => {
    http
      .get({ host: "127.0.0.1", port, path: pathname }, (response) => {
        let body = "";
        response.on("data", (chunk) => (body += chunk));
        response.on("end", () => {
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(error);
          }
        });
      })
      .on("error", reject);
  });
}

async function waitForPageTarget(timeoutMs = 20000) {
  const started = Date.now();
  for (;;) {
    try {
      const targets = await httpGetJson("/json");
      const page = targets.find((item) => item.type === "page");
      if (page && page.webSocketDebuggerUrl) return page;
    } catch (error) {
      // 调试端口未就绪，重试
    }
    if (Date.now() - started > timeoutMs) throw new Error("等待 Edge 调试端口超时");
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
}

class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.id && this.pending.has(message.id)) {
        const entry = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
        else entry.resolve(message.result);
      }
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function measureExpression() {
  return `(() => {
    const interactives = Array.from(document.querySelectorAll('button,a[href],input,select,[role="button"]'));
    const items = interactives.map((element) => {
      const rect = element.getBoundingClientRect();
      return {
        tag: element.tagName,
        id: element.id || '',
        testid: element.getAttribute('data-testid') || '',
        w: Number(rect.width.toFixed(1)),
        h: Number(rect.height.toFixed(1)),
        visible: rect.width > 0 && rect.height > 0,
      };
    });
    const visible = items.filter((item) => item.visible);
    const small = visible.filter((item) => item.w < 44 || item.h < 44);
    const scroll = document.scrollingElement || document.documentElement;
    const nav = performance.getEntriesByType('navigation')[0];
    return JSON.stringify({
      innerWidth: window.innerWidth,
      innerHeight: window.innerHeight,
      devicePixelRatio: window.devicePixelRatio,
      scrollWidth: scroll.scrollWidth,
      overflowX: scroll.scrollWidth - window.innerWidth,
      counts: { interactive: items.length, visible: visible.length, under44: small.length },
      renderedSections: document.querySelectorAll("#app section").length,
      htmlLang: document.documentElement.lang,
      hasI18n: Boolean(window.ECSIM_I18N),
      langToggle: Boolean(document.getElementById("lang-toggle")),
      cjk: (document.body.innerText.match(/[\u4e00-\u9fff]+/g) || []).slice(0, 80),
      appTextLength: (document.getElementById("app")?.innerText || "").length,
      under44: small.slice(0, 30),
      paint: performance.getEntriesByType('paint').map((entry) => ({ name: entry.name, startTime: Number(entry.startTime.toFixed(1)) })),
      navigation: nav ? {
        domContentLoaded: Number(nav.domContentLoadedEventEnd.toFixed(1)),
        load: Number(nav.loadEventEnd.toFixed(1)),
        transferSize: nav.transferSize,
      } : null,
      resources: performance.getEntriesByType('resource').map((entry) => ({
        name: entry.name.split('/').pop(),
        duration: Number(entry.duration.toFixed(1)),
        transferSize: entry.transferSize,
      })),
    });
  })()`;
}

let target;
try {
  target = await waitForPageTarget();
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  const cdp = new Cdp(socket);
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");

  const results = [];
  for (const size of SIZES) {
    await cdp.send("Emulation.setDeviceMetricsOverride", {
      width: size.width,
      height: size.height,
      deviceScaleFactor: size.scale,
      mobile: false,
    });
    await cdp.send("Page.navigate", { url });
    await sleep(1500);
    // H3：对 live/API 页面自动点“从 API 载入”，使 8 站实时数据进入渲染后再测量。
    await cdp.send("Runtime.evaluate", { expression: `(() => { const input = document.getElementById("api"); if (input) input.value = location.origin; const button = document.getElementById("load-api"); if (button) button.click(); })()` });
    await sleep(1500);
    const evaluation = await cdp.send("Runtime.evaluate", {
      expression: measureExpression(),
      returnByValue: true,
    });
    const metrics = JSON.parse(evaluation.result.value);
    const shot = await cdp.send("Page.captureScreenshot", { format: "png" });
    const pngPath = path.join(outDir, `${size.label}.png`);
    await writeFile(pngPath, Buffer.from(shot.data, "base64"));
    results.push({ ...size, ...metrics, screenshot: pngPath });
    console.log(
      `OK ${size.label}: overflowX=${metrics.overflowX}px, interactive=${metrics.counts.visible}, under44=${metrics.counts.under44}, FCP=${metrics.paint.map((p) => p.startTime).join("/")}`,
    );
  }
  await writeFile(path.join(outDir, "metrics.json"), JSON.stringify(results, null, 2));
  socket.close();
} finally {
  child.kill("SIGTERM");
  await sleep(300);
  child.kill("SIGKILL");
}
console.log("DONE:", path.join(outDir, "metrics.json"));
