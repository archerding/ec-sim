// EC-Sim 只读状态视图（M2 起点）：仅原生 JS，无第三方依赖。
// 支持四类输入（可分别/同时载入）：
//   - `ec-sim-esc --dump-state` 的 state-v1 JSON（实时模式下含 live 字段）；
//   - `ec-sim-esc --dump-dict --json` 的 dict-v1 JSON；
//   - `ec-sim-esc --serve` 实时模式的 `/pdo` 的 pdo-v1 JSON。
// B42：另含 SDO 面板（`GET /sdo` 只读浏览；`--serve-write` 下勾选+confirm 二次确认写入）。
"use strict";

const app = document.getElementById("app");
const message = document.getElementById("message");

let currentState = null;
let currentDict = null;
let currentPdo = null;
let currentPdoHistory = null;
let currentTopology = null;
let currentMetrics = null;
let liveTimer = null;
let liveSource = null;
let liveBase = "";

function el(tag, attrs, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs || {})) {
    if (key === "class") node.className = value;
    else node.setAttribute(key, value);
  }
  for (const child of children) {
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

function table(headers, rows, onRowClick) {
  const thead = el("thead", {}, el("tr", {}, ...headers.map((h) => el("th", {}, h))));
  const tbody = el(
    "tbody",
    {},
    ...rows.map((row) => {
      const tr = el("tr", {}, ...row.map((cell) => el("td", {}, cell)));
      if (onRowClick) {
        tr.style.cursor = "pointer";
        tr.addEventListener("click", () => onRowClick(row));
      }
      return tr;
    }),
  );
  return el("table", {}, thead, tbody);
}

function selectedSlave() {
  const input = document.getElementById("slave-index");
  const value = Number.parseInt(input.value, 10);
  return Number.isFinite(value) && value >= 0 ? value : 0;
}

function section(title, ...children) {
  return el("section", {}, el("h2", {}, title), ...children);
}

function kvGrid(pairs) {
  return el(
    "div",
    { class: "grid" },
    ...pairs.map(([label, value]) =>
      el("div", { class: "kv" }, el("b", {}, label), el("span", {}, value)),
    ),
  );
}

function renderState(state) {
  if (state.schema !== "ec-sim/state-v1") return;
  const identity = state.identity || {};
  app.append(
    section(
      "身份与画像",
      kvGrid([
        ["Vendor ID", identity.vendor_id || "-"],
        ["Product Code", identity.product_code || "-"],
        ["Revision", identity.revision || "-"],
        ["Serial", identity.serial || "-"],
        ["画像", state.profile ? `${state.profile.id}` : "内置"],
        ["字典对象数", state.dict_objects == null ? "-" : state.dict_objects],
      ]),
      el("p", { class: "sub" }, "名称：", (state.names || []).join(" / ")),
      el(
        "p",
        {},
        el("span", { class: "tag" }, `CoE 0x${Number(state.coe_details).toString(16)}`),
        " ",
        el(
          "span",
          { class: "tag" + (state.mailbox_declared ? "" : " warn") },
          state.mailbox_declared ? "邮箱已声明" : "无邮箱",
        ),
        " ",
        state.dc
          ? el("span", { class: "tag" }, `DC ${state.dc.cycle_time0_ns} ns`)
          : el("span", { class: "tag warn" }, "无 DC"),
      ),
    ),
  );

  if (state.live) {
    const live = state.live;
    app.append(
      section(
        "实时状态（--serve 实时模式）",
        kvGrid([
          ["站地址", live.station_address],
          ["AL 状态", `${live.al_status}${live.al_status_code !== "0x0000" ? ` / ${live.al_status_code}` : ""}`],
          ["DC 模拟时间", `${live.dc_time_ns} ns`],
          ["收/发帧", `${live.stats.frames_in} / ${live.stats.frames_out}`],
          ["丢弃帧", live.stats.frames_dropped],
          ["datagram 命中", live.stats.datagrams_hit],
          ["邮箱请求/响应", `${live.stats.mailbox_requests} / ${live.stats.mailbox_responses}`],
          ["DC 寄存器读/写", `${live.stats.dc_reg_reads} / ${live.stats.dc_reg_writes}`],
        ]),
        el("p", { class: "sub" }, "只读快照；AL 状态码为 0x0000 时省略显示。"),
      ),
    );
  }

  const smRows = (state.sync_managers || []).map((sm, index) => [
    index,
    sm.start,
    sm.length,
    sm.control,
    sm.status,
    sm.enable,
  ]);
  app.append(section("Sync Manager", table(["#", "起始地址", "长度", "Control", "Status", "Enable"], smRows)));

  for (const [label, key] of [
    ["RxPDO（主站 → 从站）", "pdo_rx"],
    ["TxPDO（从站 → 主站）", "pdo_tx"],
  ]) {
    const rows = [];
    for (const pdo of state[key] || []) {
      const entries = (pdo.entries || [])
        .map((entry) => `${entry.index}:${String(entry.sub).padStart(2, "0")} (${entry.bits} 位)`)
        .join(", ");
      rows.push([pdo.index, pdo.sync_manager, String((pdo.entries || []).length), entries]);
    }
    app.append(section(label, table(["PDO", "SM", "条目数", "映射"], rows)));
  }

  if (state.dc) {
    const dc = state.dc;
    app.append(
      section(
        "分布式时钟（DC）",
        kvGrid([
          ["cycleTime0", `${dc.cycle_time0_ns} ns`],
          ["shiftTime0", `${dc.shift_time0_ns} ns`],
          ["shiftTime1", `${dc.shift_time1_ns} ns`],
          ["sync0CycleFactor", dc.sync0_cycle_factor],
          ["sync1CycleFactor", dc.sync1_cycle_factor],
          ["assignActivate", dc.assign_activate],
        ]),
        el("p", { class: "sub" }, "⚠️ Tier0：模拟器只有帧驱动模拟时钟，无真实 SYNC0/漂移补偿。"),
      ),
    );
  }
}

function hexToBigEndian(hex) {
  if (!hex) return "-";
  const bytes = hex.match(/../g) || [];
  return "0x" + bytes.reverse().join("");
}

function renderDict(dict) {
  if (dict.schema !== "ec-sim/dict-v1") return;
  const rows = [];
  for (const object of dict.objects || []) {
    if (!object.entries || object.entries.length === 0) {
      rows.push([object.index, "-", object.name, "", "", "", ""]);
      continue;
    }
    for (const entry of object.entries) {
      rows.push([
        entry.sub === 0 ? object.index : "",
        String(entry.sub).padStart(2, "0"),
        entry.name || object.name,
        entry.type,
        entry.bits,
        entry.access,
        entry.value_hex === "" ? "-" : `${hexToBigEndian(entry.value_hex)} (${entry.value_hex})`,
      ]);
    }
  }
  app.append(
    section(
      "对象字典",
      el(
        "p",
        { class: "sub" },
        `对象 ${dict.object_count} 个 / 条目 ${dict.entry_count} 个（值按小端存储，显示为 0x 大端十六进制）。`,
      ),
      table(["索引", "子索引", "名称", "类型", "位长", "访问", "值"], rows),
    ),
  );
}

function decodeU16(hex) {
  if (!hex) return "-";
  const bytes = hex.match(/../g) || [];
  const words = [];
  for (let i = 0; i + 1 < bytes.length; i += 2) {
    words.push("0x" + bytes[i + 1] + bytes[i]);
  }
  return words.join(" ") || "-";
}

function renderPdo(pdo) {
  if (pdo.schema !== "ec-sim/pdo-v1") return;
  const rows = [];
  for (const [label, key] of [
    ["SM2（RxPDO / 输出）", "sm2"],
    ["SM3（TxPDO / 输入）", "sm3"],
  ]) {
    const item = pdo[key];
    rows.push([label, item.address, String(item.bytes), item.hex || "-", decodeU16(item.hex)]);
  }
  app.append(
    section(
      "过程数据（PDO，只读快照）",
      el(
        "p",
        { class: "sub" },
        `站地址 ${pdo.station_address} / AL ${pdo.al_status}；hex 为小端原始字节，右侧为按 u16 小端解码。`,
      ),
      table(["方向", "地址", "字节数", "hex", "u16 解码"], rows),
    ),
  );
  if (Array.isArray(pdo.decoded) && pdo.decoded.length > 0) {
    const decodedRows = pdo.decoded.map((entry) => [
      entry.dir === "rx" ? "SM2" : "SM3",
      entry.index,
      String(entry.sub).padStart(2, "0"),
      entry.name,
      String(entry.bits),
      String(entry.value),
    ]);
    app.append(
      section(
        "PDO 具名解码（按映射顺序）",
        el(
          "p",
          { class: "sub" },
          "依据 state 的 PDO 映射与 dict-v1 的对象名/类型解码；有符号类型按符号扩展。",
        ),
        table(["方向", "索引", "子索引", "名称", "位长", "值"], decodedRows),
      ),
    );
  }
}


function renderPdoHistory(history) {
  if (history.schema !== "ec-sim/pdo-history-v1") return;
  const samples = history.samples || [];
  const canvas = document.createElement("canvas");
  canvas.width = 760;
  canvas.height = 140;
  canvas.style.width = "100%";
  canvas.style.maxWidth = "760px";
  canvas.style.background = "#0f1115";
  canvas.style.borderRadius = "8px";
  const context = canvas.getContext("2d");
  const values = (key) =>
    samples.map((sample) => {
      const hex = sample[key] || "";
      const bytes = hex.match(/../g) || [];
      return bytes.length >= 2 ? parseInt(bytes[1] + bytes[0], 16) : 0;
    });
  const sm2 = values("sm2_hex");
  const sm3 = values("sm3_hex");
  const all = sm2.concat(sm3, [0]);
  const max = Math.max(1, ...all);
  const draw = (series, color) => {
    context.beginPath();
    context.strokeStyle = color;
    context.lineWidth = 2;
    series.forEach((value, index) => {
      const x = series.length <= 1 ? 0 : (index / (series.length - 1)) * (canvas.width - 20) + 10;
      const y = canvas.height - 12 - (value / max) * (canvas.height - 24);
      if (index === 0) context.moveTo(x, y);
      else context.lineTo(x, y);
    });
    context.stroke();
  };
  draw(sm2, "#6ea8fe");
  draw(sm3, "#8fe3a8");
  app.append(
    section(
      "PDO 历史（最近 " + samples.length + " 个变化采样）",
      el(
        "p",
        { class: "sub" },
        "蓝线 = SM2 前 2 字节（u16 小端）；绿线 = SM3 前 2 字节；纵轴按两者最大值归一化。",
      ),
      canvas,
    ),
  );
}



function alColor(statusHex) {
  const status = Number.parseInt(statusHex, 16) & 0x0f;
  if (status === 8) return "#8fe3a8";
  if (status === 4) return "#b98efc";
  if (status === 2) return "#6ea8fe";
  return "#f0b45a";
}

function renderTopologyGraph(topology) {
  const slaves = topology.slaves || [];
  const svgNs = "http://www.w3.org/2000/svg";
  const width = Math.max(240, slaves.length * 160 + 40);
  const svg = document.createElementNS(svgNs, "svg");
  svg.setAttribute("viewBox", `0 0 ${width} 130`);
  svg.style.width = "100%";
  svg.style.maxWidth = `${width}px`;
  svg.style.background = "#0f1115";
  svg.style.borderRadius = "8px";
  svg.style.marginBottom = "12px";
  const label = (x, y, content, options) => {
    const node = document.createElementNS(svgNs, "text");
    node.setAttribute("x", String(x));
    node.setAttribute("y", String(y));
    node.setAttribute("fill", (options && options.fill) || "#cfd6e4");
    node.setAttribute("font-size", (options && options.size) || "12");
    node.setAttribute("text-anchor", "middle");
    node.textContent = content;
    svg.append(node);
  };
  slaves.forEach((slave, index) => {
    const cx = 60 + index * 160;
    const color = alColor(slave.al_status);
    if (index > 0) {
      const line = document.createElementNS(svgNs, "line");
      line.setAttribute("x1", String(cx - 140));
      line.setAttribute("y1", "60");
      line.setAttribute("x2", String(cx - 18));
      line.setAttribute("y2", "60");
      line.setAttribute("stroke", "#3a4254");
      line.setAttribute("stroke-width", "2");
      svg.append(line);
    }
    const circle = document.createElementNS(svgNs, "circle");
    circle.setAttribute("cx", String(cx));
    circle.setAttribute("cy", "60");
    circle.setAttribute("r", "18");
    circle.setAttribute("fill", color);
    circle.setAttribute("stroke", "#0f1115");
    circle.setAttribute("stroke-width", "2");
    circle.style.cursor = "pointer";
    circle.addEventListener("click", () => {
      document.getElementById("slave-index").value = String(slave.index);
      document.getElementById("load-api").click();
    });
    svg.append(circle);
    label(cx, 64, String(slave.index), { fill: "#0f1115" });
    label(cx, 96, slave.station_address, {});
    label(cx, 112, slave.al_status, { fill: color });
  });
  return svg;
}

function renderTopology(topology) {
  if (topology.schema !== "ec-sim/topology-v1") return;
  const rows = (topology.slaves || []).map((slave) => [
    String(slave.index),
    slave.station_address,
    slave.al_status,
    slave.al_status_code,
    String(slave.frames_in) + " / " + String(slave.frames_out),
    slave.sm2.address + " " + (slave.sm2.hex || "-"),
    slave.sm3.address + " " + (slave.sm3.hex || "-"),
  ]);
  app.append(
    section(
      "总线拓扑（" + String(topology.count) + " 个从站，实时摘要）",
      el(
        "p",
        { class: "sub" },
        "节点色 = AL 状态（INIT 橙 / PREOP 蓝 / SAFEOP 紫 / OP 绿），点击节点切换到该从站；站地址未配置显示 0x0000。",
      ),
      renderTopologyGraph(topology),
      table(
        ["#", "站地址", "AL 状态", "AL 状态码", "收/发帧", "SM2（地址 hex）", "SM3（地址 hex）"],
        rows,
        (row) => {
          document.getElementById("slave-index").value = row[0];
          document.getElementById("load-api").click();
        },
      ),
    ),
  );
}


function parseMetrics(text) {
  const values = {};
  for (const line of String(text).split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const match = line.match(/^([a-zA-Z0-9_]+)\{[^}]*\}\s+([0-9.]+)$/);
    if (match) values[match[1]] = Number(match[2]);
  }
  return values;
}

