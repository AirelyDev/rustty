import { type ReactNode, useMemo } from "react";
import type { ChartCtx } from "./chart-context";
import { VIEW_W, X } from "./geometry";
import {
  cellValue,
  dayIndex,
  humanEraRanges,
  type Metric,
  metricMax,
  rampColour,
} from "./model";

/** EMPTY_CELL is the fill of a grid cell with no activity. */
const EMPTY_CELL = "#141615";

/**
 * Draws the human-era separators (dashed) and the hovered era's outline, and
 * the red hatch over quiet stretches.
 */
export function EraBands({ ctx }: { ctx: ChartCtx }) {
  const { rows: r, data, hl, ids } = ctx;
  const out: ReactNode[] = [];
  const ranges = humanEraRanges(data);
  for (let e = 0; e < ranges.length; e++) {
    const [a, b] = ranges[e];
    if (b < r.d0 || a > r.d1) continue;
    const y0 = r.y(Math.max(a, r.d0));
    const y1 = r.y(Math.min(b, r.d1)) + r.h;
    if (hl === e) {
      out.push(
        <rect
          key={`hl${e}`}
          x={X.grid - 2}
          y={y0}
          width={24 * X.cw + 4}
          height={y1 - y0}
          fill="none"
          stroke="#f4f1ed"
          strokeWidth={1.2}
          rx={3}
        />,
      );
    }
    if (a > r.d0) {
      out.push(
        <line
          key={`sep${e}`}
          x1={X.ev - 4}
          x2={VIEW_W}
          y1={y0}
          y2={y0}
          stroke="#3a3f3b"
          strokeDasharray="2 3"
        />,
      );
    }
  }
  for (let j = 0; j < data.quiet.length; j++) {
    const [qs, qe] = data.quiet[j];
    const a = dayIndex(data, qs);
    const b = dayIndex(data, qe);
    if (b < r.d0 || a > r.d1) continue;
    const y0 = r.y(Math.max(a, r.d0));
    const y1 = r.y(Math.min(b, r.d1)) + r.h;
    out.push(
      <rect
        key={`q${j}`}
        x={X.grid}
        y={y0}
        width={24 * X.cw}
        height={y1 - y0}
        fill={`url(#${ids}-quiet)`}
      />,
    );
  }
  return <g>{out}</g>;
}

/** Returns the overlay-dot radius for count in a cell of size min(cw, h). */
function dotRadius(count: number, metric: Metric, h: number): number {
  const ref = metric === "p" ? 46 : 16;
  const r = (Math.min(X.cw, h) / 2) * (0.32 + 0.6 * Math.sqrt(count / ref));
  return Math.max(0.9, r);
}

/**
 * Draws the day × hour cells on the metric's log ramp, the overlay dots
 * (prompts, or commits when the metric is prompts) and the hollow rings of
 * the founding sessions' prompts typed before the repository existed.
 */
export function GridCells({ ctx }: { ctx: ChartCtx }) {
  const { rows: r, data, metric, vis } = ctx;
  const max = useMemo(() => metricMax(data, metric), [data, metric]);
  const out: ReactNode[] = [];
  const pad = r.pad;
  for (let i = r.d0; i <= r.d1; i++) {
    if (!vis(i)) continue;
    const y = r.y(i);
    for (let hh = 0; hh < 24; hh++) {
      const k = i * 24 + hh;
      const v = cellValue(data, metric, k);
      out.push(
        <rect
          key={`c${k}`}
          x={X.grid + hh * X.cw + pad}
          y={y + pad}
          width={X.cw - 2 * pad}
          height={Math.max(0.6, r.h - 2 * pad)}
          rx={r.h > 8 ? 3 : 1}
          fill={
            v ? rampColour(metric, Math.log1p(v) / Math.log1p(max)) : EMPTY_CELL
          }
        />,
      );
      const ov = metric === "p" ? data.grid.commits[k] : data.grid.prompts[k];
      if (ov) {
        out.push(
          <circle
            key={`d${k}`}
            cx={X.grid + hh * X.cw + X.cw / 2}
            cy={y + r.h / 2}
            r={dotRadius(ov, metric, r.h)}
            fill={metric === "p" ? "#d97934" : "#64d8c6"}
            stroke="#0b0c0c"
            strokeWidth={r.h > 8 ? 1 : 0.5}
          />,
        );
      }
    }
  }
  for (let j = 0; j < data.originPrompts.length; j++) {
    const t = data.originPrompts[j];
    const i = Math.floor(t / 1440);
    const hh = Math.floor((t % 1440) / 60);
    if (!vis(i)) continue;
    out.push(
      <circle
        key={`o${j}`}
        cx={X.grid + hh * X.cw + X.cw / 2}
        cy={r.y(i) + r.h / 2}
        r={Math.max(1.4, (Math.min(X.cw, r.h) / 2) * 0.55)}
        fill="none"
        stroke="#64d8c6"
        strokeWidth={1}
      />,
    );
  }
  return <g>{out}</g>;
}
