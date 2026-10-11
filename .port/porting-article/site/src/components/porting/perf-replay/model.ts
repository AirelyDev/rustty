/**
 * Pure view-model for the "Perf replay" widget: per-sweep scoreboard, suite
 * and geomean, the replay tween, the commits behind each step, the terminal
 * title, the hero figures, chapter summaries and the computed insights.
 */
import type { PrData, Stage } from "./types";

/** TIE_BAND is the win/tie/loss band: within ±1.5% of Zig is a tie. */
export const TIE_BAND = 0.015;

/** Win, tie or loss of one Rust ÷ Zig ratio. */
export type Outcome = "win" | "tie" | "loss";

/** Rust ÷ Zig median ratio of benchmark b at stage st. */
export function ratio(st: Stage, b: string): number {
  return st.rust[b][0] / st.zig[b][0];
}

/** Returns the run-to-run ratio range [min Rust ÷ max Zig, max Rust ÷ min Zig] of b at st. */
export function spread(st: Stage, b: string): [number, number] {
  return [st.rust[b][1] / st.zig[b][2], st.rust[b][2] / st.zig[b][1]];
}

/** Classifies a ratio with the ±1.5% tie band. */
export function outcome(r: number): Outcome {
  if (Math.abs(r - 1) < TIE_BAND) return "tie";
  return r < 1 ? "win" : "loss";
}

/** Wins, ties and losses of the 14 benchmarks at stage i, recomputed from the medians. */
export function scoreboard(
  data: PrData,
  i: number,
): { W: number; T: number; L: number } {
  const n = { W: 0, T: 0, L: 0 };
  for (const b of data.bench) {
    const o = outcome(ratio(data.stages[i], b));
    if (o === "win") n.W++;
    else if (o === "tie") n.T++;
    else n.L++;
  }
  return n;
}

/** Suite (sum of the 14 medians) of Rust and Zig at stage i, and Rust's delta in percent. */
export function suite(
  data: PrData,
  i: number,
): { rust: number; zig: number; deltaPct: number } {
  const st = data.stages[i];
  let rust = 0;
  let zig = 0;
  for (const b of data.bench) {
    rust += st.rust[b][0];
    zig += st.zig[b][0];
  }
  return { rust, zig, deltaPct: (rust / zig - 1) * 100 };
}

/** Geometric mean of the 14 Rust ÷ Zig ratios at stage i, rounded to 3 decimals. */
export function geomean(data: PrData, i: number): number {
  const logs = data.bench.map((b) => Math.log(ratio(data.stages[i], b)));
  const g = Math.exp(logs.reduce((a, v) => a + v, 0) / logs.length);
  return Math.round(g * 1000) / 1000;
}

/** Benchmarks sorted by their final ratio, largest speed-up first (the replay's row order). */
export function finalOrder(data: PrData): string[] {
  const last = data.stages[data.stages.length - 1];
  return [...data.bench].sort((a, b) => ratio(last, a) - ratio(last, b));
}

/** Interpolates between ratios a and z in log space (t = 0 → a, t = 1 → z). */
export function interp(a: number, z: number, t: number): number {
  return Math.exp(Math.log(a) + (Math.log(z) - Math.log(a)) * t);
}

/** Ease-in-out quadratic easing of t in [0, 1] (the mockup's tween curve). */
export function ease(t: number): number {
  return t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
}

/** Parses a naive "YYYY-MM-DDTHH:MM:SS" timestamp to epoch ms (as UTC; only differences matter). */
export function tm(s: string): number {
  const p = s.split(/[-T:]/).map(Number);
  return Date.UTC(p[0], p[1] - 1, p[2], p[3] ?? 0, p[4] ?? 0, p[5] ?? 0);
}

/** "MM-DD HH:MM" of a timestamp. */
export function hhmm(s: string): string {
  return `${s.slice(5, 10)} ${s.slice(11, 16)}`;
}

/**
 * Commits between the previous step's code and this step's (by commit time).
 * The first step lists its own commit; a re-measure of the same code lists none.
 */