function renderMetrics(metrics) {
  if (!metrics) return;
  const value = (name) => (metrics[name] == null ? "-" : String(metrics[name]));
  const al = metrics.ecsim_al_status;
  const alClass = al === 8 || al === 4 || al === 2 ? "ok" : "warn";
  const cards = [
    ["收帧", value("ecsim_frames_in_total")],
    ["发帧", value("ecsim_frames_out_total")],
    ["丢弃帧", value("ecsim_frames_dropped_total")],
    ["datagram 命中", value("ecsim_datagrams_hit_total")],
    ["邮箱请求", value("ecsim_mailbox_requests_total")],
    ["邮箱响应", value("ecsim_mailbox_responses_total")],
    ["DC 寄存器读", value("ecsim_dc_reg_reads_total")],
    ["DC 寄存器写", value("ecsim_dc_reg_writes_total")],
    ["DC 模拟时间（ns）", value("ecsim_dc_time_ns")],
    ["AL 状态", al == null ? "-" : String(al)],
    ["AL 状态码", value("ecsim_al_status_code")],
  ];
  app.append(
    section(
      "指标（/metrics）",
      el("p", { class: "sub" }, `AL 状态色：${alClass === "ok" ? "正常（PREOP/SAFEOP/OP）" : "异常或 INIT"}`),
      el(
        "div",
        { class: "grid" },
        ...cards.map(([label, text]) =>
          el("div", { class: "kv" }, el("b", {}, label), el("span", { class: alClass }, text)),
        ),
      ),
    ),
  );
}

