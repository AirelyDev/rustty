/** Pure view-model for the moving-target widget: header figures, matrix values, the selected move's detail and the three insight tiles. */

import type { Advance, AreaRow, Cell, MtData } from "./model";

/** Which value the matrix cells show. */
export type Metric = "churn" | "commits";

/** Returns the median of a non-empty list of numbers. */
export function median(xs: number[]): number {
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

/** Returns the lines changed (inserted plus deleted) in a matrix cell. */
export function churnOf(c: Cell): number {
  return c ? c[0] + c[1] : 0;
}

/** Returns the value a matrix cell shows for metric (lines changed, or commits touching). */
export function cellVal(c: Cell, metric: Metric): number {
  if (!c) return 0;
  return metric === "churn" ? c[0] + c[1] : c[3];
}

/** Returns the biggest cell value in the matrix for metric. */
export function matrixMax(d: MtData, metric: Metric): number {
  return Math.max(...d.rows.flatMap((r) => r.v.map((c) => cellVal(c, metric))));
}

/** Returns a row's total for metric, summed over the moves. */
export function rowTotal(r: AreaRow, metric: Metric): number {
  return r.v.reduce((s, c) => s + cellVal(c, metric), 0);
}

/** Returns the teal alpha of a cell value on a log scale (0 for an untouched cell). */
export function cellAlpha(v: number, max: number): number {
  return v ? 0.1 + (0.85 * Math.log1p(v)) / Math.log1p(max) : 0;
}

/** Returns a cell's text: lines changed abbreviated to k from 1,000, or the commit count; "·" when untouched. */
export function cellLabel(v: number, metric: Metric): string {
  if (!v) return "·";
  if (metric === "commits" || v < 1000) return String(v);
  return `${(v / 1000).toFixed(v >= 10000 ? 0 : 1)}k`;
}

/** Returns a row's label as shown (translations spelled out). */
export function rowLabel(r: AreaRow): string {
  return r.a === "po" ? "po (translations)" : r.a;
}

/** The figures of the header and the matrix's "all moves" column. */
export type Totals = {
  /** Upstream non-merge commits over all moves. */
  nonMerge: number;
  /** Lines inserted / deleted in the ranges with recorded diffstats (the stat tile). */
  phaseIns: number;
  phaseDel: number;
  /** Lines inserted / deleted summed over the directory matrix (includes the drift's commits). */
  matrixIns: number;
  matrixDel: number;
  /** Files changed over all moves. */
  files: number;
  /** Rustty commits over all moves. */
  era: number;
  /** Median wait in hours (rounded to 0.01 h: the mean of two waits can carry float noise), and the first and the shortest wait. */
  medianLag: number;
  firstLag: number;
  lastLag: number;
  minLag: number;
  /** The number of the move with the shortest wait. */
  minMove: number;
  /** The drift move excluded from the header line tile: its number and lines inserted / deleted. */
  driftN: number;
  driftIns: number;
  driftDel: number;
  /** The shortest-wait moves' numbers (waits under two hours). */
  quickMoves: number[];
  /** Ghostty .zig lines at the start and at the last reference, and the growth in percent. */
  loc0: number;
  loc1: number;
  growthPct: number;
  /** The first and the last reference commit. */
  firstSha: string;
  lastSha: string;
  /** Churn in src/terminal, the matrix total, and the terminal share in percent. */
  terminalChurn: number;
  totalChurn: number;
  terminalPct: number;
  /** Churn in the translations and in pkg. */
  poChurn: number;
  pkgChurn: number;
};

/** Returns the header and matrix-total figures derived from the data. */
export function totals(d: MtData): Totals {
  const sum = (f: (a: Advance) => number) =>
    d.adv.reduce((s, a) => s + f(a), 0);
  const phase = d.adv.filter((a) => !a.drift);
  const cells = d.rows.flatMap((r) => r.v);
  const churn = (name: string) => {
    const r = d.rows.find((x) => x.a === name);
    return r ? rowTotal(r, "churn") : 0;
  };
  const lags = d.adv.map((a) => a.lag);
  const total = d.rows.reduce((s, r) => s + rowTotal(r, "churn"), 0);
  const loc0 = d.baseline.loc;
  const loc1 = d.adv[d.adv.length - 1].loc;
  return {
    nonMerge: sum((a) => a.nm),
    phaseIns: phase.reduce((s, a) => s + a.ins, 0),
    phaseDel: phase.reduce((s, a) => s + a.del, 0),
    matrixIns: cells.reduce((s, c) => s + (c ? c[0] : 0), 0),
    matrixDel: cells.reduce((s, c) => s + (c ? c[1] : 0), 0),
    files: sum((a) => a.files),
    era: sum((a) => a.era),
    medianLag: Math.round(median(lags) * 100) / 100,
    firstLag: lags[0],
    lastLag: lags[lags.length - 1],
    minLag: Math.min(...lags),
    minMove: d.adv[lags.indexOf(Math.min(...lags))].n,
    driftN: d.adv.find((a) => a.drift)?.n ?? 0,
    driftIns: d.adv.find((a) => a.drift)?.ins ?? 0,
    driftDel: d.adv.find((a) => a.drift)?.del ?? 0,
    quickMoves: d.adv.filter((a) => a.lag < 2).map((a) => a.n),
    loc0,
    loc1,
    growthPct: (loc1 / loc0 - 1) * 100,
    firstSha: d.baseline.sha,
    lastSha: d.adv[d.adv.length - 1].to,
    terminalChurn: churn("src/terminal"),
    totalChurn: total,
    terminalPct: (churn("src/terminal") / total) * 100,
    poChurn: churn("po"),
    pkgChurn: churn("pkg"),
  };
}

/** One bar of the selected move's "where the churn landed" list. */
export type ChurnBar = { name: string; churn: number; pct: number };

/** Returns the top areas of move index sel by lines changed, as shares of the move's inserted plus deleted lines. */
export function topAreas(d: MtData, sel: number, count = 5): ChurnBar[] {
  const a = d.adv[sel];
  const tot = a.ins + a.del;
  return d.rows
    .map((r) => ({ name: rowLabel(r), churn: churnOf(r.v[sel]) }))
    .filter((x) => x.churn > 0)
    .sort((p, q) => q.churn - p.churn)
    .slice(0, count)
    .map((x) => ({ ...x, pct: (x.churn / tot) * 100 }));
}

/** Formats an integer with thousands separators. */
export function int(n: number): string {
  return n.toLocaleString("en-US");
}

/** Returns the figures and texts of the three insight tiles. */
export function insights(d: MtData): { n: string; t: string }[] {
  const t = totals(d);
  const withCm = d.adv.filter((a) => a.cm !== null);
  const [c1, c2] = withCm;
  const moves = t.quickMoves.join(" and ");
  return [
    {
      n: `${t.firstLag} h / ${t.minLag} h / ${t.lastLag} h`,
      t: `Wait from the upstream commit to the reference moving: first move ${t.firstLag} h, as little as ${t.minLag} h (move ${t.minMove}), last move ${t.lastLag} h; median ${t.medianLag} h. Moves ${moves} came in under two hours.`,
    },
    {
      n: `${int(c1.cm ?? 0)} / ${int(c2.cm ?? 0)}`,
      t: `Reference comments missing the moment the reference moved in move ${c1.n} and in move ${c2.n}: the checks turn red on purpose and their count is the progress meter. Both reached 0 when the catch-up finished.`,
    },
    {
      n: `${int(t.terminalChurn)} lines`,
      t: `of the ${int(t.totalChurn)} lines of upstream churn (${Math.round(t.terminalPct)}%) touched src/terminal, the VT engine. Translations (po/) are next at ${int(t.poChurn)}, then pkg/ at ${int(t.pkgChurn)}.`,
    },
  ];
}
