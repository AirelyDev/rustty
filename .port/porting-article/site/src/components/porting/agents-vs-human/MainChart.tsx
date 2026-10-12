"use client";

import {
  type KeyboardEvent,
  memo,
  type PointerEvent,
  useMemo,
  useRef,
  useState,
} from "react";
import { type ChartCtx, visibility } from "./chart-context";
import { EraBands, GridCells } from "./GridCells";
import { HEAD_H, GRID_H, rows, VIEW_H, VIEW_W, X } from "./geometry";
import { ColumnHeads, Events, Gantt } from "./Lanes";
import {
  type AvhData,
  type DayTotals,
  hourProfile,
  type Metric,
  metricMax,
} from "./model";
import { HourProfile, Legend } from "./ProfileLegend";
import { type Maxima, Marginals, Ribbons } from "./Ribbons";
import s from "./AgentsVsHuman.module.css";

/** A hovered or focused target: a day, and an hour when inside the grid. */
export type Target = { day: number; hour: number | null };

/** Where to anchor the tooltip, in client (viewport) coordinates. */
export type Anchor = { x: number; y: number };

/** Props for MainChart. */
type MainChartProps = {
  data: AvhData;
  totals: DayTotals;
  metric: Metric;
  d0: number;
  d1: number;
  rep: number | null;
  hl: number | null;
  ids: string;
  /** Id of the tooltip element describing the chart. */
  tipId: string;
  /** Keyboard-focused cell, drawn with an outline. */
  focus: Target | null;
  /** Pointer or keyboard moved onto a target (null: left the rows). */
  onHover: (t: Target | null, at: Anchor) => void;
  /** Click or tap without dragging: pin or unpin the tooltip at t. */
  onTap: (t: Target, at: Anchor) => void;
  /** Drag finished over at least two rows: zoom to days [a, b]. */
  onZoom: (a: number, b: number) => void;
  /** Keyboard navigation inside the chart (arrows, Enter, Escape). */
  onKey: (e: KeyboardEvent<SVGSVGElement>) => void;
  /** Chart received keyboard (focus-visible) focus. */
  onFocus: () => void;
  /** Chart lost keyboard focus. */
  onBlur: () => void;
};

/** Returns the maxima of the marginal series over visible days [d0, d1]. */
function visibleMaxima(
  data: AvhData,
  t: DayTotals,
  d0: number,
  d1: number,
): Maxima {
  const m: Maxima = { commits: 1, prompts: 1, lines: 1, threads: 1, cost: 1 };
  const dy = data.daily;
  for (let i = d0; i <= d1; i++) {
    m.commits = Math.max(m.commits, t.commits[i]);
    m.prompts = Math.max(m.prompts, t.prompts[i]);
    m.lines = Math.max(m.lines, t.added[i], t.deleted[i]);
    m.threads = Math.max(m.threads, dy.threads[i]);
    m.cost = Math.max(
      m.cost,
      dy.costMacClaude[i] + dy.costVmClaude[i] + dy.costCodex[i],
    );
  }
  return m;
}

/**
 * The main SVG: annotation lane, day × hour grid, ribbons, marginals, hour
 * profile and legend. Hovering reports the day/cell under the pointer, a
 * vertical drag brushes a day range, and the chart is keyboard focusable.
 */
