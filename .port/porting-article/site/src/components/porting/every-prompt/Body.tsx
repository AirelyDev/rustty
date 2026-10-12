"use client";

import data from "@/app/porting/data/every-prompt.json";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { fmtInt } from "../lib/format";
import s from "./EveryPrompt.module.css";
import {
  applyFilter,
  commitRange,
  dayCounts,
  dayDate,
  decodeFilter,
  decodeTicks,
  defaultFilter,
  type EpData,
  encodeFilter,
  FRAGMENT,
  type Filter,
  isDefault,
  LANE_NAMES,
  longest,
  CATEGORY_DEFAULT,
  categoriesLabel,
  COMMAND_CATEGORIES,
  matchesQuery,
  resolveAreaSession,
  sameCategories,
  modelLabel,
  shortDate,
  tiles,
  weeksFromTicks,
} from "./model";
import SessionTiles from "./SessionTiles";
import SessionTimeline from "./SessionTimeline";
import SettingsView from "./SettingsView";
import {
  dayRange,
  decodeSessions,
  revealSession,
  type Session,
  sessionsInRange,
  sessionTiles,
} from "./session-model";
import type { LogMode } from "./SessionCards";
import Log, { type CardJump, type Jump, type LoadedLog } from "./Log";
import { filterRows, topRepeated } from "./log-model";
import Tiles from "./Tiles";
import Timeline from "./Timeline";

/** DATA is the widget's bundled summary, typed for the view-model. */
const DATA = data as unknown as EpData;

/** TICKS are the decoded prompts of the summary (index = log row index). */
const TICKS = decodeTicks(DATA);

/** SESSIONS are the decoded sessions of the summary (index = tick session index). */
const SESSIONS = decodeSessions(DATA);

/** SESSION_BY_KEY finds a session by its key. */
const SESSION_BY_KEY = new Map(SESSIONS.map((x) => [x.key, x]));

/** N_DAYS is the number of days in the window. */
const N_DAYS = DATA.commitsPerDay.length;

/** DEFAULTS is the unfiltered view. */
const DEFAULTS = defaultFilter(
  N_DAYS,
  DATA.lanes.length,
  DATA.categories ? CATEGORY_DEFAULT : undefined,
);

/** ALLOWED are the models, efforts and phases a linked filter may name. */
const ALLOWED = {
  models: DATA.models,
  efforts: DATA.efforts,
  phases: DATA.phases.map((p) => p.key),
  areas: DATA.areas ?? [],
  sessions: SESSIONS.map((x) => x.key),
  sessionAreas: Object.fromEntries(SESSIONS.map((x) => [x.key, x.area])),
  categories: DATA.categories ?? [],
};

/** CATEGORIES are the data's prompt categories in order. */
const CATEGORIES = DATA.categories ?? [];

/** Returns f's fragment (left without the default categories). */
function enc(f: Filter): string {
  return encodeFilter(f, DEFAULTS);
}

/** Returns the title of session key (the key itself when unknown). */
function titleOf(key: string): string {
  return SESSION_BY_KEY.get(key)?.title ?? key;
}

/** Returns the note shown when an area drops the selected session. */
function areaDropNote(key: string, area: string): string {
  return `Session “${titleOf(key)}” is not in area ${area}, so it was cleared.`;
}

/** COUNTS are per-day, per-lane counts of all prompts (for the day tooltips). */
const COUNTS = dayCounts(TICKS, N_DAYS, DATA.lanes.length);

/** HASH_DEBOUNCE_MS is the trailing delay of URL fragment writes. */
const HASH_DEBOUNCE_MS = 250;

/** REPEATED_TOP is how many repeated prompts the tile lists. */
const REPEATED_TOP = 5;

/**
 * Reads the filter from the current URL fragment (defaults when it holds
 * none), with the session dropped when the fragment's area excludes it; note
 * says so then.
 */