export function stageCommits(data: PrData, i: number) {
  const st = data.stages[i];
  if (i === 0) return data.commits.filter((c) => c.h === st.commit);
  const t1 = tm(st.t);
  const t0 = tm(data.stages[i - 1].t);
  if (t1 === t0) return [];
  return data.commits.filter((c) => tm(c.t) > t0 && tm(c.t) <= t1);
}

/** The terminal title: the real recorded command for benchmark b at stage i and its results file. */
export function cmdTitle(data: PrData, i: number, b: string): string {
  return `$ rustty-bench ${data.args[b] ?? ""}  # → ${data.stages[i].results}`;
}

/** Kind of a commit on the timeline, from its subject. */
export type CommitKind = "revert" | "merge" | "parity" | "records" | "perf";

/** Classifies a commit subject (revert, merge, parity fix, records, perf fix). */
export function commitKind(s: string): CommitKind {
  if (/^Revert/.test(s)) return "revert";
  if (/merge/i.test(s)) return "merge";
  if (/parity fix/.test(s)) return "parity";
  if (!/perf/.test(s) || /alloc-bench|official|attribution log/.test(s))
    return "records";
  return "perf";
}

/** A commit subject without its "rustty: perf:" style prefix, for the step panel list. */
export function listSubject(s: string): string {
  return s.replace(/^rustty: (perf|vt)(:| —) /, "").replace(/^rustty: /, "");
}

/** The hero row: 0/14 → 14/14, suites, geomeans, the day's perf commits and the two headline speed-ups. */
export function hero(data: PrData) {
  const last = data.stages.length - 1;
  const fin = data.stages[last];
  const end = data.commits[data.commits.length - 1].t;
  return {
    startW: scoreboard(data, 0).W,
    endW: scoreboard(data, last).W,
    n: data.bench.length,
    suiteStart: suite(data, 0).deltaPct,
    suiteEnd: suite(data, last).deltaPct,
    geoStart: geomean(data, 0),
    geoEnd: geomean(data, last),
    perfCommits: data.commits.filter(
      (c) => c.t.startsWith(end.slice(0, 10)) && /rustty: perf[: ]/.test(c.s),
    ).length,
    tParser: 1 / ratio(fin, "terminal-parser"),
    clone: 1 / ratio(fin, "screen-clone.clone"),
    from: hhmm(data.stages[0].t),
    to: hhmm(end),
  };
}

/** Geometric mean of a list of ratios, rounded to 3 decimals. */
function geo(rs: number[]): number {
  const g = Math.exp(rs.reduce((a, r) => a + Math.log(r), 0) / rs.length);
  return Math.round(g * 1000) / 1000;
}

/** The 33-config chapter: wins, Rust and Zig suites and geomean, before and after the fixes. */
export function full33Summary(data: PrData) {
  const f = data.full33;
  const sum = (k: "bR" | "bZ" | "fR" | "fZ") =>
    f.reduce((a, r) => a + r[k][0], 0);
  return {
    n: f.length,
    winsBefore: f.filter((r) => r.bR[0] < r.bZ[0]).length,
    winsAfter: f.filter((r) => r.fR[0] < r.fZ[0]).length,
    rustBefore: sum("bR"),
    rustAfter: sum("fR"),
    zigBefore: sum("bZ"),
    zigAfter: sum("fZ"),
    geoBefore: geo(f.map((r) => r.bR[0] / r.bZ[0])),
    geoAfter: geo(f.map((r) => r.fR[0] / r.fZ[0])),
  };
}

/** The advanced-pin chapter: wins, sums of medians, delta and the thinnest margin. */
export function advPinSummary(data: PrData) {
  const rows = data.advPin.rows;
  const ghostty = rows.reduce((a, r) => a + r.ghostty, 0);
  const rustty = rows.reduce((a, r) => a + r.rustty, 0);
  const thin = rows.reduce((m, r) => (r.run1 > m.run1 ? r : m), rows[0]);
  return {
    wins: rows.filter((r) => r.run1 < 1 && r.run2 < 1).length,
    n: rows.length,
    ghostty,
    rustty,
    deltaPct: (rustty / ghostty - 1) * 100,
    thinnest: thin.bench,
    thinRuns: [thin.run1, thin.run2] as const,
  };
}

