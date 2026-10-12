"use client";

import s from "./CuttingTheCycle.module.css";
import { shortName } from "./geometry";
import { dayNumber, seriesValue } from "./model";
import type { View } from "./view";

/** LATE_AFTER is the date after which a crate counts as a late arrival. */
const LATE_AFTER = "2026-07-10";

/**
 * Stage 4 side panel: how to read the replay, the leaves-first test (median
 * date crates passed 1,000 lines by depth band, rank correlation) and the
 * crates born late.
 */
export default function SideReplay({ v }: { v: View }) {
  const cr = v.data.crates;
  const d0 = v.data.replay.d0;
  const vt = v.lay.crates[v.lay.crateIndex.get("rustty-vt") ?? 0];
  const vtDay5 = seriesValue(v.data, "rustty-vt", 4);
  const late = v.lay.crates
    .filter((n) => n.birth && n.birth > LATE_AFTER)
    .sort((a, b) => ((a.birth ?? "") < (b.birth ?? "") ? -1 : 1));
  /** Returns the MM-DD birth date of crate id. */
  const born = (id: string) =>
    v.lay.crates[v.lay.crateIndex.get(id) ?? 0].birth?.slice(5) ?? "–";
  return (
    <>
      <h4 className={s.h4}>How to read it</h4>
      <p className={s.note}>
        Crates appear on the day of their first commit (most were scaffolded on
        day 1, empty). Circle area grows with physical lines on that day. The
        strip above the graph shows how much of each depth layer's final code
        exists so far.
      </p>
      <h4 className={s.h4}>Leaves first?</h4>
      <p className={s.note}>
        Only loosely. Median date a crate passed 1,000 lines, by depth:
      </p>
      <div className={s.lead}>
        {Object.entries(cr.bands).map(([k, b]) => (
          <span key={k} className={s.leadRow}>
            <span>
              depth {k} ({b.n} crates)
            </span>
            <span>{b.median.slice(5)}</span>
          </span>
        ))}
      </div>
      <p className={s.note}>
        The terminal engine (depth {vt.depth}) went first: past 1,000 lines on
        day {vt.k1000 ? dayNumber(d0, vt.k1000) : "–"} and{" "}
        {Math.floor(vtDay5 / 1000)}k by day 5. The top layers (15-18) came last.
        Rank correlation between depth and the 1,000-line date:{" "}
        <b className={s.mono}>{cr.rho}</b> over {cr.rho_n} crates.
      </p>
      <h4 className={s.h4}>Late arrivals</h4>
      <div className={s.lead}>
        {late.map((n) => (
          <span key={n.id} className={s.leadRow}>
            <span>{shortName(n.id)}</span>
            <span>{n.birth?.slice(5)}</span>
          </span>
        ))}
      </div>
      <p className={s.note}>
        No crate was ever deleted. The graph only gained leaves and wrappers (a
        version crate split out on {born("rustty-version")}, a crash-reporter
        wrapper during the Codex remediation on {born("breakpad-sys")}).
      </p>
    </>
  );
}