function renderAll() {
  app.replaceChildren();
  if (currentState) renderState(currentState);
  if (currentPdo) renderPdo(currentPdo);
  if (currentPdoHistory) renderPdoHistory(currentPdoHistory);
  if (currentTopology) renderTopology(currentTopology);
  if (currentMetrics) renderMetrics(currentMetrics);
  if (currentDict) renderDict(currentDict);
  if (!currentState && !currentDict && !currentPdo) {
    app.append(
      section(
        "载入状态",
        el(
          "p",
          { class: "sub" },
          "请选择 --dump-state / --dump-dict --json 生成的 JSON，或使用上方只读 API（--serve）。",
        ),
      ),
    );
  }
  app.append(el("p", { class: "sub" }, "只读视图，不构成一致性/认证结论。"));
}

function loadMetrics(text, source) {
  currentMetrics = parseMetrics(text);
  message.textContent = `已载入：${source}`;
  renderAll();
}

function loadJson(text, source) {
  try {
    const data = JSON.parse(text);
    if (data.schema === "ec-sim/state-v1") currentState = data;
    else if (data.schema === "ec-sim/dict-v1") currentDict = data;
    else if (data.schema === "ec-sim/pdo-v1") currentPdo = data;
    else if (data.schema === "ec-sim/pdo-history-v1") currentPdoHistory = data;
    else if (data.schema === "ec-sim/topology-v1") currentTopology = data;
    else throw new Error(`未知 schema：${data.schema}`);
    message.textContent = `已载入：${source}`;
    renderAll();
  } catch (error) {
    message.textContent = `解析失败：${error.message}`;
  }
}