function filterFromHash(): { filter: Filter; note: string } {
  const filter = decodeFilter(window.location.hash, DEFAULTS, ALLOWED);
  const loose = decodeFilter(window.location.hash, DEFAULTS, {
    ...ALLOWED,
    sessionAreas: undefined,
  });
  const note =
    loose.session && !filter.session && filter.area
      ? areaDropNote(loose.session, filter.area)
      : "";
  return { filter, note };
}

/** Returns the active conditions of f besides the day range, in words. */
function conditions(f: Filter): string[] {
  const out: string[] = [];
  if (f.lanes.size < DATA.lanes.length) {
    out.push(
      f.lanes.size === 0
        ? "no lanes"
        : [...f.lanes]
            .sort()
            .map((l) => LANE_NAMES[l])
            .join(" + "),
    );
  }
  if (f.decisionsOnly) out.push("decision answers only");
  if (f.hideArticle) out.push("article work hidden");
  if (f.model)
    out.push(
      `model ${f.model === "unknown" ? "unknown" : modelLabel(f.model)}`,
    );
  if (f.effort) out.push(`effort ${f.effort}`);
  if (f.phase)
    out.push(DATA.phases.find((p) => p.key === f.phase)?.label ?? f.phase);
  if (f.query) out.push(`search “${f.query}”`);
  if (f.area) out.push(`area ${f.area}`);
  if (f.session) {
    const x = SESSION_BY_KEY.get(f.session);
    out.push(
      `session “${x?.title ?? f.session}”${x?.commandsOnly ? " (only commands or shell escapes)" : ""}`,
    );
  }
  return out;
}

/**
 * Returns what revealing a session changed besides selecting it, in words
 * ("" when nothing else changed): the widened days and the dropped conditions.
 */
function relaxed(before: Filter, after: Filter): string {
  const out: string[] = [];
  if (after.from !== before.from || after.to !== before.to) {
    out.push(
      `days widened to ${shortDate(dayDate(DATA.window[0], after.from))} – ${shortDate(dayDate(DATA.window[0], after.to))}`,
    );
  }
  if (after.lanes.size !== before.lanes.size) out.push("lanes widened");
  if (!sameCategories(before.categories, after.categories)) {
    out.push("commands and shell escapes shown");
  }
  // Compared with the same lanes and no session, so only dropped conditions differ.
  const same = { lanes: after.lanes, session: undefined };
  const keep = new Set(conditions({ ...after, ...same }));
  for (const c of conditions({ ...before, ...same })) {
    if (!keep.has(c)) out.push(`“${c}” dropped`);
  }
  return out.length ? `To show the session: ${out.join(", ")}.` : "";
}

/**
 * The "Every prompt" widget: a timeline strip of every human prompt (brush
 * or range inputs narrow the days), live tiles for the current filter, the
 * weekly settings view, and the full log (fetched on demand). The filter
 * is mirrored in the URL fragment ("#every-prompt?…") with replaceState, so
 * a view can be linked without the page scrolling.
 */
