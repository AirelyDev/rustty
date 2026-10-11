import {
  type KeyboardEvent,
  type MouseEvent,
  type RefObject,
  useMemo,
  useState,
} from "react";
import type { ChartTip } from "../ui/useChartTip";
import Cumulative from "./Cumulative";
import Lanes from "./Lanes";
import {
  dayTicks,
  GEO,
  type Group,
  laneTotals,
  type MsData,
  placeBubbles,
  rowsOf,
} from "./model";
import s from "./MemorySafety.module.css";

/** Props for the swimlane chart. */
type SwimlaneProps = {
  data: MsData;
  group: Group;
  filter: readonly [string, string] | null;
  selected: number | null;
  hoverRule: number | null;
  tip: ChartTip;
  /** Id linking the svg to the tooltip element. */
  tipId: string;
  /** Selects an event for the detail panel. */
  onSelect: (i: number) => void;
  /** Sets the hovered rule flag. */
  onRule: (i: number | null) => void;
  svgRef: RefObject<SVGSVGElement | null>;
};

/**
 * The swimlane timeline and the cumulative curves in one scrollable svg.
 * Hover shows an event's details, a click or tap pins them and selects the
 * event for the detail panel; the plot is a keyboard widget (arrow keys step
 * through events in date order, Enter pins, Escape closes).
 */
export default function Swimlane(p: SwimlaneProps) {
  const { data, group, tip } = p;
  const [cursor, setCursor] = useState<number | null>(null);
  const placed = useMemo(() => placeBubbles(data.events, group), [data, group]);
  const totals = useMemo(() => laneTotals(data.events, group), [data, group]);
  const days = useMemo(() => dayTicks(), []);
  const rows = rowsOf(group).length;
  const lanesBottom = GEO.top + rows * GEO.rowH;
  const cumTop = lanesBottom + 30;
  const height = cumTop + GEO.cumH + 22;

  // Keyboard order: events by date (the data is date-ordered), then index.
  const order = useMemo(() => data.events.map((_, i) => i), [data]);

  /** Returns the viewport position of the centre of event i's bubble. */
  const anchorOf = (i: number) => {
    const box = p.svgRef.current?.getBoundingClientRect();
    const b = placed.find((q) => q.i === i);
    if (!box || !b) return { x: 0, y: 0 };
    const k = box.width / GEO.W;
    const ri = rowsOf(group).findIndex(([key]) => key === b.row);
    return {
      x: box.left + b.x * k,
      y: box.top + (GEO.top + ri * GEO.rowH + GEO.rowH / 2 + b.dy) * k,
    };
  };

  /** Moves the keyboard cursor to the n-th event and shows its details. */
  const focusTo = (n: number) => {
    const i = order[Math.max(0, Math.min(order.length - 1, n))];
    setCursor(i);
    tip.focus(`e${i}`, anchorOf(i));
  };

  /** Keyboard: arrows step through events, Enter/Space pins and selects, Escape closes. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    if (e.target !== e.currentTarget) return;
    const pos = cursor === null ? -1 : order.indexOf(cursor);
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(pos + 1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(Math.max(0, pos - 1));
    } else if ((e.key === "Enter" || e.key === " ") && cursor !== null) {
      e.preventDefault();
      tip.tap(`e${cursor}`, anchorOf(cursor));
      p.onSelect(cursor);
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  /** Keyboard on a rule flag: Escape closes its tip. */
  const onFlagKey = (_i: number, e: KeyboardEvent) => {
    if (e.key === "Escape") tip.clear();
  };

  const at = (e: MouseEvent) => ({ x: e.clientX, y: e.clientY });

  return (
    <>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the full timeline →
      </p>
      <div className={s.scroll}>
        <svg
          ref={p.svgRef}
          className={s.swim}
          viewBox={`0 0 ${GEO.W} ${height}`}
          role="application"
          aria-roledescription="timeline"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the plot handles arrows, Enter and Escape itself
          tabIndex={0}
          aria-label="Swimlane timeline of port memory-safety bugs, one bubble per event, with the cumulative defect count below. Use the arrow keys to step through events, Enter to pin the details and open the event panel, Escape to close."
          aria-describedby={p.tipId}
          onKeyDown={onKey}
          onFocus={(e) => {
            if (
              cursor === null &&
              e.target === e.currentTarget &&
              e.currentTarget.matches(":focus-visible")
            ) {
              focusTo(0);
            }
          }}
          onBlur={(e) => {
            if (e.target !== e.currentTarget) return;
            setCursor(null);
            tip.leave();
          }}
        >
          <Lanes
            data={data}
            group={group}
            placed={placed}
            filter={p.filter}
            selected={p.selected}
            cursor={cursor}
            hoverRule={p.hoverRule}
            totals={totals}
            lanesBottom={cumTop + GEO.cumH}
            onBubbleMove={(i, e) => tip.hover(`e${i}`, at(e))}
            onBubbleLeave={() => tip.leave()}
            onBubbleClick={(i, e) => {
              setCursor(null);
              tip.tap(`e${i}`, at(e));
              p.onSelect(i);
            }}
            onFlagMove={(i, e) => tip.hover(`r${i}`, at(e))}
            onFlagEnter={(i) => p.onRule(i)}
            onFlagFocus={(i, el) => {
              const r = el.getBoundingClientRect();
              p.onRule(i);
              tip.focus(`r${i}`, {
                x: r.left + r.width / 2,
                y: r.top + r.height / 2,
              });
            }}
            onFlagLeave={() => {
              p.onRule(null);
              tip.leave();
            }}
            onFlagKey={onFlagKey}
          />
          <Cumulative events={data.events} days={days} top={cumTop} />
        </svg>
      </div>
    </>
  );
}
