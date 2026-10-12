/**
 * View-model of the "Every prompt" widget: decodes the bundled summary's
 * ticks, applies the reader's filter, computes the live tiles and the weekly
 * settings bars, and encodes the filter in the URL fragment.
 */

/** A phase band of the timeline (one of the article's three method eras). */
export type Phase = { key: string; label: string; from: number; to: number };

/** One week of the settings view: counts per "model|effort" key, inferred models and modes. */
export type Week = {
  week: string;
  settings: Record<string, number>;
  inferred: Record<string, number>;
  modes: Record<string, number>;
};

/** Whole-period totals written by the data builder. */
export type Totals = {
  prompts: number;
  bySource: Record<string, number>;
  redactions: number;
  redactedPrompts: number;
  pastedLost: number;
  settingsUnknown: number;
  settingsInferred: number;
  articleWork: number;
  decisions: number;
  medianChars: number;
  foundingPrompts?: number;
  sessions?: number;
  sessionsCommandOnly?: number;
  foundingSessions?: number;
  sessionsByArea?: Record<string, number>;
  busiestParallelDay?: { day: string; open: number };
  longestSessionPrompts?: { key: string; prompts: number; spannedDays: number };
  medianPromptsPerSession?: number;
  byCategory?: Record<string, number>;
  commandsAndShell?: number;
  medianCharsSubstantive?: number;
};

/** The bundled summary (src/app/porting/data/every-prompt.json). */
export type EpData = {
  window: [string, string];
  purgedBefore: string;
  lanes: string[];
  models: string[];
  efforts: string[];
  /** Permission modes the mode indices of the ticks point into. */
  modes?: string[];
  /** Prompt categories in the builder's fixed order (tick element 10 indexes it). */
  categories?: string[];
  /**
   * [day, minuteOfDay, lane, flags, chars, words, modelIndex, effortIndex, sessionIndex, modeIndex,
   * categoryIndex] per prompt (T_* name the positions; the log-scale length is derived from chars).
   */
  ticks: number[][];
  /** The session areas in their fixed order ("command-only" is not one of them). */
  areas?: string[];
  /** What a session row's area index points into: the areas, then "command-only". */
  sessionAreas?: string[];
  /** What a session row's title-source index points into. */
  titleSources?: string[];
  /** Lower bounds of the prompt-count buckets that set a session bar's thickness. */
  promptBuckets?: number[];
  /** Names of the values of each session row, in order. */
  sessionFields?: string[];
  /** One row of sessionFields values per session. */
  sessions?: unknown[][];
  commitsPerDay: number[];
  phases: Phase[];
  weeks: Week[];
  totals: Totals;
  logUrl: string;
  logBytes: number;
  rules: {
    decision: string;
    articleWork: string;
    commits: string;
    sessions?: string;
  };
  sourcesHref: string;
};

/** One prompt as the timeline sees it; i is its index in the log's rows. */
export type Tick = {
  i: number;
  day: number;
  minute: number;
  lane: number;
  log2chars: number;
  flags: number;
  chars: number;
  words: number;
  model: string | null;
  effort: string | null;
  /** Permission mode, or null when not recorded. */
  mode: string | null;
  phase: string;
  /** Key of the prompt's session ("" when the data has no sessions). */
  session: string;
  /** Area of the prompt's session ("command-only" for a command-only session; "" without sessions). */
  area: string;
  /** The prompt's category (instruction, decision, shell, setting, session, other-command); "" when the data has none. */
  category?: string;
};

/** FLAG holds the tick flag bits written by the data builder. */
export const FLAG = {
  decision: 1,
  command: 2,
  article: 4,
  settingsUnknown: 8,
  pastedLost: 16,
  redacted: 32,
  /** The model is inferred from the Co-Authored-By trailers of linked commits. */
  inferred: 64,
} as const;

/** UNKNOWN is the filter value matching prompts whose model or effort is not recorded. */
export const UNKNOWN = "unknown";

/** LANE_NAMES are the display names of the lanes, in data order. */
export const LANE_NAMES = ["Mac Claude Code", "VM Claude Code", "Codex"];

