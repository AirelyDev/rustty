/**
 * Pure view-model helpers for the seven static charts. No value imports, so
 * scripts/verify-porting-helpers.mjs can run them under Node's type stripping.
 */

/** MS_PER_DAY is the number of milliseconds in one day. */
const MS_PER_DAY = 86_400_000;

/** Returns true when every value is greater than or equal to the one before it. */
export function isNonDecreasing(xs: readonly number[]): boolean {
  return xs.every((x, i) => i === 0 || x >= xs[i - 1]);
}

/** Returns true when every value is less than or equal to the one before it. */
export function isNonIncreasing(xs: readonly number[]): boolean {
  return xs.every((x, i) => i === 0 || x <= xs[i - 1]);
}

/** Parses an ISO date ("2026-07-10") or UTC timestamp without zone ("2026-07-10T12:00") to epoch ms (UTC). */
export function utc(iso: string): number {
  return Date.parse(iso.length <= 10 ? `${iso}T00:00Z` : `${iso}Z`);
}

/** Returns the number of days (fractional) from start to iso, both read as UTC. */
export function dayOffset(iso: string, start: string): number {
  return (utc(iso) - utc(start)) / MS_PER_DAY;
}

/** Formats an ISO date or timestamp as "07-10" (month-day, as written in the article). */
export function md(iso: string): string {
  return iso.slice(5, 10);
}

/**
 * Returns an SVG path for a step-after line through pts ([x, y] pairs): each
 * value holds until the next point, and the last one runs on to xEnd.
 */
export function stepPath(
  pts: readonly [number, number][],
  xEnd?: number,
): string {
  if (pts.length === 0) return "";
  let d = `M${pts[0][0]},${pts[0][1]}`;
  for (let i = 1; i < pts.length; i++) d += `H${pts[i][0]}V${pts[i][1]}`;
  if (xEnd !== undefined) d += `H${xEnd}`;
  return d;
}

/** A dated point carrying a numeric field. */
type Dated = { d: string } & Record<string, unknown>;

/**
 * Keeps the last point of each calendar day (rows are in time order) and
 * counts how many points fell on that day in `count`.
 */
export function lastPerDay<T extends Dated>(
  points: readonly T[],
  _field: keyof T,
): (T & { count: number })[] {
  const out: (T & { count: number })[] = [];
  for (const p of points) {
    const prev = out.at(-1);
    if (prev && prev.d === p.d)
      out[out.length - 1] = { ...p, count: prev.count + 1 };
    else out.push({ ...p, count: 1 });
  }
  return out;
}

/** A label to place: its left x and its width, in px. */
export type LabelBox = { x: number; w: number };

/**
 * Greedy label placer: puts each box (in input order) on the first of `rows`
 * rows where it clears every box already there by `gap` px, or -1 when no row
 * fits (the mark keeps its tooltip, the label is dropped).
 */
export function placeLabels(
  boxes: readonly LabelBox[],
  { rows, gap }: { rows: number; gap: number },
): number[] {
  const ends: number[] = Array.from(
    { length: rows },
    () => Number.NEGATIVE_INFINITY,
  );
  return boxes.map((b) => {
    const row = ends.findIndex((end) => b.x >= end + gap);
    if (row >= 0) ends[row] = b.x + b.w;
    return row;
  });
}

/** One composition group of the check registry. */
export type CheckGroup = {
  family: string;
  tier: string;
  label: string;
  n: number;
};

/** One cell of the registry waffle: its group index and grid position. */
export type WaffleCell = { group: number; col: number; row: number };

/** Lays out one cell per registry entry, group after group, `cols` cells per row. */
export function waffle(
  groups: readonly CheckGroup[],
  cols: number,
): WaffleCell[] {
  const cells: WaffleCell[] = [];
  groups.forEach((g, group) => {
    for (let i = 0; i < g.n; i++) {
      const k = cells.length;
      cells.push({ group, col: k % cols, row: Math.floor(k / cols) });
    }
  });
  return cells;
}

/** A funnel stage as built by the data step. */
export type FunnelStage = {
  label: string;
  n: number;
  note: string;
  provenance: string;
};

/** A funnel stage with its share of the first stage (frac) and of the stage before it (ofPrev). */
export type FunnelRow = FunnelStage & { frac: number; ofPrev: number };

/** Adds each stage's share of the first stage and of the previous stage. */
export function funnelView(stages: readonly FunnelStage[]): FunnelRow[] {
  const first = stages[0]?.n ?? 1;
  return stages.map((s, i) => ({
    ...s,
    frac: s.n / first,
    ofPrev: i === 0 ? 1 : s.n / stages[i - 1].n,
  }));
}

/**
 * Stacks per-week counts (weeks × series) into streamgraph layers around a
 * centred baseline: layers[series][week] = [y0, y1], with the first layer
 * starting at −total/2 and the last ending at +total/2.
 */
export function streamLayers(weeks: readonly number[][]): [number, number][][] {
  const n = weeks[0]?.length ?? 0;
  const layers: [number, number][][] = Array.from({ length: n }, () => []);
  for (const counts of weeks) {
    let y = -counts.reduce((a, b) => a + b, 0) / 2;
    counts.forEach((c, m) => {
      layers[m].push([y, y + c]);
      y += c;
    });
  }
  return layers;
}

