"use client";

import { PALETTE } from "../lib/palette";
import { cratePath, depthX, TIER_COLOR, VIEW_W } from "./geometry";
import type { ReplayFrame } from "./model";
import { crateLabel, type View } from "./view";

/** EDGE_BASE is the resting colour of a crate edge. */
const EDGE_BASE = "#5a605b";

/** Props for CrateLayer. */
type CrateLayerProps = {
  v: View;
  /** 3 (the crate graph) or 4 (replay). */
  stage: number;
  /** Selected crate (click), or null. */
  selected: number | null;
  /** Crate under the pointer or keyboard cursor, or null. */
  hovered: number | null;
  /** Crate edge under the pointer, or null. */
  edge: number | null;
  /** Highlight the longest dependency chain (copper) and dim the rest. */
  showChain: boolean;
  /** Highlight the crate edges explained by a layering decision (gold) and dim the rest. */
  showLayer: boolean;
  /** Replay frame (stage 4), else null. */
  frame: ReplayFrame | null;
  /** Pointer entered crate i (mouse). */
  onEnterCrate: (i: number, x: number, y: number) => void;
  /** Crate i was clicked or tapped. */
  onTapCrate: (i: number, x: number, y: number) => void;
  /** Pointer entered crate edge i (mouse). */
  onEnterEdge: (i: number, x: number, y: number) => void;
  /** Crate edge i was tapped. */
  onTapEdge: (i: number, x: number, y: number) => void;
  /** Pointer left a crate or edge. */
  onLeave: () => void;
};

/** Returns the stroke style of crate edge i under the current toggles, selection, hover and replay state. */
function edgeStyle(
  p: CrateLayerProps,
  i: number,
): { color: string; op: number; w: number } {
  const { v, frame, selected, hovered, edge, showChain, showLayer } = p;
  const e = v.crateEdges[i];
  if (edge === i) return { color: PALETTE.gray8, op: 1, w: 2 };
  const color = EDGE_BASE;
  let op = 0.35;
  const w = 0.7;
  if (frame) {
    const a = v.lay.crates;
    op = frame.alive[a[e.s].id] && frame.alive[a[e.t].id] ? 0.3 : 0;
  }
  if (showChain && v.chainEdge.has(e.key))
    return { color: PALETTE.rust, op: 1, w: 2 };
  if (showLayer && v.layerEdge.has(e.key))
    return { color: PALETTE.gold, op: 1, w: 2 };
  if (hovered !== null && (e.s === hovered || e.t === hovered) && op > 0) {
    return {
      color: e.s === hovered ? PALETTE.rust : PALETTE.blue,
      op: 0.9,
      w: 1.3,
    };
  }
  if (selected !== null) {
    if (e.s === selected) return { color: PALETTE.rust, op: 0.9, w: 1.3 };
    if (e.t === selected) return { color: PALETTE.blue, op: 0.9, w: 1.3 };
    op = Math.min(op, 0.06);
  } else if (showChain || showLayer) op = Math.min(op, 0.08);
  return { color, op, w };
}

/** Returns the opacity of crate i (hidden before its birth in the replay, dimmed outside the selection or chain). */
function nodeOpacity(p: CrateLayerProps, i: number): number {
  const { v, frame, selected, showChain } = p;
  const n = v.lay.crates[i];
  if (frame && !frame.alive[n.id]) return 0;
  if (
    selected !== null &&
    selected !== i &&
    !v.crateEdges.some(
      (e) => (e.s === selected && e.t === i) || (e.t === selected && e.s === i),
    )
  )
    return 0.25;
  if (showChain && !v.chainSet.has(n.id)) return 0.25;
  return 1;
}

/** Returns the radius of crate i: code-line area, or in the replay the physical lines that day. */
function nodeRadius(p: CrateLayerProps, i: number): number {
  const n = p.v.lay.crates[i];
  if (!p.frame) return n.r;
  if (n.sys) return 3;
  const lines = p.frame.lines[n.id] ?? 0;
  return lines > 0 ? 2.4 + 16 * Math.sqrt(lines / p.v.maxLines) : 2.4;
}

/**
 * Stages 3-4: the 61 packages in depth columns (area by code lines, -sys
 * crates as hollow rings, colour by tier), with dependency edges (dev dashed,
 * build dotted), the depth axis and, in the replay, the per-depth share strip.
 */
