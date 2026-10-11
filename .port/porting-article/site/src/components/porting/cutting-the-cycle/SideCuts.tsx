"use client";

import SourceLink from "../ui/SourceLink";
import type { FocusEvent, MouseEvent } from "react";
import { PALETTE } from "../lib/palette";
import s from "./CuttingTheCycle.module.css";
import { fullName } from "./geometry";
import { type Cut, mechanismCounts } from "./model";
import { GLOBAL_TAG, MECH_LABEL, type View } from "./view";

/** Draws the shatter curve: the largest cross-crate cycle after cuts 1..k, plus the crash seam. */
function Shatter({ curve }: { curve: number[] }) {
  const w = 280;
  const h = 118;
  const max = Math.max(...curve, 300);
  const last = curve.length - 1;
  /** Returns the x of curve step i. */
  const X = (i: number) => 26 + (i * (w - 36)) / last;
  /** Returns the y of a file count. */
  const Y = (n: number) => 10 + (h - 34) * (1 - n / max);
  const plateau = curve.indexOf(curve[last - 2]);
  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      className={s.miniSvg}
      role="img"
      aria-label={`Largest cycle across future crates after cuts 1 to k: ${curve.slice(0, last).join(", ")}; ${curve[last]} with the crash seam.`}
    >
      {[0, 100, 200, 300].map((t) => (
        <g key={t}>
          <line x1={24} x2={w - 6} y1={Y(t)} y2={Y(t)} stroke={PALETTE.gray2} />
          <text
            x={20}
            y={Y(t) + 3}
            fontSize={8}
            fill={PALETTE.gray3}
            textAnchor="end"
          >
            {t}
          </text>
        </g>
      ))}
      <path
        d={curve.map((n, i) => `${i ? "L" : "M"}${X(i)} ${Y(n)}`).join("")}
        fill="none"
        stroke={PALETTE.fail}
        strokeWidth={1.6}
      />
      {curve.map((n, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: one point per step
        <g key={i}>
          <circle
            cx={X(i)}
            cy={Y(n)}
            r={2.6}
            fill={i === last ? PALETTE.gold : PALETTE.fail}
          >
            <title>
              {`${i === 0 ? "no cuts (entry points as sources)" : i === last ? "+ the crash seam" : `cuts 1-${i}`}: ${n} files`}
            </title>
          </circle>
          {i % 3 === 0 || i >= last - 1 ? (
            <text
              x={X(i)}
              y={h - 12}
              fontSize={8}
              fill={PALETTE.gray4}
              textAnchor="middle"
            >
              {i === last ? `+${last}` : i}
            </text>
          ) : null}
        </g>
      ))}
      <text x={X(0) + 4} y={Y(curve[0]) - 4} fontSize={9} fill={PALETTE.gray6}>
        {curve[0]}
      </text>
      <text
        x={X(plateau) + 2}
        y={Y(curve[plateau]) - 5}
        fontSize={9}
        fill={PALETTE.gray6}
      >
        {`${curve[plateau]} after ${plateau}-${last - 2}`}
      </text>
      <text
        x={X(last - 1) - 4}
        y={Y(curve[last - 1]) - 6}
        fontSize={9}
        fill={PALETTE.gray6}
        textAnchor="end"
      >
        {curve[last - 1]}
      </text>
      <text
        x={w / 2}
        y={h - 1}
        fontSize={8}
        fill={PALETTE.gray3}
        textAnchor="middle"
      >
        cuts applied, in number order
      </text>
    </svg>
  );
}

/** Row handlers for the cut list (provided by Body, which owns the tooltip and the pinned row). */
export type CutRowHandlers = {
  onEnter: (n: number, e: MouseEvent<HTMLButtonElement>) => void;
  onLeave: () => void;
  onFocus: (n: number, e: FocusEvent<HTMLButtonElement>) => void;
  onBlur: () => void;
  onClick: (n: number, e: MouseEvent<HTMLButtonElement>) => void;
};

/** Returns the edge-count line under a cut row's name. */
function edgeCount(v: View, c: Cut): string {
  const n = v.tagEdges.get(c.n)?.length ?? 0;
  if (c.n === 1)
    return `${n} up + ${v.tagEdges.get(GLOBAL_TAG)?.length ?? 0} down`;
  return `${n} edge${n > 1 ? "s" : ""}`;
}

/**
 * Stage 2 side panel: the shatter curve, then the 14 planned cuts and the 5
 * extra seams as rows (hover or focus to see the cut, click or Enter to pin).
 */
export default function SideCuts({
  v,
  curve,
  locked,
  h,
}: {
  v: View;
  curve: number[];
  locked: number | null;
  h: CutRowHandlers;
}) {
  const m = mechanismCounts(v.data);
  const ab = v.data.zig.stats.ablation;
  const best = Object.keys(ab).reduce((a, k) => (ab[k] < ab[a] ? k : a));
  const bestCut = v.cutBy.get(Number(best));
  /** Renders one cut row. */
  const row = (c: Cut, extra: boolean) => (
    <button
      key={c.n}
      type="button"
      className={`${s.cut} ${extra ? s.cutExtra : ""}`}
      aria-pressed={locked === c.n}
      onMouseEnter={(e) => h.onEnter(c.n, e)}
      onMouseLeave={h.onLeave}
      onFocus={(e) => h.onFocus(c.n, e)}
      onBlur={h.onBlur}
      onClick={(e) => h.onClick(c.n, e)}
    >
      <span className={s.cutN}>{c.n}</span>
      <span className={s.cutT}>
        {c.name}
        <small>
          {edgeCount(v, c)}
          {c.cr ? ` · ${fullName(c.cr)}` : ""}
        </small>
      </span>
      <span className={`${s.chip} ${s[`m_${c.m}`]}`}>{MECH_LABEL[c.m]}</span>
    </button>
  );
  const last = curve.length - 1;
  return (
    <>
      <h4 className={s.h4}>Largest cycle across future crates</h4>
      <Shatter curve={curve} />
      <p className={s.note}>
        No single cut does much. The best one alone ({bestCut?.name}) takes the
        cycle from {curve[0]} files to {ab[best]}. After cuts 1-{last - 2},{" "}
        {curve[last - 2]} files are still knotted; cut {last - 1} brings that
        down to {curve[last - 1]}.
      </p>
      <h4 className={s.h4}>
        The {v.data.text.cuts.length} planned cuts · {m.trait} traits, {m.move}{" "}
        moves, {m.split} splits, {m.inject} injection
      </h4>
      <div className={s.cuts}>
        {v.data.text.cuts.map((c) => row(c, false))}
        <h4 className={s.h4}>Also needed (found edge by edge)</h4>
        {v.data.text.extra.map((c) => row(c, true))}
      </div>
      <p className={s.note}>
        Red: the planned cuts. Gold: the other edges that point upward against
        today's crate graph. Dashed grey: files that import{" "}
        <SourceLink at="4749c4e93" path="src/global.zig" text="global.zig" />{" "}
        (fine now, since it is a leaf). Hover or focus a row to see its edges,
        click or press Enter to pin it.
      </p>
    </>
  );
}
