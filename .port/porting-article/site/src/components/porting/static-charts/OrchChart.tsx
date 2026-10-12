import data from "@/app/porting/data/orchestration.json";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import { linear } from "../lib/scales";
import { ctxSplit, curveThrough, dayOffset, md, streamLayers } from "./model";
import {
  CTX,
  CTX_MAX,
  DAILY,
  H,
  kTok,
  MODEL_COLOURS,
  MODEL_TEXTURE,
  STREAM,
} from "./orch-shared";
import OrchWeeks from "./OrchWeeks";
import s from "./StaticCharts.module.css";

/**
 * The orchestration chart in a W-wide viewBox: commits per week by model as
 * a streamgraph, the median context per call with its 90th-percentile band,
 * and per day the workflow scripts added (up) and agent threads (down, square
 * root scale). One hover column per week covers all three panels.
 */
export default function OrchChart({
  W,
  idp,
  labels,
}: {
  W: number;
  idp: string;
  labels: boolean;
}) {
  const left = 44;
  const right = W - 12;
  const [start, end] = data.range;
  const x = linear([0, dayOffset(end, start)], [left, right]);
  /** Maps an ISO date (or a date plus a day fraction) to x. */
  const xd = (d: string, plus = 0) => x(dayOffset(d, start) + plus);
  const weeks = data.weeks;
  const layers = streamLayers(weeks.map((w) => w.n));
  const maxTotal = Math.max(
    ...weeks.map((w) => w.n.reduce((a, b) => a + b, 0)),
  );
  const mid = (STREAM.top + STREAM.bottom) / 2;
  const k = (STREAM.bottom - STREAM.top) / maxTotal;
  const wx = weeks.map((w) => xd(w.w, 3.5));
  const ctx = ctxSplit(data.ctx, 100);
  const yc = linear([0, CTX_MAX], [CTX.bottom, CTX.top]);
  const maxScripts = Math.max(...data.scripts.map((d) => d.n));
  const maxThreads = Math.max(...data.threads.map((t) => t.claude + t.codex));
  /** Scales a script count to a bar height above the daily axis. */
  const hs = (n: number) => (n / maxScripts) * (DAILY.axis - DAILY.top);
  /** Scales a thread count to a bar height below the daily axis (square root). */
  const ht = (n: number) =>
    Math.sqrt(n / maxThreads) * (DAILY.bottom - DAILY.axis);
  const barW = Math.max(1.5, (x(1) - x(0)) * 0.8);
  const lostEnd = xd(data.totals.lostBefore);
  const hatch = `${idp}-lost`;
  const stripe = `${idp}-stripe`;
  const claudePts = ctx.claude.map((c) => ({ ...c, cx: xd(c.w, 3.5) }));
  const lastScript = data.scripts[data.scripts.length - 1];
  const months: [string, string][] = [
    ["2026-07-01", "Jul"],
    ["2026-08-01", "Aug"],
    ["2026-09-01", "Sep"],
    ["2026-10-01", "Oct"],
  ];
  return (
    <div className={s.plot}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className={s.svg}
        role="img"
        aria-label={`Commits per week by model, median context per call from ${kTok(claudePts[0].median)} to ${kTok(Math.max(...claudePts.map((c) => c.median)))} tokens, workflow scripts and agent threads per day.`}
      >
        <defs>
          <pattern
            id={hatch}
            width="6"
            height="6"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <line
              x1="0"
              y1="0"
              x2="0"
              y2="6"
              stroke={PALETTE.gray2}
              strokeWidth={2}
            />
          </pattern>
          <pattern
            id={stripe}
            width="5"
            height="5"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(-45)"
          >
            <line
              x1="0"
              y1="0"
              x2="0"
              y2="5"
              stroke={PALETTE.card}
              strokeWidth={1.4}
              strokeOpacity={0.6}
            />
          </pattern>
        </defs>
        <text x={left} y={STREAM.top - 14} className={s.panelText}>
          {labels
            ? "commits per week, by the model in each commit's trailer"
            : "commits per week, by model"}
        </text>
        {layers.map((ly, m) => {
          const topEdge = curveThrough(
            ly.map(([, y1], i) => [wx[i], mid - y1 * k] as [number, number]),
          );
          const bottomEdge = curveThrough(
            ly
              .map(([y0], i) => [wx[i], mid - y0 * k] as [number, number])
              .reverse(),
            false,
          );
          const d = `${topEdge}${bottomEdge}Z`;
          return (
            <g key={data.models[m]}>
              <path d={d} fill={MODEL_COLOURS[m]} fillOpacity={0.9} />
              {MODEL_TEXTURE[m] ? (
                <path d={d} fill={`url(#${stripe})`} />
              ) : null}
            </g>
          );
        })}
        {labels
          ? layers.map((ly, m) => {
              const thick = ly.map(([y0, y1]) => (y1 - y0) * k);
              const i = thick.indexOf(Math.max(...thick));
              // The no-trailer layer is grey; dark ink on it would not read.
              if (thick[i] < 14 || m === data.models.length - 1) return null;
              const [y0, y1] = ly[i];
              return (
                <text
                  key={data.models[m]}
                  x={wx[i]}
                  y={mid - ((y0 + y1) / 2) * k + 3}
                  textAnchor="middle"
                  className={s.inkText}
                >
                  {data.models[m]}
                </text>
              );
            })
          : null}
        {/* Transcript-based rows only (context, threads); scripts come from git. */}
        {[
          [CTX.top, CTX.bottom],
          [DAILY.axis, DAILY.bottom],
        ].map(([y0, y1]) => (
          <rect
            key={y0}
            x={left}
            y={y0}
            width={lostEnd - left}
            height={y1 - y0}
            fill={`url(#${hatch})`}
            opacity={0.7}
          />
        ))}
        <text x={left + 4} y={CTX.top + 10} className={s.axisText}>
          {labels ? "Mac transcripts lost before " : "lost before "}
          {md(data.totals.lostBefore)}
          {labels ? ": partial data" : ""}
        </text>
        <text x={left} y={CTX.top - 8} className={s.panelText}>
          {labels
            ? "context per call: median, with the 90th-percentile band"
            : "context per call: median, 90th-percentile band"}
        </text>
        {[0, 200_000, 400_000].map((v) => (
          <g key={v}>
            <line
              x1={left}
              x2={right}
              y1={yc(v)}
              y2={yc(v)}
              stroke={PALETTE.gray2}
              strokeDasharray="2 4"
            />
            <text
              x={left - 4}
              y={yc(v) + 3}
              textAnchor="end"
              className={s.axisText}
            >
              {v ? kTok(v) : 0}
            </text>
          </g>
        ))}
        <path
          d={`${claudePts.map((c, i) => `${i ? "L" : "M"}${c.cx},${yc(c.p90)}`).join("")}${[
            ...claudePts,
          ]
            .reverse()
            .map((c) => `L${c.cx},${yc(c.median)}`)
            .join("")}Z`}
          fill={PALETTE.rust}
          fillOpacity={0.18}
        />
        <path
          d={claudePts
            .map((c, i) => `${i ? "L" : "M"}${c.cx},${yc(c.median)}`)
            .join("")}
          fill="none"
          stroke={PALETTE.rust}
          strokeWidth={2}
        />
        {claudePts.map((c) => (
          <circle
            key={c.w}
            cx={c.cx}
            cy={yc(c.median)}
            r={3}
            fill={c.lowN ? PALETTE.card : PALETTE.rust}
            stroke={PALETTE.rust}
            strokeWidth={1.4}
          />
        ))}
        {ctx.codex.map((c) => {
          const cx = xd(c.w, 3.5);
          const cy = yc(c.median);
          return (
            <path
              key={c.w}
              d={`M${cx},${cy - 4.5}L${cx + 4.5},${cy}L${cx},${cy + 4.5}L${cx - 4.5},${cy}Z`}
              fill="none"
              stroke={PALETTE.blue}
              strokeWidth={1.5}
            />
          );
        })}
        <text x={left} y={DAILY.top - 8} className={s.panelText}>
          {labels
            ? "per day: workflow scripts added (up), agent threads (down)"
            : "per day: scripts added (up), threads (down)"}
        </text>
        <line
          x1={left}
          x2={right}
          y1={DAILY.axis}
          y2={DAILY.axis}
          stroke={PALETTE.gray3}
        />
        {data.scripts.map((d) => (
          <rect
            key={d.d}
            x={xd(d.d)}
            y={DAILY.axis - hs(d.n)}
            width={barW}
            height={hs(d.n)}
            fill={PALETTE.rust}
          />
        ))}
        {data.threads.map((t) => (
          <g key={t.d}>
            <rect
              x={xd(t.d)}
              y={DAILY.axis}
              width={barW}
              height={ht(t.claude + t.codex)}
              fill={PALETTE.blue}
            />
            <rect
              x={xd(t.d)}
              y={DAILY.axis}
              width={barW}
              height={ht(t.claude)}
              fill={PALETTE.upstream}
            />
          </g>
        ))}
        <text
          x={xd(lastScript.d) + 4}
          y={DAILY.axis - hs(lastScript.n) - 4}
          className={s.axisText}
        >
          last script {md(lastScript.d)}
        </text>
        <text
          x={right}
          y={DAILY.bottom + 12}
          textAnchor="end"
          className={s.axisText}
        >
          threads on a square-root scale; tallest {fmtInt(maxThreads)}
        </text>
        {months.map(([d, label]) => (
          <text
            key={d}
            x={xd(d)}
            y={H - 6}
            textAnchor="middle"
            className={s.axisText}
          >
            {label}
          </text>
        ))}
      </svg>
      <OrchWeeks
        W={W}
        idp={idp}
        xs={weeks.map((w) => xd(w.w))}
        xEnd={xd(end)}
      />
    </div>
  );
}
