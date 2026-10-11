/**
 * Pure view-model for the "Agents vs human" widget: day aggregates, the
 * statistics behind the six insight tiles, the hour-of-day profile, the
 * per-era autonomy histograms and leverage, and the colour ramps. No imports,
 * so scripts/verify-porting-helpers.mjs can run it under Node's type stripping.
 *
 * Time units: grid cell k = day * 24 + hour; commit and prompt times are
 * minutes since 2026-06-20 00:00 Pacific; lags are minutes (-1 = no recorded
 * prompt before the commit).
 */

/** One engineering era: [group, start date, end date, label]. */
export type Era = [string, string, string, string];
/** One human-story era: [start date, end date, label]. */
export type HumanEra = [string, string, string];
/** One sourced event: [date, "HH:MM", kind, short label, detail]. */
export type EventRow = [string, string, EventKind, string, string];
/** One quiet stretch: [start date, end date, reason]. */
export type QuietRow = [string, string, string];
/** Event kinds: idea pivot, rule change, reference move, milestone, limit or pause. */
export type EventKind = "pivot" | "rule" | "pin" | "mile" | "limit";

/** The widget's data file (src/app/porting/data/agents-vs-human.json). */
export type AvhData = {
  nCommits: number;
  grid: {
    commits: number[];
    merges: number[];
    prompts: number[];
    added: number[];
    deleted: number[];
  };
  subjects: Record<string, [string, string]>;
  commits: { t: number[]; lag: number[] };
  prompts: number[];
  originPrompts: number[];
  daily: {
    date: string[];
    mix: number[][];
    costMacClaude: number[];
    costVmClaude: number[];
    costCodex: number[];
    lost: number[];
    threads: number[];
    otherPrompts: number[];
  };
  models: string[];
  eras: Era[];
  humanEras: HumanEra[];
  events: EventRow[];
  quiet: QuietRow[];
};

/** The cell metric shown by the grid: commits, lines changed or prompts. */
export type Metric = "c" | "l" | "p";

/** MINUTES_PER_DAY is the number of minutes in a day. */
const MINUTES_PER_DAY = 1440;

/** MONTHS are the short English month names used in date labels. */
const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/** WEEKDAYS are the short English weekday names, Sunday first. */
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** LAG_BUCKETS are the autonomy histogram bucket edges in minutes. */
export const LAG_BUCKETS = [0, 30, 60, 120, 240, 480, 1440, 1e9];

/** LAG_LABELS name the seven autonomy histogram buckets. */
export const LAG_LABELS = [
  "<30m",
  "30m–1h",
  "1–2h",
  "2–4h",
  "4–8h",
  "8–24h",
  ">24h",
];

/** MODEL_NAMES are the display names of the commit-trailer models, in data order. */
export const MODEL_NAMES = [
  "Opus 4.8",
  "Fable 5",
  "Opus 5",
  "Opus 5.5",
  "Sonnet 5.5",
  "no trailer",
];

/** MODEL_COLOURS colour the model ribbon, in data order. */
export const MODEL_COLOURS = [
  "#ffbc7a",
  "#c678dd",
  "#61aeee",
  "#98c379",
  "#d19a66",
  "#4a4f4b",
];

/**
 * MODEL_TEXTURES give each model a fill pattern id suffix ("" = solid), so the
 * two closest colours (Fable 5 purple, Opus 5 blue) differ by texture as well.
 */
export const MODEL_TEXTURES = ["", "dots", "", "stripe", "", ""];

/** ERA_GROUPS are the engineering-era lanes of the left Gantt, in column order. */
export const ERA_GROUPS = [
  "foundation",
  "port",
  "platform",
  "strategy",
  "perf",
  "resync",
  "audit",
];

/** GROUP_COLOURS colour each engineering-era lane. */
export const GROUP_COLOURS: Record<string, string> = {
  foundation: "#9aa09a",
  port: "#d97934",
  platform: "#61aeee",
  strategy: "#c678dd",
  perf: "#ffbc7a",
  resync: "#64d8c6",
  audit: "#e6c07b",
};

