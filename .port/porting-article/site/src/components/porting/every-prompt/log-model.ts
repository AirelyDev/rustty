/**
 * View-model of the "Every prompt" log: decodes the fetched log
 * (public/porting/every-prompt-log.json), filters its rows with the same
 * predicate as the timeline (applyFilter + matchesQuery), finds the most
 * repeated prompts and splits prompt text around glossary ids. Text stays
 * plain strings throughout; the components render it as text nodes only.
 */
import {
  applyFilter,
  dayIndexOf,
  FLAG,
  type Filter,
  matchesQuery,
  type Tick,
} from "./model";

/** One prompt as the log file holds it (text already redacted). */
export type LogRow = {
  id: string;
  ts: string;
  tool: string;
  machine: string;
  model: string | null;
  effort: string | null;
  mode: string | null;
  settingsSource: string;
  chars: number;
  kind: string;
  pasted: string;
  articleWork: boolean;
  decision: boolean;
  phase: string;
  commits: string[];
  commitsBasis: string;
  /** The prompt's category (absent in logs written before categories). */
  category?: string;
  /** Key of the prompt's session (absent in logs written before sessions). */
  session?: string;
  /** Typed in the website folders (absent in logs written before the website folders were included). */
  website?: boolean;
  text: string;
};

/** What the log file holds about one session (keyed by session key). */
export type LogSession = {
  title: string;
  titleSource: string;
  area: string;
  branches: string[];
  models: string[];
  commits: string[];
  commitsAmbiguous: number;
};

/** A decoded log row: i is its index (= the timeline tick index), day and lane as in the ticks. */
export type Row = LogRow & {
  i: number;
  day: number;
  minute: number;
  lane: number;
  /** Session key ("" when the log has none) and its area. */
  session: string;
  area: string;
};

/** The whole fetched log file. */
export type LogFile = {
  glossary: Record<string, string>;
  rows: LogRow[];
  sessions?: Record<string, LogSession>;
};

/** REDACTED_MARK starts every redaction in the text ("[redacted: secret]"). */
export const REDACTED_MARK = "[redacted:";