export default function CrateLayer(p: CrateLayerProps) {
  const { v, frame } = p;
  const c = v.lay.crates;
  return (
    <g>
      {Array.from({ length: v.lay.maxDepth + 1 }, (_, d) => (
        <text
          // biome-ignore lint/suspicious/noArrayIndexKey: one label per depth
          key={d}
          x={depthX(d, v.lay.maxDepth)}
          y={612}
          textAnchor="middle"
          fontSize={9}
          fill={PALETTE.gray3}
        >
          {d}
        </text>
      ))}
      <text
        x={340}
        y={598}
        textAnchor="middle"
        fontSize={9}
        fill={PALETTE.gray3}
      >
        depth: longest dependency path to a leaf →
      </text>
      {v.crateEdges.map((e, i) => {
        const st = edgeStyle(p, i);
        const d = cratePath(c[e.s], c[e.t]);
        return (
          <g key={e.key + e.k}>
            <path
              d={d}
              fill="none"
              stroke={st.color}
              strokeOpacity={st.op}
              strokeWidth={st.w}
              strokeDasharray={
                e.k === "dev" ? "3 3" : e.k === "build" ? "1 2" : undefined
              }
            />
            {st.op > 0 ? (
              // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; keyboard access is on the role=application svg
              <path
                d={d}
                fill="none"
                stroke="transparent"
                strokeWidth={5}
                style={{ cursor: "help" }}
                onPointerEnter={(ev) => {
                  if (ev.pointerType === "mouse")
                    p.onEnterEdge(i, ev.clientX, ev.clientY);
                }}
                onPointerLeave={(ev) => {
                  if (ev.pointerType === "mouse") p.onLeave();
                }}
                onClick={(ev) => p.onTapEdge(i, ev.clientX, ev.clientY)}
              />
            ) : null}
          </g>
        );
      })}
      {c.map((n, i) => {
        const r = nodeRadius(p, i);
        const op = nodeOpacity(p, i);
        const on = p.selected === i || p.hovered === i;
        return (
          // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; keyboard access is on the role=application svg
          <g
            key={n.id}
            opacity={op}
            style={{ cursor: "pointer" }}
            onPointerEnter={(ev) => {
              if (ev.pointerType === "mouse" && op > 0)
                p.onEnterCrate(i, ev.clientX, ev.clientY);
            }}
            onPointerLeave={(ev) => {
              if (ev.pointerType === "mouse") p.onLeave();
            }}
            onClick={(ev) => {
              if (op > 0) p.onTapCrate(i, ev.clientX, ev.clientY);
            }}
          >
            <circle
              cx={n.x}
              cy={n.y}
              r={r}
              fill={n.sys ? PALETTE.card : TIER_COLOR[n.tier]}
              fillOpacity={n.sys ? 1 : 0.92}
              stroke={on ? PALETTE.gray8 : n.sys ? PALETTE.gray4 : PALETTE.card}
              strokeWidth={on ? 2 : 1}
            />
            <text
              x={n.x + r + 2.5}
              y={n.y + 3}
              fontSize={n.loc > 20000 ? 10 : 8.5}
              fill={n.sys ? PALETTE.gray4 : PALETTE.gray6}
            >
              {crateLabel(v, i)}
            </text>
          </g>
        );
      })}
      {frame ? (
        <g>
          <text
            x={VIEW_W - 8}
            y={27}
            textAnchor="end"
            fontSize={8}
            fill={PALETTE.gray4}
          >
            bars: share of today's lines reached, per depth
          </text>
          {frame.share.map((f, d) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: one bar per depth
            <g key={d}>
              <rect
                x={depthX(d, v.lay.maxDepth) - 9}
                y={2}
                width={18}
                height={12}
                fill="#1a1c1b"
                rx={2}
              />
              <rect
                x={depthX(d, v.lay.maxDepth) - 9}
                y={2}
                width={18 * Math.min(1, f ?? 0)}
                height={12}
                fill={PALETTE.rust}
                fillOpacity={0.85}
                rx={2}
              >
                <title>{`depth ${d}: ${Math.round(100 * (f ?? 0))}% of today's lines`}</title>
              </rect>
            </g>
          ))}
        </g>
      ) : null}
    </g>
  );
}
