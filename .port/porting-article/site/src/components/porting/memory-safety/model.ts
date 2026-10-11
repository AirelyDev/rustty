/** Pure view-model for the memory-safety widget: grouping, bubble placement, cumulative curves, heat table. */

/** One curated port memory-safety event, as generated into memory-safety.json. */
export type MsEvent = {
  /** Date of the event (YYYY-MM-DD). */
  d: string;
  /** Commit hash, or null for a port-findings ledger reference. */
  ref: string | null;
  /** Number of ledger entries when `ref` is null. */
  entries: number | null;
  /** Detecting tool, in plain words. */
  tool: string;
  /** Bug class text. */
  bc: string;
  /** Scope text (production, test, bench, or a mixed row). */
  scope: string;
  /** Crate area text. */
  area: string;
  /** Defects in the event. */
  n: number;
  /** One-sentence summary. */
  sum: string;
  /** Detector row key. */
  lane: string;
  /** Bug class key. */
  cls: string;
  /** Crate group key. */
  crate: string;
};

/** One rule flag: the working rule a wave of bugs produced and the commit that introduced it. */
export type MsRule = { d: string; ref: string; t: string };

/** The generated data file. */
export type MsData = { events: MsEvent[]; rules: MsRule[] };

/** Which facet the swimlane rows show. */
export type Group = "lane" | "crate";

/** LANES are the detector rows, in display order, with their labels. */
export const LANES: readonly (readonly [string, string])[] = [
  ["sb", "Miri, lane (Stacked Borrows)"],
  ["har", "Miri, proof harness"],
  ["tb", "Miri, Tree Borrows"],
  ["san", "sanitizers (ASan, LSan)"],
  ["rev", "review, audits, gates"],
  ["sym", "symptom: crash, hang, stress, profiling"],
];

/** CRATES are the crate-area rows, in display order, with their long labels. */
export const CRATES: readonly (readonly [string, string])[] = [
  ["vt", "rustty-vt (terminal engine)"],
  ["vt-c", "rustty-vt-c (C API)"],
  ["xev", "rustty-xev (event loop)"],
  ["app", "core, renderer, app runtimes, embed ABI"],
  ["other", "other crates (vaxis, os, lib, bench, oniguruma)"],
];

/** CLASSES are the bug-class columns of the heat table, with long labels. */
export const CLASSES: readonly (readonly [string, string])[] = [
  ["alias", "aliasing (SB/TB)"],
  ["prov", "provenance"],
  ["own", "ownership: leak, double free, freed arg, uninit"],
  ["conc", "concurrency: race, deadlock, C init"],
  ["abi", "ABI / header layout"],
  ["sem", "Zig→Rust semantics"],
];

/** CLASS_SHORT are the heat-table column heads. */
export const CLASS_SHORT: Readonly<Record<string, string>> = {
  alias: "alias",
  prov: "prov.",
  own: "owner.",
  conc: "conc.",
  abi: "ABI",
  sem: "sem.",
};

/** CRATE_SHORT are the heat-table row heads. */
export const CRATE_SHORT: Readonly<Record<string, string>> = {
  vt: "rustty-vt",
  "vt-c": "rustty-vt-c",
  xev: "rustty-xev",
  app: "app tiers + ABI",
  other: "other",
};

/** Plot geometry in viewBox units: left and right edges of the date axis, lane height, lane top. */
export const GEO = {
  L: 150,
  R: 922,
  W: 960,
  rowH: 44,
  top: 56,
  cumH: 110,
} as const;

/** The date axis: first day (a little before the first event) and last day (after the last). */
const AXIS = { from: "2026-06-19", to: "2026-10-11" } as const;

/** Day length in milliseconds. */
const DAY = 864e5;

/** Returns the timestamp of a YYYY-MM-DD date at 12:00 UTC (so every day compares exactly). */
export function dayMs(d: string): number {
  return Date.parse(`${d}T12:00:00Z`);
}

