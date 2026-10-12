"use client";

import { memo, type RefObject, useMemo } from "react";
import { PALETTE } from "../lib/palette";
import { filePath, filePaths } from "./geometry";
import { shortestCycle } from "./model";
import { baseEdgeGroups, GLOBAL_TAG, type View } from "./view";

/** Props for CycleLayer. */
type CycleLayerProps = {
  v: View;
  /** 1 (the cycle) or 2 (the cuts). */
  stage: number;
  /** File under the pointer or keyboard cursor, or null. */
  file: number | null;
  /** Cut row hovered or pinned (stage 2), or null. */
  cut: number | null;
  /** Example cycle picked in the side panel (stage 1), or null. */
  example: number | null;
  /** True while the dots morph into the crate graph (edges and sectors hidden). */
  morphing: boolean;
  /** Ref to the group of file dots (animated directly during the morph). */
  dotsRef: RefObject<SVGGElement | null>;
  /** Pointer entered file i (mouse). */
  onEnter: (i: number, x: number, y: number) => void;
  /** Pointer left a file. */
  onLeave: () => void;
  /** File i was clicked or tapped. */
  onTap: (i: number, x: number, y: number) => void;
};

/** Draws the base edge groups (one path per style), memoised per stage. */
const BaseEdges = memo(function BaseEdges({
  v,
  stage,
}: {
  v: View;
  stage: number;
}) {
  const groups = useMemo(
    () => baseEdgeGroups(v, stage, (es) => filePaths(v.data, v.lay.files, es)),
    [v, stage],
  );
  return (
    <>
      {groups.map((g) => (
        <path
          key={g.key}
          d={g.d}
          fill="none"
          stroke={g.color}
          strokeOpacity={g.opacity}
          strokeWidth={g.width}
          strokeDasharray={g.dash}
        />
      ))}
    </>
  );
});

/** Draws a path of files (a cycle) in white with rings on its files. */
function CyclePath({ v, path }: { v: View; path: number[] }) {
  const f = v.lay.files;
  return (
    <g>
      <path
        d={path
          .slice(1)
          .map((b, i) => filePath(f[path[i]], f[b]))
          .join("")}
        fill="none"
        stroke={PALETTE.gray8}
        strokeWidth={2.2}
        strokeOpacity={0.95}
      />
      {path.slice(1).map((i) => (
        <circle
          key={i}
          cx={f[i].x}
          cy={f[i].y}
          r={4}
          fill="none"
          stroke={PALETTE.gray8}
          strokeWidth={1.4}
        />
      ))}
    </g>
  );
}

/**
 * Stages 1-2: the 345 files of the cycle laid out radially by directory, with
 * sector arcs and counts, bundled import edges, and the highlight of a hovered
 * file (imports copper, importers teal, shortest cycle white), a cut (its
 * edges, other files dimmed) or an example cycle.
 */
export default function CycleLayer({
  v,
  stage,
  file,
  cut,
  example,
  morphing,
  dotsRef,
  onEnter,
  onLeave,
  onTap,
}: CycleLayerProps) {
  const f = v.lay.files;
  const edges = v.data.zig.edges;

  // Highlight overlay and how much the base edges fade under it.
  const hi = useMemo(() => {
    if (file !== null) {
      return {
        fade: 0.28,
        outs: filePaths(v.data, f, v.g.outEdges[file]),
        ins: filePaths(v.data, f, v.g.inEdges[file]),
        cycle: shortestCycle(v.g, file),
        cut: null,
        touched: null,
      };
    }
    if (cut !== null) {
      const own = v.tagEdges.get(cut) ?? [];
      const ctx = cut === 1 ? (v.tagEdges.get(GLOBAL_TAG) ?? []) : [];
      const touched = new Set<number>();
      for (const e of own) {
        touched.add(edges[e][0]);
        touched.add(edges[e][1]);
      }
      for (const e of ctx) touched.add(edges[e][0]);
      return {
        fade: 0.05,
        outs: null,
        ins: null,
        cycle: null,
        cut: {
          d: filePaths(v.data, f, own),
          ctx: filePaths(v.data, f, ctx),
          color: cut > 14 ? PALETTE.gold : PALETTE.fail,
        },
        touched,
      };
    }
    if (example !== null) {
      const idx = new Map(v.g.names.map((n, i) => [n, i]));
      return {
        fade: 0.33,
        outs: null,
        ins: null,
        cycle: v.data.zig.examples[example].map((n) => idx.get(n) ?? 0),
        cut: null,
        touched: null,
      };
    }
    return null;
  }, [v, f, edges, file, cut, example]);

  return (
    <g>
      {morphing ? null : (
        <g>
          {v.lay.sectors.map((s) => (
            <g key={s.d}>
              <path d={s.arc} stroke="#3a3f3b" strokeWidth={1} fill="none" />
              <text
                x={s.x}
                y={s.y}
                textAnchor={s.anchor}
                fontSize={10}
                fill={PALETTE.gray5}
              >
                {s.label}
              </text>
            </g>
          ))}
          <text
            x={v.lay.centre.x}
            y={v.lay.centre.y - 10}
            textAnchor="middle"
            fontSize={10}
            fill={PALETTE.gray8}
          >
            global.zig
          </text>
          <g opacity={hi ? hi.fade : 1}>
            <BaseEdges v={v} stage={stage} />
          </g>
          {hi?.outs ? (
            <path
              d={hi.outs}
              fill="none"
              stroke={PALETTE.rust}
              strokeOpacity={0.7}
              strokeWidth={0.8}
            />
          ) : null}
          {hi?.ins ? (
            <path
              d={hi.ins}
              fill="none"
              stroke={PALETTE.upstream}
              strokeOpacity={0.55}
              strokeWidth={0.8}
            />
          ) : null}
          {hi?.cut ? (
            <>
              <path
                d={hi.cut.ctx}
                fill="none"
                stroke={PALETTE.gray5}
                strokeOpacity={0.8}
                strokeWidth={0.8}
                strokeDasharray="2 2"
              />
              <path
                d={hi.cut.d}
                fill="none"
                stroke={hi.cut.color}
                strokeWidth={2.4}
              />
            </>
          ) : null}
          {hi?.cycle?.length ? <CyclePath v={v} path={hi.cycle} /> : null}
        </g>
      )}
      <g ref={dotsRef}>
        {f.map((p, i) => (
          // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; keyboard access is on the role=application svg
          <circle
            // biome-ignore lint/suspicious/noArrayIndexKey: files are a fixed list in data order
            key={i}
            cx={p.x}
            cy={p.y}
            r={p.r}
            fill={p.color}
            stroke={PALETTE.card}
            strokeWidth={0.8}
            opacity={hi?.touched && !hi.touched.has(i) ? 0.18 : 1}
            style={{ cursor: "pointer" }}
            onPointerEnter={(e) => {
              if (e.pointerType === "mouse" && !morphing)
                onEnter(i, e.clientX, e.clientY);
            }}
            onPointerLeave={(e) => {
              if (e.pointerType === "mouse" && !morphing) onLeave();
            }}
            onClick={(e) => {
              if (!morphing) onTap(i, e.clientX, e.clientY);
            }}
          />
        ))}
      </g>
    </g>
  );
}
