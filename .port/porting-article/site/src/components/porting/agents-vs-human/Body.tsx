"use client";

import data from "@/app/porting/data/agents-vs-human.json";
import {
  type CSSProperties,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { type ReplayState, replayStart, replayStep } from "../lib/replay";
import {
  TOOLTIP_IDLE,
  tooltipReducer,
  visibleTooltip,
} from "../lib/tooltip-state";
import ReplayControls from "../ui/ReplayControls";
import useReducedMotion from "../ui/useReducedMotion";
import s from "./AgentsVsHuman.module.css";
import { visibility } from "./chart-context";
import DayTooltip from "./DayTooltip";
import { Header, InsightTiles } from "./Header";
import { AutonomyPanel, LeverageBars } from "./EraPanels";
import { rows, VIEW_W, X } from "./geometry";
import MainChart, { type Anchor, type Target } from "./MainChart";
import {
  autonomy,
  type AvhData,
  brushStats,
  dayLabel,
  dayTotals,
  humanEraRanges,
  insightTiles,
  type Metric,
} from "./model";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as AvhData;

/** TIP_GAP is the distance in px between the pointer and the tooltip. */
const TIP_GAP = 14;

/** Encodes a target as a tooltip id ("day:hour" or "day:row"). */
function tipKey(t: Target): string {
  return `${t.day}:${t.hour ?? "row"}`;
}

/** Decodes a tooltip id back into a target. */
function fromKey(key: string): Target {
  const [d, h] = key.split(":");
  return { day: Number(d), hour: h === "row" ? null : Number(h) };
}

/**
 * The "Agents vs human" widget: 111 Pacific days × 24 hours of commits and
 * prompts with ribbons, marginals, an hour profile, autonomy histograms,
 * leverage bars and six live insight tiles. A metric toggle, a vertical
 * brush (zoom), era cards (zoom), a day-by-day replay and hover/tap/keyboard
 * tooltips drive every panel.
 */
export default function Body() {
  const reduced = useReducedMotion();
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const ids = `avh${uid}`;
  const tipId = `${ids}-tip`;
  const nDays = DATA.daily.date.length;
  const totals = useMemo(() => dayTotals(DATA), []);
  const eraRanges = useMemo(() => humanEraRanges(DATA), []);

  const [metric, setMetric] = useState<Metric>("c");
  const [range, setRange] = useState<[number, number]>([0, nDays - 1]);
  const [d0, d1] = range;
  const last = d1 - d0;
  // The widget opens on the full picture; Play replays from the first day,
  // resumes after a pause, and with reduced motion jumps straight to the
  // final state.
  const [replay, setReplay] = useState<ReplayState>({
    index: last,
    playing: false,
  });
  const rep = replay.index >= last ? null : d0 + replay.index;
  const [hl, setHl] = useState<number | null>(null);

  const [tip, dispatch] = useReducer(tooltipReducer, TOOLTIP_IDLE);
  const anchors = useRef(new Map<string, Anchor>());
  const [focus, setFocus] = useState<Target | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const [tipPos, setTipPos] = useState<{ left: number; top: number }>({
    left: 0,
    top: 0,
  });

  /** Sets a new day range and resets the replay to its final state. */
  const zoom = useCallback((a: number, b: number) => {
    setRange([a, b]);
    setReplay({ index: b - a, playing: false });
    setHl(null);
    dispatch({ type: "escape" });
  }, []);

  // Advance the replay one day per tick (about 7 s for the whole range).
  useEffect(() => {
    if (!replay.playing) return;
    const ms = Math.max(40, Math.min(160, 7000 / (last + 1)));
    const timer = setInterval(() => setReplay((r) => replayStep(r, last)), ms);
    return () => clearInterval(timer);
  }, [replay.playing, last]);

  const end = rep === null ? d1 : Math.min(d1, rep);
  const stats = useMemo(() => brushStats(DATA, d0, end), [d0, end]);
  const tiles = useMemo(() => insightTiles(DATA, stats), [stats]);
  const eras = useMemo(() => autonomy(DATA, d0, end), [d0, end]);

  const onHover = useCallback((t: Target | null, at: Anchor) => {
    if (!t) {
      dispatch({ type: "leave" });
      return;
    }
    const key = tipKey(t);
    anchors.current.set(key, at);
    dispatch({ type: "enter", id: key });
  }, []);

  const onTap = useCallback((t: Target, at: Anchor) => {
    const key = tipKey(t);
    anchors.current.set(key, at);
    dispatch({ type: "tap", id: key });
  }, []);

  /** Returns the viewport anchor of a keyboard target (right edge of its cell). */
  const cellAnchor = (t: Target): Anchor => {
    const svg = rootRef.current?.querySelector("svg");
    if (!svg) return { x: 0, y: 0 };
    const box = svg.getBoundingClientRect();
    const k = box.width / VIEW_W;
    const r = rows(d0, d1);
    const x = t.hour === null ? X.grid : X.grid + (t.hour + 1) * X.cw;
    return { x: box.left + x * k, y: box.top + (r.y(t.day) + r.h) * k };
  };

  /** Moves the keyboard focus to t and shows its tooltip. */
  const focusTarget = (t: Target) => {
    setFocus(t);
    const key = tipKey(t);
    anchors.current.set(key, cellAnchor(t));
    // A pinned tooltip moves with the keyboard cursor.
    const pinned = tip.pinned !== null;
    dispatch({ type: "escape" });
    dispatch({ type: pinned ? "tap" : "focus", id: key });
  };

  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const cur = focus ?? { day: d0, hour: 0 };
    let next: Target | null = null;
    if (e.key === "ArrowDown")
      next = { ...cur, day: Math.min(d1, cur.day + 1) };
    else if (e.key === "ArrowUp")
      next = { ...cur, day: Math.max(d0, cur.day - 1) };
    else if (e.key === "ArrowRight")
      next = {
        ...cur,
        hour: cur.hour === null ? 0 : Math.min(23, cur.hour + 1),
      };
    else if (e.key === "ArrowLeft")
      next = {
        ...cur,
        hour: cur.hour === null || cur.hour === 0 ? null : cur.hour - 1,
      };
    else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const key = tipKey(cur);
      anchors.current.set(key, cellAnchor(cur));
      setFocus(cur);
      dispatch({ type: "tap", id: key });
      return;
    } else if (e.key === "Escape") {
      dispatch({ type: "escape" });
      return;
    }
    if (next) {
      e.preventDefault();
      focusTarget(next);
    }
  };

  /** Focuses the first cell when the chart gets keyboard focus without a target. */
  const onChartFocus = () => {
    if (!focus) focusTarget({ day: d0, hour: 0 });
  };

  // Keyboard handlers close over the current range and focus; MainChart gets
  // stable wrappers so hover updates do not re-render its thousands of cells.
  const latest = useRef({ onKey, onChartFocus });
  latest.current = { onKey, onChartFocus };
  const stableKey = useCallback(
    (e: KeyboardEvent<SVGSVGElement>) => latest.current.onKey(e),
    [],
  );
  const stableFocus = useCallback(() => latest.current.onChartFocus(), []);

  const onBlur = useCallback(() => {
    setFocus(null);
    dispatch({ type: "blur" });
  }, []);

  const shown = visibleTooltip(tip);
  const target = shown ? fromKey(shown) : null;
  const vis = visibility(d0, d1, rep);

  // While any tooltip is visible (hovered, focused or pinned), Escape anywhere
  // on the page closes it. A press outside the chart clears a pinned one.
  const tipVisible = shown !== null;
  useEffect(() => {
    if (!tipVisible) return;
    const onDoc = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") dispatch({ type: "escape" });
    };
    document.addEventListener("keydown", onDoc);
    return () => document.removeEventListener("keydown", onDoc);
  }, [tipVisible]);
  useEffect(() => {
    if (!tip.pinned) return;
    const onDown = (e: PointerEvent) => {
      const svg = rootRef.current?.querySelector("svg");
      const t = e.target as Node;
      if (!svg?.contains(t) && !tipRef.current?.contains(t)) {
        dispatch({ type: "escape" });
      }
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [tip.pinned]);

  // Place the tooltip next to its anchor, flipped to stay inside the widget.
  useLayoutEffect(() => {
    if (!shown || !rootRef.current || !tipRef.current) return;
    const at = anchors.current.get(shown);
    if (!at) return;
    const root = rootRef.current.getBoundingClientRect();
    const tw = tipRef.current.offsetWidth;
    const th = tipRef.current.offsetHeight;
    let left = at.x - root.left + TIP_GAP;
    let top = at.y - root.top + TIP_GAP;
    if (left + tw > root.width - 8) left = at.x - root.left - tw - TIP_GAP;
    if (left < 8) left = Math.max(0, Math.min(8, root.width - tw));
    if (top + th > root.height - 8) top = at.y - root.top - th - TIP_GAP;
    setTipPos({ left, top: Math.max(0, top) });
  }, [shown, tip]);

  const replayLabel = `${rep === null ? "Showing" : "Replaying"} ${dayLabel(DATA, d0)} – ${dayLabel(DATA, end)}`;

  return (
    <div ref={rootRef} className={s.root}>
      <Header
        data={DATA}
        stats={stats}
        metric={metric}
        onMetric={setMetric}
        onReset={() => zoom(0, nDays - 1)}
      />
      <div className={s.replayRow}>
        <ReplayControls
          index={replay.index}
          lastIndex={last}
          playing={replay.playing}
          onPlay={() =>
            setReplay((r) =>
              reduced
                ? replayStart(true, last)
                : // Resume from the paused day; start over only from the end.
                  { index: r.index >= last ? 0 : r.index, playing: true },
            )
          }
          onPause={() => setReplay((r) => ({ ...r, playing: false }))}
          onSeek={(i) => setReplay({ index: i, playing: false })}
          label={replayLabel}
        />
      </div>
      <InsightTiles tiles={tiles} />
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the ribbons and per-day totals →
      </p>
      <div className={s.scroll}>
        <div className={s.main}>
          <MainChart
            data={DATA}
            totals={totals}
            metric={metric}
            d0={d0}
            d1={d1}
            rep={rep}
            hl={hl}
            ids={ids}
            tipId={tipId}
            focus={focus}
            onHover={onHover}
            onTap={onTap}
            onZoom={zoom}
            onKey={stableKey}
            onFocus={stableFocus}
            onBlur={onBlur}
          />
        </div>
      </div>
      <div
        ref={tipRef}
        id={tipId}
        role="tooltip"
        className={s.tip}
        hidden={!target}
        data-pinned={tip.pinned ? "" : undefined}
        style={{ left: tipPos.left, top: tipPos.top } as CSSProperties}
      >
        {target ? (
          <DayTooltip
            data={DATA}
            totals={totals}
            visible={vis(target.day)}
            target={target}
          />
        ) : null}
      </div>
      <p className={s.hint}>
        Hover a cell for that hour; hover anywhere else on a row for that day.
        Tap to pin; a pinned tooltip's commit links can be clicked. Drag up or
        down across rows to zoom to a date range; every panel recomputes for it.
        With the keyboard, focus the chart and use the arrow keys, Enter to pin
        (Tab then reaches its commit links) and Escape to close.
      </p>
      <div className={s.sub}>
        <div>
          <h4>Autonomy: hours from the author's last prompt to each commit</h4>
          <AutonomyPanel
            data={DATA}
            eras={eras}
            hl={hl}
            onHighlight={setHl}
            onZoom={(e) => zoom(eraRanges[e][0], eraRanges[e][1])}
          />
        </div>
        <div>
          <h4>Leverage by era</h4>
          <LeverageBars eras={eras} hl={hl} />
        </div>
      </div>
    </div>
  );
}
