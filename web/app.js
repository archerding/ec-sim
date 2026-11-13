// EC-Sim 只读状态视图（M2 起点）：仅原生 JS，无第三方依赖。
// 支持四类输入（可分别/同时载入）：
//   - `ec-sim-esc --dump-state` 的 state-v1 JSON（实时模式下含 live 字段）；
//   - `ec-sim-esc --dump-dict --json` 的 dict-v1 JSON；
//   - `ec-sim-esc --serve` 实时模式的 `/pdo` 的 pdo-v1 JSON。
// B42：另含 SDO 面板（`GET /sdo` 只读浏览；`--serve-write` 下勾选+confirm 二次确认写入）。
"use strict";

const app = document.getElementById("app");
// DOD-09：动态文案翻译（无 i18n.js 时回退中文）。
const tr = (key, fallback) => (window.ECSIM_I18N ? window.ECSIM_I18N.t(key) : fallback);
const message = document.getElementById("message");

let currentState = null;
let currentDict = null;
let currentPdo = null;
let currentPdoHistory = null;
let currentTopology = null;
let currentMetrics = null;
let currentAudit = null;
let liveBase = "";

// ---- DOD-03（G-09）连接状态机（页面级）----
// IDLE → CONNECTING → LIVE_SSE ⇄ LIVE_POLL → STOPPED；另有 STALE / PAUSED / RECONNECTING。
// 规格来源：ECSIM-HMI-GO-B-DOD §3.2/§3.3。任何非 LIVE_* 状态都不得显示 "LIVE" 字样。
const conn = {
  mode: "IDLE", // IDLE | CONNECTING | LIVE_SSE | LIVE_POLL | STALE | STOPPED | PAUSED | RECONNECTING
  lastDataAt: null, // 最后一次**真实数据**到达时间（ms）
  lastMode: "LIVE_SSE", // 恢复时回到 SSE 还是轮询
  failures: 0,
  paused: false,
  hidden: false,
  timer: null, // 唯一轮询 interval
  heartbeat: null, // 唯一心跳 interval（数据时间/Δ/STALE 判定）
  source: null, // 唯一 EventSource
  staleTimer: null, // 单次 STALE 探测（不用 setInterval）
  pollIntervalMs: 1000,
  sseIntervalMs: 500,
  maxFailures: 3,
  requestSeq: 0, // 丢弃过期响应（切站/重连时）
};
const MAX_DELTA_MS = 2 * 1000; // 2× 刷新间隔无新数据 → STALE

// V-04：对象字典筛选/L1 抽屉状态。
const dictFilters = { search: "", access: "all", value: "all", selected: null };

// V-06/V-07：证据导出与关于页状态。
const evidenceState = {
  selected: {
    identity: true,
    al: true,
    pdo: true,
    history: true,
    topology: true,
    metrics: true,
    dict: true,
    audit: true,
  },
};
let aboutHealth = null;

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
  return tableWithTestidRows(headers, rows, null, onRowClick);
}

