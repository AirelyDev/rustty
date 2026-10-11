import { PALETTE } from "../lib/palette";
import { fmt, type TcData } from "./model";
import s from "./TestContract.module.css";

/** W is the chart's viewBox width. */
export const W = 460;
/** H is the chart's viewBox height. */
export const H = 300;
/** M holds the chart's plot margins. */
export const M = { l: 44, r: 14, t: 12, b: 26 };
/** PW is the plot area's width. */
export const PW = W - M.l - M.r;
/** PH is the plot area's height. */
export const PH = H - M.t - M.b;
/** YMAX is the y-axis maximum (tests). */
const YMAX = 8000;
/** Y_TICKS are the y-axis gridline values. */
const Y_TICKS = [0, 2000, 4000, 6000, 8000];
/** X_TICKS are the dated x-axis labels (MM-DD). */
const X_TICKS = ["06-21", "07-15", "08-10", "09-05", "10-01"];
/** FIRST_CONTRACT is the first-day contract counting identifier-named Zig tests. */
const FIRST_CONTRACT = 3234;

/** Maps a value to y. */
export function Y(v: number): number {
  return M.t + PH - (v / YMAX) * PH;
}

/** Gridlines, dated x labels, the first-day contract reference and the numbered sync ticks. */
export default function MovingAxes({ data }: { data: TcData }) {
  const nd = data.dates.length;
  /** Maps a day index to x. */
  const X = (i: number) => M.l + (i / (nd - 1)) * PW;
  return (
    <>
      {Y_TICKS.map((t) => (
        <g key={t}>
          <line x1={M.l} x2={M.l + PW} y1={Y(t)} y2={Y(t)} stroke="#1b1e1d" />
          <text x={M.l - 6} y={Y(t) + 3} textAnchor="end">
            {t ? fmt(t) : 0}
          </text>
        </g>
      ))}
      {X_TICKS.map((t) => (
        <text
          key={t}
          x={X(data.dates.indexOf(`2026-${t}`))}
          y={H - 8}
          textAnchor="middle"
        >
          {t}
        </text>
      ))}
      <line
        x1={M.l}
        x2={M.l + PW}
        y1={Y(FIRST_CONTRACT)}
        y2={Y(FIRST_CONTRACT)}
        stroke={PALETTE.gray3}
        strokeDasharray="2 3"
      />
      <text x={M.l + 4} y={Y(FIRST_CONTRACT) + 12} fill={PALETTE.gray4}>
        {fmt(FIRST_CONTRACT)} · first-day contract incl. identifier-named tests
      </text>
      {data.pins.map((p, n) => {
        if (!n) return null;
        const x = X(data.dates.indexOf(p.date));
        return (
          <g key={p.sha}>
            <line
              x1={x}
              x2={x}
              y1={M.t}
              y2={M.t + PH}
              stroke={PALETTE.upstream}
              opacity={0.18}
            />
            <text
              x={x}
              y={M.t + 8}
              textAnchor="middle"
              fill={PALETTE.upstream}
              opacity={0.8}
              className={s.tick9}
            >
              {n === 4 ? "" : n === 3 ? "3·4" : n}
            </text>
          </g>
        );
      })}
    </>
  );
}
