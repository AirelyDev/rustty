/**
 * Pure view-model for the "Translation lens: Zig ↔ Rust" widget: line
 * mapping groups and their symmetric lookups, line classes, the map-gutter
 * ribbons, and the evidence and side-panel series.
 */

import type {
  CodeLine,
  Harness,
  ModuleRow,
  Pattern,
  PerfStep,
  Sub,
  TlData,
  UnsafeCrate,
} from "./types";

export type {
  CodeLine,
  Harness,
  LedgerClass,
  LedgerRow,
  ModuleRow,
  Pattern,
  PerfStep,
  Sub,
  TlData,
  UnsafeCrate,
} from "./types";

/** A pane side: Zig ("z") or Rust ("r"). */
export type Side = "z" | "r";

/** Kind of a mapping group: hand-drawn code mapping, carried comment, or Zig line kept as a comment. */
export type GroupKind = "code" | "carried" | "echo";

/** One mapping group: the Zig lines and the Rust lines it links. */
export type Group = { z: number[]; r: number[]; k: GroupKind };

/** PORT_NOTE_RE marks a Rust line as a port note. */
const PORT_NOTE_RE = /PORT (NOTE|FINDING)|PERF\(port\)/;

/** Returns whether a source line is a whole-line comment. */
export function isComment(text: string): boolean {
  return /^\s*\/\//.test(text);
}

/** Returns every mapping group of an excerpt pair: code mappings, then carried comments, then echoes. */
export function groups(sub: Sub): Group[] {
  return [
    ...sub.map.map(([z, r]): Group => ({ z, r, k: "code" })),
    ...sub.carried.map(([z, r]): Group => ({ z: [z], r: [r], k: "carried" })),
    ...sub.echoes.map(([z, r]): Group => ({ z: [z], r: [r], k: "echo" })),
  ];
}

/** Returns the sorted unique numbers of xs. */
function uniqSorted(xs: Iterable<number>): number[] {
  return [...new Set(xs)].sort((a, b) => a - b);
}

/**
 * Returns the lines on the other side linked to line n of side by any
 * mapping group (sorted). Symmetric: every returned line maps back to n.
 */
export function mappedLines(sub: Sub, side: Side, n: number): number[] {
  const other: number[] = [];
  for (const g of groups(sub)) {
    if ((side === "z" ? g.z : g.r).includes(n)) {
      other.push(...(side === "z" ? g.r : g.z));
    }
  }
  return uniqSorted(other);
}

/** Lines to highlight for a hovered line: both sides, plus the indices of the groups hit. */
export type Linked = { z: number[]; r: number[]; groups: number[] };

/** Returns everything linked to line n of side: all lines of every group containing it (n included). */
export function linkedLines(sub: Sub, side: Side, n: number): Linked {
  const z: number[] = side === "z" ? [n] : [];
  const r: number[] = side === "r" ? [n] : [];
  const hit: number[] = [];
  groups(sub).forEach((g, i) => {
    if ((side === "z" ? g.z : g.r).includes(n)) {
      hit.push(i);
      z.push(...g.z);
      r.push(...g.r);
    }
  });
  return { z: uniqSorted(z), r: uniqSorted(r), groups: hit };
}

/** Line class shown in a pane (empty for plain lines). */
export type LineKind = "carried" | "echo" | "note" | "only" | "";

/**
 * Returns the class of every line on both sides. Zig: carried comments.
 * Rust: carried comment, echo of a Zig code line, port note (and any other
 * Rust-only comment), or Rust-only code (non-blank, unmapped).
 */
export function lineKinds(sub: Sub): {
  z: Map<number, LineKind>;
  r: Map<number, LineKind>;
} {
  const mappedR = new Set<number>();
  const carriedR = new Set(sub.carried.map((c) => c[1]));
  const echoR = new Set(sub.echoes.map((c) => c[1]));
  for (const g of groups(sub)) for (const n of g.r) mappedR.add(n);
  const z = new Map<number, LineKind>();
  for (const [n] of sub.carried) z.set(n, "carried");
  const r = new Map<number, LineKind>();
  for (const line of sub.rust) {
    if (!line) continue;
    const [n, t] = line;
    let k: LineKind = "";
    if (carriedR.has(n)) k = "carried";
    else if (echoR.has(n)) k = "echo";
    else if (PORT_NOTE_RE.test(t) || isComment(t)) k = "note";
    else if (!mappedR.has(n) && t.trim()) k = "only";
    if (k) r.set(n, k);
  }
  return { z, r };
}

