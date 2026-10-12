"use client";

import { log } from "../lib/scales";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { tipHandlers, type Tween } from "./hooks";
import { interp, outcome, ratio, spread } from "./model";
import s from "./PerfReplay.module.css";
import type { PrData } from "./types";

/** Geometry of one layout: width, label column, value column, top, row height, ticks. */
type Geo = {
  W: number;
  L: number;
  RR: number;
  top: number;
  rh: number;
  ticks: number[];
  stacked: boolean;
};

/** WIDE is the desktop layout (mockup: 640 wide, labels left, values right). */
const WIDE: Geo = {
  W: 640,
  L: 162,
  RR: 128,
  top: 30,
  rh: 26,
  ticks: [0.25, 0.333, 0.5, 0.667, 1, 1.5, 2, 3, 4],
  stacked: false,
};

/** STACKED is the phone layout: label and value text above each row's line, fewer ticks. */
const STACKED: Geo = {
  W: 360,
  L: 8,
  RR: 8,
  top: 30,
  rh: 38,
  ticks: [0.25, 0.5, 1, 2, 4],
  stacked: true,
};

/** DOMAIN is the ratio axis: 4× faster to 4× slower. */
const DOMAIN: [number, number] = [0.25, 4];

/** TICK_TEXT labels the axis ticks. */
const TICK_TEXT: Record<number, string> = {
  0.25: "4×",
  0.333: "3×",
  0.5: "2×",
  0.667: "1.5×",
  1: "Zig",
  1.5: "1.5×",
  2: "2×",
  3: "3×",
  4: "4×",
};

/** OUTCOME_FILL colours a dot: copper win, red loss, grey tie. */
export const OUTCOME_FILL = {
  win: PALETTE.rust,
  loss: PALETTE.fail,
  tie: PALETTE.gray5,
} as const;

/** Clamps a ratio into the axis domain. */
function clamp(v: number): number {
  return Math.max(DOMAIN[0], Math.min(DOMAIN[1], v));
}

/** Props for the replay dumbbell. */
type DumbbellProps = {
  data: PrData;
  order: string[];
  tween: Tween;
  narrow: boolean;
  tip: ChartTip;
  /** Called with the benchmark under the pointer or keyboard cursor (terminal title). */
  onBench: (b: string) => void;
};

/**
 * The main replay: one row per benchmark on a log Rust ÷ Zig axis, with the
 * hollow 07-15 start, a dashed trail from the previous step, the dot at the
 * current step (tweened), the run-to-run range bar and a gold ring on
 * benchmarks that moved outside their spread at this step.
 */
export default function Dumbbell({
  data,
  order,
  tween,
  narrow,
  tip,
  onBench,
}: DumbbellProps) {
  const g = narrow ? STACKED : WIDE;
  const X = log(DOMAIN, [g.L, g.W - g.RR]);
  const from = data.stages[tween.from];
  const to = data.stages[tween.to];
  const moved = new Set(to.moved.map((m) => m[0]));
  const floor = new Set(data.floor);
  const bottom = g.top + g.rh * order.length;
  const handlers = tipHandlers(tip, {
    axis: "vertical",
    onItem: (id) => {
      if (id.startsWith("row:")) onBench(id.slice(4));
    },
  });

  return (
    <svg
      className={s.svg}
      viewBox={`0 0 ${g.W} ${bottom + 34}`}
      role="application"
      aria-roledescription="chart"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
      tabIndex={0}
      aria-label="Rust over Zig ratio per benchmark at the selected step. Up and down arrows move between benchmarks and show their numbers; left and right step the replay."
      {...handlers}
    >
      <text x={X(0.25)} y={12} fill={PALETTE.rust} fontSize={10}>
        ◂ Rustty faster
      </text>
      <text x={X(4)} y={12} fill={PALETTE.fail} fontSize={10} textAnchor="end">
        Rustty slower ▸
      </text>
      {g.ticks.map((t) => (
        <g key={t}>
          <line
            x1={X(t)}
            x2={X(t)}
            y1={g.top - 8}
            y2={bottom}
            stroke={t === 1 ? PALETTE.upstream : "#1c1f1d"}
            strokeWidth={t === 1 ? 1.5 : 1}
          />
          <text
            x={X(t)}
            y={bottom + 14}
            fill={t === 1 ? PALETTE.upstream : PALETTE.gray4}
            fontSize={9.5}
            textAnchor="middle"
          >
            {TICK_TEXT[t]}
          </text>
        </g>
      ))}
      <text x={g.L} y={bottom + 28} fill={PALETTE.gray4} fontSize={9}>
        Rust ÷ Zig, log scale (same-session medians)
      </text>
      {order.map((b, i) => {
        const yc = g.top + i * g.rh + g.rh / 2;
        const y = g.stacked ? yc + 7 : yc;
        const ty = g.stacked ? yc - 6 : yc + 3.5;
        const a = ratio(from, b);
        const z = ratio(to, b);
        const v = interp(a, z, tween.t);
        const [lo, hi] = spread(to, b);
        const x = X(clamp(v));
        const id = `row:${b}`;
        const on = tip.shown === id;
        return (
          <g key={b} data-tip={id} data-on={on || undefined}>
            <rect
              x={0}
              y={yc - g.rh / 2}
              width={g.W}
              height={g.rh}
              className={on ? s.rowOn : undefined}
              fill="transparent"
            />
            <text
              x={g.stacked ? g.L : g.L - 10}
              y={ty}
              fill={floor.has(b) ? PALETTE.gray4 : PALETTE.gray7}
              fontSize={10.5}
              textAnchor={g.stacked ? "start" : "end"}
            >
              {b}
              {g.stacked && floor.has(b) ? " · floor" : ""}
            </text>
            {!g.stacked && floor.has(b) ? (
              <text
                x={g.L - 10}
                y={yc + 13}
                fill={PALETTE.gray4}
                fontSize={8}
                textAnchor="end"
              >
                floor
              </text>
            ) : null}
            <line
              x1={X(clamp(lo))}
              x2={X(clamp(hi))}
              y1={y}
              y2={y}
              stroke={OUTCOME_FILL[outcome(z)]}
              strokeWidth={3.5}
              strokeLinecap="round"
              opacity={0.38}
            />
            <line
              x1={X(clamp(a))}
              x2={x}
              y1={y}
              y2={y}
              stroke={PALETTE.gray4}
              strokeDasharray="2 2"
            />
            <circle
              cx={X(clamp(ratio(data.stages[0], b)))}
              cy={y}
              r={4.5}
              fill="none"
              stroke={PALETTE.gray4}
              strokeWidth={1.3}
            />
            <circle
              cx={x}
              cy={y}
              r={8.5}
              fill="none"
              stroke={PALETTE.gold}
              strokeWidth={1.3}
              opacity={moved.has(b) ? 1 : 0}
            />
            <circle
              className={s.hl}
              cx={x}
              cy={y}
              r={5}
              fill={OUTCOME_FILL[outcome(v)]}
            />
            <text
              x={g.stacked ? g.W - g.RR : g.W - g.RR + 8}
              y={ty}
              fill={PALETTE.gray6}
              fontSize={10}
              textAnchor={g.stacked ? "end" : "start"}
            >
              {v.toFixed(3)} {to.rust[b][0].toFixed(1)}/
              {to.zig[b][0].toFixed(1)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
