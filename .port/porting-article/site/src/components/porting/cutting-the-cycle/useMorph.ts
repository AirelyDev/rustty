"use client";

import { type RefObject, useCallback, useEffect, useState } from "react";
import { morphProgress } from "./geometry";
import type { View } from "./view";

/** MORPH_MS is the duration of the file-to-crate morph. */
const MORPH_MS = 1400;

/** What useMorph returns: whether a morph runs and how to start one. */
export type Morph = { active: boolean; start: (to: number) => void };

/**
 * Animates the 345 file dots from the circle into their crates' positions
 * (staggered by crate depth), writing cx/cy directly on the dot elements for
 * speed, then switches to stage `to`. Callers skip it under reduced motion.
 */
export default function useMorph(
  v: View,
  dotsRef: RefObject<SVGGElement | null>,
  setStage: (s: number) => void,
): Morph {
  const [to, setTo] = useState<number | null>(null);
  /** Starts a morph that ends on stage n. */
  const start = useCallback((n: number) => setTo(n), []);

  useEffect(() => {
    if (to === null) return;
    let raf = 0;
    let t0: number | null = null;
    /** Moves every dot to its position at animation time ts. */
    const step = (ts: number) => {
      if (t0 === null) t0 = ts;
      const p = Math.min(1, (ts - t0) / MORPH_MS);
      const dots = dotsRef.current?.children;
      if (dots) {
        v.lay.files.forEach((f, i) => {
          const q = morphProgress(p, f.depth, v.lay.maxDepth);
          const el = dots[i];
          el?.setAttribute("cx", (f.x + (f.home.x - f.x) * q).toFixed(1));
          el?.setAttribute("cy", (f.y + (f.home.y - f.y) * q).toFixed(1));
        });
      }
      if (p < 1) raf = requestAnimationFrame(step);
      else {
        setStage(to);
        setTo(null);
      }
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [to, v, dotsRef, setStage]);

  return { active: to !== null, start };
}
