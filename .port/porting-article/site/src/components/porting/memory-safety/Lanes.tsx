import type { KeyboardEvent, MouseEvent } from "react";
import { PALETTE } from "../lib/palette";
import {
  dateX,
  GEO,
  type Group,
  type MsData,
  type MsEvent,
  matches,
  type Placed,
  rowsOf,
  scopeKind,
  wrapLabel,
} from "./model";
import s from "./MemorySafety.module.css";

/** SCOPE_COLOURS are the bubble fills: production red, test blue, benchmark gray. */
export const SCOPE_COLOURS = {
  prod: PALETTE.fail,
  test: PALETTE.blue,
  bench: PALETTE.gray5,
} as const;

/** MONTHS are the gridline dates and their labels. */
const MONTHS = [
  ["2026-07-01", "Jul"],
  ["2026-08-01", "Aug"],
  ["2026-09-01", "Sep"],
  ["2026-10-01", "Oct"],
] as const;

/** Returns the fill colour of an event's bubble from its scope. */
export function scopeColour(e: MsEvent): string {
  return SCOPE_COLOURS[scopeKind(e.scope)];
}

/** Props for one bubble. */
type BubbleProps = {
  kind: "prod" | "test" | "bench";
  cx: number;
  cy: number;
  r: number;
  fill: string;
  dim: boolean;
  ring: boolean;
  onMouseMove: (e: MouseEvent) => void;
  onMouseLeave: () => void;
  onClick: (e: MouseEvent) => void;
};

/**
 * One event bubble. Scope is encoded by shape and outline as well as colour:
 * production is a solid circle, test-only a circle with a dashed outline,
 * benchmark a square of the same area.
 */
function Bubble({ kind, cx, cy, r, fill, dim, ring, ...on }: BubbleProps) {
  const common = {
    className: s.bubble,
    fill,
    fillOpacity: dim ? 0.12 : 0.85,
    stroke: ring
      ? PALETTE.gray8
      : kind === "test"
        ? PALETTE.gray8
        : PALETTE.card,
    strokeWidth: ring ? 2 : kind === "test" ? 1.1 : 1.2,
    strokeDasharray: kind === "test" && !ring ? "2 2" : undefined,
    strokeOpacity: dim ? 0.3 : 1,
    ...on,
  };
  if (kind === "bench") {
    const h = r * 0.89;
    return (
      <rect
        x={(cx - h).toFixed(1)}
        y={(cy - h).toFixed(1)}
        width={(2 * h).toFixed(1)}
        height={(2 * h).toFixed(1)}
        {...common}
      />
    );
  }
  return (
    <circle
      cx={cx.toFixed(1)}
      cy={cy.toFixed(1)}
      r={r.toFixed(1)}
      {...common}
    />
  );
}

/** Props for the swimlane layer. */
type LanesProps = {
  data: MsData;
  group: Group;
  placed: Placed[];
  /** Heat-table filter: [crate, class] or null. */
  filter: readonly [string, string] | null;
  /** Selected event index, or null. */
  selected: number | null;
  /** Keyboard cursor event index, or null. */
  cursor: number | null;
  /** Hovered rule index, or null. */
  hoverRule: number | null;
  /** Row totals. */
  totals: Record<string, number>;
  /** Y of the cumulative chart's top (end of the lanes). */
  lanesBottom: number;
  onBubbleMove: (i: number, e: MouseEvent) => void;
  onBubbleLeave: () => void;
  onBubbleClick: (i: number, e: MouseEvent) => void;
  onFlagMove: (i: number, e: MouseEvent) => void;
  onFlagEnter: (i: number) => void;
  onFlagFocus: (i: number, el: Element) => void;
  onFlagLeave: () => void;
  onFlagKey: (i: number, e: KeyboardEvent) => void;
};

/**
 * The swimlane layer: month gridlines, rule flags with their dashed guides, one
 * striped row per detector (or crate area) with its label and defect total, and
 * one bubble per event (area proportional to its defects, the count printed
 * for four or more).
 */