export default function Body() {
  const [filter, setFilter] = useState<Filter>(DEFAULTS);
  // The log view (loaded on demand) fills this; until then search and
  // "most repeated" wait for it.
  const [log, setLog] = useState<LoadedLog | null>(null);
  const [logWanted, setLogWanted] = useState(false);
  const [jump, setJump] = useState<Jump | null>(null);
  const [cardJump, setCardJump] = useState<CardJump | null>(null);
  // The session last opened from the timeline, a tile or a link, with the
  // filter it opened under: the live region names it once, until the filter
  // changes again.
  const [opened, setOpened] = useState<{ key: string; at: string } | null>(
    null,
  );
  // The log's view (session cards or all prompts), for the live region's wording.
  const [logMode, setLogMode] = useState<LogMode>("sessions");
  // What selecting a session relaxed (or what an area dropped), shown while the filter it produced stays.
  const [relaxNote, setRelaxNote] = useState<{
    key: string;
    text: string;
  } | null>(null);
  const touched = useRef(false);

  // Adopt a filter from the URL on mount and when the fragment changes. A
  // bare "#every-prompt" (the table of contents, a plain section link) means
  // the whole log, so the view resets and agrees with the URL.
  useEffect(() => {
    /** Applies the fragment's filter when it is one of ours. */
    const read = () => {
      const hash = window.location.hash;
      if (hash.startsWith(`#${FRAGMENT}?`)) {
        const { filter: linked, note: areaNote } = filterFromHash();
        // A linked session the rest of the link hides (a command-only session
        // under the default categories, say) is revealed, with a note.
        const x = linked.session
          ? SESSION_BY_KEY.get(linked.session)
          : undefined;
        const f = x ? revealSession(linked, x, TICKS, DATA.phases) : linked;
        const n = [areaNote, x ? relaxed(linked, f) : ""]
          .filter(Boolean)
          .join(" ");
        setRelaxNote(n ? { key: enc(f), text: n } : null);
        setFilter(f);
        // A linked session opens its card in the log, which loads for it.
        if (f.session) {
          setLogWanted(true);
          setCardJump({ key: f.session, n: Date.now(), focus: true });
          setOpened({ key: f.session, at: enc(f) });
        }
      } else if (hash === `#${FRAGMENT}`) setFilter(DEFAULTS);
    };
    read();
    window.addEventListener("hashchange", read);
    return () => window.removeEventListener("hashchange", read);
  }, []);

  // Mirror the reader's changes in the URL fragment without scrolling. The
  // write is debounced (range inputs fire per step, and WebKit throws once
  // replaceState is called too often) and flushed when a pointer or key is
  // released; a failed write leaves the URL as it was.
  const pendingHash = useRef<string | null>(null);
  const hashTimer = useRef<number | null>(null);
  /** Writes the pending fragment now, if any. */
  const flushHash = useCallback(() => {
    if (hashTimer.current !== null) {
      window.clearTimeout(hashTimer.current);
      hashTimer.current = null;
    }
    const hash = pendingHash.current;
    pendingHash.current = null;
    if (hash === null || window.location.hash === hash) return;
    try {
      const url = `${window.location.pathname}${window.location.search}${hash}`;
      window.history.replaceState(window.history.state, "", url);
    } catch {
      // Rate-limited or blocked: the filter still works, only the link is stale.
    }
  }, []);
  useEffect(() => {
    if (!touched.current) return;
    pendingHash.current = isDefault(filter, DEFAULTS)
      ? `#${FRAGMENT}`
      : `#${enc(filter)}`;
    if (hashTimer.current !== null) window.clearTimeout(hashTimer.current);
    hashTimer.current = window.setTimeout(flushHash, HASH_DEBOUNCE_MS);
  }, [filter, flushHash]);
  // Flush a pending write when the widget unmounts.
  useEffect(() => flushHash, [flushHash]);

  /** Updates the filter from a reader action (and so the URL). */
  const update = useCallback((next: Filter) => {
    touched.current = true;
    setFilter(next);
  }, []);

  const match = useMemo(() => {
    const q = filter.query;
    if (!log || !q) return undefined;
    return (i: number) => {
      const row = log.rows[i];
      return row ? matchesQuery(row, q) : false;
    };
  }, [log, filter.query]);

  // filtered: the prompts shown (the category choice applies); everyKind: the
  // same filter over every category, for the all-inclusive headline tiles.
  const filtered = useMemo(
    () => applyFilter(TICKS, filter, match),
    [filter, match],
  );
  const noCats = useMemo(
    () => ({ ...filter, categories: undefined }),
    [filter],
  );
  const everyKind = useMemo(
    () => applyFilter(TICKS, noCats, match),
    [noCats, match],
  );
  const shown = useMemo(() => new Set(filtered.map((t) => t.i)), [filtered]);
  const values = useMemo(
    () =>
      tiles(everyKind, commitRange(noCats, DATA.phases, DATA.commitsPerDay)),
    [everyKind, noCats],
  );
  const kindValues = useMemo(() => tiles(filtered), [filtered]);
  const long = useMemo(() => longest(everyKind), [everyKind]);
  const hidden = everyKind.length - filtered.length;
  const hiddenWord = CATEGORIES.filter((c) => !filter.categories?.has(c)).every(
    (c) => COMMAND_CATEGORIES.includes(c),
  )
    ? "commands"
    : "prompts";
  const kindLabel = categoriesLabel(filter.categories, CATEGORIES);
  const rows = useMemo(
    () => (log ? filterRows(log.rows, filter) : null),
    [log, filter],
  );
  const repeated = useMemo(
    () => (rows ? topRepeated(rows, REPEATED_TOP) : null),
    [rows],
  );
  const want = useCallback(() => setLogWanted(true), []);
  const keptSessions = useMemo(
    () => sessionsInRange(SESSIONS, filter, DATA.phases),
    [filter],
  );
  const kept = useMemo(
    () => new Set(keptSessions.map((x) => x.key)),
    [keptSessions],
  );
  const sValues = useMemo(
    () =>
      sessionTiles(
        keptSessions,
        TICKS,
        DATA.areas ?? [],
        dayRange(filter, DATA.phases),
      ),
    [keptSessions, filter],
  );
  const weeks = useMemo(() => weeksFromTicks(filtered, DATA), [filtered]);

  /** Sets or clears the area; a selected session outside the new area is cleared, with a note. */
  const onArea = useCallback(
    (area: string | undefined) => {
      const { filter: next, dropped } = resolveAreaSession(
        { ...filter, area },
        (k) => SESSION_BY_KEY.get(k)?.area,
      );
      setRelaxNote(
        dropped && area
          ? { key: enc(next), text: areaDropNote(dropped, area) }
          : null,
      );
      update(next);
    },
    [filter, update],
  );

  /**
   * Shows session x: selects it (relaxing only the conditions that hide it,
   * with a visible note of what changed) unless it is already selected, then
   * opens its card in the log (loading the log if needed) and moves focus
   * there; the live region names the opened card.
   */
  const showSession = useCallback(
    (x: Session) => {
      let f = filter;
      if (filter.session !== x.key) {
        f = revealSession(filter, x, TICKS, DATA.phases, match);
        const text = relaxed(filter, f);
        setRelaxNote(text ? { key: enc(f), text } : null);
        update(f);
      }
      setOpened({ key: x.key, at: enc(f) });
      setLogWanted(true);
      setCardJump({ key: x.key, n: Date.now(), focus: true });
    },
    [filter, match, update],
  );
  /** A timeline bar toggles: selecting shows the session; selecting it again clears it. */
  const onSelect = useCallback(
    (x: Session) => {
      if (filter.session === x.key) update({ ...filter, session: undefined });
      else showSession(x);
    },
    [filter, update, showSession],
  );
  const note = relaxNote && relaxNote.key === enc(filter) ? relaxNote.text : "";
  const whole = isDefault(filter, DEFAULTS);
  // The headline tiles ignore the category choice.
  const wholeEveryKind = isDefault(
    { ...filter, categories: DEFAULTS.categories },
    DEFAULTS,
  );
  const conds = conditions(filter);

  // The screen-reader status follows the filter after the same pause as the
  // URL, so dragging a range does not queue an announcement per step.
  const selected = filter.session
    ? SESSION_BY_KEY.get(filter.session)
    : undefined;
  // Named once, right after the session was opened: a later filter change drops it.
  const justOpened =
    selected && opened?.key === selected.key && opened.at === enc(filter);
  const shownText = `${fmtInt(filtered.length)} prompt${filtered.length === 1 ? "" : "s"} shown.`;
  const openedText = !justOpened
    ? ""
    : logMode === "sessions"
      ? ` Session “${selected.title}” opened in the log, ${shownText}`
      : ` Filtered the log to session “${selected.title}”, ${shownText}`;
  const statusText = `Showing ${fmtInt(filtered.length)} of ${fmtInt(DATA.totals.prompts)} prompts${hidden > 0 ? `, ${fmtInt(hidden)} ${hiddenWord} hidden` : ""}, ${shortDate(dayDate(DATA.window[0], filter.from))} to ${shortDate(dayDate(DATA.window[0], filter.to))}${selected ? `, session “${selected.title}”${selected.commandsOnly ? " (only commands or shell escapes)" : ""}` : ""}.${note ? ` ${note}` : ""}${openedText}`;
  const [spoken, setSpoken] = useState(statusText);
  useEffect(() => {
    const id = window.setTimeout(() => setSpoken(statusText), HASH_DEBOUNCE_MS);
    return () => window.clearTimeout(id);
  }, [statusText]);

  /** Shows prompt i in the log (loading it first if needed): scrolls to it and highlights it. */
  const onLongest = (i: number) => {
    setLogWanted(true);
    setJump({ i, n: Date.now() });
  };

  const start = DATA.window[0];
  return (
    <div
      className={s.root}
      onPointerUpCapture={flushHash}
      onKeyUpCapture={flushHash}
    >
      <div className={s.top}>
        <div className={s.big}>
          <b>{fmtInt(DATA.totals.prompts)}</b> prompts
          <small>
            {Object.entries(DATA.totals.bySource)
              .map(
                ([k, n]) => `${LANE_NAMES[DATA.lanes.indexOf(k)]} ${fmtInt(n)}`,
              )
              .join(" · ")}
          </small>
        </div>
        <p className={s.srOnly} aria-live="polite">
          {spoken}
        </p>
        <div className={s.status}>
          <span className={s.showing}>
            Showing {fmtInt(filtered.length)} of {fmtInt(DATA.totals.prompts)}{" "}
            prompts
            {hidden > 0 ? (
              <>
                {" "}
                · {fmtInt(hidden)} {hiddenWord} hidden ·{" "}
                <button
                  type="button"
                  className={s.linkBtn}
                  onClick={() => update({ ...filter, categories: undefined })}
                >
                  show all
                </button>
              </>
            ) : null}
          </span>
          <span>
            {shortDate(dayDate(start, filter.from))} –{" "}
            {shortDate(dayDate(start, filter.to))} ·{" "}
            {filter.to - filter.from + 1} days
            {conds.length ? ` · ${conds.join(" · ")}` : ""}
          </span>
          {filter.query && !log ? (
            <span className={s.statusNote}>
              the search applies once the log is open
            </span>
          ) : null}
          {note ? <span className={s.statusNote}>{note}</span> : null}
          {!whole ? (
            <button
              type="button"
              className={s.btn}
              onClick={() => update(DEFAULTS)}
            >
              Clear filter
            </button>
          ) : null}
        </div>
      </div>
      <SessionTimeline
        data={DATA}
        sessions={SESSIONS}
        kept={kept}
        filter={filter}
        onSelect={onSelect}
        onArea={onArea}
        onCommandOnly={() =>
          update({ ...filter, commandOnly: !filter.commandOnly || undefined })
        }
      />
      <Timeline
        data={DATA}
        ticks={TICKS}
        shown={shown}
        filter={filter}
        counts={COUNTS}
        onRange={(from, to) => update({ ...filter, from, to })}
      />
      <SessionTiles
        data={DATA}
        values={sValues}
        whole={keptSessions.length === SESSIONS.length}
        onSelect={showSession}
        onDay={(day) => update({ ...filter, from: day, to: day })}
      />
      <Tiles
        data={DATA}
        values={values}
        kindValues={kindValues}
        kindLabel={kindLabel}
        whole={wholeEveryKind}
        longest={long}
        repeated={repeated}
        onLongest={onLongest}
      />
      <SettingsView
        data={DATA}
        weeks={weeks}
        whole={whole}
        kindLabel={kindLabel}
      />
      <Log
        data={DATA}
        filter={filter}
        onFilter={update}
        log={log}
        onLoaded={setLog}
        rows={rows}
        matched={filtered.length}
        wanted={logWanted}
        onWant={want}
        jump={jump}
        sessions={SESSIONS}
        cardJump={cardJump}
        mode={logMode}
        onMode={setLogMode}
      />
    </div>
  );
}
