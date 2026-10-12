/** Pure view-model for the moving-target widget: data types, chart geometry and the size series (pin step function, lag area, hover lookups). */

/** One move of the reference commit, as generated into moving-target.json. */
export type Advance = {
  /** 1-based move number, shown as "move n". */
  n: number;
  /** Optional qualifier of the move (the two-commit drift). */
  label: string | null;
  /** Reference commit before the move (short hash). */
  from: string;
  /** Reference commit after the move (short hash). */
  to: string;
  /** Upstream commit time of the new reference (local, YYYY-MM-DDTHH:MM). */
  up: string;
  /** Time of the Rustty commit that moved the reference (local, YYYY-MM-DDTHH:MM). */
  adv: string;
  /** Hours from the upstream commit to the Rustty commit. */
  lag: number;
  /** Upstream commits in the range, merges included. */
  all: number;
  /** Upstream non-merge commits in the range. */
  nm: number;
  /** Files changed in the range. */
  files: number;
  /** Lines inserted in the range. */
  ins: number;
  /** Lines deleted in the range. */
  del: number;
  /** Ghostty .zig lines at the new reference. */
  loc: number;
  /** Short duration of the catch-up (porting the new range). */
  dur: string;
  /** Resync timing, in words. */
  durTxt: string;
  /** Rustty commits assigned to the catch-up by the era classifier. */
  era: number;
  /** Non-merge Rustty commits from the move until the catch-up finished, or null. */
  fin: number | null;
  /** Remark on the Rustty commit count, or null. */
  finNote: string | null;
  /** Reference comments missing on arrival, or null when not recorded. */
  cm: number | null;
  /** Test gap on arrival, in words, or null. */
  ts: string | null;
  /** The Rustty commit that moved the reference. */
  commit: string;
  /** The move's commit message, in plain words. */
  q: string;
  /** Remark under the churn bars, or null. */
  note: string | null;
  /** True for the two-commit drift, whose line counts come from the directory diffstat. */
  drift: boolean;
};

/** A [ins, del, files, commits] cell of the directory matrix, or null for an untouched directory. */
export type Cell = [number, number, number, number] | null;

/** One directory row of the matrix: a cell per move. */
export type AreaRow = { a: string; v: Cell[]; other?: boolean };

/** The generated data file. */
export type MtData = {
  baseline: { sha: string; date: string; loc: number };
  gh: [string, number][];
  rs: [string, number][];
  adv: Advance[];
  rows: AreaRow[];
  otherNames: string[];
};

/** Parses YYYY-MM-DD (taken as noon) or YYYY-MM-DDTHH:MM into epoch ms, reading every time as UTC so all series share one clock. */
export function tm(s: string): number {
  return Date.parse(s.length === 10 ? `${s}T12:00:00Z` : `${s}:00Z`);
}

/** GEO holds the chart geometry: the shared x axis and the two stacked panels. */
export const GEO = {
  W: 960,
  L: 62,
  R: 18,
  /** Top panel: Ghostty zoomed on 260k to 380k. */
  A: { t: 14, h: 160, lo: 260000, hi: 380000 },
  /** Bottom panel: both series from zero. */
  B: { t: 228, h: 170, lo: 0, hi: 900000 },
  H: 420,
  x0: tm("2026-06-20"),
  x1: tm("2026-10-11"),
} as const;

/** Maps v from [d0, d1] onto [r0, r1] linearly (the shared scales module cannot be imported here: Node type stripping needs extensionless imports to be types only). */
function lerp(
  v: number,
  d0: number,
  d1: number,
  r0: number,
  r1: number,
): number {
  return r0 + ((v - d0) * (r1 - r0)) / (d1 - d0);
}

/** Returns the x of epoch ms t on the shared axis. */
export function xOf(t: number): number {
  return lerp(t, GEO.x0, GEO.x1, GEO.L, GEO.W - GEO.R);
}

/** Returns the epoch ms at x on the shared axis (inverse of xOf). */
export function timeAt(x: number): number {
  return lerp(x, GEO.L, GEO.W - GEO.R, GEO.x0, GEO.x1);
}

/** Returns the y of line count v in the zoomed top panel. */
export function yA(v: number): number {
  const { t, h, lo, hi } = GEO.A;
  return lerp(v, lo, hi, t + h, t);
}

/** Returns the y of line count v in the bottom panel (from zero). */
export function yB(v: number): number {
  const { t, h, lo, hi } = GEO.B;
  return lerp(v, lo, hi, t + h, t);
}

/** Returns the pinned reference's size steps as [t, v] points: it holds, then jumps at each move. */
export function pinSteps(d: MtData): [number, number][] {
  const pins = [
    { t: tm(`${d.rs[0][0]}T00:00`), v: d.baseline.loc },
    ...d.adv.map((a) => ({ t: tm(a.adv), v: a.loc })),
  ];
  const pts: [number, number][] = [];
  pins.forEach((p, i) => {
    if (i > 0) pts.push([p.t, pins[i - 1].v]);
    pts.push([p.t, p.v]);
  });
  pts.push([tm(`${d.rs[d.rs.length - 1][0]}T23:00`), pins[pins.length - 1].v]);
  return pts;
}

