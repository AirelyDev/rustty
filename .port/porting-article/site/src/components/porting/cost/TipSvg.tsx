import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
  useRef,
} from "react";
import type { ChartTip } from "../ui/useChartTip";
import s from "./Cost.module.css";

/** Props for a chart whose marks carry tooltips. */
type TipSvgProps = {
  tip: ChartTip;
  /** Id of the tooltip element (aria-describedby). */
  tipId: string;
  /** What the chart shows; the keyboard instructions are appended. */
  label: string;
  viewBox: string;
  className?: string;
  children: ReactNode;
};

/** Returns the element carrying a tooltip id at or above target, inside root. */
function tipTarget(target: EventTarget, root: Element | null) {
  const el = (target as Element).closest?.("[data-tip]");
  return el && root?.contains(el) ? el : null;
}

/** Returns the viewport centre of an element (the keyboard tooltip anchor). */
function centre(el: Element) {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/**
 * An SVG chart whose marks carry `data-tip="<id>"`. Mouse hover shows a
 * mark's tooltip, a tap or click pins it, and the chart is one tab stop:
 * the arrow keys step through the marks in drawing order (Home and End jump
 * to the ends), Enter or Space pins, Escape closes. Marks with
 * `data-nokey` repeat another mark's tooltip and are skipped by the keys.
 */
export default function TipSvg({
  tip,
  tipId,
  label,
  viewBox,
  className,
  children,
}: TipSvgProps) {
  const ref = useRef<SVGSVGElement>(null);
  const cur = useRef(-1);

  /** Returns the keyboard-reachable marks in drawing order. */
  const marks = () =>
    Array.from(
      ref.current?.querySelectorAll("[data-tip]:not([data-nokey])") ?? [],
    );
  /** Moves the keyboard highlight to el (or clears it). */
  const highlight = (el: Element | null) => {
    for (const m of ref.current?.querySelectorAll("[data-active]") ?? []) {
      m.removeAttribute("data-active");
    }
    el?.setAttribute("data-active", "true");
  };
  /** Shows the hovered mark's tooltip (mouse only; touch uses tap). */
  const onMove = (e: PointerEvent<SVGSVGElement>) => {
    if (e.pointerType !== "mouse") return;
    const el = tipTarget(e.target, ref.current);
    if (el)
      tip.hover(el.getAttribute("data-tip") ?? "", {
        x: e.clientX,
        y: e.clientY,
      });
    else tip.leave();
  };
  /** Pins (or unpins) the tapped mark's tooltip. */
  const onClick = (e: MouseEvent<SVGSVGElement>) => {
    const el = tipTarget(e.target, ref.current);
    if (el)
      tip.tap(el.getAttribute("data-tip") ?? "", {
        x: e.clientX,
        y: e.clientY,
      });
  };
  /** Steps through the marks with the arrow keys; Enter or Space pins; Escape closes. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const list = marks();
    if (!list.length) return;
    let i = cur.current;
    if (e.key === "ArrowRight" || e.key === "ArrowDown")
      i = Math.min(list.length - 1, i + 1);
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp")
      i = Math.max(0, i < 0 ? 0 : i - 1);
    else if (e.key === "Home") i = 0;
    else if (e.key === "End") i = list.length - 1;
    else if ((e.key === "Enter" || e.key === " ") && i >= 0) {
      e.preventDefault();
      tip.tap(list[i].getAttribute("data-tip") ?? "", centre(list[i]));
      return;
    } else if (e.key === "Escape") {
      tip.clear();
      return;
    } else return;
    e.preventDefault();
    cur.current = i;
    highlight(list[i]);
    list[i].scrollIntoView?.({ block: "nearest", inline: "nearest" });
    tip.focus(list[i].getAttribute("data-tip") ?? "", centre(list[i]));
  };

  return (
    <svg
      ref={ref}
      className={`${s.svg} ${className ?? ""}`}
      viewBox={viewBox}
      role="application"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
      tabIndex={0}
      aria-label={`${label}. Use the arrow keys to step through the marks, Enter to pin the details, Escape to close them.`}
      aria-describedby={tipId}
      onPointerMove={onMove}
      onPointerLeave={(e) => {
        if (e.pointerType === "mouse") tip.leave();
      }}
      onClick={onClick}
      onKeyDown={onKey}
      onBlur={() => {
        highlight(null);
        cur.current = -1;
        tip.leave();
      }}
    >
      {children}
    </svg>
  );
}
