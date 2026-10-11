/**
 * View-model of the sessions of the "Every prompt" widget: decodes the
 * summary's session rows (sent as arrays of sessionFields values), selects
 * the sessions a filter keeps, computes the session tiles and relaxes a
 * filter so that a chosen session shows.
 */
import {
  AREA_COMMAND_ONLY,
  applyFilter,
  type EpData,
  FLAG,
  type Filter,
  named,
  type Phase,
  type Tick,
} from "./model";

/** One session: a Claude Code or Codex session id on one machine. */
export type Session = {
  /** Index in the summary's sessions (= the tick's session index). */
  index: number;
  key: string;
  lane: number;
  /** Pacific day index and minute of day of the first and last prompt. */
  firstDay: number;
  firstMin: number;
  lastDay: number;
  lastMin: number;
  prompts: number;
  /** Index into the data's promptBuckets (sets the bar thickness). */
  promptsBucket: number;
  /** One of the data's areas, or "command-only". */
  area: string;
  title: string;
  /** "claude-code" (Claude Code's own title) or "claude-written". */
  titleSource: string;
  founding: boolean;
  articleWork: boolean;
  commandsOnly: boolean;
  /** Number of commits linked to the session's prompts. */
  commits: number;
  /** Packed row of the session timeline. */
  row: number;
  /** Pacific days from the first to the last prompt, inclusive. */
  spannedDays: number;
};

/** The session tile values for a set of sessions; null where undefined (no sessions). */
export type SessionTileValues = {
  sessions: number;
  commandOnly: number;
  founding: number;
  /** Sessions per area, in the data's area order (command-only sessions left out). */
  byArea: Array<{ area: string; n: number }>;
  /** The session with the most prompts (the earliest on a tie). */
  longestPrompts: Session | null;
  /** The day with the most sessions open (a prompt that is not a slash command), the earliest on a tie. */
  busiestDay: { day: number; open: number } | null;
  medianPrompts: number | null;
};

/** Decodes the summary's session rows into Session objects (index = tick session index). */
export function decodeSessions(d: EpData): Session[] {
  const f = d.sessionFields ?? [];
  /** Returns the value of field name in row r. */
  const at = (r: unknown[], name: string) => r[f.indexOf(name)];
  return (d.sessions ?? []).map((r, index) => ({
    index,
    key: String(at(r, "key")),
    lane: Number(at(r, "lane")),
    firstDay: Number(at(r, "firstDay")),
    firstMin: Number(at(r, "firstMin")),
    lastDay: Number(at(r, "lastDay")),
    lastMin: Number(at(r, "lastMin")),
    prompts: Number(at(r, "prompts")),
    promptsBucket: Number(at(r, "promptsBucket")),
    area: named(at(r, "area"), d.sessionAreas),
    title: String(at(r, "title")),
    titleSource: named(at(r, "titleSource"), d.titleSources),
    founding: at(r, "founding") === true,
    articleWork: at(r, "articleWork") === true,
    commandsOnly: at(r, "commandsOnly") === true,
    commits: Number(at(r, "commits")),
    row: Number(at(r, "row")),
    spannedDays: Number(at(r, "spannedDays")),
  }));
}

/**
 * BAR_MIN_W and HIT_W are the narrowest session bar and pointer target, in
 * viewBox units of the session timeline. The data builder packs rows with the
 * same numbers (w12_every_prompt.py: BAR_MIN_UNITS, HIT_UNITS, GAP_UNITS), so
 * bars on a row keep a visible gap and their hit targets never overlap.
 */
export const BAR_MIN_W = 2;
export const HIT_W = 7;

/** A session bar on a plot plotW units wide over nDays days: left edge, width and hit target. */
export type BarGeo = { x: number; w: number; hitX: number; hitW: number };

/** Returns the bar of session s on a plot plotW units wide (x from the plot's left edge). */
export function sessionBar(s: Session, nDays: number, plotW: number): BarGeo {
  const colW = plotW / nDays;
  const x = (s.firstDay + s.firstMin / 1440) * colW;
  const x2 = (s.lastDay + s.lastMin / 1440) * colW;
  const w = Math.max(BAR_MIN_W, x2 - x);
  const hitW = Math.max(HIT_W, w);
  return { x, w, hitX: x - (hitW - w) / 2, hitW };
}

/** Returns the day range a filter keeps: its from–to, narrowed to its phase when one is set. */
export function dayRange(f: Filter, phases?: Phase[]): [number, number] {
  let from = f.from;
  let to = f.to;
  const p = f.phase ? phases?.find((x) => x.key === f.phase) : undefined;
  if (p) {
    from = Math.max(from, p.from);
    to = Math.min(to, p.to);
  }
  return [from, to];
}

/**
 * Returns the sessions filter f keeps, in order: those whose first-to-last
 * span overlaps the day range (narrowed to the phase when phases are given),
 * in a selected lane, of the filter's area and of its session. Conditions on
 * single prompts (decisions, article work, settings, search) do not apply to
 * sessions.
 */
