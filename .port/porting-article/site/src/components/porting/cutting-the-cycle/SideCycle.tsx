"use client";

import CommitLink from "../ui/CommitLink";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import s from "./CuttingTheCycle.module.css";
import type { View } from "./view";

/** Draws the histogram of shortest-cycle lengths (hops per file). */
function Histogram({ dist }: { dist: Record<string, number> }) {
  const keys = Object.keys(dist);
  const max = Math.max(...keys.map((k) => dist[k]));
  const w = 280;
  const h = 86;
  const bw = (w - 10) / keys.length;
  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      className={s.miniSvg}
      role="img"
      aria-label={`Files by shortest cycle length: ${keys.map((k) => `${dist[k]} files in ${k} hops`).join(", ")}`}
    >
      {keys.map((k, i) => {
        const bh = ((h - 26) * dist[k]) / max;
        const x = 4 + i * bw;
        return (
          <g key={k}>
            <rect
              x={x + 2}
              y={h - 16 - bh}
              width={bw - 4}
              height={bh}
              fill={PALETTE.upstream}
              fillOpacity={0.75}
            >
              <title>{`${dist[k]} files: shortest cycle ${k} hops`}</title>
            </rect>
            <text
              x={x + bw / 2}
              y={h - 4}
              fontSize={9}
              fill={PALETTE.gray4}
              textAnchor="middle"
            >
              {k}
            </text>
            <text
              x={x + bw / 2}
              y={h - 19 - bh}
              fontSize={9}
              fill={PALETTE.gray6}
              textAnchor="middle"
            >
              {dist[k]}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

/**
 * Stage 1 side panel: size of the cycle, the shortest-cycle histogram, the
 * most-imported files and four real cycles to trace.
 */
export default function SideCycle({
  v,
  example,
  onExample,
}: {
  v: View;
  example: number | null;
  onExample: (i: number | null) => void;
}) {
  const st = v.data.zig.stats;
  const dist = st.cycle_len_dist;
  const hops = Object.keys(dist).map(Number);
  const minHop = Math.min(...hops);
  return (
    <>
      <h4 className={s.h4}>The cycle at the first reference commit</h4>
      <div className={s.kv}>
        <span>.zig files in src/</span>
        <b>{fmtInt(st.files_total)}</b>
        <span>files in the one big cycle</span>
        <b className={s.copper}>
          {st.scc} ({Math.round((100 * st.scc) / st.files_total)}%)
        </b>
        <span>import edges inside it</span>
        <b>{fmtInt(st.scc_edges)}</b>
        <span>directories it spans</span>
        <b>{v.data.zig.dirs.length}</b>
        <span>other cycles (files)</span>
        <b>{st.other_sccs.join(", ")}</b>
        <span>
          at today's reference <CommitLink hash={st.now.commit} />
        </span>
        <b>
          {st.now.scc} files · {fmtInt(st.now.scc_edges)} edges
        </b>
      </div>
      <h4 className={s.h4}>Shortest way back: hops per file</h4>
      <Histogram dist={dist} />
      <p className={s.note}>
        Every one of the {st.scc} files can reach itself again. {dist[minHop]}{" "}
        do so in {minHop} hops; the longest shortest loop is {Math.max(...hops)}{" "}
        hops.
      </p>
      <h4 className={s.h4}>Most-imported files inside the cycle</h4>
      <div className={s.lead}>
        {v.hubs.map((h) => (
          <span key={h.name} className={s.leadRow}>
            <span>{h.name}</span>
            <span>{h.in}</span>
          </span>
        ))}
      </div>
      <h4 className={s.h4}>Trace a real cycle</h4>
      <div>
        {v.data.zig.examples.map((p, i) => (
          <button
            key={p[0]}
            type="button"
            className={s.btn}
            aria-pressed={example === i}
            onClick={() => onExample(example === i ? null : i)}
          >
            {p[0].replace(/\.zig$/, "")} · {p.length - 1}
          </button>
        ))}
      </div>
      <p className={s.note}>
        Hover or tap any dot to trace its shortest cycle (white). Copper = what
        it imports, teal = what imports it. With the keyboard, focus the graph
        and use the arrow keys.
      </p>
    </>
  );
}
