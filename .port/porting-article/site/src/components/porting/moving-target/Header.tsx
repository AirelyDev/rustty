import CommitLink from "../ui/CommitLink";
import { PALETTE } from "../lib/palette";
import type { MaxLag } from "./model";
import { int, type Totals } from "./stats";
import s from "./MovingTarget.module.css";

/**
 * The widget header: the upstream non-merge commit count as the hero figure,
 * then five tiles (lines inserted, Ghostty growth, median wait, the terminal
 * engine's share of the churn, the largest gap between the pin and main).
 */
export default function Header({
  t,
  lag,
  moves,
}: {
  t: Totals;
  lag: MaxLag;
  moves: number;
}) {
  const tiles: [string, string][] = [
    [
      `+${int(t.phaseIns)}`,
      `lines inserted upstream (−${int(t.phaseDel)}), excluding move ${t.driftN} (the 2-commit drift)`,
    ],
    [
      `+${t.growthPct.toFixed(1)}%`,
      `Ghostty .zig lines, ${int(t.loc0)} → ${int(t.loc1)}`,
    ],
    [
      `${t.medianLag} h`,
      `median wait, upstream commit → reference moved (first ${t.firstLag} h, as little as ${t.minLag} h, last ${t.lastLag} h)`,
    ],
    [
      `${Math.round(t.terminalPct)}%`,
      "of upstream line churn was in src/terminal",
    ],
    [
      `${(lag.lag / 1000).toFixed(1)}k`,
      `max lines the reference trailed Ghostty main (${lag.date.slice(5)})`,
    ],
  ];
  return (
    <>
      <div className={s.hero}>
        <span className={s.big}>{int(t.nonMerge)}</span>
        <span className={s.sub}>
          upstream non-merge commits absorbed in {moves} moves of the reference
          commit,{" "}
          <span className={s.mono}>
            <CommitLink hash={t.firstSha} /> → <CommitLink hash={t.lastSha} />
          </span>
        </span>
      </div>
      <div className={s.stats} style={{ color: PALETTE.gray7 }}>
        {tiles.map(([v, k]) => (
          <div key={k} className={s.stat}>
            <div className={s.statV}>{v}</div>
            <div className={s.statK}>{k}</div>
          </div>
        ))}
      </div>
    </>
  );
}
