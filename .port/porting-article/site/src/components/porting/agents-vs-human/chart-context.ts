import type { Rows } from "./geometry";
import type { AvhData, DayTotals, Metric } from "./model";

/** Everything a main-SVG sub-view needs to draw itself for the current view state. */
export type ChartCtx = {
  data: AvhData;
  totals: DayTotals;
  rows: Rows;
  metric: Metric;
  /** Replay position (last revealed day), or null when not replaying. */
  rep: number | null;
  /** Human era outlined on the grid (era card hover), or null. */
  hl: number | null;
  /** Unique SVG id prefix for this widget instance's patterns. */
  ids: string;
  /** Whether day i is inside the range and already revealed by the replay. */
  vis: (i: number) => boolean;
};

/** Returns the visibility predicate for range [d0, d1] and replay position rep. */
export function visibility(d0: number, d1: number, rep: number | null) {
  return (i: number) => i >= d0 && i <= d1 && (rep === null || i <= rep);
}
