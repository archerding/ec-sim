// task-78：C4 浏览器侧 JSONL 最终化（可复现证据）——多来源合并、按时间排序、ts 校验、source 标注、
// timers_live 稳定性与崩溃统计；输出 browser.jsonl + browser-summary.json（不写未验证 PASS）。
// 用法：node scripts/spikes/hmi-c4-browser-finalize.mjs <outDir> <in1.jsonl> [in2.jsonl ...]
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const [outDir, ...inputs] = process.argv.slice(2);
if (!outDir || inputs.length === 0) { console.error("usage: hmi-c4-browser-finalize.mjs <outDir> <in1.jsonl> [...]"); process.exit(2); }

const rows = [];
for (const input of inputs) {
  const source = path.basename(input);
  const text = await readFile(input, "utf8");
  for (const [lineNo, line] of text.split("\n").entries()) {
    if (!line.trim()) continue;
    let row;
    try { row = JSON.parse(line); } catch (error) { console.error(`SKIP_INVALID ${source}:${lineNo + 1}`); continue; }
    if (!row.ts || Number.isNaN(Date.parse(row.ts))) { console.error(`SKIP_BAD_TS ${source}:${lineNo + 1}`); continue; }
    rows.push({ source, ...row, ts_ms: Date.parse(row.ts) });
  }
}
rows.sort((a, b) => a.ts_ms - b.ts_ms);

const samples = rows.filter((row) => !row.retry && !row.crashed && row.heap !== undefined);
const crashed = rows.filter((row) => row.crashed);
const retries = rows.filter((row) => row.retry);
const heaps = samples.map((row) => row.heap).filter((v) => v != null);
const timers = samples.map((row) => row.timers_live).filter((v) => typeof v === "number");
const sseOpens = samples.map((row) => row.sse_opens).filter((v) => typeof v === "number");
const sseErrors = samples.map((row) => row.sse_errors).filter((v) => typeof v === "number");
const requests = samples.map((row) => row.requests).filter((v) => typeof v === "number");
const intervals = rows.map((row) => row.ts_ms);
const monotonic = intervals.every((value, index) => index === 0 || value >= intervals[index - 1]);
const timerSpread = timers.length ? Math.max(...timers) - Math.min(...timers) : null;

const summary = {
  inputs,
  total_rows: rows.length,
  samples: samples.length,
  crashed_rows: crashed.length,
  retry_rows: retries.length,
  timestamps_monotonic: monotonic,
  first_ts: rows[0] ? rows[0].ts : null,
  last_ts: rows[rows.length - 1] ? rows[rows.length - 1].ts : null,
  duration_seconds: rows.length ? Math.round((intervals[intervals.length - 1] - intervals[0]) / 1000) : 0,
  heap: heaps.length ? { first: heaps[0], last: heaps[heaps.length - 1], min: Math.min(...heaps), max: Math.max(...heaps) } : null,
  requests: requests.length ? { first: requests[0], last: requests[requests.length - 1] } : null,
  timers_live: timers.length ? { first: timers[0], last: timers[timers.length - 1], min: Math.min(...timers), max: Math.max(...timers), spread: timerSpread } : null,
  sse: sseOpens.length ? { opens: sseOpens[sseOpens.length - 1], errors: sseErrors[sseErrors.length - 1] } : null,
  caveats: [
    "headless + SSH 隧道（参考证据，非人工走查/SLA）",
    "timers_live 稳定性阈值仅供参考；spread 大需检查记账或真实泄漏",
    "浏览器侧与 4h 控制面/数据面必须同一窗口",
  ],
};
if (!monotonic) summary.verdict = "FAIL_TIMESTAMPS_NOT_MONOTONIC";
else if (samples.length < 200) summary.verdict = "PARTIAL_SAMPLES_UNDER_200";
else if (crashed.length > 0) summary.verdict = "PARTIAL_CRASHED_ROWS";
else summary.verdict = "SAMPLES_OK_FOR_REVIEW";

await writeFile(path.join(outDir, "browser.jsonl"), rows.map(({ ts_ms, ...rest }) => JSON.stringify(rest)).join("\n") + "\n");
await writeFile(path.join(outDir, "browser-summary.json"), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
process.exit(summary.verdict === "SAMPLES_OK_FOR_REVIEW" ? 0 : 1);