/** GROUP_NAMES are the legend names of the engineering-era lanes (keys are data ids). */
export const GROUP_NAMES: Record<string, string> = {
  foundation: "foundation",
  port: "port",
  platform: "platform",
  strategy: "strategy",
  perf: "perf",
  resync: "upstream",
  audit: "audit",
};

/** KIND_COLOURS colour each event kind. */
export const KIND_COLOURS: Record<EventKind, string> = {
  pivot: "#c678dd",
  rule: "#ffbc7a",
  pin: "#64d8c6",
  mile: "#d9ded6",
  limit: "#e06c75",
};

/** KIND_NAMES are the legend names of the event kinds. */
export const KIND_NAMES: [EventKind, string][] = [
  ["pivot", "idea pivot"],
  ["rule", "rule change"],
  ["pin", "reference move"],
  ["mile", "milestone"],
  ["limit", "limit / pause"],
];

/** RAMP_STOPS are the log colour ramps per metric (dark to light). */
const RAMP_STOPS: Record<Metric, string[]> = {
  c: ["#2a1a0f", "#5a3014", "#99501f", "#d97934", "#ffc58f"],
  l: ["#241d0f", "#4d3d16", "#8c6e2a", "#e6c07b", "#fff1c9"],
  p: ["#0e2420", "#15493f", "#278575", "#64d8c6", "#cafff4"],
};

/** Parses "#rrggbb" into [r, g, b]. */
function hexRgb(hex: string): number[] {
  return [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16));
}

/** Returns the colour at t ∈ [0, 1] on metric's ramp (clamped), as "rgb(r,g,b)". */
export function rampColour(metric: Metric, t: number): string {
  const stops = RAMP_STOPS[metric].map(hexRgb);
  const c = Math.max(0, Math.min(1, t));
  const x = c * (stops.length - 1);
  const i = Math.min(stops.length - 2, Math.floor(x));
  const f = x - i;
  const rgb = stops[i].map((v, j) => Math.round(v + (stops[i + 1][j] - v) * f));
  return `rgb(${rgb.join(",")})`;
}

/** Returns the value of metric in grid cell k. */
export function cellValue(data: AvhData, metric: Metric, k: number): number {
  const g = data.grid;
  if (metric === "c") return g.commits[k];
  if (metric === "p") return g.prompts[k];
  return g.added[k] + g.deleted[k];
}

/** Returns the largest single-cell value of metric over the whole grid. */
export function metricMax(data: AvhData, metric: Metric): number {
  let m = 0;
  for (let k = 0; k < data.grid.commits.length; k++) {
    m = Math.max(m, cellValue(data, metric, k));
  }
  return m;
}

/** Per-day totals of the grid series. */
export type DayTotals = {
  commits: number[];
  merges: number[];
  prompts: number[];
  added: number[];
  deleted: number[];
};

/** Returns per-day totals of commits, merges, prompts and lines added/removed. */
export function dayTotals(data: AvhData): DayTotals {
  const n = data.daily.date.length;
  const out: DayTotals = {
    commits: [],
    merges: [],
    prompts: [],
    added: [],
    deleted: [],
  };
  const g = data.grid;
  for (let i = 0; i < n; i++) {
    let c = 0;
    let m = 0;
    let p = 0;
    let a = 0;
    let d = 0;
    for (let h = 0; h < 24; h++) {
      const k = i * 24 + h;
      c += g.commits[k];
      m += g.merges[k];
      p += g.prompts[k];
      a += g.added[k];
      d += g.deleted[k];
    }
    out.commits.push(c);
    out.merges.push(m);
    out.prompts.push(p);
    out.added.push(a);
    out.deleted.push(d);
  }
  return out;
}

/** Returns the index of a "YYYY-MM-DD" date in the day axis (-1 if absent). */
export function dayIndex(data: AvhData, date: string): number {
  return data.daily.date.indexOf(date);
}

/** Returns "Sep 23" for day index i. */
export function dayLabel(data: AvhData, i: number): string {
  const [, m, d] = data.daily.date[i].split("-");
  return `${MONTHS[Number(m) - 1]} ${Number(d)}`;
}

