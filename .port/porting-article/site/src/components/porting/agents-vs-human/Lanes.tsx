import type { ReactNode } from "react";
import type { ChartCtx } from "./chart-context";
import { HEAD_H, VIEW_W, X } from "./geometry";
import {
  dayIndex,
  dayLabel,
  ERA_GROUPS,
  type EventKind,
  GROUP_COLOURS,
  KIND_COLOURS,
  weekday,
  weekdayIndex,
} from "./model";

/** LABEL_GAP is the minimum vertical distance between two non-strong event labels. */
const LABEL_GAP = 10;

/** Returns true for event kinds whose labels always show (pivots and rule changes). */
function strong(kind: EventKind): boolean {
  return kind === "pivot" || kind === "rule";
}

/**
 * Draws the left annotation lane: the engineering-era Gantt (one coloured
 * column per group) and the day labels, which thin out to Mondays when rows
 * are short.
 */
export function Gantt({ ctx }: { ctx: ChartCtx }) {
  const { rows: r, data } = ctx;
  const out: ReactNode[] = [];
  for (let j = 0; j < data.eras.length; j++) {
    const [g, es, ee] = data.eras[j];
    const a = dayIndex(data, es);
    const b = dayIndex(data, ee);
    if (b < r.d0 || a > r.d1) continue;
    const gi = ERA_GROUPS.indexOf(g);
    const y0 = r.y(Math.max(a, r.d0));
    const y1 = r.y(Math.min(b, r.d1)) + r.h;
    out.push(
      <rect
        key={`era${j}`}
        x={X.gantt + gi * 7}
        y={y0 + 0.5}
        width={5}
        height={Math.max(2, y1 - y0 - 1)}
        rx={2}
        fill={GROUP_COLOURS[g]}
        opacity={0.85}
      />,
    );
  }
  const step = r.h >= 11 ? 1 : r.h >= 6 ? 7 : 14;
  for (let i = r.d0; i <= r.d1; i++) {
    const wd = weekdayIndex(data, i);
    const show =
      step === 1 || (wd === 1 && (i - r.d0) % step < 7) || i === r.d0;
    if (!show) continue;
    out.push(
      <text
        key={`d${i}`}
        x={X.dateR}
        y={r.y(i) + Math.min(r.h, 11) * 0.5 + 3}
        fill={wd === 0 || wd === 6 ? "#4a4f4b" : "#858a84"}
        fontSize={Math.min(9, Math.max(7, r.h * 0.8))}
        textAnchor="end"
      >
        {step === 1 && r.h >= 20 ? `${weekday(data, i)} ` : ""}
        {dayLabel(data, i)}
      </text>,
    );
  }
  return <g>{out}</g>;
}

/**
 * Draws the sourced events: a glyph per kind in the event lane (◆ pin
 * advance, ■ limit or pause, ● milestone, pivot or rule), an outline on the
 * event's exact hour cell (pins have no reliable hour, so no outline),
 * full-width rules for pivots and rule changes, and short labels placed with
 * collision avoidance (pivot and rule labels always show).
 */
export function Events({ ctx }: { ctx: ChartCtx }) {
  const { rows: r, data, vis } = ctx;
  const evs = data.events
    .map(([d, t, k, l], j) => ({
      j,
      i: dayIndex(data, d),
      hour: Number(t.slice(0, 2)),
      k,
      l,
    }))
    .filter((e) => e.i >= r.d0 && e.i <= r.d1);
  const out: ReactNode[] = [];
  for (const e of evs) {
    const top = r.y(e.i);
    const y = top + r.h / 2;
    const c = KIND_COLOURS[e.k];
    if (strong(e.k)) {
      out.push(
        <line
          key={`rule${e.j}`}
          x1={X.ev}
          x2={VIEW_W}
          y1={top}
          y2={top}
          stroke={c}
          strokeWidth={1}
          opacity={0.75}
        />,
      );
    }
    if (e.k !== "pin" && vis(e.i)) {
      out.push(
        <rect
          key={`cell${e.j}`}
          x={X.grid + e.hour * X.cw + 0.5}
          y={top + 0.3}
          width={X.cw - 1}
          height={Math.max(1.5, r.h - 0.6)}
          rx={2}
          fill="none"
          stroke={c}
          strokeWidth={1.3}
        />,
      );
    }
    out.push(<EventGlyph key={`g${e.j}`} kind={e.k} x={X.ev} y={y} />);
  }
  const placed: number[] = [];
  const ordered = evs
    .filter((e) => strong(e.k))
    .concat(evs.filter((e) => !strong(e.k)));
  for (const e of ordered) {
    const y = r.y(e.i) + r.h / 2;
    const st = strong(e.k);
    if (!st && placed.some((py) => Math.abs(py - y) < LABEL_GAP)) continue;
    placed.push(y);
    out.push(
      <text
        key={`l${e.j}`}
        x={X.evl}
        y={y + 3}
        fill={st ? KIND_COLOURS[e.k] : "#9aa09a"}
        fontSize={8.5}
        fontWeight={st ? 700 : 400}
      >
        {e.l}
      </text>,
    );
  }
  return <g>{out}</g>;
}