function MainChart(props: MainChartProps) {
  const { data, totals, metric, d0, d1, rep, hl, ids, focus } = props;
  const svgRef = useRef<SVGSVGElement>(null);
  const [brush, setBrush] = useState<[number, number] | null>(null);
  const drag = useRef<{ start: number; end: number } | null>(null);
  const r = useMemo(() => rows(d0, d1), [d0, d1]);
  const ctx: ChartCtx = {
    data,
    totals,
    rows: r,
    metric,
    rep,
    hl,
    ids,
    vis: visibility(d0, d1, rep),
  };
  const max = useMemo(
    () => visibleMaxima(data, totals, d0, d1),
    [data, totals, d0, d1],
  );
  const profile = useMemo(
    () => hourProfile(data, d0, rep === null ? d1 : Math.min(d1, rep)),
    [data, d0, d1, rep],
  );
  const metricTop = useMemo(() => metricMax(data, metric), [data, metric]);

  /** Converts a pointer event to viewBox coordinates. */
  const toView = (e: PointerEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const k = VIEW_W / box.width;
    return { x: (e.clientX - box.left) * k, y: (e.clientY - box.top) * k };
  };

  /** Returns the target under viewBox point p. */
  const targetAt = (p: { x: number; y: number }): Target | null => {
    const day = r.at(p.y);
    if (day === null) return null;
    const inGrid = p.x >= X.grid && p.x < X.grid + 24 * X.cw;
    return { day, hour: inGrid ? Math.floor((p.x - X.grid) / X.cw) : null };
  };

  const onMove = (e: PointerEvent<SVGSVGElement>) => {
    const p = toView(e);
    if (drag.current) {
      const day = r.at(Math.max(HEAD_H, Math.min(HEAD_H + GRID_H - 1, p.y)));
      if (day !== null && day !== drag.current.end) {
        drag.current.end = day;
        setBrush([drag.current.start, day]);
      }
      return;
    }
    props.onHover(targetAt(p), { x: e.clientX, y: e.clientY });
  };

  const onDown = (e: PointerEvent<SVGSVGElement>) => {
    if (e.button > 0) return;
    const day = r.at(toView(e).y);
    if (day === null) return;
    drag.current = { start: day, end: day };
    if (e.pointerType === "mouse") {
      e.currentTarget.setPointerCapture?.(e.pointerId);
      e.preventDefault();
    }
  };

  const onUp = (e: PointerEvent<SVGSVGElement>) => {
    const d = drag.current;
    drag.current = null;
    setBrush(null);
    if (!d) return;
    if (Math.abs(d.end - d.start) >= 1) {
      props.onZoom(Math.min(d.start, d.end), Math.max(d.start, d.end));
      return;
    }
    const t = targetAt(toView(e));
    if (t) props.onTap(t, { x: e.clientX, y: e.clientY });
  };

  const onCancel = () => {
    drag.current = null;
    setBrush(null);
  };

  const cursorY =
    rep !== null && rep >= d0 && rep <= d1 ? r.y(rep) + r.h : null;
  const [ba, bb] = brush
    ? [Math.min(brush[0], brush[1]), Math.max(brush[0], brush[1])]
    : [0, 0];

  return (
    <svg
      ref={svgRef}
      className={s.mainSvg}
      viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
      role="application"
      aria-roledescription="heatmap"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
      tabIndex={0}
      aria-label="Heatmap of commits and prompts by day and Pacific hour, with per-day margins. Use the arrow keys to move between days and hours, Enter to pin the details, Escape to close them."
      aria-describedby={props.tipId}
      onPointerMove={onMove}
      onPointerLeave={(e) => {
        if (!drag.current) props.onHover(null, { x: e.clientX, y: e.clientY });
      }}
      onPointerDown={onDown}
      onPointerUp={onUp}
      onPointerCancel={onCancel}
      onKeyDown={props.onKey}
      onFocus={(e) => {
        if (e.currentTarget.matches(":focus-visible")) props.onFocus();
      }}
      onBlur={props.onBlur}
    >
      <defs>
        <pattern
          id={`${ids}-lost`}
          width={5}
          height={5}
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect width={5} height={5} fill="#0b0c0c" />
          <line x1={0} y1={0} x2={0} y2={5} stroke="#2c302d" strokeWidth={2} />
        </pattern>
        <pattern
          id={`${ids}-quiet`}
          width={6}
          height={6}
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(-45)"
        >
          <line
            x1={0}
            y1={0}
            x2={0}
            y2={6}
            stroke="#e06c75"
            strokeOpacity={0.22}
            strokeWidth={1.4}
          />
        </pattern>
        <pattern
          id={`${ids}-dots`}
          width={3}
          height={3}
          patternUnits="userSpaceOnUse"
        >
          <circle cx={1.5} cy={1.5} r={0.7} fill="#0b0c0c" />
        </pattern>
        <pattern
          id={`${ids}-stripe`}
          width={3}
          height={3}
          patternUnits="userSpaceOnUse"
        >
          <line x1={0} y1={1.5} x2={3} y2={1.5} stroke="#0b0c0c" />
        </pattern>
      </defs>
      <ColumnHeads maxima={max} />
      <EraBands ctx={ctx} />
      <Gantt ctx={ctx} />
      <GridCells ctx={ctx} />
      <Ribbons ctx={ctx} max={max} />
      <Marginals ctx={ctx} max={max} />
      <Events ctx={ctx} />
      {cursorY !== null ? (
        <line
          x1={X.ev - 4}
          x2={VIEW_W}
          y1={cursorY}
          y2={cursorY}
          stroke="#d97934"
          strokeWidth={1.5}
        />
      ) : null}
      {brush ? (
        <rect
          x={X.ev - 4}
          y={r.y(ba)}
          width={VIEW_W - X.ev + 4}
          height={r.y(bb) + r.h - r.y(ba)}
          fill="#d97934"
          fillOpacity={0.12}
          stroke="#d97934"
          strokeDasharray="3 2"
        />
      ) : null}
      {focus ? (
        <rect
          x={focus.hour === null ? X.ev - 4 : X.grid + focus.hour * X.cw}
          y={r.y(focus.day)}
          width={focus.hour === null ? VIEW_W - X.ev + 4 : X.cw}
          height={r.h}
          fill="none"
          stroke="#64d8c6"
          strokeWidth={1.5}
          rx={2}
        />
      ) : null}
      <HourProfile profile={profile} />
      <Legend metric={metric} max={metricTop} ids={ids} />
    </svg>
  );
}

export default memo(MainChart);
