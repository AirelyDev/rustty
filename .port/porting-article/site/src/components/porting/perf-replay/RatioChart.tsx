"use client";

import { PALETTE } from "../lib/palette";
import { log } from "../lib/scales";
import type { ChartTip } from "../ui/useChartTip";
import { OUTCOME_FILL } from "./Dumbbell";
import { tipHandlers } from "./hooks";
import { outcome } from "./model";
import s from "./PerfReplay.module.css";
import type { RatioRow } from "./rows";

/** Props for a log-ratio chart. */
type RatioChartProps = {
  /** Chart key (tooltip ids are `rc:<chart>:<row>`). */
  chart: string;
  rows: RatioRow[];
  /** Axis domain (ratios). */
  d0: number;
  d1: number;
  ticks: number[];
  /** Desktop geometry: total width, label and value column widths, row height. */
  w?: number;
  lw?: number;
  rw?: number;
  rh?: number;
  /** Phone: stacked rows (label and value above the line). */
  narrow: boolean;
  tip: ChartTip;
  label: string;
};

/** Axis text for a tick: "Zig" at 1, "4×" on the faster side, "1.5×" on the slower side. */
function tickText(t: number): string {
  if (t === 1) return "Zig";
  return t < 1 ? `${(1 / t).toFixed(t > 0.5 ? 1 : 0)}×` : `${t}×`;
}

/**
 * HALO paints a card-coloured stroke under row text, so the gridlines and
 * the Zig line that cross a stacked (phone) row never run through its labels.
 */
const HALO = {
  stroke: PALETTE.card,
  strokeWidth: 3,
  strokeLinejoin: "round",
  paintOrder: "stroke",
} as const;

/** Clamps v into [lo, hi]. */
function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

/**
 * A generic Rustty ÷ Ghostty log-ratio chart: per row an optional hollow
 * "before" dot with its thin range, the filled "now" dot (copper faster, red
 * slower, grey tie) with its thick range, a value label, or a hatched slot
 * when the row is pending. Arrow keys move between rows.
 */
