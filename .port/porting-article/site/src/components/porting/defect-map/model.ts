/**
 * Pure view-model for the defect-map widget: the class, method, status and
 * ReleaseFast vocabularies, the squarified treemap of Ghostty's Zig source
 * with one mark per defect, the filters, and the side-panel aggregates.
 * Ported from the approved mockup (08-defect-map).
 */

/** A colour role name of the shared PALETTE (resolved by the components). */
export type Role =
  | "fail"
  | "purple"
  | "yellow"
  | "blue"
  | "orange"
  | "pass"
  | "gray3"
  | "gray4"
  | "gray5";

/** A mark glyph: one per discovery method. */
export type Glyph =
  | "circle"
  | "diamond"
  | "tri"
  | "square"
  | "tridown"
  | "pent"
  | "star"
  | "hex"
  | "bow"
  | "cross"
  | "trileft";

/** One defect row of the generated data (ids are not shipped; n is the ledger number, never shown). */
export type Def = {
  n: number;
  t: string;
  c: string;
  m: string;
  md: string;
  conf: "h" | "m" | "k";
  sev: string;
  p: string;
  leaf: string;
  f: string;
  s: string;
  k: string;
  dt: string;
  doc: boolean;
  rf: string;
};

/** The generated defect-map.json: top tiles [name, Zig lines, [leaf, lines][]] and defects. */
export type DmData = {
  tree: { n: string; v: number; k: [string, number][] }[];
  defs: Def[];
  reported: number;
};

/** The lazily loaded defect-map-details.json: [n, mechanism, ReleaseFast, Rustty instead, upstream]. */
export type DmDetails = { d: [number, string, string, string, string][] };

/**
 * CLASSES lists the defect classes in legend order (hollow = not live; dash =
 * test weakness; dot = a centre dot, so hang stays apart from UB, whose purple
 * is close to hang's blue under deuteranopia).
 */
export const CLASSES = [
  { key: "crash", name: "crash", short: "crash", role: "fail" },
  { key: "ub", name: "UB", short: "UB", role: "purple" },
  { key: "leak", name: "leak", short: "leak", role: "yellow" },
  {
    key: "hang",
    name: "hang / deadlock",
    short: "hang",
    role: "blue",
    dot: true,
  },
  { key: "intent", name: "broken intent", short: "intent", role: "orange" },
  {
    key: "latent",
    name: "latent",
    short: "latent",
    role: "gray5",
    hollow: true,
  },
  {
    key: "test",
    name: "test weakness",
    short: "test",
    role: "gray4",
    hollow: true,
    dash: true,
  },
] as const satisfies readonly {
  key: string;
  name: string;
  short: string;
  role: Role;
  hollow?: boolean;
  dash?: boolean;
  dot?: boolean;
}[];

/** METHODS lists the discovery methods (estimated hand pass) with their mark glyph. */
export const METHODS = [
  { key: "read", name: "reading / review", shape: "circle" },
  { key: "arith", name: "checked arithmetic, casts, bounds", shape: "diamond" },
  { key: "xplat", name: "cross-platform bring-up", shape: "tri" },
  {
    key: "compile",
    name: "compiling code Zig never analysed",
    shape: "square",
  },
  { key: "fault", name: "fault injection", shape: "tridown" },
  { key: "testport", name: "test port", shape: "pent" },
  { key: "types", name: "type / borrow checker", shape: "star" },
  { key: "build", name: "build-system port", shape: "hex" },
  { key: "live", name: "live testing of the port", shape: "bow" },
  { key: "san", name: "sanitizer", shape: "cross" },
  {
    key: "withheld",
    name: "not hand-classified (was private)",
    shape: "trileft",
  },
] as const satisfies readonly { key: string; name: string; shape: Glyph }[];

/** STATUSES lists the upstream statuses in bar order. */
export const STATUSES = [
  { key: "fixed", name: "fixed upstream independently", role: "pass" },
  { key: "partly", name: "partly fixed upstream", role: "blue" },
  { key: "obsolete", name: "obsolete upstream", role: "gray4" },
  { key: "priv", name: "to be reported privately", role: "purple" },
  { key: "nr", name: "not reported yet", role: "gray3" },
] as const satisfies readonly { key: string; name: string; role: Role }[];

/** RFAST lists what a crash-class entry becomes in a ReleaseFast build (estimated). */
export const RFAST = [
  { key: "ub", name: "undefined behaviour", role: "purple" },
  { key: "wrong", name: "silently wrong value", role: "orange" },
  { key: "term", name: "still crashes / aborts", role: "fail" },
] as const satisfies readonly { key: string; name: string; role: Role }[];

