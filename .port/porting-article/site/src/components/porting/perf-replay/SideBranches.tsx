"use client";

import CommitText from "../ui/CommitText";
import SourceLink from "../ui/SourceLink";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import type { ChartTip } from "../ui/useChartTip";
import { tipHandlers } from "./hooks";
import s from "./PerfReplay.module.css";
import type { PrData } from "./types";

/** LTO_BENCHES are the four benchmarks of the build-profile experiment. */
const LTO_BENCHES = [
  "terminal-parser",
  "terminal-stream",
  "osc-parser",
  "screen-clone.clone",
];

/** PROFILE_COLOURS colour the five profiles (legend text names each one). */
const PROFILE_COLOURS = [
  PALETTE.gray5,
  PALETTE.rust,
  PALETTE.fail,
  PALETTE.orange,
  PALETTE.purple,
];

/** Short axis label of a benchmark ("sc.clone", "t-parser"). */
function short(b: string): string {
  return b.replace("screen-clone.", "sc.").replace("terminal-", "t-");
}

/** Percent change of profile p over the shipped profile for benchmark b. */
function pctVsShipped(data: PrData, p: number, b: string): number {
  const ms = data.lto.profiles;
  return (ms[p].ms[b] / ms[1].ms[b] - 1) * 100;
}

/**
 * Side branch 1: grouped bars of four benchmarks under five build profiles on
 * identical code, with the note on why LTO stays off.
 */
export function LtoPanel({ data, tip }: { data: PrData; tip: ChartTip }) {
  const P = data.lto.profiles;
  const Y = (v: number) => 210 - (v / 560) * 150;
  const handlers = tipHandlers(tip);
  const tp = (i: number) => P[i].ms["terminal-parser"];
  const lto = (b: string) => Math.abs(pctVsShipped(data, 2, b));
  return (
    <div className={s.panel}>
      <div className={s.sec}>Side branch 1 · the LTO detour</div>
      <svg
        className={s.svg}
        viewBox="0 0 460 230"
        role="application"
        aria-roledescription="bar chart"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
        tabIndex={0}
        aria-label="Build profile experiment: four benchmarks under five build profiles on the same code. Arrow keys move between bars."
        {...handlers}
      >
        {P.map((p, i) => {
          const x = 8 + (i % 2) * 225;
          const y = 10 + Math.floor(i / 2) * 13;
          return (
            <g key={p.name}>
              <rect
                x={x}
                y={y - 7}
                width={8}
                height={8}
                fill={PROFILE_COLOURS[i]}
                rx={1}
              />
              <text x={x + 12} y={y} fill={PALETTE.gray5} fontSize={9}>
                {p.label}
              </text>
            </g>
          );
        })}
        {LTO_BENCHES.map((b, j) => {
          const x0 = 10 + j * 110;
          return (
            <g key={b}>
              <text
                x={x0 + 49}
                y={224}
                fill={PALETTE.gray6}
                fontSize={9}
                textAnchor="middle"
              >
                {short(b)}
              </text>
              {P.map((p, i) => {
                const v = p.ms[b];
                const id = `lto:${b}:${i}`;
                return (
                  <g
                    key={p.name}
                    data-tip={id}
                    data-on={tip.shown === id || undefined}
                  >
                    <rect
                      className={s.hl}
                      x={x0 + i * 18}
                      y={Y(v)}
                      width={15}
                      height={210 - Y(v)}
                      fill={PROFILE_COLOURS[i]}
                      rx={1.5}
                    />
                    {b === "terminal-parser" && (i === 1 || i === 2) ? (
                      <text
                        x={x0 + i * 18 + 7.5}
                        y={Y(v) - 3}
                        fill={PALETTE.gray7}
                        fontSize={8.5}
                        textAnchor="middle"
                      >
                        {Math.round(v)}
                      </text>
                    ) : null}
                  </g>
                );
              })}
            </g>
          );
        })}
        <line x1={6} x2={450} y1={210} y2={210} stroke="#262927" />
      </svg>
      <p className={s.pnote}>
        Same code (
        <CommitLink hash={data.lto.chain[data.lto.chain.length - 1].commit} />
        ), five build profiles. Fat LTO + one codegen unit + panic=abort helped
        osc-parser and screen-clone by {Math.round(lto("osc-parser"))}% but made
        terminal-parser {Math.round(pctVsShipped(data, 2, "terminal-parser"))}%
        slower ({Math.round(tp(1))} → {Math.round(tp(2))} ms). Thin LTO and
        demoting <code>do_action</code> did not recover it. Rustty ships
        overflow-checks-off only, and{" "}
        <SourceLink at="58aa62a81" path="Cargo.toml" lines="502-517" /> records
        why LTO stays off. With LTO off, a missing cross-crate{" "}
        <code>#[inline]</code> later became a recurring cause of perf gaps.
      </p>
    </div>
  );
}

/** Side branch 2: the eleven optimisations that measured worse, with the effect and why. */
export function Negatives({ data }: { data: PrData }) {
  return (
    <div className={s.panel}>
      <div className={s.sec}>
        Side branch 2 · falsified optimisations ({data.negatives.length})
      </div>
      <ul className={s.neg}>
        {data.negatives.map((n) => (
          <li key={n.exp}>
            <span>
              <b>{n.exp}</b> · {n.bench} · {n.date.slice(5)}
            </span>
            <span className={s.ef}>
              <CommitText text={n.effect} />
            </span>
            <span className={s.why}>
              <CommitText text={n.why} />
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
