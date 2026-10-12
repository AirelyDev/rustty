import type { ReactNode } from "react";
import { PALETTE } from "../lib/palette";
import { linear } from "../lib/scales";
import Hot, { TipBody } from "./Hot";
import { dayOffset, stepPath } from "./model";
import s from "./StaticCharts.module.css";
import KeyGroup from "../ui/KeyGroup";

/** One step series: its legend label, colour, optional dash and dated values. */
export type StepSeries = {
  label: string;
  colour: string;
  dash?: string;
  points: { d: string; v: number }[];
};

/** One hover column: the date it starts at, its accessible label and tooltip body. */
export type StepHot = {
  d: string;
  label: string;
  title: ReactNode;
  rows: ReactNode[];
};

/** MONTHS are the month ticks drawn on every step chart, with their labels. */
const MONTHS: [string, string][] = [
  ["2026-07-01", "Jul"],
  ["2026-08-01", "Aug"],
  ["2026-09-01", "Sep"],
  ["2026-10-01", "Oct"],
];

/** MIN_HOT is the narrowest hover column in viewBox px. */
const MIN_HOT = 12;

/**
 * A step-line chart over the port's dates in a W × H viewBox: each series
 * holds its value until the next point and runs to the end of the range, with
 * its last value labelled at the right. Hover columns start at each hot's
 * date and run to the next one.
 */
export default function StepChart({
  W,
  H,
  range,
  yDomain,
  yTicks,
  title,
  series,
  hots,
  idp,
  ariaLabel,
  left = 34,
}: {
  W: number;
  H: number;
  range: [string, string];
  yDomain: [number, number];
  yTicks: number[];
  title: string;
  series: StepSeries[];
  hots: StepHot[];
  idp: string;
  ariaLabel: string;
  /** Left edge of the plot in viewBox px (to align with a chart above). */
  left?: number;
}) {
  const right = W - 34;
  const top = 30;
  const bottom = H - 18;
  const x = linear([0, dayOffset(range[1], range[0])], [left, right]);
  /** Maps an ISO date to x. */
  const xd = (d: string) => x(dayOffset(d, range[0]));
  const y = linear(yDomain, [bottom, top]);
  const hx = hots.map((h) => xd(h.d) - MIN_HOT / 2);
  return (
    <div className={s.plot}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className={s.svg}
        role="img"
        aria-label={ariaLabel}
      >
        <text x={left} y={16} className={s.panelText}>
          {title}
        </text>
        {yTicks.map((v) => (
          <g key={v}>
            <line
              x1={left}
              x2={right}
              y1={y(v)}
              y2={y(v)}
              stroke={PALETTE.gray2}
              strokeDasharray="2 4"
            />
            <text
              x={left - 4}
              y={y(v) + 3}
              textAnchor="end"
              className={s.axisText}
            >
              {v}
            </text>
          </g>
        ))}
        {MONTHS.map(([d, label]) => (
          <text
            key={d}
            x={xd(d)}
            y={H - 4}
            textAnchor="middle"
            className={s.axisText}
          >
            {label}
          </text>
        ))}
        <line
          x1={left}
          x2={right}
          y1={bottom}
          y2={bottom}
          stroke={PALETTE.gray3}
        />
        {series.map((sr) => {
          const pts = sr.points.map(
            (p) => [xd(p.d), y(p.v)] as [number, number],
          );
          const last = sr.points[sr.points.length - 1];
          return (
            <g key={sr.label}>
              <path
                d={stepPath(pts, right)}
                fill="none"
                stroke={sr.colour}
                strokeWidth={2}
                strokeDasharray={sr.dash}
              />
              {pts.map(([px, py], i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: points are fixed and ordered
                <circle key={i} cx={px} cy={py} r={2} fill={sr.colour} />
              ))}
              <text
                x={right + 4}
                y={y(last.v) + 3}
                className={s.valText}
                fill={sr.colour}
              >
                {last.v}
              </text>
            </g>
          );
        })}
      </svg>
      <KeyGroup
        label={`${title}: ${hots.length} points. Arrow keys move between them.`}
      >
        {hots.map((h, i) => {
          const x0 = Math.max(left - MIN_HOT / 2, hx[i]);
          const x1 =
            i + 1 < hots.length
              ? Math.max(hx[i + 1], x0 + MIN_HOT)
              : right + 30;
          return (
            <Hot
              key={h.d}
              id={`${idp}-${h.d}`}
              x={x0}
              y={top}
              w={x1 - x0}
              h={bottom - top}
              W={W}
              H={H}
              label={h.label}
            >
              <TipBody title={h.title} rows={h.rows} />
            </Hot>
          );
        })}
      </KeyGroup>
    </div>
  );
}