/** RF_ORDER is the waffle order of ReleaseFast consequences. */
export const RF_ORDER: readonly string[] = RFAST.map((r) => r.key);

/** The map filter: visible classes and methods, an isolated matrix cell, status and ReleaseFast consequence. */
export type Filter = {
  cls: ReadonlySet<string>;
  met: ReadonlySet<string>;
  /** [method, class] of the isolated matrix cell. */
  cell: readonly [string, string] | null;
  status: string | null;
  rf: string | null;
};

/** CLASS_KEYS lists every class key, in legend order. */
export const CLASS_KEYS: readonly string[] = CLASSES.map((c) => c.key);

/** METHOD_KEYS lists every method key, in legend order. */
export const METHOD_KEYS: readonly string[] = METHODS.map((m) => m.key);

/** ALL_FILTER shows every defect. */
export const ALL_FILTER: Filter = {
  cls: new Set(CLASS_KEYS),
  met: new Set(METHOD_KEYS),
  cell: null,
  status: null,
  rf: null,
};

/**
 * Chip toggle: with every chip on, a click isolates k; otherwise it adds or
 * removes k, and removing the last one turns every chip back on.
 */
export function toggleChip(
  set: ReadonlySet<string>,
  k: string,
  all: readonly string[],
): Set<string> {
  if (set.size === all.length) return new Set([k]);
  const next = new Set(set);
  if (next.has(k)) {
    next.delete(k);
    return next.size ? next : new Set(all);
  }
  next.add(k);
  return next;
}

/** Whether defect d passes filter f. */
export function visible(d: Def, f: Filter): boolean {
  return (
    f.cls.has(d.c) &&
    f.met.has(d.m) &&
    (!f.cell || (f.cell[0] === d.m && f.cell[1] === d.c)) &&
    (!f.status || d.s === f.status) &&
    (!f.rf || d.rf === f.rf)
  );
}

/** Whether filter f hides anything (the reset button is then meaningful). */
export function isFiltered(f: Filter): boolean {
  return (
    f.cls.size !== CLASS_KEYS.length ||
    f.met.size !== METHOD_KEYS.length ||
    f.cell !== null ||
    f.status !== null ||
    f.rf !== null
  );
}

/** Counts of the class × method matrix with totals and the defect indices of each cell. */
export type Matrix = {
  get: (m: string, c: string) => number;
  items: (m: string, c: string) => number[];
  rowTotal: (m: string) => number;
  colTotal: (c: string) => number;
  total: number;
  max: number;
};

/** Builds the class × discovery-method matrix. */
export function matrix(defs: readonly Def[]): Matrix {
  const cells = new Map<string, number[]>();
  defs.forEach((d, i) => {
    const key = `${d.m}|${d.c}`;
    const l = cells.get(key);
    if (l) l.push(i);
    else cells.set(key, [i]);
  });
  const items = (m: string, c: string) => cells.get(`${m}|${c}`) ?? [];
  return {
    get: (m, c) => items(m, c).length,
    items,
    rowTotal: (m) => defs.filter((d) => d.m === m).length,
    colTotal: (c) => defs.filter((d) => d.c === c).length,
    total: defs.length,
    max: Math.max(...[...cells.values()].map((l) => l.length)),
  };
}

/** Cell background alpha (0..1) for a count: grows with the square root, as in the mockup. */
export function cellAlpha(v: number, max: number): number {
  return (25 + 200 * Math.sqrt(v / max)) / 255;
}

/** Header figures: totals, per-class counts, project split and method confidence. */
export type Headline = {
  total: number;
  live: number;
  latent: number;
  test: number;
  byClass: Record<string, number>;
  ghostty: number;
  deps: number;
  conf: { h: number; m: number; k: number };
};

/** Computes the header figures. */
export function headline(data: DmData): Headline {
  const byClass: Record<string, number> = {};
  for (const d of data.defs) byClass[d.c] = (byClass[d.c] ?? 0) + 1;
  const conf = { h: 0, m: 0, k: 0 };
  for (const d of data.defs) conf[d.conf] += 1;
  const ghostty = data.defs.filter((d) => d.p === "ghostty").length;
  return {
    total: data.defs.length,
    live: CLASSES.filter((c) => !("hollow" in c)).reduce(
      (s, c) => s + (byClass[c.key] ?? 0),
      0,
    ),
    latent: byClass.latent ?? 0,
    test: byClass.test ?? 0,
    byClass,
    ghostty,
    deps: data.defs.length - ghostty,
    conf,
  };
}

