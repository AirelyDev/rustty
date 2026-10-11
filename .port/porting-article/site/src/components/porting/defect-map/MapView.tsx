import {
  type KeyboardEvent,
  type MouseEvent,
  useMemo,
  useRef,
  useState,
} from "react";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { fmtLines, GEO, type MapLayout, type Mark, shapePath } from "./layout";
import {
  CLASS_BY,
  type DmData,
  type Filter,
  METHOD_BY,
  visible,
} from "./model";
import s from "./DefectMap.module.css";

/** Props for the treemap. */
type MapViewProps = {
  data: DmData;
  layout: MapLayout;
  filter: Filter;
  /** Index of the defect open in the detail panel. */
  selected: number | null;
  /** Top tile highlighted from the density list. */
  dir: string | null;
  tip: ChartTip;
  /** Id linking the svg to the tooltip element. */
  tipId: string;
  /** Opens defect i in the detail panel. */
  onSelect: (i: number) => void;
};

/**
 * The treemap of Ghostty's Zig source with one mark per defect (colour =
 * class, glyph = discovery method, hollow = latent or test weakness, dimmed
 * when filtered out) and the dependency strip below it. Hover shows a mark's
 * or tile's details, a click or tap pins them and opens the mark in the detail
 * panel; the map is a keyboard widget (arrows step through the visible marks,
 * Enter opens one, Escape closes the tooltip).
 */