function stopLive() {
  if (liveSource) {
    liveSource.close();
    liveSource = null;
  }
  if (liveTimer) {
    clearInterval(liveTimer);
    liveTimer = null;
  }
}

function startLive() {
  stopLive();
  if (!liveBase) return;
  // 优先 SSE（`GET /events`）：状态由服务端推送；PDO/拓扑/指标仍定时拉取（2s）。
  if (typeof window !== "undefined" && window.EventSource) {
    liveSource = new EventSource(`${liveBase}/events?slave=${selectedSlave()}`);
    liveSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.schema === "ec-sim/state-v1") {
          currentState = data;
          message.textContent = `SSE：${liveBase}/events`;
          renderAll();
        }
      } catch (error) {
        message.textContent = `SSE 解析失败：${error.message}`;
      }
    };
    liveSource.onerror = () => {
      message.textContent = "SSE 连接中断（浏览器会自动重连）";
    };
    liveTimer = setInterval(async () => {
      try {
        const slave = selectedSlave();
        const pdoResponse = await fetch(`${liveBase}/pdo?slave=${slave}`);
        if (pdoResponse.ok)
          loadJson(await pdoResponse.text(), `${liveBase}/pdo?slave=${slave} (live)`);
        const historyResponse = await fetch(`${liveBase}/pdo?slave=${slave}&history=1`);
        if (historyResponse.ok)
          loadJson(
            await historyResponse.text(),
            `${liveBase}/pdo?slave=${slave}&history=1 (live)`,
          );
        const topologyResponse = await fetch(`${liveBase}/topology`);
        if (topologyResponse.ok)
          loadJson(await topologyResponse.text(), `${liveBase}/topology (live)`);
        const metricsResponse = await fetch(`${liveBase}/metrics?slave=${slave}`);
        if (metricsResponse.ok)
          loadMetrics(await metricsResponse.text(), `${liveBase}/metrics?slave=${slave} (live)`);
      } catch (error) {
        message.textContent = `实时拉取失败：${error.message}`;
      }
    }, 2000);
    return;
  }
  // 回退：无 EventSource 时用 1s 轮询。
  liveTimer = setInterval(async () => {
    try {
      const slave = selectedSlave();
      const stateResponse = await fetch(`${liveBase}/state?slave=${slave}`);
      if (stateResponse.ok)
        loadJson(await stateResponse.text(), `${liveBase}/state?slave=${slave} (live)`);
      const pdoResponse = await fetch(`${liveBase}/pdo?slave=${slave}`);
      if (pdoResponse.ok)
        loadJson(await pdoResponse.text(), `${liveBase}/pdo?slave=${slave} (live)`);
      const historyResponse = await fetch(`${liveBase}/pdo?slave=${slave}&history=1`);
      if (historyResponse.ok)
        loadJson(
          await historyResponse.text(),
          `${liveBase}/pdo?slave=${slave}&history=1 (live)`,
        );
      const topologyResponse = await fetch(`${liveBase}/topology`);
      if (topologyResponse.ok)
        loadJson(await topologyResponse.text(), `${liveBase}/topology (live)`);
      const metricsResponse = await fetch(`${liveBase}/metrics?slave=${slave}`);
      if (metricsResponse.ok)
        loadMetrics(await metricsResponse.text(), `${liveBase}/metrics?slave=${slave} (live)`);
    } catch (error) {
      message.textContent = `实时刷新失败：${error.message}`;
    }
  }, 1000);
}

