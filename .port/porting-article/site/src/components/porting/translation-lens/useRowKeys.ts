"use client";

import { type KeyboardEvent, type RefObject, useState } from "react";
import type { ChartTip } from "../ui/useChartTip";

/** What useRowKeys returns: the keyboard row and the chart's key and blur handlers. */
export type RowKeys = {
  /** Row under the keyboard cursor, or null. */
  cursor: number | null;
  onKeyDown: (e: KeyboardEvent<SVGSVGElement>) => void;
  onBlur: () => void;
};

/**
 * Keyboard access to a row chart: Up/Down (or Left/Right) step through the
 * rows showing each one's tooltip, Enter or Space pins it, Escape closes it.
 * Rows are `[data-row]` elements inside the chart; tooltip ids are prefix + row.
 */
export default function useRowKeys(
  count: number,
  prefix: string,
  svgRef: RefObject<SVGSVGElement | null>,
  tip: ChartTip,
): RowKeys {
  const [cursor, setCursor] = useState<number | null>(null);

  /** Returns the viewport anchor (right end of the row's label) of row i. */
  const anchor = (i: number) => {
    const el = svgRef.current?.querySelector(`[data-row="${i}"]`);
    const r = el?.getBoundingClientRect();
    return r
      ? { x: r.left + r.width * 0.4, y: r.top + r.height / 2 }
      : { x: 0, y: 0 };
  };
  /** Moves the cursor to row i (clamped) and shows its tooltip. */
  const go = (i: number) => {
    if (!count) return;
    const k = Math.max(0, Math.min(count - 1, i));
    setCursor(k);
    tip.focus(`${prefix}${k}`, anchor(k));
  };
  /** Handles the arrow keys, Enter/Space and Escape on the chart. */
  const onKeyDown = (e: KeyboardEvent<SVGSVGElement>) => {
    const at = cursor ?? -1;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") {
      e.preventDefault();
      go(at + 1);
    } else if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
      e.preventDefault();
      go(Math.max(0, at - 1));
    } else if ((e.key === "Enter" || e.key === " ") && cursor !== null) {
      e.preventDefault();
      tip.tap(`${prefix}${cursor}`, anchor(cursor));
    } else if (e.key === "Escape") {
      tip.clear();
      setCursor(null);
    }
  };
  return {
    cursor,
    onKeyDown,
    onBlur: () => {
      setCursor(null);
      if (!tip.pinned) tip.leave();
    },
  };
}
