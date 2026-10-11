"use client";

import { fmtPct } from "../lib/format";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import type { ChartTip } from "../ui/useChartTip";
import { tipHandlers } from "./hooks";
import s from "./PerfReplay.module.css";
import type { PrData } from "./types";

/** FIX_COMMIT is the commit that landed the compile-time-flattening fixes (24 files, +1178/−171). */
const FIX_COMMIT = "1d50073f6";

/** Formats a step value: "3.9×" for per-op ratios, "+85%" for wall-clock gaps. */
function val(v: number, ratio: boolean): string {
  return ratio ? `${v}×` : fmtPct(v, v % 1 ? 1 : 0);
}

/**
 * The compile-time-flattening chapter: one step-bar panel per benchmark (the
 * gap to Zig after each fix, labelled with its hazard subclass) and the
 * ranked table of the eleven hazard classes.
 */
export default function Comptime({
  data,
  tip,
}: {
  data: PrData;
  tip: ChartTip;
}) {
  const handlers = tipHandlers(tip);
  const enc = data.comptime[0].steps;
  return (
    <>
      <div className={s.chhead}>
        <span className={s.big}>
          {val(enc[0].value, false)} → {val(enc[enc.length - 1].value, false)}
        </span>
        <p>
          Every big gap found on 2026-08-28 was the port turning a Zig
          compile-time construct into runtime code; no upstream optimisation was
          missing. Each bar is the gap to Zig after one more fix (hyperlink
          churn is per operation, floor-corrected, as Rust ÷ Zig; the others are
          wall-clock % slower). The fixes landed in{" "}
          <CommitLink hash={FIX_COMMIT} /> (24 files, +1178/−171).
        </p>
      </div>
      <div className={s.wf}>
        {data.comptime.map((c, k) => {
          const rs = c.steps;
          const mx = Math.max(...rs.map((r) => r.value));
          const base = c.ratio ? 1 : 0;
          const h = rs.length * 30 + 10;
          return (
            <div key={c.bench} className={s.wfp}>
              <div className={s.wft}>
                {c.bench}
                <span>
                  {c.ratio ? "per-op Rust ÷ Zig" : "% slower than Zig"}
                </span>
              </div>
              <svg
                className={s.svg}
                viewBox={`0 0 300 ${h}`}
                role="application"
                aria-roledescription="bar chart"
                // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
                tabIndex={0}
                aria-label={`${c.bench}: gap to Zig after each fix. Arrow keys move between steps.`}
                {...handlers}
              >
                {rs.map((r, j) => {
                  const y = 6 + j * 30;
                  const len = ((r.value - base) / (mx - base)) * 170;
                  const id = `ct:${k}:${j}`;
                  return (
                    <g
                      key={r.order}
                      data-tip={id}
                      data-on={tip.shown === id || undefined}
                    >
                      <rect
                        className={s.hl}
                        x={len >= 0 ? 100 : 100 + len}
                        y={y}
                        width={Math.max(1.5, Math.abs(len))}
                        height={12}
                        fill={r.value > base ? PALETTE.fail : PALETTE.rust}
                        rx={2}
                      />
                      <text
                        x={96}
                        y={y + 9.5}
                        fill={PALETTE.gray7}
                        fontSize={9}
                        textAnchor="end"
                      >
                        {val(r.value, c.ratio)}
                      </text>
                      <text
                        x={100}
                        y={y + 23}
                        fill={PALETTE.gray4}
                        fontSize={8}
                      >
                        {(j === 0 ? "start" : r.cls || r.fix).slice(0, 52)}
                      </text>
                    </g>
                  );
                })}
                <line
                  x1={100}
                  x2={100}
                  y1={0}
                  y2={h - 4}
                  stroke={PALETTE.upstream}
                />
              </svg>
            </div>
          );
        })}
      </div>
      <div className={s.tblwrap}>
        <table className={s.tbl}>
          <thead>
            <tr>
              <th>#</th>
              <th>hazard class</th>
              <th>Zig construct</th>
              <th>Rust spelling that lost</th>
              <th>fix</th>
              <th>worst recorded gap</th>
            </tr>
          </thead>
          <tbody>
            {data.gaps.map((g, j) => (
              <tr key={g.class}>
                <td>{j + 1}</td>
                <td>{g.class}</td>
                <td>{g.zig}</td>
                <td>{g.lost}</td>
                <td>{g.fix}</td>
                <td className={s.m}>
                  {g.worst}
                  <br />
                  <span>{g.example}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className={s.pnote}>
        Ranked by the worst recorded gap: ratio gaps first, then share-of-time
        figures, then classes with no recorded number. Units differ by row and
        are shown as recorded.
      </p>
    </>
  );
}