document.getElementById("file").addEventListener("change", async (event) => {
  const files = Array.from(event.target.files || []);
  for (const file of files) loadJson(await file.text(), file.name);
});

function tryFetchJson(name) {
  return fetch(name)
    .then((response) => (response.ok ? response.text() : Promise.reject(new Error(String(response.status)))))
    .then((text) => loadJson(text, name))
    .catch(() => {});
}

// 从只读 HTTP API（`ec-sim-esc --serve`）载入 /state、/dict 与 /pdo。
document.getElementById("load-api").addEventListener("click", async () => {
  const base = (document.getElementById("api").value || "").replace(/\/+$/, "");
  try {
    const slave = selectedSlave();
    const stateResponse = await fetch(`${base}/state?slave=${slave}`);
    if (!stateResponse.ok) throw new Error(`/state ${stateResponse.status}`);
    loadJson(await stateResponse.text(), `${base}/state?slave=${slave}`);
    const dictResponse = await fetch(`${base}/dict`);
    if (dictResponse.ok) loadJson(await dictResponse.text(), `${base}/dict`);
    const pdoResponse = await fetch(`${base}/pdo?slave=${slave}`);
    if (pdoResponse.ok) loadJson(await pdoResponse.text(), `${base}/pdo?slave=${slave}`);
    const historyResponse = await fetch(`${base}/pdo?slave=${slave}&history=1`);
    if (historyResponse.ok)
      loadJson(await historyResponse.text(), `${base}/pdo?slave=${slave}&history=1`);
    const topologyResponse = await fetch(`${base}/topology`);
    if (topologyResponse.ok) loadJson(await topologyResponse.text(), `${base}/topology`);
    const metricsResponse = await fetch(`${base}/metrics?slave=${slave}`);
    if (metricsResponse.ok)
      loadMetrics(await metricsResponse.text(), `${base}/metrics?slave=${slave}`);
    liveBase = base;
    if (document.getElementById("live").checked) startLive();
  } catch (error) {
    message.textContent = `API 载入失败：${error.message}`;
  }
});

