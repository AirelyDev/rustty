import type { ReactNode } from "react";
import type { ChartCtx } from "./chart-context";
import { barH, HEAD_H, GRID_H, X } from "./geometry";
import { MODEL_COLOURS, MODEL_TEXTURES } from "./model";

/** COST_SERIES are the stacked cost segments: data field and colour (Mac Claude, VM Claude, Codex). */
export const COST_SERIES = [
  ["costMacClaude", "#ffbc7a", "Mac Claude"],
  ["costVmClaude", "#d19a66", "VM Claude"],
  ["costCodex", "#61aeee", "Codex"],
] as const;

/** Visible-range maxima the ribbons and marginals scale to. */
export type Maxima = {
  commits: number;
  prompts: number;
  lines: number;
  threads: number;
  cost: number;
};

/**
 * Draws the three ribbons aligned to the day axis: the 100%-stacked model mix
 * (textured where two model colours are close), the stacked API-equivalent
 * cost (hatched "not recorded" over the lost Mac transcript window, never a
 * zero bar) and the agent threads per day on a log scale.
 */
export function Ribbons({ ctx, max }: { ctx: ChartCtx; max: Maxima }) {
  const { rows: r, data, vis, ids } = ctx;
  const dy = data.daily;
  const out: ReactNode[] = [];
  const bh = barH(r);
  for (let i = r.d0; i <= r.d1; i++) {
    if (!vis(i)) continue;
    const y = r.y(i);
    const mix = dy.mix[i];
    const total = mix.reduce((a, b) => a + b, 0);
    let xx = X.model;
    if (total) {
      for (let j = 0; j < mix.length; j++) {
        const v = mix[j];
        if (!v) continue;
        const w = (X.mw * v) / total;
        const width = Math.max(0.5, w - 0.4);
        out.push(
          <rect
            key={`m${i}-${j}`}
            x={xx}
            y={y + r.pad}
            width={width}
            height={bh}
            fill={MODEL_COLOURS[j]}
          />,
        );
        if (MODEL_TEXTURES[j]) {
          out.push(
            <rect
              key={`mt${i}-${j}`}
              x={xx}
              y={y + r.pad}
              width={width}
              height={bh}
              fill={`url(#${ids}-${MODEL_TEXTURES[j]})`}
            />,
          );
        }
        xx += w;
      }
    }
    if (dy.lost[i]) {
      out.push(
        <rect
          key={`lost${i}`}
          x={X.cost}
          y={y}
          width={X.costW}
          height={r.h}
          fill={`url(#${ids}-lost)`}
        />,
      );
    }
    let cx = X.cost;
    for (const [field, colour] of COST_SERIES) {
      const v = dy[field][i];
      if (v > 0) {
        const w = Math.max(1, (X.costW * v) / max.cost);
        out.push(
          <rect
            key={`$${field}${i}`}
            x={cx}
            y={y + r.pad}
            width={w}
            height={bh}
            fill={colour}
          />,
        );
        cx += w;
      }
    }
    const th = dy.threads[i];
    if (th) {
      out.push(
        <rect
          key={`t${i}`}
          x={X.thr}
          y={y + r.pad}
          width={Math.max(
            1,
            (X.thrW * Math.log1p(th)) / Math.log1p(max.threads),
          )}
          height={bh}
          fill="#9aa09a"
          opacity={0.7}
        />,
      );
    }
  }
  return <g>{out}</g>;
}

/**
 * Draws the per-day marginals: commits, prompts, and lines removed (red, to
 * the left) and added (green, to the right) on a √ scale. All re-scale to the
 * visible range.
 */
export function Marginals({ ctx, max }: { ctx: ChartCtx; max: Maxima }) {
  const { rows: r, totals: t, vis } = ctx;
  const out: ReactNode[] = [];
  const bh = barH(r);
  const mid = X.lin + X.linW / 2;
  /** Half-width of a lines bar for v lines on the √ scale. */
  const sq = (v: number) => (X.linW / 2) * Math.sqrt(v / max.lines);
  for (let i = r.d0; i <= r.d1; i++) {
    if (!vis(i)) continue;
    const y = r.y(i) + r.pad;
    if (t.commits[i]) {
      out.push(
        <rect
          key={`c${i}`}
          x={X.com}
          y={y}
          width={Math.max(1, (X.comW * t.commits[i]) / max.commits)}
          height={bh}
          fill="#d97934"
        />,
      );
    }
    if (t.prompts[i]) {
      out.push(
        <rect
          key={`p${i}`}
          x={X.pro}
          y={y}
          width={Math.max(1, (X.proW * t.prompts[i]) / max.prompts)}
          height={bh}
          fill="#64d8c6"
        />,
      );
    }
    if (t.added[i]) {
      out.push(
        <rect
          key={`a${i}`}
          x={mid}
          y={y}
          width={Math.max(1, sq(t.added[i]))}
          height={bh}
          fill="#98c379"
        />,
      );
    }
    if (t.deleted[i]) {
      const w = Math.max(1, sq(t.deleted[i]));
      out.push(
        <rect
          key={`r${i}`}
          x={mid - w}
          y={y}
          width={w}
          height={bh}
          fill="#e06c75"
        />,
      );
    }
  }
  out.push(
    <line
      key="mid"
      x1={mid}
      x2={mid}
      y1={HEAD_H}
      y2={HEAD_H + GRID_H}
      stroke="#262927"
    />,
  );
  return <g>{out}</g>;
}
