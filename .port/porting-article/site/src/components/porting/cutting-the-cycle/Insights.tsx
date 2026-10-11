"use client";

import type { ReactNode } from "react";
import { fmtInt } from "../lib/format";
import SourceLink from "../ui/SourceLink";
import s from "./CuttingTheCycle.module.css";
import { shortName } from "./geometry";
import { mechanismCounts } from "./model";
import { GLOBAL_TAG, type View } from "./view";

/** Returns the six insight tiles (headline, text), every number taken from the data. */
export function insightTiles(v: View): [string, string][] {
  const z = v.data.zig;
  const st = z.stats;
  const down = v.tagEdges.get(GLOBAL_TAG)?.length ?? 0;
  const planned = v.data.text.cuts.map((c) => c.n);
  const left = planned.map((n) => z.skip_new[String(n)]?.[0] ?? []);
  const bearing = left.filter((c) => c.length > 0).length;
  const smallest = Math.min(...left.map((c) => c.length));
  const largest = Math.max(...left.map((c) => c.length));
  const smallPairs = left
    .filter((c) => c.length === smallest)
    .map((c) => c.join(" ↔ "))
    .join(", ");
  const worst = planned[left.findIndex((c) => c.length === largest)];
  const m = mechanismCounts(v.data);
  const chain = v.data.crates.chain;
  const crates = v.lay.crates;
  const util = crates[v.lay.crateIndex.get("rustty-util") ?? 0];
  const vt = crates[v.lay.crateIndex.get("rustty-vt") ?? 0];
  const rust = crates.filter((n) => !n.sys).reduce((a, n) => a + n.loc, 0);
  const knot = st.cum[planned.length];
  return [
    [
      `${st.inverted} of ${fmtInt(st.scc_edges)}`,
      `import edges inside the cycle point upward against today's crate graph. Those ${st.inverted} are all the port had to cut. Another ${down} import global.zig, which is harmless once it is a leaf.`,
    ],
    [
      `${bearing} of ${planned.length} load-bearing`,
      `Skip any one of the ${planned.length} cuts and Cargo sees a cycle. The smallest leftover is ${smallest} crates (${smallPairs}); skipping cut ${worst} leaves an ${largest}-crate cycle.`,
    ],
    [
      "1 unplanned seam",
      `After the ${planned.length} cuts, a ${knot}-file knot of renderer, inspector, termio, core and crash remained, held together by one edge: crash/sentry.zig → Surface.zig. It became a trait.`,
    ],
    [
      `${m.trait} · ${m.move} · ${m.split} · ${m.inject}`,
      `of the ${planned.length} cuts were traits, moves, splits and one injection. Every cut is a structural change only: the porting rules forbid changing logic, comments or tests.`,
    ],
    [`${chain.length - 1} hops`, chain.map(shortName).join(" → ")],
    [
      `${util.tin} of ${crates.length - 1}`,
      `other packages depend on rustty-util, all ${fmtInt(util.loc)} lines of it. The terminal engine, ${Math.round((100 * vt.loc) / rust)}% of all Rust code, has ${vt.fanin} direct dependents and sits at depth ${vt.depth}.`,
    ],
  ];
}

/** FILE_LINKS maps the Zig file names the tiles mention to their path and line at the first reference commit. */
const FILE_LINKS: Record<string, { path: string; lines?: string }> = {
  "global.zig": { path: "src/global.zig" },
  "crash/sentry.zig": { path: "src/crash/sentry.zig", lines: "11" },
  "Surface.zig": { path: "src/Surface.zig" },
};

/** FILE_RE splits tile text around the file names in FILE_LINKS. */
const FILE_RE = /(crash\/sentry\.zig|Surface\.zig|global\.zig)/;

/** Renders tile text with each known Zig file name linked at the first reference commit. */
function withFileLinks(t: string): ReactNode {
  let at = 0;
  return t.split(FILE_RE).map((part) => {
    const key = `f${at}`;
    at += part.length;
    const f = FILE_LINKS[part];
    return f ? (
      <SourceLink
        key={key}
        at="4749c4e93"
        path={f.path}
        lines={f.lines}
        text={part}
      />
    ) : (
      <span key={key}>{part}</span>
    );
  });
}

/** The insights strip under the graph: six computed tiles. */
export default function Insights({ v }: { v: View }) {
  return (
    <div className={s.ins}>
      {insightTiles(v).map(([b, t]) => (
        <div key={b}>
          <b>{b}</b>
          {withFileLinks(t)}
        </div>
      ))}
    </div>
  );
}