// 静态服务下尝试自动读取同目录 state.json / dict.json / pdo.json；file:// 下会失败，忽略即可。
Promise.all([tryFetchJson("state.json"), tryFetchJson("dict.json"), tryFetchJson("pdo.json")]).then(() => {
  if (!currentState && !currentDict && !currentPdo) {
    message.textContent =
      "请选择 --dump-state / --dump-dict --json 生成的 JSON，或使用只读 API（--serve）。";
    renderAll();
  }
});

document.getElementById("live").addEventListener("change", (event) => {
  if (event.target.checked) startLive();
  else stopLive();
});

// ---- B42：SDO 面板（`GET /sdo` 只读浏览 + `--serve-write` 下二次确认写入）----
// 说明：本页无法探测服务端是否开启 `--serve-write`，因此写入默认关闭；
// 勾选“允许写入”是第一步，点击“写入…”后的 confirm 对话框是第二步（二次确认）。
// 服务端仍会做白名单/长度/安全排除校验，并以 405/403/404/400 拒绝非法的写请求。

function sdoApiBase() {
  return (document.getElementById("api").value || "").trim().replace(/\/+$/, "");
}

function setSdoMessage(id, text, isError) {
  const node = document.getElementById(id);
  if (!node) return;
  node.textContent = text;
  node.className = isError ? "sub error" : "sub";
}