/** Returns the pinned reference's .zig line count at epoch ms t. */
export function pinAt(d: MtData, t: number): number {
  let v = d.baseline.loc;
  for (const a of d.adv) if (tm(a.adv) <= t) v = a.loc;
  return v;
}

/** Returns the number of the move in effect at epoch ms t (0 before the first move). */
export function moveAt(d: MtData, t: number): number {
  let n = 0;
  for (const a of d.adv) if (tm(a.adv) <= t) n = a.n;
  return n;
}

/** Returns Ghostty main's .zig lines at epoch ms t, interpolated between weekly samples. */
export function mainAt(d: MtData, t: number): number {
  const g = d.gh.map(([day, v]) => [tm(day), v] as const);
  for (let i = 1; i < g.length; i++) {
    if (t <= g[i][0]) {
      const [a, va] = g[i - 1];
      const [b, vb] = g[i];
      return va + ((vb - va) * (t - a)) / (b - a);
    }
  }
  return g[g.length - 1][1];
}

/** Returns the latest Rustty .rs sample [t, lines] up to 12 hours after t, or null before the first. */
export function rsAt(d: MtData, t: number): [number, number] | null {
  let hit: [number, number] | null = null;
  for (const [day, v] of d.rs) {
    const at = tm(day);
    if (at <= t + 12 * 3600e3) hit = [at, v];
  }
  return hit;
}

/** Returns the polygon [t, v] points of the area between Ghostty main and the pin (lines the pin trails), sampled every 6 hours. */
export function lagArea(d: MtData): {
  top: [number, number][];
  bottom: [number, number][];
} {
  const end = tm(`${d.gh[d.gh.length - 1][0]}T13:00`);
  const top: [number, number][] = [];
  const bottom: [number, number][] = [];
  for (let t = tm(`${d.rs[0][0]}T00:00`); t <= end; t += 6 * 3600e3) {
    const pin = pinAt(d, t);
    top.push([t, Math.max(mainAt(d, t), pin)]);
    bottom.push([t, pin]);
  }
  return { top, bottom };
}

/** The largest gap between Ghostty main and the pin at a weekly sample. */
export type MaxLag = {
  date: string;
  main: number;
  pin: number;
  lag: number;
  pct: number;
};

/** Returns the weekly sample where the pin trailed Ghostty main by the most lines. */
export function maxLag(d: MtData): MaxLag {
  let best: MaxLag = { date: "", main: 0, pin: 0, lag: -1, pct: 0 };
  for (const [day, main] of d.gh) {
    const pin = pinAt(d, tm(day));
    if (main - pin > best.lag) {
      best = { date: day, main, pin, lag: main - pin, pct: (main - pin) / pin };
    }
  }
  return best;
}

/** What the hover tooltip shows at one moment. */
export type TimeTip = {
  /** The day (YYYY-MM-DD). */
  date: string;
  /** Ghostty main's lines. */
  main: number;
  /** The pinned reference's short hash. */
  pinSha: string;
  /** The pinned reference's lines. */
  pin: number;
  /** Lines the pin trails main (never negative). */
  trail: number;
  /** Rustty .rs lines, or null before the first sample. */
  rs: number | null;
  /** Rustty lines divided by the pinned Ghostty lines, or null. */
  ratio: number | null;
};

/** Returns the tooltip values at epoch ms t (main is held at its last sample after the final week). */
export function tipAt(d: MtData, t: number): TimeTip {
  const last = tm(`${d.gh[d.gh.length - 1][0]}T13:00`);
  const main = Math.round(mainAt(d, Math.min(t, last)));
  const pin = pinAt(d, t);
  const n = moveAt(d, t);
  const r = rsAt(d, t);
  return {
    date: new Date(t).toISOString().slice(0, 10),
    main,
    pinSha: n === 0 ? d.baseline.sha : d.adv[n - 1].to,
    pin,
    trail: Math.max(0, main - pin),
    rs: r ? r[1] : null,
    ratio: r ? r[1] / pin : null,
  };
}

/** Returns the index of the move whose Rustty commit time is nearest to epoch ms t. */
export function nearestAdvance(d: MtData, t: number): number {
  let best = 0;
  let bd = Number.POSITIVE_INFINITY;
  d.adv.forEach((a, i) => {
    const dd = Math.abs(tm(a.adv) - t);
    if (dd < bd) {
      bd = dd;
      best = i;
    }
  });
  return best;
}

/** Returns the label of a move as shown: "move 3", or "move 4 (2-commit drift)". */
export function moveName(a: Advance): string {
  return a.label ? `move ${a.n} (${a.label})` : `move ${a.n}`;
}
