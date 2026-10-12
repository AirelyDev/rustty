"use client";

import SourceLink from "../ui/SourceLink";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import s from "./CuttingTheCycle.module.css";
import { shortName, TIER_SHORT } from "./geometry";
import { cutsOfCrate, type View } from "./view";

/** Draws a thin horizontal bar of value against max. */
function Bar({
  value,
  max,
  color,
}: {
  value: number;
  max: number;
  color: string;
}) {
  return (
    <div className={s.bar}>
      <i
        style={{
          width: `${Math.max(1, (100 * value) / max).toFixed(1)}%`,
          background: color,
        }}
      />
    </div>
  );
}

/** KIND_SHORT is the short label of each Cargo dependency kind. */
const KIND_SHORT = {
  normal: "normal",
  dev: "dev-only",
  build: "build",
} as const;

/**
 * Lists the direct crate edges of one direction (what the crate depends on, or
 * what depends on it): partner crate, dependency kind, the depth pair and the
 * layering note when there is one, so the edge tooltips' content is readable
 * without a pointer.
 */
function EdgeList({ v, index, out }: { v: View; index: number; out: boolean }) {
  const c = v.lay.crates;
  const edges = v.crateEdges
    .filter((e) => (out ? e.s : e.t) === index)
    .sort((a, b) =>
      out ? c[b.t].depth - c[a.t].depth : c[a.s].depth - c[b.s].depth,
    );
  if (!edges.length) return <p className={s.note}>none</p>;
  return (
    <ul className={s.edgeList}>
      {edges.map((e) => {
        const note = v.layerEdge.get(e.key);
        return (
          <li key={e.key + e.k}>
            <span className={s.leadRow}>
              <span>{shortName(c[out ? e.t : e.s].id)}</span>
              <span>
                {KIND_SHORT[e.k]} · depth {c[e.s].depth} → {c[e.t].depth}
              </span>
            </span>
            {note ? <span className={s.edgeNote}>{note}</span> : null}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * The detail panel of a selected crate: tier, depth, birth, Zig → Rust module
 * size and tests (or the vendored C for a -sys crate), its own metrics and
 * fan-in/out, its direct edges (kind, depths, layering notes), the Zig directories it came from, the cuts that shaped it and
 * its layering decisions.
 */
export default function CrateDetail({
  v,
  index,
  onClear,
}: {
  v: View;
  index: number;
  onClear: () => void;
}) {
  const n = v.lay.crates[index];
  const lay = v.data.text.layer.filter((l) => l.c.includes(n.id));
  const cuts = cutsOfCrate(v, n.id);
  const max = Math.max(n.zcode ?? 0, n.mrcode ?? 0) || 1;
  return (
    <div className={s.detail}>
      <div className={s.detailName}>{n.id}</div>
      <p className={s.note}>
        {TIER_SHORT[n.tier]} · depth {n.depth} · born {n.birth ?? "–"}
        {n.bcommit ? <span className={s.mono}> {n.bcommit}</span> : null}
        {n.k1000 ? ` · 1k lines ${n.k1000}` : ""}
      </p>
      {n.sys ? (
        <>
          <h4 className={s.h4}>C library wrapper</h4>
          <div className={s.kv}>
            <span>wraps</span>
            <b>{n.pkg ?? "–"}</b>
            <span>vendored C/C++ lines</span>
            <b>{n.vend === null ? "–" : fmtInt(n.vend)}</b>
            <span>Rust code lines</span>
            <b>{fmtInt(n.loc)}</b>
          </div>
        </>
      ) : n.mod ? (
        <>
          <h4 className={s.h4}>
            Zig → Rust ({n.mod}
            {n.modc?.includes("+")
              ? `, shared by ${n.modc.replace(/rustty-/g, "").replace(/\+/g, " + ")}`
              : ""}
            )
          </h4>
          {n.zcode ? (
            <>
              <div className={s.kv}>
                <span style={{ color: PALETTE.upstream }}>Zig code lines</span>
                <b>{fmtInt(n.zcode)}</b>
              </div>
              <Bar value={n.zcode} max={max} color={PALETTE.upstream} />
            </>
          ) : (
            <p className={s.note}>
              No Zig counterpart (Rust-only infrastructure).
            </p>
          )}
          <div className={s.kv}>
            <span style={{ color: PALETTE.rust }}>Rust code lines</span>
            <b>
              {fmtInt(n.mrcode ?? 0)}
              {n.zcode ? ` (${((n.mrcode ?? 0) / n.zcode).toFixed(2)}×)` : ""}
            </b>
          </div>
          <Bar value={n.mrcode ?? 0} max={max} color={PALETTE.rust} />
          <div className={s.kv}>
            <span>tests Zig → Rust</span>
            <b>
              {n.ztests === null ? "–" : fmtInt(n.ztests)} →{" "}
              {fmtInt(n.mrtests ?? 0)}
            </b>
          </div>
        </>
      ) : null}
      <h4 className={s.h4}>This crate</h4>
      <div className={s.kv}>
        <span>code lines</span>
        <b>{fmtInt(n.loc)}</b>
        <span>#[test] functions</span>
        <b>{fmtInt(n.tests)}</b>
        <span>unsafe sites</span>
        <b>{n.uns === null ? "–" : fmtInt(n.uns)}</b>
        <span className={s.indent}>per 1k code lines</span>
        <b>{n.unsk ?? "–"}</b>
        <span>depends on (direct · all)</span>
        <b>
          {n.fanout} · {n.tout}
        </b>
        <span>used by (direct · all)</span>
        <b>
          {n.fanin} · {n.tin}
        </b>
      </div>
      <h4 className={s.h4}>Depends on (direct edges)</h4>
      <EdgeList v={v} index={index} out />
      <h4 className={s.h4}>Used by (direct edges)</h4>
      <EdgeList v={v} index={index} out={false} />
      {n.zf ? (
        <>
          <h4 className={s.h4}>
            Zig files at the first reference ({n.zf}, {n.zscc} in the cycle)
          </h4>
          <div className={s.lead}>
            {n.zdirs.map(([d, k]) => (
              <span key={d} className={s.leadRow}>
                <SourceLink
                  at="4749c4e93"
                  path={`src/${d}`}
                  tree={d.endsWith("/")}
                  text={d}
                />
                <span>{k}</span>
              </span>
            ))}
          </div>
        </>
      ) : null}
      {cuts.length ? (
        <>
          <h4 className={s.h4}>Cuts that shaped it</h4>
          <ul className={s.list}>
            {cuts.map((k) => (
              <li key={k}>
                <span style={{ color: k > 14 ? PALETTE.gold : PALETTE.fail }}>
                  {k > 14 ? "extra" : `cut ${k}`}
                </span>
                : {v.cutBy.get(k)?.name}
              </li>
            ))}
          </ul>
        </>
      ) : null}
      {lay.length ? (
        <>
          <h4 className={s.h4}>Layering decisions</h4>
          <ul className={s.list}>
            {lay.map((l) => (
              <li key={l.t}>{l.t}</li>
            ))}
          </ul>
        </>
      ) : null}
      <button type="button" className={s.btn} onClick={onClear}>
        Clear selection
      </button>
    </div>
  );
}
