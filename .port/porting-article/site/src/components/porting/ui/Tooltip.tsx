"use client";

import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useId,
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
import s from "./ui.module.css";

/** EDGE_GAP is the minimum distance in px kept between a tooltip and the viewport edge. */
const EDGE_GAP = 8;

/**
 * Wraps children in a tooltip trigger. Hover or keyboard focus shows content,
 * a tap or click pins it (a second tap unpins), and Escape or a press
 * elsewhere clears it. The bubble is a sibling of the trigger button (button
 * content is presentational, so a bubble inside it would lose its tooltip role
 * and be announced twice); it has role="tooltip" and is linked to the button
 * with aria-describedby. Links inside the bubble are reachable: a pinned
 * bubble takes pointer events, and keyboard focus moving from the button into
 * the bubble keeps it shown until focus leaves both.
 *
 * With boundary (optional), the bubble is placed with fixed positioning
 * instead: below the trigger, or above it when there is no room below,
 * and kept inside the boundary element's box (and the viewport). Being
 * fixed, it is not cut off by an ancestor that clips its overflow (a
 * scroll box, a collapsed text); it follows scrolling while shown.
 */
export default function Tooltip({
  id,
  content,
  children,
  boundary,
}: {
  id: string;
  content: ReactNode;
  children: ReactNode;
  /** The element whose box the bubble stays inside (fixed placement); omitted: placed above the trigger. */
  boundary?: () => HTMLElement | null;
}) {
  const [state, dispatch] = useReducer(tooltipReducer, TOOLTIP_IDLE);
  const [dx, setDx] = useState(0);
  const [fixedPos, setFixedPos] = useState<{
    left: number;
    top: number;
  } | null>(null);
  const tipId = useId();
  const wrapRef = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const visible = visibleTooltip(state) === id;
  const pinned = state.pinned === id;
  /** Clears the transient state unless focus moved to another element of this tooltip. */
  const blurUnlessInside = (next: EventTarget | null) => {
    if (!wrapRef.current?.contains(next as Node | null)) {
      dispatch({ type: "blur", id });
    }
  };

  // While shown, Escape anywhere and a press outside the trigger clear it.
  useEffect(() => {
    if (!visible) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dispatch({ type: "escape" });
    };
    const onDown = (e: PointerEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) {
        dispatch({ type: "escape" });
      }
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onDown);
    };
  }, [visible]);

  // With a boundary: place the bubble below (or above) the trigger inside
  // the boundary's box, and again whenever anything scrolls or resizes.
  useLayoutEffect(() => {
    if (!boundary || !visible) {
      setFixedPos(null);
      return;
    }
    /** Computes the bubble's fixed position from the trigger, bubble and boundary boxes. */
    const place = () => {
      const tip = tipRef.current;
      const anchor = wrapRef.current?.firstElementChild;
      if (!tip || !anchor) return;
      const a = anchor.getBoundingClientRect();
      const w = tip.offsetWidth;
      const h = tip.offsetHeight;
      const b = boundary()?.getBoundingClientRect();
      const vw = document.documentElement.clientWidth;
      const vh = window.innerHeight;
      const minX = Math.max(EDGE_GAP, (b?.left ?? 0) + EDGE_GAP);
      const maxX = Math.min(vw - EDGE_GAP, (b?.right ?? vw) - EDGE_GAP);
      const minY = Math.max(EDGE_GAP, (b?.top ?? 0) + EDGE_GAP);
      const maxY = Math.min(vh - EDGE_GAP, (b?.bottom ?? vh) - EDGE_GAP);
      let top = a.bottom + 6;
      if (top + h > maxY && a.top - 6 - h >= minY) top = a.top - 6 - h;
      const left = Math.min(
        Math.max(a.left + a.width / 2 - w / 2, minX),
        Math.max(minX, maxX - w),
      );
      setFixedPos({ left, top });
    };
    place();
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [visible, boundary]);

  // Nudge the bubble horizontally so it never overflows the viewport.
  useLayoutEffect(() => {
    if (boundary || !visible || !tipRef.current) {
      setDx(0);
      return;
    }
    const r = tipRef.current.getBoundingClientRect();
    const left = r.left - dx;
    const right = r.right - dx;
    const max = document.documentElement.clientWidth - EDGE_GAP;
    if (left < EDGE_GAP) setDx(EDGE_GAP - left);
    else if (right > max) setDx(max - right);
    else setDx(0);
  }, [visible, boundary]);

  const tipStyle = boundary
    ? ({
        position: "fixed",
        left: fixedPos?.left ?? 0,
        top: fixedPos?.top ?? 0,
        bottom: "auto",
        transform: "none",
        visibility: fixedPos ? undefined : "hidden",
      } as CSSProperties)
    : ({ "--tip-dx": `${dx}px` } as CSSProperties);

  return (
    <span ref={wrapRef} className={s.tipWrap}>
      <button
        type="button"
        className={s.tipAnchor}
        aria-describedby={tipId}
        onPointerEnter={(e) => {
          if (e.pointerType === "mouse") dispatch({ type: "enter", id });
        }}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") dispatch({ type: "leave", id });
        }}
        onFocus={() => dispatch({ type: "focus", id })}
        onBlur={(e) => blurUnlessInside(e.relatedTarget)}
        onClick={() => dispatch({ type: "tap", id })}
      >
        {children}
      </button>
      <span
        ref={tipRef}
        id={tipId}
        role="tooltip"
        className={s.tip}
        hidden={!visible}
        data-pinned={pinned ? "" : undefined}
        onBlur={(e) => blurUnlessInside(e.relatedTarget)}
        style={tipStyle}
      >
        {content}
      </span>
    </span>
  );
}