/** Indices of the crash-class defects, ordered by ReleaseFast consequence (waffle order). */
export function waffle(defs: readonly Def[]): number[] {
  return defs
    .map((d, i) => [d, i] as const)
    .filter(([d]) => d.c === "crash")
    .sort(([a], [b]) => RF_ORDER.indexOf(a.rf) - RF_ORDER.indexOf(b.rf))
    .map(([, i]) => i);
}

/** One upstream-status segment: key, count and defect indices. */
export type StatusRow = {
  key: string;
  name: string;
  role: Role;
  n: number;
  items: number[];
};

/** Upstream status counts in bar order. */
export function statusRows(defs: readonly Def[]): StatusRow[] {
  return STATUSES.map((s) => {
    const items = defs.flatMap((d, i) => (d.s === s.key ? [i] : []));
    return { ...s, n: items.length, items };
  });
}

/** Zig lines per leaf and leaf → top tile name, from the data tree. */
function leafIndex(data: DmData): Map<string, { top: string; lines: number }> {
  const m = new Map<string, { top: string; lines: number }>();
  for (const t of data.tree)
    for (const [n, v] of t.k) m.set(n, { top: t.n, lines: v });
  return m;
}

/** One density row: top tile, defects, Zig lines and defects per 10,000 lines. */
export type DensityRow = { n: string; c: number; v: number; d: number };

/** Defects per 10k Zig lines for top tiles with at least three defects, densest first. */
export function densityRows(data: DmData): DensityRow[] {
  const idx = leafIndex(data);
  const count = new Map<string, number>();
  for (const d of data.defs) {
    const top = idx.get(d.leaf)?.top;
    if (top) count.set(top, (count.get(top) ?? 0) + 1);
  }
  return data.tree
    .filter((t) => (count.get(t.n) ?? 0) >= 3)
    .map((t) => {
      const c = count.get(t.n) ?? 0;
      return { n: t.n, c, v: t.v, d: (c / t.v) * 1e4 };
    })
    .sort((a, b) => b.d - a.d);
}

/** One leaf's defects, Zig lines and per-class counts (for its tooltip). */
export function leafStats(
  data: DmData,
  leaf: string,
): { n: number; lines: number; byClass: [string, number][] } {
  const list = data.defs.filter((d) => d.leaf === leaf);
  return {
    n: list.length,
    lines: leafIndex(data).get(leaf)?.lines ?? 0,
    byClass: CLASSES.map((c): [string, number] => [
      c.key,
      list.filter((d) => d.c === c.key).length,
    ]).filter(([, k]) => k > 0),
  };
}

/** The src/terminal note: its share of Ghostty's Zig and defects, and the tmux leaf. */
export function terminalShare(data: DmData) {
  const term = data.tree.find((t) => t.n === "src/terminal");
  const leaves = new Set(term?.k.map(([n]) => n));
  const tmux = leafStats(data, "src/terminal/tmux");
  return {
    lines: term?.v ?? 0,
    total: data.tree.reduce((s, t) => s + t.v, 0),
    defects: data.defs.filter((d) => leaves.has(d.leaf)).length,
    ghostty: data.defs.filter((d) => d.p === "ghostty").length,
    tmux: tmux.n,
    tmuxLines: tmux.lines,
    tmuxDensity: (tmux.n / tmux.lines) * 1e4,
  };
}

/** A class, method, status or ReleaseFast vocabulary entry. */
type Vocab = { key: string; name: string };

/** Indexes a vocabulary list by key. */
function index<T extends Vocab>(
  list: readonly T[],
): Readonly<Record<string, T>> {
  return Object.fromEntries(list.map((v) => [v.key, v]));
}

/** CLASS_BY looks a class up by key. */
export const CLASS_BY = index<(typeof CLASSES)[number]>(CLASSES);

/** METHOD_BY looks a discovery method up by key. */
export const METHOD_BY = index<(typeof METHODS)[number]>(METHODS);

/** STATUS_BY looks an upstream status up by key. */
export const STATUS_BY = index<(typeof STATUSES)[number]>(STATUSES);

/** RF_BY looks a ReleaseFast consequence up by key. */
export const RF_BY = index<(typeof RFAST)[number]>(RFAST);

/** Whether class key c is drawn hollow (latent or test weakness). */
export function isHollow(c: string): boolean {
  return "hollow" in (CLASS_BY[c] ?? {});
}

/** Shortens a title to n characters with an ellipsis (tooltip lists). */
export function clip(t: string, n = 70): string {
  return t.length > n ? `${t.slice(0, n - 2)}…` : t;
}
