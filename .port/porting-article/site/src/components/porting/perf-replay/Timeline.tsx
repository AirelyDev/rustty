"use client";

import { fmtPct } from "../lib/format";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { tipHandlers } from "./hooks";
import { type CommitKind, commitKind, hhmm, tm } from "./model";
import s from "./PerfReplay.module.css";
import { KIND } from "./StepPanel";
import type { PrData } from "./types";

/** T0 and T1 bound the shared time axis: 07-15 23:20 → 07-16 10:55. */
const T0 = tm("2026-07-15T23:20:00");
const T1 = tm("2026-07-16T10:55:00");

/** Maps a time (epoch ms) to x in the 960-wide timeline. */
function TX(t: number): number {
  return 92 + ((t - T0) / (T1 - T0)) * (960 - 104);
}

/** Maps a suite delta (%) to y in the top lane. */
function YS(d: number): number {
  return 14 + ((45 - d) / 70) * 58;
}

/** Maps a terminal-parser time (ms) to y in the LTO lane. */
function LY(v: number): number {
  return 120 + ((700 - v) / 200) * 26;
}

/** TICK_COLOUR colours commit ticks by kind (the tooltip names each commit). */
export const TICK_COLOUR: Record<CommitKind, string> = {
  perf: PALETTE.rust,
  revert: PALETTE.fail,
  merge: PALETTE.gray4,
  parity: PALETTE.pass,
  records: PALETTE.blue,
};

/** LANES are the timeline's lane labels and their y. */
const LANES: [string, number][] = [
  ["suite Δ vs Zig", 40],
  ["commits", 96],
  ["LTO detour", 130],
  ["measured worse", 170],
];

/** Returns each step's x, offset by 11 px per step sharing a commit time. */
export function stageXs(data: PrData): number[] {
  const seen = new Map<string, number>();
  return data.stages.map((st) => {
    const n = (seen.get(st.t) ?? 0) + 1;
    seen.set(st.t, n);
    return TX(tm(st.t)) + (n - 1) * 11;
  });
}

/** Props for the timeline. */
type TimelineProps = {
  data: PrData;
  cur: number;
  tip: ChartTip;
  onStep: (i: number) => void;
};

/**
 * The shared-time timeline: the suite Δ line through the 14 steps (progress in
 * copper up to the cursor), every commit of the window as a tick, the LTO
 * detour of terminal-parser across the round-1 stage binaries, and the
 * optimisations that measured worse. Arrow keys step through the marks;
 * Enter on a step node shows that step.
 */
