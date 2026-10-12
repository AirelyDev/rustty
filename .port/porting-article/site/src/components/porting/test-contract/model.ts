/**
 * Pure view-model for "The test contract" widget: per-day Rust test counts per
 * module expanded from change points, the Zig contract in force each day, the
 * square grid (order, layout, colour classes), the parity scatter, the
 * per-sync table and the insight values. No imports, so
 * scripts/verify-porting-helpers.mjs can run it under Node's type stripping.
 *
 * Day index i runs over data.dates (2026-06-21 … 2026-10-10); pin index k runs
 * over data.pins (0 = first reference commit, 1…7 = upstream syncs).
 */

/** One reference commit of the Ghostty tree the port was checked against. */
export type Pin = {
  /** Day the port moved to it (YYYY-MM-DD). */
  date: string;
  /** Ghostty commit. */
  sha: string;
  /** Zig tests in the 211 mapped modules at this commit. */
  mapped: number;
  /** Zig test blocks in Ghostty src/ at this commit. */
  src: number;
  /** Upstream non-merge commits absorbed by this sync (0 for the first). */
  upstream: number;
};

/** One mapped module as stored: name, crate, Zig count per pin, Rust count change points. */
export type ModRow = {
  m: string;
  c: string;
  z: number[];
  r: [number, number][];
};

/** The 185 silent skips and the Rust/Zig skip-mechanism census. */
export type SkipData = {
  byCls: Record<string, number>;
  byKind: Record<string, number>;
  byCrate: [string, number][];
  grp: Record<string, number>;
  zigPort: number;
  zig: { total: number; skip: number; build: number; runtime: number };
  rust: {
    total: number;
    none: number;
    miri: number;
    miriDebugOff: number;
    cfgOnly: number;
    cfgIgnore: number;
    ignore: number;
    /** Tests with both a Miri and a platform/feature ignore (counted in miri, not in cfgIgnore). */
    miriAndCfgIgnore: number;
  };
};

/** The widget's data file (src/app/porting/data/test-contract.json). */
export type TcData = {
  dates: string[];
  revs: string[];
  rust: { all: number[]; vt: number[]; vtc: number[] };
  pins: Pin[];
  mods: ModRow[];
  skip: SkipData;
};

/** A module with its Rust count expanded to one value per day. */
export type Mod = {
  m: string;
  c: string;
  z: number[];
  R: number[];
  tier: number;
  /** Largest Zig count over all pins (squares the module owns in the grid). */
  zmax: number;
  /** First day the module was at or above the Zig count in force (nDays if never). */
  first: number;
};

/** Contract status on one day. */
export type DayStats = {
  met: number;
  need: number;
  below: number;
  deficit: number;
  pin: number;
};

/** TIERS groups crates for the scatter: label and member crates (colour is by index in the view). */
export const TIERS: readonly [string, readonly string[]][] = [
  ["Terminal engine", ["rustty-vt"]],
  ["C APIs", ["rustty-vt-c", "rustty-apprt-c"]],
  [
    "Font & renderer",
    [
      "rustty-font",
      "rustty-font-shaper",
      "rustty-renderer",
      "rustty-render-types",
      "rustty-wuffs",
    ],
  ],
  [
    "App runtime",
    [
      "rustty-apprt",
      "rustty-apprt-api",
      "rustty-core",
      "rustty-termio",
      "rustty-crash",
      "rustty-inspector",
      "rustty",
    ],
  ],
  [
    "Libraries & deps",
    [
      "rustty-xev",
      "rustty-vaxis",
      "rustty-zf",
      "rustty-simd",
      "rustty-datastruct",
      "rustty-lib",
    ],
  ],
  [
    "Config, CLI, input",
    [
      "rustty-config",
      "rustty-cfg-fmt",
      "rustty-cli",
      "rustty-args",
      "rustty-input",
      "rustty-extra",
      "rustty-terminfo",
    ],
  ],
];