/** LANE_SHORT are short lane names for tight places (tooltips, phone labels). */
export const LANE_SHORT = ["Mac", "VM", "Codex"];

/** LANE_COLOURS colour the lanes: Mac copper, VM gold, Codex teal. */
export const LANE_COLOURS = ["#d97934", "#ffbc7a", "#64d8c6"];

/** The reader's filter over the prompts; it is mirrored in the URL fragment. */
export type Filter = {
  from: number;
  to: number;
  lanes: Set<number>;
  decisionsOnly: boolean;
  hideArticle: boolean;
  model?: string;
  effort?: string;
  phase?: string;
  query?: string;
  /** Session area (one of the data's areas). */
  area?: string;
  /** Session key ("claude-mac-1662fcc7"). */
  session?: string;
  /** Prompt categories shown (instruction, decision, shell, …); undefined shows every category. */
  categories?: Set<string>;
  /** Whether the session timeline draws the command-only sessions. */
  commandOnly?: boolean;
};

/** CATEGORY_DEFAULT are the categories shown by default: instructions and decisions. */
export const CATEGORY_DEFAULT: readonly string[] = ["instruction", "decision"];

/** CATEGORY_ALL is the fragment value of the "every category" choice ("c=all"). */
const CATEGORY_ALL = "all";

/** CATEGORY_NAMES are the display names of the prompt categories. */
export const CATEGORY_NAMES: Record<string, string> = {
  instruction: "Instructions",
  decision: "Decisions",
  shell: "Shell",
  setting: "Settings",
  session: "Session commands",
  "other-command": "Other commands",
};

/** COMMAND_CATEGORIES are the categories of commands: shell escapes and slash commands. */
export const COMMAND_CATEGORIES: readonly string[] = [
  "shell",
  "setting",
  "session",
  "other-command",
];

/** Returns whether two category choices are the same (undefined = every category). */
export function sameCategories(
  a: Set<string> | undefined,
  b: Set<string> | undefined,
): boolean {
  if (!a || !b) return a === b;
  return a.size === b.size && [...a].every((x) => b.has(x));
}

/**
 * Returns the category choice in words for tile labels: "instructions and
 * decisions", "all prompts", or the chosen names joined ("no prompts" when
 * none is chosen).
 */