/** Returns the excerpt's line ranges, e.g. "1749–1796, 1842–1849". */
export function segText(lines: CodeLine[]): string {
  const segs: string[] = [];
  let a: number | null = null;
  let b = 0;
  for (const l of lines) {
    if (!l) {
      if (a !== null) segs.push(`${a}–${b}`);
      a = null;
      continue;
    }
    if (a === null) a = l[0];
    b = l[0];
  }
  if (a !== null) segs.push(`${a}–${b}`);
  return segs.join(", ");
}

/** Returns line numbers as compact ranges: [1749, 1763, 1764, 1765] → "1749, 1763–1765". */
export function rangeText(ns: number[]): string {
  const out: string[] = [];
  let st: number | null = null;
  let p = 0;
  for (const v of uniqSorted(ns)) {
    if (st === null) st = p = v;
    else if (v === p + 1) p = v;
    else {
      out.push(st === p ? `${st}` : `${st}–${p}`);
      st = p = v;
    }
  }
  if (st !== null) out.push(st === p ? `${st}` : `${st}–${p}`);
  return out.join(", ");
}

/** Returns a map from line number to its row index in the pane (gaps take a row). */
export function rowIndex(lines: CodeLine[]): Map<number, number> {
  const m = new Map<number, number>();
  lines.forEach((l, i) => {
    if (l) m.set(l[0], i);
  });
  return m;
}

/** Returns the contiguous row-index runs [first, last] of the given line numbers. */
function runs(ns: number[], ix: Map<number, number>): [number, number][] {
  const a = uniqSorted(
    ns.map((n) => ix.get(n)).filter((v): v is number => v !== undefined),
  );
  const out: [number, number][] = [];
  for (const v of a) {
    const last = out[out.length - 1];
    if (last && v === last[1] + 1) last[1] = v;
    else out.push([v, v]);
  }
  return out;
}

/** MAP_W is the map gutter width in px; its two rails are 5 px wide. */
export const MAP_W = 64;

/** One ribbon of the map gutter: the group it draws and its SVG path. */
export type Ribbon = { g: number; k: GroupKind; d: string };

/**
 * Returns the map-gutter ribbons for an excerpt pair in a gutter of height
 * h: every run of a group's Zig rows joined to every run of its Rust rows,
 * at their proportional positions in each pane.
 */
export function ribbons(sub: Sub, h: number): Ribbon[] {
  const zi = rowIndex(sub.zig);
  const ri = rowIndex(sub.rust);
  /** Y of Zig row i. */
  const zy = (i: number) => 4 + (i / sub.zig.length) * (h - 8);
  /** Y of Rust row i. */
  const ry = (i: number) => 4 + (i / sub.rust.length) * (h - 8);
  const out: Ribbon[] = [];
  const x1 = 5;
  const x2 = MAP_W - 5;
  const mid = MAP_W / 2;
  groups(sub).forEach((g, gi) => {
    for (const a of runs(g.z, zi)) {
      for (const b of runs(g.r, ri)) {
        const y1 = zy(a[0]);
        const y2 = Math.max(zy(a[1] + 1), y1 + 1.2);
        const y3 = ry(b[0]);
        const y4 = Math.max(ry(b[1] + 1), y3 + 1.2);
        const f = (v: number) => v.toFixed(1);
        out.push({
          g: gi,
          k: g.k,
          d: `M${x1} ${f(y1)} C${mid} ${f(y1)} ${mid} ${f(y3)} ${x2} ${f(y3)} L${x2} ${f(y4)} C${mid} ${f(y4)} ${mid} ${f(y2)} ${x1} ${f(y2)} Z`,
        });
      }
    }
  });
  return out;
}

/** Returns [Zig total, Rust total] of module metric m (0 code, 1 comments, 2 tests). */
export function moduleTotals(
  mods: ModuleRow[],
  m: 0 | 1 | 2,
): [number, number] {
  let z = 0;
  let r = 0;
  for (const row of mods) {
    z += row.zig[m];
    r += row.rust[m];
  }
  return [z, r];
}

/** The crates where a pattern lives: the top 6 by marker count, and all counts. */
export type PatternCrates = { top: string[]; all: Record<string, number> };

/** Returns the pattern's crates with any marker count; the top 6 are highlighted in the side panels. */
export function patternCrates(p: Pattern): PatternCrates {
  const all = p.counts.crates;
  return {
    top: Object.keys(all)
      .filter((k) => all[k] > 0)
      .slice(0, 6),
    all,
  };
}