/** DEFAULT_TIER is the tier of crates not listed in TIERS (libraries & deps). */
const DEFAULT_TIER = 4;

/**
 * EXCLUDED is the one mapping artifact left out of "all met": config_key's
 * single test lived in config_struct.rs until a 10-01 file split, so today's
 * file mapping reads 0 for it before then.
 */
export const EXCLUDED = "config_key";

/** CRATE_NAMES are the reader-facing names of the crates the insight text names. */
const CRATE_NAMES: Record<string, string> = {
  "rustty-font-shaper": "font shaper",
  "rustty-xev": "libxev",
  "rustty-vaxis": "libvaxis",
  "rustty-cli": "CLI",
  "rustty-termio": "termio",
};

/** Formats an integer with thousands separators. */
export function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

/** Returns the MM-DD part of a YYYY-MM-DD date. */
export function md(date: string): string {
  return date.slice(5);
}

/** Returns the reader-facing name of pin k. */
export function pinLabel(k: number): string {
  return k === 0 ? "first reference commit" : `upstream sync ${k}`;
}

/** Returns the reader-facing name of a crate (its short name for a few, else the crate name). */
export function crateName(c: string): string {
  return CRATE_NAMES[c] ?? c;
}

/** Returns the tier index of a crate. */
export function tierOf(c: string): number {
  const i = TIERS.findIndex(([, crates]) => crates.includes(c));
  return i < 0 ? DEFAULT_TIER : i;
}

/** Returns the index of the pin in force on each day. */
export function pinOfDay(data: TcData): number[] {
  return data.dates.map((d) => {
    let k = 0;
    data.pins.forEach((p, i) => {
      if (p.date <= d) k = i;
    });
    return k;
  });
}

/** Expands every module's Rust change points into a per-day series and adds tier, zmax and first-met day. */
export function expandMods(data: TcData): Mod[] {
  const nd = data.dates.length;
  const pod = pinOfDay(data);
  return data.mods.map(({ m, c, z, r }) => {
    const R = new Array<number>(nd).fill(0);
    let k = 0;
    for (let i = 0; i < nd; i++) {
      while (k < r.length && r[k][0] <= i) k++;
      R[i] = k ? r[k - 1][1] : 0;
    }
    let first = nd;
    for (let i = 0; i < nd; i++) {
      const zk = z[pod[i]];
      if (zk > 0 && R[i] >= zk) {
        first = i;
        break;
      }
    }
    return { m, c, z, R, tier: tierOf(c), zmax: Math.max(...z), first };
  });
}

/** Returns the contract status on day i. */
export function dayStats(data: TcData, mods: Mod[], i: number): DayStats {
  const k = pinOfDay(data)[i];
  let met = 0;
  let need = 0;
  let below = 0;
  let deficit = 0;
  for (const o of mods) {
    const z = o.z[k];
    met += Math.min(o.R[i], z);
    need += z;
    if (o.R[i] < z) {
      below++;
      deficit += z - o.R[i];
    }
  }
  return { met, need, below, deficit, pin: k };
}

/** Returns the contract status for every day. */
export function allDayStats(data: TcData, mods: Mod[]): DayStats[] {
  return data.dates.map((_, i) => dayStats(data, mods, i));
}

/** Returns the contract status on a date (YYYY-MM-DD). */
export function coverageOn(data: TcData, mods: Mod[], date: string): DayStats {
  const i = data.dates.indexOf(date);
  if (i < 0) throw new Error(`no snapshot on ${date}`);
  return dayStats(data, mods, i);
}

/** Returns whether every module but EXCLUDED is at or above its Zig count on day i. */
function allMet(mods: Mod[], i: number, k: number): boolean {
  return mods.every((o) => o.m === EXCLUDED || o.R[i] >= o.z[k]);
}