/** Returns the x position of a date on the plot's axis. */
export function dateX(d: string): number {
  const a = dayMs(AXIS.from);
  const b = dayMs(AXIS.to);
  return GEO.L + ((dayMs(d) - a) / (b - a)) * (GEO.R - GEO.L);
}

/** Returns the MM-DD part of a date. */
export function shortDate(d: string): string {
  return d.slice(5);
}

/** Returns "prod", "bench" or "test" from the first word of a scope text (mixed rows go by their first word). */
export function scopeKind(scope: string): "prod" | "bench" | "test" {
  if (scope.startsWith("production")) return "prod";
  if (scope.startsWith("bench")) return "bench";
  return "test";
}

/** Whether an event is in production code. */
export function isProd(e: MsEvent): boolean {
  return scopeKind(e.scope) === "prod";
}

/** Whether Miri (or a Miri-proven audit) found the event. */
export function isMiri(e: MsEvent): boolean {
  return /Miri/.test(e.tool);
}

/** Returns the sum of defects over events satisfying test. */
function defects(events: MsEvent[], test: (e: MsEvent) => boolean): number {
  return events.filter(test).reduce((a, e) => a + e.n, 0);
}

/** The header figures. */
export type Headline = {
  defects: number;
  events: number;
  miri: number;
  prodEvents: number;
  prodDefects: number;
  /** Defects whose class is aliasing (protector, reborrow, raw alias, Tree Borrows). */
  alias: number;
  /** Defects whose class is pointer provenance. */
  prov: number;
  aliasOrProv: number;
  /** Aliasing or provenance defects in production code. */
  aliasOrProvProd: number;
};

/** Returns the header figures computed from the events. */
export function headline(events: MsEvent[]): Headline {
  const alias = defects(events, (e) => e.cls === "alias");
  const prov = defects(events, (e) => e.cls === "prov");
  return {
    defects: defects(events, () => true),
    events: events.length,
    miri: defects(events, isMiri),
    prodEvents: events.filter(isProd).length,
    prodDefects: defects(events, isProd),
    alias,
    prov,
    aliasOrProv: alias + prov,
    aliasOrProvProd: defects(
      events,
      (e) => isProd(e) && (e.cls === "alias" || e.cls === "prov"),
    ),
  };
}

/** Returns the rows (key, label) of a facet. */
export function rowsOf(group: Group): readonly (readonly [string, string])[] {
  return group === "lane" ? LANES : CRATES;
}

/** Returns defect totals per row key of a facet. */
export function laneTotals(
  events: MsEvent[],
  group: Group,
): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [k] of rowsOf(group)) {
    out[k] = defects(events, (e) => e[group] === k);
  }
  return out;
}

/** Returns the bubble radius for an event with n defects (area grows with n). */
export function bubbleR(n: number): number {
  return 3.5 + 3.2 * Math.sqrt(n);
}

/** One placed bubble: event index, row key, x, vertical offset within its row and radius. */
export type Placed = {
  i: number;
  row: string;
  x: number;
  dy: number;
  r: number;
};

/** OFFSETS are the vertical offsets tried, in order, to keep overlapping bubbles apart. */
const OFFSETS = [0, -11, 11, -20, 20];

/** Places every event in its row of the facet, nudging overlapping bubbles vertically. */
export function placeBubbles(events: MsEvent[], group: Group): Placed[] {
  const out: Placed[] = [];
  for (const [k] of rowsOf(group)) {
    const placed: { x: number; y: number; r: number }[] = [];
    const evs = events
      .map((e, i) => ({ e, i }))
      .filter((o) => o.e[group] === k)
      .sort((a, b) => dayMs(a.e.d) - dayMs(b.e.d));
    for (const { e, i } of evs) {
      const r = bubbleR(e.n);
      const x = dateX(e.d);
      let dy = 0;
      for (const off of OFFSETS) {
        dy = off;
        if (!placed.some((p) => Math.hypot(p.x - x, p.y - off) < p.r + r + 1)) {
          break;
        }
      }
      placed.push({ x, y: dy, r });
      out.push({ i, row: k, x, dy, r });
    }
  }
  return out;
}

