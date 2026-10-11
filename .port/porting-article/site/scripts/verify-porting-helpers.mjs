/** Unit tests for the porting widgets' pure helpers (scales, format, replay, tooltip state). */
import assert from "node:assert/strict";
import { linear, log, time, band, ticksLinear, ticksLog } from "../src/components/porting/lib/scales.ts";
import { fmtInt, fmtPct, fmtMs, fmtUsd, fmtRatio, fmtDatePT } from "../src/components/porting/lib/format.ts";
import { replayStart, replayStep } from "../src/components/porting/lib/replay.ts";
import { tooltipReducer } from "../src/components/porting/lib/tooltip-state.ts";
import { readdirSync, readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import * as avh from "../src/components/porting/agents-vs-human/model.ts";
import * as tc from "../src/components/porting/test-contract/model.ts";
import * as cc from "../src/components/porting/cutting-the-cycle/model.ts";
import * as tl from "../src/components/porting/translation-lens/model.ts";
import { tokenize } from "../src/components/porting/translation-lens/highlight.ts";
import * as tr from "../src/components/porting/three-reviewers/model.ts";
import * as ms from "../src/components/porting/memory-safety/model.ts";
import * as pr from "../src/components/porting/perf-replay/model.ts";
import * as mtm from "../src/components/porting/moving-target/model.ts";
import * as mts from "../src/components/porting/moving-target/stats.ts";
import * as prRows from "../src/components/porting/perf-replay/rows.ts";
import * as dm from "../src/components/porting/defect-map/model.ts";
import * as dml from "../src/components/porting/defect-map/layout.ts";
import * as vm from "../src/components/porting/verification-matrix/model.ts";
import * as costModel from "../src/components/porting/cost/model.ts";
import * as costStats from "../src/components/porting/cost/stats.ts";
import * as sc from "../src/components/porting/static-charts/model.ts";
import { rovingTarget } from "../src/components/porting/ui/useRovingFocus.ts";
import { commitUrl, resolveCommit, sourceUrl, splitCommits } from "../src/components/porting/lib/links.ts";

const x = linear([0, 10], [0, 100]);
assert.equal(x(5), 50);
assert.equal(x.invert(25), 2.5);
const l = log([0.3, 3.5], [150, 500]);
assert.ok(Math.abs(l(1) - (150 + (Math.log(1 / 0.3) / Math.log(3.5 / 0.3)) * 350)) < 1e-9);
const b = band(["a", "b", "c"], [0, 300], 0);
assert.equal(b("b"), 100);
assert.equal(b.bandwidth, 100);
assert.deepEqual(ticksLog([0.3, 3.5]), [0.5, 1, 2]);

assert.equal(fmtInt(2968), "2,968");
assert.equal(fmtPct(-21.7), "−21.7%");
assert.equal(fmtPct(4.2), "+4.2%");
assert.equal(fmtUsd(11032.43), "$11,032");
assert.equal(fmtUsd(23791, true), "$23.8k");
assert.equal(fmtRatio(2.849), "2.85×");

assert.deepEqual(replayStart(true, 13), { index: 13, playing: false });
assert.deepEqual(replayStart(false, 13), { index: 0, playing: false });
assert.deepEqual(replayStep({ index: 12, playing: true }, 13), { index: 13, playing: false });

let t = { pinned: null, hover: null };
t = tooltipReducer(t, { type: "enter", id: "a" });
assert.equal(t.pinned ?? t.hover, "a");
t = tooltipReducer(t, { type: "tap", id: "b" });
t = tooltipReducer(t, { type: "leave" });
assert.equal(t.pinned ?? t.hover, "b", "tap pins through mouse leave");
t = tooltipReducer(t, { type: "tap", id: "b" });
assert.equal(t.pinned, null, "second tap unpins");
t = tooltipReducer(tooltipReducer(t, { type: "focus", id: "c" }), { type: "escape" });
assert.equal(t.pinned ?? t.hover, null);

// Beyond the core contract: inverses, padding, nice ticks and the remaining formatters.
assert.ok(Math.abs(l.invert(l(2)) - 2) < 1e-9);
const tm = time([new Date("2026-06-20T00:00:00Z"), new Date("2026-06-30T00:00:00Z")], [0, 100]);
assert.equal(tm(new Date("2026-06-25T00:00:00Z")), 50);
assert.equal(tm.invert(50).toISOString(), "2026-06-25T00:00:00.000Z");
const bp = band(["a", "b"], [0, 210], 0.1);
assert.ok(Math.abs(bp.step - 100) < 1e-9 && Math.abs(bp("a") - 10) < 1e-9 && Math.abs(bp.bandwidth - 90) < 1e-9);
assert.ok(Number.isNaN(bp("z")));
assert.deepEqual(ticksLinear([0, 100], 5), [0, 20, 40, 60, 80, 100]);
assert.deepEqual(ticksLinear([0.1, 0.5], 4), [0.1, 0.2, 0.3, 0.4, 0.5]);
assert.deepEqual(ticksLog([1, 1000]), [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000]);
assert.equal(fmtPct(0), "0.0%");
assert.equal(fmtPct(12.345, 2, false), "12.35%");
assert.equal(fmtInt(-1234), "−1,234");
assert.equal(fmtMs(2269.4), "2,269 ms");
assert.equal(fmtMs(3.4167), "3.42 ms");
assert.equal(fmtUsd(420), "$420");
assert.equal(fmtDatePT("2026-07-16"), "Jul 16");
assert.equal(fmtDatePT("2026-07-16T05:00:00Z"), "Jul 15", "timestamps are shown in Pacific time");
// Agents vs human: whole-period tiles and the 09-23 → 09-24 unattended run.
const avhData = JSON.parse(readFileSync("src/app/porting/data/agents-vs-human.json", "utf8"));
const avhAll = avh.brushStats(avhData, 0, 110);
assert.equal(avhAll.n, 2718);
assert.equal(avhAll.nPrompts, 898);
assert.deepEqual(
  avh.insightTiles(avhData, avhAll).map((t) => t.value),
  ["52%", "18% vs 10%", "20h / 08h", "3.0×", "180", "79 h"],
);
assert.deepEqual(
  avh.insightTiles(avhData, avhAll).map((t) => t.text),
  [
    "of 2,718 commits landed >1 h after the last prompt; 9% after >8 h",
    "overnight (00–06 PT) share: commits vs prompts",
    "peak hour for commits / for prompts",
    "commits per prompt (898 prompts)",
    "longest unattended run: commits between two prompts, Sep 23 19:00 → Sep 24 06:33",
    "hands-on prompting time → 34 commits per hands-on hour; 3 days had commits and no prompt",
  ],
);
const avhDay = (date) => avhData.daily.date.indexOf(date);
const avhRun = avh.brushStats(avhData, avhDay("2026-09-23"), avhDay("2026-09-24"));
assert.equal(avhRun.bestRun?.commits, 180);
assert.equal(avh.minuteLabel(avhData, avhData.prompts[avhRun.bestRun.prompt]), "Sep 23 19:00");
// Median lags: 129 min renders "2.1 h" with the mockup's toFixed(1) (the summary file rounds it to 2.15 h).
const avhEras = avh.autonomy(avhData, 0, 110);
assert.deepEqual(avhEras.map((e) => e.commits), [452, 327, 125, 488, 1042, 284]);
assert.equal(avhEras[0].noPrompt, 41);
assert.deepEqual(avhEras.map((e) => e.commitsPerPrompt.toFixed(1)), ["3.9", "1.6", "0.7", "3.0", "9.1", "2.4"]);
assert.deepEqual(avhEras.map((e) => avh.fmtLag(e.medianLag)), ["2.1 h", "1.2 h", "12 min", "34 min", "1.7 h", "1.1 h"]);
const avhProfile = avh.hourProfile(avhData, 0, 110);
assert.equal(avhProfile.commits.length, 24);
assert.ok(Math.abs(avhProfile.commits.reduce((a, b) => a + b, 0) - 1) < 1e-9);
// Test contract: week-1 coverage, the first contract met, sync table and the ≥2× modules.
const tcData = JSON.parse(readFileSync("src/app/porting/data/test-contract.json", "utf8"));
const tcMods = tc.expandMods(tcData);
const tcWeek1 = tc.coverageOn(tcData, tcMods, "2026-06-28");
assert.deepEqual([tcWeek1.met, tcWeek1.need, tcWeek1.below, tcWeek1.deficit], [2684, 3134, 60, 450]);
assert.equal(tcData.dates[tc.firstAllMet(tcData, tcMods, 0)], "2026-07-30");
assert.equal(tcData.dates[tc.engineFirstMet(tcData, tcMods)], "2026-06-25");
assert.deepEqual(
  tc.deficitByCrate(tcData, tcMods, tcData.dates.indexOf("2026-06-28")).slice(0, 5),
  [["rustty-font-shaper", 103], ["rustty-xev", 95], ["rustty-vaxis", 87], ["rustty-cli", 42], ["rustty-termio", 41]],
);
const tcLast = tc.coverageOn(tcData, tcMods, "2026-10-10");
assert.deepEqual([tcLast.met, tcLast.need, tcLast.below], [4182, 4182, 0]);
const tcOrder = tc.gridOrder(tcMods);
assert.equal(tcOrder.total, 4182);
const tcSync = tc.syncTable(tcData, tcMods);
assert.equal(tcSync.rows.length, 8);
assert.deepEqual([tcSync.added, tcSync.pre], [1049, 616], "fact contract_precovered");
assert.deepEqual([tcSync.rows[7].added, tcSync.rows[7].pre], [10, 10], "the eighth move was covered in advance");
assert.equal(tcSync.rows[2].added, 369, "largest sync: 3");
assert.equal(tc.longestGap(tcData, tcMods).days, 55);
const tcTwo = tc.doubled(tcData, tcMods);
assert.deepEqual([tcTwo.mods.length, tcTwo.zig, tcTwo.rust], [33, 125, 461]);
const tcPts = tc.scatterPoints(tcData, tcMods);
assert.equal(tcPts.length, 211);
assert.ok(tcPts.every((p) => p.r >= p.z), "no module below the diagonal");
const tcL = tc.gridLayout(4182, 958);
assert.deepEqual([tcL.cell, tcL.rows, tcL.cols], [7, 31, 135]);
assert.deepEqual(tc.gridHit(tcL, 0, 0), 0);
assert.equal(tc.gridHit(tcL, 7 * 2 + 1, 7 * 3 + 1), 2 * 31 + 3);
assert.equal(tc.replayDelay(0), 320);
assert.equal(tc.replayDelay(28), 110);
// Cutting the cycle: the shatter curve, the longest chain, shortest cycles and the replay's last day.
const ccData = JSON.parse(readFileSync("src/app/porting/data/cutting-the-cycle.json", "utf8"));
assert.equal(cc.cycleSizeAfter(ccData, 0), 332, "entry points as pure sources");
assert.equal(cc.cycleSizeAfter(ccData, 13), 219);
assert.equal(cc.cycleSizeAfter(ccData, 14), 45);
assert.equal(cc.cycleSizeAfter(ccData, 15), 43, "with the crash seam");
assert.deepEqual(cc.shatterCurve(ccData), ccData.zig.stats.cum);
const ccChain = cc.longestChain(ccData);
assert.equal(ccChain.length, 19, "18 hops");
assert.deepEqual(ccChain, ccData.crates.chain);
const ccGraph = cc.fileGraph(ccData);
const ccKeymap = ccData.zig.files.findIndex((f) => f[0] === "input/KeymapDarwin.zig");
assert.equal(cc.shortestCycle(ccGraph, ccKeymap).length - 1, 9, "longest shortest loop");
assert.deepEqual(cc.cycleLengthCounts(ccGraph), ccData.zig.stats.cycle_len_dist);
assert.deepEqual(cc.hubs(ccData, 3).map((h) => [h.name, h.in]), [["quirks.zig", 113], ["font/main.zig", 47], ["terminal/main.zig", 47]]);
assert.deepEqual(cc.mechanismCounts(ccData), { trait: 6, move: 5, split: 2, inject: 1, other: 0 });
const ccLast = cc.replayFrame(ccData, ccData.replay.n - 1);
assert.deepEqual([ccLast.born, ccLast.tests], [61, 7284]);
assert.equal(ccLast.total, 836905, "tracked non-vendor physical .rs lines on the last day (fact-checked)");
assert.equal(cc.replayFrame(ccData, 0).total, 33040, "physical lines on day 1 (fact-checked)");
assert.ok(ccLast.share.every((v) => v === null || Math.abs(v - 1) < 1e-9), "every depth complete on the last day");
assert.equal(cc.replayFrame(ccData, 0).tests, 314);
// Translation lens: line mappings are symmetric, panes classify lines, the computed insights.
const tlData = JSON.parse(readFileSync("src/app/porting/data/translation-lens.json", "utf8"));
const tlRaw = tlData.patterns[0].subs[0];
const range = (a, b) => Array.from({ length: b - a + 1 }, (_, i) => a + i);
assert.deepEqual(tl.mappedLines(tlRaw, "z", 1758), range(165, 171));
assert.deepEqual(tl.mappedLines(tlRaw, "z", 1763), [...range(180, 183), ...range(304, 308)]);
assert.deepEqual(tl.mappedLines(tlRaw, "r", 304), range(1763, 1767), "Rust side maps back");
assert.deepEqual(tl.mappedLines(tlRaw, "z", 1749), [148], "carried comment");
assert.deepEqual(tl.mappedLines(tlRaw, "z", 1759), [], "blank line has no mapping");
for (const p of tlData.patterns) {
  for (const sub of p.subs) {
    assert.ok(sub.map.length >= 1, `${p.id}/${sub.key} has a mapping`);
    for (const [zs, rs] of sub.map) {
      for (const z of zs) {
        const got = tl.mappedLines(sub, "z", z);
        for (const r of rs) {
          assert.ok(got.includes(r), `${p.id}/${sub.key}: Zig ${z} → Rust ${r}`);
          assert.ok(tl.mappedLines(sub, "r", r).includes(z), `${p.id}/${sub.key}: Rust ${r} → Zig ${z}`);
        }
      }
    }
  }
}
const tlLink = tl.linkedLines(tlRaw, "z", 1763);
assert.deepEqual([tlLink.z, tlLink.r.length, tlLink.groups.length], [range(1763, 1767), 9, 1]);
assert.deepEqual(tl.linkedLines(tlRaw, "z", 1759), { z: [1759], r: [], groups: [] });
assert.equal(tl.segText(tlRaw.zig), "1749–1796, 1842–1849");
assert.equal(tl.rangeText([1763, 1764, 1765, 1766, 1767, 1749]), "1749, 1763–1767");
const tlKinds = tl.lineKinds(tlRaw);
assert.equal(tlKinds.r.get(148), "carried");
assert.equal(tlKinds.r.get(165), "echo");
assert.equal(tlKinds.r.get(152), "note", "PORT NOTE line");
assert.equal(tlKinds.z.get(1749), "carried");
assert.ok(tl.ribbons(tlRaw, 440).length >= tlRaw.map.length, "one ribbon per mapping run at least");
assert.deepEqual(tl.moduleTotals(tlData.modules, 1), [65725, 168505]);
assert.deepEqual(tl.moduleTotals(tlData.modules, 2), [4901, 7204]);
assert.deepEqual(tl.patternCrates(tlData.patterns[0]).top, ["rustty-xev", "rustty-vt", "rustty-renderer", "rustty-os", "rustty-lib", "rustty-apprt"]);
assert.deepEqual(tl.harnessTally(tlData.patterns[1].harness), { ub: 24, clean: 18 });
assert.deepEqual(tl.zigFieldLine(tlData.patterns[3].subs[0], "style_id"), 2162);
assert.deepEqual(tl.perfGroups(tlData.patterns[2].perf)[0].steps.map((s) => s.value), [295, 85, 28, 16]);
assert.deepEqual(
  tl.insights(tlData).map((i) => i.value),
  ["43%", "95 → 57", "1.68×", "62 / 73", "228 → 54", "696"],
);
assert.deepEqual(
  tokenize("pub fn clearCells(self: *Screen) void {", "zig").filter((t) => t.k),
  [
    { t: "pub", k: "keyword" },
    { t: "fn", k: "keyword" },
    { t: "Screen", k: "type" },
    { t: "void", k: "type" },
  ],
);
const tlToks = tokenize('    unsafe { (*page).x("s") }; // c', "rust");
assert.ok(tlToks.some((t) => t.t === "unsafe" && t.k === "unsafe"));
assert.ok(tlToks.some((t) => t.t === '"s"' && t.k === "string"));
assert.ok(tlToks.some((t) => t.t === "// c" && t.k === "comment"));
assert.equal(tlToks.map((t) => t.t).join(""), '    unsafe { (*page).x("s") }; // c', "tokens cover the line");
assert.ok(tokenize("comptime T: type", "zig").some((t) => t.t === "comptime" && t.k === "keyword"));
assert.ok(tokenize("@fieldParentPtr(x)", "zig").some((t) => t.t === "@fieldParentPtr" && t.k === "built_in"));
// Three reviewers: stackAt replays the borrow stack; the flagged invalid frame equals the mockup's.
const trData = JSON.parse(readFileSync("src/app/porting/data/three-reviewers.json", "utf8"));
const trWant = {
  clear: { sb: [["2475594", "protected", null]], tb: [["2366379", "protected", null]] },
  shaper: { sb: [["3065", "popped", 2]], tb: [["2954", "popped", 1]] },
  search: { sb: [["4954250", "popped", 4]], tb: [["4739779", "popped", 2], ["4781909", "state", null]] },
  mpsc: { sb: [["2681", "popped", 3]], tb: [["2627", "popped", 2], ["2666", "state", null]] },
};
for (const bug of trData.bugs) {
  for (const model of ["sb", "tb"]) {
    const steps = bug.miri[model];
    const last = steps.length - 1;
    const f = tr.stackAt(bug, model, last);
    const where = `${bug.key}/${model}`;
    assert.deepEqual(f.invalid.map((v) => [v.id, v.reason, v.since]), trWant[bug.key][model], where);
    assert.deepEqual(
      f.invalid.map((v) => v.id),
      steps[last].items.filter((i) => i.st === "bad").map((i) => i.id),
      `${where}: the mockup's flagged frame`,
    );
    assert.equal(f.access?.ok, false, `${where}: last step is the UB access`);
    for (let k = 0; k < last; k++) assert.deepEqual(tr.stackAt(bug, model, k).invalid, [], `${where} step ${k}`);
    for (const v of f.invalid.filter((x) => x.reason === "popped")) {
      assert.ok(!f.live.includes(v.id), `${where}: ${v.id} was popped before its use`);
    }
    for (const v of f.invalid.filter((x) => x.reason === "protected")) {
      assert.ok(f.live.includes(v.id), `${where}: protected ${v.id} is still on the stack`);
    }
    const shown = f.items.map((i) => i.id);
    const raw = steps[last].items.map((i) => i.id);
    assert.deepEqual(shown, model === "sb" ? [...raw].reverse() : raw, `${where}: SB top first, TB in tree order`);
  }
}
assert.deepEqual(trData.bugs.map((b) => [b.miri.sb.length, b.miri.tb.length]), [[5, 3], [5, 3], [6, 4], [5, 4]]);
assert.deepEqual(tr.stackAt(trData.bugs[0], "sb", 1).pushed, ["2475590"], "caller reborrow pushes a Unique tag");
assert.deepEqual(tr.stackAt(trData.bugs[2], "sb", 4).popped, ["4954250"], "whole-Screen retag pops the stored alias");
assert.deepEqual(
  tr.insights(trData).map((i) => i.value),
  ["57 / 80", "55 / 80", "29 / 37", "17"],
);
assert.match(tr.insights(trData)[0].text, /\b62\b/);
assert.match(tr.insights(trData)[1].text, /20 of 37 events/);
assert.match(tr.insights(trData)[2].text, /61 defects/);
assert.match(tr.insights(trData)[3].text, /13 of them in the October Tree Borrows lane, all test-side/);
assert.deepEqual(trData.bugs.map((b) => tr.latency(b.life)), ["6.9 h", "3.4 days", "16.0 days", "8.8 days"]);
const trTot = tr.laneTotals(trData);
assert.deepEqual([trTot.reduce((a, l) => a + l.events, 0), trTot.reduce((a, l) => a + l.defects, 0)], [37, 80]);
assert.equal(tr.railX("2026-06-21"), tr.RAIL.x0);
assert.equal(tr.railX("2026-10-08"), tr.RAIL.x1);
const trDots = tr.railDots(trData);
assert.equal(trDots.length, 37);
assert.deepEqual(trData.bugs.map((b) => tr.bugEvent(trData, b) >= 0), [true, true, true, true]);
const trMarks = tr.codeMarks(trData.bugs[0].miri.sb, 4);
assert.ok(trMarks.bad && trMarks.hi.has("page_impl.rs (verify_integrity):550") && trMarks.hi.has("screen.rs (clear_cells):2040"));
assert.ok(trMarks.seen.has("screen.rs (clear_rows):1907") && !trMarks.seen.has("page_impl.rs (verify_integrity):550"));
const trHit = tr.termHighlighter(trData.bugs[0].miri.sb[4].at);
assert.ok(trHit("   --> crates/rustty-vt/src/page_impl.rs:550:17"));
assert.ok(trHit("550 |                 core::slice::from_raw_parts(row.cells().ptr(base), self.size.cols as usize)"));
assert.ok(!trHit("1945 |     pub fn clear_cells(&mut self, page: &mut Page, row: &mut Row, cells: &mut [Cell]) {"));
assert.equal(tr.termLineKind("error: Undefined Behavior: x"), "error");
assert.equal(tr.termLineKind("test result: ok. 1 passed; 0 failed"), "ok");
assert.equal(tr.termLineKind("boxq: other=4"), "ok");
assert.deepEqual(tr.linkParts("commit 3ee9fd06d, 2026-06-24 19:38"), [
  { text: "commit " },
  { hash: "3ee9fd06d" },
  { text: ", 2026-06-24 19:38" },
]);
assert.deepEqual(tr.linkParts("before fix (9e3fe1c25~1)"), [{ text: "before fix (9e3fe1c25~1)" }], "a parent ref is not the fix commit");
assert.deepEqual(tr.mdParts("a `&mut` b"), [{ text: "a " }, { code: "&mut" }, { text: " b" }]);
assert.equal(tr.indentOf(trData.bugs[0].impl.segs[2].lines), 4);
assert.deepEqual(tr.barPercents(trData.bugs[0].count.series), [59.8, 62.2, 100]);
assert.equal(tr.birthDots(trData.bugs[1].count.births).length, 33);
assert.equal(tr.diffKind("@@ -1,2 +1,2 @@"), "hunk");
assert.equal(tr.diffKind("+x"), "add");
assert.equal(tr.diffKind("-x"), "del");
assert.deepEqual(tr.headline(trData), { miri: 62, total: 80, events: 37, first: "2026-06-22", last: "2026-10-08" });
assert.deepEqual(trData.bugs.map((b) => tr.bugModels(b)), Array(4).fill({ sb: true, tb: true }));
assert.equal(tr.modelsText({ models: "SB" }), "Stacked Borrows");
assert.equal(tr.modelsText(trData.bugs[0]), "Stacked and Tree Borrows");
assert.equal(tr.stateNote({ id: "x", perm: "Frozen", label: "", st: "pop" }, "tb"), "invalidated: now Frozen");
assert.equal(tr.stateNote({ id: "x", perm: "Unique", label: "", st: "pop" }, "sb"), "popped");
for (const bug of trData.bugs) {
  for (const st of bug.miri.tb) {
    for (const it of st.items) if (it.st === "pop") assert.match(tr.stateNote(it, "tb"), /^invalidated: now (Frozen|Disabled)$/, bug.key);
  }
}

// memory-safety: the curated events, the cumulative curves and the crate x class table.
const msData = JSON.parse(readFileSync(new URL("../src/app/porting/data/memory-safety.json", import.meta.url), "utf8"));
const msHead = ms.headline(msData.events);
assert.deepEqual(msHead, { defects: 83, events: 39, miri: 64, prodEvents: 29, prodDefects: 61, alias: 57, prov: 5, aliasOrProv: 62, aliasOrProvProd: 43 });
const msDays = ms.dayTicks();
const msEnd = (key) => ms.cumulative(msData.events, ms.SERIES_DEFS.find((d) => d.key === key).test, msDays).at(-1).v;
assert.deepEqual(["all", "miri", "prod"].map(msEnd), [83, 64, 61], "cumulative curve endpoints (the axis reaches the 10-09 events)");
const msCum = ms.cumulative(msData.events, () => true, msDays);
assert.ok(msCum.every((p, i) => i === 0 || p.v >= msCum[i - 1].v), "cumulative is monotone");
assert.equal(msCum[0].v, 0);
assert.deepEqual(ms.laneTotals(msData.events, "lane"), { sb: 30, har: 19, tb: 15, san: 7, rev: 4, sym: 8 });
assert.equal(Object.values(ms.laneTotals(msData.events, "crate")).reduce((a, b) => a + b, 0), 83);
const msHeat = ms.heat(msData.events);
assert.equal(msHeat.max, 24, "vt aliasing is the largest cell");
assert.equal(msHeat.rowTotals.vt + msHeat.rowTotals["vt-c"] + msHeat.rowTotals.xev + msHeat.rowTotals.app + msHeat.rowTotals.other, 83);
assert.equal(msHeat.cell("vt", "alias").events > 0, true);
for (const [c] of ms.CRATES) assert.ok(msHeat.cell(c, "alias").n + msHeat.cell(c, "prov").n > 0, `${c} has aliasing or provenance defects`);
assert.equal(ms.scopeKind("production + test"), "prod");
assert.equal(ms.scopeKind("bench"), "bench");
assert.equal(ms.scopeKind("test harness + latent production"), "test");
assert.equal(ms.matches({ crate: "vt", cls: "alias" }, ["vt", "alias"]), true);
assert.equal(ms.matches({ crate: "xev", cls: "alias" }, ["vt", "alias"]), false);
assert.equal(ms.matches({ crate: "xev", cls: "alias" }, null), true);
const msPlaced = ms.placeBubbles(msData.events, "lane");
assert.equal(msPlaced.length, 39);
for (const group of ["lane", "crate"]) {
  const pl = ms.placeBubbles(msData.events, group);
  assert.ok(pl.every((p) => Math.abs(p.dy) <= 20), "offsets stay within the row");
}
assert.equal(ms.refText(msData.events[0]), "64f3861bc (commit)");
assert.match(ms.refText(msData.events.find((e) => e.entries === 4)), /port-findings ledger \(4 entries\)/);

// moving-target: the pin step function, lag area, hover lookups, matrix values and header figures.
const mtData = JSON.parse(readFileSync(new URL("../src/app/porting/data/moving-target.json", import.meta.url), "utf8"));
const mtT = mts.totals(mtData);
assert.equal(mtT.nonMerge, 1169);
assert.deepEqual([mtT.phaseIns, mtT.phaseDel], [185622, 33862], "stat tile: the seven ranges with a diffstat");
assert.deepEqual([mtT.matrixIns, mtT.matrixDel, mtT.files, mtT.era], [185834, 33878, 1447, 700], "matrix all-moves column");
assert.deepEqual([mtT.firstLag, mtT.minLag, mtT.medianLag], [47.6, 1.7, 7.55]);
assert.deepEqual(mtT.quickMoves, [3, 5]);
assert.deepEqual([mtT.lastLag, mtT.minMove, mtT.driftN, mtT.driftIns, mtT.driftDel], [7.7, 3, 4, 212, 16]);
assert.equal(mtT.growthPct.toFixed(1), "34.0");
assert.deepEqual([mtT.loc0, mtT.loc1], [273827, 367013]);
assert.deepEqual([mtT.terminalChurn, mtT.totalChurn, Math.round(mtT.terminalPct)], [95301, 219712, 43]);
assert.deepEqual([mtT.poChurn, mtT.pkgChurn], [39679, 21363]);
assert.deepEqual(mtm.maxLag(mtData), { date: "2026-08-15", main: 328622, pin: 297110, lag: 31512, pct: 31512 / 297110 });
assert.equal((mtm.maxLag(mtData).pct * 100).toFixed(1), "10.6");
// the pin step function holds its value and jumps at each move
const mtSteps = mtm.pinSteps(mtData);
assert.equal(mtSteps.length, 2 * mtData.adv.length + 2);
assert.ok(mtSteps.every((p, i) => i === 0 || p[0] >= mtSteps[i - 1][0]), "steps are in time order");
assert.equal(mtm.pinAt(mtData, mtm.tm("2026-07-30")), 273827);
assert.equal(mtm.pinAt(mtData, mtm.tm("2026-07-31T07:55")), 297110);
assert.equal(mtm.pinAt(mtData, mtm.tm("2026-10-08")), 366687);
assert.equal(mtm.pinAt(mtData, mtm.tm("2026-10-10")), 367013);
assert.equal(mtm.moveAt(mtData, mtm.tm("2026-06-25")), 0);
assert.equal(mtm.moveAt(mtData, mtm.tm("2026-08-26T01:00")), 4);
// the lag area is closed: main is never below the pin inside it
const mtArea = mtm.lagArea(mtData);
assert.equal(mtArea.top.length, mtArea.bottom.length);
assert.ok(mtArea.top.every((p, i) => p[1] >= mtArea.bottom[i][1]));
// hover lookups
const mtTip0 = mtm.tipAt(mtData, mtm.tm("2026-06-21"));
assert.equal(mtTip0.pinSha, "4749c4e93");
assert.equal(mtTip0.ratio.toFixed(2), "0.12");
const mtTip1 = mtm.tipAt(mtData, mtm.tm("2026-10-10"));
assert.equal(mtTip1.ratio.toFixed(2), "2.34");
assert.equal(mtTip1.pinSha, "246f70287");
assert.equal(mtm.tipAt(mtData, mtm.tm("2026-08-15")).trail, 31512);
assert.equal(mtm.tipAt(mtData, mtm.tm("2026-06-20")).rs, null, "no Rustty sample before 06-21");
assert.equal(mtm.nearestAdvance(mtData, mtm.tm("2026-08-24")), 2);
assert.equal(mtm.nearestAdvance(mtData, mtm.tm("2026-10-08")), 6);
assert.equal(mtm.nearestAdvance(mtData, mtm.tm("2026-10-09T18:00")), 7);
assert.equal(mtm.moveName(mtData.adv[3]), "move 4 (2-commit drift)");
assert.equal(mtm.moveName(mtData.adv[0]), "move 1");
assert.ok(mtm.xOf(mtm.tm("2026-06-20")) >= mtm.GEO.L && mtm.xOf(mtm.tm("2026-10-10")) <= mtm.GEO.W - mtm.GEO.R + 1e-9);
assert.ok(Math.abs(mtm.timeAt(mtm.xOf(mtm.tm("2026-08-01"))) - mtm.tm("2026-08-01")) < 1);
assert.ok(mtm.yA(260000) > mtm.yA(380000), "higher count plots higher");
// matrix values
assert.equal(mts.median([3, 1, 2]), 2);
assert.equal(mts.median([4, 1, 2, 3]), 2.5);
assert.deepEqual(mts.cellLabel(0, "churn"), "·");
assert.equal(mts.cellLabel(950, "churn"), "950");
assert.equal(mts.cellLabel(1234, "churn"), "1.2k");
assert.equal(mts.cellLabel(27267 + 2289, "churn"), "30k");
assert.equal(mts.cellLabel(1234, "commits"), "1234");
assert.equal(mts.cellAlpha(0, 100), 0);
assert.equal(mts.cellAlpha(100, 100), 0.95);
assert.equal(mtData.rows.length, 20);
const mtTerm = mtData.rows.find((r) => r.a === "src/terminal");
assert.equal(mts.rowTotal(mtTerm, "churn"), 95301);
assert.equal(mts.cellVal(mtTerm.v[0], "commits"), 127);
assert.equal(mts.cellVal(null, "churn"), 0);
assert.equal(mts.matrixMax(mtData, "churn"), Math.max(...mtData.rows.flatMap((r) => r.v.map((c) => (c ? c[0] + c[1] : 0)))));
const mtTop = mts.topAreas(mtData, 1);
assert.equal(mtTop.length, 5);
assert.deepEqual(mtTop.slice(0, 2).map((x) => x.name), ["src/terminal", "po (translations)"], "move 2: the engine, then the translation catalogues");
assert.ok(mtTop.every((x, i) => i === 0 || x.churn <= mtTop[i - 1].churn), "bars are ranked");
const mtIns = mts.insights(mtData);
assert.deepEqual(mtIns.map((i) => i.n), ["47.6 h / 1.7 h / 7.7 h", "1,230 / 1,536", "95,301 lines"]);
assert.match(mtIns[0].t, /first move 47\.6 h, as little as 1\.7 h \(move 3\), last move 7\.7 h; median 7\.55 h\. Moves 3 and 5/);
assert.doesNotMatch(mtIns[0].t, /fell|once the rule/);
assert.match(mtIns[2].t, /\(43%\)/);

// verification-matrix: cell lookup, the 11 x 4 grid, no pending cell once results exist, hero total, history layout, found and duration series.
const vmData = JSON.parse(readFileSync(new URL("../src/app/porting/data/verification-matrix.json", import.meta.url), "utf8"));
assert.equal(vmData.tools.length, 11);
assert.equal(vmData.hosts.length, 4);
assert.equal(vmData.pins.length, 9);
const vmCurrent = vmData.pins.length - 1;
const vmPrev = vmCurrent - 1;
const vmFirst = vmData.pins.findIndex((p) => p.sha === "12752b2ac");
assert.equal(vmFirst, 6, "the first complete matrix");
// every cell of the grid at every commit is exactly one of: a run, "not available", "no record"
for (let p = 0; p < vmData.pins.length; p += 1) {
  let runs = 0;
  for (const t of vmData.tools) {
    for (const h of vmData.hosts) {
      const c = vm.cellView(vmData, p, t.key, h.key);
      assert.ok(["run", "na", "none"].includes(c.kind));
      if (c.kind === "run") runs += 1;
      if (c.kind === "na") assert.equal(vm.cellRuns(vmData, p, t.key, h.key).length, 0, "unavailable cells hold no run");
    }
  }
  assert.equal(runs, vmData.runs.filter((r) => r.pin === p).map((r) => `${r.tool}|${r.host}`).filter((k, n, a) => a.indexOf(k) === n).length, "one cell per tool and host with a run");
}
// a cell holding several runs shows the latest, sums the defects and counts the earlier runs
const vmLsanMac = vm.cellView(vmData, vmFirst, "lsan", "mac");
assert.equal(vmLsanMac.kind, "run");
// partial counts (one crate of many) are not shown as a cell total; targeted suites are tagged
assert.equal(vm.cellView(vmData, vmFirst, "ts", "lin").run.tests, null);
assert.equal(vm.cellView(vmData, vmPrev, "ts", "lin").run.targeted, false, "the full Linux run replaced the targeted one");
assert.equal(vm.cellView(vmData, vmFirst, "sb", "mac").run.tests, null);
assert.equal(vm.cellView(vmData, vmFirst, "ts", "msvc").run.tests, 442);
assert.deepEqual([vmLsanMac.run.tests, vmLsanMac.run.secs, vmLsanMac.port, vmLsanMac.gh, vmLsanMac.earlier], [9850, 1980, 2, 1, 0]);
const vmSbFirst = vm.cellView(vmData, vmFirst, "sb", "mac");
assert.equal(vmSbFirst.run.secs, 13354);
assert.equal(vmSbFirst.run.secs, vmData.hero.miriSecs);
assert.equal(vmData.runs.length, 65);
assert.equal(vmData.runs[vmSbFirst.index], vmSbFirst.run);
const vmMultiple = vm.cellView(vmData, 0, "sb", "mac");
assert.deepEqual([vmMultiple.earlier, vmMultiple.port, vmMultiple.run.date], [1, 11, "07-30"], "latest run, defects summed over both");
// unavailable and unrecorded cells
assert.deepEqual(vm.cellView(vmData, vmCurrent, "msan", "mac"), { kind: "na", reason: vmData.na["msan|mac"] });
assert.match(vmData.na["msan|mac"], /MemorySanitizer is not available on macOS/);
assert.equal(vm.cellView(vmData, vmPrev, "msan", "gnu").kind, "none");
assert.equal(vm.cellView(vmData, vmCurrent, "msan", "lin").kind, "none", "Linux sanitizers not yet re-run at the current commit");
assert.equal(vm.cellView(vmData, 3, "ts", "mac").kind, "none", "no record at the third move");
// the cells that were pending at the previous commit now hold results; nothing is pending anywhere
for (const t of ["asan", "lsan", "msan", "tsan", "hwasan", "vg"]) {
  assert.ok(["green", "fixed"].includes(vm.cellView(vmData, vmPrev, t, "lin").run.st), t);
}
for (const t of ["sb", "tb", "capi"]) {
  assert.ok(["green", "fixed"].includes(vm.cellView(vmData, vmPrev, t, "mac").run.st), t);
  assert.equal(vm.cellView(vmData, vmCurrent, t, "mac").run.st, "green", t);
}
assert.equal(vmData.runs.filter((r) => r.st === "pending").length, 0);
assert.equal(vm.cellView(vmData, vmCurrent, "ts", "msvc").run.tests, 6418);
// the headline: the eight lanes of the first complete matrix (61,403 at 12752b2ac), each at its latest completed run
assert.equal(vm.sanitizerTotal(vmData, vmFirst), 61403);
assert.deepEqual(vm.sanitizerRuns(vmData, vmFirst).map((r) => r.tests).sort((a, b) => a - b), [4380, 4428, 6183, 6614, 9850, 9850, 9987, 10111]);
assert.equal(vmData.hero.runs, 58767);
assert.equal(vmData.hero.lanes.reduce((a, l) => a + l.tests, 0), vmData.hero.runs);
assert.deepEqual(vm.heroByPin(vmData).map(([p, ls]) => [p, ls.length]), [["246f70287", 4], ["a60e9e2a5", 4]]);
for (const l of vmData.hero.lanes) {
  const c = vm.cellView(vmData, vmData.pins.findIndex((p) => p.sha === l.pin), l.tool, l.host);
  assert.equal(c.run.tests, l.tests, `${l.tool}|${l.host}`);
}
// status strip and tags
assert.equal(vm.pinStrip(vmData, vmCurrent).length, 12);
assert.equal(vm.pinStrip(vmData, 3).length, 0);
assert.deepEqual(Object.keys(vm.STATUS_INFO), ["green", "fixed", "incomplete", "pending"]);
assert.equal(new Set(Object.values(vm.STATUS_INFO).map((s) => s.shape)).size, 4, "each status has its own shape");
assert.equal(vm.fmt(61403), "61,403");
assert.equal(vm.fmt(null), "");
assert.deepEqual([vm.dur(null), vm.dur(50), vm.dur(900), vm.dur(1980), vm.dur(13354), vm.dur(26220)], ["", "50 s", "15 min", "33 min", "3.7 h", "7.3 h"]);
assert.equal(vm.toolName(vmData, "asan"), "AddressSanitizer");
assert.equal(vm.runHeading(vmData, vmLsanMac.run), "LeakSanitizer · macOS");
// defects found, by tool and overall
const vmFound = vm.foundByTool(vmData);
assert.deepEqual(vmFound.slice(0, 3).map((r) => [r.key, r.port, r.gh]), [["sb", 18, 0], ["tb", 15, 0], ["capi", 8, 0]]);
assert.equal(vmFound.filter((r) => !["sb", "tb", "capi"].includes(r.key)).reduce((a, r) => a + r.port, 0), 10, "fact matrix_other_bugs");
assert.ok(vmFound.every((r, i) => i === 0 || r.port + r.gh <= vmFound[i - 1].port + vmFound[i - 1].gh), "ranked");
assert.deepEqual(vm.defectTotals(vmData), { port: vmFound.reduce((a, r) => a + r.port, 0), gh: vmFound.reduce((a, r) => a + r.gh, 0) });
assert.equal(vm.defectTotals(vmData).gh, 4);
assert.equal(vmData.hero.ghostty, 3, "Ghostty-side defects found by sanitizers");
// lane history: one dot per run, pending dots are small, a bigger find is a bigger dot
const vmDots = vm.historyDots(vmData);
assert.equal(vmDots.length, vmData.runs.length);
assert.ok(vmDots.every((d) => d.x > vm.HIST.left && d.x < vm.HIST.width && d.y > vm.HIST.top));
assert.ok(vmDots.filter((d) => d.st === "pending").every((d) => d.r === 3.5));
assert.ok(vmDots.find((d) => vmData.runs[d.k].port === 13).r > vmDots.find((d) => vmData.runs[d.k].port === 1).r);
assert.equal(vm.histX(vmData, 7) > vm.histX(vmData, 0), true);
assert.equal(vm.histHeight(11), vm.HIST.top + 11 * vm.HIST.row + vm.HIST.pad);
// wall-time bars: ranked, the longest is the two Tree Borrows runs
const vmDur = vm.durationRows(vmData);
assert.equal(vmDur[0].secs, 26220);
assert.ok(vmDur.every((d, i) => i === 0 || d.secs <= vmDur[i - 1].secs));
assert.equal(vm.durFraction(vmData, 26220), 1);
assert.equal(vmDur.length, 11);

// perf-replay: the 14 sweeps of 07-15/16, the hero figures, chapters and computed insights.
const prData = JSON.parse(readFileSync(new URL("../src/app/porting/data/perf-replay.json", import.meta.url), "utf8"));
const prLast = prData.stages.length - 1;
assert.deepEqual(pr.scoreboard(prData, prLast), { W: 14, T: 0, L: 0 }, "scoreboard at the last stage");
assert.equal(pr.geomean(prData, prLast), 0.758, "geomean at the last July stage");
assert.deepEqual(pr.scoreboard(prData, 0), { W: 0, T: 1, L: 13 });
assert.equal(pr.geomean(prData, 0), 1.592);
prData.stages.forEach((st, i) => {
  assert.deepEqual(pr.scoreboard(prData, i), { W: st.W, T: st.T, L: st.L }, st.key);
  assert.equal(pr.geomean(prData, i), st.geo, st.key);
  assert.equal(Math.round(pr.suite(prData, i).rust * 10) / 10, st.suiteR, st.key);
});
assert.deepEqual(pr.suite(prData, prLast).deltaPct.toFixed(1), "-21.7");
const prHero = pr.hero(prData);
assert.deepEqual([prHero.perfCommits, prHero.tParser.toFixed(2), prHero.clone.toFixed(2)], [29, "2.85", "2.42"]);
assert.deepEqual([prHero.from, prHero.to], ["07-15 23:57", "07-16 10:41"]);
assert.equal(pr.finalOrder(prData)[0], "terminal-parser", "largest final speed-up first");
assert.equal(pr.outcome(1.014), "tie");
assert.equal(pr.outcome(0.98), "win");
assert.equal(pr.interp(0.5, 2, 0), 0.5);
assert.ok(Math.abs(pr.interp(0.5, 2, 0.5) - 1) < 1e-12, "log-space tween");
assert.equal(pr.ease(0.5), 0.5);
assert.equal(pr.stageCommits(prData, 0).length, 1);
assert.equal(pr.stageCommits(prData, 2).length, 0, "same code re-measured: no commits");
assert.equal(pr.stageCommits(prData, 3).length, 14, "round 1: 01:18 → 03:20");
assert.equal(pr.cmdTitle(prData, prLast, "terminal-parser"), "$ rustty-bench +terminal-parser --data=…/corpora/utf8.bin  # → perf-v5/r5ship.jsonl");
assert.equal(pr.commitKind("Revert \"x\""), "revert");
assert.equal(pr.commitKind("rustty: vt — parity fix: x"), "parity");
assert.equal(pr.listSubject("rustty: perf: inline packed Style::eql"), "inline packed Style::eql");
const prF33 = pr.full33Summary(prData);
assert.deepEqual([prF33.winsBefore, prF33.winsAfter, prF33.geoBefore, prF33.geoAfter], [22, 23, 0.725, 0.75]);
assert.deepEqual([Math.round(prF33.rustBefore), Math.round(prF33.rustAfter), Math.round(prF33.zigBefore), Math.round(prF33.zigAfter)], [8202, 7607, 9930, 8968]);
const prAdv = pr.advPinSummary(prData);
assert.deepEqual([prAdv.wins, Math.round(prAdv.ghostty), Math.round(prAdv.rustty), prAdv.deltaPct.toFixed(1), prAdv.thinnest], [14, 3281, 2364, "-27.9", "terminal-stream"]);
const prIns = pr.insightNumbers(prData);
assert.equal(prIns.parser.toFixed(1), "114.7");
assert.equal(prIns.parserShareOfZig.toFixed(2), "3.96");
assert.equal(Math.round(prIns.bundle), 302);
assert.deepEqual([prIns.best.commit, prIns.best.delta.toFixed(1), prIns.next.commit, prIns.next.delta.toFixed(1)], ["e1f4829", "-89.2", "ab20827", "-36.5"]);
assert.deepEqual([prIns.noise.bench, prIns.noise.spread.toFixed(3)], ["screen-clone.render", "1.077"]);
assert.deepEqual([prIns.thinnest.bench, prIns.thinnest.pct.toFixed(1)], ["codepoint-width.simd", "-6.9"]);
assert.deepEqual([prIns.ltoChainEnd, prIns.ltoShipped], [531.05, 322]);
assert.equal(prIns.profileOnly.toFixed(1), "-1.2");
assert.equal(prRows.julyRows(prData).length, 14);
assert.equal(prRows.full33Rows(prData).length, 33);
const prPin = prRows.pinRows(prData);
assert.equal(prPin.length, 14, "current pin: one measured row per benchmark");
assert.ok(prPin.every((r) => !r.pending && r.b > 0 && r.b < 1), "current pin: 14 wins, no pending row");
assert.ok(prPin.every((r) => r.blo === undefined && r.bhi === undefined), "medians only: no range drawn");
assert.equal(prRows.pinHasRanges(prData), false);
assert.equal(prPin.at(-1).name, "terminal-stream", "closest benchmark last");
assert.equal(prPin.at(-1).b.toFixed(3), "0.979");
assert.equal(prPin[0].name, "terminal-parser");
assert.deepEqual(prPin.at(-1).tip.lines, ["Rustty 45.64 ms", "Ghostty 46.64 ms"]);
const prRanged = { ...prData, pin: { ...prData.pin, rows: [{ bench: "osc-parser", ghostty: [10, 9, 11], rustty: [5, 4.5, 5.5] }] } };
assert.deepEqual([prRows.pinRows(prRanged)[0].blo, prRows.pinRows(prRanged)[0].bhi], [4.5 / 11, 5.5 / 9]);
assert.equal(prRows.memoryRows(prData).length, 13);
assert.equal(prRows.apcRows(prData)[0].b, 1.81);
// defect-map: treemap layout, mark placement, filters, the class x method matrix and the side panels.
const dmData = JSON.parse(readFileSync(new URL("../src/app/porting/data/defect-map.json", import.meta.url), "utf8"));
assert.ok(readFileSync(new URL("../src/app/porting/data/defect-map-details.json", import.meta.url)).length < 200 * 1024, "defect-map-details.json under 200 KB");
const dmSq = dml.squarify([{ n: "a", v: 6 }, { n: "b", v: 3 }, { n: "c", v: 1 }], 0, 0, 100, 50);
assert.ok(Math.abs(dmSq.reduce((s, r) => s + r.w * r.h, 0) - 5000) < 1e-6, "squarify keeps the total area");
assert.ok(dmSq.every((r) => r.x >= -1e-9 && r.y >= -1e-9 && r.x + r.w <= 100 + 1e-9 && r.y + r.h <= 50 + 1e-9));
assert.ok(Math.abs(dmSq.find((r) => r.n === "a").w * dmSq.find((r) => r.n === "a").h - 3000) < 1e-6, "area ∝ value");
const dmMap = dml.layoutMap(dmData, dm.CLASS_KEYS);
assert.equal(dmMap.tops.length, 46);
assert.equal(dmMap.tops.reduce((s, t) => s + t.kids.length, 0), 276);
const dmIn = (r, b, e = 1e-6) => r.x >= b.x - e && r.y >= b.y - e && r.x + r.w <= b.x + b.w + e && r.y + r.h <= b.y + b.h + e;
assert.ok(dmMap.tops.every((t) => dmIn(t, { x: 0, y: 0, w: dml.GEO.W, h: dml.GEO.mapH })), "top tiles inside the map");
assert.ok(dmMap.tops.every((t) => t.kids.every((k) => dmIn(k, t))), "leaves inside their top tile");
for (let i = 0; i < dmMap.tops.length; i++) {
  for (let j = i + 1; j < dmMap.tops.length; j++) {
    const a = dmMap.tops[i];
    const c = dmMap.tops[j];
    const ox = Math.min(a.x + a.w, c.x + c.w) - Math.max(a.x, c.x);
    const oy = Math.min(a.y + a.h, c.y + c.h) - Math.max(a.y, c.y);
    assert.ok(ox <= 1e-6 || oy <= 1e-6, `top tiles ${a.n} and ${c.n} overlap`);
  }
}
const dmLeafTotal = dmMap.tops.reduce((s, t) => s + t.v0, 0);
assert.equal(dmLeafTotal, 367013);
const dmTerm = dmMap.tops.find((t) => t.n === "src/terminal");
const dmTmux = dmTerm.kids.find((k) => k.n === "src/terminal/tmux");
const dmScale = (dml.GEO.W * dml.GEO.mapH) / dmMap.tops.reduce((s, t) => s + t.v, 0);
assert.ok(dmTmux.w * dmTmux.h >= dmLeafTotal * dml.MIN_SHARE * (10 / 3) * dmScale * 0.98, "defect leaves get the minimum area");
assert.equal(dmMap.marks.length, 206, "one mark per defect");
assert.equal(new Set(dmMap.marks.map((m) => m.i)).size, 206);
const dmBox = new Map([...dmMap.tops.flatMap((t) => t.kids), ...dmMap.deps].map((k) => [k.n, k]));
for (const m of dmMap.marks) {
  const box = dmBox.get(dmData.defs[m.i].leaf);
  assert.ok(box && dmIn({ x: m.x - 0.5, y: m.y - 0.5, w: 1, h: 1 }, box), `mark ${m.i} inside its tile`);
}
assert.deepEqual(dmMap.deps.map((d) => [d.label, d.count]), [["libvaxis", 17], ["libxev", 8], ["Ghostty, non-Zig build files", 4], ["Zig std", 3], ["Highway", 2], ["GStreamer", 1], ["GTK", 1], ["zf", 1]]);
assert.ok(dmMap.deps.every((d) => d.y >= dml.GEO.stripY && d.x + d.w <= dml.GEO.W + 1e-6));
assert.equal(dml.shapePath("circle", 10, 10, 4).startsWith("M6,10"), true);
assert.ok(dm.METHODS.every((m) => dml.shapePath(m.shape, 0, 0, 4).endsWith("Z")));
// Filters: the first chip click isolates, later clicks add or remove, emptying restores all.
const dmAll = dm.CLASSES.map((c) => c.key);
let dmSet = dm.toggleChip(new Set(dmAll), "crash", dmAll);
assert.deepEqual([...dmSet], ["crash"]);
dmSet = dm.toggleChip(dmSet, "ub", dmAll);
assert.deepEqual([...dmSet].sort(), ["crash", "ub"]);
dmSet = dm.toggleChip(dm.toggleChip(dmSet, "ub", dmAll), "crash", dmAll);
assert.equal(dmSet.size, dmAll.length, "removing the last restores all");
const dmF = dm.ALL_FILTER;
const dmCount = (f) => dmData.defs.filter((d) => dm.visible(d, f)).length;
assert.equal(dmCount(dmF), 206);
assert.equal(dmCount({ ...dmF, cls: new Set(["crash"]) }), 60);
assert.equal(dmCount({ ...dmF, cell: ["arith", "crash"] }), 29);
assert.equal(dmCount({ ...dmF, status: "fixed" }), 4);
assert.equal(dmCount({ ...dmF, rf: "ub" }), 37);
assert.equal(dmCount({ ...dmF, met: new Set(["compile"]), cls: new Set(["crash"]) }), 0);
assert.equal(dm.isFiltered(dmF), false);
assert.equal(dm.isFiltered({ ...dmF, rf: "term" }), true);
// Matrix, headline and side panels.
const dmMx = dm.matrix(dmData.defs);
assert.deepEqual([dmMx.get("arith", "crash"), dmMx.get("arith", "ub"), dmMx.rowTotal("arith")], [29, 1, 30]);
assert.deepEqual([dmMx.rowTotal("compile"), dmMx.get("compile", "crash"), dmMx.get("compile", "latent")], [17, 0, 14]);
assert.deepEqual([dmMx.rowTotal("read"), dmMx.get("read", "ub"), dmMx.colTotal("ub")], [93, 11, 19]);
assert.equal(dmMx.total, 206);
const dmHead = dm.headline(dmData);
assert.deepEqual([dmHead.total, dmHead.live, dmHead.latent, dmHead.test, dmHead.ghostty, dmHead.deps], [206, 136, 59, 11, 173, 33]);
assert.deepEqual([dmHead.conf.h, dmHead.conf.m, dmHead.conf.k], [74, 23, 109]);
const dmWaffle = dm.waffle(dmData.defs);
assert.equal(dmWaffle.length, 60);
assert.deepEqual(["ub", "wrong", "term"].map((k) => dmWaffle.filter((i) => dmData.defs[i].rf === k).length), [37, 13, 10]);
assert.equal(dmData.defs.find((d) => d.n === 131).rf, "term", "UD131 still aborts in ReleaseFast");
assert.ok(dmWaffle.every((i, j) => j === 0 || dm.RF_ORDER.indexOf(dmData.defs[dmWaffle[j - 1]].rf) <= dm.RF_ORDER.indexOf(dmData.defs[i].rf)), "waffle ordered ub, wrong, term");
assert.deepEqual(dm.statusRows(dmData.defs).map((r) => [r.key, r.n]), [["fixed", 4], ["partly", 5], ["obsolete", 1], ["priv", 2], ["nr", 194]]);
const dmDens = dm.densityRows(dmData);
assert.deepEqual(dmDens.slice(0, 3).map((r) => [r.n, r.d.toFixed(1)]), [["pkg/fontconfig", "25.5"], ["pkg/opengl", "24.1"], ["src/termio", "22.3"]]);
assert.equal(dmDens.find((r) => r.n === "src/termio").c, 16);
assert.ok(dmDens.every((r) => r.c >= 3) && dmDens.every((r, j) => j === 0 || r.d <= dmDens[j - 1].d));
const dmTerminal = dm.terminalShare(dmData);
assert.deepEqual([dmTerminal.lines, dmTerminal.total, dmTerminal.defects, dmTerminal.ghostty], [182418, 367013, 36, 173]);
assert.deepEqual([dmTerminal.tmux, dmTerminal.tmuxLines, dmTerminal.tmuxDensity.toFixed(1)], [10, 4406, "22.7"]);
assert.equal(dm.leafStats(dmData, "src/termio/Exec.zig").n, 8);

// cost: quota-week totals, the modeled lost window, paid vs list price, token anatomy, delegation, eras, context, concurrency and the Codex meter.
const cost = { ...costModel, ...costStats };
const costData = JSON.parse(readFileSync(new URL("../src/app/porting/data/cost.json", import.meta.url), "utf8"));
const costBars = cost.weekBars(costData, "rustty", "usd");
assert.equal(costBars.length, 17, "17 quota weeks");
const costSum = cost.weekTotals(costData, "rustty", "usd");
assert.equal(costSum.rec.toFixed(2), "11032.43", "recovered Rustty usage");
assert.deepEqual(costSum.ext.map(Math.round), [8725, 12759, 20331], "modeled lost window");
assert.equal(cost.weekTotals(costData, "all", "usd").ext.map(Math.round)[1] > 20000, true, "all projects models every lost week maxed");
assert.deepEqual(cost.ceiling(costData, "rustty", "usd"), [987.01, 1384.725, 2131.215]);
assert.deepEqual(cost.ceiling(costData, "all", "tok"), [4.648e9, 4.671e9, 11.591e9]);
assert.ok(costBars.every((b) => b.top <= cost.yTop(costData, "rustty", "usd")), "bars and whiskers fit the y scale");
assert.equal(cost.niceCeil(2131 * 1.04), 2500);
const costParts = cost.recoveredParts(costData);
assert.deepEqual([costParts.mac, costParts.codex, costParts.vm].map(Math.round), [7957, 2351, 724]);
assert.equal(cost.fmtMoney(11032.43), "$11.0k");
assert.equal(cost.fmtMoney(2769.45), "$2,769");
assert.equal(cost.fmtMoney(46.15), "$46");
assert.equal(cost.fmtTok(4.671e9), "4.67B");
// paid vs list price and the leverage with its consistent bounds
const costLev = cost.leverage(costData);
assert.equal(Math.round(costData.paid.central), 422);
assert.equal(costData.paid.high, 600);
assert.equal(costLev.central.toFixed(1), "56.3");
assert.deepEqual([Math.round(costLev.low), Math.round(costLev.high), Math.round(costLev.lowAtTop)], [40, 74, 33]);
assert.deepEqual(cost.paidRows(costData).map((r) => Math.round(r.v)), [361, 61, 422, 11032, 12759, 23792]);
assert.ok(cost.paidRows(costData).every((r) => !/Bun/.test(r.name + r.sub + r.how)), "no other-project reference row");
const costWeek = cost.maxedWeek(costData);
assert.deepEqual([costWeek.perWeek.toFixed(2), Math.round(costWeek.central), costWeek.range.map((v) => v.toFixed(1))], ["46.15", 60, ["42.8", "92.4"]]);
// token anatomy and delegation
const costMix = cost.tokenMix(costData, "rustty");
const costCache = costMix.find((r) => r.key === "cache_read");
assert.deepEqual([costCache.shareTok.toFixed(3), costCache.shareUsd.toFixed(2), costCache.perM.toFixed(2)], ["0.965", "0.55", "0.31"]);
const costOut = costMix.find((r) => r.key === "output");
assert.deepEqual([(costOut.shareTok * 100).toFixed(1), (costOut.shareUsd * 100).toFixed(1), costOut.perM.toFixed(1)], ["0.3", "12.7", "22.9"]);
assert.equal(cost.delegation(costData, "rustty").delegated.toFixed(3), "0.756");
const costKinds = cost.kindTable(costData, "rustty");
const costSub = costKinds.find((k) => k.key === "claude/subagent");
const costMain = costKinds.find((k) => k.key === "claude/main");
assert.deepEqual([costSub.perCall.toFixed(3), costMain.perCall.toFixed(3), costSub.perM.toFixed(2), costMain.perM.toFixed(2)], ["0.095", "0.157", "0.43", "0.60"]);
assert.deepEqual(cost.topModels(costData).slice(0, 3).map((m) => [m.name, Math.round(m.usd), Math.round(m.share * 100)]), [["Opus 5.5", 4567, 41], ["Codex gpt-5.5", 2014, 18], ["Opus 5", 1788, 16]]);
// eras: the orchestrator era is measured; the earlier two are mostly modeled
const costEras = cost.eraStats(costData);
assert.deepEqual(costEras.map((e) => e.mostlyModeled), [true, true, false]);
assert.deepEqual(costEras.map((e) => e.perCommit[1].toFixed(1)), ["8.4", "12.0", "6.6"]);
assert.deepEqual(costEras[2].perCommit.map((v) => v.toFixed(1)), ["6.4", "6.6", "7.1"]);
assert.equal(costEras[2].commitsPerPrompt.toFixed(1), "5.8");
assert.deepEqual(cost.unitRange(costData).map((v) => v.toFixed(1)), ["4.5", "7.6"]);
// context, concurrency and the Codex meter
const costCtx = cost.contextGrowth(costData);
assert.deepEqual([costCtx.from.w, costCtx.from.med, costCtx.to.w, costCtx.to.med, costCtx.max], ["2026-07-13", 76903, "2026-09-21", 203988, 999021]);
const costPeak = cost.concurrencyPeak(costData);
assert.deepEqual([costPeak.d, costPeak.ratio.toFixed(1), costPeak.threads], ["2026-09-08", "7.4", 452]);
const costCodex = cost.codexPerPoint(costData);
assert.deepEqual([costCodex.usd.toFixed(2), costCodex.points, Math.round(costCodex.perPoint)], ["2113.40", 98, 22]);
// the limit lane: every weekly and session limit gets a label row (as in the mockup, the 09-08 credits marker may stay unlabelled); labels in one row never overlap
const costX = (t) => 58 + ((Date.parse(`${t}Z`) - Date.UTC(2026, 5, 16, 22)) / (17 * 7 * 864e5)) * 886;
const costPlaced = cost.placeEventLabels(costData.ev, costX, 960);
costData.ev.forEach((e, i) => {
  if (["weekly", "session"].includes(e.k)) assert.ok(costPlaced.has(i), e.t);
  if (e.k === "slash") assert.ok(!costPlaced.has(i), "slash commands are unlabelled");
});
for (const [i, a] of costPlaced) {
  for (const [j, b] of costPlaced) {
    if (i < j && a.row === b.row) assert.ok(a.x1 + 4 <= b.x0 || b.x1 + 4 <= a.x0, `labels ${i} and ${j} overlap`);
  }
}
assert.equal(cost.coverageCounts(costData).rec, 17);
// cost framing: list-price usage is never called spend; only "not money spent" may say spent (model text and widget sources)
/** Returns the text with the allowed phrase removed, for the spend-wording check. */
const costUnframed = (t) => t.replaceAll("not money spent", "");
for (const r of cost.paidRows(costData)) assert.doesNotMatch(costUnframed(`${r.name} ${r.sub} ${r.how}`), /\bspen[dt]/i, r.name);
for (const f of readdirSync(new URL("../src/components/porting/cost/", import.meta.url))) {
  if (!/\.tsx?$/.test(f)) continue;
  const src = readFileSync(new URL(`../src/components/porting/cost/${f}`, import.meta.url), "utf8").replace(/\/\*\*[\s\S]*?\*\/|\/\/.*$/gm, "");
  assert.doesNotMatch(costUnframed(src), /\bspen[dt]/i, `spend wording in cost/${f}`);
}
// the shared placer also places all seven Codex audit labels in four rows
const costCodexX = (t) => 34 + ((Date.parse(`${t}Z`) - Date.parse("2026-07-31T14:00Z")) / (50 * 36e5)) * 386;
assert.equal(cost.placeLabels(costData.cev.map((e, i) => ({ i, x: costCodexX(e.t), label: e.label })), { rows: 4, charW: 5.6, pad: 0, width: 462, gap: 3 }).size, 7);

// ---- static charts: step series monotonic, funnel non-increasing, stream layers contiguous, tiles resolve to facts
/** Loads a generated porting data file by name. */
const scData = (name) => JSON.parse(readFileSync(new URL(`../src/app/porting/data/${name}.json`, import.meta.url), "utf8"));
const scFacts = scData("facts");
assert.ok(sc.isNonDecreasing([1, 1, 2]) && !sc.isNonDecreasing([2, 1]));
assert.ok(sc.isNonIncreasing([3, 3, 1]) && !sc.isNonIncreasing([1, 2]));
assert.equal(sc.stepPath([[0, 10], [5, 4], [9, 2]], 12), "M0,10H5V4H9V2H12");
assert.equal(sc.dayOffset("2026-06-22", "2026-06-21"), 1);
assert.equal(sc.dayOffset("2026-06-21T12:00", "2026-06-21"), 0.5);
// strategy timeline: ledger by day ends at 314 and never shrinks; label slots never collide in a lane row
const scSt = scData("strategy-timeline");
const scLedger = sc.lastPerDay(scSt.ledger, "rows");
assert.equal(scLedger.at(-1).rows, 314);
assert.equal(scLedger[0].rows, 216);
assert.ok(sc.isNonDecreasing(scLedger.map((p) => p.rows)));
assert.ok(sc.isNonDecreasing(scSt.registry.map((p) => p.n)));
const scPlaced = sc.placeLabels([{ x: 0, w: 50 }, { x: 20, w: 50 }, { x: 40, w: 50 }, { x: 200, w: 10 }], { rows: 2, gap: 4 });
assert.deepEqual(scPlaced, [0, 1, -1, 0]);
// checks growth: registry and self-test step lines are monotonic
const scCg = scData("checks-growth");
assert.ok(sc.isNonDecreasing(scCg.registry.map((p) => p.n)));
assert.ok(sc.isNonDecreasing(scCg.registry.map((p) => p.self)));
assert.ok(sc.isNonDecreasing(scCg.scripts.map((p) => p.selfTest)));
assert.equal(sc.waffle(scCg.composition, 17).length, 105);
// audit funnel: stages non-increasing; widths are shares of the first stage
const scFunnel = sc.funnelView(scData("audit-funnel").stages);
assert.ok(sc.isNonIncreasing(scFunnel.map((s) => s.n)));
assert.equal(scFunnel[0].frac, 1);
assert.ok(Math.abs(scFunnel.at(-1).frac - 140 / 659) < 1e-9);
assert.ok(Math.abs(scFunnel.at(-1).ofPrev - 140 / 144) < 1e-9);
// orchestration: stream layers stack contiguously around a centred baseline
const scOr = scData("orchestration");
const scLayers = sc.streamLayers(scOr.weeks.map((w) => w.n));
assert.equal(scOr.models.length, scOr.weeks[0].n.length, "one layer per model, Haiku 5.5 included");
for (let w = 0; w < scOr.weeks.length; w++) {
  const total = scOr.weeks[w].n.reduce((a, b) => a + b, 0);
  assert.equal(scLayers[0][w][0], -total / 2);
  assert.equal(scLayers.at(-1)[w][1], total / 2);
  for (let m = 1; m < scLayers.length; m++) assert.equal(scLayers[m][w][0], scLayers[m - 1][w][1]);
}
const scCtx = sc.ctxSplit(scOr.ctx, 100);
assert.equal(scCtx.claude[0].median, 76903);
assert.ok(scCtx.claude.some((c) => c.lowN) && scCtx.codex.length === 2);
// unsafe density: domain covers every census point
const scUd = scData("unsafe-density");
const [scLo, scHi] = sc.paddedExtent(scUd.series.map((p) => p.density), 0.5);
assert.ok(scLo <= 26.54 && scHi >= 28.15 && scLo < scHi);
// stats card: every tile resolves to fact displays; an unknown id throws
const scTiles = sc.resolveTiles(scData("stats-card").tiles, scFacts);
assert.equal(scTiles.find((t) => t.key === "days").value, "112 days");
assert.equal(scTiles.find((t) => t.key === "verify").extra.display, "58,767");
assert.equal(scTiles.find((t) => t.key === "perf").value, "14/14");
assert.equal(scTiles.find((t) => t.key === "cost").extra.label, "API-equivalent (reference)");
assert.throws(() => sc.resolveTiles([{ key: "x", value: "nope", label: "x" }], scFacts));
// tile notes: {fact_id} placeholders resolve to facts between plain text; an unknown id throws
const scDays = scTiles.find((t) => t.key === "days").note;
assert.equal(scDays[0], "Agents committed on ");
assert.equal(scDays[1].id, "commit_days");
assert.equal(scDays[3].display, "41–51");
assert.deepEqual(sc.noteParts("plain", () => assert.fail()), ["plain"]);
assert.throws(() => sc.resolveTiles([{ key: "x", value: "days_total", label: "x", note: "{nope}" }], scFacts));
assert.equal(sc.statusMark("partial").text, "partly");
assert.equal(sc.curveThrough([[0, 0], [10, 4]]), "M0,0C5,0 5,4 10,4");
assert.ok(sc.curveThrough([[1, 2]], false).startsWith("L1,2"));

// one Codex meter treatment page-wide: the audit funnel and the cost widget share series, events, run names and note
const scAf = scData("audit-funnel");
assert.deepEqual(scAf.meter, costData.meter, "same hourly meter series");
assert.deepEqual(scAf.events.map((e) => [e.t, e.label]), costData.cev.map((e) => [e.t, e.label]), "same event times and labels");
assert.deepEqual(scAf.meterNote, costData.meterNote, "same note on the unexplained readings");
assert.ok(scAf.events.some((e) => e.t === "2026-08-02T03:36"), "100th round at 03:36 UTC (fact-check)");
for (const [dir, files] of [["static-charts", null], ["cost", ["Codex.tsx"]]]) {
  for (const f of files ?? readdirSync(new URL(`../src/components/porting/${dir}/`, import.meta.url))) {
    if (!/\.tsx?$/.test(f)) continue;
    const src = readFileSync(new URL(`../src/components/porting/${dir}/${f}`, import.meta.url), "utf8");
    assert.doesNotMatch(src, /\b(first|second) run\b|\baudit [12]\b/i, `run names in ${dir}/${f}`);
  }
}
// static-chart sources and their registry entries: no Bun (outside the data's one trigger) and no internal ids
const scIdRe = /\b(UD\d+|N\d{1,3}|R\d{1,2}[a-z]?|G\d+[a-z]?|G-[A-Z]+|P\d+-W\d+|T\d+[a-z]|A\d|D\d{1,2}r?)\b/;
for (const f of readdirSync(new URL("../src/components/porting/static-charts/", import.meta.url))) {
  if (!/\.tsx?$/.test(f)) continue;
  const src = readFileSync(new URL(`../src/components/porting/static-charts/${f}`, import.meta.url), "utf8").replace(/\/\*\*[\s\S]*?\*\/|\/\/.*$|\/\*[\s\S]*?\*\//gm, "");
  assert.doesNotMatch(src, /\bBun\b/, `Bun in static-charts/${f}`);
  assert.doesNotMatch(src, scIdRe, `internal id in static-charts/${f}`);
}
const { WIDGETS: scWidgets } = await import("../src/components/porting/lib/registry.ts");
for (const slug of ["stats-card", "strategy-timeline", "checks-growth", "audit-funnel", "orchestration", "unsafe-density", "platform-table"]) {
  const w = scWidgets.find((x) => x.slug === slug);
  const text = [w.title, w.label, w.source, w.caption, w.deck ?? "", w.summary].join(" ");
  assert.doesNotMatch(text, /\bBun\b/, `Bun in registry ${slug}`);
  assert.doesNotMatch(text, scIdRe, `internal id in registry ${slug}`);
}

// Roving focus: arrows step and clamp, Home/End jump, cols moves by a row, other keys pass through.
assert.equal(rovingTarget("ArrowRight", 0, 5), 1);
assert.equal(rovingTarget("ArrowRight", 4, 5), 4);
assert.equal(rovingTarget("ArrowLeft", 0, 5), 0);
assert.equal(rovingTarget("ArrowDown", 2, 5), 3);
assert.equal(rovingTarget("Home", 3, 5), 0);
assert.equal(rovingTarget("End", 1, 5), 4);
assert.equal(rovingTarget("ArrowDown", 2, 30, 15), 17);
assert.equal(rovingTarget("ArrowUp", 2, 30, 15), 0);
assert.equal(rovingTarget("ArrowDown", 20, 30, 15), 29);
assert.equal(rovingTarget("Tab", 1, 5), null);
assert.equal(rovingTarget("Enter", 1, 5), null);

/** referencePinRe finds "pin" meaning the Ghostty reference commit (mirrors REFERENCE_PIN_RE in test_build_data.py). */
const referencePinRe = /\b(?:the|at|current|new|each|Ghostty's)(?: (?:first|last|next|previous|current|new|old))? pin\b|\bpin (?:moves?|moved|advances?)\b|\b[Cc]urrent[- ]pin\b|\bPin \+\d|\b[Uu]pstream pin\b/;
// Registry text uses plain words for upstream moves, not the internal "advance"/"resync"/"oracle"/"finale".
const { WIDGETS: allWidgets } = await import("../src/components/porting/lib/registry.ts");
for (const w of allWidgets) {
  const text = [w.title, w.label, w.source, w.caption, w.deck ?? "", w.summary].join(" ");
  assert.doesNotMatch(text, /\bresync|\badvances?\b|\boracle\b|\bfinale\b/i, `internal process word in registry ${w.slug}`);
  assert.doesNotMatch(text, referencePinRe, `"pin" for the reference commit in registry ${w.slug}`);
}
// Component sources (comments stripped) never say "pin" for the reference commit either;
// "pin" as the tooltip verb ("Enter to pin") and Ghostty's Pin type do not match.
for (const dir of readdirSync(new URL("../src/components/porting/", import.meta.url))) {
  if (dir === "lib") continue;
  const base = new URL(`../src/components/porting/${dir}/`, import.meta.url);
  let files = [];
  try { files = readdirSync(base); } catch { continue; }
  for (const f of files) {
    if (!/\.tsx?$/.test(f)) continue;
    const src = readFileSync(new URL(f, base), "utf8").replace(/\/\*[\s\S]*?\*\/|\/\/.*$/gm, "");
    assert.doesNotMatch(src, referencePinRe, `"pin" for the reference commit in ${dir}/${f}`);
  }
}

// Permalinks: commit tokens resolve by key or sha prefix to the right repository, at the full sha.
const linkMap = {
  "58aa62a81": "r:58aa62a81e7db5f8762b0158acc661dafc31f230",
  "246f70287": "g:246f702876b924a1cb7cade1e99274d1470302fc",
  "7cde540171": "google/highway:7cde540171",
};
assert.equal(commitUrl(resolveCommit(linkMap, "58aa62a81")), "https://github.com/AirelyDev/rustty/commit/58aa62a81e7db5f8762b0158acc661dafc31f230");
assert.equal(commitUrl(resolveCommit(linkMap, "246f7028")), "https://github.com/ghostty-org/ghostty/commit/246f702876b924a1cb7cade1e99274d1470302fc");
assert.equal(commitUrl(resolveCommit(linkMap, "7cde540171")), "https://github.com/google/highway/commit/7cde540171");
assert.equal(resolveCommit(linkMap, "246f70287", "rustty"), null);
assert.equal(resolveCommit(linkMap, "97e244c"), null);
assert.equal(resolveCommit(linkMap, "a".repeat(40), "ghostty").repo, "ghostty-org/ghostty");
assert.equal(
  sourceUrl(resolveCommit(linkMap, "246f70287"), "src/terminal/Screen.zig", "2747-2760"),
  "https://github.com/ghostty-org/ghostty/blob/246f702876b924a1cb7cade1e99274d1470302fc/src/terminal/Screen.zig#L2747-L2760",
);
assert.equal(sourceUrl(resolveCommit(linkMap, "58aa62a81"), "Cargo.toml", "12"), "https://github.com/AirelyDev/rustty/blob/58aa62a81e7db5f8762b0158acc661dafc31f230/Cargo.toml#L12");
assert.equal(sourceUrl(resolveCommit(linkMap, "246f70287"), "src/", "3", true), "https://github.com/ghostty-org/ghostty/tree/246f702876b924a1cb7cade1e99274d1470302fc/src");
assert.deepEqual(
  splitCommits(linkMap, "fixed in 58aa62a81, not 97e244c or #d97934").map((p) => [p.kind, p.text]),
  [["text", "fixed in "], ["commit", "58aa62a81"], ["text", ", not 97e244c or #d97934"]],
);

// Agents vs human: the caption's count of hollow rings is the data's.
{
  const { WIDGETS: avhWidgets } = await import("../src/components/porting/lib/registry.ts");
  const avhReg = avhWidgets.find((w) => w.slug === "agents-vs-human");
  const avhData = JSON.parse(readFileSync("src/app/porting/data/agents-vs-human.json", "utf8"));
  const ringText = `hollow rings the ${avhData.originPrompts.length} prompts of the founding sessions typed before the repository existed`;
  assert.ok(avhReg.caption.includes(ringText), `agents-vs-human caption lacks "${ringText}"`);
}

// Every prompt: view-model unit tests, then the whole-period tiles agree with the builder's totals.
{
  const ep = await import("../src/components/porting/every-prompt/model.ts");
  const { runModelTests } = await import("../src/components/porting/every-prompt/model.test.ts");
  runModelTests(ep);
  const epData = JSON.parse(readFileSync("src/app/porting/data/every-prompt.json", "utf8"));
  const epTicks = ep.decodeTicks(epData);
  const epAll = ep.tiles(epTicks);
  assert.equal(epAll.prompts, epData.totals.prompts);
  assert.equal(epAll.medianChars, epData.totals.medianChars);
  assert.equal(epAll.decisions, epData.totals.decisions);
  assert.equal(epTicks.filter((t) => t.flags & ep.FLAG.settingsUnknown).length, epData.totals.settingsUnknown);
  assert.equal(ep.dayCount(epData), ep.dayIndexOf(epData.window[0], epData.window[1]) + 1);
  const epBars = ep.weekBars(epData.weeks, epData.models, "settings");
  assert.equal(epBars.reduce((a, b) => a + b.total, 0), epData.totals.prompts);
  assert.equal(ep.weekBars(epData.weeks, epData.models, "modes").reduce((a, b) => a + b.total, 0), epData.totals.prompts);

  // The registry's caption and summary are typed text: each number must be the builder's (in its
  // sentence), and no other number may appear (ISO dates aside).
  {
    const { WIDGETS: regWidgets } = await import("../src/components/porting/lib/registry.ts");
    const reg = regWidgets.find((w) => w.slug === "every-prompt");
    const t = epData.totals;
    const logRows = JSON.parse(readFileSync("public/porting/every-prompt-log.json", "utf8")).rows;
    const linked = logRows.filter((r) => r.commits.length > 0).length;
    const websiteCodex = logRows.filter((r) => r.website && r.tool === "codex").length;
    const days = ep.dayCount(epData);
    const want = {
      caption: [
        `except ${t.redactions} redactions`,
        `for ${t.pastedLost} prompts it no longer exists`,
        `The ${t.articleWork} prompts of the October sessions about this article are included`,
        `the ${t.website} prompts typed in the website folders (${websiteCodex} of them in the Codex app)`,
        "(typed prompt, slash command or shell escape)",
        `the ${t.sessionsCommandOnly} sessions that hold only Claude Code commands or shell escapes`,
        `the ${t.commandsAndShell} Claude Code commands and shell escapes are one click away`,
        `diamonds mark the ${t.foundingSessions} founding sessions`,
        `The ${t.foundingPrompts} founding prompts`,
      ],
      summary: [
        `${fmtInt(t.prompts)} human prompts to the port and its website (${t.articleWork} of them about this article)`,
        `over ${days} days`,
        `${fmtInt(t.bySource["claude-mac"])} in Claude Code on the Mac`,
        `${fmtInt(t.bySource["claude-vm"])} in Claude Code on the Linux VM`,
        `${fmtInt(t.bySource.codex)} in the Codex app`,
        `They came in ${fmtInt(t.sessions)} sessions, ${t.sessionsCommandOnly} of them only Claude Code commands or shell escapes`,
        `on the busiest days, ${[t.busiestParallelDay.day, ...t.busiestParallelDay.tied].join(" and ")}, ${t.busiestParallelDay.open} sessions each had an instruction or decision`,
        `the longest session held ${t.longestSessionPrompts.prompts} prompts over ${t.longestSessionPrompts.spannedDays} days`,
        `${t.medianChars} characters long (all prompts)`,
        `${t.decisions} prompts only answer`,
        `unknown for ${t.settingsUnknown} prompts`,
        `for ${t.settingsInferred} more the model is inferred`,
        `${t.redactions} redactions in ${t.redactedPrompts} prompts`,
        `${linked} prompts are linked`,
      ],
      deck: [`is ${t.medianChars} characters long`],
    };
    for (const [field, parts] of Object.entries(want)) {
      const text = reg[field];
      for (const part of parts) assert.ok(text.includes(part), `every-prompt ${field} lacks "${part}"`);
      // Parts first (a part may carry a date the builder wrote), then the remaining ISO dates.
      let rest = text;
      for (const part of parts) rest = rest.replace(part, "");
      rest = rest.replace(/\b\d{4}-\d{2}-\d{2}\b/g, "");
      assert.equal(rest.match(/\d/), null, `every-prompt ${field} has an unchecked number: ${rest.match(/.{0,30}\d.{0,30}/)?.[0]}`);
    }
  }

  // The log view-model imports "./model" without an extension (bundler style); resolve such
  // relative imports of the every-prompt folder to their .ts files for this run.
  // module.registerHooks (synchronous hooks) needs Node 22.15+ or 23.5+; package.json has no
  // "engines" field, so the requirement is recorded here.
  registerHooks({
    resolve(specifier, context, next) {
      if (/^\.\.?\/[^.]*$/.test(specifier) && context.parentURL?.includes("/every-prompt/")) {
        return next(`${specifier}.ts`, context);
      }
      return next(specifier, context);
    },
  });
  const lm = await import("../src/components/porting/every-prompt/log-model.ts");
  const { runLogModelTests } = await import("../src/components/porting/every-prompt/log-model.test.ts");
  runLogModelTests(lm, ep);
  // The real log lines up with the summary row by row, and log and timeline agree on every filter.
  const epLog = lm.decodeLog(JSON.parse(readFileSync("public/porting/every-prompt-log.json", "utf8")), epData.window[0], epData.lanes);
  assert.equal(epLog.rows.length, epTicks.length);
  for (const r of epLog.rows) {
    const t = epTicks[r.i];
    assert.deepEqual([r.day, r.minute, r.lane, r.chars, r.model, r.effort, r.phase], [t.day, t.minute, t.lane, t.chars, t.model, t.effort, t.phase], r.id);
    assert.equal(lm.rowFlags(r), t.flags, r.id);
  }
  const epDef = ep.defaultFilter(epTicks.length ? ep.dayCount(epData) : 0, epData.lanes.length);
  const epFilters = [
    epDef,
    { ...epDef, from: 30, to: 70 },
    { ...epDef, lanes: new Set([1]) },
    { ...epDef, decisionsOnly: true },
    { ...epDef, hideArticle: true },
    { ...epDef, model: "unknown" },
    { ...epDef, model: epData.models[0] },
    { ...epDef, effort: "high" },
    { ...epDef, phase: epData.phases[1].key },
    { ...epDef, query: "continue" },
    { ...epDef, query: "Miri", lanes: new Set([0, 2]) },
    { ...epDef, query: epLog.rows[500].id.toUpperCase() },
    { ...epDef, query: "no prompt says this \u0000" },
  ];
  for (const f of epFilters) {
    const want = ep.applyFilter(epTicks, f, f.query ? (i) => ep.matchesQuery(epLog.rows[i], f.query) : undefined).map((t) => t.i);
    assert.deepEqual(lm.filterRows(epLog.rows, f).map((r) => r.i), want, ep.encodeFilter(f));
  }
  assert.equal(lm.filterRows(epLog.rows, epFilters[12]).length, 0);
  assert.ok(lm.filterRows(epLog.rows, epFilters[9]).length > 0);

  // Sessions: view-model unit tests, then the whole-period session tiles agree with the builder's totals.
  const sm = await import("../src/components/porting/every-prompt/session-model.ts");
  const { runSessionModelTests } = await import("../src/components/porting/every-prompt/session-model.test.ts");
  runSessionModelTests(sm, ep);
  const epSessions = sm.decodeSessions(epData);
  const et = epData.totals;
  assert.equal(epSessions.length, et.sessions);
  for (const r of epLog.rows) assert.equal(r.session, epTicks[r.i].session, r.id);
  assert.ok(epTicks.every((t) => t.session !== ""), "every prompt has a session");
  assert.equal(epSessions.reduce((a, x) => a + x.prompts, 0), et.prompts);
  const st = sm.sessionTiles(epSessions, epTicks, epData.areas);
  assert.equal(st.commandOnly, et.sessionsCommandOnly);
  assert.equal(st.founding, et.foundingSessions);
  assert.deepEqual(Object.fromEntries(st.byArea.map((x) => [x.area, x.n])), et.sessionsByArea);
  assert.deepEqual(
    { day: ep.shortDate(ep.dayDate(epData.window[0], st.busiestDay.day)), open: st.busiestDay.open },
    { day: ep.shortDate(new Date(`${et.busiestParallelDay.day}T00:00:00Z`)), open: et.busiestParallelDay.open },
  );
  assert.deepEqual(
    st.busiestTies.map((d) => ep.shortDate(ep.dayDate(epData.window[0], d))),
    (et.busiestParallelDay.tied ?? []).map((d) => ep.shortDate(new Date(`${d}T00:00:00Z`))),
  );
  assert.deepEqual(
    { key: st.longestPrompts.key, prompts: st.longestPrompts.prompts, spannedDays: st.longestPrompts.spannedDays },
    et.longestSessionPrompts,
  );
  assert.equal(st.medianPrompts, et.medianPromptsPerSession);
  assert.equal(epTicks.filter((t) => epSessions[epData.ticks[t.i][ep.T_SESSION]].founding).length, et.foundingPrompts);
  // Session filters agree between log and timeline, and every session can be revealed from a narrow filter.
  const sessFilters = [
    { ...epDef, area: epData.areas[0] },
    { ...epDef, session: epSessions[40].key },
    { ...epDef, area: "command-only" },
    { ...epDef, categories: new Set(ep.CATEGORY_DEFAULT) },
    { ...epDef, categories: new Set(["shell", "setting"]), lanes: new Set([0]) },
  ];
  // Categories: the default view hides exactly the commands and shell escapes.
  {
    const def = ep.defaultFilter(ep.dayCount(epData), epData.lanes.length, ep.CATEGORY_DEFAULT);
    assert.equal(epTicks.length - ep.applyFilter(epTicks, def).length, et.commandsAndShell);
    for (const c of epData.categories) {
      assert.equal(ep.applyFilter(epTicks, { ...def, categories: new Set([c]) }).length, et.byCategory[c], c);
    }
    assert.equal(ep.tiles(ep.applyFilter(epTicks, def)).medianChars, et.medianCharsSubstantive);
    // Session cards over the real log: every prompt in exactly one card, cards in first-prompt order.
    const isCo = (k) => epSessions.find((x) => x.key === k)?.commandsOnly ?? false;
    const cardsAll = lm.groupBySession(epLog.rows, epLog.rows, isCo);
    assert.equal(cardsAll.length, et.sessions);
    assert.equal(cardsAll.reduce((a, c) => a + c.rows.length, 0), et.prompts);
    assert.equal(cardsAll.filter((c) => c.commandOnly).length, et.sessionsCommandOnly);
    const cardsDef = lm.groupBySession(epLog.rows, lm.filterRows(epLog.rows, def), isCo);
    assert.equal(cardsDef.some((c) => c.commandOnly), false, "command-only cards hidden by default");
    assert.equal(cardsDef.length, et.sessions - et.sessionsCommandOnly);
  }
  for (const f of sessFilters) {
    assert.deepEqual(lm.filterRows(epLog.rows, f).map((r) => r.i), ep.applyFilter(epTicks, f).map((t) => t.i), ep.encodeFilter(f));
  }
  // The weekly settings bars rebuilt from the ticks equal the builder's.
  assert.deepEqual(ep.weeksFromTicks(epTicks, epData), epData.weeks);
  // Session bars: the widget's geometry matches the builder's packing constants, and on every
  // packed row neighbours keep a visible gap of 3 units and their hit targets never overlap.
  {
    const py = readFileSync("scripts/porting/widgets/w12_every_prompt.py", "utf8");
    const tl = readFileSync("src/components/porting/every-prompt/Timeline.tsx", "utf8");
    const num = (src, name) => Number(new RegExp(`\\b${name} = (\\d+)`).exec(src)?.[1]);
    const plotW = num(tl, "W") - num(tl, "LBL") - num(tl, "PR");
    assert.equal(num(py, "PLOT_UNITS"), plotW);
    assert.equal(num(py, "BAR_MIN_UNITS"), sm.BAR_MIN_W);
    assert.equal(num(py, "HIT_UNITS"), sm.HIT_W);
    const rowsOf = new Map();
    for (const x of epSessions) {
      if (!rowsOf.has(x.row)) rowsOf.set(x.row, []);
      rowsOf.get(x.row).push(sm.sessionBar(x, ep.dayCount(epData), plotW));
    }
    for (const [row, bars] of rowsOf) {
      bars.sort((a, b) => a.x - b.x);
      for (let k = 1; k < bars.length; k++) {
        const a = bars[k - 1];
        const b = bars[k];
        assert.ok(b.x - (a.x + a.w) >= 3, `row ${row}: bars ${k - 1} and ${k} closer than 3 units`);
        assert.ok(b.hitX > a.hitX + a.hitW, `row ${row}: hit targets ${k - 1} and ${k} overlap`);
      }
    }
  }
  const narrow = { ...epDef, from: 100, to: 100, lanes: new Set([2]), decisionsOnly: true, area: epData.areas[9] };
  for (const x of epSessions) {
    const f = sm.revealSession(narrow, x, epTicks, epData.phases);
    const n = ep.applyFilter(epTicks, f).length;
    assert.ok(n > 0 && n <= x.prompts && f.session === x.key, x.key);
  }
}

console.log("porting helpers OK");