/** Returns the first day ≥ from on which every module (but EXCLUDED) met the contract in force, or -1. */
export function firstAllMet(data: TcData, mods: Mod[], from: number): number {
  const pod = pinOfDay(data);
  for (let i = Math.max(0, from); i < data.dates.length; i++) {
    if (allMet(mods, i, pod[i])) return i;
  }
  return -1;
}

/** Returns the first day every terminal-engine (rustty-vt) module met its Zig count, or -1. */
export function engineFirstMet(data: TcData, mods: Mod[]): number {
  const pod = pinOfDay(data);
  const vt = mods.filter((o) => o.c === "rustty-vt");
  for (let i = 0; i < data.dates.length; i++) {
    if (vt.every((o) => o.R[i] >= o.z[pod[i]])) return i;
  }
  return -1;
}

/** Returns the missing tests per crate on day i, largest first (crates with no deficit omitted). */
export function deficitByCrate(
  data: TcData,
  mods: Mod[],
  i: number,
): [string, number][] {
  const k = pinOfDay(data)[i];
  const by = new Map<string, number>();
  for (const o of mods) {
    const d = o.z[k] - o.R[i];
    if (d > 0) by.set(o.c, (by.get(o.c) ?? 0) + d);
  }
  return [...by.entries()].sort((a, b) => b[1] - a[1]);
}

/** A module placed in the grid: its first square index and the pin each square arrived with. */
export type GridMod = Mod & { start: number; arr: number[] };

/** The grid's module order and its number of squares. */
export type GridOrder = { mods: GridMod[]; total: number };

/**
 * Orders the modules with Zig tests by the day they first met their Zig count
 * (then largest first) and assigns each square the pin it arrived with.
 */
export function gridOrder(mods: Mod[]): GridOrder {
  const last = mods[0]?.z.length - 1 || 0;
  const sorted = mods
    .filter((o) => o.zmax > 0)
    .sort((a, b) => a.first - b.first || b.zmax - a.zmax);
  let start = 0;
  const out = sorted.map((o) => {
    const arr: number[] = [];
    for (let j = 0; j < o.zmax; j++) {
      const k = o.z.findIndex((v) => v > j);
      arr.push(k < 0 ? last : k);
    }
    const g = { ...o, start, arr };
    start += o.zmax;
    return g;
  });
  return { mods: out, total: start };
}

/** Square grid geometry: cell pitch in px, rows, columns, pixel size and square count. */
export type GridLayout = {
  cell: number;
  rows: number;
  cols: number;
  width: number;
  height: number;
  total: number;
};

/**
 * Lays out total squares column-major in a box width px wide: 7 px cells
 * (4 px under 520 px), rows chosen to keep the band readable.
 */
export function gridLayout(total: number, width: number): GridLayout {
  const want = width < 520 ? 4 : 7;
  const rows = Math.max(
    Math.ceil(Math.sqrt(total * 0.22)),
    Math.ceil(total / Math.max(1, Math.floor(width / want))),
  );
  const cols = Math.ceil(total / rows);
  const cell = Math.min(want, Math.floor(width / cols)) || 4;
  return {
    cell,
    rows,
    cols,
    width: cols * cell,
    height: rows * cell,
    total,
  };
}

/** Returns the square index under grid-local point (x, y), or -1. */
export function gridHit(l: GridLayout, x: number, y: number): number {
  const col = Math.floor(x / l.cell);
  const row = Math.floor(y / l.cell);
  if (x < 0 || row < 0 || row >= l.rows) return -1;
  const idx = col * l.rows + row;
  return idx < l.total ? idx : -1;
}

/** Returns the top-left pixel of square idx. */
export function squareAt(l: GridLayout, idx: number): [number, number] {
  return [Math.floor(idx / l.rows) * l.cell, (idx % l.rows) * l.cell];
}

