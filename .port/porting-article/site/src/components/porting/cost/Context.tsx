import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { type CostData, fmtTok } from "./model";
import {
  BUSY_MIN_HOURS,
  concurrencyPeak,
  contextGrowth,
  CTX_MIN_CALLS,
} from "./stats";
import TipSvg from "./TipSvg";

/** SOURCE_COLOR is the colour of each activity source. */
export const SOURCE_COLOR: Record<string, string> = {
  "mac/claude": "#d97934",
  "rvm/claude": "#98c379",
  "mac/codex": "#61aeee",
};

/** Context plot frame: left, width and the y of zero. */
const C = { X: 36, W: 410, Y0: 132 };

/** Maps context tokens to y (1M tokens = 100 px). */
function cy(v: number): number {
  return C.Y0 - (v / 1e6) * 100;
}

/**
 * Context tokens per call by Monday week (Claude: p90 bar, median dot, max
 * tick, a dashed median line over weeks with enough calls; Codex: diamond at
 * its median) against the 1M-token window, then agent concurrency per day
 * and source (agent-thread hours ÷ busy hours, dot size = threads; days
 * under half a busy hour are hollow artifacts).
 */
export default function Context({
  d,
  tip,
  tipId,
}: {
  d: CostData;
  tip: ChartTip;
  tipId: string;
}) {
  const weeks = [...new Set(d.ctx.map((c) => c.w))].sort();
  const bw = (C.W / weeks.length) * 0.55;
  /** Maps a week index to the x of its centre. */
  const x = (i: number) => C.X + ((i + 0.5) * C.W) / weeks.length;
  const growth = contextGrowth(d);
  const line: string[] = [];
  const days = [...new Set(d.act.map((a) => a.d))].sort();
  /** Maps a day to the x of its centre. */
  const dx = (day: string) =>
    C.X + ((days.indexOf(day) + 0.5) * C.W) / days.length;
  /** Maps a concurrency ratio to y (8× = 70 px). */
  const ky = (v: number) => 96 - (v / 8) * 70;
  const peak = concurrencyPeak(d);
  return (
    <>
      <TipSvg
        tip={tip}
        tipId={tipId}
        viewBox="0 0 460 150"
        label="Context tokens per call by week: median, p90 and maximum"
      >
        <text x={0} y={10} fontSize={9.5} fill={PALETTE.gray4}>
          context tokens per call, by week · bar = p90 · dot = median · ◆ Codex
        </text>
        {[0, 250e3, 500e3, 750e3, 1e6].map((v) => (
          <g key={v}>
            <line
              x1={C.X}
              x2={C.X + C.W}
              y1={cy(v)}
              y2={cy(v)}
              stroke={PALETTE.gray2}
              strokeWidth={0.6}
            />
            <text
              x={C.X - 4}
              y={cy(v) + 3}
              fontSize={8.5}
              fill={PALETTE.gray4}
              textAnchor="end"
            >
              {v ? fmtTok(v) : 0}
            </text>
          </g>
        ))}
        <line
          x1={C.X}
          x2={C.X + C.W}
          y1={cy(1e6)}
          y2={cy(1e6)}
          stroke={PALETTE.fail}
          strokeDasharray="3 3"
          strokeOpacity={0.6}
        />
        <text
          x={C.X + C.W}
          y={cy(1e6) - 3}
          fontSize={8.5}
          fill={PALETTE.fail}
          textAnchor="end"
        >
          1M context window
        </text>
        {weeks.map((w, i) => {
          const ci = d.ctx.findIndex((r) => r.w === w && r.tool === "claude");
          const xi = d.ctx.findIndex((r) => r.w === w && r.tool === "codex");
          const cl = d.ctx[ci];
          const few = cl && cl.n < CTX_MIN_CALLS;
          if (cl && !few) line.push(`${x(i)},${cy(cl.med)}`);
          return (
            <g key={w}>
              {cl ? (
                <g data-tip={`ctx:${ci}`}>
                  <rect
                    x={x(i) - bw / 2}
                    y={cy(cl.p90)}
                    width={bw}
                    height={C.Y0 - cy(cl.p90)}
                    fill={PALETTE.rust}
                    fillOpacity={few ? 0.12 : 0.28}
                  />
                  <line
                    x1={x(i) - bw / 2}
                    x2={x(i) + bw / 2}
                    y1={cy(cl.mx)}
                    y2={cy(cl.mx)}
                    stroke={PALETTE.gray4}
                  />
                  <circle cx={x(i)} cy={cy(cl.med)} r={3} fill={PALETTE.rust} />
                  <rect
                    x={x(i) - bw / 2}
                    y={cy(1e6)}
                    width={bw}
                    height={C.Y0 - cy(1e6)}
                    fill="transparent"
                  />
                </g>
              ) : null}
              {xi >= 0 ? (
                <path
                  data-tip={`ctx:${xi}`}
                  d={`M${x(i) + bw / 2 + 4} ${cy(d.ctx[xi].med) - 4}l4 4l-4 4l-4-4z`}
                  fill={PALETTE.blue}
                />
              ) : null}
              {i % 2 === 0 ? (
                <text
                  x={x(i)}
                  y={145}
                  fontSize={8.5}
                  fill={PALETTE.gray4}
                  textAnchor="middle"
                >
                  {w.slice(5)}
                </text>
              ) : null}
            </g>
          );
        })}
        <polyline
          points={line.join(" ")}
          fill="none"
          stroke={PALETTE.rust}
          strokeWidth={1.2}
          strokeDasharray="2 2"
        />
        <text
          x={x(weeks.indexOf(growth.from.w)) + 8}
          y={cy(growth.from.med) + 3}
          fontSize={9.5}
          fill={PALETTE.gray8}
        >
          {fmtTok(growth.from.med)}
        </text>
        <text
          x={x(weeks.indexOf(growth.to.w)) + 8}
          y={cy(growth.to.med) + 3}
          fontSize={9.5}
          fill={PALETTE.gray8}
        >
          {fmtTok(growth.to.med)}
        </text>
        <text
          x={x(weeks.indexOf(growth.from.w)) + 6}
          y={cy(growth.max) + 10}
          fontSize={8.5}
          fill={PALETTE.gray4}
        >
          {`max ~${fmtTok(growth.max)}`}
        </text>
      </TipSvg>
      <TipSvg
        tip={tip}
        tipId={tipId}
        viewBox="0 0 460 112"
        label="Agent concurrency per day and source"
      >
        <text x={0} y={10} fontSize={9.5} fill={PALETTE.gray4}>
          concurrency per day = agent-thread hours ÷ busy wall-clock hours · dot
          size = threads
        </text>
        {[1, 2, 4, 8].map((v) => (
          <g key={v}>
            <line
              x1={C.X}
              x2={C.X + C.W}
              y1={ky(v)}
              y2={ky(v)}
              stroke={PALETTE.gray2}
              strokeWidth={0.6}
            />
            <text
              x={C.X - 4}
              y={ky(v) + 3}
              fontSize={8.5}
              fill={PALETTE.gray4}
              textAnchor="end"
            >
              {`${v}×`}
            </text>
          </g>
        ))}
        {d.act.map((a, i) => {
          const r = a.a / a.b;
          const small = a.b < BUSY_MIN_HOURS;
          const xx =
            dx(a.d) + (a.s === "rvm/claude" ? -2 : a.s === "mac/codex" ? 2 : 0);
          const col = SOURCE_COLOR[a.s];
          return (
            <g key={`${a.d}-${a.s}`} data-tip={`act:${i}`}>
              <line
                x1={xx}
                x2={xx}
                y1={ky(1)}
                y2={ky(r)}
                stroke={col}
                strokeOpacity={0.4}
              />
              <circle
                cx={xx}
                cy={ky(r)}
                r={Math.min(7, 1.5 + Math.sqrt(a.th) / 4)}
                fill={small ? PALETTE.card : col}
                fillOpacity={small ? 1 : 0.8}
                stroke={col}
              />
            </g>
          );
        })}
        <text
          x={dx(peak.d) + 9}
          y={ky(peak.ratio) + 3}
          fontSize={9.5}
          fill={PALETTE.gray8}
        >
          {`${peak.ratio.toFixed(1)}× on ${peak.d.slice(5)} (${peak.threads} threads)`}
        </text>
        {[days[0], "2026-08-01", "2026-09-01", "2026-10-01"]
          .filter((day) => days.includes(day))
          .map((day) => (
            <text
              key={day}
              x={dx(day)}
              y={110}
              fontSize={8.5}
              fill={PALETTE.gray4}
              textAnchor="middle"
            >
              {day.slice(5)}
            </text>
          ))}
      </TipSvg>
    </>
  );
}