export default function MapView(p: MapViewProps) {
  const { data, layout, filter, tip } = p;
  const svgRef = useRef<SVGSVGElement>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const order = useMemo(
    () => layout.marks.filter((m) => visible(data.defs[m.i], filter)),
    [layout, data, filter],
  );

  /** Returns the viewport position of mark m. */
  const anchorOf = (m: Mark) => {
    const box = svgRef.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    const k = box.width / GEO.W;
    return { x: box.left + m.x * k, y: box.top + m.y * k };
  };

  /** Moves the keyboard cursor to the n-th visible mark and shows its tooltip. */
  const focusTo = (n: number) => {
    if (!order.length) return;
    const m = order[Math.max(0, Math.min(order.length - 1, n))];
    setCursor(m.i);
    tip.focus(`m${m.i}`, anchorOf(m));
  };

  /** Keyboard: arrows step through visible marks, Enter/Space opens one, Escape closes the tooltip. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const pos = order.findIndex((m) => m.i === cursor);
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(pos + 1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(Math.max(0, pos - 1));
    } else if ((e.key === "Enter" || e.key === " ") && pos >= 0) {
      e.preventDefault();
      tip.tap(`m${order[pos].i}`, anchorOf(order[pos]));
      p.onSelect(order[pos].i);
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  /** The viewport point of a pointer event. */
  const at = (e: MouseEvent) => ({ x: e.clientX, y: e.clientY });
  const cursorMark = layout.marks.find((m) => m.i === cursor);

  return (
    <>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the whole map →
      </p>
      <div className={s.scroll}>
        <svg
          ref={svgRef}
          className={s.map}
          viewBox={`0 0 ${GEO.W} ${GEO.H}`}
          role="application"
          aria-roledescription="treemap"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the map handles arrows, Enter and Escape itself
          tabIndex={0}
          aria-label="Treemap of Ghostty's Zig source by directory, one mark per recorded defect. Use the arrow keys to step through the marks shown, Enter to open one in the detail panel, Escape to close the tooltip."
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
          onBlur={() => {
            setCursor(null);
            tip.leave();
          }}
        >
          {layout.tops.map((t) => (
            <g key={t.n}>
              <rect
                className={p.dir === t.n ? s.topOn : s.top}
                x={t.x + 0.5}
                y={t.y + 0.5}
                width={Math.max(0, t.w - 1)}
                height={Math.max(0, t.h - 1)}
                rx="3"
              />
              {t.label ? (
                <text
                  x={t.x + 5}
                  y={t.y + 11}
                  fontSize="10"
                  fill={PALETTE.gray5}
                >
                  {t.label} <tspan fill={PALETTE.gray3}>{fmtLines(t.v0)}</tspan>
                </text>
              ) : null}
              {t.kids.map((k) => (
                // biome-ignore lint/a11y/noStaticElementInteractions: pointer-only tile tooltip; the same facts are in the mark tooltips and the density list
                <rect
                  key={k.n}
                  className={s.leaf}
                  x={k.x + 0.5}
                  y={k.y + 0.5}
                  width={Math.max(0, k.w - 1)}
                  height={Math.max(0, k.h - 1)}
                  rx="2"
                  onMouseMove={(e) => tip.hover(`l${k.n}`, at(e))}
                  onMouseLeave={() => tip.leave()}
                  onClick={(e) => tip.tap(`l${k.n}`, at(e))}
                />
              ))}
              {t.kids.map((k) =>
                k.label ? (
                  <text
                    key={`t${k.n}`}
                    x={k.label.x}
                    y={k.label.y}
                    fontSize="9"
                    fill={k.label.dim ? PALETTE.gray3 : PALETTE.gray4}
                  >
                    {k.label.text}
                  </text>
                ) : null,
              )}
            </g>
          ))}
          <text x="0" y={GEO.stripY - 6} fontSize="10" fill={PALETTE.gray5}>
            Outside Ghostty's Zig tree: pinned dependencies and non-Zig build
            files (tiles sized by defect count, not code size)
          </text>
          {layout.deps.map((d) => (
            <g key={d.n}>
              {/* biome-ignore lint/a11y/noStaticElementInteractions: pointer-only tile tooltip; the counts are in the tile label */}
              <rect
                className={s.dep}
                x={d.x + 0.5}
                y={d.y + 0.5}
                width={Math.max(0, d.w - 1)}
                height={d.h - 1}
                rx="3"
                onMouseMove={(e) => tip.hover(`l${d.n}`, at(e))}
                onMouseLeave={() => tip.leave()}
                onClick={(e) => tip.tap(`l${d.n}`, at(e))}
              />
              <text
                x={d.x + 5}
                y={d.y + 11}
                fontSize="9.5"
                fill={PALETTE.gray4}
              >
                {d.label} {d.count}
              </text>
            </g>
          ))}
          {layout.marks.map((m) => {
            const d = data.defs[m.i];
            const c = CLASS_BY[d.c];
            const colour = PALETTE[c.role];
            const hollow = "hollow" in c;
            const sel = p.selected === m.i;
            return (
              <g key={m.i}>
                {/* biome-ignore lint/a11y/noStaticElementInteractions: marks are reached with the map's arrow keys (role="application") */}
                <path
                  className={s.mark}
                  d={shapePath(METHOD_BY[d.m].shape, m.x, m.y, m.r)}
                  fill={hollow ? PALETTE.card : colour}
                  stroke={sel ? PALETTE.gray8 : hollow ? colour : PALETTE.card}
                  strokeWidth={hollow ? 1.4 : sel ? 2 : 1}
                  strokeDasharray={"dash" in c ? "2 1.5" : undefined}
                  opacity={visible(d, filter) ? 1 : 0.12}
                  onMouseMove={(e) => tip.hover(`m${m.i}`, at(e))}
                  onMouseLeave={() => tip.leave()}
                  onClick={(e) => {
                    setCursor(null);
                    tip.tap(`m${m.i}`, at(e));
                    p.onSelect(m.i);
                  }}
                />
                {"dot" in c ? (
                  <circle
                    cx={m.x}
                    cy={m.y}
                    r={Math.max(1.2, m.r * 0.35)}
                    fill={PALETTE.card}
                    opacity={visible(d, filter) ? 1 : 0.12}
                    pointerEvents="none"
                  />
                ) : null}
              </g>
            );
          })}
          {cursorMark ? (
            <circle
              cx={cursorMark.x}
              cy={cursorMark.y}
              r={cursorMark.r + 4}
              fill="none"
              stroke={PALETTE.upstream}
              strokeWidth="1.5"
              pointerEvents="none"
            />
          ) : null}
        </svg>
      </div>
    </>
  );
}