/** Draws one event glyph: diamond for pins, square for limits, circle otherwise. */
export function EventGlyph({
  kind,
  x,
  y,
}: {
  kind: EventKind;
  x: number;
  y: number;
}) {
  const c = KIND_COLOURS[kind];
  if (kind === "pin") {
    return <path d={`M${x - 3} ${y} l3 -3 l3 3 l-3 3z`} fill={c} />;
  }
  if (kind === "limit") {
    return <rect x={x - 2.5} y={y - 2.5} width={5} height={5} fill={c} />;
  }
  return <circle cx={x} cy={y} r={3} fill={c} />;
}

/** Draws the column heads above the rows, with the visible maxima of the marginals. */
export function ColumnHeads({
  maxima,
}: {
  maxima: {
    commits: number;
    prompts: number;
    cost: number;
    threads: number;
    lines: number;
  };
}) {
  /** Draws one small grey heading. */
  const head = (x: number, t: string, anchor: "start" | "middle" | "end") => (
    <text
      key={t}
      x={x}
      y={14}
      fill="#858a84"
      fontSize={8}
      letterSpacing={0.5}
      textAnchor={anchor}
    >
      {t}
    </text>
  );
  /** Draws one dim scale note at the right end of a column. */
  const note = (x: number, t: string) => (
    <text
      key={`n${x}`}
      x={x}
      y={HEAD_H - 6}
      fill="#4a4f4b"
      fontSize={8}
      textAnchor="end"
    >
      {t}
    </text>
  );
  const hours: ReactNode[] = [];
  for (let hh = 0; hh < 24; hh += 3) {
    hours.push(
      <text
        key={`h${hh}`}
        x={X.grid + hh * X.cw + X.cw / 2}
        y={HEAD_H - 6}
        fill="#858a84"
        fontSize={9}
        textAnchor="middle"
      >
        {String(hh).padStart(2, "0")}
      </text>,
    );
  }
  const k = (v: number) => `${Math.round(v / 1000).toLocaleString("en-US")}k`;
  return (
    <g>
      {head(4, "ERAS", "start")}
      {head(X.ev, "EVENTS", "start")}
      {head(X.dateR, "PT DAY", "end")}
      {hours}
      {head(X.grid, "HOUR OF DAY →", "start")}
      <text
        x={X.model + 8}
        y={HEAD_H - 6}
        fill="#858a84"
        fontSize={8}
        textAnchor="middle"
      >
        MODEL
      </text>
      {head(X.cost, "API-EQ. $", "start")}
      {head(X.thr, "THREADS", "start")}
      {head(X.com, "COMMITS", "start")}
      {head(X.pro, "PROMPTS", "start")}
      {head(X.lin + X.linW / 2, "LINES −/+ (√)", "middle")}
      {note(X.com + X.comW, `max ${maxima.commits}`)}
      {note(X.pro + X.proW, String(maxima.prompts))}
      {note(
        X.cost + X.costW,
        `$${Math.round(maxima.cost).toLocaleString("en-US")}`,
      )}
      {note(X.thr + X.thrW, `${maxima.threads} log`)}
      {note(X.lin + X.linW, k(maxima.lines))}
    </g>
  );
}
