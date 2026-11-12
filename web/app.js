// EC-Sim 只读视图（M2 起点）：仅原生 JS，无第三方依赖。
// 支持四类输入（可分别/同时载入）：
//   - `ec-sim-esc --dump-state` 的 state-v1 JSON（实时模式下含 live 字段）；
//   - `ec-sim-esc --dump-dict --json` 的 dict-v1 JSON；
//   - `ec-sim-esc --serve` 实时模式的 `/pdo` 的 pdo-v1 JSON。
"use strict";

const app = document.getElementById("app");
const message = document.getElementById("message");

let currentState = null;
let currentDict = null;
let currentPdo = null;
let currentPdoHistory = null;
let liveTimer = null;
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

function table(headers, rows) {
  const thead = el("thead", {}, el("tr", {}, ...headers.map((h) => el("th", {}, h))));
  const tbody = el(
    "tbody",
    {},
    ...rows.map((row) => el("tr", {}, ...row.map((cell) => el("td", {}, cell)))),
  );
  return el("table", {}, thead, tbody);
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

function renderAll() {
  app.replaceChildren();
  if (currentState) renderState(currentState);
  if (currentPdo) renderPdo(currentPdo);
  if (currentPdoHistory) renderPdoHistory(currentPdoHistory);
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

function loadJson(text, source) {
  try {
    const data = JSON.parse(text);
    if (data.schema === "ec-sim/state-v1") currentState = data;
    else if (data.schema === "ec-sim/dict-v1") currentDict = data;
    else if (data.schema === "ec-sim/pdo-v1") currentPdo = data;
    else if (data.schema === "ec-sim/pdo-history-v1") currentPdoHistory = data;
    else throw new Error(`未知 schema：${data.schema}`);
    message.textContent = `已载入：${source}`;
    renderAll();
  } catch (error) {
    message.textContent = `解析失败：${error.message}`;
  }
}

function stopLive() {
  if (liveTimer) {
    clearInterval(liveTimer);
    liveTimer = null;
  }
}

function startLive() {
  stopLive();
  if (!liveBase) return;
  liveTimer = setInterval(async () => {
    try {
      const stateResponse = await fetch(`${liveBase}/state`);
      if (stateResponse.ok) loadJson(await stateResponse.text(), `${liveBase}/state (live)`);
      const pdoResponse = await fetch(`${liveBase}/pdo`);
      if (pdoResponse.ok) loadJson(await pdoResponse.text(), `${liveBase}/pdo (live)`);
      const historyResponse = await fetch(`${liveBase}/pdo?history=1`);
      if (historyResponse.ok)
        loadJson(await historyResponse.text(), `${liveBase}/pdo?history=1 (live)`);
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
    const stateResponse = await fetch(`${base}/state`);
    if (!stateResponse.ok) throw new Error(`/state ${stateResponse.status}`);
    loadJson(await stateResponse.text(), `${base}/state`);
    const dictResponse = await fetch(`${base}/dict`);
    if (dictResponse.ok) loadJson(await dictResponse.text(), `${base}/dict`);
    const pdoResponse = await fetch(`${base}/pdo`);
    if (pdoResponse.ok) loadJson(await pdoResponse.text(), `${base}/pdo`);
    const historyResponse = await fetch(`${base}/pdo?history=1`);
    if (historyResponse.ok) loadJson(await historyResponse.text(), `${base}/pdo?history=1`);
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
