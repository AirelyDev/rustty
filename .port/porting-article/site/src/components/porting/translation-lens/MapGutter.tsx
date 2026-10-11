import { useMemo } from "react";
import { MAP_W, type Group, ribbons, type Sub } from "./model";
import s from "./TranslationLens.module.css";

/** MAP_H is the map gutter height in px (the pane height on wide screens). */
export const MAP_H = 440;

/** RIBBON_FILL colours a ribbon by group kind: grey code mapping, green carried comment, teal echo. */
const RIBBON_FILL = { code: "#8f8a80", carried: "#98c379", echo: "#64d8c6" };

/** A pane viewport as fractions of its scroll height: top and visible height. */
export type Viewport = { top: number; h: number };

/** Props for MapGutter. */
type MapGutterProps = {
  sub: Sub;
  groups: Group[];
  /** Indices of the highlighted groups (outlined). */
  active: number[];
  zView: Viewport;
  rView: Viewport;
  /** Mouse hover on ribbon group gi at a viewport point. */
  onEnter: (gi: number, x: number, y: number) => void;
  /** Tap or click on ribbon group gi. */
  onTap: (gi: number, x: number, y: number) => void;
  onLeave: () => void;
};

/**
 * The MAP gutter between the panes: each mapping group drawn as a ribbon from
 * its proportional Zig position to its Rust position, plus a viewport marker
 * on each rail that follows the pane's scroll. Mouse-only overview: the same
 * links are reachable from the panes by keyboard, so it is hidden from
 * assistive technology.
 */
export default function MapGutter({
  sub,
  groups,
  active,
  zView,
  rView,
  onEnter,
  onTap,
  onLeave,
}: MapGutterProps) {
  const rib = useMemo(() => ribbons(sub, MAP_H), [sub]);
  const on = new Set(active);
  const span = MAP_H - 8;

  /** Returns the group index of the ribbon under a pointer or click event, or null. */
  const groupAt = (e: { target: EventTarget }) => {
    const g = (e.target as Element).getAttribute("data-g");
    return g === null ? null : Number(g);
  };

  return (
    <div className={s.gut} aria-hidden="true">
      <div className={s.pHdr}>MAP</div>
      <svg
        aria-hidden="true"
        viewBox={`0 0 ${MAP_W} ${MAP_H}`}
        preserveAspectRatio="none"
        onPointerMove={(e) => {
          const gi = groupAt(e);
          if (gi !== null && e.pointerType === "mouse")
            onEnter(gi, e.clientX, e.clientY);
        }}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") onLeave();
        }}
        onClick={(e) => {
          const gi = groupAt(e);
          if (gi !== null) onTap(gi, e.clientX, e.clientY);
        }}
      >
        <rect x="0" y="4" width="5" height={span} fill="#123a35" />
        <rect x={MAP_W - 5} y="4" width="5" height={span} fill="#3a2414" />
        {rib.map((r, i) => (
          <path
            // biome-ignore lint/suspicious/noArrayIndexKey: ribbons are derived in a fixed order from the excerpt
            key={i}
            data-g={r.g}
            d={r.d}
            fill={RIBBON_FILL[r.k]}
            fillOpacity={groups[r.g].k === "code" ? 0.22 : 0.35}
            stroke={on.has(r.g) ? "#f4f1ed" : "none"}
            strokeWidth={0.8}
          />
        ))}
        <rect
          x="0"
          y={4 + zView.top * span}
          width="5"
          height={Math.min(span, Math.max(6, zView.h * span))}
          fill="#64d8c6"
          fillOpacity={0.9}
        />
        <rect
          x={MAP_W - 5}
          y={4 + rView.top * span}
          width="5"
          height={Math.min(span, Math.max(6, rView.h * span))}
          fill="#d97934"
          fillOpacity={0.9}
        />
      </svg>
    </div>
  );
}
