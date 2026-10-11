/**
 * Pure view-model for the "Three reviewers" widget: the borrow-stack replay,
 * the rail geometry and lane totals, the computed insights, code and terminal
 * highlighting, and the small text helpers (inline code, commit links).
 */
import type {
  Bug,
  Lane,
  Model,
  RailEvent,
  Seg,
  StackItem,
  Step,
  TrData,
} from "./types";

/** Why a tag is flagged invalid at the UB step. */
export type InvalidReason = "protected" | "popped" | "state";

/** One tag flagged invalid at a step, with the reason the replay found. */
export type Invalid = {
  id: string;
  /** protected: the access would pop a protected tag; popped: the tag was popped at step `since`; state: its tree state forbids the access. */
  reason: InvalidReason;
  /** Step at which the tag was popped (reason "popped"), else null. */
  since: number | null;
};

/** The borrow stack or tree at one step, after replaying every step up to it. */
export type StackFrame = {
  /** Items in display order: Stacked Borrows top first, Tree Borrows in tree order. */
  items: StackItem[];
  /** The memory access at this step, if any. */
  access: Step["acc"] | null;
  /** Named tags pushed (first created) at this step. */
  pushed: string[];
  /** Named tags popped (or, under Tree Borrows, frozen or disabled) at this step. */
  popped: string[];
  /** Named tags still on the stack after this step. */
  live: string[];
  /** Tags flagged invalid at this step (the UB step only). */
  invalid: Invalid[];
};

/** Returns whether a tag id names one tag (Miri printed it), rather than "·". */
function named(id: string): boolean {
  return id !== "·";
}

/**
 * Replays bug's borrow steps under model from the first step through `step`:
 * pushes each named tag when it first appears, pops it when a step marks it
 * popped, and reports the stack at `step` with the tags flagged invalid there
 * (protected, popped earlier, or forbidden by its tree state).
 */
export function stackAt(bug: Bug, model: Model, step: number): StackFrame {
  const steps = bug.miri[model];
  const k = Math.max(0, Math.min(steps.length - 1, step));
  const live = new Set<string>();
  const poppedAt = new Map<string, number>();
  let pushed: string[] = [];
  let popped: string[] = [];
  for (let j = 0; j <= k; j++) {
    pushed = [];
    popped = [];
    for (const it of steps[j].items) {
      if (!named(it.id)) continue;
      if (it.st === "pop") {
        if (!poppedAt.has(it.id)) {
          poppedAt.set(it.id, j);
          popped.push(it.id);
        }
        live.delete(it.id);
      } else if (it.st !== "bad" && !live.has(it.id) && !poppedAt.has(it.id)) {
        live.add(it.id);
        if (it.st === "new") pushed.push(it.id);
      }
    }
  }
  const cur = steps[k];
  const invalid: Invalid[] = cur.items
    .filter((it) => it.st === "bad")
    .map((it) => {
      if (it.prot) return { id: it.id, reason: "protected", since: null };
      const since = poppedAt.get(it.id);
      return since === undefined
        ? { id: it.id, reason: "state", since: null }
        : { id: it.id, reason: "popped", since };
    });
  return {
    items: model === "sb" ? [...cur.items].reverse() : cur.items,
    access: cur.acc ?? null,
    pushed,
    popped,
    live: [...live],
    invalid,
  };
}

/** Code-line marks for one step: lines executed now, lines visited earlier, and whether this step is the UB. */
export type CodeMarks = { hi: Set<string>; seen: Set<string>; bad: boolean };

/** Returns the key of a code line: "<seg file header>:<line number>". */
export function lineKey(file: string, line: number): string {
  return `${file}:${line}`;
}

/** Returns which trace lines step `step` executes (hi) and which earlier steps visited (seen). */
export function codeMarks(steps: Step[], step: number): CodeMarks {
  const hi = new Set(steps[step].at.map(([f, n]) => lineKey(f, n)));
  const seen = new Set<string>();
  for (let k = 0; k < step; k++) {
    for (const [f, n] of steps[k].at) seen.add(lineKey(f, n));
  }
  const acc = steps[step].acc;
  return { hi, seen, bad: !!acc && !acc.ok };
}

