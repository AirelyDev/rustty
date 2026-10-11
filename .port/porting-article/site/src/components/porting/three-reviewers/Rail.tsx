"use client";

import { type KeyboardEvent, useRef, useState } from "react";
import type { ChartTip } from "../ui/useChartTip";
import {
  bugEvent,
  eventBug,
  LANE_COLORS,
  laneTotals,
  MONTHS,
  RAIL,
  railDots,
  railX,
} from "./model";
import s from "./ThreeReviewers.module.css";
import type { TrData } from "./types";

/** LETTERS are the replayed bugs' labels. */
const LETTERS = ["A", "B", "C", "D"];

/** Props for Rail. */
type RailProps = {
  data: TrData;
  /** Selected bug index. */
  bug: number;
  onBug: (i: number) => void;
  tip: ChartTip;
};

/**
 * The context rail: all events by date (x) and detecting tool (lane),
 * dot area ∝ defects, filled = production code, hollow = test, harness or
 * bench, lane totals on the right. The selected bug gets a white ring and a
 * leader line; A–D markers and replayable dots select a bug. Hover shows an
 * event's details; tap or Enter pins them. Arrow keys step through events.
 */
export default function Rail({ data, bug, onBug, tip }: RailProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const dots = railDots(data);
  const totals = laneTotals(data);
  const yAxis = RAIL.y0 + RAIL.laneH * data.lanes.length;
  const yb = yAxis + 10;
  const sel = dots[bugEvent(data, data.bugs[bug])];

  /** Returns the viewport anchor of dot k. */
  const anchor = (k: number) => {
    const r = svgRef.current
      ?.querySelector(`[data-k="${k}"]`)
      ?.getBoundingClientRect();
    return r
      ? { x: r.left + r.width / 2, y: r.top + r.height / 2 }
      : { x: 0, y: 0 };
  };
  /** Moves the keyboard cursor to event k (clamped) and shows its tooltip. */
  const go = (k: number) => {
    const j = Math.max(0, Math.min(dots.length - 1, k));
    setCursor(j);
    tip.focus(`ev:${j}`, anchor(j));
  };
  /** Pins event k's tooltip and selects its bug when it is one of the replayed four. */
  const activate = (k: number, at: { x: number; y: number }) => {
    tip.tap(`ev:${k}`, at);
    const b = eventBug(data, dots[k].e);
    if (b >= 0) onBug(b);
  };
  /** Arrow keys step through events, Enter/Space pins (and selects a replayed bug), Escape closes. */
  const onKeyDown = (e: KeyboardEvent<SVGSVGElement>) => {
    const at = cursor ?? -1;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      go(at + 1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      go(Math.max(0, at - 1));
    } else if ((e.key === "Enter" || e.key === " ") && cursor !== null) {
      e.preventDefault();
      activate(cursor, anchor(cursor));
    } else if (e.key === "Escape") {
      tip.clear();
      setCursor(null);
    }
  };

  return (
    <>
      <p className={s.scrollHint}>
        Swipe the rail sideways to see every lane and date.
      </p>
      <div className={s.railWrap}>
        <svg
          ref={svgRef}
          className={s.rail}
          viewBox={`0 0 ${RAIL.width} ${RAIL.height}`}
          role="application"
          aria-roledescription="timeline"
          aria-label={`Timeline of ${data.timeline.length} port memory-safety events by detecting tool. Arrow keys step through events, Enter shows the details and selects a replayed bug, Escape closes.`}
          // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the rail handles arrows, Enter and Escape itself
          tabIndex={0}
          onKeyDown={onKeyDown}
          onBlur={() => {
            setCursor(null);
            if (!tip.pinned) tip.leave();
          }}
          onPointerLeave={(e) => {
            if (e.pointerType === "mouse") tip.leave();
          }}
        >
          {MONTHS.map(([d, m]) => (
            <g key={d}>
              <line
                x1={railX(d)}
                x2={railX(d)}
                y1={16}
                y2={yAxis}
                stroke="#1d201e"
              />
              <text x={railX(d) + 3} y={13} fontSize="10" fill="#858a84">
                {m}
              </text>
            </g>
          ))}
          {totals.map((l) => (
            <g key={l.key}>
              <line
                x1={RAIL.x0}
                x2={RAIL.x1}
                y1={l.y}
                y2={l.y}
                stroke="#1a1d1b"
              />
              <circle cx={10} cy={l.y} r={4} fill={LANE_COLORS[l.key]} />
              <text x={20} y={l.y + 3.5} fontSize="10.5" fill="#9aa09a">
                {l.label}
              </text>
              <text
                x={958}
                y={l.y + 3.5}
                fontSize="11"
                fill="#c1c8c0"
                textAnchor="end"
              >
                {l.events} ·{" "}
                <tspan fill="#f4f1ed" fontWeight="700">
                  {l.defects}
                </tspan>
              </text>
            </g>
          ))}
          {dots.map((d) => (
            // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; keyboard access is on the role=application svg
            <circle
              key={d.k}
              data-k={d.k}
              className={s.dot}
              cx={d.cx}
              cy={d.cy}
              r={d.r}
              fill={d.e.prod ? d.color : "#0d0f0e"}
              fillOpacity={d.e.prod ? 0.85 : 1}
              stroke={cursor === d.k ? "#f4f1ed" : d.color}
              strokeWidth={cursor === d.k ? 2 : 1.4}
              onPointerMove={(e) => {
                if (e.pointerType === "mouse")
                  tip.hover(`ev:${d.k}`, { x: e.clientX, y: e.clientY });
              }}
              onClick={(e) => activate(d.k, { x: e.clientX, y: e.clientY })}
            />
          ))}
          {sel ? (
            <g pointerEvents="none">
              <circle
                cx={sel.cx}
                cy={sel.cy}
                r={sel.r + 5}
                fill="none"
                stroke="#f4f1ed"
                strokeWidth={1.5}
              />
              <line
                x1={sel.cx}
                x2={sel.cx}
                y1={sel.cy - sel.r - 5}
                y2={18}
                stroke="#f4f1ed"
                strokeDasharray="2 2"
              />
            </g>
          ) : null}
          <text x={20} y={yb + 12} fontSize="10" fill="#858a84">
            replayable below (A–D):
          </text>
          <text
            x={958}
            y={yb + 12}
            fontSize="9.5"
            fill="#858a84"
            textAnchor="end"
          >
            events · defects
          </text>
          {data.bugs.map((b, i) => {
            const x = railX(data.timeline[bugEvent(data, b)].d);
            const on = i === bug;
            return (
              // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; the bug cards above are the keyboard path
              <g key={b.key} className={s.marker} onClick={() => onBug(i)}>
                <line x1={x} x2={x} y1={yb - 4} y2={yb + 2} stroke="#4a4f4b" />
                <text
                  x={x}
                  y={yb + 12 + (i % 2) * 11}
                  fontSize="9.5"
                  fill={on ? "#f4f1ed" : "#858a84"}
                  fontWeight={on ? 700 : 400}
                  textAnchor="middle"
                >
                  {LETTERS[i]}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </>
  );
}
