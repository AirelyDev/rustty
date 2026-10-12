/** Pure view-model for the "What it cost" widget: number formats, the quota-week bars, the limit-lane label placement and the paid-vs-list-price rows. Its imports are type-only so Node can run it directly. */

import type { CostData, CovClass, LimitEvent, Scope, Unit } from "./types";

export type {
  Scope,
  Unit,
  WeekSeries,
  QuotaWeek,
  CovDay,
  CovClass,
  LimitEvent,
  MixPart,
  KindRow,
  Era,
  CostData,
} from "./types";

/** CATS are the recovered-usage categories, in stacking order. */
export const CATS = [
  "Opus 4.8",
  "Opus 5",
  "Opus 5.5",
  "Fable 5 / 5.1",
  "Sonnet 5 / 5.5",
  "Opus 4.7",
  "Other Claude",
  "VM Claude",
  "Codex",
] as const;

/** CAT_COLOR is each category's fill (article palette). */
export const CAT_COLOR: Record<string, string> = {
  "Opus 4.8": "#ffbc7a",
  "Opus 5": "#d19a66",
  "Opus 5.5": "#d97934",
  "Fable 5 / 5.1": "#c678dd",
  "Sonnet 5 / 5.5": "#e6c07b",
  "Opus 4.7": "#9aa09a",
  "Other Claude": "#4a4f4b",
  "VM Claude": "#98c379",
  Codex: "#61aeee",
};

/** CAT_NAME spells out the categories whose short key needs it. */
export const CAT_NAME: Record<string, string> = {
  "Opus 4.7": "Opus 4.7 (mostly the automated security reviewer)",
  "VM Claude": "Linux VM Claude (all models)",
  Codex: "Codex (GPT models)",
};

/** TRAILER_COLOR is the fill of each Co-Authored-By model in the trailer strip. */
export const TRAILER_COLOR: Record<string, string> = {
  "Opus 4.8": "#ffbc7a",
  "Opus 5": "#d19a66",
  "Opus 5.5": "#d97934",
  "Fable 5": "#c678dd",
  "Fable 5+Opus 4.8": "#c678dd",
  "Sonnet 5.5": "#e6c07b",
  none: "#4a4f4b",
};

/** WEEK_MS is one quota week in milliseconds. */
export const WEEK_MS = 7 * 864e5;
/** Q0 is the start of the first quota week (2026-06-16 22:00 UTC). */
export const Q0 = Date.UTC(2026, 5, 16, 22);

/** Parses "YYYY-MM-DD", "YYYY-MM-DDTHH" or "YYYY-MM-DDTHH:MM" as UTC milliseconds. */
export function utc(s: string): number {
  if (s.length === 10) return Date.parse(`${s}T00:00Z`);
  if (s.length === 13) return Date.parse(`${s}:00Z`);
  return Date.parse(`${s}Z`);
}

/** Formats list-price dollars as the mockup does: "$11.0k", "$2,769", "$46", "$0.95". */
export function fmtMoney(v: number): string {
  if (v >= 10000) return `$${(v / 1000).toFixed(1)}k`;
  if (v >= 1000) return `$${Math.round(v).toLocaleString("en-US")}`;
  if (v >= 10) return `$${Math.round(v)}`;
  return `$${v.toFixed(2)}`;
}

/** Formats a token count: "4.67B", "9.9M", "77k". */
export function fmtTok(v: number): string {
  if (v >= 1e9) return `${(v / 1e9).toFixed(2)}B`;
  if (v >= 1e6) return `${(v / 1e6).toFixed(1)}M`;
  if (v >= 1e3) return `${Math.round(v / 1e3)}k`;
  return String(Math.round(v));
}

/** Formats a value in the current unit. */
export function fmtVal(v: number, u: Unit): string {
  return u === "usd" ? fmtMoney(v) : fmtTok(v);
}

/** Formats a 0–1 share as a percentage with d decimals. */
export function pct(v: number, d = 1): string {
  return `${(v * 100).toFixed(d)}%`;
}

/** Rounds m up to a "nice" axis maximum (1, 2, 2.5, 5 or 10 × a power of ten). */
export function niceCeil(m: number): number {
  const p = 10 ** Math.floor(Math.log10(m));
  for (const k of [1, 2, 2.5, 5, 10]) if (k * p >= m) return k * p;
  return 10 * p;
}

/** One quota-week bar: stacked recovered categories, the modeled block and its whisker. */
export type WeekBar = {
  i: number;
  /** Start of the week, UTC ms. */
  start: number;
  /** Recovered value per category, in stacking order, non-zero only. */
  rec: [string, number][];
  recSum: number;
  /** Modeled lost-window value [low, central, high]. */
  ext: [number, number, number];
  /** Recovered + modeled central. */
  total: number;
  /** Highest drawn value (recovered + modeled high). */
  top: number;
};

