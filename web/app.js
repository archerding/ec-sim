// EC-Sim 只读状态视图（M2 起点）：仅原生 JS，无第三方依赖。
// 支持两类输入（也可同时载入，先状态后字典）：
//   - `ec-sim-esc --dump-state` 的 state-v1 JSON；
//   - `ec-sim-esc --dump-dict --json` 的 dict-v1 JSON。
"use strict";

const app = document.getElementById("app");
const message = document.getElementById("message");

let currentState = null;
let currentDict = null;

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
  if (state.schema !== "ec-sim/state-v1") {
    app.append(section("状态", el("div", { class: "error" }, `未知 schema：${state.schema}`)));
    return;
  }
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
  if (dict.schema !== "ec-sim/dict-v1") {
    app.append(section("对象字典", el("div", { class: "error" }, `未知 schema：${dict.schema}`)));
    return;
  }
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

function renderAll() {
  app.replaceChildren();
  if (currentState) renderState(currentState);
  if (currentDict) renderDict(currentDict);
  if (!currentState && !currentDict) {
    app.append(
      section(
        "载入状态",
        el("p", { class: "sub" }, "请选择 --dump-state / --dump-dict --json 生成的 JSON（可分别载入，先状态后字典）。"),
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
    else throw new Error(`未知 schema：${data.schema}`);
    message.textContent = `已载入：${source}`;
    renderAll();
  } catch (error) {
    message.textContent = `解析失败：${error.message}`;
  }
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

// 静态服务下尝试自动读取同目录 state.json / dict.json；file:// 下会失败，忽略即可。
Promise.all([tryFetchJson("state.json"), tryFetchJson("dict.json")]).then(() => {
  if (!currentState && !currentDict) {
    message.textContent =
      "请选择 --dump-state（state-v1）或 --dump-dict --json（dict-v1）生成的 JSON，或放置 web/state.json、web/dict.json。";
    renderAll();
  }
});
