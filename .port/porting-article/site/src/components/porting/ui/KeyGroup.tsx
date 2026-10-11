"use client";

import { type ReactNode, useRef } from "react";
import s from "./ui.module.css";
import { useRovingFocus } from "./useRovingFocus";

/**
 * Groups a chart's tooltip marks into one Tab stop: arrow keys, Home and End
 * move between the marks (see useRovingFocus). The group is an absolutely
 * placed layer covering its positioned parent, so marks placed in percent of
 * the chart box keep their positions; the layer itself ignores the pointer.
 * label names the group for assistive technology and should mention the keys.
 */
export default function KeyGroup({
  label,
  cols,
  children,
}: {
  label: string;
  cols?: number;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useRovingFocus(ref, { cols });
  return (
    // biome-ignore lint/a11y/useSemanticElements: a group of chart marks, not form fields (fieldset)
    <div ref={ref} role="group" aria-label={label} className={s.keyLayer}>
      {children}
    </div>
  );
}