export default function RatioChart(p: RatioChartProps) {
  const stacked = p.narrow;
  const w = stacked ? 420 : (p.w ?? 960);
  const lw = stacked ? 6 : (p.lw ?? 230);
  const rw = stacked ? 6 : (p.rw ?? 150);
  const rh = stacked ? 30 : (p.rh ?? 16);
  const tp = 24;
  const n = p.rows.length;
  const h = tp + n * rh + 30;
  const X = log([p.d0, p.d1], [lw, w - rw]);
  const cx = (v: number) => X(clamp(v, p.d0, p.d1));
  const handlers = tipHandlers(p.tip);
  const hatch = `hatch-${p.chart}`;
  return (
    <svg
      className={s.svg}
      viewBox={`0 0 ${w} ${h}`}
      role="application"
      aria-roledescription="chart"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
      tabIndex={0}
      aria-label={`${p.label} Arrow keys move between rows.`}
      {...handlers}
    >
      <defs>
        <pattern
          id={hatch}
          width={6}
          height={6}
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <line
            x1={0}
            y1={0}
            x2={0}
            y2={6}
            stroke={PALETTE.gold}
            strokeWidth={2}
          />
        </pattern>
      </defs>
      {p.ticks.map((t) => (
        <g key={t}>
          {stacked ? (
            // Phone rows carry their text above the dot line: the gridline is drawn
            // per row, only across the dot band, so it never crosses the text.
            p.rows.map((row, i) => {
              const yd = tp + i * rh + rh / 2 + 6;
              return (
                <line
                  key={`${t}:${row.name}`}
                  x1={X(t)}
                  x2={X(t)}
                  y1={yd - 7}
                  y2={yd + 7}
                  stroke={t === 1 ? PALETTE.upstream : "#1c1f1d"}
                  strokeWidth={t === 1 ? 1.5 : 1}
                />
              );
            })
          ) : (
            <line
              x1={X(t)}
              x2={X(t)}
              y1={tp - 6}
              y2={tp + n * rh}
              stroke={t === 1 ? PALETTE.upstream : "#1c1f1d"}
              strokeWidth={t === 1 ? 1.5 : 1}
            />
          )}
          <text
            x={X(t)}
            y={tp + n * rh + 13}
            fill={t === 1 ? PALETTE.upstream : PALETTE.gray4}
            fontSize={9}
            textAnchor="middle"
          >
            {tickText(t)}
          </text>
        </g>
      ))}
      <text x={lw} y={12} fill={PALETTE.rust} fontSize={9.5}>
        ◂ Rustty faster
      </text>
      <text
        x={w - rw}
        y={12}
        fill={PALETTE.fail}
        fontSize={9.5}
        textAnchor="end"
      >
        slower ▸
      </text>
      {p.rows.map((r, i) => {
        const yc = tp + i * rh + rh / 2;
        const y = stacked ? yc + 6 : yc;
        const ty = stacked ? yc - 5 : yc + 3.5;
        const id = `rc:${p.chart}:${i}`;
        const tipped = !r.pending && r.tip;
        return (
          <g
            key={`${r.name}-${i}`}
            data-tip={tipped ? id : undefined}
            data-on={p.tip.shown === id || undefined}
          >
            <rect
              x={0}
              y={yc - rh / 2}
              width={w}
              height={rh}
              className={p.tip.shown === id ? s.rowOn : undefined}
              fill="transparent"
            />
            <text
              x={stacked ? lw : lw - 8}
              y={ty}
              fill={r.floor ? PALETTE.gray4 : PALETTE.gray7}
              fontSize={9.5}
              textAnchor={stacked ? "start" : "end"}
              {...HALO}
            >
              {r.name}
              {r.floor ? " (floor)" : ""}
            </text>
            {r.pending ? (
              <>
                <rect
                  x={lw}
                  y={y - 4}
                  width={w - rw - lw}
                  height={8}
                  fill={`url(#${hatch})`}
                  opacity={0.6}
                />
                <text
                  x={stacked ? w - rw : w - rw + 8}
                  y={ty}
                  fill={PALETTE.gold}
                  fontSize={9}
                  textAnchor={stacked ? "end" : "start"}
                >
                  pending
                </text>
              </>
            ) : (
              <>
                {r.alo !== undefined && r.ahi !== undefined ? (
                  <line
                    x1={cx(r.alo)}
                    x2={cx(r.ahi)}
                    y1={y - 3}
                    y2={y - 3}
                    stroke={PALETTE.gray4}
                    strokeWidth={1.5}
                    opacity={0.6}
                  />
                ) : null}
                {r.blo !== undefined &&
                r.bhi !== undefined &&
                r.b !== undefined ? (
                  <line
                    x1={cx(r.blo)}
                    x2={cx(r.bhi)}
                    y1={y}
                    y2={y}
                    stroke={OUTCOME_FILL[outcome(r.b)]}
                    strokeWidth={3.5}
                    opacity={0.4}
                    strokeLinecap="round"
                  />
                ) : null}
                {r.a !== undefined && r.b !== undefined ? (
                  <>
                    <line
                      x1={cx(r.a)}
                      x2={cx(r.b)}
                      y1={y}
                      y2={y}
                      stroke={PALETTE.gray4}
                      strokeDasharray="2 2"
                    />
                    <circle
                      cx={cx(r.a)}
                      cy={y}
                      r={3.8}
                      fill="none"
                      stroke={PALETTE.gray4}
                      strokeWidth={1.3}
                    />
                  </>
                ) : null}
                {r.b !== undefined ? (
                  <circle
                    className={s.hl}
                    cx={cx(r.b)}
                    cy={y}
                    r={4.3}
                    fill={OUTCOME_FILL[outcome(r.b)]}
                  />
                ) : null}
                {r.mark && r.b !== undefined ? (
                  <text
                    x={stacked ? cx(r.b) + 7 : cx(r.b)}
                    y={stacked ? y + 3 : y - 7}
                    fill={PALETTE.gold}
                    fontSize={8.5}
                    textAnchor={stacked ? "start" : "middle"}
                    {...HALO}
                  >
                    {r.mark}
                  </text>
                ) : null}
                <text
                  x={stacked ? w - rw : w - rw + 8}
                  y={ty}
                  fill={PALETTE.gray6}
                  fontSize={9.5}
                  textAnchor={stacked ? "end" : "start"}
                  {...HALO}
                >
                  {r.lab}
                </text>
              </>
            )}
          </g>
        );
      })}
    </svg>
  );
}