/** Returns the index of the module that owns square idx. */
export function modAtSquare(order: GridOrder, idx: number): number {
  let lo = 0;
  let hi = order.mods.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (order.mods[mid].start <= idx) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Returns the sync whose day is within the last 3 days of day i (squares it brought are highlighted), or -1. */
export function recentSync(data: TcData, i: number): number {
  const k = pinOfDay(data)[i];
  for (let q = k; q > 0; q--) {
    const di = data.dates.indexOf(data.pins[q].date);
    if (di >= 0 && i >= di && i - di <= 2) return q;
  }
  return -1;
}

/**
 * A square's colour class: future (arrives with a later sync), open (no Rust
 * counterpart counted), covered; "New" = arrived with a sync in the last 3
 * days; the 0/1 suffix alternates per module.
 */
export type SquareClass =
  | "future0"
  | "future1"
  | "open0"
  | "open1"
  | "openNew"
  | "cov0"
  | "cov1"
  | "covNew";

/** Returns one SVG path per colour class drawing every square of the grid on day i. */
export function gridPaths(
  data: TcData,
  order: GridOrder,
  l: GridLayout,
  i: number,
): Partial<Record<SquareClass, string>> {
  const k = pinOfDay(data)[i];
  const recent = recentSync(data, i);
  const s = l.cell - 1;
  const parts: Partial<Record<SquareClass, string[]>> = {};
  order.mods.forEach((o, mi) => {
    const alt = mi % 2;
    for (let j = 0; j < o.zmax; j++) {
      let cls: SquareClass;
      const isNew = recent > 0 && o.arr[j] === recent;
      if (j >= o.z[k]) cls = alt ? "future1" : "future0";
      else if (o.R[i] > j) cls = isNew ? "covNew" : alt ? "cov1" : "cov0";
      else cls = isNew ? "openNew" : alt ? "open1" : "open0";
      const [x, y] = squareAt(l, o.start + j);
      const list = parts[cls] ?? [];
      list.push(`M${x} ${y}h${s}v${s}h-${s}z`);
      parts[cls] = list;
    }
  });
  const out: Partial<Record<SquareClass, string>> = {};
  for (const [key, list] of Object.entries(parts)) {
    out[key as SquareClass] = (list as string[]).join("");
  }
  return out;
}

/** Returns the outline path around every square of grid module mi. */
export function moduleOutline(
  order: GridOrder,
  l: GridLayout,
  mi: number,
): string {
  const o = order.mods[mi];
  const s = l.cell;
  let d = "";
  for (let j = 0; j < o.zmax; j++) {
    const [x, y] = squareAt(l, o.start + j);
    d += `M${x - 0.5} ${y - 0.5}h${s}v${s}h-${s}z`;
  }
  return d;
}

/** Labels for big modules (≥ 140 squares): x position in px and name. */
export function gridLabels(
  order: GridOrder,
  l: GridLayout,
): [number, string][] {
  const out: [number, string][] = [];
  let lastX = -99;
  for (const o of order.mods) {
    if (o.zmax < 140) continue;
    const x = Math.floor((o.start + o.zmax / 2) / l.rows) * l.cell;
    if (x - lastX > 60) {
      out.push([x, o.m.replace(/_/g, " ")]);
      lastX = x;
    }
  }
  return out;
}

/** Returns the replay tick in ms after day i: slow for the first four weeks, then fast. */
export function replayDelay(i: number): number {
  return i < 28 ? 320 : 110;
}

/** Returns the sparkline path of the covered share per day in a w × h box. */
export function sparkPath(stats: DayStats[], w: number, h: number): string {
  const n = stats.length;
  return stats
    .map((d, i) => {
      const x = (i / (n - 1)) * w;
      const y = h - 2 - (d.met / d.need) * (h - 6);
      return `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join("");
}

/** One annotated event of the replay: day, emphasised lead and the rest of the line. */
export type ReplayEvent = { day: number; lead: string; text: string };

/** Returns the replay's annotated events: first tests, engine contract met, and every sync. */
export function replayEvents(data: TcData, mods: Mod[]): ReplayEvent[] {
  const vt = engineFirstMet(data, mods);
  const out: ReplayEvent[] = [
    {
      day: 0,
      lead: "",
      text: `first Rust tests counted: ${fmt(data.rust.vt[0])} in the terminal engine`,
    },
  ];
  if (vt >= 0) {
    out.push({
      day: vt,
      lead: "every terminal-engine module at or above its Zig count",
      text: `: day ${vt + 1}, ${fmt(data.rust.vt[vt])} engine tests`,
    });
  }
  data.pins.forEach((p, k) => {
    if (!k) return;
    out.push({
      day: data.dates.indexOf(p.date),
      lead: pinLabel(k),
      text: `: reference moved to ${p.sha} (${p.upstream} upstream commits), Zig contract ${fmt(data.pins[k - 1].mapped)} → ${fmt(p.mapped)}`,
    });
  });
  return out;
}

/** Returns the events shown on day i (those of the last 5 days). */
export function eventsAt(events: ReplayEvent[], i: number): ReplayEvent[] {
  return events.filter((e) => e.day <= i && i - e.day <= 4);
}

/** The longest stretch a sync left a module below its new Zig count. */
export type Gap = { days: number; mod: string; from: number; to: number };

/** Returns the longest number of days a sync left one module (but EXCLUDED) below its Zig count. */
export function longestGap(data: TcData, mods: Mod[]): Gap {
  const pod = pinOfDay(data);
  let best: Gap = { days: 0, mod: "", from: -1, to: -1 };
  data.pins.forEach((p, k) => {
    if (!k) return;
    const i0 = data.dates.indexOf(p.date);
    for (const o of mods) {
      if (o.m === EXCLUDED || o.R[i0] >= o.z[k]) continue;
      let j = i0;
      while (j < data.dates.length && o.R[j] < o.z[pod[j]]) j++;
      if (j < data.dates.length && j - i0 > best.days) {
        best = { days: j - i0, mod: o.m, from: i0, to: j };
      }
    }
  });
  return best;
}

/** One sync row: Zig tests added, how many were pre-covered by count, and the day all were met again. */
export type SyncRow = {
  k: number;
  date: string;
  upstream: number;
  added: number;
  pre: number;
  allMet: number;
  lag: number;
};

/** The per-sync table and its totals. */
export type SyncTable = {
  rows: SyncRow[];
  upstream: number;
  added: number;
  pre: number;
};

/**
 * Returns one row per sync: mapped Zig tests added (per module, growth only),
 * how many of them the module's Rust surplus already covered the day before,
 * and the first day every module (but EXCLUDED) was at or above again.
 */
export function syncTable(data: TcData, mods: Mod[]): SyncTable {
  const rows: SyncRow[] = [];
  data.pins.forEach((p, k) => {
    if (!k) return;
    const i0 = data.dates.indexOf(p.date);
    let added = 0;
    let pre = 0;
    for (const o of mods) {
      const a = Math.max(0, o.z[k] - o.z[k - 1]);
      added += a;
      pre += Math.min(a, Math.max(0, o.R[i0 - 1] - o.z[k - 1]));
    }
    const j = firstAllMet(data, mods, i0);
    rows.push({
      k,
      date: p.date,
      upstream: p.upstream,
      added,
      pre,
      allMet: j,
      lag: j < 0 ? -1 : j - i0,
    });
  });
  return {
    rows,
    upstream: rows.reduce((s, r) => s + r.upstream, 0),
    added: rows.reduce((s, r) => s + r.added, 0),
    pre: rows.reduce((s, r) => s + r.pre, 0),
  };
}

/** One scatter point: a module's Zig and Rust test counts on the last day. */
export type Point = { mod: Mod; z: number; r: number; tier: number };

/** Returns the scatter points (final Zig count vs final Rust count) for every module. */
export function scatterPoints(data: TcData, mods: Mod[]): Point[] {
  const last = data.dates.length - 1;
  const lastPin = data.pins.length - 1;
  return mods.map((o) => ({
    mod: o,
    z: o.z[lastPin],
    r: o.R[last],
    tier: o.tier,
  }));
}

/** Modules with at least twice as many Rust tests as Zig tests, and their totals. */
export type Doubled = { mods: Point[]; zig: number; rust: number };

/** Returns the modules (with Zig tests) at ≥ 2× on the last day. */
export function doubled(data: TcData, mods: Mod[]): Doubled {
  const pts = scatterPoints(data, mods).filter(
    (p) => p.z > 0 && p.r >= 2 * p.z,
  );
  return {
    mods: pts,
    zig: pts.reduce((s, p) => s + p.z, 0),
    rust: pts.reduce((s, p) => s + p.r, 0),
  };
}

/** Returns log10(1 + v), the scatter's axis transform. */
export function log1p10(v: number): number {
  return Math.log10(1 + v);
}

/** Returns a step-line path through one value per day using the x and y scales. */
export function stepPath(
  values: number[],
  x: (i: number) => number,
  y: (v: number) => number,
): string {
  let d = "";
  values.forEach((v, i) => {
    d += i
      ? `H${x(i).toFixed(1)}V${y(v).toFixed(1)}`
      : `M${x(0).toFixed(1)},${y(v).toFixed(1)}`;
  });
  return `${d}H${x(values.length - 1).toFixed(1)}`;
}

/** An insight tile: big value and explanation. */
export type Tile = { value: string; text: string };

/** Returns the three insight tiles under the grid. */
export function gridInsights(data: TcData, mods: Mod[]): Tile[] {
  const vt = engineFirstMet(data, mods);
  const w1 = data.dates.indexOf("2026-06-28");
  const s1 = dayStats(data, mods, w1);
  const all0 = firstAllMet(data, mods, 0);
  const sync1 = data.dates.indexOf(data.pins[1].date);
  const deficit = deficitByCrate(data, mods, w1)
    .slice(0, 5)
    .map(([c, n]) => `${crateName(c)} ${n}`)
    .join(", ");
  const vaxis = mods.find((o) => o.m === "vaxis");
  let vxMet = -1;
  if (vaxis) {
    const pod = pinOfDay(data);
    for (let i = sync1; i < data.dates.length && vxMet < 0; i++) {
      if (vaxis.R[i] >= vaxis.z[pod[i]]) vxMet = i;
    }
  }
  const gap = longestGap(data, mods);
  return [
    {
      value: `day ${vt + 1} · ${md(data.dates[vt])}`,
      text: `Every terminal-engine module first reached its Zig count. Engine tests went ${fmt(data.rust.vt[0])} → ${fmt(data.rust.vt[vt])} in ${vt} days, and all crates hit ${fmt(data.rust.all[5])} by ${md(data.dates[5])} (the 06-28 weekly snapshot is the same commit).`,
    },
    {
      value: `${((100 * s1.met) / s1.need).toFixed(0)}% in week 1`,
      text: `${fmt(s1.met)} of the ${fmt(s1.need)} mapped Zig tests were covered by count on 06-28. The last ${fmt(s1.need - s1.met)} took until ${md(data.dates[all0])}: ${deficit}. Everything outside the engine and the C API came later.`,
    },
    {
      value: `${gap.days} days`,
      text: `The longest gap the moving contract opened. The whole first-day contract was met on ${md(data.dates[all0])}, ${sync1 - all0 === 1 ? "one day" : `${sync1 - all0} days`} before the first upstream sync. That sync also bumped the libvaxis TUI package (+${vaxis ? vaxis.z[1] - vaxis.z[0] : 0} tests), and libvaxis was not level again until ${vxMet < 0 ? "—" : md(data.dates[vxMet])}.`,
    },
  ];
}
