"use client";

import {
  type RefObject,
  useCallback,
  useEffect,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  TOOLTIP_IDLE,
  tooltipReducer,
  visibleTooltip,
} from "../lib/tooltip-state";

/** TIP_GAP is the distance in px between the anchor point and the tooltip. */
const TIP_GAP = 14;

/** A viewport point the tooltip is placed next to. */
export type TipAnchor = { x: number; y: number };

/** What useChartTip returns: the visible id, its position and the event dispatchers. */
export type ChartTip = {
  /** Id of the visible tooltip target, or null. */
  shown: string | null;
  /** Whether the visible tooltip is pinned (tap, click or Enter). */
  pinned: boolean;
  /** Ref for the tooltip element (measured to keep it inside the box). */
  tipRef: RefObject<HTMLDivElement | null>;
  /** Tooltip position relative to the box. */
  pos: { left: number; top: number };
  /** Shows id transiently (mouse hover) at a viewport point. */
  hover: (id: string, at: TipAnchor) => void;
  /** Shows id for keyboard focus at a viewport point (moves a pinned tooltip). */
  focus: (id: string, at: TipAnchor) => void;
  /** Pins or unpins id at a viewport point (tap, click, Enter). */
  tap: (id: string, at: TipAnchor) => void;
  /** Hides the transient tooltip (mouse leave, blur). */
  leave: () => void;
  /** Clears everything, pinned included. */
  clear: () => void;
};

/**
 * Floating chart tooltip for one panel: hover shows it, a tap or Enter pins
 * it (so touch users can read it), Escape or a press outside the box clears
 * it. Positions it next to its anchor, flipped to stay inside boxRef.
 */
export default function useChartTip(
  boxRef: RefObject<HTMLElement | null>,
): ChartTip {
  const [state, dispatch] = useReducer(tooltipReducer, TOOLTIP_IDLE);
  const anchors = useRef(new Map<string, TipAnchor>());
  const tipRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: 0, top: 0 });
  const shown = visibleTooltip(state);
  const pinned = state.pinned !== null;

  const hover = useCallback((id: string, at: TipAnchor) => {
    anchors.current.set(id, at);
    dispatch({ type: "enter", id });
  }, []);
  const tap = useCallback((id: string, at: TipAnchor) => {
    anchors.current.set(id, at);
    dispatch({ type: "tap", id });
  }, []);
  const leave = useCallback(() => dispatch({ type: "leave" }), []);
  const clear = useCallback(() => dispatch({ type: "escape" }), []);
  const focus = useCallback(
    (id: string, at: TipAnchor) => {
      anchors.current.set(id, at);
      if (pinned) {
        dispatch({ type: "escape" });
        dispatch({ type: "tap", id });
      } else {
        dispatch({ type: "focus", id });
      }
    },
    [pinned],
  );

  // While any tooltip is shown (pinned, focused or hovered), Escape clears it.
  useEffect(() => {
    if (!shown) return;
    /** Clears the visible tooltip on Escape. */
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dispatch({ type: "escape" });
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [shown]);

  // While pinned, a press outside the box clears it.
  useEffect(() => {
    if (!pinned) return;
    /** Clears the pinned tooltip on a press outside the box. */
    const onDown = (e: PointerEvent) => {
      if (!boxRef.current?.contains(e.target as Node)) {
        dispatch({ type: "escape" });
      }
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [pinned, boxRef]);

  // Place the tooltip next to its anchor, flipped to stay inside the box.
  useLayoutEffect(() => {
    if (!shown || !boxRef.current || !tipRef.current) return;
    const at = anchors.current.get(shown);
    if (!at) return;
    const box = boxRef.current.getBoundingClientRect();
    const tw = tipRef.current.offsetWidth;
    const th = tipRef.current.offsetHeight;
    let left = at.x - box.left + TIP_GAP;
    let top = at.y - box.top + TIP_GAP;
    if (left + tw > box.width) left = at.x - box.left - tw - TIP_GAP;
    if (left < 0) left = Math.max(0, Math.min(4, box.width - tw));
    if (top + th > box.height + 40) top = at.y - box.top - th - TIP_GAP;
    setPos({ left, top: Math.max(-th, top) });
  }, [shown, state, boxRef]);

  return { shown, pinned, tipRef, pos, hover, focus, tap, leave, clear };
}