/** One row of the module dumbbell. */
export type ModuleDot = {
  row: ModuleRow;
  z: number;
  r: number;
  on: boolean;
};

/**
 * Returns the module rows for metric m: modules with lines on both sides,
 * sorted by Zig code lines; on marks modules holding the pattern's top crates.
 */
export function moduleDots(
  mods: ModuleRow[],
  m: 0 | 1 | 2,
  pc: PatternCrates,
): ModuleDot[] {
  const top = new Set(pc.top);
  return mods
    .filter((row) => row.zig[m] > 0 && row.rust[m] > 0)
    .sort((a, b) => b.zig[0] - a.zig[0])
    .map((row) => ({
      row,
      z: row.zig[m],
      r: row.rust[m],
      on: row.crates.some((c) => top.has(c)),
    }));
}

/** unsafe metric of the crate panel: non-test per kLOC, all per kLOC, or total sites. */
export type UnsafeMetric = "nt" | "all" | "sites";

/** Returns the value of crate c for metric m. */
export function unsafeValue(c: UnsafeCrate, m: UnsafeMetric): number {
  return m === "nt" ? c.ntPerKloc : m === "all" ? c.perKloc : c.sites;
}

/** Returns the crates sorted by metric m, largest first. */
export function unsafeRows(
  crates: UnsafeCrate[],
  m: UnsafeMetric,
): UnsafeCrate[] {
  return [...crates].sort((a, b) => unsafeValue(b, m) - unsafeValue(a, m));
}

/** Returns the UB and clean verdict counts over every harness row (both models). */
export function harnessTally(hs: Harness[]): { ub: number; clean: number } {
  let ub = 0;
  let clean = 0;
  for (const h of hs) {
    for (const row of h.rows) {
      for (const v of row.slice(1)) {
        if (v === "CLEAN") clean++;
        else ub++;
      }
    }
  }
  return { ub, clean };
}

/** One bench's fix chain: unit, largest value and its steps in order. */
export type PerfGroup = {
  bench: string;
  unit: "%" | "×";
  max: number;
  steps: PerfStep[];
};

/** Returns the perf fix chains grouped by bench, in data order. */
export function perfGroups(steps: PerfStep[]): PerfGroup[] {
  const out: PerfGroup[] = [];
  for (const s of steps) {
    let g = out.find((x) => x.bench === s.bench);
    if (!g) {
      g = {
        bench: s.bench,
        unit: s.metric === "rust_over_zig_x" ? "×" : "%",
        max: 0,
        steps: [],
      };
      out.push(g);
    }
    g.steps.push(s);
    g.max = Math.max(g.max, Math.abs(s.value));
  }
  return out;
}

/** Returns the Zig line declaring field name (`name:` at line start), or null. */
export function zigFieldLine(sub: Sub, name: string): number | null {
  const re = new RegExp(`^\\s*${name}:`);
  for (const l of sub.zig) if (l && re.test(l[1])) return l[0];
  return null;
}

/** Returns the count item of a pattern whose label starts with prefix (primary included). */
function item(p: Pattern, prefix: string) {
  const it = [p.counts.primary, ...p.counts.items].find((i) =>
    i.label.startsWith(prefix),
  );
  if (!it) throw new Error(`translation-lens: no count "${prefix}" in ${p.id}`);
  return it;
}

/** INT formats integers with thousands separators. */
const INT = new Intl.NumberFormat("en-US");

/** One insight tile: the figure and its explanation. */
export type Insight = { value: string; text: string };