export function categoriesLabel(
  c: Set<string> | undefined,
  order: string[],
): string {
  if (!c) return "all prompts";
  const names = order
    .filter((x) => c.has(x))
    .map((x) => (CATEGORY_NAMES[x] ?? x).toLowerCase());
  if (names.length === 0) return "no prompts";
  if (names.length === order.length) return "all prompts";
  return names.length === 1
    ? names[0]
    : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** The live tile values for a filtered set of prompts; null where undefined (no prompts). */
export type TileValues = {
  prompts: number;
  medianChars: number | null;
  underTenWordsShare: number | null;
  decisions: number;
  commitsPerPrompt: number | null;
};

/** Commits per day and the day range they are summed over (for commits per prompt). */
export type CommitRange = { perDay: number[]; from: number; to: number };

/** FRAGMENT is the URL fragment name the filter is stored under ("#every-prompt?…"). */
export const FRAGMENT = "every-prompt";

/** QUERY_MAX caps the search text kept from a URL. */
const QUERY_MAX = 200;

/** Returns the number of days in the data window. */
export function dayCount(d: EpData): number {
  return d.commitsPerDay.length;
}

/** Returns the key of the phase containing day, or "" outside every phase. */
export function phaseOf(phases: Phase[], day: number): string {
  return phases.find((p) => day >= p.from && day <= p.to)?.key ?? "";
}

/** AREA_COMMAND_ONLY is the area value of a session whose every prompt is a slash command. */
export const AREA_COMMAND_ONLY = "command-only";

/** Returns the key and area of each session row of the summary, in data order. */
export function sessionKeys(d: EpData): { key: string; area: string }[] {
  const f = d.sessionFields ?? [];
  const k = f.indexOf("key");
  const a = f.indexOf("area");
  return (d.sessions ?? []).map((r) => ({
    key: String(r[k]),
    area: named(r[a], d.sessionAreas),
  }));
}

/** T_* are the positions of a tick's values (the builder's T_* constants). */
export const T_DAY = 0;
export const T_MINUTE = 1;
export const T_LANE = 2;
export const T_FLAGS = 3;
export const T_CHARS = 4;
export const T_WORDS = 5;
export const T_MODEL = 6;
export const T_EFFORT = 7;
export const T_SESSION = 8;
export const T_MODE = 9;
export const T_CATEGORY = 10;

/** Returns log2 of a prompt's length, rounded to 0.1 (the tick height scale). */
export function log2Chars(chars: number): number {
  return Math.round(Math.log2(Math.max(chars, 1)) * 10) / 10;
}

/** Returns a session row's area or title source: an index into names, or (older data) the name. */
export function named(v: unknown, names: string[] | undefined): string {
  return typeof v === "number" ? (names?.[v] ?? "") : String(v ?? "");
}

/** Decodes the summary's tick arrays into Tick objects (index i = log row index). */
export function decodeTicks(d: EpData): Tick[] {
  const sk = sessionKeys(d);
  return d.ticks.map((t, i) => ({
    i,
    day: t[T_DAY],
    minute: t[T_MINUTE],
    lane: t[T_LANE],
    log2chars: log2Chars(t[T_CHARS]),
    flags: t[T_FLAGS],
    chars: t[T_CHARS],
    words: t[T_WORDS],
    model: t[T_MODEL] >= 0 ? d.models[t[T_MODEL]] : null,
    effort: t[T_EFFORT] >= 0 ? d.efforts[t[T_EFFORT]] : null,
    mode:
      t[T_MODE] !== undefined && t[T_MODE] >= 0
        ? (d.modes?.[t[T_MODE]] ?? null)
        : null,
    phase: phaseOf(d.phases, t[T_DAY]),
    session: sk[t[T_SESSION]]?.key ?? "",
    area: sk[t[T_SESSION]]?.area ?? "",
    category:
      t[T_CATEGORY] !== undefined && t[T_CATEGORY] >= 0
        ? (d.categories?.[t[T_CATEGORY]] ?? "")
        : "",
  }));
}

/** Returns the unfiltered view: every day, every lane, no other condition. */
export function defaultFilter(
  nDays: number,
  nLanes: number,
  categories?: readonly string[],
): Filter {
  const f: Filter = {
    from: 0,
    to: nDays - 1,
    lanes: new Set(Array.from({ length: nLanes }, (_, i) => i)),
    decisionsOnly: false,
    hideArticle: false,
  };
  if (categories) f.categories = new Set(categories);
  return f;
}

/** Returns true when a value matches a model/effort filter ("unknown" matches null). */
function settingMatches(value: string | null, want: string | undefined) {
  if (!want) return true;
  return want === UNKNOWN ? value === null : value === want;
}

/**
 * Returns the ticks passing filter f, in order. The search query needs the
 * log's text: when f.query is set, match(i) decides for the tick with log row
 * index i; without match (log not loaded) the query is not applied.
 */
export function applyFilter(
  ticks: Tick[],
  f: Filter,
  match?: (i: number) => boolean,
): Tick[] {
  const useQuery = Boolean(f.query) && match !== undefined;
  return ticks.filter(
    (t) =>
      t.day >= f.from &&
      t.day <= f.to &&
      f.lanes.has(t.lane) &&
      (!f.decisionsOnly || (t.flags & FLAG.decision) !== 0) &&
      (!f.hideArticle || (t.flags & FLAG.article) === 0) &&
      settingMatches(t.model, f.model) &&
      settingMatches(t.effort, f.effort) &&
      (!f.phase || t.phase === f.phase) &&
      (!f.area || t.area === f.area) &&
      (!f.session || t.session === f.session) &&
      (!f.categories || !t.category || f.categories.has(t.category)) &&
      (!useQuery || (match as (i: number) => boolean)(t.i)),
  );
}

/**
 * Returns true when a log row matches the search text: a case-insensitive
 * substring of its text, or its whole id (case-insensitive exact match, so
 * "show the longest prompt" finds exactly that row). Blank text matches all.
 */
export function matchesQuery(
  row: { id: string; text: string },
  query: string,
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return row.id.toLowerCase() === q || row.text.toLowerCase().includes(q);
}

/** Returns the median of a non-empty numeric list. */
function median(xs: number[]): number {
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

/**
 * Returns the tile values for the given prompts. commitsPerPrompt divides all
 * commits made on the days of commits' range by the number of prompts; it is
 * null without a range or without prompts.
 */
export function tiles(ticks: Tick[], commits?: CommitRange): TileValues {
  const n = ticks.length;
  if (n === 0) {
    return {
      prompts: 0,
      medianChars: null,
      underTenWordsShare: null,
      decisions: 0,
      commitsPerPrompt: null,
    };
  }
  let c = 0;
  if (commits) {
    for (let d = commits.from; d <= commits.to; d++)
      c += commits.perDay[d] ?? 0;
  }
  return {
    prompts: n,
    medianChars: median(ticks.map((t) => t.chars)),
    underTenWordsShare: ticks.filter((t) => t.words < 10).length / n,
    decisions: ticks.filter((t) => (t.flags & FLAG.decision) !== 0).length,
    commitsPerPrompt: commits ? c / n : null,
  };
}

/**
 * Returns the days whose commits count toward commits per prompt, or
 * undefined when the filter selects prompts by anything other than days
 * (lanes, decisions, article work, settings, search, area, session): commits are not
 * attributed to lanes or kinds of prompt, so the ratio would mislead. A
 * phase narrows the day range to the phase's days.
 */
export function commitRange(
  f: Filter,
  phases: Phase[],
  perDay: number[],
): CommitRange | undefined {
  if (
    f.lanes.size < LANE_NAMES.length ||
    f.decisionsOnly ||
    f.hideArticle ||
    f.model ||
    f.effort ||
    f.query ||
    f.area ||
    f.session
  ) {
    return undefined;
  }
  let from = f.from;
  let to = f.to;
  if (f.phase) {
    const p = phases.find((x) => x.key === f.phase);
    if (!p) return undefined;
    from = Math.max(from, p.from);
    to = Math.min(to, p.to);
  }
  return from <= to ? { perDay, from, to } : undefined;
}

/** Returns the longest of the given prompts (the earliest on a tie), or null. */
export function longest(ticks: Tick[]): Tick | null {
  let best: Tick | null = null;
  for (const t of ticks) if (!best || t.chars > best.chars) best = t;
  return best;
}

/** Returns true when f is the unfiltered view def. */
export function isDefault(f: Filter, def: Filter): boolean {
  return encodeFilter(f, def) === encodeFilter(def, def);
}

/**
 * Encodes f as the URL fragment "every-prompt?from=…&to=…&lanes=0,2&d=1&a=0"
 * followed by m (model), e (effort), p (phase), q (search), ar (area) and
 * s (session) when set, c (categories: a sorted list, "all" for every
 * category; left out when it equals def's) and co=1 (command-only sessions
 * drawn) when on.
 */
export function encodeFilter(f: Filter, def?: Filter): string {
  const parts = [
    `from=${f.from}`,
    `to=${f.to}`,
    `lanes=${[...f.lanes].sort((a, b) => a - b).join(",")}`,
    `d=${f.decisionsOnly ? 1 : 0}`,
    `a=${f.hideArticle ? 1 : 0}`,
  ];
  const opt: [string, string | undefined][] = [
    ["m", f.model],
    ["e", f.effort],
    ["p", f.phase],
    ["q", f.query],
    ["ar", f.area],
    ["s", f.session],
  ];
  for (const [k, v] of opt) if (v) parts.push(`${k}=${encodeURIComponent(v)}`);
  if (!sameCategories(f.categories, def?.categories)) {
    parts.push(
      `c=${f.categories ? [...f.categories].sort().map(encodeURIComponent).join(",") : CATEGORY_ALL}`,
    );
  }
  if (f.commandOnly) parts.push("co=1");
  return `${FRAGMENT}?${parts.join("&")}`;
}

/**
 * Returns f without its session when its area excludes that session
 * (areaOf maps a session key to its area), and the dropped session key, so
 * an area and a session never combine into an empty view.
 */
export function resolveAreaSession(
  f: Filter,
  areaOf: (key: string) => string | undefined,
): { filter: Filter; dropped?: string } {
  if (!f.area || !f.session || areaOf(f.session) === f.area)
    return { filter: f };
  return { filter: { ...f, session: undefined }, dropped: f.session };
}

/** Returns the decoded URI component, or null when it is malformed. */
function safeDecode(v: string): string | null {
  try {
    return decodeURIComponent(v.replace(/\+/g, " "));
  } catch {
    return null;
  }
}

/** Returns v as a non-negative integer, or null when it is not one. */
function intOf(v: string | undefined): number | null {
  return v !== undefined && /^\d{1,4}$/.test(v) ? Number(v) : null;
}

/** Returns a flag value "0"/"1" as a boolean, or the fallback for anything else. */
function boolOf(v: string | undefined, fallback: boolean): boolean {
  return v === "1" ? true : v === "0" ? false : fallback;
}

/** The values a decoded filter may name: model ids, efforts, phase keys, areas and session keys of the data. */
export type Allowed = {
  models: string[];
  efforts: string[];
  phases: string[];
  areas?: string[];
  sessions?: string[];
  /** Area of each session key: a decoded area that excludes the decoded session drops the session. */
  sessionAreas?: Record<string, string>;
  /** Prompt categories a "c" list may name. */
  categories?: string[];
};

/**
 * Decodes a URL fragment written by encodeFilter (with or without the
 * leading "#"). Anything malformed falls back to defaults field by field:
 * days are clamped to the defaults' range (an inverted range is dropped),
 * lanes outside the defaults' lanes are ignored, and a fragment that is not
 * "every-prompt?…" yields the defaults. With allowed, a model, effort,
 * phase, area or session outside its list ("unknown" is always allowed for
 * model and effort; a missing areas or sessions list allows none) is ignored
 * too.
 */
export function decodeFilter(
  s: string,
  defaults: Filter,
  allowed?: Allowed,
): Filter {
  const out: Filter = { ...defaults, lanes: new Set(defaults.lanes) };
  if (defaults.categories) out.categories = new Set(defaults.categories);
  const h = s.replace(/^#/, "");
  if (!h.startsWith(`${FRAGMENT}?`)) return out;
  const q = new Map<string, string>();
  for (const kv of h.slice(FRAGMENT.length + 1).split("&")) {
    const eq = kv.indexOf("=");
    if (eq > 0) q.set(kv.slice(0, eq), kv.slice(eq + 1));
  }
  const lo = defaults.from;
  const hi = defaults.to;
  const from = intOf(q.get("from"));
  const to = intOf(q.get("to"));
  const a = from === null ? lo : Math.min(hi, Math.max(lo, from));
  const b = to === null ? hi : Math.min(hi, Math.max(lo, to));
  if (a <= b) {
    out.from = a;
    out.to = b;
  }
  const lanes = q.get("lanes");
  if (lanes !== undefined) {
    const picked = lanes
      .split(",")
      .map(intOf)
      .filter((x): x is number => x !== null && defaults.lanes.has(x));
    if (lanes === "" || picked.length > 0) out.lanes = new Set(picked);
  }
  out.decisionsOnly = boolOf(q.get("d"), defaults.decisionsOnly);
  if (boolOf(q.get("co"), defaults.commandOnly ?? false))
    out.commandOnly = true;
  else delete out.commandOnly;
  const c = q.get("c");
  if (c === CATEGORY_ALL) delete out.categories;
  else if (c !== undefined) {
    const list = allowed?.categories;
    const picked = c
      .split(",")
      .map((x) => safeDecode(x) ?? "")
      .filter((x) => x && (!list || list.includes(x)));
    if (c === "" || picked.length > 0) out.categories = new Set(picked);
  }
  out.hideArticle = boolOf(q.get("a"), defaults.hideArticle);
  const text: [keyof Filter, string][] = [
    ["model", "m"],
    ["effort", "e"],
    ["phase", "p"],
    ["query", "q"],
    ["area", "ar"],
    ["session", "s"],
  ];
  const lists: Partial<Record<keyof Filter, string[]>> = allowed
    ? {
        model: [...allowed.models, UNKNOWN],
        effort: [...allowed.efforts, UNKNOWN],
        phase: allowed.phases,
        area: allowed.areas ?? [],
        session: allowed.sessions ?? [],
      }
    : {};
  for (const [field, k] of text) {
    const raw = q.get(k);
    const v = raw === undefined ? null : safeDecode(raw);
    const list = lists[field];
    if (v?.trim() && (!list || list.includes(v))) {
      (out as Record<string, unknown>)[field] = v.slice(0, QUERY_MAX);
    }
  }
  const areas = allowed?.sessionAreas;
  return areas ? resolveAreaSession(out, (k) => areas[k]).filter : out;
}

/** Per-lane prompt counts of one day. */
export type DayCounts = { lanes: number[]; total: number };

/** Returns per-day, per-lane prompt counts of ticks over nDays days. */
export function dayCounts(
  ticks: Tick[],
  nDays: number,
  nLanes: number,
): DayCounts[] {
  const out = Array.from({ length: nDays }, () => ({
    lanes: new Array<number>(nLanes).fill(0),
    total: 0,
  }));
  for (const t of ticks) {
    out[t.day].lanes[t.lane] += 1;
    out[t.day].total += 1;
  }
  return out;
}

/** Returns the date of day index i of a window starting at start ("YYYY-MM-DD"), as a UTC Date. */
export function dayDate(start: string, i: number): Date {
  const d = new Date(`${start}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + i);
  return d;
}

/** MONTHS are short English month names. */
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

/** Formats a UTC date as "Jun 21". */
export function shortDate(d: Date): string {
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}`;
}

/** Returns the day index of an ISO date within a window starting at start. */
export function dayIndexOf(start: string, iso: string): number {
  return Math.round(
    (Date.parse(`${iso}T00:00:00Z`) - Date.parse(`${start}T00:00:00Z`)) /
      86_400_000,
  );
}

/** Returns a model id in display words: "claude-opus-4-8" → "Opus 4.8", "gpt-5.6-sol" → "GPT-5.6 Sol". */
export function modelLabel(id: string): string {
  const claude = /^claude-([a-z]+)-(\d+(?:-\d+)*)$/.exec(id);
  if (claude) {
    const fam = claude[1][0].toUpperCase() + claude[1].slice(1);
    return `${fam} ${claude[2].replace(/-/g, ".")}`;
  }
  const gpt = /^gpt-([\d.]+)(?:-([a-z]+))?$/.exec(id);
  if (gpt) {
    const tail = gpt[2] ? ` ${gpt[2][0].toUpperCase()}${gpt[2].slice(1)}` : "";
    return `GPT-${gpt[1]}${tail}`;
  }
  return id;
}

/** Returns the Monday ("YYYY-MM-DD") of the week holding day index day of a window starting at start. */
export function weekOf(start: string, day: number): string {
  const d = dayDate(start, day);
  d.setUTCDate(d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
  return d.toISOString().slice(0, 10);
}

/**
 * Returns the weekly settings of the given prompts, one entry per week of
 * the summary (empty weeks kept, in its order), counted as the data builder
 * counts them: a model inferred from commits under inferred, else
 * "model|effort" (effort "unknown" when not recorded), else "unknown"; the
 * permission mode, or "unknown", under modes.
 */
export function weeksFromTicks(ticks: Tick[], d: EpData): Week[] {
  const out = d.weeks.map((w) => ({
    week: w.week,
    settings: {} as Record<string, number>,
    inferred: {} as Record<string, number>,
    modes: {} as Record<string, number>,
  }));
  const at = new Map(out.map((w) => [w.week, w]));
  /** Adds one to key in counts. */
  const inc = (counts: Record<string, number>, key: string) => {
    counts[key] = (counts[key] ?? 0) + 1;
  };
  for (const t of ticks) {
    const w = at.get(weekOf(d.window[0], t.day));
    if (!w) continue;
    if ((t.flags & FLAG.inferred) !== 0 && t.model) inc(w.inferred, t.model);
    else if (t.model) inc(w.settings, `${t.model}|${t.effort ?? UNKNOWN}`);
    else inc(w.settings, UNKNOWN);
    inc(w.modes, t.mode ?? UNKNOWN);
  }
  return out;
}

/** Kind of a settings segment: recorded settings, inferred from commits, or unknown. */
export type SegKind = "known" | "inferred" | "unknown";

/** One segment of a weekly stacked bar. */
export type Segment = {
  /** Stable key: "model|effort", "inferred|model", a mode, or "unknown". */
  key: string;
  kind: SegKind;
  /** Model id (settings view) or mode (modes view); null for unknown. */
  model: string | null;
  /** Effort, or null when not recorded or not applicable. */
  effort: string | null;
  count: number;
};

/** One week's bar: its Monday, segments in legend order and total prompts. */
export type WeekBar = { week: string; segments: Segment[]; total: number };

/** EFFORT_ORDER orders efforts inside a model: high, medium, then not recorded. */
const EFFORT_ORDER = ["high", "medium", UNKNOWN];

/** MODE_ORDER orders the permission modes of the modes view. */
const MODE_ORDER = ["bypass", "plan", "default", "acceptEdits"];

/**
 * Returns the legend keys of the view in drawing order: recorded settings
 * (by model, then effort), inferred models, then unknown last.
 */
export function legendKeys(
  weeks: Week[],
  models: string[],
  view: "settings" | "modes",
): Segment[] {
  const sum = new Map<string, Segment>();
  /** Adds count to the segment keyed key, creating it from proto. */
  const add = (key: string, proto: Omit<Segment, "count">, n: number) => {
    const s = sum.get(key) ?? { ...proto, count: 0 };
    s.count += n;
    sum.set(key, s);
  };
  for (const w of weeks) {
    if (view === "modes") {
      for (const [m, n] of Object.entries(w.modes)) {
        if (m === UNKNOWN)
          add(
            UNKNOWN,
            { key: UNKNOWN, kind: "unknown", model: null, effort: null },
            n,
          );
        else add(m, { key: m, kind: "known", model: m, effort: null }, n);
      }
      continue;
    }
    for (const [k, n] of Object.entries(w.settings)) {
      if (k === UNKNOWN) {
        add(
          UNKNOWN,
          { key: UNKNOWN, kind: "unknown", model: null, effort: null },
          n,
        );
      } else {
        const [model, effort] = k.split("|");
        add(k, { key: k, kind: "known", model, effort: effort ?? UNKNOWN }, n);
      }
    }
    for (const [model, n] of Object.entries(w.inferred)) {
      add(
        `inferred|${model}`,
        { key: `inferred|${model}`, kind: "inferred", model, effort: null },
        n,
      );
    }
  }
  /** Sort rank of a segment. */
  const rank = (s: Segment): [number, number, number] => {
    const kind = s.kind === "known" ? 0 : s.kind === "inferred" ? 1 : 2;
    if (view === "modes") {
      const m = MODE_ORDER.indexOf(s.model ?? "");
      return [kind, m < 0 ? MODE_ORDER.length : m, 0];
    }
    const mi = models.indexOf(s.model ?? "");
    const ei = EFFORT_ORDER.indexOf(s.effort ?? UNKNOWN);
    return [kind, mi < 0 ? models.length : mi, ei];
  };
  return [...sum.values()].sort((a, b) => {
    const ra = rank(a);
    const rb = rank(b);
    return (
      ra[0] - rb[0] ||
      ra[1] - rb[1] ||
      ra[2] - rb[2] ||
      a.key.localeCompare(b.key)
    );
  });
}

/** Returns one stacked bar per week (empty weeks kept) with segments in legend order. */
export function weekBars(
  weeks: Week[],
  models: string[],
  view: "settings" | "modes",
): WeekBar[] {
  const order = legendKeys(weeks, models, view);
  return weeks.map((w) => {
    /** Returns this week's count for a legend segment. */
    const count = (s: Segment): number => {
      if (view === "modes") return w.modes[s.key] ?? 0;
      if (s.kind === "inferred") return w.inferred[s.model ?? ""] ?? 0;
      return w.settings[s.key] ?? 0;
    };
    const segments = order
      .map((s) => ({ ...s, count: count(s) }))
      .filter((s) => s.count > 0);
    return {
      week: w.week,
      segments,
      total: segments.reduce((a, s) => a + s.count, 0),
    };
  });
}
