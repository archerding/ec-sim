// G-19 共享扫描器：en 模式下"载入数据后"的 innerText CJK 残留（双方同一口径）。
// 流程：headless Edge → 打开 ?lang=en → 等待自动载入 → 模拟"从 API 载入"（填 origin + 点击）→
//       等 MutationObserver 静默 1.5s → 取 body.innerText 扫 CJK → 输出 JSON（残留行 + 白名单判断）。
// 用法：node scripts/spikes/hmi-en-cjk-scan.mjs <url> [outDir]
// 退出码：0 = 仅白名单（中文）；1 = 有残留（打印逐行）。
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import http from "node:http";
import path from "node:path";

const url = process.argv[2];
const outDir = process.argv[3] || "/tmp/ecsim-cjk-scan";
const edgeBin = process.env.EDGE_BIN || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
if (!url) {
  console.error("usage: node hmi-en-cjk-scan.mjs <url> [outDir]");
  process.exit(2);
}
const WHITELIST = ["中文"];
await mkdir(outDir, { recursive: true });
const port = 9444;
const child = spawn(edgeBin, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${path.join(outDir, ".edge")}`, "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function getJson(pathname) {
  return new Promise((resolve, reject) => {
    http.get({ host: "127.0.0.1", port, path: pathname }, (response) => {
      let body = "";
      response.on("data", (chunk) => (body += chunk));
      response.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    }).on("error", reject);
  });
}

async function pageTarget() {
  for (let index = 0; index < 100; index += 1) {
    try {
      const targets = await getJson("/json");
      const page = targets.find((item) => item.type === "page");
      if (page) return page;
    } catch (error) {
      // 端口未就绪
    }
    await sleep(150);
  }
  throw new Error("Edge 调试端口超时");
}

class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.id = 1;
    this.pending = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const entry = this.pending.get(message.id);
      if (!entry) return;
      this.pending.delete(message.id);
      if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
      else entry.resolve(message.result);
    });
  }
  send(method, params = {}) {
    const id = this.id++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
}

try {
  const target = await pageTarget();
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  const cdp = new Cdp(socket);
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  await cdp.send("Page.navigate", { url });
  await sleep(2000);
  // 同一页面：填写 API 地址 = 当前 origin，点击"从 API 载入"，再等渲染稳定。
  await cdp.send("Runtime.evaluate", {
    expression: `(() => { const input = document.getElementById("api"); if (!input) return "no-api-input"; input.value = location.origin; const button = document.getElementById("load-api"); if (!button) return "no-load-button"; button.click(); return "clicked"; })()`,
    returnByValue: true,
  });
  await sleep(2000);
  const evaluation = await cdp.send("Runtime.evaluate", {
    expression: `(() => {
      const text = document.body.innerText || "";
      const cjk = (text.match(/[\\u4e00-\\u9fff]+/g) || []);
      const lines = text.split("\\n").filter((line) => /[\\u4e00-\\u9fff]/.test(line)).slice(0, 80);
      return JSON.stringify({ lang: window.ECSIM_I18N ? window.ECSIM_I18N.lang : null, cjk, lines });
    })()`,
    returnByValue: true,
  });
  const report = JSON.parse(evaluation.result.value);
  const allowed = new Set(WHITELIST);
  const unexpected = report.cjk.filter((item) => !allowed.has(item));
  report.unexpected = unexpected;
  report.unexpected_lines = report.lines.filter((line) => !line.split(/[\u4e00-\u9fff]+/).every((part) => WHITELIST.includes(part.trim()) || !/[\u4e00-\u9fff]/.test(part)));
  const reportPath = path.join(outDir, "en-cjk-report.json");
  await writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(`lang=${report.lang} cjk_groups=${report.cjk.length} unexpected=${unexpected.length} report=${reportPath}`);
  if (unexpected.length > 0) {
    console.log("UNEXPECTED_CJK:", JSON.stringify(unexpected));
    console.log("LINES:", JSON.stringify(report.unexpected_lines.slice(0, 20)));
    process.exitCode = 1;
  } else {
    console.log("OK: en 载入后 CJK 仅白名单", JSON.stringify(WHITELIST));
  }
  socket.close();
} finally {
  child.kill("SIGTERM");
  await sleep(200);
  child.kill("SIGKILL");
}