/** Returns the 17 quota-week bars for a scope and unit. */
export function weekBars(d: CostData, sc: Scope, u: Unit): WeekBar[] {
  return d.qw.map((q, i) => {
    const [r, ext] = q[sc][u];
    const rec = CATS.filter((c) => (r[c] ?? 0) > 0).map(
      (c) => [c, r[c]] as [string, number],
    );
    const recSum = rec.reduce((a, [, v]) => a + v, 0);
    return {
      i,
      start: Q0 + i * WEEK_MS,
      rec,
      recSum,
      ext,
      total: recSum + ext[1],
      top: recSum + ext[2],
    };
  });
}

/** Returns the recovered sum and the modeled [low, central, high] sums over all quota weeks. */
export function weekTotals(
  d: CostData,
  sc: Scope,
  u: Unit,
): { rec: number; ext: [number, number, number] } {
  const bars = weekBars(d, sc, u);
  const ext = [0, 1, 2].map((k) => bars.reduce((a, b) => a + b.ext[k], 0));
  return {
    rec: bars.reduce((a, b) => a + b.recSum, 0),
    ext: ext as [number, number, number],
  };
}

/** Returns the maxed-week ceiling band in the current scope (halved for the port's 50% share). */
export function ceiling(
  d: CostData,
  sc: Scope,
  u: Unit,
): [number, number, number] {
  const half = sc === "rustty" ? 0.5 : 1;
  return d.ceil[u].map((v) => v * half) as [number, number, number];
}

/** Returns the y-axis maximum: the highest bar or band top, plus 4%, rounded up to a nice value. */
export function yTop(d: CostData, sc: Scope, u: Unit): number {
  let mx = ceiling(d, sc, u)[2];
  for (const b of weekBars(d, sc, u)) mx = Math.max(mx, b.top);
  return niceCeil(mx * 1.04);
}

/** Splits recovered Rustty $ into Mac Claude, Codex and Linux VM Claude. */
export function recoveredParts(d: CostData): {
  mac: number;
  codex: number;
  vm: number;
} {
  let mac = 0;
  let codex = 0;
  let vm = 0;
  for (const q of d.qw) {
    for (const [c, v] of Object.entries(q.rustty.usd[0])) {
      if (c === "Codex") codex += v;
      else if (c === "VM Claude") vm += v;
      else mac += v;
    }
  }
  return { mac, codex, vm };
}

/** Returns the label text of a limit event: the quoted fragment, or the message and its thread count. */
export function eventLabel(e: LimitEvent): string {
  if (e.k === "prompt") return `“${e.s}”`;
  return e.s + (e.n && e.n > 10 ? ` · ${e.n} threads` : "");
}

/** A placed label of the limit lane: its row (0–3), text and horizontal extent. */
export type PlacedLabel = {
  row: number;
  label: string;
  x0: number;
  x1: number;
  /** True when the label is drawn to the left of its marker (it would overflow the right edge). */
  end: boolean;
};

/** LABEL_CHAR is the estimated width of one label character in the 9.5px monospace font. */
const LABEL_CHAR = 5.8;

/** Placement priority: weekly limits first, then session/credits, Rustty prompts, other prompts. */
function priority(e: LimitEvent): number {
  if (e.k === "weekly") return 0;
  if (e.k === "session" || e.k === "usage_credits_exhausted") return 1;
  if (e.k === "prompt") return e.r ? 2 : 4;
  return 5;
}

/** A label to place: index, anchor x and text, in placement order. */
export type LabelItem = { i: number; x: number; label: string };

/** Options of the greedy label placer: rows, character width, padding, right edge and gap. */
export type LabelOpts = {
  rows: number;
  charW: number;
  pad: number;
  width: number;
  gap: number;
};

/**
 * Places labels greedily (mockup algorithm): in the given order, each label
 * goes in the first row where it clears every placed label by `gap`; a label
 * that would overflow the right edge is drawn to the left of its anchor.
 * Returns placements by item index; items that fit no row are left out.
 */
export function placeLabels(
  items: LabelItem[],
  o: LabelOpts,
): Map<number, PlacedLabel> {
  const rows: [number, number][][] = Array.from({ length: o.rows }, () => []);
  const out = new Map<number, PlacedLabel>();
  for (const { i, x, label } of items) {
    const w = label.length * o.charW + o.pad;
    const end = x + w > o.width - 4;
    const x0 = end ? x - w : x;
    const row = rows.findIndex((r) =>
      r.every(([p, q]) => x0 + w < p - o.gap || x0 > q + o.gap),
    );
    if (row < 0) continue;
    rows[row].push([x0, x0 + w]);
    out.set(i, { row, label, x0, x1: x0 + w, end });
  }
  return out;
}

/**
 * Places the limit-lane labels in four rows: by priority then time, slash
 * commands unlabelled. Unplaced events keep only their marker and tooltip.
 */