/** One weekly context-size row from the data step. */
export type CtxRow = {
  w: string;
  tool: string;
  calls: number;
  median: number;
  p90: number;
  max: number;
};

/** Context rows split by tool; Claude weeks with fewer than minCalls calls are flagged lowN. */
export function ctxSplit(
  rows: readonly CtxRow[],
  minCalls: number,
): { claude: (CtxRow & { lowN: boolean })[]; codex: CtxRow[] } {
  return {
    claude: rows
      .filter((r) => r.tool === "claude")
      .map((r) => ({ ...r, lowN: r.calls < minCalls })),
    codex: rows.filter((r) => r.tool === "codex"),
  };
}

/** Returns [min − pad, max + pad] of xs, rounded outward to multiples of pad. */
export function paddedExtent(
  xs: readonly number[],
  pad: number,
): [number, number] {
  const lo = Math.floor((Math.min(...xs) - pad) / pad) * pad;
  const hi = Math.ceil((Math.max(...xs) + pad) / pad) * pad;
  return [lo, hi];
}

/** A fact record as written to facts.json. */
export type FactRecord = {
  display: string;
  provenance: string;
  source: string;
  asOf: string;
  /** Source link of the fact (optional here so model tests can omit it). */
  href?: string;
  hrefLabel?: string;
};

/** A stats-card tile as built by the data step: fact ids plus plain labels. */
export type TileSpec = {
  key: string;
  value: string;
  label: string;
  compare?: string;
  compareLabel?: string;
  extra?: string;
  extraLabel?: string;
  note?: string;
};

/** A resolved fact: its display text and where it comes from. */
export type ResolvedFact = FactRecord & { id: string };

/** One piece of a tile note: plain text, or a fact named by a {fact_id} placeholder. */
export type NotePart = string | ResolvedFact;

/** NOTE_FACT matches a {fact_id} placeholder in a tile note. */
const NOTE_FACT = /\{([a-z0-9_]+)\}/g;

/** Splits a tile note into text and resolved facts, throwing on an unknown placeholder id. */
export function noteParts(
  note: string,
  get: (id: string) => ResolvedFact,
): NotePart[] {
  const parts: NotePart[] = [];
  let at = 0;
  for (const m of note.matchAll(NOTE_FACT)) {
    const i = m.index ?? 0;
    if (i > at) parts.push(note.slice(at, i));
    parts.push(get(m[1]));
    at = i + m[0].length;
  }
  if (at < note.length) parts.push(note.slice(at));
  return parts;
}

/** A stats-card tile with every fact id resolved to its record. */
export type Tile = {
  key: string;
  label: string;
  note?: NotePart[];
  value: string;
  main: ResolvedFact;
  compare: (ResolvedFact & { label: string }) | null;
  extra: (ResolvedFact & { label: string }) | null;
};

/** Resolves each tile's fact ids against facts.json, throwing on an unknown id. */
export function resolveTiles(
  tiles: readonly TileSpec[],
  facts: Record<string, FactRecord>,
): Tile[] {
  /** Looks up one fact id, throwing when it is missing. */
  const get = (id: string): ResolvedFact => {
    const f = facts[id];
    if (!f) throw new Error(`stats-card: unknown fact id ${id}`);
    return { ...f, id };
  };
  return tiles.map((t) => {
    const main = get(t.value);
    return {
      key: t.key,
      label: t.label,
      note: t.note ? noteParts(t.note, get) : undefined,
      value: main.display,
      main,
      compare: t.compare
        ? { ...get(t.compare), label: t.compareLabel ?? "" }
        : null,
      extra: t.extra ? { ...get(t.extra), label: t.extraLabel ?? "" } : null,
    };
  });
}

/** A platform-table cell status. */
export type Status = "yes" | "partial" | "no" | "na";

/** STATUS_MARKS maps each status to its glyph and plain word (colour is never the only cue). */
const STATUS_MARKS: Record<Status, { glyph: string; text: string }> = {
  yes: { glyph: "●", text: "yes" },
  partial: { glyph: "◐", text: "partly" },
  no: { glyph: "○", text: "no" },
  na: { glyph: "–", text: "not applicable" },
};

/** Returns the glyph and plain word for a platform-table status. */
export function statusMark(status: string): { glyph: string; text: string } {
  return STATUS_MARKS[status as Status] ?? STATUS_MARKS.na;
}

/**
 * Returns a smooth SVG path segment list through pts ([x, y] pairs): cubic
 * curves with horizontal tangents at every point, so a streamgraph edge never
 * overshoots its data. With move = false it starts with "L" (to continue an
 * existing path) instead of "M".
 */
export function curveThrough(
  pts: readonly [number, number][],
  move = true,
): string {
  if (pts.length === 0) return "";
  let d = `${move ? "M" : "L"}${pts[0][0]},${pts[0][1]}`;
  for (let i = 1; i < pts.length; i++) {
    const [x0, y0] = pts[i - 1];
    const [x1, y1] = pts[i];
    const mx = (x0 + x1) / 2;
    d += `C${mx},${y0} ${mx},${y1} ${x1},${y1}`;
  }
  return d;
}
