"use client";

import { fmtInt } from "../lib/format";
import CrateDetail from "./CrateDetail";
import s from "./CuttingTheCycle.module.css";
import { shortName } from "./geometry";
import type { View } from "./view";

/**
 * Stage 3 side panel: graph metrics, the longest-chain and layering toggles,
 * then either the most depended-on and most dependent crates or the detail
 * panel of the selected crate.
 */
export default function SideCrates({
  v,
  selected,
  showChain,
  showLayer,
  onToggle,
  onClear,
}: {
  v: View;
  selected: number | null;
  showChain: boolean;
  showLayer: boolean;
  onToggle: (t: "chain" | "layer") => void;
  onClear: () => void;
}) {
  const c = v.lay.crates;
  const kinds = { normal: 0, dev: 0, build: 0 };
  for (const e of v.crateEdges) kinds[e.k]++;
  const sys = c.filter((n) => n.sys);
  const crates = c.filter((n) => !n.sys && n.id !== "xtask").length;
  const io = v.data.crates.cratesIo;
  const byIn = [...c]
    .sort((a, b) => b.fanin - a.fanin || b.tin - a.tin)
    .slice(0, 5);
  const byOut = [...c].sort((a, b) => b.fanout - a.fanout).slice(0, 5);
  return (
    <>
      <h4 className={s.h4}>Graph metrics</h4>
      <div className={s.kv}>
        <span>packages</span>
        <b>{c.length}</b>
        <span className={s.indent}>
          {crates} crates + xtask + {sys.length} -sys
        </span>
        <b />
        <span>internal edges</span>
        <b>{v.crateEdges.length}</b>
        <span className={s.indent}>normal / dev / build</span>
        <b>
          {kinds.normal} / {kinds.dev} / {kinds.build}
        </b>
        <span>depth (longest chain)</span>
        <b className={s.copper}>{v.lay.maxDepth}</b>
        <span>crates.io packages resolved</span>
        <b>{io.resolved}</b>
        <span className={s.indent}>direct normal/build deps</span>
        <b>{io.direct}</b>
        <span>vendored C/C++ in -sys (incl. Obj-C++)</span>
        <b>{fmtInt(sys.reduce((a, n) => a + (n.vend ?? 0), 0))}</b>
        <span className={s.indent}>Rust glue in -sys</span>
        <b>{fmtInt(sys.reduce((a, n) => a + n.loc, 0))}</b>
      </div>
      <div className={s.toggles}>
        <button
          type="button"
          className={s.btn}
          aria-pressed={showChain}
          onClick={() => onToggle("chain")}
        >
          Longest chain
        </button>
        <button
          type="button"
          className={s.btn}
          aria-pressed={showLayer}
          onClick={() => onToggle("layer")}
        >
          Layering decisions
        </button>
      </div>
      {selected === null ? (
        <>
          <h4 className={s.h4}>Most depended on (direct · transitive)</h4>
          <div className={s.lead}>
            {byIn.map((n) => (
              <span key={n.id} className={s.leadRow}>
                <span>{shortName(n.id)}</span>
                <span>
                  {n.fanin} · {n.tin}
                </span>
              </span>
            ))}
          </div>
          <h4 className={s.h4}>Most dependencies (direct)</h4>
          <div className={s.lead}>
            {byOut.map((n) => (
              <span key={n.id} className={s.leadRow}>
                <span>{shortName(n.id)}</span>
                <span>{n.fanout}</span>
              </span>
            ))}
          </div>
          <p className={s.note}>
            Click a crate (or focus the graph and press Enter) for its Zig
            origin, size, tests, unsafe density, direct edges and layering
            notes. Copper edges = what it depends on, blue = what depends on it,
            dashed = dev-only.
          </p>
        </>
      ) : (
        <CrateDetail v={v} index={selected} onClear={onClear} />
      )}
    </>
  );
}