export default function Timeline({ data, cur, tip, onStep }: TimelineProps) {
  const xs = stageXs(data);
  const byHash = new Map(data.commits.map((c) => [c.h, c]));
  const path = (n: number) =>
    data.stages
      .slice(0, n + 1)
      .map((st, i) => `${i ? "L" : "M"}${xs[i]},${YS(st.suiteD)}`)
      .join("");
  const chain = data.lto.chain
    .map((c, i) => ({ c, i }))
    .filter(({ c }) => "terminal-parser" in c.ms);
  const cx = (h: string) => TX(tm(byHash.get(h)?.t ?? data.stages[0].t));
  const lastC = chain[chain.length - 1].c;
  const lastX = cx(lastC.commit);
  const lastY = LY(lastC.ms["terminal-parser"]);
  const revert = data.stages.find((st) => st.key === "round1");
  const rx = revert ? TX(tm(revert.t)) : lastX;
  const ry = YS(revert?.suiteD ?? 0);
  const shipped = data.lto.profiles[1].ms["terminal-parser"];
  const handlers = tipHandlers(tip, {
    onEnter: (id) => {
      if (!id.startsWith("node:")) return false;
      onStep(Number(id.slice(5)));
      return true;
    },
  });
  const hours = Array.from({ length: 12 }, (_, h) => h);
  const curSt = data.stages[cur];

  return (
    <svg
      className={`${s.svg} ${s.wideSvg}`}
      viewBox="0 0 960 196"
      role="application"
      aria-roledescription="timeline"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the timeline handles arrows, Enter and Escape itself
      tabIndex={0}
      aria-label="Timeline of the 14 measured steps, the perf commits, the LTO detour and the optimisations that measured worse. Arrow keys move between marks; Enter on a step shows it."
      {...handlers}
    >
      {LANES.map(([t, y]) => (
        <text key={t} x={0} y={y + 3} fill={PALETTE.gray4} fontSize={9.5}>
          {t}
        </text>
      ))}
      <text x={0} y={142} fill={PALETTE.gray4} fontSize={8.5}>
        (t-parser ms)
      </text>
      {hours.map((h) => {
        const x = TX(tm("2026-07-16T00:00:00") + h * 3600e3);
        return (
          <g key={h}>
            <line x1={x} x2={x} y1={8} y2={182} stroke="#151816" />
            <text
              x={x}
              y={192}
              fill={PALETTE.gray4}
              fontSize={9}
              textAnchor="middle"
            >
              {String(h).padStart(2, "0")}:00
            </text>
          </g>
        );
      })}
      <text x={92} y={192} fill={PALETTE.gray4} fontSize={9}>
        07-15
      </text>
      <line
        x1={92}
        x2={956}
        y1={YS(0)}
        y2={YS(0)}
        stroke={PALETTE.upstream}
        strokeDasharray="3 3"
        opacity={0.6}
      />
      <text
        x={956}
        y={YS(0) - 3}
        fill={PALETTE.upstream}
        fontSize={8.5}
        textAnchor="end"
      >
        parity
      </text>
      <path
        d={path(data.stages.length - 1)}
        fill="none"
        stroke={PALETTE.gray3}
        strokeWidth={1.2}
      />
      <path d={path(cur)} fill="none" stroke={PALETTE.rust} strokeWidth={2} />
      {data.stages.map((st, i) => (
        <g
          key={st.key}
          data-tip={`node:${i}`}
          data-on={tip.shown === `node:${i}` || undefined}
          opacity={i <= cur ? 1 : 0.35}
        >
          <circle
            className={s.hl}
            cx={xs[i]}
            cy={YS(st.suiteD)}
            r={st.kind === "ship" ? 5 : 3.6}
            fill={KIND[st.kind][1]}
            stroke="#000"
            strokeWidth={1.5}
          />
        </g>
      ))}
      <line
        x1={xs[cur]}
        x2={xs[cur]}
        y1={8}
        y2={182}
        stroke={PALETTE.gold}
        opacity={0.8}
      />
      <text
        x={Math.min(930, Math.max(110, xs[cur]))}
        y={10}
        fill={PALETTE.gold}
        fontSize={10}
        textAnchor="middle"
      >
        {hhmm(curSt.t).slice(6)} · {fmtPct(curSt.suiteD)}
      </text>
      {data.commits.map((c, i) => {
        const x = TX(tm(c.t));
        return (
          <g key={c.h} data-tip={`commit:${i}`}>
            <rect x={x - 3} y={86} width={6} height={20} fill="transparent" />
            <line
              x1={x}
              x2={x}
              y1={88}
              y2={104}
              stroke={TICK_COLOUR[commitKind(c.s)]}
              strokeWidth={tip.shown === `commit:${i}` ? 3 : 1.6}
            />
          </g>
        );
      })}
      <path
        d={chain
          .map(
            ({ c }, i) =>
              `${i ? "L" : "M"}${cx(c.commit)},${LY(c.ms["terminal-parser"])}`,
          )
          .join("")}
        fill="none"
        stroke={PALETTE.fail}
        strokeWidth={1.4}
      />
      {chain.map(({ c, i }) => (
        <g
          key={c.order}
          data-tip={`chain:${i}`}
          data-on={tip.shown === `chain:${i}` || undefined}
        >
          <circle
            className={s.hl}
            cx={cx(c.commit)}
            cy={LY(c.ms["terminal-parser"])}
            r={2.6}
            fill={/reverted/.test(c.verdict) ? "#000" : PALETTE.fail}
            stroke={PALETTE.fail}
          />
        </g>
      ))}
      <path
        d={`M${lastX},${lastY} C${lastX + 60},${lastY} ${rx - 40},${ry} ${rx},${ry}`}
        fill="none"
        stroke={PALETTE.fail}
        strokeDasharray="3 3"
      />
      <text x={lastX + 8} y={lastY + 12} fill={PALETTE.fail} fontSize={9}>
        {Math.round(lastC.ms["terminal-parser"])} ms with LTO →{" "}
        {Math.round(shipped)} ms when reverted ({revert?.commit},{" "}
        {revert ? revert.t.slice(11, 16) : ""})
      </text>
      {data.negMarks.map((n, i) => {
        const x = TX(tm(n.t));
        const on = tip.shown === `neg:${i}`;
        return (
          <g key={n.exp} data-tip={`neg:${i}`}>
            <rect x={x - 5} y={162} width={10} height={16} fill="transparent" />
            <text
              x={x}
              y={174}
              fill={on ? PALETTE.gold : PALETTE.fail}
              fontSize={12}
              textAnchor="middle"
            >
              ×
            </text>
          </g>
        );
      })}
    </svg>
  );
}