export function placeEventLabels(
  evs: LimitEvent[],
  xOf: (t: string) => number,
  width: number,
): Map<number, PlacedLabel> {
  const items = evs
    .map((e, i) => ({ e, i, x: xOf(e.t) }))
    .filter(({ e }) => e.k !== "slash")
    .sort((a, b) => priority(a.e) - priority(b.e) || a.x - b.x)
    .map(({ e, i, x }) => ({ i, x, label: eventLabel(e) }));
  return placeLabels(items, {
    rows: 4,
    charW: LABEL_CHAR,
    pad: 4,
    width,
    gap: 4,
  });
}

/** A row of the paid-vs-list-price chart. */
export type PaidRow = {
  name: string;
  sub: string;
  v: number;
  /** [low, high] when the row has a range. */
  range: [number, number] | null;
  color: string;
  kind: "pt" | "bar" | "hatch";
  /** How the value is computed (tooltip). */
  how: string;
};

/** Returns the paid and list-price rows: the two subscription shares, the paid range, then the API-equivalent parts and total. */
export function paidRows(d: CostData): PaidRow[] {
  const p = d.paid;
  const parts = recoveredParts(d);
  const months = `${p.months} months (${p.days} days)`;
  return [
    {
      name: "Claude Max 20x",
      sub: "port's half, prorated",
      v: p.claude,
      range: null,
      color: "#d97934",
      kind: "pt",
      how: `$${p.price} × ${months} × 50% = ${fmtMoney(p.claude)}`,
    },
    {
      name: "ChatGPT Pro month",
      sub: `port's ${pct(p.codexShare)}`,
      v: p.codex,
      range: null,
      color: "#61aeee",
      kind: "pt",
      how: `$${p.price} × ${pct(p.codexShare)} (the port's ${fmtMoney(p.codexMonth.rustty)} of the month's ${fmtMoney(p.codexMonth.rustty + p.codexMonth.other)} Codex usage) = ${fmtMoney(p.codex)}`,
    },
    {
      name: "Paid, attributable",
      sub: "central and range",
      v: p.central,
      range: [p.central, p.high],
      color: "#d97934",
      kind: "bar",
      how: `Central ${fmtMoney(p.central)} (= ${fmtMoney(p.claude)} + ${fmtMoney(p.codex)}). Upper end ~${fmtMoney(p.high)} if the plan renews on the 28th: ${p.cycles} Max billing cycles touched × $${p.price} × 50%, plus Codex at 50%. Extra-usage credits unknown, not included.`,
    },
    {
      name: "Recovered, measured",
      sub: "list price, lower bound",
      v: d.api.recovered,
      range: null,
      color: "#9aa09a",
      kind: "pt",
      how: `Mac Claude ${fmtMoney(parts.mac)} + Codex ${fmtMoney(parts.codex)} + Linux VM Claude ${fmtMoney(parts.vm)} = $${Math.round(d.api.recovered).toLocaleString("en-US")}, at API list price`,
    },
    {
      name: "Lost window, modeled",
      sub: "maxed week × 50%",
      v: d.api.modeled[1],
      range: [d.api.modeled[0], d.api.modeled[2]],
      color: "#d97934",
      kind: "hatch",
      how: `Every lost week with port activity, at ${d.ceil.usd.map(fmtMoney).join(" / ")} per maxed week × 50%, minus Claude usage already recovered in that week. At API list price.`,
    },
    {
      name: "API-equivalent total",
      sub: "reference value, not money spent",
      v: d.api.total[1],
      range: [d.api.total[0], d.api.total[2]],
      color: "#ffbc7a",
      kind: "bar",
      how: `${fmtMoney(d.api.recovered)} recovered + ${fmtMoney(d.api.modeled[1])} modeled = ${fmtMoney(d.api.total[1])} (range ${fmtMoney(d.api.total[0])}–${fmtMoney(d.api.total[2])}). What the same tokens would have cost pay-as-you-go; not money spent.`,
    },
  ];
}

/** Leverage: API-equivalent per subscription dollar, central and consistent bounds (total low ÷ five cycles at 50%, total high ÷ central paid), and the low end at the top of the paid range. */
export function leverage(d: CostData): {
  central: number;
  low: number;
  high: number;
  lowAtTop: number;
} {
  const p = d.paid;
  const [lo, mid, hi] = d.api.total;
  return {
    central: mid / p.central,
    low: lo / (p.cycles * p.price * 0.5),
    high: hi / p.central,
    lowAtTop: lo / p.high,
  };
}

/** A maxed quota week against the plan's weekly price: the price per week, the central multiple and the low–high multiples. */
export function maxedWeek(d: CostData): {
  perWeek: number;
  central: number;
  range: [number, number];
} {
  const perWeek = (d.paid.price * 12) / 52;
  const [lo, mid, hi] = d.ceil.usd;
  return {
    perWeek,
    central: mid / perWeek,
    range: [lo / perWeek, hi / perWeek],
  };
}

/** Counts days per coverage class. */
export function coverageCounts(d: CostData): Record<CovClass, number> {
  const out: Record<CovClass, number> = {
    rec: 0,
    remnant: 0,
    vmcodex: 0,
    prompt: 0,
    none: 0,
  };
  for (const c of d.cov) out[c[1]] += 1;
  return out;
}