/** Returns the weekday of day index i: 0 = Sunday … 6 = Saturday. */
export function weekdayIndex(data: AvhData, i: number): number {
  const [y, m, d] = data.daily.date[i].split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d)).getUTCDay();
}

/** Returns the short weekday name ("Wed") of day index i. */
export function weekday(data: AvhData, i: number): string {
  return WEEKDAYS[weekdayIndex(data, i)];
}

/** Returns "Sep 23 19:00" for a time in minutes since the first day. */
export function minuteLabel(data: AvhData, minutes: number): string {
  const d = Math.floor(minutes / MINUTES_PER_DAY);
  const r = minutes % MINUTES_PER_DAY;
  return `${dayLabel(data, d)} ${pad2(Math.floor(r / 60))}:${pad2(r % 60)}`;
}

/** Zero-pads n to two digits. */
export function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** Formats an integer with en-US thousands separators. */
export function fmtN(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** Formats a lag in minutes as "12 min" or "2.2 h"; null as "–". */
export function fmtLag(minutes: number | null): string {
  if (minutes === null) return "–";
  return minutes < 60
    ? `${Math.round(minutes)} min`
    : `${(minutes / 60).toFixed(1)} h`;
}

/** Returns each human era as [first day index, last day index, label]. */
export function humanEraRanges(data: AvhData): [number, number, string][] {
  return data.humanEras.map(([s, e, l]) => [
    dayIndex(data, s),
    dayIndex(data, e),
    l,
  ]);
}

/** Returns the human era index containing day i, or -1. */
export function eraOfDay(ranges: [number, number, string][], i: number) {
  return ranges.findIndex(([a, b]) => i >= a && i <= b);
}

/** Returns the first index in the sorted array whose value is ≥ x. */
export function lowerBound(sorted: number[], x: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (sorted[m] < x) lo = m + 1;
    else hi = m;
  }
  return lo;
}

/**
 * Returns hands-on prompting hours for sorted prompt minutes: prompts less
 * than or equal to 30 minutes apart form one cluster, and each cluster counts
 * its span plus 5 minutes.
 */
export function handsOnHours(ts: number[]): number {
  if (!ts.length) return 0;
  let total = 0;
  let s = ts[0];
  let e = ts[0];
  for (let j = 1; j < ts.length; j++) {
    if (ts[j] - e <= 30) e = ts[j];
    else {
      total += e - s + 5;
      s = ts[j];
      e = ts[j];
    }
  }
  return (total + e - s + 5) / 60;
}

/** The longest run of commits between two consecutive prompts. */
export type Run = {
  /** Index into data.prompts of the prompt before the run (-1: before any prompt). */
  prompt: number;
  /** Commits in the run. */
  commits: number;
};

/** Statistics for a day range [a, b] (inclusive), behind the tiles and counters. */
export type BrushStats = {
  a: number;
  b: number;
  /** Non-merge commits with a commit-time record in range. */
  n: number;
  /** Commits more than 1 h / 8 h after the last prompt. */
  gt1: number;
  gt8: number;
  /** Overnight (00–06) commits and prompts, and grid totals. */
  overnightCommits: number;
  overnightPrompts: number;
  totalCommits: number;
  totalPrompts: number;
  /** Lines added + removed. */
  lines: number;
  peakCommitHour: number;
  peakPromptHour: number;
  bestRun: Run | null;
  handsOn: number;
  /** Days with commits and no prompt. */
  noPromptDays: number;
  /** Prompts in range. */
  nPrompts: number;
};

/** Returns the day of each time in minutes. */
function daysOf(ts: number[]): number[] {
  return ts.map((t) => Math.floor(t / MINUTES_PER_DAY));
}

/** Returns the index of the first maximum in arr. */
function argMax(arr: number[]): number {
  return arr.indexOf(Math.max(...arr));
}

/**
 * Returns the statistics for day range [startDay, endDay] (inclusive). The
 * caller clips endDay to the replay position when a replay is running.
 */
