"use client";

import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import s from "./CuttingTheCycle.module.css";
import { fullName, shortName, TIER_SHORT } from "./geometry";
import { cutsTouching, type ReplayFrame, shortestCycle } from "./model";
import { MECH_LABEL, type View } from "./view";

/** EDGE_KIND describes each Cargo dependency kind in the edge tooltip. */
const EDGE_KIND = {
  normal: "normal dependency",
  dev: "dev-dependency (tests only; Cargo allows these to point anywhere)",
  build: "build dependency",
} as const;

/** Tooltip body for file i: size, degrees, target crate, its shortest cycle and the cuts it touches. */
function FileTip({ v, i }: { v: View; i: number }) {
  const f = v.data.zig.files[i];
  const crate = v.lay.crates[v.lay.crateIndex.get(fullName(f[7])) ?? 0];
  const cyc = shortestCycle(v.g, i);
  const cuts = cutsTouching(v.data, v.g, i);
  return (
    <>
      <b>{f[0]}</b>
      <br />
      <span className={s.k}>
        {fmtInt(f[6])} lines · imports {f[5]} · imported by {f[4]} (inside the
        cycle)
      </span>
      <br />
      Became <b>{fullName(f[7])}</b>{" "}
      <span className={s.k}>({TIER_SHORT[crate.tier]})</span>
      <br />
      Shortest cycle back to itself: <b>{cyc.length - 1}</b> hops
      <br />
      <span className={s.k}>
        {cyc.map((p) => v.g.names[p].replace(/\.zig$/, "")).join(" → ")}
      </span>
      {cuts.length ? (
        <>
          <br />
          Touches cut{cuts.length > 1 ? "s" : ""} <b>{cuts.join(", ")}</b>
        </>
      ) : null}
    </>
  );
}

/** Tooltip body for cut n: mechanism, explanation, the crate cycle left by skipping it, and up to four real edges. */
function CutTip({ v, n }: { v: View; n: number }) {
  const c = v.cutBy.get(n);
  if (!c) return null;
  const skip = v.data.zig.skip_new[String(n)]?.[0];
  const edges = v.tagEdges.get(n) ?? [];
  const z = v.data.zig;
  return (
    <>
      <b>
        {c.n} · {c.name}
      </b>{" "}
      <span className={`${s.chip} ${s[`m_${c.m}`]}`}>{MECH_LABEL[c.m]}</span>
      <br />
      {c.d}
      {skip?.length ? (
        <>
          <br />
          <span className={s.k}>
            Skip this cut and Cargo sees a cycle among {skip.length} crates:
          </span>{" "}
          {skip.join(", ")}
        </>
      ) : null}
      <br />
      <span className={s.k}>
        {edges.slice(0, 4).map((e) => (
          <span key={e} className={s.tipLine}>
            {z.files[z.edges[e][0]][0]} → {z.files[z.edges[e][1]][0]}
          </span>
        ))}
        {edges.length > 4 ? (
          <span className={s.tipLine}>… {edges.length - 4} more</span>
        ) : null}
      </span>
    </>
  );
}

/** Tooltip body for crate i: depth, tier, size, tests, fan-in/out (and lines on the replay date). */
function CrateTip({
  v,
  i,
  frame,
}: {
  v: View;
  i: number;
  frame: ReplayFrame | null;
}) {
  const n = v.lay.crates[i];
  const lines = frame?.lines[n.id];
  return (
    <>
      <b>{n.id}</b>{" "}
      <span className={s.k}>
        · depth {n.depth} · {TIER_SHORT[n.tier]}
      </span>
      <br />
      {fmtInt(n.loc)} code lines · {fmtInt(n.tests)} tests
      {frame && lines != null ? (
        <>
          <br />
          <span className={s.k}>
            on {frame.date}: {fmtInt(lines)} physical lines
          </span>
        </>
      ) : null}
      <br />
      <span style={{ color: PALETTE.rust }}>depends on {n.fanout}</span> ·{" "}
      <span style={{ color: PALETTE.blue }}>used by {n.fanin}</span>{" "}
      <span className={s.k}>
        (transitively {n.tin}; dev-only edges excluded)
      </span>
      <br />
      <span className={s.k}>Click or press Enter for details</span>
    </>
  );
}

/** Tooltip body for crate edge i: kind, depths and the layering note if there is one. */
function EdgeTip({ v, i }: { v: View; i: number }) {
  const e = v.crateEdges[i];
  const a = v.lay.crates[e.s];
  const b = v.lay.crates[e.t];
  const note = v.layerEdge.get(e.key);
  return (
    <>
      <b>{shortName(a.id)}</b> depends on <b>{shortName(b.id)}</b>
      <br />
      <span className={s.k}>
        {EDGE_KIND[e.k]} · depth {a.depth} → {b.depth}
      </span>
      {note ? (
        <>
          <br />
          {note}
        </>
      ) : null}
    </>
  );
}

/**
 * Returns the tooltip body for a tooltip id: f<i> file, k<n> cut, n<i> crate,
 * e<i> crate edge.
 */
export default function TipContent({
  v,
  id,
  frame,
}: {
  v: View;
  id: string;
  frame: ReplayFrame | null;
}) {
  const k = Number(id.slice(1));
  switch (id[0]) {
    case "f":
      return <FileTip v={v} i={k} />;
    case "k":
      return <CutTip v={v} n={k} />;
    case "n":
      return <CrateTip v={v} i={k} frame={frame} />;
    case "e":
      return <EdgeTip v={v} i={k} />;
    default:
      return null;
  }
}
