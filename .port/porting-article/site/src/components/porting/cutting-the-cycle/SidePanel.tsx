"use client";

import type { FocusEvent, MouseEvent } from "react";
import type { ChartTip } from "../ui/useChartTip";
import SideCrates from "./SideCrates";
import SideCuts from "./SideCuts";
import SideCycle from "./SideCycle";
import SideReplay from "./SideReplay";
import type { View } from "./view";

/** Props for SidePanel: the stage, the shared tooltip and the state the side panels read and change. */
type SidePanelProps = {
  v: View;
  stage: number;
  tip: ChartTip;
  example: number | null;
  setExample: (i: number | null) => void;
  locked: number | null;
  setLocked: (n: number | null) => void;
  selected: number | null;
  setSelected: (i: number | null) => void;
  chain: boolean;
  setChain: (on: boolean) => void;
  layer: boolean;
  setLayer: (on: boolean) => void;
};

/** Returns the anchor below the left edge of a row (keyboard focus or Enter). */
function rowAnchor(el: Element): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.bottom };
}

/**
 * The side panel of the current stage. Owns the cut-row handlers (hover or
 * focus shows a cut, click or Enter pins and locks it) and the crate-graph
 * toggles, so Body stays a layout of the views.
 */
export default function SidePanel(p: SidePanelProps) {
  const { v, stage, tip } = p;
  if (stage === 1)
    return <SideCycle v={v} example={p.example} onExample={p.setExample} />;
  if (stage === 4) return <SideReplay v={v} />;
  if (stage === 3) {
    /** Turns the longest-chain or layering highlight on (the other off) or off, closing any tooltip. */
    const onToggle = (t: "chain" | "layer") => {
      tip.clear();
      p.setChain(t === "chain" && !p.chain);
      p.setLayer(t === "layer" && !p.layer);
    };
    /** Clears the selected crate and any tooltip. */
    const onClear = () => {
      tip.clear();
      p.setSelected(null);
    };
    return (
      <SideCrates
        v={v}
        selected={p.selected}
        showChain={p.chain}
        showLayer={p.layer}
        onToggle={onToggle}
        onClear={onClear}
      />
    );
  }
  /** Shows cut n's tooltip at the pointer (mouse hover). */
  const onEnter = (n: number, e: MouseEvent<HTMLButtonElement>) =>
    tip.hover(`k${n}`, { x: e.clientX, y: e.clientY });
  /** Shows cut n's tooltip below its row (keyboard focus). */
  const onFocus = (n: number, e: FocusEvent<HTMLButtonElement>) =>
    tip.focus(`k${n}`, rowAnchor(e.currentTarget));
  /** Locks or unlocks cut n, pinning its tooltip at the pointer (or below the row for Enter). */
  const onClick = (n: number, e: MouseEvent<HTMLButtonElement>) => {
    const at =
      e.detail === 0
        ? rowAnchor(e.currentTarget)
        : { x: e.clientX, y: e.clientY };
    const next = p.locked === n ? null : n;
    p.setLocked(next);
    if (next === null) tip.clear();
    else if (!(tip.pinned && tip.shown === `k${n}`)) tip.tap(`k${n}`, at);
  };
  return (
    <SideCuts
      v={v}
      curve={v.data.zig.stats.cum}
      locked={p.locked}
      h={{ onEnter, onLeave: tip.leave, onFocus, onBlur: tip.leave, onClick }}
    />
  );
}