function parseSdoIndex(raw) {
  const text = String(raw || "").trim();
  // 面板标注为十六进制；允许 0x 前缀，也允许裸 hex（如 6040 → 0x6040）。
  if (/^0x[0-9a-f]{1,4}$/i.test(text)) return Number.parseInt(text.slice(2), 16);
  if (/^[0-9a-f]{1,4}$/i.test(text)) return Number.parseInt(text, 16);
  return null;
}

function parseSdoSub(raw) {
  const text = String(raw || "").trim();
  if (text === "") return 0;
  if (!/^\d{1,3}$/.test(text)) return null;
  const value = Number.parseInt(text, 10);
  return value >= 0 && value <= 255 ? value : null;
}

function sdoQuery() {
  const index = parseSdoIndex(document.getElementById("sdo-index").value);
  if (index == null) {
    setSdoMessage("sdo-status", "索引非法：请输入十六进制 0x0000–0xFFFF（可省略 0x）。", true);
    return null;
  }
  const sub = parseSdoSub(document.getElementById("sdo-sub").value);
  if (sub == null) {
    setSdoMessage("sdo-status", "子索引非法：0–255。", true);
    return null;
  }
  return { index, sub, query: `?index=0x${index.toString(16).padStart(4, "0").toUpperCase()}&sub=${sub}` };
}

function hexToBytes(hex) {
  const text = String(hex || "").replace(/\s+/g, "");
  if (!text || !/^[0-9a-f]+$/i.test(text) || text.length % 2 !== 0) return null;
  const bytes = [];
  for (let i = 0; i < text.length; i += 2) bytes.push(Number.parseInt(text.slice(i, i + 2), 16));
  return bytes;
}

function renderSdoResult(data) {
  const container = document.getElementById("sdo-result");
  container.replaceChildren();
  if (!data || data.schema !== "ec-sim/sdo-v1") {
    container.append(el("p", { class: "sub error" }, "响应不是 ec-sim/sdo-v1。"));
    return;
  }
  const rows = [
    ["索引", `${data.index}（子索引 ${String(data.sub).padStart(2, "0")}）`],
    ["对象名", data.object_name || "-"],
    ["条目名", data.name || "-"],
    ["类型", data.type],
    ["位长", `${data.bits} 位`],
    ["访问", data.access],
    ["值（大端显示）", hexToBigEndian(data.value_hex)],
    ["值（原始小端 hex）", data.value_hex === "" ? "-" : data.value_hex],
  ];
  container.append(table(["字段", "值"], rows));
}

async function sdoFailureHint(response) {
  let detail = "";
  try {
    const data = JSON.parse(await response.text());
    detail = [data.error, data.hint].filter(Boolean).join(" / ");
  } catch (error) {
    detail = "";
  }
  if (response.status === 401) {
    return "401：需要 Authorization: Bearer token。浏览器 fetch 无法自动带 token，请经反向代理注入或改用 curl（--serve-token-env 模式）。";
  }
  if (response.status === 403) return `403：服务端拒绝${detail ? `（${detail}）` : ""}`;
  if (response.status === 404) return `404：对象/子索引不存在或参数缺失${detail ? `（${detail}）` : ""}`;
  if (response.status === 405) return `405：写操作未启用${detail ? `（${detail}）` : ""}——服务端需 --serve-write`;
  return `${response.status}：${detail || response.statusText || "请求失败"}`;
}