export function sessionsInRange(
  sessions: Session[],
  f: Filter,
  phases?: Phase[],
): Session[] {
  const [from, to] = dayRange(f, phases);
  return sessions.filter(
    (s) =>
      s.lastDay >= from &&
      s.firstDay <= to &&
      f.lanes.has(s.lane) &&
      (!f.area || s.area === f.area) &&
      (!f.session || s.key === f.session),
  );
}

/** Returns the median of a numeric list, or null when it is empty. */
function median(xs: number[]): number | null {
  if (xs.length === 0) return null;
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

/**
 * Returns whether a prompt keeps its session open that day: an instruction
 * or a decision by the builder's categories (so slash commands and shell
 * escapes do not count); for data without categories, any prompt that is not
 * a slash command.
 */
function substantive(t: Tick): boolean {
  if (t.category)
    return t.category === "instruction" || t.category === "decision";
  return (t.flags & FLAG.command) === 0;
}

/**
 * Returns the session tiles for the given sessions. ticks are the prompts
 * (each naming its session) used for the busiest parallel day: a session is
 * open on a Pacific day when it has an instruction or a decision that day
 * (substantive); with range, only days inside it count. areas is the data's
 * area order.
 */
export function sessionTiles(
  sessions: Session[],
  ticks: Tick[] = [],
  areas: string[] = [],
  range?: [number, number],
): SessionTileValues {
  const keys = new Set(sessions.map((s) => s.key));
  const open = new Map<number, Set<string>>();
  for (const t of ticks) {
    if (!keys.has(t.session) || !substantive(t)) continue;
    if (range && (t.day < range[0] || t.day > range[1])) continue;
    let set = open.get(t.day);
    if (!set) {
      set = new Set();
      open.set(t.day, set);
    }
    set.add(t.session);
  }
  let busiestDay: { day: number; open: number } | null = null;
  for (const [day, set] of open) {
    if (
      !busiestDay ||
      set.size > busiestDay.open ||
      (set.size === busiestDay.open && day < busiestDay.day)
    ) {
      busiestDay = { day, open: set.size };
    }
  }
  let longestPrompts: Session | null = null;
  for (const s of sessions) {
    if (!longestPrompts || s.prompts > longestPrompts.prompts)
      longestPrompts = s;
  }
  return {
    sessions: sessions.length,
    commandOnly: sessions.filter((s) => s.area === AREA_COMMAND_ONLY).length,
    founding: sessions.filter((s) => s.founding).length,
    byArea: areas.map((area) => ({
      area,
      n: sessions.filter((s) => s.area === area).length,
    })),
    longestPrompts,
    busiestDay,
    medianPrompts: median(sessions.map((s) => s.prompts)),
  };
}

/**
 * Returns f with the session filter set to s and only the conditions that
 * hide s relaxed: the day range widens to cover s, its lane is added, an
 * area or phase it lies outside is dropped, and a prompt condition
 * (decisions, article work, model, effort, search) is dropped when none of
 * the session's prompts passes it. When conditions that each keep some of
 * its prompts still keep none together, they are all dropped. ticks are the
 * prompts; match decides the search as in applyFilter (without it the search
 * does not hide anything and stays).
 */
export function revealSession(
  f: Filter,
  s: Session,
  ticks: Tick[] = [],
  phases: Phase[] = [],
  match?: (i: number) => boolean,
): Filter {
  const out: Filter = { ...f, lanes: new Set(f.lanes), session: s.key };
  out.from = Math.min(f.from, s.firstDay);
  out.to = Math.max(f.to, s.lastDay);
  out.lanes.add(s.lane);
  if (f.area && f.area !== s.area) out.area = undefined;
  if (f.phase) {
    const p = phases.find((x) => x.key === f.phase);
    if (!p || s.lastDay < p.from || s.firstDay > p.to) out.phase = undefined;
  }
  const own = ticks.filter((t) => t.session === s.key);
  if (own.length === 0) return out;
  /** The session's prompts that out keeps. */
  const kept = (x: Filter) => applyFilter(own, x, match).length;
  if (kept(out) > 0) return out;
  /** Unconditioned view of the session's prompts, to test one condition at a time. */
  const base: Filter = {
    ...out,
    decisionsOnly: false,
    hideArticle: false,
    model: undefined,
    effort: undefined,
    query: undefined,
    categories: undefined,
  };
  const single: Array<[keyof Filter, Partial<Filter>, Partial<Filter>]> = [
    ["decisionsOnly", { decisionsOnly: true }, { decisionsOnly: false }],
    ["hideArticle", { hideArticle: true }, { hideArticle: false }],
    ["model", { model: out.model }, { model: undefined }],
    ["effort", { effort: out.effort }, { effort: undefined }],
    ["query", { query: out.query }, { query: undefined }],
    ["categories", { categories: out.categories }, { categories: undefined }],
  ];
  for (const [field, on, off] of single) {
    if (!out[field]) continue;
    if (kept({ ...base, ...on }) === 0) Object.assign(out, off);
  }
  if (kept(out) === 0) Object.assign(out, base);
  return out;
}
