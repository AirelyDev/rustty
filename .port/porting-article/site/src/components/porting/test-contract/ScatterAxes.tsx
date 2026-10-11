import { PALETTE } from "../lib/palette";
import { log1p10 } from "./model";

/** W is the scatter's viewBox width. */
export const W = 460;
/** H is the scatter's viewBox height. */
export const H = 380;
/** M holds the scatter's plot margins. */
const M = { l: 44, r: 12, t: 10, b: 36 };
/** PW and PH are the plot area's width and height. */
const PW = W - M.l - M.r;
/** PH is the plot area's height. */
const PH = H - M.t - M.b;
/** MAXV is the largest axis value (both axes). */
const MAXV = log1p10(700);
/** TICKS are the axis tick values. */
const TICKS = [0, 1, 3, 10, 30, 100, 300];
/** Maps a Zig count to x. */
export function X(v: number): number {
  return M.l + (log1p10(v) / MAXV) * PW;
}

/** Maps a Rust count to y. */
export function Y(v: number): number {
  return M.t + PH - (log1p10(v) / MAXV) * PH;
}

/** Returns the y = k·x reference path over the axis range. */
function refPath(k: number): string {
  let d = "";
  for (let v = 0; v <= 700; v += v < 10 ? 0.25 : 5) {
    if (k * v > 700) break;
    d += `${v ? "L" : "M"}${X(v).toFixed(1)},${Y(k * v).toFixed(1)}`;
  }
  return d;
}

/** DIAG is the y = x path. */
const DIAG = refPath(1);
/** DOUBLE is the y = 2x path. */
const DOUBLE = refPath(2);

/** Gridlines, tick labels, the y = x and y = 2x references, the dropped-test zone (with its module count) and the axis titles. */
export default function ScatterAxes({ dropped }: { dropped: number }) {
  return (
    <>
      {TICKS.map((t) => (
        <g key={t}>
          <line x1={X(t)} x2={X(t)} y1={M.t} y2={M.t + PH} stroke="#1b1e1d" />
          <text x={X(t)} y={H - 20} textAnchor="middle">
            {t}
          </text>
          <line x1={M.l} x2={M.l + PW} y1={Y(t)} y2={Y(t)} stroke="#1b1e1d" />
          <text x={M.l - 6} y={Y(t) + 3} textAnchor="end">
            {t}
          </text>
        </g>
      ))}
      <path
        d={`${DIAG} L${X(700)},${Y(0)} L${X(0)},${Y(0)}Z`}
        fill={PALETTE.fail}
        opacity={0.05}
      />
      <path d={DIAG} stroke={PALETTE.gray5} strokeDasharray="4 3" fill="none" />
      <path
        d={DOUBLE}
        stroke={PALETTE.gray3}
        strokeDasharray="1 3"
        fill="none"
      />
      <text x={X(60) + 4} y={Y(60) + 12} fill={PALETTE.gray5}>
        Rust = Zig
      </text>
      <text x={X(14)} y={Y(28) - 6} textAnchor="end" fill={PALETTE.gray3}>
        2×
      </text>
      <text
        x={X(40)}
        y={Y(1.2)}
        textAnchor="middle"
        fill={PALETTE.fail}
        opacity={0.7}
      >
        dropped-test zone · {dropped} modules
      </text>
      <text
        x={M.l + PW / 2}
        y={H - 3}
        textAnchor="middle"
        fill={PALETTE.upstream}
      >
        Zig tests per module →
      </text>
      <text
        transform={`translate(10,${M.t + PH / 2}) rotate(-90)`}
        textAnchor="middle"
        fill={PALETTE.rust}
      >
        Rust tests per module →
      </text>
    </>
  );
}