async function readSdo() {
  const base = sdoApiBase();
  const parsed = sdoQuery();
  if (!parsed) return;
  if (!base) {
    setSdoMessage("sdo-status", "请先在上方填写 API 地址（--serve）并载入。", true);
    return;
  }
  setSdoMessage("sdo-status", `读取中：/sdo${parsed.query}`, false);
  try {
    const response = await fetch(`${base}/sdo${parsed.query}`, { cache: "no-store" });
    if (!response.ok) {
      setSdoMessage("sdo-status", await sdoFailureHint(response), true);
      return;
    }
    const data = JSON.parse(await response.text());
    renderSdoResult(data);
    setSdoMessage(
      "sdo-status",
      `已读取 ${data.index}:${String(data.sub).padStart(2, "0")} ${data.name || ""}`.trim(),
      false,
    );
  } catch (error) {
    setSdoMessage("sdo-status", `读取失败：${error.message}`, true);
  }
}

async function writeSdo() {
  const base = sdoApiBase();
  const parsed = sdoQuery();
  if (!parsed) return;
  if (!base) {
    setSdoMessage("sdo-write-status", "请先在上方填写 API 地址（--serve）并载入。", true);
    return;
  }
  if (!document.getElementById("sdo-write-enable").checked) {
    setSdoMessage("sdo-write-status", "写入未开启：请先勾选“允许写入”。", true);
    return;
  }
  const bytes = hexToBytes(document.getElementById("sdo-write-value").value);
  if (bytes == null || bytes.length === 0) {
    setSdoMessage("sdo-write-status", "新值非法：需要偶数个十六进制字符（小端原始字节，如 0f00）。", true);
    return;
  }
  const written = bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
  // 二次确认：勾选“允许写入”是第一步，此 confirm 是第二步。
  const confirmed = window.confirm(
    `确认写入对象 ${parsed.index.toString(16).toUpperCase().padStart(4, "0")}h:${String(parsed.sub).padStart(2, "0")}？\n` +
      `新值（小端 hex）：${written}\n\n` +
      "该操作仅在服务端以 --serve-write 启动时生效；写入会记入 /sdo-audit。",
  );
  if (!confirmed) {
    setSdoMessage("sdo-write-status", "已取消写入。", false);
    return;
  }
  setSdoMessage("sdo-write-status", "写入中…", false);
  try {
    const response = await fetch(`${base}/sdo${parsed.query}`, {
      method: "POST",
      body: new Uint8Array(bytes),
      cache: "no-store",
    });
    if (!response.ok) {
      setSdoMessage("sdo-write-status", await sdoFailureHint(response), true);
      return;
    }
    const data = JSON.parse(await response.text());
    setSdoMessage(
      "sdo-write-status",
      `写入成功：${data.old_hex || "-"} → ${data.written_hex}（审计见 GET /sdo-audit）`,
      false,
    );
    await readSdo();
  } catch (error) {
    setSdoMessage("sdo-write-status", `写入失败：${error.message}`, true);
  }
}

document.getElementById("sdo-read").addEventListener("click", () => {
  void readSdo();
});
document.getElementById("sdo-index").addEventListener("keydown", (event) => {
  if (event.key === "Enter") void readSdo();
});
document.getElementById("sdo-write-enable").addEventListener("change", (event) => {
  document.getElementById("sdo-write-fields").style.display = event.target.checked ? "" : "none";
  setSdoMessage(
    "sdo-write-status",
    event.target.checked ? "写入已允许：点击“写入…”后还会再次确认。" : "",
    false,
  );
});
document.getElementById("sdo-write").addEventListener("click", () => {
  void writeSdo();
});