export function brushStats(
  data: AvhData,
  startDay: number,
  endDay: number,
): BrushStats {
  const a = startDay;
  const b = endDay;
  const g = data.grid;
  const cday = daysOf(data.commits.t);
  const idx: number[] = [];
  for (let j = 0; j < cday.length; j++) {
    if (cday[j] >= a && cday[j] <= b) idx.push(j);
  }
  const lag = data.commits.lag;
  const n = idx.length;
  const gt1 = idx.filter((j) => lag[j] > 60).length;
  const gt8 = idx.filter((j) => lag[j] > 480).length;
  let oc = 0;
  let op = 0;
  let tc = 0;
  let tp = 0;
  let ta = 0;
  const hc = new Array(24).fill(0);
  const hp = new Array(24).fill(0);
  for (let i = a; i <= b; i++) {
    for (let h = 0; h < 24; h++) {
      const k = i * 24 + h;
      tc += g.commits[k];
      tp += g.prompts[k];
      ta += g.added[k] + g.deleted[k];
      hc[h] += g.commits[k];
      hp[h] += g.prompts[k];
      if (h < 6) {
        oc += g.commits[k];
        op += g.prompts[k];
      }
    }
  }
  const counts = new Map<number, number>();
  for (const j of idx) {
    const pi = lowerBound(data.prompts, data.commits.t[j] + 1) - 1;
    counts.set(pi, (counts.get(pi) ?? 0) + 1);
  }
  let bestRun: Run | null = null;
  counts.forEach((v, k) => {
    if (!bestRun || v > bestRun.commits) bestRun = { prompt: k, commits: v };
  });
  const pday = daysOf(data.prompts);
  const ps = data.prompts.filter((_, j) => pday[j] >= a && pday[j] <= b);
  const totals = dayTotals(data);
  let noPromptDays = 0;
  for (let i = a; i <= b; i++) {
    if (totals.commits[i] && !totals.prompts[i]) noPromptDays++;
  }
  return {
    a,
    b,
    n,
    gt1,
    gt8,
    overnightCommits: oc,
    overnightPrompts: op,
    totalCommits: tc,
    totalPrompts: tp,
    lines: ta,
    peakCommitHour: argMax(hc),
    peakPromptHour: argMax(hp),
    bestRun,
    handsOn: handsOnHours(ps),
    noPromptDays,
    nPrompts: ps.length,
  };
}

/** Returns x / y as a whole percentage ("52%"), or "–" when y is 0. */
export function pct(x: number, y: number): string {
  return y ? `${Math.round((100 * x) / y)}%` : "–";
}

/** One insight tile: the big value and its explanation. */
export type Tile = { value: string; text: string };

/** Returns the six insight tiles for stats, worded as in the mockup. */
export function insightTiles(data: AvhData, s: BrushStats): Tile[] {
  let run = "–";
  let runText = "no commits in range";
  if (s.bestRun) {
    const { prompt, commits } = s.bestRun;
    run = fmtN(commits);
    runText =
      prompt < 0
        ? "commits before the first recorded prompt"
        : `commits between two prompts, ${minuteLabel(data, data.prompts[prompt])} → ${
            prompt + 1 < data.prompts.length
              ? minuteLabel(data, data.prompts[prompt + 1])
              : "now"
          }`;
  }
  const perHour = s.handsOn ? (s.n / s.handsOn).toFixed(0) : "–";
  return [
    {
      value: pct(s.gt1, s.n),
      text: `of ${fmtN(s.n)} commits landed >1 h after the last prompt; ${pct(s.gt8, s.n)} after >8 h`,
    },
    {
      value: `${pct(s.overnightCommits, s.totalCommits)} vs ${pct(s.overnightPrompts, s.totalPrompts)}`,
      text: "overnight (00–06 PT) share: commits vs prompts",
    },
    {
      value: `${pad2(s.peakCommitHour)}h / ${pad2(s.peakPromptHour)}h`,
      text: "peak hour for commits / for prompts",
    },
    {
      value: s.nPrompts ? `${(s.n / s.nPrompts).toFixed(1)}×` : "–",
      text: `commits per prompt (${fmtN(s.nPrompts)} prompts)`,
    },
    { value: run, text: `longest unattended run: ${runText}` },
    {
      value: s.handsOn ? `${s.handsOn.toFixed(0)} h` : "–",
      text: `hands-on prompting time → ${perHour} commits per hands-on hour; ${s.noPromptDays} day${
        s.noPromptDays === 1 ? "" : "s"
      } had commits and no prompt`,
    },
  ];
}

