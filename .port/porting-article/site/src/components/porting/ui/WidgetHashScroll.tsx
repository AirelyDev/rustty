"use client";

import { useEffect } from "react";

/** PHONE_TOC_QUERY matches the widths where the sticky contents bar shows (see PortingPage.module.css). */
const PHONE_TOC_QUERY = "(max-width: 1276px)";

/** TOC_BAR_HEIGHT is the contents bar's height in px (--toc-bar-height). */
const TOC_BAR_HEIGHT = 44;

/** GAP is the space in px kept between the sticky bars and the target. */
const GAP = 8;

/**
 * WIDGET_SCROLL_EVENT is the window event (a CustomEvent whose detail is an
 * element) a widget dispatches when a link names something inside it (a
 * session card): the pending re-alignment then aligns that element instead
 * of the widget's heading.
 */
export const WIDGET_SCROLL_EVENT = "porting-widget-scroll";

/** SETTLE_MS is how long after the jump the target is re-aligned while widgets above it mount. */
const SETTLE_MS = 2500;

/** Returns the height in px covered by the sticky site header and, where shown, the contents bar. */
function stickyOffset(): number {
  const header =
    Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue(
        "--header-height",
      ),
    ) || 72;
  const bar = window.matchMedia(PHONE_TOC_QUERY).matches ? TOC_BAR_HEIGHT : 0;
  return header + bar + GAP;
}

/**
 * Makes filtered widget links land: a fragment "#<slug>?…" (e.g.
 * "#every-prompt?from=3&to=9") names no element, and the widget body only
 * mounts near the viewport. On load and on hashchange this scrolls to the
 * widget [data-porting-widget="<slug>"] (else to the element with id
 * <slug>, the section heading; a heading can sit far above its widget on a
 * phone, out of the widget's mount range), below the sticky header and contents bar
 * (instantly under reduced motion), so the widget mounts and reads its filter
 * from the fragment. It re-aligns once while lazily mounted widgets above the
 * target settle, unless the reader scrolls or types first; a widget can
 * name a more precise element to align (WIDGET_SCROLL_EVENT). Only slugs in
 * `slugs` are handled.
 */
export default function WidgetHashScroll({ slugs }: { slugs: string[] }) {
  const key = slugs.join(" ");
  useEffect(() => {
    const allowed = key.split(" ");
    let timers: number[] = [];
    // An element inside the widget that the widget asked to align instead of its heading.
    let inner: HTMLElement | null = null;
    /** Cancels the pending re-alignment once the reader scrolls or types. */
    const cancel = () => {
      for (const t of timers) window.clearTimeout(t);
      timers = [];
    };
    /** Scrolls to the widget named by the current fragment, if it is one of ours. */
    const go = () => {
      const m = /^#([a-z0-9-]+)\?/.exec(window.location.hash);
      if (!m || !allowed.includes(m[1])) return;
      const slug = m[1];
      /** Returns the scroll target: an element the widget named, else the widget, else the heading. */
      const target = () =>
        (inner?.isConnected ? inner : null) ??
        document.querySelector<HTMLElement>(
          `[data-porting-widget="${slug}"]`,
        ) ??
        document.getElementById(slug);
      const reduced = window.matchMedia(
        "(prefers-reduced-motion: reduce)",
      ).matches;
      /** Aligns the target's top under the sticky bars. */
      const align = (smooth: boolean) => {
        const el = target();
        if (!el) return;
        const top =
          el.getBoundingClientRect().top + window.scrollY - stickyOffset();
        window.scrollTo({ top, behavior: smooth ? "smooth" : "auto" });
      };
      cancel();
      inner = null;
      align(!reduced);
      timers.push(window.setTimeout(() => align(false), SETTLE_MS));
    };
    go();
    window.addEventListener("hashchange", go);
    window.addEventListener("wheel", cancel, { passive: true });
    window.addEventListener("touchstart", cancel, { passive: true });
    window.addEventListener("keydown", cancel);
    /** Adopts the element a widget names as the alignment target. */
    const onInner = (e: Event) => {
      const el = (e as CustomEvent<unknown>).detail;
      if (el instanceof HTMLElement) inner = el;
    };
    window.addEventListener(WIDGET_SCROLL_EVENT, onInner);
    return () => {
      window.removeEventListener("hashchange", go);
      window.removeEventListener("wheel", cancel);
      window.removeEventListener(WIDGET_SCROLL_EVENT, onInner);
      window.removeEventListener("touchstart", cancel);
      window.removeEventListener("keydown", cancel);
      cancel();
    };
  }, [key]);
  return null;
}