// DOD-01：带行级 `data-testid="<prefix><index>"` 的表格（参数化 testid 模板）。
function tableWithTestidRows(headers, rows, rowTestidPrefix, onRowClick) {
  const thead = el("thead", {}, el("tr", {}, ...headers.map((h) => el("th", {}, h))));
  const tbody = el(
    "tbody",
    {},
    ...rows.map((row, index) => {
      const tr = el("tr", {}, ...row.map((cell) => el("td", {}, cell)));
      if (rowTestidPrefix) tr.setAttribute("data-testid", `${rowTestidPrefix}${index}`);
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

// ---- DOD-13：AL 派生（state = al_status & 0x0F；bit4 error；bit5 ID 请求）----
const AL_STATE_NAMES = { 1: "INIT", 2: "PREOP", 4: "SAFEOP", 8: "OP" };

function deriveAl(statusHex) {
  const raw = Number.parseInt(statusHex, 16);
  const value = Number.isFinite(raw) ? raw : 0;
  const state = value & 0x0f;
  const error = (value & 0x10) !== 0;
  const idRequest = (value & 0x20) !== 0;
  const stateName = AL_STATE_NAMES[state] || `状态 ${state}`;
  let severity = "info";
  if (error) severity = "error";
  else if (idRequest) severity = "warn";
  else if (state === 8) severity = "ok";
  else if (state === 4) severity = "info";
  else if (state === 2) severity = "info";
  return { raw: value, state, stateName, error, idRequest, severity };
}

function alChip(statusHex) {
  const derived = deriveAl(statusHex);
  const suffix = [derived.error ? "error" : null, derived.idRequest ? "id-req" : null]
    .filter(Boolean)
    .join(" · ");
  const chip = el(
    "span",
    { class: "al-chip", "data-sev": derived.severity },
    `${derived.stateName}${suffix ? `（${suffix}）` : ""}`,
  );
  chip.title = `al_status=${statusHex}（state=${derived.stateName}；error=${derived.error}；id_request=${derived.idRequest}）`;
  return chip;
}

// ---- DOD-03：连接状态 UI（徽标/横幅/数据时间/陈旧标记）----
function connBadgeText(mode) {
  switch (mode) {
    case "LIVE_SSE":
      return tr("conn.liveSse", "LIVE · SSE");
    case "LIVE_POLL":
      return tr("conn.livePoll", "LIVE · 轮询");
    case "STALE":
      return tr("conn.stale", "STALE（数据陈旧）");
    case "STOPPED":
      return tr("conn.stopped", "未连接");
    case "PAUSED":
      return tr("conn.paused", "已暂停");
    case "CONNECTING":
      return tr("conn.connecting", "连接中…");
    case "RECONNECTING":
      return tr("conn.reconnecting", "重连中…");
    default:
      return tr("conn.idle", "未连接");
  }
}

function ageText() {
  if (!conn.lastDataAt) return "-";
  const age = Math.max(0, Date.now() - conn.lastDataAt);
  return age < 1000 ? `${age} ms 前` : `${(age / 1000).toFixed(1)} s 前`;
}

// 脱敏：只显示 host:port（不含 scheme/路径/token/凭据）。
function sourceHostPort() {
  if (!liveBase) return "";
  try {
    const url = new URL(liveBase);
    return url.host;
  } catch (error) {
    return "";
  }
}

function updateShellContext() {
  const source = document.getElementById("api-source");
  if (source) {
    const hostPort = sourceHostPort();
    source.textContent = `数据源：${hostPort || "-"}`;
  }
  const address = document.querySelector('[data-testid="shell-context-address"]');
  const alChipNode = document.querySelector('[data-testid="shell-context-al-chip"]');
  const profile = document.querySelector('[data-testid="shell-context-profile"]');
  const live = currentState && currentState.live;
  if (address) {
    address.textContent = `站地址：${live && live.station_address ? live.station_address : "-"}`;
  }
  if (alChipNode) {
    alChipNode.replaceChildren();
    if (live) {
      alChipNode.append("AL：", alChip(live.al_status));
    } else {
      alChipNode.textContent = "AL：-";
    }
  }
  if (profile) {
    profile.textContent = `画像：${
      currentState && currentState.profile ? currentState.profile.id : currentState ? "内置" : "-"
    }`;
  }
}

function updateConnUi() {
  const badge = document.getElementById("conn-badge");
  const dataTime = document.getElementById("data-time");
  const pauseButton = document.getElementById("pause-toggle");
  const writeButton = document.getElementById("sdo-write");
  if (badge) {
    badge.dataset.mode = conn.mode;
    badge.textContent = connBadgeText(conn.mode);
    // 非 LIVE_* 状态不得出现 "LIVE" 字样（DOD-03 §3.3-1）。
    if (conn.mode !== "LIVE_SSE" && conn.mode !== "LIVE_POLL") {
      badge.textContent = badge.textContent.replace(/LIVE/g, "非实时");
    }
  }
  if (dataTime) {
    dataTime.textContent = `数据时间：${conn.lastDataAt ? new Date(conn.lastDataAt).toLocaleTimeString() : "-"}（${ageText()}）`;
  }
  if (pauseButton) {
    pauseButton.setAttribute("aria-pressed", conn.paused ? "true" : "false");
    const i18n = window.ECSIM_I18N;
    pauseButton.textContent = conn.paused
      ? i18n ? i18n.t("topbar.resume") : "继续"
      : i18n ? i18n.t("topbar.pause") : "暂停";
  }
  if (writeButton) {
    // STOPPED 下写按钮禁用并给 E-10 说明（§3.3-6）。
    writeButton.disabled = conn.mode === "STOPPED";
  }
  const banner = document.getElementById("conn-banner");
  if (banner) {
    let text = "";
    let level = "error";
    if (conn.mode === "LIVE_POLL") {
      text =
        tr("banner.livePoll", "E-10：SSE 推送已中断，页面已降级为轮询（LIVE · 轮询）；数据可能落后 1 个刷新周期。恢复后自动回到 LIVE · SSE。");
      level = "warn";
    } else if (conn.mode === "STOPPED") {
      text =
        tr("banner.stopped", "E-10：连续 3 次请求失败，已停止自动刷新（未连接）。请确认服务已启动后点“重新连接”；当前显示的是最后一次数据，可能已过期。");
    } else if (conn.mode === "STALE") {
      text = tr("banner.stale", "数据陈旧（STALE）：超过 2 个刷新周期没有新数据；以下所有数值都带陈旧标记，请勿当作实时值。");
      level = "warn";
    } else if (conn.mode === "PAUSED") {
      text = tr("banner.paused", "已暂停：页面停止发请求，显示最后一次数据与时间戳；点“继续”会立即拉取一次。");
      level = "info";
    } else if (conn.mode === "RECONNECTING") {
      text = tr("banner.reconnecting", "正在重连：已发起一次探测请求…");
      level = "info";
    }
    banner.setAttribute("data-testid", `shell-banner-${level}`);
    banner.textContent = text;
    banner.classList.toggle("visible", text !== "");
  }
  updateShellContext();
  // 陈旧标记覆盖所有依赖值（PDO/AL/拓扑/指标/数据时间）。
  document.body.classList.toggle("stale", conn.mode === "STALE");
}

function noteData() {
  conn.lastDataAt = Date.now();
  conn.failures = 0;
  if (conn.mode === "LIVE_SSE" || conn.mode === "LIVE_POLL" || conn.mode === "STALE") {
    if (conn.mode === "STALE") conn.mode = conn.lastMode;
  }
  updateConnUi();
}

function setConnMode(mode) {
  if (conn.mode === mode) {
    updateConnUi();
    return;
  }
  conn.mode = mode;
  if (mode === "LIVE_SSE" || mode === "LIVE_POLL") {
    conn.lastMode = mode;
  }
  updateConnUi();
}

function setStaticModeNotice(visible) {
  const notice = document.getElementById("mode-notice");
  if (!notice) return;
  notice.classList.toggle("visible", visible);
  notice.textContent = visible
    ? tr("notice.static", "静态快照模式：当前 state 没有 live 字段（来自 --dump-state 的静态快照或文件载入），不提供实时/离线数据刷新。需要实时视图请用 ec-sim-esc --serve <addr> --uds <sock>（或 --tap）启动同进程实时模式。")
    : "";
}

function section(title, ...children) {
  return el("section", {}, el("h2", {}, title), ...children);
}

// DOD-01：带 data-testid 的 section（契约钩子，不改变显示）。
function sectionT(testid, title, ...children) {
  return el("section", { "data-testid": testid }, el("h2", {}, title), ...children);
}

function toast(text) {
  const node = document.getElementById("toast");
  if (!node) return;
  node.textContent = text;
  node.classList.add("visible");
  if (node._toastTimer) clearTimeout(node._toastTimer);
  node._toastTimer = setTimeout(() => node.classList.remove("visible"), 2500);
}

function copyText(text, label) {
  const done = () => toast(`${label || "已复制"}：${String(text).slice(0, 60)}`);
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(String(text)).then(done, () => toast("复制失败（权限/非安全上下文）"));
  } else {
    toast("复制不可用（浏览器未提供 clipboard）");
  }
}

function summaryText() {
  const live = currentState && currentState.live;
  const parts = [
    `EC-Sim 摘要 ${new Date().toISOString()}`,
    `站=${selectedSlave()} 模式=${connBadgeText(conn.mode)} 数据=${ageText()}`,
    `API=${sourceHostPort() || "-"}`,
  ];
  if (live) {
    const derived = deriveAl(live.al_status);
    parts.push(`AL=${derived.stateName} error=${derived.error} code=${live.al_status_code}`);
  }
  if (currentState && currentState.identity) {
    parts.push(`VID=${currentState.identity.vendor_id} PID=${currentState.identity.product_code}`);
  }
  return parts.join(" | ");
}

function currentExport() {
  return {
    schema: "ecsim-web-export-v1",
    generated_at: new Date().toISOString(),
    mode: connBadgeText(conn.mode),
    api: liveBase || null,
    slave: selectedSlave(),
    state: currentState,
    pdo: currentPdo,
    topology: currentTopology,
    metrics: currentMetrics,
    dict: currentDict,
    audit: currentAudit,
    boundaries: ["快照语义，非实时 SLA", "不宣称一致性/CTT/认证", "0x18 维持 NOT-JUDGED（BL-20 v1.x）"],
  };
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
    sectionT(
      "slave-identity",
      tr("section.slaveIdentity", "身份与画像"),
      kvGrid([
        ["Vendor ID", identity.vendor_id || "-"],
        ["Product Code", identity.product_code || "-"],
        ["Revision", identity.revision || "-"],
        ["Serial", identity.serial || "-"],
        ["画像", state.profile ? `${state.profile.id}` : "内置"],
        ["字典对象数", state.dict_objects == null ? "-" : state.dict_objects],
      ]),
      el(
        "p",
        { class: "sub", "data-testid": "slave-sii" },
        "名称：",
        (state.names || []).join(" / "),
        " · 画像：",
        el(
          "span",
          { "data-testid": "slave-profile" },
          state.profile ? `${state.profile.id}` : "内置",
        ),
      ),
      el(
        "p",
        {},
        el("button", { type: "button", "data-testid": "slave-goto-pdo" }, "查看 PDO 映射"),
        " ",
        el("button", { type: "button", "data-testid": "slave-copy-identity" }, "复制身份"),
      ),
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
      sectionT(
        "slave-live",
        tr("section.slaveLive", "实时状态（--serve 实时模式）"),
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
        el(
          "p",
          {},
          "AL 派生（DOD-13）：",
          alChip(live.al_status),
          " ",
          el("span", { class: "sub" }, `al_status_code=${live.al_status_code}`),
        ),
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
  app.append(sectionT("slave-sm-table", tr("section.sm", "Sync Manager"), table(["#", "起始地址", "长度", "Control", "Status", "Enable"], smRows)));

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
    app.append(sectionT(key === "pdo_rx" ? "slave-pdo-rx" : "slave-pdo-tx", label, table(["PDO", "SM", "条目数", "映射"], rows)));
  }

  if (state.dc) {
    const dc = state.dc;
    app.append(
      sectionT(
        "slave-dc",
        tr("section.dc", "分布式时钟（DC）"),
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
  // V-04：搜索 / 访问过滤 / 有值过滤 / L1 抽屉（H2 实现；DOM 交互仍由浏览器走查判定）。
  const search = (dictFilters.search || "").trim().toLowerCase();
  const matches = (object, entry) => {
    const index = (entry && entry.index) || object.index;
    const name = ((entry && (entry.name || object.name)) || "").toLowerCase();
    if (search && !`${index} ${name}`.toLowerCase().includes(search)) return false;
    const access = entry ? entry.access : "";
    if (dictFilters.access === "ro" && access !== "ro") return false;
    if (dictFilters.access === "rw" && access !== "rw") return false;
    const hasValue = Boolean(entry && entry.value_hex);
    if (dictFilters.value === "with" && !hasValue) return false;
    if (dictFilters.value === "without" && hasValue) return false;
    return true;
  };
  const rows = [];
  for (const object of dict.objects || []) {
    if (!object.entries || object.entries.length === 0) {
      if (matches(object, null)) {
        rows.push([object.index, "-", object.name, "", "", "", ""]);
      }
      continue;
    }
    for (const entry of object.entries) {
      if (!matches(object, entry)) continue;
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
  const filterButton = (testid, label, active, onClick) =>
    el(
      "button",
      {
        type: "button",
        "data-testid": testid,
        "aria-pressed": active ? "true" : "false",
        style: active ? "background:#2c3a58" : "",
      },
      label,
      {},
    );
  const toolbar = el(
    "div",
    { class: "row", style: "margin-bottom:8px" },
    el("input", {
      id: "dict-search",
      "data-testid": "dict-search",
      type: "text",
      placeholder: "搜索索引/名称（/ 聚焦）",
      value: dictFilters.search,
    }),
    filterButton("dict-filter-access", `访问=全部`, dictFilters.access === "all", () => {
      dictFilters.access = "all";
      renderAll();
    }),
    filterButton("dict-filter-access", `ro`, dictFilters.access === "ro", () => {
      dictFilters.access = "ro";
      renderAll();
    }),
    filterButton("dict-filter-access", `rw`, dictFilters.access === "rw", () => {
      dictFilters.access = "rw";
      renderAll();
    }),
    filterButton("dict-filter-value", `有值`, dictFilters.value === "with", () => {
      dictFilters.value = dictFilters.value === "with" ? "all" : "with";
      renderAll();
    }),
    filterButton("dict-filter-value", `无值`, dictFilters.value === "without", () => {
      dictFilters.value = dictFilters.value === "without" ? "all" : "without";
      renderAll();
    }),
  );
  const drawer = dictFilters.selected
    ? el(
        "div",
        {
          "data-testid": "dict-drawer",
          class: "kv",
          style: "margin-top:10px",
          role: "dialog",
          "aria-label": "对象详情",
        },
        el("b", {}, `${dictFilters.selected.index}:${String(dictFilters.selected.sub).padStart(2, "0")}`),
        el("span", {}, ` ${dictFilters.selected.name || ""}（${dictFilters.selected.bits} 位 / ${dictFilters.selected.access}）`),
        " ",
        el(
          "button",
          {
            type: "button",
            "data-testid": "dict-drawer-goto-sdo",
          },
          "用 SDO 读取该对象",
        ),
        " ",
        el("button", { type: "button", "id": "dict-drawer-close" }, "关闭（Esc）"),
      )
    : null;
  app.append(
    sectionT(
      "dict-table",
      tr("section.dict", "对象字典"),
      el(
        "p",
        { class: "sub", "data-testid": "dict-meta" },
        `对象 ${dict.object_count} 个 / 条目 ${dict.entry_count} 个（值按小端存储，显示为 0x 大端十六进制）。` +
          `当前筛选命中 ${rows.length} 行。`,
      ),
      toolbar,
      rows.length === 0
        ? el(
            "p",
            { class: "sub", "data-testid": "dict-empty" },
            "无结果：空字典或筛选条件过严。",
            " ",
            el(
              "button",
              { type: "button", id: "dict-clear-filters" },
              "清空筛选",
            ),
          )
        : tableWithTestidRows(
            ["索引", "子索引", "名称", "类型", "位长", "访问", "值"],
            rows,
            (row) => `dict-row-${String(row[0] || "").replace(/^0x/i, "")}-${String(row[1]).padStart(2, "0")}`,
            (row) => {
            const object = (dict.objects || []).find((item) => item.index === row[0]) ||
              (dict.objects || []).find((item) => (item.entries || []).some((entry) => entry.sub === Number(row[1])));
            const entry = object && (object.entries || []).find((item) => item.sub === Number(row[1]));
            dictFilters.selected = {
              index: row[0] || (object && object.index) || "",
              sub: Number(row[1]) || 0,
              name: (entry && entry.name) || row[2],
              bits: (entry && entry.bits) || 0,
              access: (entry && entry.access) || "",
            };
            renderAll();
            },
          ),
      drawer,
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
    rows.push([
      label,
      item.address,
      String(item.bytes),
      el("span", { "data-testid": `pdo-hex-${key}` }, item.hex || "-"),
      decodeU16(item.hex),
    ]);
  }
  app.append(
    sectionT(
      "pdo-current-table",
      tr("section.pdoCurrent", "过程数据（PDO，只读快照）"),
      el(
        "p",
        { class: "sub" },
        `站地址 ${pdo.station_address} / AL ${pdo.al_status}；hex 为小端原始字节，右侧为按 u16 小端解码。`,
      ),
      table(["方向", "地址", "字节数", "hex", "u16 解码"], rows),
      el(
        "p",
        {},
        el("button", { type: "button", "data-testid": "pdo-copy-hex", id: "pdo-copy-hex" }, "复制 SM2/SM3 hex"),
        " ",
        el("button", { type: "button", "data-testid": "pdo-pause-toggle" }, "暂停/继续"),
      ),
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
      sectionT(
        "pdo-decoded-table",
        tr("section.pdoDecoded", "PDO 具名解码（按映射顺序）"),
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
  canvas.setAttribute("data-testid", "pdo-waveform-canvas");
  canvas.setAttribute("role", "img");
  canvas.setAttribute("aria-label", `PDO 历史波形：${samples.length} 个采样，SM2/SM3 前 2 字节`);
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
    sectionT(
      "pdo-waveform-summary",
      tr("section.pdoHistory", "PDO 历史") + "（" + samples.length + "）",
      el(
        "p",
        { class: "sub", "data-testid": "pdo-sample-info" },
        `样本 ${samples.length} 个（按 PDO 变化去重）；蓝线 = SM2 前 2 字节（u16 小端）；绿线 = SM3 前 2 字节；纵轴按两者最大值归一化。`,
      ),
      canvas,
      el(
        "details",
        { "data-testid": "pdo-waveform-table" },
        el("summary", {}, `展开前 20 条采样（共 ${samples.length} 条）`),
        table(
          ["#", "SM2 hex", "SM3 hex", "AL"],
          samples.slice(0, 20).map((sample, index) => [
            String(index),
            sample.sm2_hex || "-",
            sample.sm3_hex || "-",
            sample.al_status || "-",
          ]),
        ),
      ),
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
      line.setAttribute("stroke", "#6b7790"); // WCAG-CHK-02：拓扑链线用 muted（≥3:1）
      line.setAttribute("stroke-width", "2");
      svg.append(line);
    }
    const circle = document.createElementNS(svgNs, "circle");
    circle.setAttribute("data-testid", `overview-topology-node-${slave.index}`);
    circle.setAttribute("cx", String(cx));
    circle.setAttribute("role", "button");
    circle.setAttribute("aria-label", `从站 ${slave.index}，AL ${slave.al_status}，点击切换`);
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
    sectionT(
      "overview-topology",
      tr("section.topology", "总线拓扑") + "（" + String(topology.count) + " 个从站）",
      el(
        "p",
        { class: "sub" },
        "节点色 = AL 状态（INIT 橙 / PREOP 蓝 / SAFEOP 紫 / OP 绿），点击节点切换到该从站；站地址未配置显示 0x0000。",
      ),
      renderTopologyGraph(topology),
      el(
        "div",
        { "data-testid": "overview-slaves-table" },
        tableWithTestidRows(
          ["#", "站地址", "AL 状态", "AL 状态码", "收/发帧", "SM2（地址 hex）", "SM3（地址 hex）"],
          rows,
          "overview-slaves-row-",
          (row) => {
            document.getElementById("slave-index").value = row[0];
            document.getElementById("load-api").click();
          },
        ),
      ),
      el(
        "p",
        {},
        el("button", { type: "button", "data-testid": "overview-copy-summary" }, "复制拓扑摘要"),
        " ",
        el("button", { type: "button", "data-testid": "overview-pause-toggle" }, "暂停/继续"),
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
    ["收帧", "frames-in", value("ecsim_frames_in_total")],
    ["发帧", "frames-out", value("ecsim_frames_out_total")],
    ["丢弃帧", "frames-dropped", value("ecsim_frames_dropped_total")],
    ["datagram 命中", "datagrams-hit", value("ecsim_datagrams_hit_total")],
    ["邮箱请求", "mailbox-requests", value("ecsim_mailbox_requests_total")],
    ["邮箱响应", "mailbox-responses", value("ecsim_mailbox_responses_total")],
    ["DC 寄存器读", "dc-reg-reads", value("ecsim_dc_reg_reads_total")],
    ["DC 寄存器写", "dc-reg-writes", value("ecsim_dc_reg_writes_total")],
    ["DC 模拟时间（ns）", "dc-time-ns", value("ecsim_dc_time_ns")],
    ["AL 状态", "al-status", al == null ? "-" : String(al)],
    ["AL 状态码", "al-status-code", value("ecsim_al_status_code")],
  ];
  app.append(
    section(
      tr("section.metrics", "指标（/metrics）"),
      el("p", { class: "sub" }, `AL 状态色：${alClass === "ok" ? "正常（PREOP/SAFEOP/OP）" : "异常或 INIT"}`),
      el(
        "div",
        { class: "grid" },
        ...cards.map(([label, slug, text]) =>
          el(
            "div",
            { class: "kv", "data-testid": `overview-metric-${slug}` },
            el("b", {}, label),
            el("span", { class: alClass }, text),
          ),
        ),
      ),
    ),
  );
}

function renderOverview(topology) {
  const slaves = topology.slaves || [];
  const derived = slaves.map((slave) => deriveAl(slave.al_status));
  const problemSlaves = slaves.filter(
    (slave, index) => derived[index].error || derived[index].idRequest || derived[index].state !== 8,
  );
  const opCount = slaves.filter((_slave, index) => derived[index].state === 8).length;
  const health = `${slaves.length} 站 / OP ${opCount} / 异常 ${problemSlaves.length}`;
  const problemRows = problemSlaves.map((slave) => {
    const info = deriveAl(slave.al_status);
    return [
      String(slave.index),
      info.stateName,
      info.error ? "error" : info.idRequest ? "id-req" : "-",
      slave.al_status_code,
    ];
  });
  app.append(
    sectionT(
      "overview-health",
      tr("section.overview", "总线总览（health）"),
      el("p", { class: "sub" }, `${health}（AL 派生：state & 0x0F，bit4=error，bit5=ID 请求）`),
      problemRows.length === 0
        ? el(
            "p",
            { class: "sub", "data-testid": "overview-problems-empty" },
            "无异常从站（全部 OP 且无 error/ID 请求）。",
          )
        : el(
            "div",
            { "data-testid": "overview-problems-table" },
            table(["#", "AL 状态", "异常位", "AL 状态码"], problemRows),
          ),
      el(
        "p",
        {},
        el("button", { type: "button", "data-testid": "overview-copy-summary" }, "复制总览摘要"),
        " ",
        el("button", { type: "button", "data-testid": "overview-pause-toggle" }, "暂停/继续"),
      ),
    ),
  );
}

function buildEvidenceMarkdown() {
  const lines = [];
  lines.push(`# EC-Sim 证据导出（ecsim-evidence-v1）`);
  lines.push("");
  lines.push(`- 生成时间：${new Date().toISOString()}`);
  lines.push(`- 模式：${connBadgeText(conn.mode)}；站：${selectedSlave()}（0-based）；API：${sourceHostPort() || "-"}`);
  lines.push(`- 数据年龄：${ageText()}`);
  lines.push("");
  lines.push("## 边界（必读）");
  lines.push("");
  lines.push("- 快照语义，非实时 SLA；数值为读取时刻近似值。");
  lines.push("- 不宣称 EtherCAT 一致性 / CTT / LinuxCNC 或任何认证。");
  lines.push("- ⚠️ 未验证项：DOD-13b `OP|error=0x18`（NOT-JUDGED，BL-20 v1.x）；DC 仅 Tier0 模拟时钟。");
  lines.push("");
  const pick = (key, label, data, format) => {
    if (!evidenceState.selected[key] || data == null) return;
    lines.push(`## ${label}`);
    lines.push("");
    lines.push("```json");
    lines.push(JSON.stringify(format ? format(data) : data, null, 2).slice(0, 8000));
    lines.push("```");
    lines.push("");
  };
  const live = currentState && currentState.live;
  pick("identity", tr("section.slaveIdentity", "身份与画像"), currentState && currentState.identity);
  pick(
    "al",
    "AL / 实时",
    live
      ? {
          al_status: live.al_status,
          al_status_code: live.al_status_code,
          derived: deriveAl(live.al_status),
          dc_time_ns: live.dc_time_ns,
        }
      : null,
  );
  pick("pdo", "PDO 当前", currentPdo);
  pick("history", "PDO 历史", currentPdoHistory);
  pick("topology", "拓扑", currentTopology);
  pick("metrics", "指标", currentMetrics);
  pick("dict", "对象字典（元信息）", currentDict && {
    object_count: currentDict.object_count,
    entry_count: currentDict.entry_count,
  });
  pick("audit", "SDO 写审计", currentAudit);
  return lines.join("\n");
}

function downloadText(filename, text, mime) {
  const blob = new Blob([text], { type: mime || "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  toast(`已下载：${filename}`);
}

function renderEvidence() {
  if (!currentState && !currentPdo && !currentTopology) return;
  const checklist = Object.entries({
    identity: "身份/画像",
    al: "AL / 实时",
    pdo: "PDO 当前",
    history: "PDO 历史",
    topology: "拓扑",
    metrics: "指标",
    dict: tr("section.dict", "对象字典"),
    audit: "SDO 审计",
  }).map(([key, label]) =>
    el(
      "label",
      { "data-testid": `evidence-item-${key}`, style: "display:inline-flex;min-height:44px;align-items:center;gap:6px;margin-right:12px" },
      el("input", {
        type: "checkbox",
        checked: evidenceState.selected[key] ? "checked" : undefined,
        "data-evidence-key": key,
      }),
      label,
    ),
  );
  app.append(
    sectionT(
      "evidence-boundaries",
      tr("section.evidence", "证据导出（只读；S-11 / V-06）"),
      el(
        "p",
        { class: "sub", "data-testid": "evidence-meta" },
        `版本：ecsim-evidence-v1 · 模式：${connBadgeText(conn.mode)} · 来源：${liveBase || "本地 JSON"} · ` +
          `时间：${new Date().toLocaleString()} · 站：${selectedSlave()}（0-based）`,
      ),
      el("div", { class: "row", "data-testid": "evidence-checklist" }, ...checklist),
      el(
        "details",
        { open: "open" },
        el("summary", {}, "导出预览（Markdown）"),
        el(
          "pre",
          { "data-testid": "evidence-preview", style: "white-space:pre-wrap;max-height:260px;overflow:auto;background:#0f1115;padding:10px;border-radius:8px" },
          buildEvidenceMarkdown(),
        ),
      ),
      el(
        "p",
        {},
        el("button", { type: "button", "data-testid": "evidence-copy" }, "复制 Markdown"),
        " ",
        el("button", { type: "button", "data-testid": "evidence-download-md" }, "下载 .md"),
        " ",
        el("button", { type: "button", "data-testid": "evidence-download-json" }, "下载 .json"),
      ),
      el(
        "p",
        { class: "sub", "data-testid": "evidence-boundaries" },
        "⚠️ 边界：快照语义（非实时 SLA）；不宣称一致性/CTT/认证；未验证项标 ⚠️ 并给方法；不含 token/绝对路径。",
      ),
    ),
  );
}

function renderAbout() {
  app.append(
    sectionT(
      "about-boundaries",
      tr("section.about", "关于 / 边界（V-07）"),
      el(
        "p",
        { class: "sub", "data-testid": "about-version" },
        "EC-Sim 只读状态视图 · 版本：ec-sim-web（与 ec-sim-esc 同仓发布）",
      ),
      el(
        "p",
        { class: "sub", "data-testid": "about-health" },
        aboutHealth == null ? "健康检查：未检查（需实时模式）" : `健康检查：${aboutHealth}`,
      ),
      el(
        "div",
        { "data-testid": "about-support-matrix" },
        table(
          ["组件", "已验证", "备注"],
          [
            ["IgH 主站（ethercat）", "1.6.13", "验收机/虚拟闭环"],
            ["SOEM（夹具）", "2.0.0", "TAP/UDS 夹具"],
            ["LinuxCNC + lcec", "⚠️ 待验证", "V1/V2 虚拟闭环由 CNC 侧验收"],
            ["其它主站/系统", "未验证", "不在本视图支持范围"],
          ],
        ),
      ),
      el(
        "p",
        { class: "sub", "data-testid": "about-token" },
        "token 只提示不输入：--serve-token-env 模式下浏览器 fetch 无法自动带 Authorization，需反向代理或 curl；本页不保存 token。",
      ),
      el(
        "p",
        { class: "sub", "data-testid": "about-boundaries" },
        "不宣称：EtherCAT 一致性 / CTT / LinuxCNC 认证 / 实时 SLA。快照语义，观察值为读取时刻近似值。",
      ),
      el(
        "p",
        {},
        el("button", { type: "button", "data-testid": "about-copy-version" }, "复制版本信息"),
      ),
    ),
  );
}

function refreshAboutHealth() {
  if (!liveBase) return;
  fetch(`${liveBase}/health`, { cache: "no-store" })
    .then((response) => {
      aboutHealth = response.ok ? `200 OK（${liveBase}/health）` : `${response.status}`;
    })
    .catch(() => {
      aboutHealth = "失败（无法连接）";
    })
    .finally(() => renderAll());
}

// DOD-01 通用态槽位（<view>-loading / -error-state / -stale-badge / -empty）。
function viewTestid(view, slot) {
  return `${view}-${slot}`;
}

function renderAll() {
  app.replaceChildren();
  if (currentState) renderState(currentState);
  if (currentPdo) renderPdo(currentPdo);
  if (currentPdoHistory) renderPdoHistory(currentPdoHistory);
  if (currentTopology) {
    renderOverview(currentTopology);
    renderTopology(currentTopology);
  }
  if (currentMetrics) renderMetrics(currentMetrics);
  if (currentDict) renderDict(currentDict);
  renderEvidence();
  renderAbout();
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
  const hasAnyData = Boolean(currentState || currentPdo || currentDict || currentTopology);
  if (!hasAnyData) {
    app.append(
      el(
        "p",
        { class: "sub", "data-testid": viewTestid("overview", "empty") },
        "尚未载入数据：请选择 JSON 文件或填写 API 地址后点“从 API 载入”。",
      ),
    );
  }
  if (conn.mode === "CONNECTING") {
    app.append(
      el(
        "p",
        { class: "sub", "data-testid": viewTestid("overview", "loading") },
        "加载中：正在拉取首次快照（确定性文案，无转圈动画）。",
      ),
    );
  }
  if (conn.mode === "STOPPED") {
    app.append(
      sectionT(
        viewTestid("overview", "error-state"),
        "错误状态（E-10）",
        el("p", { class: "sub" }, "原因：连续 3 次请求失败。影响：自动刷新已停止，当前值为最后一次快照。下一步：确认服务启动后点“重新连接”。"),
        el("p", {}, el("button", { type: "button", id: "error-retry" }, "重新连接")),
      ),
    );
  }
  if (conn.mode === "STALE") {
    app.append(
      el(
        "p",
        { class: "sub", "data-testid": viewTestid("overview", "stale-badge") },
        "⚠️ 陈旧角标：当前视图数据超过 2 个刷新周期未更新。",
      ),
    );
  }
  app.append(el("p", { class: "sub" }, "只读视图，不构成一致性/认证结论。"));
  // DOD-02：静态模式说明（无 live 数据时显式提示，不再静默缺失）。
  setStaticModeNotice(Boolean(currentState && !currentState.live));
  updateSdoCurrent();
  renderAudit();
}

// ---- DOD-02：`/sdo-audit` 只读展示（I-12/G-07；实时模式）----
let lastSdoRead = null;

function updateSdoCurrent() {
  const node = document.getElementById("sdo-current");
  if (!node) return;
  node.replaceChildren();
  const base = sdoApiBase();
  const age = conn.lastDataAt ? ageText() : "静态/未刷新";
  const staleNote =
    conn.mode === "STALE" || conn.mode === "PAUSED" || conn.mode === "STOPPED"
      ? `；⚠️ 数据可能过期（${connBadgeText(conn.mode)}）`
      : "";
  node.append(
    el(
      "span",
      {},
      `当前站：${selectedSlave()}（0-based，0..N-1） · API：${sourceHostPort() || "未填写"} · 数据：${age}${staleNote}`,
    ),
  );
}

function renderAudit() {
  const container = document.getElementById("sdo-audit");
  const statusNode = document.getElementById("sdo-audit-status");
  if (!container) return;
  container.replaceChildren();
  if (!currentAudit || !Array.isArray(currentAudit.events)) {
    if (statusNode) statusNode.textContent = "无审计数据（需实时模式且发生过受控写）。";
    return;
  }
  if (currentAudit.events.length === 0) {
    if (statusNode) statusNode.textContent = "审计为空：尚未发生受控写。";
    container.append(el("p", { class: "sub", "data-testid": "sdo-audit-empty" }, "暂无写事件。"));
    return;
  }
  const rows = currentAudit.events.map((event) => [
    event.ts_ms == null ? "-" : new Date(Number(event.ts_ms)).toLocaleTimeString(),
    String(event.slave ?? "-"),
    event.index,
    String(event.sub).padStart(2, "0"),
    event.old_hex || "-",
    event.new_hex || "-",
  ]);
  container.append(
    tableWithTestidRows(
      ["时间", "站（0-based）", "索引", "子索引", "旧值（小端 hex）", "新值（小端 hex）"],
      rows,
      "sdo-audit-row-",
    ),
  );
  if (statusNode) statusNode.textContent = `共 ${currentAudit.events.length} 条（服务端最多 64 条）。`;
}

async function loadAudit() {
  const statusNode = document.getElementById("sdo-audit-status");
  const base = liveBase;
  if (!base) {
    if (statusNode) statusNode.textContent = "需要实时模式（--serve + --uds/--tap）。";
    return;
  }
  try {
    const response = await fetch(`${base}/sdo-audit?slave=${selectedSlave()}`, { cache: "no-store" });
    if (!response.ok) {
      if (statusNode) {
        statusNode.textContent =
          response.status === 404
            ? "审计不可用：该路由需要实时模式（静态快照无写事件）。"
            : `审计读取失败：${response.status}`;
      }
      return;
    }
    const data = JSON.parse(await response.text());
    if (data.schema === "ec-sim/sdo-audit-v1") {
      currentAudit = data;
      renderAudit();
    }
  } catch (error) {
    if (statusNode) statusNode.textContent = `审计读取失败：${error.message}`;
  }
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
  if (conn.source) {
    conn.source.close();
    conn.source = null;
  }
  if (conn.timer) {
    clearInterval(conn.timer);
    conn.timer = null;
  }
  if (conn.heartbeat) {
    clearInterval(conn.heartbeat);
    conn.heartbeat = null;
  }
  if (conn.staleTimer) {
    clearTimeout(conn.staleTimer);
    conn.staleTimer = null;
  }
}

function stopPollingTimer() {
  if (conn.timer) {
    clearInterval(conn.timer);
    conn.timer = null;
  }
}

async function pollOnce(reason) {
  if (!liveBase || conn.paused || conn.hidden) return;
  const seq = ++conn.requestSeq;
  const slave = selectedSlave();
  const requestOpts = { cache: "no-store" };
  try {
    const stateResponse = await fetch(`${liveBase}/state?slave=${slave}`, requestOpts);
    if (seq !== conn.requestSeq) return; // 切站/重连：丢弃过期响应
    if (!stateResponse.ok) throw new Error(`/state ${stateResponse.status}`);
    loadJson(await stateResponse.text(), `${liveBase}/state?slave=${slave} (${reason})`);
    const pdoResponse = await fetch(`${liveBase}/pdo?slave=${slave}`, requestOpts);
    if (pdoResponse.ok && seq === conn.requestSeq) {
      loadJson(await pdoResponse.text(), `${liveBase}/pdo?slave=${slave} (${reason})`);
    }
    const historyResponse = await fetch(`${liveBase}/pdo?slave=${slave}&history=1`, requestOpts);
    if (historyResponse.ok && seq === conn.requestSeq) {
      loadJson(await historyResponse.text(), `${liveBase}/pdo?slave=${slave}&history=1 (${reason})`);
    }
    const topologyResponse = await fetch(`${liveBase}/topology`, requestOpts);
    if (topologyResponse.ok && seq === conn.requestSeq) {
      loadJson(await topologyResponse.text(), `${liveBase}/topology (${reason})`);
    }
    const metricsResponse = await fetch(`${liveBase}/metrics?slave=${slave}`, requestOpts);
    if (metricsResponse.ok && seq === conn.requestSeq) {
      loadMetrics(await metricsResponse.text(), `${liveBase}/metrics?slave=${slave} (${reason})`);
    }
    await loadAudit();
    noteData();
    if (conn.mode === "STOPPED" || conn.mode === "RECONNECTING") setConnMode(conn.lastMode);
  } catch (error) {
    conn.failures += 1;
    if (conn.failures >= conn.maxFailures) {
      stopPollingTimer();
      if (conn.source) {
        conn.source.close();
        conn.source = null;
      }
      setConnMode("STOPPED");
    } else {
      setConnMode("STALE");
    }
    message.textContent = `实时请求失败（${conn.failures}/${conn.maxFailures}）：${error.message}`;
  }
}

function startPollingTimer(intervalMs) {
  if (conn.timer) clearInterval(conn.timer);
  conn.pollIntervalMs = intervalMs;
  conn.timer = setInterval(() => {
    void pollOnce("poll");
  }, intervalMs);
}

function startHeartbeat() {
  if (conn.heartbeat) clearInterval(conn.heartbeat);
  // 2× 刷新间隔无新数据 → STALE（保留最后值 + 时间/Δ；不得伪造实时）。
  conn.heartbeat = setInterval(() => {
    if (conn.paused || conn.mode === "STOPPED" || conn.mode === "IDLE") {
      updateConnUi();
      return;
    }
    if (conn.lastDataAt && Date.now() - conn.lastDataAt > MAX_DELTA_MS) {
      if (conn.mode === "LIVE_SSE" || conn.mode === "LIVE_POLL") setConnMode("STALE");
    }
    updateConnUi();
  }, 500);
}

function connectSse() {
  if (!liveBase || conn.paused) return;
  if (conn.source) {
    conn.source.close();
    conn.source = null;
  }
  const slave = selectedSlave();
  setConnMode("CONNECTING");
  const source = new EventSource(`${liveBase}/events?slave=${slave}`);
  conn.source = source;
  source.onopen = () => setConnMode("LIVE_SSE");
  source.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      if (data.schema === "ec-sim/state-v1") {
        currentState = data;
        message.textContent = `SSE：${liveBase}/events?slave=${slave}`;
        noteData();
        if (conn.mode !== "LIVE_SSE") setConnMode("LIVE_SSE");
        renderAll();
      }
    } catch (error) {
      message.textContent = `SSE 解析失败：${error.message}`;
    }
  };
  source.onerror = () => {
    // 断流 → 显式降级为 LIVE_POLL（不得继续显示 LIVE 单字）；保留最后数据 + 时间戳。
    if (conn.source) {
      conn.source.close();
      conn.source = null;
    }
    if (conn.mode === "STOPPED") return;
    setConnMode("LIVE_POLL");
    if (!conn.timer) startPollingTimer(1000);
    // 5s 后单次重连探测（只用 1 个 EventSource；不做无限叠加定时器）。
    if (conn.staleTimer) clearTimeout(conn.staleTimer);
    conn.staleTimer = setTimeout(() => {
      if (conn.mode === "LIVE_POLL") connectSse();
    }, 5000);
  };
  // SSE 期间其余块按 2s 轮询（PDO/拓扑/指标/审计）。
  startPollingTimer(2000);
}

function startLive() {
  stopLive();
  if (!liveBase) return;
  conn.lastDataAt = null;
  conn.failures = 0;
  if (conn.paused) {
    setConnMode("PAUSED");
    return;
  }
  startHeartbeat();
  void pollOnce("initial").then(() => {
    if (typeof window !== "undefined" && window.EventSource) connectSse();
    else setConnMode("LIVE_POLL");
  });
}

function reconnectNow() {
  if (!liveBase) {
    message.textContent = "请先填写 API 地址并载入。";
    return;
  }
  stopLive();
  conn.failures = 0;
  setConnMode("RECONNECTING");
  setStaticModeNotice(false);
  void pollOnce("reconnect").then(() => {
    if (conn.mode === "STOPPED") {
      setConnMode("STOPPED");
      return;
    }
    if (typeof window !== "undefined" && window.EventSource) connectSse();
    else setConnMode("LIVE_POLL");
  });
}

function togglePause() {
  conn.paused = !conn.paused;
  if (conn.paused) {
    stopLive();
    setConnMode("PAUSED");
  } else {
    setConnMode("RECONNECTING");
    startLive();
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
    conn.lastDataAt = Date.now();
    await loadAudit();
    updateSdoCurrent();
    if (document.getElementById("live").checked) startLive();
    else {
      setConnMode("IDLE");
      updateConnUi();
    }
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
  else {
    stopLive();
    setConnMode("IDLE");
    updateConnUi();
  }
});

// DOD-03：切站 = 切换数据上下文；重连（丢弃旧 SSE/轮询，避免混站）。
document.getElementById("slave-index").addEventListener("change", () => {
  lastSdoRead = null;
  conn.lastDataAt = null;
  conn.failures = 0;
  updateSdoCurrent();
  if (liveBase && document.getElementById("live").checked && !conn.paused) reconnectNow();
});

document.getElementById("reconnect").addEventListener("click", () => reconnectNow());
document.getElementById("pause-toggle").addEventListener("click", () => togglePause());
document.getElementById("sdo-audit-refresh").addEventListener("click", () => {
  void loadAudit();
});

// DOD-03 §3.3-7：页面隐藏暂停轮询；恢复可见立即拉一次。
document.addEventListener("visibilitychange", () => {
  conn.hidden = document.hidden;
  if (document.hidden) {
    stopPollingTimer();
    if (conn.staleTimer) {
      clearTimeout(conn.staleTimer);
      conn.staleTimer = null;
    }
  } else if (liveBase && document.getElementById("live").checked && !conn.paused) {
    void pollOnce("visible");
  }
});

// 初始 UI：徽标/数据时间/暂停按钮。
updateConnUi();
updateSdoCurrent();

// DOD-09：语言切换后重渲染动态文案（静态文案由 i18n.js 处理）。
window.addEventListener("ecsim-lang", () => {
  updateConnUi();
  updateSdoCurrent();
  renderAll();
});
const langToggle = document.getElementById("lang-toggle");
if (langToggle) {
  langToggle.addEventListener("click", () => {
    if (window.ECSIM_I18N) window.ECSIM_I18N.toggle();
  });
}

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

function setFieldError(id, text) {
  const node = document.getElementById(id);
  if (node) node.textContent = text;
}

function sdoQuery() {
  const index = parseSdoIndex(document.getElementById("sdo-index").value);
  if (index == null) {
    setSdoMessage("sdo-status", "索引非法：请输入十六进制 0x0000–0xFFFF（可省略 0x）。", true);
    setFieldError("sdo-index-error", "索引非法：需要十六进制 0x0000–0xFFFF（可省略 0x）。");
    return null;
  }
  setFieldError("sdo-index-error", "");
  const sub = parseSdoSub(document.getElementById("sdo-sub").value);
  if (sub == null) {
    setSdoMessage("sdo-status", "子索引非法：0–255。", true);
    setFieldError("sdo-sub-error", "子索引非法：需要 0–255。");
    return null;
  }
  setFieldError("sdo-sub-error", "");
  const slave = selectedSlave();
  return {
    index,
    sub,
    slave,
    // DOD-13（G-04 UI 半）：`/sdo` 请求必须带当前站（0-based）；B55 起后端校验非法/越界 → 404。
    query: `?index=0x${index.toString(16).padStart(4, "0").toUpperCase()}&sub=${sub}&slave=${slave}`,
  };
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
    [
      "值（大端显示）",
      el(
        "span",
        { "data-testid": "sdo-value-hex" },
        `${hexToBigEndian(data.value_hex)}（原始小端：${data.value_hex === "" ? "-" : data.value_hex}）`,
      ),
    ],
    [
      "复制",
      el("button", { type: "button", "data-testid": "sdo-value-copy", id: "sdo-value-copy" }, "复制旧值 hex"),
    ],
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
  // DOD-02：每条错误给「原因 + 影响 + 下一步」（不是只显示 HTTP 码）。
  if (response.status === 400) {
    return `400：请求参数非法${detail ? `（${detail}）` : ""}。影响：本次读写未执行。下一步：检查值长度是否等于对象位长（小端原始字节）。`;
  }
  if (response.status === 401) {
    return "401：需要 Authorization: Bearer token。影响：当前页面无法鉴权（浏览器 fetch 无法自动带 token）。下一步：经反向代理注入请求头，或改用 curl（--serve-token-env 模式）。";
  }
  if (response.status === 403) {
    return `403：服务端拒绝${detail ? `（${detail}）` : ""}。影响：对象被写白名单/安全排除规则阻止。下一步：确认 manifest 声明 writable 且不是身份/配置对象。`;
  }
  if (response.status === 404) {
    return `404：对象/子索引不存在或参数缺失${detail ? `（${detail}）` : ""}。影响：未取得数据。下一步：核对索引/子索引与站号（0-based，0..N-1）。`;
  }
  if (response.status === 405) {
    return `405：写操作未启用${detail ? `（${detail}）` : ""}。影响：服务端未开启写路径。下一步：以 --serve-write 启动服务。`;
  }
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
    lastSdoRead = data;
    setSdoMessage(
      "sdo-status",
      `已读取站 ${parsed.slave} 的 ${data.index}:${String(data.sub).padStart(2, "0")} ${data.name || ""}`.trim(),
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
    setFieldError("sdo-write-value-error", "新值非法：需要偶数个十六进制字符（如 0f00）。");
    return;
  }
  setFieldError("sdo-write-value-error", "");
  const written = bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
  // DOD-02：confirm 信息增强（对象名/位长/旧值/新值/站号 + STALE/暂停警告 + 审计提示）。
  const current =
    lastSdoRead &&
    lastSdoRead.schema === "ec-sim/sdo-v1" &&
    lastSdoRead.index === `0x${parsed.index.toString(16).padStart(4, "0").toUpperCase()}` &&
    Number(lastSdoRead.sub) === parsed.sub
      ? lastSdoRead
      : null;
  const staleWarning =
    conn.mode === "STALE" || conn.mode === "PAUSED" || conn.mode === "STOPPED"
      ? `当前数据可能过期（${connBadgeText(conn.mode)}，最后数据 ${ageText()}）`
      : "";
  const info = {
    slave: parsed.slave,
    object: `${parsed.index.toString(16).toUpperCase().padStart(4, "0")}h:${String(parsed.sub).padStart(2, "0")}`,
    name: current ? current.object_name || current.name || "-" : "（未读取；建议先点“读取”核对旧值/位长）",
    meta: current ? `${current.bits} 位 / ${current.access}` : "-",
    oldValue: current ? current.value_hex || "-" : "-",
    newValue: `${written}（${bytes.length} 字节）`,
    stale: Boolean(staleWarning),
  };
  // 二次确认：勾选“允许写入”是第一步；L2 对话框是第二步（DOD-02/H2，UXD-05）。
  const dialog = document.getElementById("sdo-confirm-dialog");
  if (dialog) {
    openSdoConfirm(parsed, bytes, info);
    return;
  }
  // 回退：无 L2 对话框（旧静态资源/极简宿主）时用原生 confirm，保证二次确认不缺失。
  const confirmed = window.confirm(
    `确认写入从站 ${parsed.slave}（0-based，0..N-1）的对象 ${info.object}？\n` +
      `对象名：${info.name}\n位长/访问：${info.meta}\n旧值（小端 hex）：${info.oldValue}\n新值（小端 hex）：${info.newValue}\n\n` +
      "该操作仅在服务端以 --serve-write 启动时生效；写入会记入 /sdo-audit。" +
      (staleWarning ? `\n⚠️ ${staleWarning}` : ""),
  );
  if (!confirmed) {
    setSdoMessage("sdo-write-status", "已取消写入。", false);
    return;
  }
  await performSdoWrite(parsed, bytes);
}

// H2（DOD-02/UXD-05）：L2 自定义确认框（替代原生 confirm 的默认路径；Esc=取消，默认焦点在取消）。
let pendingSdoWrite = null;

function openSdoConfirm(parsed, bytes, info) {
  pendingSdoWrite = { parsed, bytes };
  const dialog = document.getElementById("sdo-confirm-dialog");
  if (!dialog) return;
  const set = (testid, text) => {
    const node = dialog.querySelector(`[data-testid="${testid}"]`);
    if (node) node.textContent = text;
  };
  set("sdo-confirm-slave", String(info.slave));
  set("sdo-confirm-object", `${info.object} ${info.name}`);
  set("sdo-confirm-meta", info.meta);
  set("sdo-confirm-old-value", info.oldValue);
  set("sdo-confirm-new-value", info.newValue);
  const staleNode = document.getElementById("sdo-confirm-stale");
  if (staleNode) {
    staleNode.hidden = !info.stale;
    staleNode.textContent = info.stale
      ? `⚠️ 数据可能过期：${connBadgeText(conn.mode)}，最后数据 ${ageText()}；写入前请确认现场状态。`
      : "";
  }
  dialog.hidden = false;
  dialog.style.display = "grid";
  document.getElementById("sdo-confirm-cancel")?.focus();
}

function closeSdoConfirm() {
  pendingSdoWrite = null;
  const dialog = document.getElementById("sdo-confirm-dialog");
  if (!dialog) return;
  dialog.hidden = true;
  dialog.style.display = "none";
}

async function performSdoWrite(parsed, bytes) {
  const base = sdoApiBase();
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
      `写入成功（站 ${parsed.slave}）：${data.old_hex || "-"} → ${data.written_hex}（审计见 GET /sdo-audit）`,
      false,
    );
    lastSdoRead = null; // 旧值已变化，下次 confirm 前重新读取
    await readSdo();
    await loadAudit();
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

// ---- H2（task-44）：testid 动作委托 / 字典交互 / 证据导出 / 关于页 ----
document.addEventListener("click", (event) => {
  const target = event.target.closest ? event.target.closest("[data-testid]") : null;
  if (!target) return;
  const testid = target.getAttribute("data-testid") || "";
  const actions = {
    "shell-context-copy-summary": () => copyText(summaryText(), "摘要已复制"),
    "shell-context-export": () =>
      downloadText("ecsim-web-export.json", JSON.stringify(currentExport(), null, 2), "application/json"),
    "slave-goto-pdo": () =>
      document.querySelector('[data-testid="pdo-current-table"]')?.scrollIntoView({ block: "start" }),
    "slave-copy-identity": () =>
      copyText(currentState ? JSON.stringify(currentState.identity || {}) : "", "身份已复制"),
    "overview-copy-summary": () => copyText(summaryText(), "摘要已复制"),
    "overview-pause-toggle": () => togglePause(),
    "pdo-pause-toggle": () => togglePause(),
    "pdo-copy-hex": () =>
      copyText(currentPdo ? `SM2=${currentPdo.sm2.hex}\nSM3=${currentPdo.sm3.hex}` : "", "PDO hex 已复制"),
    "sdo-value-copy": () => copyText(lastSdoRead ? lastSdoRead.value_hex : "", "SDO 旧值 hex 已复制"),
    "evidence-copy": () => copyText(buildEvidenceMarkdown(), "证据 Markdown 已复制"),
    "evidence-download-md": () =>
      downloadText("ecsim-evidence.md", buildEvidenceMarkdown(), "text/markdown;charset=utf-8"),
    "evidence-download-json": () =>
      downloadText("ecsim-evidence.json", JSON.stringify(currentExport(), null, 2), "application/json"),
    "about-copy-version": () =>
      copyText(
        `ec-sim-web；模式=${connBadgeText(conn.mode)}；数据=${ageText()}；不宣称一致性/CTT/认证`,
        "版本信息已复制",
      ),
    "dict-drawer-goto-sdo": () => {
      const selected = dictFilters.selected;
      if (!selected) return;
      document.getElementById("sdo-index").value = selected.index;
      document.getElementById("sdo-sub").value = String(selected.sub || 0);
      dictFilters.selected = null;
      renderAll();
      document.querySelector("#sdo-index")?.scrollIntoView({ block: "center" });
    },
  };
  const action = actions[testid];
  if (action) {
    event.preventDefault();
    action();
  }
});

document.addEventListener("change", (event) => {
  const key = event.target && event.target.getAttribute && event.target.getAttribute("data-evidence-key");
  if (key && key in evidenceState.selected) {
    evidenceState.selected[key] = Boolean(event.target.checked);
    renderAll();
  }
});

document.addEventListener("input", (event) => {
  if (event.target && event.target.id === "dict-search") {
    dictFilters.search = event.target.value;
    const caret = event.target.selectionStart;
    renderAll();
    const next = document.getElementById("dict-search");
    if (next) {
      next.focus();
      next.setSelectionRange(caret, caret);
    }
  }
});

document.addEventListener("click", (event) => {
  const id = event.target && event.target.id;
  if (id === "dict-clear-filters") {
    dictFilters.search = "";
    dictFilters.access = "all";
    dictFilters.value = "all";
    dictFilters.selected = null;
    renderAll();
  } else if (id === "dict-drawer-close") {
    dictFilters.selected = null;
    renderAll();
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "/" && document.activeElement && document.activeElement.tagName !== "INPUT") {
    event.preventDefault();
    document.getElementById("dict-search")?.focus();
  } else if (event.key === "Escape" && dictFilters.selected) {
    dictFilters.selected = null;
    renderAll();
  }
});
