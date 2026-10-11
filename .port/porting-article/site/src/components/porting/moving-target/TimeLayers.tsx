import { memo } from "react";
import { PALETTE } from "../lib/palette";
import {
  GEO,
  lagArea,
  maxLag,
  type MtData,
  pinSteps,
  tm,
  xOf,
  yA,
  yB,
} from "./model";
import { int } from "./stats";

/** TICKS_A are the y gridlines of the zoomed top panel. */
const TICKS_A = [260000, 300000, 340000, 380000];

/** TICKS_B are the y gridlines of the bottom panel. */
const TICKS_B = [0, 300000, 600000, 900000];

/** MONTHS are the month-start gridlines of the shared x axis (month number is 0-based). */
const MONTHS: [number, string][] = [
  [6, "Jul"],
  [7, "Aug"],
  [8, "Sep"],
  [9, "Oct"],
];

/** RUNNABLE_DAY is the day the first runnable app appeared (the macOS windowing milestone). */
const RUNNABLE_DAY = "2026-07-12";

/** Joins [t, v] points into an svg points string using the given y scale. */
function poly(pts: [number, number][], y: (v: number) => number): string {
  return pts
    .map(([t, v]) => `${xOf(t).toFixed(1)},${y(v).toFixed(1)}`)
    .join(" ");
}

/**
 * The static layers of the size chart: gridlines and axis labels, the shaded
 * area the pin trails main, Ghostty main (dashed) and the pinned reference
 * (solid step) in both panels, the Rustty .rs area in the bottom panel and
 * the annotations. Memoised: hover only redraws the overlay.
 */
function TimeLayers({ d }: { d: MtData }) {
  const { L, W, R, A, B } = GEO;
  const area = lagArea(d);
  const steps = pinSteps(d);
  const gh = d.gh.map(([day, v]) => [tm(day), v] as [number, number]);
  const rs = d.rs.map(([day, v]) => [tm(day), v] as [number, number]);
  const lag = maxLag(d);
  const lagT = tm(lag.date);
  const noteT = lagT - 26 * 864e5;
  const lastRs = rs[rs.length - 1];
  const runnable = rs.find(([t]) => t === tm(RUNNABLE_DAY));
  const year = new Date(GEO.x0).getUTCFullYear();
  const polygon = [...area.top, ...[...area.bottom].reverse()];
  return (
    <g>
      {TICKS_A.map((v) => (
        <g key={`a${v}`}>
          <line x1={L} x2={W - R} y1={yA(v)} y2={yA(v)} stroke="#1d201e" />
          <text x={L - 8} y={yA(v) + 3} textAnchor="end">{`${v / 1000}k`}</text>
        </g>
      ))}
      {TICKS_B.map((v) => (
        <g key={`b${v}`}>
          <line x1={L} x2={W - R} y1={yB(v)} y2={yB(v)} stroke="#1d201e" />
          <text x={L - 8} y={yB(v) + 3} textAnchor="end">{`${v / 1000}k`}</text>
        </g>
      ))}
      <text x={L} y={A.t - 2} fill={PALETTE.gray5}>
        Ghostty .zig lines (zoomed: 260k–380k)
      </text>
      <text x={L} y={B.t - 6} fill={PALETTE.gray5}>
        Same axis from zero: Rustty .rs lines vs the pinned Ghostty
      </text>
      {MONTHS.map(([m, name]) => {
        const x = xOf(Date.UTC(year, m, 1, 12));
        return (
          <g key={name}>
            <line x1={x} x2={x} y1={A.t} y2={B.t + B.h} stroke="#171a18" />
            <text x={x + 3} y={B.t + B.h + 14}>
              {name}
            </text>
          </g>
        );
      })}
      <polygon points={poly(polygon, yA)} fill="rgba(100,216,198,.16)" />
      <polyline
        points={poly(gh, yA)}
        fill="none"
        stroke={PALETTE.upstream}
        strokeWidth={1.5}
        strokeDasharray="4 3"
      />
      <polyline
        points={poly(steps, yA)}
        fill="none"
        stroke={PALETTE.upstream}
        strokeWidth={2}
      />
      <line
        x1={xOf(lagT)}
        x2={xOf(lagT)}
        y1={yA(lag.main)}
        y2={yA(lag.pin)}
        stroke={PALETTE.gray6}
      />
      <line
        x1={xOf(lagT)}
        y1={yA((lag.main + lag.pin) / 2)}
        x2={xOf(noteT)}
        y2={yA(352000)}
        stroke={PALETTE.gray3}
      />
      <text
        x={xOf(noteT) - 4}
        y={yA(356000)}
        textAnchor="end"
        fill={PALETTE.gray6}
      >
        {`${(lag.lag / 1000).toFixed(1)}k lines behind (${lag.date.slice(5)})`}
      </text>
      <text x={xOf(noteT) - 4} y={yA(356000) + 12} textAnchor="end">
        during the Codex audit and a quiet stretch
      </text>
      <polygon
        points={`${xOf(rs[0][0])},${yB(0)} ${poly(rs, yB)} ${xOf(lastRs[0])},${yB(0)}`}
        fill="rgba(217,121,52,.12)"
      />
      <polyline
        points={poly(rs, yB)}
        fill="none"
        stroke={PALETTE.rust}
        strokeWidth={2}
      />
      <polyline
        points={poly(steps, yB)}
        fill="none"
        stroke={PALETTE.upstream}
        strokeWidth={2}
      />
      <text
        x={xOf(lastRs[0]) - 4}
        y={yB(lastRs[1]) - 6}
        textAnchor="end"
        fill={PALETTE.rust}
      >
        {int(lastRs[1])}
      </text>
      <text
        x={xOf(lastRs[0]) - 4}
        y={yB(d.adv[d.adv.length - 1].loc) - 6}
        textAnchor="end"
        fill={PALETTE.upstream}
      >
        {int(d.adv[d.adv.length - 1].loc)}
      </text>
      {runnable ? (
        <text x={xOf(runnable[0])} y={yB(runnable[1]) - 18} textAnchor="middle">
          first runnable app
        </text>
      ) : null}
    </g>
  );
}

export default memo(TimeLayers);