export default function Lanes(p: LanesProps) {
  const rows = rowsOf(p.group);
  const { top, rowH, L, R } = GEO;
  const bottom = top + rows.length * rowH;
  const ordered = [...p.placed].sort((a, b) => b.r - a.r);
  return (
    <g>
      {MONTHS.map(([d, name]) => {
        const x = dateX(d);
        return (
          <g key={d}>
            <line
              x1={x}
              x2={x}
              y1={top - 8}
              y2={p.lanesBottom}
              stroke="#191c1a"
            />
            <text x={x + 3} y={p.lanesBottom + 14}>
              {name}
            </text>
          </g>
        );
      })}
      {p.data.rules.map((r, i) => {
        const x = dateX(r.d);
        const on = p.hoverRule === i;
        const cy = top - 22 - (i === 4 ? 17 : 0);
        return (
          <g key={r.ref}>
            <line
              x1={x}
              x2={x}
              y1={top - 14}
              y2={bottom}
              stroke={on ? PALETTE.gray6 : "#3a3e3b"}
              strokeDasharray={on ? undefined : "2 3"}
            />
            {/* biome-ignore lint/a11y/useSemanticElements: an SVG flag cannot be a <button>; the rule list below is the keyboard route */}
            <g
              className={s.flag}
              role="button"
              tabIndex={0}
              aria-label={`Rule ${i + 1}, ${r.d}: ${r.t}`}
              onMouseEnter={() => p.onFlagEnter(i)}
              onMouseMove={(e) => p.onFlagMove(i, e)}
              onMouseLeave={p.onFlagLeave}
              onFocus={(e) => p.onFlagFocus(i, e.currentTarget)}
              onBlur={p.onFlagLeave}
              onKeyDown={(e) => p.onFlagKey(i, e)}
            >
              <circle
                cx={x}
                cy={cy}
                r={8}
                fill={PALETTE.card}
                stroke={on ? PALETTE.gray8 : PALETTE.gray6}
              />
              <text x={x} y={cy + 3} textAnchor="middle" fill={PALETTE.gray8}>
                {i + 1}
              </text>
            </g>
          </g>
        );
      })}
      {rows.map(([k, label], ri) => {
        const y = top + ri * rowH;
        const [l1, l2] = wrapLabel(label);
        return (
          <g key={k}>
            <rect
              x={L}
              y={y + 2}
              width={R - L}
              height={rowH - 4}
              fill={ri % 2 ? "#0e100f" : "#111312"}
              rx={4}
            />
            <text
              x={L - 8}
              y={y + rowH / 2 - (l2 ? 2 : -3)}
              textAnchor="end"
              fill={PALETTE.gray6}
            >
              {l1}
            </text>
            {l2 ? (
              <text x={L - 8} y={y + rowH / 2 + 10} textAnchor="end">
                {l2}
              </text>
            ) : null}
            <text x={R + 4} y={y + rowH / 2 + 3} fill={PALETTE.gray5}>
              {p.totals[k]}
            </text>
          </g>
        );
      })}
      {ordered.map((b) => {
        const e = p.data.events[b.i];
        const ri = rows.findIndex(([k]) => k === b.row);
        const cy = top + ri * rowH + rowH / 2 + b.dy;
        const dim = !matches(e, p.filter);
        const sel = p.selected === b.i;
        const cur = p.cursor === b.i;
        return (
          <g key={b.i}>
            <Bubble
              kind={scopeKind(e.scope)}
              cx={b.x}
              cy={cy}
              r={b.r}
              fill={scopeColour(e)}
              dim={dim}
              ring={sel || cur}
              onMouseMove={(ev) => p.onBubbleMove(b.i, ev)}
              onMouseLeave={p.onBubbleLeave}
              onClick={(ev) => p.onBubbleClick(b.i, ev)}
            />
            {e.n >= 4 && !dim ? (
              <text
                x={b.x.toFixed(1)}
                y={(cy + 3.5).toFixed(1)}
                textAnchor="middle"
                fill={PALETTE.card}
                fontWeight={700}
                pointerEvents="none"
              >
                {e.n}
              </text>
            ) : null}
          </g>
        );
      })}
    </g>
  );
}
