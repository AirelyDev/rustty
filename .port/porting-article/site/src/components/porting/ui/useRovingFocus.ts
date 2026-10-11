"use client";

import { type RefObject, useEffect } from "react";

/** Options for useRovingFocus. */
export type RovingOptions = {
  /** Selector of the focusable items inside the container (default: buttons). */
  selector?: string;
  /**
   * Items per row for grids: ArrowUp/ArrowDown then move by a whole row.
   * Without it every arrow key steps through the items in document order.
   */
  cols?: number;
  /**
   * Table grids with gaps (cells without a control): ArrowUp/ArrowDown move to
   * the item in the nearest row above or below whose cell column is closest.
   */
  grid?: boolean;
};

/** An item that can take focus: an HTML control or an SVG mark with a tabindex. */
type Focusable = HTMLElement | SVGElement;

/** DEFAULT_SELECTOR picks the tooltip triggers and other buttons of a chart. */
const DEFAULT_SELECTOR = "button";

/**
 * Returns the index an arrow, Home or End key moves to from i among n items,
 * or null for any other key. With cols, ArrowUp/ArrowDown move by a row.
 */
export function rovingTarget(
  key: string,
  i: number,
  n: number,
  cols?: number,
): number | null {
  const row = cols && cols > 0 ? cols : 1;
  switch (key) {
    case "ArrowRight":
      return Math.min(n - 1, i + 1);
    case "ArrowLeft":
      return Math.max(0, i - 1);
    case "ArrowDown":
      return Math.min(n - 1, i + row);
    case "ArrowUp":
      return Math.max(0, i - row);
    case "Home":
      return 0;
    case "End":
      return n - 1;
    default:
      return null;
  }
}

/**
 * Returns the index of the item in the nearest table row above (dir -1) or
 * below (dir 1) item i whose cell column is closest to i's, or null when
 * there is no such row. Items outside a table cell are skipped.
 */
export function gridTarget(
  all: Focusable[],
  i: number,
  dir: 1 | -1,
): number | null {
  /** Returns the row and column of an item's table cell, or null. */
  const pos = (el: Focusable) => {
    const cell = el.closest("td, th") as HTMLTableCellElement | null;
    const row = cell?.parentElement as HTMLTableRowElement | null;
    return cell && row
      ? { row: row.sectionRowIndex, col: cell.cellIndex }
      : null;
  };
  const from = pos(all[i]);
  if (!from) return null;
  let best = -1;
  let bestRow = Number.POSITIVE_INFINITY;
  let bestCol = Number.POSITIVE_INFINITY;
  for (let idx = 0; idx < all.length; idx++) {
    const p = pos(all[idx]);
    if (!p || Math.sign(p.row - from.row) !== dir) continue;
    const rowGap = Math.abs(p.row - from.row);
    const colGap = Math.abs(p.col - from.col);
    if (rowGap < bestRow || (rowGap === bestRow && colGap < bestCol)) {
      best = idx;
      bestRow = rowGap;
      bestCol = colGap;
    }
  }
  return best >= 0 ? best : null;
}

/**
 * Makes the items inside a container one Tab stop (a roving tabindex): the
 * last focused item (initially the first) has tabIndex 0, the others -1, and
 * arrow keys, Home and End move focus between them. Items added later (a
 * filter or selection re-rendering the chart) are folded in. Focus still
 * reaches every item, so tooltips shown on focus stay keyboard-reachable.
 */
export function useRovingFocus(
  ref: RefObject<HTMLElement | SVGSVGElement | null>,
  { selector = DEFAULT_SELECTOR, cols, grid = false }: RovingOptions = {},
) {
  useEffect(() => {
    const root: Element | null = ref.current;
    if (!root) return;
    let active = 0;
    /** Returns the current items in document order. */
    const items = () =>
      [...root.querySelectorAll<Focusable>(selector)].filter(
        (el) => !(el as HTMLButtonElement).disabled,
      );
    /** Gives the active item tabIndex 0 and every other item -1. */
    const apply = () => {
      const all = items();
      if (all.length === 0) return;
      active = Math.min(active, all.length - 1);
      all.forEach((el, i) => {
        const want = i === active ? 0 : -1;
        if (el.tabIndex !== want) el.tabIndex = want;
      });
    };
    /** Tracks the focused item so Tab returns to it. */
    const onFocusIn = (e: Event) => {
      const i = items().indexOf(e.target as Focusable);
      if (i >= 0 && i !== active) {
        active = i;
        apply();
      }
    };
    /** Moves focus on arrow, Home and End keys. */
    const onKeyDown = (ev: Event) => {
      const e = ev as KeyboardEvent;
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      const all = items();
      const i = all.indexOf(e.target as Focusable);
      if (i < 0) return;
      const vertical = e.key === "ArrowUp" || e.key === "ArrowDown";
      const next =
        grid && vertical
          ? gridTarget(all, i, e.key === "ArrowDown" ? 1 : -1)
          : rovingTarget(e.key, i, all.length, cols);
      if (next === null) return;
      e.preventDefault();
      active = next;
      apply();
      all[next].focus();
    };
    apply();
    const mo = new MutationObserver(apply);
    mo.observe(root, { childList: true, subtree: true });
    root.addEventListener("focusin", onFocusIn);
    root.addEventListener("keydown", onKeyDown);
    return () => {
      mo.disconnect();
      root.removeEventListener("focusin", onFocusIn);
      root.removeEventListener("keydown", onKeyDown);
    };
  }, [ref, selector, cols, grid]);
}
