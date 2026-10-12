"use client";

import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { tipHandlers } from "./hooks";
import { tm, waterfallStepTime } from "./model";
import s from "./PerfReplay.module.css";
import type { PrData } from "./types";

/**
 * "Who moved the key benchmarks": one small bar chart per key benchmark, Rust
 * ms per official step with the same-session Zig line dashed, gold deltas for
 * steps that cut more than 5%, and the first commit and round under each bar.
 * Steps whose commits are later than the replay cursor are dimmed.
 */
export default function Waterfalls({
  data,
  cur,
  tip,
}: {
  data: PrData;
  cur: number;
  tip: ChartTip;
}) {
  const now = tm(data.stages[cur].t);
  const handlers = tipHandlers(tip);
  return (
    <div className={s.wf}>
      {data.waterfall.map((w, k) => {
        const rs = w.steps;
        const first = rs[0].rust;
        const last = rs[rs.length - 1];
        const mx = Math.max(...rs.map((r) => Math.max(r.rust, r.zig))) * 1.12;
        const n = rs.length;
        const bw = Math.min(40, 250 / n - 8);
        const Y = (v: number) => 140 - (v / mx) * 118;
        return (
          <div key={w.bench} className={s.wfp}>
            <div className={s.wft}>
              {w.bench}
              <span>
                {first.toFixed(1)} → {last.rust.toFixed(1)} ms ·{" "}
                {(last.zig / last.rust).toFixed(2)}× vs Zig
              </span>
            </div>
            <svg
              className={s.svg}
              viewBox="0 0 300 176"
              role="application"
              aria-roledescription="bar chart"
              // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
              tabIndex={0}
              aria-label={`${w.bench}: Rust ms per official step against same-session Zig. Arrow keys move between steps.`}
              {...handlers}
            >
              <line x1={20} x2={296} y1={140} y2={140} stroke="#262927" />
              {rs.map((r, i) => {
                const x = 30 + i * (262 / n);
                const id = `wf:${k}:${i}`;
                const prev = i ? rs[i - 1].rust : null;
                const d = prev === null ? 0 : r.rust - prev;
                const later = waterfallStepTime(data, r) > now + 1000;
                return (
                  <g
                    key={r.step}
                    data-tip={id}
                    data-on={tip.shown === id || undefined}
                    opacity={later ? 0.2 : 1}
                  >
                    <rect
                      className={s.hl}
                      x={x}
                      y={Y(r.rust)}
                      width={bw}
                      height={140 - Y(r.rust)}
                      fill={r.rust < r.zig ? PALETTE.rust : PALETTE.fail}
                      rx={2}
                    />
                    <line
                      x1={x - 3}
                      x2={x + bw + 3}
                      y1={Y(r.zig)}
                      y2={Y(r.zig)}
                      stroke={PALETTE.upstream}
                      strokeWidth={1.4}
                      strokeDasharray="3 2"
                    />
                    <text
                      x={x + bw / 2}
                      y={Y(r.rust) - 4}
                      fill={PALETTE.gray7}
                      fontSize={9}
                      textAnchor="middle"
                    >
                      {r.rust.toFixed(1)}
                    </text>
                    {prev !== null ? (
                      <text
                        x={x + bw / 2}
                        y={Y(r.rust) - 14}
                        fill={d < -0.05 * prev ? PALETTE.gold : PALETTE.gray4}
                        fontSize={8.5}
                        textAnchor="middle"
                      >
                        {d > 0 ? "+" : d < 0 ? "−" : ""}
                        {Math.abs(d).toFixed(0)}
                      </text>
                    ) : null}
                    <text
                      x={x + bw / 2}
                      y={152}
                      fill={PALETTE.rust}
                      fontSize={8.5}
                      textAnchor="middle"
                    >
                      {r.commits[0] ?? "—"}
                    </text>
                    <text
                      x={x + bw / 2}
                      y={163}
                      fill={PALETTE.gray4}
                      fontSize={7.5}
                      textAnchor="middle"
                    >
                      {r.round}
                    </text>
                    {r.commits.length > 1 ? (
                      <text
                        x={x + bw / 2}
                        y={173}
                        fill={PALETTE.gray4}
                        fontSize={7.5}
                        textAnchor="middle"
                      >
                        +{r.commits.length - 1} more
                      </text>
                    ) : null}
                  </g>
                );
              })}
            </svg>
          </div>
        );
      })}
    </div>
  );
}