/** Returns the stage with key k. */
function stage(data: PrData, k: string): Stage {
  const st = data.stages.find((s) => s.key === k);
  if (!st) throw new Error(`perf-replay: no stage ${k}`);
  return st;
}

/** Rust suite of a stage (sum of medians). */
function suiteOf(data: PrData, st: Stage): number {
  return data.bench.reduce((a, b) => a + st.rust[b][0], 0);
}

/**
 * Numbers behind the computed insights: the round 4+5 bundle against the
 * same-session control of the round-3 binary, the best consecutive stage
 * steps, the LTO detour (the fat-LTO chain's final 14-benchmark suite and
 * terminal-parser against the shipped profile), profile and session effects,
 * and the noise floor of
 * the final sweep (widest max/min on a non-floor config; thinnest margin).
 */
export function insightNumbers(data: PrData) {
  const ctl = stage(data, "control");
  const r5 = stage(data, "round5");
  const drop = (b: string) => ctl.rust[b][0] - r5.rust[b][0];
  const bundle = suiteOf(data, ctl) - suiteOf(data, r5);
  const zigSuite = data.bench.reduce((a, b) => a + r5.zig[b][0], 0);
  const steps = ["render", "arrayvec", "clone", "oscwin"].map((k) => {
    const i = data.stages.findIndex((s) => s.key === k);
    const st = data.stages[i];
    return {
      commit: st.commit,
      delta: suiteOf(data, st) - suiteOf(data, data.stages[i - 1]),
    };
  });
  const ranked = [...steps].sort((a, b) => a.delta - b.delta);
  const floor = new Set(data.floor);
  let noise = { bench: "", spread: 0 };
  let thinnest = { bench: "", pct: -100 };
  for (const b of data.bench) {
    if (floor.has(b)) continue;
    for (const side of [r5.rust[b], r5.zig[b]]) {
      if (side[2] / side[1] > noise.spread)
        noise = { bench: b, spread: side[2] / side[1] };
    }
    const pct = (ratio(r5, b) - 1) * 100;
    if (pct > thinnest.pct) thinnest = { bench: b, pct };
  }
  const tp = data.lto.chain.filter((c) => "terminal-parser" in c.ms);
  const shipped = data.lto.profiles[1].ms["terminal-parser"];
  return {
    parser: drop("terminal-parser"),
    parserFrom: ctl.rust["terminal-parser"][0],
    parserTo: r5.rust["terminal-parser"][0],
    parserShareOfZig: (drop("terminal-parser") / zigSuite) * 100,
    bundle,
    shares: {
      parser: drop("terminal-parser") / bundle,
      stream: drop("terminal-stream") / bundle,
      clone: drop("screen-clone.clone") / bundle,
    },
    best: ranked[0],
    next: ranked[1],
    ltoChainEnd: tp[tp.length - 1].ms["terminal-parser"],
    ltoChainSuite: Object.values(
      data.lto.chain[data.lto.chain.length - 1].ms,
    ).reduce((a, b) => a + b, 0),
    ltoShipped: shipped,
    profileOnly:
      (suiteOf(data, stage(data, "profile")) /
        suiteOf(data, stage(data, "session")) -
        1) *
      100,
    sessionFrom: suite(data, 0).deltaPct,
    sessionTo: suite(
      data,
      data.stages.findIndex((s) => s.key === "session"),
    ).deltaPct,
    noise,
    thinnest,
  };
}

/**
 * Commit time of a waterfall step: its latest commit, or for a re-measure
 * with no commit, the round-5 or round-3 ship commit it was measured at.
 */
export function waterfallStepTime(
  data: PrData,
  step: { commits: string[]; step: string },
): number {
  const times = step.commits
    .map((h) => data.commits.find((c) => c.h === h))
    .filter((c) => c !== undefined)
    .map((c) => tm(c.t));
  if (times.length) return Math.max(...times);
  const key = /round 5/.test(step.step) ? "round5" : "round3";
  return tm(stage(data, key).t);
}