/** The cumulative curves: key, label, test and colour role. */
export const SERIES_DEFS: readonly {
  key: "all" | "miri" | "prod";
  label: string;
  test: (e: MsEvent) => boolean;
}[] = [
  { key: "all", label: "all", test: () => true },
  { key: "miri", label: "found by Miri", test: isMiri },
  { key: "prod", label: "in production code", test: isProd },
];

/** Returns the days of the cumulative axis, 2026-06-21 to 2026-10-10, as YYYY-MM-DD strings. */
export function dayTicks(): string[] {
  const out: string[] = [];
  for (let t = dayMs("2026-06-21"); t <= dayMs("2026-10-10"); t += DAY) {
    out.push(new Date(t).toISOString().slice(0, 10));
  }
  return out;
}

/** One cumulative step point: the day and the running defect total at the end of it. */
export type CumPoint = { d: string; before: number; v: number };

/** Returns the running defect total (counting defects, not events) of the events passing test, one point per day. */
export function cumulative(
  events: MsEvent[],
  test: (e: MsEvent) => boolean,
  days: string[],
): CumPoint[] {
  let acc = 0;
  return days.map((d) => {
    const before = acc;
    for (const e of events) if (e.d === d && test(e)) acc += e.n;
    return { d, before, v: acc };
  });
}

/** Whether an event is selected by the heat-table cell filter (no filter selects everything). */
export function matches(
  e: Pick<MsEvent, "crate" | "cls">,
  hl: readonly [string, string] | null,
): boolean {
  return !hl || (e.crate === hl[0] && e.cls === hl[1]);
}

/** The crate x class table. */
export type Heat = {
  /** Largest cell value (the colour scale's maximum). */
  max: number;
  /** Defects and event count of one cell. */
  cell: (crate: string, cls: string) => { n: number; events: number };
  rowTotals: Record<string, number>;
  colTotals: Record<string, number>;
  total: number;
};

/** Returns the crate x class table of defects with event counts. */
export function heat(events: MsEvent[]): Heat {
  const n: Record<string, number> = {};
  const ev: Record<string, number> = {};
  let max = 0;
  for (const e of events) {
    const k = `${e.crate}|${e.cls}`;
    n[k] = (n[k] ?? 0) + e.n;
    ev[k] = (ev[k] ?? 0) + 1;
    max = Math.max(max, n[k]);
  }
  const rowTotals: Record<string, number> = {};
  const colTotals: Record<string, number> = {};
  for (const [c] of CRATES)
    rowTotals[c] = defects(events, (e) => e.crate === c);
  for (const [k] of CLASSES) colTotals[k] = defects(events, (e) => e.cls === k);
  return {
    max,
    cell: (crate, cls) => ({
      n: n[`${crate}|${cls}`] ?? 0,
      events: ev[`${crate}|${cls}`] ?? 0,
    }),
    rowTotals,
    colTotals,
    total: defects(events, () => true),
  };
}

/** Returns the heat-cell fill opacity for value v (red alpha grows with the square root of defects). */
export function heatAlpha(v: number, max: number): number {
  return Math.round((0.12 + 0.8 * Math.sqrt(v / max)) * 100) / 100;
}

/** Returns a swimlane row label split in at most two lines of about 21 characters (the second ellipsised at 22). */
export function wrapLabel(label: string): [string, string] {
  let l1 = "";
  let l2 = "";
  for (const w of label.split(" ")) {
    if (`${l1} ${w}`.trim().length <= 21 && !l2) l1 = `${l1} ${w}`.trim();
    else l2 = `${l2} ${w}`.trim();
  }
  return [l1, l2.length > 22 ? `${l2.slice(0, 21)}…` : l2];
}

/** Returns how an event's source is shown: "<hash> (commit)" or "port-findings ledger (n entries)". */
export function refText(e: MsEvent): string {
  if (e.ref) return `${e.ref} (commit)`;
  const n = e.entries ?? 1;
  return `port-findings ledger (${n} ${n === 1 ? "entry" : "entries"})`;
}
