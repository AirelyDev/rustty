import { type KeyboardEvent, type MouseEvent, useMemo, useState } from "react";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import {
  GEO,
  moveName,
  type MtData,
  nearestAdvance,
  timeAt,
  tipAt,
  tm,
  xOf,
  yA,
  yB,
} from "./model";
import TimeLayers from "./TimeLayers";
import s from "./MovingTarget.module.css";

/** SIX_HOURS is the tooltip time step in ms (ids quantise the hovered time to it). */
const SIX_HOURS = 6 * 3600e3;

/** Returns the tooltip id for epoch ms t. */
export function timeTipId(t: number): string {
  return `t${Math.round(t / SIX_HOURS)}`;
}

/** Returns the epoch ms a time tooltip id stands for. */
export function timeOfTipId(id: string): number {
  return Number(id.slice(1)) * SIX_HOURS;
}

/** Props for the size-over-time chart. */
type TimeChartProps = {
  data: MtData;
  /** Selected move index. */
  sel: number;
  onSelect: (i: number) => void;
  tip: ChartTip;
  /** Id linking the svg to the tooltip element. */
  tipId: string;
};

/**
 * The size-over-time chart: two panels sharing one x axis, dotted verticals
 * at each reference move (the selected one solid white) and a hover crosshair with a
 * tooltip. Clicking or tapping selects the nearest move and pins the tooltip;
 * the plot is a keyboard widget (left and right step through the moves, Enter
 * pins, Escape closes) and the #n labels are buttons.
 */
export default function TimeChart({
  data,
  sel,
  onSelect,
  tip,
  tipId,
}: TimeChartProps) {
  const [cursor, setCursor] = useState<number | null>(null);
  const [kb, setKb] = useState<number | null>(null);
  const [svg, setSvg] = useState<SVGSVGElement | null>(null);
  const { W, L, R, A, B, H } = GEO;
  const tops = useMemo(() => data.adv.map((a) => tm(a.adv)), [data]);

  /** Returns the svg x of a pointer event. */
  const svgX = (e: MouseEvent) => {
    const box = (svg as SVGSVGElement).getBoundingClientRect();
    return ((e.clientX - box.left) * W) / box.width;
  };

  /** Returns the viewport position of the plot point at epoch ms t. */
  const anchorAt = (t: number) => {
    const box = (svg as SVGSVGElement).getBoundingClientRect();
    const k = box.width / W;
    return { x: box.left + xOf(t) * k, y: box.top + (A.t + 60) * k };
  };

  /** Selects move i, puts the crosshair on it and shows its details. */
  const goTo = (i: number) => {
    setKb(i);
    onSelect(i);
    setCursor(tops[i]);
    tip.focus(timeTipId(tops[i]), anchorAt(tops[i]));
  };

  /** Keyboard: arrows step through the moves, Enter/Space pins, Escape closes. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    if (e.target !== e.currentTarget) return;
    const last = data.adv.length - 1;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      goTo(kb === null ? sel : Math.min(last, kb + 1));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      goTo(kb === null ? sel : Math.max(0, kb - 1));
    } else if ((e.key === "Enter" || e.key === " ") && kb !== null) {
      e.preventDefault();
      tip.tap(timeTipId(tops[kb]), anchorAt(tops[kb]));
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  const at = cursor === null ? null : tipAt(data, cursor);
  return (
    <>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the full timeline →
      </p>
      <div className={s.scroll}>
        <svg
          ref={setSvg}
          className={s.time}
          viewBox={`0 0 ${W} ${H}`}
          role="application"
          aria-roledescription="chart"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the plot handles arrows, Enter and Escape itself
          tabIndex={0}
          aria-label={`Ghostty and Rustty size over time with the ${data.adv.length} reference moves. Use the left and right arrow keys to step through the moves, Enter to pin the details, Escape to close.`}
          aria-describedby={tipId}
          onKeyDown={onKey}
          onBlur={(e) => {
            if (e.target !== e.currentTarget) return;
            setKb(null);
            setCursor(null);
            tip.leave();
          }}
        >
          <TimeLayers d={data} />
          {data.adv.map((a, i) => {
            const x = xOf(tops[i]);
            const on = i === sel;
            const crowded = i > 0 && x - xOf(tops[i - 1]) < 20;
            return (
              <g key={a.n}>
                <line
                  x1={x}
                  x2={x}
                  y1={A.t}
                  y2={B.t + B.h}
                  stroke={on ? PALETTE.gray8 : "#3a3e3b"}
                  strokeWidth={on ? 1.2 : 1}
                  strokeDasharray={on ? undefined : "2 3"}
                />
                {/* biome-ignore lint/a11y/useSemanticElements: an svg label acting as a button */}
                <g
                  className={s.moveLabel}
                  role="button"
                  tabIndex={0}
                  aria-label={`Select ${moveName(a)}`}
                  aria-pressed={on}
                  onClick={() => onSelect(i)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onSelect(i);
                    }
                  }}
                >
                  <text
                    x={crowded ? x + 3 : x}
                    y={crowded ? A.t + 22 : A.t + 10}
                    textAnchor={crowded ? "start" : "middle"}
                    fill={on ? PALETTE.gray8 : PALETTE.gray5}
                  >
                    {`#${a.n}`}
                  </text>
                </g>
              </g>
            );
          })}
          {at && cursor !== null ? (
            <g pointerEvents="none">
              <line
                x1={xOf(cursor)}
                x2={xOf(cursor)}
                y1={A.t}
                y2={B.t + B.h}
                stroke={PALETTE.gray4}
              />
              <circle
                cx={xOf(cursor)}
                cy={yA(at.main)}
                r={3.5}
                fill={PALETTE.upstream}
              />
              <circle
                cx={xOf(cursor)}
                cy={yA(at.pin)}
                r={3.5}
                fill={PALETTE.card}
                stroke={PALETTE.upstream}
                strokeWidth={2}
              />
              {at.rs !== null ? (
                <circle
                  cx={xOf(cursor)}
                  cy={yB(at.rs)}
                  r={3.5}
                  fill={PALETTE.rust}
                />
              ) : null}
            </g>
          ) : null}
          {/* biome-ignore lint/a11y/noStaticElementInteractions: pointer hit area; the plot itself is the keyboard widget */}
          <rect
            x={L}
            y={A.t}
            width={W - L - R}
            height={B.t + B.h - A.t + 8}
            fill="transparent"
            onMouseMove={(e) => {
              const t = timeAt(svgX(e));
              setCursor(t);
              tip.hover(timeTipId(t), { x: e.clientX, y: e.clientY });
            }}
            onMouseLeave={() => {
              setCursor(null);
              tip.leave();
            }}
            onClick={(e) => {
              const t = timeAt(svgX(e));
              onSelect(nearestAdvance(data, t));
              setCursor(t);
              tip.tap(timeTipId(t), { x: e.clientX, y: e.clientY });
            }}
          />
        </svg>
      </div>
    </>
  );
}
