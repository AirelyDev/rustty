"use client";

import {
  type KeyboardEvent,
  type PointerEvent,
  type MouseEvent as ReactMouseEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ChartTip } from "../ui/useChartTip";
import { ease } from "./model";

/** NARROW_QUERY is the phone layout breakpoint (stacked layouts below 800 px). */
const NARROW_QUERY = "(max-width: 799px)";

/** TWEEN_MS is the duration of the dot tween between two replay steps (mockup: 650 ms). */
const TWEEN_MS = 650;

/** Returns true below the phone breakpoint; tracks changes. */
export function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(
    () =>
      typeof window !== "undefined" && window.matchMedia(NARROW_QUERY).matches,
  );
  useEffect(() => {
    const mq = window.matchMedia(NARROW_QUERY);
    /** Syncs the state with the media query. */
    const onChange = () => setNarrow(mq.matches);
    onChange();
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return narrow;
}

/** A tween between two replay steps: draw `from` → `to` at eased progress t. */
export type Tween = { from: number; to: number; t: number };

/**
 * Tweens from the previously shown step to `index` (eased, 650 ms). With
 * reduced motion, or when `animate` is false, it jumps straight to the step.
 */
export function useTween(index: number, reduced: boolean): Tween {
  const [tw, setTw] = useState<Tween>({ from: index, to: index, t: 1 });
  const shown = useRef(index);
  useEffect(() => {
    const from = shown.current;
    shown.current = index;
    if (from === index) return;
    if (reduced) {
      setTw({ from: index, to: index, t: 1 });
      return;
    }
    const t0 = performance.now();
    let raf = 0;
    /** Advances the tween one animation frame. */
    const frame = (now: number) => {
      const p = Math.min(1, (now - t0) / TWEEN_MS);
      setTw({ from, to: index, t: ease(p) });
      if (p < 1) raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [index, reduced]);
  return tw;
}

/** Returns the tip id of the nearest `[data-tip]` ancestor of an event target, or null. */
function tipIdOf(target: EventTarget | null): string | null {
  const el = (target as Element | null)?.closest?.("[data-tip]");
  return el?.getAttribute("data-tip") ?? null;
}

/** Returns the viewport centre of an element. */
function centre(el: Element): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/** Options for tipHandlers. */
type TipOptions = {
  /** Called with a hovered, tapped or keyboard-focused tip id. */
  onItem?: (id: string) => void;
  /** "vertical": only Up/Down move (Left/Right bubble up, e.g. to step the replay). */
  axis?: "vertical" | "both";
  /** Called on Enter/Space for the keyboard-focused id; return true when handled (otherwise it pins). */
  onEnter?: (id: string) => boolean;
};

/**
 * Pointer and keyboard handlers for a chart whose hover targets carry
 * `data-tip` ids. Mouse hover shows the tooltip, a tap or click pins it, and
 * on the focused chart the arrow keys step through the targets in document
 * order (Enter pins or activates, Escape closes).
 */
export function tipHandlers(tip: ChartTip, opts: TipOptions = {}) {
  return {
    /** Shows the hovered target's tooltip (mouse only). */
    onPointerMove: (e: PointerEvent<Element>) => {
      if (e.pointerType !== "mouse") return;
      const id = tipIdOf(e.target);
      if (!id) {
        tip.leave();
        return;
      }
      if (id !== tip.shown) opts.onItem?.(id);
      tip.hover(id, { x: e.clientX, y: e.clientY });
    },
    /** Hides the transient tooltip. */
    onPointerLeave: () => tip.leave(),
    /** Pins or unpins the clicked target's tooltip (and activates it, e.g. a step node). */
    onClick: (e: ReactMouseEvent<Element>) => {
      const id = tipIdOf(e.target);
      if (!id) return;
      opts.onItem?.(id);
      opts.onEnter?.(id);
      tip.tap(id, { x: e.clientX, y: e.clientY });
    },
    /** Arrow keys step through the targets; Enter/Space pins or activates; Escape closes. */
    onKeyDown: (e: KeyboardEvent<Element>) => {
      const items = Array.from(
        e.currentTarget.querySelectorAll<Element>("[data-tip]"),
      );
      if (!items.length) return;
      const at = items.findIndex(
        (el) => el.getAttribute("data-tip") === tip.shown,
      );
      const both = opts.axis !== "vertical";
      let next = -1;
      if (e.key === "ArrowDown" || (both && e.key === "ArrowRight"))
        next = at < 0 ? 0 : Math.min(items.length - 1, at + 1);
      else if (e.key === "ArrowUp" || (both && e.key === "ArrowLeft"))
        next = at < 0 ? 0 : Math.max(0, at - 1);
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = items.length - 1;
      else if ((e.key === "Enter" || e.key === " ") && at >= 0) {
        e.preventDefault();
        const id = items[at].getAttribute("data-tip") as string;
        if (!opts.onEnter?.(id)) tip.tap(id, centre(items[at]));
        return;
      } else if (e.key === "Escape") {
        tip.clear();
        return;
      }
      if (next < 0) return;
      e.preventDefault();
      e.stopPropagation();
      const id = items[next].getAttribute("data-tip") as string;
      opts.onItem?.(id);
      tip.focus(id, centre(items[next]));
    },
    /** Hides the tooltip when focus leaves the chart. */
    onBlur: () => tip.leave(),
  };
}