/** Returns the six computed insight tiles. */
export function insights(d: TlData): Insight[] {
  const P = Object.fromEntries(d.patterns.map((p) => [p.id, p]));
  const cats = Object.fromEntries(d.unsafeHeur.cats);
  const tot = d.unsafeHeur.total;
  const raw = cats["raw-pointer deref / arithmetic"];
  const ffi = cats["FFI / foreign call"];
  const [zc, rc] = moduleTotals(d.modules, 0);
  const [zcm, rcm] = moduleTotals(d.modules, 1);
  const [zt, rt] = moduleTotals(d.modules, 2);
  let car = 0;
  let zcom = 0;
  let ex = 0;
  let rn = 0;
  let zn = 0;
  let pn = 0;
  for (const p of d.patterns) {
    for (const s of p.subs) {
      car += s.stats.carried;
      zcom += s.stats.zig_comments;
      ex += s.stats.echoes;
      rn += s.stats.rust_code + s.stats.rust_comment;
      zn += s.stats.zig_code + s.stats.zig_comment;
      pn += s.stats.port_note_lines;
    }
  }
  const fp = item(P.intrusive, "@fieldParentPtr in Zig");
  const fr = P.intrusive.counts.primary.rust;
  const ofs = item(P.intrusive, "offset_of!").rust ?? 0;
  const ct = item(P.comptime, "comptime parameters").zig ?? 0;
  const cg = P.comptime.counts.primary.rust;
  const ia = item(P.comptime, "#[inline(always)]").rust ?? 0;
  const ed = item(P.errdefer, "errdefer in Zig").zig ?? 0;
  const encode = perfGroups(P.comptime.perf ?? [])[0];
  const chain = encode.steps.map((s) => `+${s.value}%`).join(" → ");
  const split = fp.zig_split ?? [0, 0];
  const n = (v: number) => INT.format(v);
  return [
    {
      value: `${Math.round((100 * raw) / tot)}%`,
      text: `of same-line unsafe blocks outside tests dereference or offset a raw pointer (${n(raw)} of ${n(tot)}), against ${Math.round((100 * ffi) / tot)}% that call foreign code (${n(ffi)}). Taking raw-pointer arguments where Zig passes *Page or *Row is the most visible source: the same shape gives ${n(P.rawptr.counts.primary.rust)} functions an explicit lint allowance that cites the Zig signature. Same-line heuristic.`,
    },
    {
      value: `${n(fp.zig ?? 0)} → ${n(fr)}`,
      text: `@fieldParentPtr calls in the Zig scope (${split[0]} in Ghostty, ${split[1]} in libxev) against Rust lines that keep the Zig recovery as a comment. The arithmetic is offset_of!, which appears ${n(ofs)} times, though GPU vertex layouts use it too. In the C API terminal wrapper, the one recovery helper has ${d.helperCallSites} call sites on each side.`,
    },
    {
      value: `${(rc / zc).toFixed(2)}×`,
      text: `Rust code lines per Zig code line across the ${d.modules.length} module groups (${n(zc)} → ${n(rc)}). Comments grew more: ${(rcm / zcm).toFixed(2)}× (${n(zcm)} → ${n(rcm)}), because Zig comments are carried and port notes are added on top. Tests in the same module count went ${n(zt)} → ${n(rt)} (${(rt / zt).toFixed(2)}×).`,
    },
    {
      value: `${car} / ${zcom}`,
      text: `Zig comment lines in these seven excerpts that reappear word for word in Rust. Another ${ex} Zig code lines survive as Rust comments next to their translation, and ${pn} Rust lines are port notes explaining a divergence. Excerpt size: ${zn} Zig lines → ${rn} Rust lines (${(rn / zn).toFixed(2)}×).`,
    },
    {
      value: `${n(ct)} → ${n(cg)}`,
      text: `comptime parameters in Zig signatures against const-generic parameters in Rust. Most comptime parameters become ordinary generics or plain values. The ones that set a store width or pick a code path became const generics after measurement. On snapshot encode, the gap to Zig went ${chain} over three fixes. Rust also has ${n(ia)} #[inline(always)] attributes, because link-time optimisation is off.`,
    },
    {
      value: n(ed),
      text: `errdefer statements in the Zig scope. Few became guard types: only ${n(item(P.errdefer, "named").rust ?? 0)} named *Errdefer guards and ${n(item(P.errdefer, "Rust guards created armed").rust ?? 0)} armed flags. Most became cleanup written out in the error arm, with the Zig line kept beside it (${n(item(P.errdefer, "Rust comment lines").rust ?? 0)} comment lines cite errdefer).`,
    },
  ];
}

/** Returns the occurrence count shown for an item and the side it belongs to. */
export function countOf(it: { rust?: number; zig?: number; ledger?: number }): {
  side: "rust" | "zig" | "ledger";
  v: number;
} {
  if (it.rust !== undefined) return { side: "rust", v: it.rust };
  if (it.zig !== undefined) return { side: "zig", v: it.zig };
  return { side: "ledger", v: it.ledger ?? 0 };
}

/** Returns a sqrt-scaled bar width in percent of v against the largest value mx (at least 1.5%). */
export function sqrtWidth(v: number, mx: number): number {
  return Math.max(1.5, 100 * Math.sqrt(v / mx));
}