/**
 * Returns a predicate for Miri output lines that point at the step's code:
 * "file.rs:<line>:" locations or the " <line> |" source gutter.
 */
export function termHighlighter(
  at: [string, number][],
): (line: string) => boolean {
  const keys = at.map(([file, n]) => {
    let base = file.split(" ")[0];
    if (/main\.rs$/.test(file)) base = "main.rs";
    return {
      base: base.split("/").pop() ?? base,
      n,
      gutter: new RegExp(`^\\s*${n} \\|`),
    };
  });
  return (line) =>
    keys.some((k) => line.includes(`${k.base}:${k.n}:`) || k.gutter.test(line));
}

/** Returns how a terminal output line is coloured: Miri error, passing result, or plain. */
export function termLineKind(line: string): "error" | "ok" | null {
  if (/^error/.test(line)) return "error";
  if (/test result: ok|\.\.\. ok$|^boxq:|^row_realistic:/.test(line))
    return "ok";
  return null;
}

/** Returns the kind of a unified-diff line: added, deleted, hunk header or context. */
export function diffKind(line: string): "add" | "del" | "hunk" | "ctx" {
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "del";
  if (line.startsWith("@@")) return "hunk";
  return "ctx";
}

/** Returns the common leading-space indent of an excerpt (blank and "…"-elided lines ignored). */
export function indentOf(lines: Seg["lines"]): number {
  let ind = Number.POSITIVE_INFINITY;
  for (const [, t] of lines) {
    if (!/\S/.test(t) || t.startsWith("…")) continue;
    ind = Math.min(ind, (t.match(/^ */) ?? [""])[0].length);
  }
  return Number.isFinite(ind) ? ind : 0;
}

/** A piece of rich text: plain text, inline code or a commit hash. */
export type Part = { text: string } | { code: string } | { hash: string };

