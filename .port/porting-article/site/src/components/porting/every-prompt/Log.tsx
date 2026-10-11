"use client";

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { useInView } from "react-intersection-observer";
import { fmtInt } from "../lib/format";
import useReducedMotion from "../ui/useReducedMotion";
import { WIDGET_SCROLL_EVENT } from "../ui/WidgetHashScroll";
import s from "./EveryPrompt.module.css";
import {
  cardEntries,
  decodeLog,
  type Entry,
  entryIndex,
  entryPlaces,
  fmtBytes,
  fmtPacific,
  groupBySession,
  type LogSession,
  type RowResize,
  type Row,
  revealCard,
  revealFilter,
  rowDomId,
  scrollShift,
} from "./log-model";
import LogRow from "./LogRow";
import {
  CATEGORY_NAMES,
  COMMAND_CATEGORIES,
  type EpData,
  type Filter,
  LANE_COLOURS,
  LANE_NAMES,
  modelLabel,
  UNKNOWN,
} from "./model";
import SessionCard from "./SessionCard";
import { CommandOnlyGroup, type LogMode, ModeToggle } from "./SessionCards";
import type { Session } from "./session-model";

/** The loaded log: decoded rows (index = tick index) and the id glossary. */
export type LoadedLog = {
  rows: Row[];
  glossary: Record<string, string>;
  sessions?: Record<string, LogSession>;
};

/** A request to show one prompt in the log; n tells repeated requests apart. */
export type Jump = { i: number; n: number };

/**
 * A request to show one session's card (from the timeline, a tile or a
 * link): it opens; with focus, the page scrolls to the log and focus moves
 * to the card. n tells repeated requests apart.
 */
export type CardJump = { key: string; n: number; focus: boolean };

/** HIGHLIGHT_MS is how long a jumped-to prompt or card stays highlighted. */
const HIGHLIGHT_MS = 4000;

/** EST_ROW is the height assumed for a row not yet measured (px). */
const EST_ROW = 104;

/** OVERSCAN is how far beyond the visible box rows are kept rendered (px). */
const OVERSCAN = 900;

/** Props of the log section. */
type LogProps = {
  data: EpData;
  filter: Filter;
  onFilter: (f: Filter) => void;
  log: LoadedLog | null;
  onLoaded: (log: LoadedLog) => void;
  /** The rows passing the filter (null before the log has loaded). */
  rows: Row[] | null;
  /** How many prompts pass the filter by the timeline (the search waits for the log). */
  matched: number;
  /** Whether something asked for the log (the longest-prompt jump). */
  wanted: boolean;
  onWant: () => void;
  jump: Jump | null;
  /** The summary's sessions (index = tick session index). */
  sessions: Session[];
  cardJump: CardJump | null;
  /** The view: session cards or all prompts (held by the widget for its live region). */
  mode: LogMode;
  onMode: (m: LogMode) => void;
};

/** Fetch states of the log file. */
type Status = "idle" | "loading" | "error" | "done";

/**
 * The full log of the "Every prompt" widget. The log file is fetched when
 * the section comes near the viewport, or when the reader presses "Read
 * every prompt", searches, or jumps to a prompt. It shows the filter
 * controls (shared with the timeline through the URL fragment), the match
 * count and a virtualized list of rows inside its own scroll box.
 */