/** Share of commits and of prompts per hour of day over a day range. */
export type HourProfile = { commits: number[]; prompts: number[] };

/** Returns the share of commits and prompts per hour of day for days [a, b]. */
export function hourProfile(data: AvhData, a: number, b: number): HourProfile {
  const hc = new Array(24).fill(0);
  const hp = new Array(24).fill(0);
  for (let i = a; i <= b; i++) {
    for (let h = 0; h < 24; h++) {
      hc[h] += data.grid.commits[i * 24 + h];
      hp[h] += data.grid.prompts[i * 24 + h];
    }
  }
  const tc = hc.reduce((x, y) => x + y, 0);
  const tp = hp.reduce((x, y) => x + y, 0);
  return {
    commits: hc.map((v) => (tc ? v / tc : 0)),
    prompts: hp.map((v) => (tp ? v / tp : 0)),
  };
}

/** Autonomy and leverage of one human era within a day range. */
export type EraAutonomy = {
  era: number;
  label: string;
  start: number;
  end: number;
  /** Commits in range, including those with no recorded prompt. */
  commits: number;
  noPrompt: number;
  /** Commit counts per lag bucket (LAG_LABELS). */
  hist: number[];
  /** Median lag in minutes, or null. */
  medianLag: number | null;
  /** Median position on the bucket axis, in bucket units, or null. */
  medianPos: number | null;
  prompts: number;
  commitsPerPrompt: number;
  commitsPerHour: number;
  handsOn: number;
};

/** Returns per-human-era autonomy histograms and leverage for days [a, b]. */
export function autonomy(data: AvhData, a: number, b: number): EraAutonomy[] {
  const ranges = humanEraRanges(data);
  const per = ranges.map(() => ({
    lags: [] as number[],
    none: 0,
    prompts: [] as number[],
  }));
  const cday = daysOf(data.commits.t);
  for (let j = 0; j < cday.length; j++) {
    const i = cday[j];
    if (i < a || i > b) continue;
    const e = eraOfDay(ranges, i);
    if (e < 0) continue;
    if (data.commits.lag[j] < 0) per[e].none++;
    else per[e].lags.push(data.commits.lag[j]);
  }
  const pday = daysOf(data.prompts);
  data.prompts.forEach((t, j) => {
    const i = pday[j];
    if (i < a || i > b) return;
    const e = eraOfDay(ranges, i);
    if (e >= 0) per[e].prompts.push(t);
  });
  return per.map((x, e) => {
    const commits = x.lags.length + x.none;
    const hist = LAG_LABELS.map(
      (_, k) =>
        x.lags.filter((v) => v >= LAG_BUCKETS[k] && v < LAG_BUCKETS[k + 1])
          .length,
    );
    const sorted = x.lags.slice().sort((p, q) => p - q);
    const med = sorted.length ? sorted[sorted.length >> 1] : null;
    let pos: number | null = null;
    if (med !== null) {
      let k = 0;
      while (med >= LAG_BUCKETS[k + 1]) k++;
      pos =
        k +
        (med - LAG_BUCKETS[k]) /
          (Math.min(LAG_BUCKETS[k + 1], 2880) - LAG_BUCKETS[k]);
    }
    const eh = handsOnHours(x.prompts);
    return {
      era: e,
      label: ranges[e][2],
      start: ranges[e][0],
      end: ranges[e][1],
      commits,
      noPrompt: x.none,
      hist,
      medianLag: med,
      medianPos: pos,
      prompts: x.prompts.length,
      commitsPerPrompt: x.prompts.length ? commits / x.prompts.length : 0,
      commitsPerHour: eh ? commits / eh : 0,
      handsOn: eh,
    };
  });
}