/** PT_PARTS formats an instant into Pacific calendar and clock parts. */
const PT_PARTS = new Intl.DateTimeFormat("en-US", {
  timeZone: "America/Los_Angeles",
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

/** Returns the Pacific date ("YYYY-MM-DD") and time ("HH:mm") of an ISO instant. */
export function pacific(ts: string): { date: string; time: string } {
  const p: Record<string, string> = {};
  for (const x of PT_PARTS.formatToParts(new Date(ts))) p[x.type] = x.value;
  return {
    date: `${p.year}-${p.month}-${p.day}`,
    time: `${p.hour}:${p.minute}`,
  };
}

/** Formats an ISO instant as Pacific "YYYY-MM-DD HH:mm". */
export function fmtPacific(ts: string): string {
  const { date, time } = pacific(ts);
  return `${date} ${time}`;
}

/** Returns the lane key of a log row: "codex", or "claude-<machine>". */
export function laneKey(r: Pick<LogRow, "tool" | "machine">): string {
  return r.tool === "codex" ? "codex" : `${r.tool}-${r.machine}`;
}

/**
 * Checks the shape of a fetched log and decodes its rows against the
 * summary's window start and lanes. Throws on a malformed file or a row in
 * an unknown lane, so the view shows its retry state instead of crashing.
 */
export function decodeLog(
  raw: unknown,
  start: string,
  lanes: string[],
): {
  glossary: Record<string, string>;
  rows: Row[];
  sessions: Record<string, LogSession>;
} {
  const f = raw as Partial<LogFile> | null;
  if (!f || !Array.isArray(f.rows) || typeof f.glossary !== "object") {
    throw new Error("the log file is malformed");
  }
  const sessions: Record<string, LogSession> =
    f.sessions && typeof f.sessions === "object" ? { ...f.sessions } : {};
  const rows = f.rows.map((r, i) => {
    if (typeof r?.text !== "string" || typeof r.id !== "string") {
      throw new Error(`log row ${i} is malformed`);
    }
    const lane = lanes.indexOf(laneKey(r));
    if (lane < 0) throw new Error(`log row ${r.id} has an unknown lane`);
    const { date, time } = pacific(r.ts);
    const [h, m] = time.split(":").map(Number);
    return {
      ...r,
      commits: Array.isArray(r.commits) ? r.commits : [],
      i,
      day: dayIndexOf(start, date),
      minute: h * 60 + m,
      lane,
      session: typeof r.session === "string" ? r.session : "",
      area: (r.session && sessions[r.session]?.area) || "",
    };
  });
  return { glossary: { ...(f.glossary ?? {}) }, rows, sessions };
}

/** Returns the timeline flags of a row (the same bits the builder writes into ticks). */
export function rowFlags(r: LogRow): number {
  return (
    (r.decision ? FLAG.decision : 0) |
    (r.kind === "command" ? FLAG.command : 0) |
    (r.articleWork ? FLAG.article : 0) |
    (r.settingsSource === "unknown" ? FLAG.settingsUnknown : 0) |
    (r.pasted === "lost" ? FLAG.pastedLost : 0) |
    (r.text.includes(REDACTED_MARK) ? FLAG.redacted : 0) |
    (r.settingsSource === "commit-trailer" ? FLAG.inferred : 0) |
    (r.website ? FLAG.website : 0)
  );
}

/** Returns the row as a timeline tick, so it can go through applyFilter. */
function rowTick(r: Row): Tick {
  return {
    i: r.i,
    day: r.day,
    minute: r.minute,
    lane: r.lane,
    log2chars: Math.log2(Math.max(1, r.chars)),
    flags: rowFlags(r),
    chars: r.chars,
    words: 0,
    model: r.model,
    effort: r.effort,
    mode: r.mode,
    phase: r.phase,
    session: r.session,
    area: r.area,
    category: r.category ?? "",
  };
}

/**
 * Returns the rows passing filter f, in order. It runs the timeline's own
 * applyFilter over the rows (with matchesQuery as the search), so the log
 * and the timeline always agree on which prompts match.
 */
export function filterRows(rows: Row[], f: Filter): Row[] {
  const byIndex = new Map(rows.map((r) => [r.i, r]));
  const q = f.query;
  const match = q
    ? (i: number) => {
        const r = byIndex.get(i);
        return r ? matchesQuery(r, q) : false;
      }
    : undefined;
  return applyFilter(rows.map(rowTick), f, match).map(
    (t) => byIndex.get(t.i) as Row,
  );
}

/** Returns text with surrounding whitespace dropped and inner runs collapsed to one space. */
export function normalizeText(text: string): string {
  return text.trim().replace(/\s+/g, " ");
}

/**
 * Returns the key two prompts share when they count as the same repeated
 * prompt: normalizeText, lowercased, with trailing ".", "?", "!" and
 * whitespace dropped ("Status?" and "status" share "status").
 */
export function repeatKey(text: string): string {
  return normalizeText(text)
    .toLowerCase()
    .replace(/[.?!\s]+$/, "");
}

/**
 * Returns the n prompt texts typed most often (at least twice), by count and
 * then first appearance. Slash commands are left out, and texts count as the
 * same when their repeatKey is equal; each is shown as first typed (with
 * whitespace normalized).
 */
export function topRepeated(
  rows: Pick<LogRow, "kind" | "text">[],
  n: number,
): { text: string; count: number }[] {
  const seen = new Map<string, { text: string; count: number; at: number }>();
  rows.forEach((r, at) => {
    if (r.kind === "command") return;
    const key = repeatKey(r.text);
    if (!key) return;
    const e = seen.get(key);
    if (e) e.count += 1;
    else seen.set(key, { text: normalizeText(r.text), count: 1, at });
  });
  return [...seen.values()]
    .filter((e) => e.count > 1)
    .sort((a, b) => b.count - a.count || a.at - b.at)
    .slice(0, n)
    .map(({ text, count }) => ({ text, count }));
}

/** Escapes a string for use inside a regular expression. */
function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Cache of glossary matchers keyed by their joined ids. */
const GLOSS_RE = new Map<string, RegExp>();

/**
 * Splits text into plain parts and glossary ids, in order. An id matches as
 * a whole token (not inside a longer word or id such as "P3-W0" for "P3"),
 * with its exact spelling; the longest id wins where two overlap. Joining
 * the parts' t gives back text unchanged.
 */
export function splitGlossed(
  text: string,
  glossary: Record<string, string>,
): Array<{ t: string; id?: string }> {
  const ids = Object.keys(glossary).sort(
    (a, b) => b.length - a.length || a.localeCompare(b),
  );
  if (ids.length === 0 || !text) return text ? [{ t: text }] : [];
  const key = ids.join("\u0000");
  let re = GLOSS_RE.get(key);
  if (!re) {
    re = new RegExp(
      `(?<![A-Za-z0-9_-])(?:${ids.map(escapeRe).join("|")})(?![A-Za-z0-9_-])`,
      "g",
    );
    GLOSS_RE.set(key, re);
  }
  const out: Array<{ t: string; id?: string }> = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    const at = m.index ?? 0;
    if (at > last) out.push({ t: text.slice(last, at) });
    out.push({ t: m[0], id: m[0] });
    last = at + m[0].length;
  }
  if (last < text.length) out.push({ t: text.slice(last) });
  return out;
}

/** OTHER_REPO_NOTE is shown on a website-folder prompt instead of commit links. */
export const OTHER_REPO_NOTE = "commits in the website repo are not linked";

/** Labels of the commit evidence kinds (commitsBasis). */
export const BASIS_LABEL: Record<string, string> = {
  "session-window": "made in this prompt's session before its next prompt",
  "time-window-branch": "made on this prompt's branch before its next prompt",
};

/** The badges of a row, in display order. */
export function badges(r: LogRow): string[] {
  const out: string[] = [];
  if (r.decision) out.push("decision");
  if (r.kind === "command") out.push("command");
  if (r.settingsSource === "unknown") out.push("settings unknown");
  if (r.pasted === "lost") out.push("pasted text not kept");
  if (r.website) out.push("website folder");
  if (r.articleWork) out.push("article work");
  if (r.text.includes(REDACTED_MARK)) out.push("redacted");
  return out;
}

/** Returns the size of n bytes in words: "443 KB". */
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** A measured height change of one rendered row. */
export type RowResize = { id: string; top: number; oldH: number; newH: number };

/**
 * Returns how far to move a scroll box so the rows being read stay still
 * after rows changed height: the sum of the changes of rows that lay wholly
 * above the visible top (old bottom at or above scrollTop). A row partly in
 * view, and the row the reader just expanded or collapsed (skip), do not
 * move the box.
 */
export function scrollShift(
  changes: RowResize[],
  scrollTop: number,
  skip?: string | null,
): number {
  let shift = 0;
  for (const c of changes) {
    if (c.id === skip) continue;
    if (c.top + c.oldH <= scrollTop) shift += c.newH - c.oldH;
  }
  return shift;
}

/**
 * Returns the filter f with only the conditions that hide row r relaxed:
 * the day range widened to its day, its lane added, and each other
 * condition it fails (decision answers only, hidden article work, model,
 * effort, phase, search) dropped. Conditions the row passes stay.
 */
export function revealFilter(f: Filter, r: Row): Filter {
  const out: Filter = { ...f, lanes: new Set(f.lanes) };
  out.from = Math.min(f.from, r.day);
  out.to = Math.max(f.to, r.day);
  out.lanes.add(r.lane);
  if (f.decisionsOnly && !r.decision) out.decisionsOnly = false;
  if (f.hideArticle && r.articleWork) out.hideArticle = false;
  /** Returns whether a setting passes a model/effort condition ("unknown" matches null). */
  const passes = (v: string | null, want?: string) =>
    !want || (want === "unknown" ? v === null : v === want);
  if (!passes(r.model, f.model)) out.model = undefined;
  if (!passes(r.effort, f.effort)) out.effort = undefined;
  if (f.phase && f.phase !== r.phase) out.phase = undefined;
  if (f.query && !matchesQuery(r, f.query)) out.query = undefined;
  if (f.area && f.area !== r.area) out.area = undefined;
  if (f.session && f.session !== r.session) out.session = undefined;
  if (f.categories && r.category && !f.categories.has(r.category)) {
    out.categories = undefined;
  }
  return out;
}

/** One session card of the log: its rows passing the filter (in order) and its prompt count. */
export type Card = {
  key: string;
  /** The session's rows passing the filter, in time order. */
  rows: Row[];
  /** All of the session's prompts in the log. */
  total: number;
  /** Whether the session holds only commands or shell escapes. */
  commandOnly: boolean;
};

/**
 * Returns the session cards of the log: one per session with at least one
 * row in shown (the rows passing the filter), in the order of each session's
 * first prompt in all, each with its shown rows in order and its whole
 * prompt count. isCommandOnly tells the command-only sessions.
 */
export function groupBySession(
  all: Row[],
  shown: Row[],
  isCommandOnly: (key: string) => boolean,
): Card[] {
  const total = new Map<string, number>();
  const order: string[] = [];
  for (const r of all) {
    if (!total.has(r.session)) order.push(r.session);
    total.set(r.session, (total.get(r.session) ?? 0) + 1);
  }
  const kept = new Map<string, Row[]>();
  for (const r of shown) {
    const list = kept.get(r.session);
    if (list) list.push(r);
    else kept.set(r.session, [r]);
  }
  return order
    .filter((k) => kept.has(k))
    .map((key) => ({
      key,
      rows: kept.get(key) as Row[],
      total: total.get(key) ?? 0,
      commandOnly: isCommandOnly(key),
    }));
}

/** One item of the session-card list: a card, the command-only group, or a prompt of an open card. */
export type Entry =
  | { kind: "card"; id: string; card: Card; open: boolean }
  | {
      kind: "group";
      id: string;
      /** Command-only sessions with prompts passing the filter. */
      cards: number;
      /** Their prompts passing the filter. */
      prompts: number;
      open: boolean;
    }
  | { kind: "row"; id: string; row: Row; card: string };

/** GROUP_ID is the entry id of the command-only group. */
export const GROUP_ID = "group:command-only";

/**
 * Returns the flat list the session-card log renders: the cards of sessions
 * with an area, each followed by its rows when open; then one collapsed group
 * of the command-only sessions (when any has rows, or when showEmptyGroup),
 * followed when open by their cards and open rows.
 */
export function cardEntries(
  cards: Card[],
  open: Set<string>,
  groupOpen: boolean,
  showEmptyGroup = false,
): Entry[] {
  const out: Entry[] = [];
  /** Appends a card and, when open, its rows. */
  const push = (c: Card) => {
    const isOpen = open.has(c.key);
    out.push({ kind: "card", id: `card:${c.key}`, card: c, open: isOpen });
    if (isOpen) {
      for (const r of c.rows)
        out.push({ kind: "row", id: r.id, row: r, card: c.key });
    }
  };
  const co = cards.filter((c) => c.commandOnly);
  for (const c of cards) if (!c.commandOnly) push(c);
  if (co.length > 0 || showEmptyGroup) {
    out.push({
      kind: "group",
      id: GROUP_ID,
      cards: co.length,
      prompts: co.reduce((a, c) => a + c.rows.length, 0),
      open: groupOpen,
    });
    if (groupOpen) for (const c of co) push(c);
  }
  return out;
}

/**
 * Returns what must open so session key's card shows: the card itself, and
 * the command-only group when the card is in it; null when no card of key
 * passes the filter.
 */
export function revealCard(
  cards: Card[],
  key: string,
): { card: string; group: boolean } | null {
  const c = cards.find((x) => x.key === key);
  return c ? { card: c.key, group: c.commandOnly } : null;
}

/** Returns the index of the entry with id, or -1. */
export function entryIndex(entries: Entry[], id: string): number {
  return entries.findIndex((e) => e.id === id);
}

/** An entry's place in the nested list: its level and its position among its siblings. */
export type EntryPlace = { level: number; setsize: number; posinset: number };

/**
 * Returns each entry's place in the nested list the flat entries stand for:
 * level 1 holds the cards of sessions with an area and the command-only
 * group (or, in the flat view, the prompts); the prompts of an open card sit
 * one level below their card, and the cards of an open command-only group
 * one level below the group, so a command-only card's prompts are at level 3.
 */
export function entryPlaces(entries: Entry[]): EntryPlace[] {
  const coCard = new Set<string>();
  for (const e of entries) {
    if (e.kind === "card" && e.card.commandOnly) coCard.add(e.card.key);
  }
  const level = entries.map((e) => {
    if (e.kind === "row") {
      if (!e.card) return 1;
      return coCard.has(e.card) ? 3 : 2;
    }
    if (e.kind === "card") return e.card.commandOnly ? 2 : 1;
    return 1;
  });
  /** Returns the key of an entry's parent ("" at the top level). */
  const parent = (e: Entry): string =>
    e.kind === "row"
      ? e.card
      : e.kind === "card" && e.card.commandOnly
        ? GROUP_ID
        : "";
  const size = new Map<string, number>();
  for (const e of entries) size.set(parent(e), (size.get(parent(e)) ?? 0) + 1);
  const seen = new Map<string, number>();
  return entries.map((e, k) => {
    const p = parent(e);
    const pos = (seen.get(p) ?? 0) + 1;
    seen.set(p, pos);
    return { level: level[k], setsize: size.get(p) ?? 1, posinset: pos };
  });
}

/** Returns the element id of a prompt's list item (the target of its card's aria-controls). */
export function rowDomId(id: string): string {
  return `ep-row-${id.replace(/[^A-Za-z0-9_-]/g, "_")}`;
}