/** Splits text on `backticks` into plain and inline-code parts. */
export function mdParts(text: string): Part[] {
  return text
    .split(/`([^`]+)`/)
    .map((t, i): Part => (i % 2 ? { code: t } : { text: t }))
    .filter((p) => !("text" in p) || p.text !== "");
}

/** HASH_RE matches a 9-character commit hash not followed by a parent suffix (~ or ^). */
const HASH_RE = /\b([0-9a-f]{9})\b(?![~^])/g;

/** Splits text into plain parts and commit-hash parts (hashes have at least one digit and one letter). */
export function linkParts(text: string): Part[] {
  const out: Part[] = [];
  let last = 0;
  for (const m of text.matchAll(HASH_RE)) {
    const h = m[1];
    if (!/\d/.test(h) || !/[a-f]/.test(h)) continue;
    const at = m.index ?? 0;
    if (at > last) out.push({ text: text.slice(last, at) });
    out.push({ hash: h });
    last = at + h.length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}

/** Returns UTC milliseconds of "YYYY-MM-DD" or "YYYY-MM-DD HH:MM". */
function utc(s: string): number {
  const [d, t = "00:00"] = s.split(" ");
  const [y, mo, da] = d.split("-").map(Number);
  const [h, mi] = t.split(":").map(Number);
  return Date.UTC(y, mo - 1, da, h, mi);
}

/** Returns the first-written → fixed latency of a bug's life: hours under two days, else days. */
export function latency(life: Bug["life"]): string {
  const hrs = (utc(life[life.length - 1].date) - utc(life[0].date)) / 36e5;
  return hrs < 48 ? `${hrs.toFixed(1)} h` : `${(hrs / 24).toFixed(1)} days`;
}

/** Returns the fix date (YYYY-MM-DD) of a bug: the last entry of its life. */
export function fixDate(bug: Bug): string {
  return bug.life[bug.life.length - 1].date.slice(0, 10);
}

/** RAIL is the context rail's SVG geometry (the mockup's viewBox and scales). */
export const RAIL = {
  width: 960,
  height: 222,
  x0: 262,
  x1: 880,
  y0: 26,
  laneH: 26,
  t0: "2026-06-21",
  t1: "2026-10-08",
} as const;

/** MONTHS are the rail's month gridlines: date and label. */
export const MONTHS: [string, string][] = [
  ["2026-07-01", "Jul"],
  ["2026-08-01", "Aug"],
  ["2026-09-01", "Sep"],
  ["2026-10-01", "Oct"],
];

/** LANE_COLORS maps each rail lane to its colour (rows also separate lanes, so colour is never the only cue). */
export const LANE_COLORS: Record<string, string> = {
  sb: "#d97934",
  tb: "#c678dd",
  harness: "#ffbc7a",
  review: "#61aeee",
  san: "#e6c07b",
  symptom: "#9aa09a",
};

/** Returns the rail x of a date. */
export function railX(date: string): number {
  return (
    RAIL.x0 +
    ((utc(date) - utc(RAIL.t0)) / (utc(RAIL.t1) - utc(RAIL.t0))) *
      (RAIL.x1 - RAIL.x0)
  );
}

/** Returns the y of the centre line of lane i. */
export function laneY(i: number): number {
  return RAIL.y0 + RAIL.laneH * i + RAIL.laneH / 2;
}

/** Per-lane totals on the rail. */
export type LaneTotal = Lane & { events: number; defects: number; y: number };

/** Returns each lane with its event and defect totals and its centre y. */
export function laneTotals(data: TrData): LaneTotal[] {
  return data.lanes.map((l, i) => {
    const ev = data.timeline.filter((e) => e.lane === l.key);
    return {
      ...l,
      events: ev.length,
      defects: ev.reduce((a, e) => a + e.n, 0),
      y: laneY(i),
    };
  });
}

/** One plotted rail event. */
export type RailDot = {
  /** Index into data.timeline. */
  k: number;
  e: RailEvent;
  cx: number;
  cy: number;
  r: number;
  color: string;
};

/**
 * Returns one dot per event: x by date, y by lane (same-day events in a lane
 * fan out ±6 px), area proportional to the defect count.
 */
export function railDots(data: TrData): RailDot[] {
  const seen = new Map<string, number>();
  return data.timeline.map((e, k) => {
    const li = data.lanes.findIndex((l) => l.key === e.lane);
    const key = e.lane + e.d;
    const j = seen.get(key) ?? 0;
    seen.set(key, j + 1);
    const off = j ? (j % 2 ? -6 : 6) * Math.ceil(j / 2) : 0;
    return {
      k,
      e,
      cx: railX(e.d),
      cy: laneY(li) + off,
      r: 3 + 2.2 * Math.sqrt(e.n),
      color: LANE_COLORS[e.lane] ?? "#9aa09a",
    };
  });
}

/** Returns the timeline index of a bug's rail event (its fix commit), or -1. */
export function bugEvent(data: TrData, bug: Bug): number {
  return data.timeline.findIndex((e) => e.ref === bug.ref);
}

/** Returns the index of the replayed bug whose fix commit is the event's ref, or -1. */
export function eventBug(data: TrData, e: RailEvent): number {
  return data.bugs.findIndex((b) => b.ref === e.ref);
}

/** Which aliasing models a bug is undefined behaviour under, from its `models` field ("SB + TB"). */
export function bugModels(bug: Bug): { sb: boolean; tb: boolean } {
  const parts = bug.models.split("+").map((p) => p.trim());
  return { sb: parts.includes("SB"), tb: parts.includes("TB") };
}

/** Returns the models a bug is UB under, in words: "Stacked and Tree Borrows", "Stacked Borrows" or "Tree Borrows". */
export function modelsText(bug: Bug): string {
  const m = bugModels(bug);
  if (m.sb && m.tb) return "Stacked and Tree Borrows";
  return m.sb ? "Stacked Borrows" : "Tree Borrows";
}

/** Returns the state note shown on a borrow item: under Tree Borrows a "pop" is a state change (Frozen, Disabled), not a pop. */
export function stateNote(it: StackItem, model: Model): string {
  if (it.st === "new") return "new";
  if (it.st === "bad") return "invalid: UB";
  if (it.st === "pop")
    return model === "sb" ? "popped" : `invalidated: now ${it.perm}`;
  return "";
}

/** Headline figures: defects found by Miri or a Miri-proven audit, the total, the event count and the date span. */
export function headline(data: TrData): {
  miri: number;
  total: number;
  events: number;
  first: string;
  last: string;
} {
  const tl = data.timeline;
  const dates = tl.map((e) => e.d).sort();
  return {
    miri: tl.filter((e) => /Miri/.test(e.tool)).reduce((a, e) => a + e.n, 0),
    total: tl.reduce((a, e) => a + e.n, 0),
    events: tl.length,
    first: dates[0],
    last: dates[dates.length - 1],
  };
}

/** One computed insight tile: headline value and its sentence. */
export type Insight = { value: string; text: string };

/** Returns the four rail insight tiles computed from the rail events (the mockup's computation). */
export function insights(data: TrData): Insight[] {
  const tl = data.timeline;
  const sum = (f: (e: RailEvent) => boolean) =>
    tl.filter(f).reduce((a, e) => a + e.n, 0);
  const tot = sum(() => true);
  const ev = tl.length;
  const direct = sum((e) => ["sb", "tb", "harness"].includes(e.lane));
  const miri = sum((e) => /Miri|harness/i.test(e.tool));
  const alias = tl.filter((e) => /aliasing/.test(e.cls));
  const prod = tl.filter((e) => e.prod);
  const tb = sum((e) => e.lane === "tb");
  const octTb = tl.filter((e) => e.lane === "tb" && e.d >= "2026-10");
  const octN = octTb.reduce((a, e) => a + e.n, 0);
  const octTest = octTb.every((e) => !e.prod);
  return [
    {
      value: `${direct} / ${tot}`,
      text: `defects found by a Miri lane or proof harness directly; ${miri} counting audits and reviews that Miri then proved`,
    },
    {
      value: `${alias.reduce((a, e) => a + e.n, 0)} / ${tot}`,
      text: `defects in aliasing-model classes (${alias.length} of ${ev} events): Zig has no aliasing model to translate`,
    },
    {
      value: `${prod.length} / ${ev}`,
      text: `events in production code (${prod.reduce((a, e) => a + e.n, 0)} defects); the rest in tests, harnesses or benches`,
    },
    {
      value: `${tb}`,
      text: `defects surfaced by Tree Borrows runs; ${octN} of them in the October Tree Borrows lane, ${octTest ? "all test-side" : "some in production"}`,
    },
  ];
}

/** Returns each count bar's width as a percentage of the series maximum (one decimal). */
export function barPercents(series: [string, number][]): number[] {
  const mx = Math.max(...series.map((s) => s[1])) || 1;
  return series.map((s) => Math.round((s[1] / mx) * 1000) / 10);
}

/** BIRTHS_W is the width of the harness-birth strip's viewBox. */
export const BIRTHS_W = 300;

/** Returns one stacked dot per harness-crate birth date on the 06-21 → 10-08 strip. */
export function birthDots(births: string[]): { cx: number; cy: number }[] {
  const a = utc(RAIL.t0);
  const z = utc(RAIL.t1);
  const stack = new Map<number, number>();
  return births.map((d) => {
    const x = 6 + ((utc(d) - a) / (z - a)) * (BIRTHS_W - 12);
    const k = Math.round(x);
    const n = (stack.get(k) ?? 0) + 1;
    stack.set(k, n);
    return { cx: x, cy: 55 - (n - 1) * 4.6 };
  });
}
