"use client";

import { type KeyboardEvent, type RefObject, useState } from "react";
import type { ChartTip } from "../ui/useChartTip";
import { VIEW_W } from "./geometry";
import type { ReplayFrame } from "./model";
import type { View } from "./view";

/** What useGraphKeys returns: the keyboard cursor and the svg's key and focus handlers. */
export type GraphKeys = {
  /** Point under the keyboard cursor (file or crate index), or null. */
  cursor: number | null;
  /** Clears the cursor (stage change). */
  reset: () => void;
  onKeyDown: (e: KeyboardEvent<SVGSVGElement>) => void;
  /** Puts the cursor on the first point when the graph gains keyboard focus. */
  onFocus: (e: { currentTarget: Element }) => void;
  onBlur: () => void;
};

/**
 * Keyboard access to the main graph: the arrow keys step through the files
 * (clockwise) or the crates (by depth, then top to bottom; only crates alive
 * on the replay date), showing each one's tooltip; Enter or Space pins it and,
 * in the crate graph, selects the crate; Escape closes it.
 */
export default function useGraphKeys(
  v: View,
  stage: number,
  frame: ReplayFrame | null,
  svgRef: RefObject<SVGSVGElement | null>,
  tip: ChartTip,
  onSelect: (i: number) => void,
): GraphKeys {
  const [cursor, setCursor] = useState<number | null>(null);
  const files = stage <= 2;
  const order = files
    ? v.fileOrder
    : v.crateOrder.filter((i) => !frame || frame.alive[v.lay.crates[i].id]);

  /** Returns the tooltip id and viewport anchor of point i of the current view. */
  const pointOf = (i: number) => {
    const p = files ? v.lay.files[i] : v.lay.crates[i];
    const r = svgRef.current?.getBoundingClientRect();
    const k = r ? r.width / VIEW_W : 0;
    return {
      id: `${files ? "f" : "n"}${i}`,
      at: r ? { x: r.left + p.x * k, y: r.top + p.y * k } : { x: 0, y: 0 },
    };
  };
  /** Moves the cursor to the k-th point in keyboard order. */
  const focusTo = (k: number) => {
    if (!order.length) return;
    const i = order[Math.max(0, Math.min(order.length - 1, k))];
    setCursor(i);
    const p = pointOf(i);
    tip.focus(p.id, p.at);
  };
  /** Handles arrows, Enter/Space and Escape on the graph. */
  const onKeyDown = (e: KeyboardEvent<SVGSVGElement>) => {
    const pos = cursor === null ? -1 : order.indexOf(cursor);
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(pos + 1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(Math.max(0, pos - 1));
    } else if ((e.key === "Enter" || e.key === " ") && cursor !== null) {
      e.preventDefault();
      const p = pointOf(cursor);
      tip.tap(p.id, p.at);
      if (stage === 3) onSelect(cursor);
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };
  return {
    cursor,
    reset: () => setCursor(null),
    onKeyDown,
    onFocus: (e) => {
      if (cursor === null && e.currentTarget.matches(":focus-visible"))
        focusTo(0);
    },
    onBlur: () => {
      setCursor(null);
      tip.leave();
    },
  };
}
