"use client";

import { type ComponentType, type LazyExoticComponent, lazy } from "react";
import s from "./ui.module.css";

/**
 * LOADERS maps a widget slug to the dynamic import of its client Body
 * (`@/components/porting/<slug>`). Each widget task adds its entry here.
 */
const LOADERS: Record<string, () => Promise<{ default: ComponentType }>> = {
  "three-reviewers": () => import("@/components/porting/three-reviewers"),
  "agents-vs-human": () => import("@/components/porting/agents-vs-human"),
  "test-contract": () => import("@/components/porting/test-contract"),
  "cutting-the-cycle": () => import("@/components/porting/cutting-the-cycle"),
  "translation-lens": () => import("@/components/porting/translation-lens"),
  "moving-target": () => import("@/components/porting/moving-target"),
  "memory-safety": () => import("@/components/porting/memory-safety"),
  "defect-map": () => import("@/components/porting/defect-map"),
  "verification-matrix": () =>
    import("@/components/porting/verification-matrix"),
  "perf-replay": () => import("@/components/porting/perf-replay"),
  cost: () => import("@/components/porting/cost"),
  "every-prompt": () => import("@/components/porting/every-prompt"),
};

/** LAZY caches one React.lazy component per slug so its identity is stable across renders. */
const LAZY = new Map<string, LazyExoticComponent<ComponentType>>();

/** Returns the cached lazy component for slug, creating it from its loader on first use. */
function lazyBody(
  slug: string,
  load: () => Promise<{ default: ComponentType }>,
): LazyExoticComponent<ComponentType> {
  let Body = LAZY.get(slug);
  if (!Body) {
    Body = lazy(load);
    LAZY.set(slug, Body);
  }
  return Body;
}

/**
 * Renders the interactive body of widget slug (suspends while its chunk
 * loads), or a "data pending" placeholder when the widget is not built yet.
 */
export default function WidgetMount({ slug }: { slug: string }) {
  const load = LOADERS[slug];
  if (!load) {
    return <div className={s.pending}>Interactive view: data pending</div>;
  }
  const Body = lazyBody(slug, load);
  return <Body />;
}
