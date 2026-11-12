// EC-Sim 只读状态视图（M2 起点）：仅原生 JS，无第三方依赖。
// 输入：`ec-sim-esc --dump-state` 输出的 state-v1 JSON（见 docs/DEVICE-PLUGINS.md / bin）。
"use strict";

const app = document.getElementById("app");
const message = document.getElementById("message");

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
  const tbody = el("tbody", {}, ...rows.map((row) => el("tr", {}, ...row.map((cell) => el("td", {}, cell)))));
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

function render(state) {
  app.replaceChildren();

  if (state.schema !== "ec-sim/state-v1") {
    app.append(el("section", {}, el("div", { class: "error" }, `未知 schema：${state.schema}`)));
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

  for (const [label, key] of [["RxPDO（主站 → 从站）", "pdo_rx"], ["TxPDO（从站 → 主站）", "pdo_tx"]]) {
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

  app.append(el("p", { class: "sub" }, `由 ${state.tool} 生成；只读视图，不构成一致性/认证结论。`));
}

async function loadState(text, source) {
  try {
    const state = JSON.parse(text);
    message.textContent = `已载入：${source}`;
    render(state);
  } catch (error) {
    message.textContent = `解析失败：${error.message}`;
  }
}

document.getElementById("file").addEventListener("change", async (event) => {
  const file = event.target.files[0];
  if (file) loadState(await file.text(), file.name);
});

// 若页面由静态服务器提供，则尝试同目录 state.json；file:// 下 fetch 会失败，忽略即可。
fetch("state.json")
  .then((response) => (response.ok ? response.text() : Promise.reject(new Error(String(response.status)))))
  .then((text) => loadState(text, "state.json"))
  .catch(() => {
    if (!message.textContent) message.textContent = "请选择 --dump-state 生成的 JSON 文件（或放置 web/state.json）。";
  });
