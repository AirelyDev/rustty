import { PALETTE } from "../lib/palette";
import {
  type CumPoint,
  cumulative,
  dateX,
  GEO,
  type MsEvent,
  SERIES_DEFS,
} from "./model";
import s from "./MemorySafety.module.css";

/** CURVE_COLOURS are the cumulative curves' stroke colours (all white, Miri purple, production red). */
const CURVE_COLOURS = {
  all: PALETTE.gray8,
  miri: PALETTE.purple,
  prod: PALETTE.fail,
} as const;

/** Y_MAX is the top of the cumulative y axis. */
const Y_MAX = 90;

/** Returns the SVG polyline points of a cumulative series: a flat run, then a riser on each event day. */
export function curvePoints(
  pts: CumPoint[],
  x: (d: string) => number,
  y: (v: number) => number,
): string {
  const out: string[] = [];
  for (const p of pts) {
    out.push(`${x(p.d).toFixed(1)},${y(p.before).toFixed(1)}`);
    if (p.v !== p.before) out.push(`${x(p.d).toFixed(1)},${y(p.v).toFixed(1)}`);
  }
  return out.join(" ");
}

/**
 * The cumulative curves under the swimlanes, on the same date axis: all
 * defects (white), found by Miri (purple, dashed), in production code (red),
 * with end labels and the note on the stretch with no curated entries.
 */
export default function Cumulative({
  events,
  days,
  top,
}: {
  events: MsEvent[];
  days: string[];
  /** Y of the plot's top edge. */
  top: number;
}) {
  const { L, R, cumH } = GEO;
  const y = (v: number) => top + cumH - (v / Y_MAX) * cumH;
  return (
    <g>
      <text x={L} y={top - 8} fill={PALETTE.gray5}>
        Cumulative defects
      </text>
      {[0, 30, 60, 90].map((v) => (
        <g key={v}>
          <line x1={L} x2={R} y1={y(v)} y2={y(v)} stroke="#191c1a" />
          <text x={L - 8} y={y(v) + 3} textAnchor="end">
            {v}
          </text>
        </g>
      ))}
      {SERIES_DEFS.map((def, si) => {
        const pts = cumulative(events, def.test, days);
        const end = pts[pts.length - 1].v;
        const colour = CURVE_COLOURS[def.key];
        return (
          <g key={def.key}>
            <polyline
              points={curvePoints(pts, dateX, y)}
              fill="none"
              stroke={colour}
              strokeWidth={si ? 1.6 : 2}
              strokeDasharray={si === 1 ? "5 3" : undefined}
            />
            <text
              x={R + 4}
              y={y(end) + (si === 2 ? 10 : 3)}
              fill={colour}
              className={s.endLabel}
            >
              {end}
            </text>
            <text x={dateX("2026-08-05")} y={top + 4 + si * 12} fill={colour}>
              {def.label}
            </text>
          </g>
        );
      })}
      <text
        x={dateX("2026-08-20")}
        y={y(52)}
        textAnchor="middle"
        fill={PALETTE.gray4}
      >
        no curated entries 08-26 → 09-22
      </text>
      <text
        x={dateX("2026-08-20")}
        y={y(52) + 12}
        textAnchor="middle"
        fill={PALETTE.gray4}
      >
        (a floor: the port-findings ledger opened the week of 09-07)
      </text>
    </g>
  );
}