export default function Log({
  data,
  filter,
  onFilter,
  log,
  onLoaded,
  rows,
  matched,
  wanted,
  onWant,
  jump,
  sessions,
  cardJump,
  mode,
  onMode: setMode,
}: LogProps) {
  const [openCards, setOpenCards] = useState<Set<string>>(() => new Set());
  const [groupOpen, setGroupOpen] = useState(false);
  const sessionByKey = useMemo(
    () => new Map(sessions.map((x) => [x.key, x])),
    [sessions],
  );
  const cards = useMemo(
    () =>
      log && rows
        ? groupBySession(
            log.rows,
            rows,
            (k) => sessionByKey.get(k)?.commandsOnly ?? false,
          )
        : [],
    [log, rows, sessionByKey],
  );
  /** Opens session key's card (and the command-only group when it is in it). */
  const openCard = useCallback(
    (key: string) => {
      const r = revealCard(cards, key);
      if (!r) return;
      setOpenCards((m) => (m.has(r.card) ? m : new Set(m).add(r.card)));
      if (r.group) setGroupOpen(true);
    },
    [cards],
  );
  // A selected session's card opens by itself.
  useEffect(() => {
    if (filter.session) openCard(filter.session);
  }, [filter.session, openCard]);
  // A card jump opens its card.
  useEffect(() => {
    if (cardJump) openCard(cardJump.key);
  }, [cardJump, openCard]);
  /** Expands or collapses a card. */
  const onToggleCard = useCallback((key: string) => {
    setOpenCards((m) => {
      const next = new Set(m);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);
  const commandsHidden =
    filter.categories !== undefined &&
    COMMAND_CATEGORIES.every((c) => !filter.categories?.has(c));
  const entries = useMemo<Entry[]>(
    () =>
      mode === "sessions"
        ? cardEntries(
            cards,
            openCards,
            groupOpen,
            commandsHidden && !filter.session,
          )
        : (rows ?? []).map((r) => ({
            kind: "row" as const,
            id: r.id,
            row: r,
            card: "",
          })),
    [mode, cards, openCards, groupOpen, commandsHidden, filter.session, rows],
  );
  // A jump the filter hides shows a note (until done or dismissed) instead.
  const [dismissed, setDismissed] = useState<number | null>(null);
  const [jumped, setJumped] = useState<number | null>(null);
  const live = jump && dismissed !== jump.n ? jump : null;
  const jumpRow = live && log ? log.rows[live.i] : undefined;
  const jumpHidden =
    live !== null &&
    jumped !== live.n &&
    jumpRow !== undefined &&
    rows !== null &&
    !rows.some((r) => r.i === live.i);
  const [status, setStatus] = useState<Status>(log ? "done" : "idle");
  const [error, setError] = useState("");
  const sectionRef = useRef<HTMLElement>(null);
  const noteRef = useRef<HTMLOutputElement>(null);
  const headRef = useRef<HTMLHeadingElement>(null);
  // Set when the reader asked for the log with the button: once it loads, focus
  // moves to the log's heading instead of falling back to the page.
  const focusOnLoad = useRef(false);
  const reducedMotion = useReducedMotion();
  const noteJump = jumpHidden && live ? live.n : null;
  // In the session view a prompt jump opens the card holding the prompt.
  const jumpSession = jumpRow?.session;
  useEffect(() => {
    if (mode === "sessions" && jumpSession) openCard(jumpSession);
  }, [mode, jumpSession, openCard]);
  // A jump the filter hides lands on its note, so the reader sees why nothing scrolled.
  useEffect(() => {
    if (noteJump === null) return;
    noteRef.current?.scrollIntoView({
      block: "center",
      behavior: reducedMotion ? "auto" : "smooth",
    });
  }, [noteJump, reducedMotion]);
  const { ref: viewRef, inView } = useInView({
    rootMargin: "300px 0px",
    triggerOnce: true,
    fallbackInView: true,
  });
  const abort = useRef<AbortController | null>(null);

  /** Fetches and decodes the log file. */
  const load = useCallback(async () => {
    abort.current?.abort();
    const ctl = new AbortController();
    abort.current = ctl;
    setStatus("loading");
    setError("");
    try {
      const res = await fetch(data.logUrl, { signal: ctl.signal });
      if (!res.ok) throw new Error(`the server answered ${res.status}`);
      const decoded = decodeLog(await res.json(), data.window[0], data.lanes);
      if (decoded.rows.length !== data.ticks.length) {
        throw new Error("the log does not match this page; reload it");
      }
      onLoaded(decoded);
      setStatus("done");
    } catch (e) {
      if (ctl.signal.aborted) return;
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  }, [data, onLoaded]);
  useEffect(() => () => abort.current?.abort(), []);

  useEffect(() => {
    if (status === "idle" && !log && (inView || wanted)) void load();
  }, [status, log, inView, wanted, load]);

  useEffect(() => {
    if (status !== "done" || !focusOnLoad.current) return;
    focusOnLoad.current = false;
    headRef.current?.focus({ preventScroll: true });
  }, [status]);

  /** Sets one field of the filter. */
  const set = (patch: Partial<Filter>) => onFilter({ ...filter, ...patch });

  const n = data.totals.prompts;
  return (
    <section
      ref={(el) => {
        sectionRef.current = el;
        viewRef(el);
      }}
      className={s.log}
      aria-labelledby="ep-log-head"
    >
      <div className={s.logHead}>
        <h4 id="ep-log-head" ref={headRef} tabIndex={-1}>
          The log, verbatim
        </h4>
        {status === "idle" || status === "loading" ? (
          // Stays mounted (inert while loading) so focus is not dropped on click.
          <button
            type="button"
            className={s.btn}
            aria-disabled={status === "loading"}
            onClick={() => {
              if (status !== "idle") return;
              focusOnLoad.current = true;
              onWant();
            }}
          >
            {status === "loading"
              ? "Loading every prompt…"
              : `Read every prompt (${fmtInt(n)}) · ${fmtBytes(data.logBytes)}`}
          </button>
        ) : null}
      </div>
      <FilterBar data={data} filter={filter} set={set} onSearch={onWant} />
      <div className={s.logModeRow}>
        <ModeToggle mode={mode} onMode={setMode} />
      </div>
      <p className={s.logCount}>
        {rows
          ? `${fmtInt(rows.length)} of ${fmtInt(n)} prompts shown${mode === "sessions" ? ` in ${fmtInt(cards.length)} session${cards.length === 1 ? "" : "s"}` : ""}`
          : `${fmtInt(matched)} of ${fmtInt(n)} prompts match${filter.query ? " (before the search)" : ""}`}
      </p>
      {jumpHidden && jumpRow && live ? (
        <output ref={noteRef} className={s.jumpNote}>
          The prompt of {fmtPacific(jumpRow.ts)} ({LANE_NAMES[jumpRow.lane]}) is
          hidden by the current filter.{" "}
          <button
            type="button"
            className={s.btn}
            onClick={() => onFilter(revealFilter(filter, jumpRow))}
          >
            Show it
          </button>{" "}
          <button
            type="button"
            className={s.linkBtn}
            onClick={() => setDismissed(live.n)}
          >
            dismiss
          </button>
        </output>
      ) : null}
      {status === "done" && log && rows ? (
        <RowList
          entries={entries}
          glossary={log.glossary}
          filter={filter}
          mode={mode}
          jump={live}
          onJumped={setJumped}
          cardJump={mode === "sessions" ? cardJump : null}
          section={sectionRef}
          card={{
            sessions: sessionByKey,
            info: log.sessions ?? {},
            start: data.window[0],
            onToggle: onToggleCard,
            groupTotal: data.totals.sessionsCommandOnly ?? 0,
            onToggleGroup: () => setGroupOpen((o) => !o),
            onShowAll: filter.categories
              ? () => onFilter({ ...filter, categories: undefined })
              : null,
          }}
        />
      ) : (
        <div className={s.logBox}>
          {status === "error" ? (
            <div className={s.logState} role="alert">
              The log could not be loaded ({error}).{" "}
              <button type="button" className={s.btn} onClick={load}>
                Retry
              </button>
            </div>
          ) : (
            <div className={s.logState}>
              {status === "loading"
                ? `Loading every prompt (${fmtBytes(data.logBytes)})…`
                : `The log loads when you reach it (${fmtBytes(data.logBytes)}).`}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

/** The filter controls: search, lanes, model, effort, phase and the two switches. */
function FilterBar({
  data,
  filter: f,
  set,
  onSearch,
}: {
  data: EpData;
  filter: Filter;
  set: (patch: Partial<Filter>) => void;
  onSearch: () => void;
}) {
  const cats = data.categories ?? [];
  /** Shows or hides category c (every category shown is stored as no condition). */
  const toggleCat = (c: string) => {
    const next = new Set(f.categories ?? cats);
    if (next.has(c)) next.delete(c);
    else next.add(c);
    set({
      categories:
        next.size === cats.length && cats.every((x) => next.has(x))
          ? undefined
          : next,
    });
  };
  return (
    <div className={s.filters}>
      {cats.length ? (
        <fieldset className={s.cats}>
          <legend className={s.srOnly}>Kinds of prompt shown</legend>
          {cats.map((c) => (
            <button
              key={c}
              type="button"
              className={s.chip}
              aria-pressed={!f.categories || f.categories.has(c)}
              onClick={() => toggleCat(c)}
            >
              {CATEGORY_NAMES[c] ?? c}{" "}
              <span className={s.legendN}>
                {fmtInt(data.totals.byCategory?.[c] ?? 0)}
              </span>
            </button>
          ))}
        </fieldset>
      ) : null}
      <label className={s.search}>
        <span>Search</span>
        <input
          type="search"
          value={f.query ?? ""}
          placeholder="text or prompt id"
          maxLength={200}
          onFocus={onSearch}
          onChange={(e) => {
            onSearch();
            set({ query: e.target.value || undefined });
          }}
        />
      </label>
      <fieldset className={s.lanes}>
        <legend className={s.srOnly}>Lanes</legend>
        {LANE_NAMES.map((name, l) => (
          <button
            key={name}
            type="button"
            className={s.btn}
            aria-pressed={f.lanes.has(l)}
            onClick={() => {
              const lanes = new Set(f.lanes);
              if (lanes.has(l)) lanes.delete(l);
              else lanes.add(l);
              set({ lanes });
            }}
          >
            <i
              className={s.sw}
              style={{ background: LANE_COLOURS[l] }}
              aria-hidden="true"
            />
            {name}
          </button>
        ))}
      </fieldset>
      <div className={s.selects}>
        <label>
          <span>Model</span>
          <select
            value={f.model ?? ""}
            onChange={(e) => set({ model: e.target.value || undefined })}
          >
            <option value="">all</option>
            {data.models.map((m) => (
              <option key={m} value={m}>
                {modelLabel(m)}
              </option>
            ))}
            <option value={UNKNOWN}>unknown</option>
          </select>
        </label>
        <label>
          <span>Effort</span>
          <select
            value={f.effort ?? ""}
            onChange={(e) => set({ effort: e.target.value || undefined })}
          >
            <option value="">all</option>
            {data.efforts.map((x) => (
              <option key={x} value={x}>
                {x}
              </option>
            ))}
            <option value={UNKNOWN}>unknown or not recorded</option>
          </select>
        </label>
        <label>
          <span>Phase</span>
          <select
            value={f.phase ?? ""}
            onChange={(e) => set({ phase: e.target.value || undefined })}
          >
            <option value="">all</option>
            {data.phases.map((p) => (
              <option key={p.key} value={p.key}>
                {p.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className={s.checks}>
        <label>
          <input
            type="checkbox"
            checked={f.decisionsOnly}
            onChange={(e) => set({ decisionsOnly: e.target.checked })}
          />
          decision answers only
        </label>
        <label>
          <input
            type="checkbox"
            checked={f.hideArticle}
            onChange={(e) => set({ hideArticle: e.target.checked })}
          />
          hide article work
        </label>
      </div>
    </div>
  );
}

/** Returns the index of the last offset not above y (binary search over ascending offsets). */
function rowAt(offsets: number[], y: number): number {
  let lo = 0;
  let hi = offsets.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (offsets[mid] <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** What the session cards of the list need besides their entries. */
type CardProps = {
  sessions: Map<string, Session>;
  info: Record<string, LogSession>;
  start: string;
  onToggle: (key: string) => void;
  /** All command-only sessions in the data. */
  groupTotal: number;
  onToggleGroup: () => void;
  /** Shows every category (null when every category is shown). */
  onShowAll: (() => void) | null;
};

/** EST_CARD is the height assumed for a card header not yet measured (px). */
const EST_CARD = 92;

/**
 * The virtualized list in its own scroll box: prompts (the flat view), or
 * session cards with the prompts of the open ones and the command-only
 * group (the session view). Items start at an estimated height and are
 * measured once rendered; an item above the visible box that changes height
 * moves the scroll position by the same amount, so the items being read
 * stay still. A prompt jump scrolls the page to the log, then the box to the
 * row, expands it, highlights it and moves focus to it; a card jump scrolls
 * the box to the card and, when asked, the page to the log and focus to the
 * card's button.
 */
function RowList({
  entries,
  glossary,
  filter,
  mode,
  jump,
  onJumped,
  cardJump,
  section,
  card,
}: {
  entries: Entry[];
  glossary: Record<string, string>;
  filter: Filter;
  mode: LogMode;
  jump: Jump | null;
  /** Called with the jump's n once its row has been shown. */
  onJumped: (n: number) => void;
  cardJump: CardJump | null;
  section: React.RefObject<HTMLElement | null>;
  card: CardProps;
}) {
  const rows = entries;
  const box = useRef<HTMLElement>(null);
  const heights = useRef(new Map<string, number>());
  const [, remeasured] = useReducer((x: number) => x + 1, 0);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(640);
  const [open, setOpen] = useState<Map<string, boolean>>(() => new Map());
  const [highlight, setHighlight] = useState<number | null>(null);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  // The row the reader just expanded or collapsed: its own growth never moves the box.
  const toggled = useRef<string | null>(null);
  const reduced = useReducedMotion();
  /** Returns the scroll box, the boundary of the rows' tooltips. */
  const boundary = useCallback(() => box.current, []);
  /** Expands or collapses row id. */
  const onToggle = useCallback((id: string) => {
    toggled.current = id;
    setOpen((m) => new Map(m).set(id, !m.get(id)));
  }, []);

  // Item tops from measured heights (estimated until an item has been rendered).
  const live = new Array<number>(rows.length + 1);
  live[0] = 0;
  for (let k = 0; k < rows.length; k++) {
    const e = rows[k];
    live[k + 1] =
      live[k] +
      (heights.current.get(e.id) ?? (e.kind === "row" ? EST_ROW : EST_CARD));
  }
  const offsetsRef = useRef(live);
  offsetsRef.current = live;

  // Measure rendered rows; keep the reading position when rows above it change.
  const ro = useRef<ResizeObserver | null>(null);
  /** Returns the shared row observer, creating it on first use. */
  const observer = useCallback(() => {
    if (ro.current || typeof ResizeObserver === "undefined") return ro.current;
    ro.current = new ResizeObserver((entries) => {
      const el = box.current;
      const changes: RowResize[] = [];
      for (const e of entries) {
        const t = e.target as HTMLElement;
        const id = t.dataset.id;
        if (!id || !t.isConnected) continue;
        const h = t.offsetHeight;
        const old =
          heights.current.get(id) ??
          (id.startsWith("card:") || id.startsWith("group:")
            ? EST_CARD
            : EST_ROW);
        if (h === old || h === 0) continue;
        heights.current.set(id, h);
        const top = offsetsRef.current[Number(t.dataset.k)] ?? 0;
        changes.push({ id, top, oldH: old, newH: h });
      }
      if (changes.length === 0) return;
      const skip = toggled.current;
      if (changes.some((c) => c.id === skip)) toggled.current = null;
      const shift = el ? scrollShift(changes, el.scrollTop, skip) : 0;
      if (shift && el) el.scrollTop += shift;
      remeasured();
    });
    return ro.current;
  }, []);
  useEffect(() => () => ro.current?.disconnect(), []);

  // Track the box's height.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setViewH(el.clientHeight);
    if (typeof ResizeObserver === "undefined") return;
    const r = new ResizeObserver(() => setViewH(el.clientHeight));
    r.observe(el);
    return () => r.disconnect();
  }, []);

  const pendingCard = useRef<CardJump | null>(null);
  // The card the reader jumped to, highlighted for a moment.
  const [hlCard, setHlCard] = useState<string | null>(null);
  // Highlights fade after HIGHLIGHT_MS, or at the reader's next press or key in the list.
  useEffect(() => {
    if (highlight === null && hlCard === null) return;
    const t = window.setTimeout(() => {
      setHighlight(null);
      setHlCard(null);
    }, HIGHLIGHT_MS);
    return () => window.clearTimeout(t);
  }, [highlight, hlCard]);
  /** Ends the jump highlights. */
  const unhighlight = () => {
    if (highlight !== null) setHighlight(null);
    if (hlCard !== null) setHlCard(null);
  };

  // A new filter starts the list from the top (a jump then scrolls to its row).
  const filterKey = `${mode}|${filter.from}|${filter.to}|${[...filter.lanes].join(",")}|${filter.decisionsOnly}|${filter.hideArticle}|${filter.model}|${filter.effort}|${filter.phase}|${filter.query}|${filter.area}|${filter.session}|${filter.categories ? [...filter.categories].sort().join(",") : "all"}`;
  const lastKey = useRef(filterKey);
  useEffect(() => {
    if (lastKey.current === filterKey) return;
    lastKey.current = filterKey;
    if (box.current) box.current.scrollTop = 0;
    setScrollTop(0);
    // A card jump still waiting for its card is dropped (a jump set with this
    // same change is recorded after this effect, so it survives).
    pendingCard.current = null;
  }, [filterKey]);

  // Jump: show the page section, then scroll the box to the row, expand, highlight and focus it.
  const lastJump = useRef<number | null>(null);
  useEffect(() => {
    if (!jump || lastJump.current === jump.n) return;
    const k = rows.findIndex((e) => e.kind === "row" && e.row.i === jump.i);
    // Hidden by the filter (or its card not open yet): the jump runs once the row is in.
    if (k < 0) return;
    lastJump.current = jump.n;
    onJumped(jump.n);
    const id = rows[k].id;
    setOpen((m) => new Map(m).set(id, true));
    setHighlight(jump.i);
    section.current?.scrollIntoView({
      block: "start",
      behavior: reduced ? "auto" : "smooth",
    });
    /** Scrolls the box so row k sits at its top, then focuses it. */
    const align = (focus: boolean) => {
      const el = box.current;
      if (!el) return;
      el.scrollTop = Math.max(0, (offsetsRef.current[k] ?? 0) - 8);
      setScrollTop(el.scrollTop);
      if (focus) {
        const row = el.querySelector<HTMLElement>(`[data-k="${k}"]`);
        row?.focus({ preventScroll: true });
      }
    };
    align(false);
    const t1 = window.setTimeout(() => align(false), 120);
    const t2 = window.setTimeout(() => align(true), 450);
    return () => {
      window.clearTimeout(t1);
      window.clearTimeout(t2);
    };
  }, [jump, rows, section, reduced, onJumped]);

  // Card jump: scroll the box to the card; with focus, the page to the log
  // and focus to its button. The jump stays pending until the card is in the
  // list and rendered (the filter it came with may still be applying).
  const lastCard = useRef<number | null>(null);
  useEffect(() => {
    if (!cardJump || lastCard.current === cardJump.n) return;
    lastCard.current = cardJump.n;
    pendingCard.current = cardJump;
    if (cardJump.focus) {
      section.current?.scrollIntoView({
        block: "start",
        behavior: reduced ? "auto" : "smooth",
      });
    }
  }, [cardJump, section, reduced]);
  useEffect(() => {
    const p = pendingCard.current;
    const el = box.current;
    if (!p || !el) return;
    const k = entryIndex(rows, `card:${p.key}`);
    if (k < 0) return;
    const top = Math.max(0, (offsetsRef.current[k] ?? 0) - 8);
    if (el.scrollTop !== top) el.scrollTop = top;
    setScrollTop(el.scrollTop);
    const t = window.setTimeout(() => {
      const btn = el.querySelector<HTMLElement>(
        `[data-card="${CSS.escape(p.key)}"]`,
      );
      if (!btn || pendingCard.current !== p) return;
      pendingCard.current = null;
      setHlCard(p.key);
      if (p.focus) {
        // The page-level link handler re-aligns to the card, not the widget's top.
        window.dispatchEvent(
          new CustomEvent(WIDGET_SCROLL_EVENT, { detail: btn }),
        );
        btn.focus({ preventScroll: true });
        btn.scrollIntoView({
          block: "center",
          behavior: reduced ? "auto" : "smooth",
        });
      }
    }, 300);
    return () => window.clearTimeout(t);
  });

  const total = live[rows.length];
  const first = rowAt(live, Math.max(0, scrollTop - OVERSCAN));
  const lastRow = Math.min(
    rows.length - 1,
    rowAt(live, scrollTop + viewH + OVERSCAN),
  );

  /** Observes a rendered row element until it unmounts. */
  const observe = useCallback(
    (el: HTMLLIElement | null) => {
      const o = observer();
      if (!el || !o) return;
      o.observe(el);
      return () => o.unobserve(el);
    },
    [observer],
  );

  // The rendered rows: those near the view, plus the row holding focus (so
  // focus is not dropped to the page when the reader scrolls it away).
  const indices: number[] = [];
  if (rows.length > 0) {
    for (let k = first; k <= lastRow; k++) indices.push(k);
    const fk = focusedId ? rows.findIndex((e) => e.id === focusedId) : -1;
    if (fk >= 0 && (fk < first || fk > lastRow)) {
      indices.push(fk);
      indices.sort((a, b) => a - b);
    }
  }
  const places = entryPlaces(rows);
  const items = [];
  for (const k of indices) {
    const e = rows[k];
    let body: React.ReactNode;
    if (e.kind === "row") {
      body = (
        <LogRow
          row={e.row}
          glossary={glossary}
          expanded={open.get(e.id) ?? false}
          onToggle={onToggle}
          highlighted={highlight === e.row.i}
          boundary={boundary}
        />
      );
    } else if (e.kind === "card") {
      body = (
        <SessionCard
          card={e.card}
          session={card.sessions.get(e.card.key)}
          info={card.info[e.card.key]}
          start={card.start}
          open={e.open}
          onToggle={card.onToggle}
          highlighted={hlCard === e.card.key}
        />
      );
    } else {
      body = (
        <CommandOnlyGroup
          cards={e.cards}
          prompts={e.prompts}
          total={card.groupTotal}
          open={e.open}
          onToggle={card.onToggleGroup}
          onShowAll={card.onShowAll}
        />
      );
    }
    items.push(
      // biome-ignore lint/a11y/useAriaPropsSupportedByRole: WAI-ARIA 1.2 lists aria-level among listitem's supported states; it nests the flat (virtualized) items
      <li
        key={e.id}
        ref={observe}
        id={e.kind === "row" ? rowDomId(e.row.id) : undefined}
        aria-level={places[k].level}
        aria-setsize={places[k].setsize}
        aria-posinset={places[k].posinset}
        tabIndex={-1}
        data-id={e.id}
        data-k={k}
        className={`${s.rowSlot} ${e.kind === "row" && e.card ? s.rowInCard : ""}`}
        style={{ top: live[k] }}
      >
        {body}
      </li>,
    );
  }

  return (
    <section
      ref={box}
      className={s.logBox}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus so keys can scroll it
      tabIndex={0}
      aria-label={
        mode === "sessions"
          ? "Sessions with prompts matching the filter (scrollable)"
          : "Prompts matching the filter (scrollable)"
      }
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      onPointerDown={unhighlight}
      onKeyDown={unhighlight}
    >
      {rows.length === 0 ? (
        <div className={s.logState}>No prompts match this filter.</div>
      ) : (
        <ul
          className={s.rowList}
          style={{ height: total }}
          onFocus={(e) => {
            const li = (e.target as HTMLElement).closest<HTMLElement>(
              "li[data-id]",
            );
            setFocusedId(li?.dataset.id ?? null);
          }}
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
              setFocusedId(null);
            }
          }}
        >
          {items}
        </ul>
      )}
    </section>
  );
}
