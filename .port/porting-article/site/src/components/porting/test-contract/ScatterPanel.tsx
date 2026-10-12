"use client";

import {
  type KeyboardEvent,
  type MouseEvent,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { PALETTE } from "../lib/palette";
import { type Doubled, fmt, type Point, TIERS } from "./model";
import DoubledNotes from "./DoubledNotes";
import ScatterAxes, { H, W, X, Y } from "./ScatterAxes";
import s from "./TestContract.module.css";
import useChartTip from "../ui/useChartTip";

/** TEST_COUNTS_URL is the permalink of the per-module test-count check at the snapshot commit. */
const TEST_COUNTS_URL =
  "https://github.com/AirelyDev/rustty/blob/58aa62a81e7db5f8762b0158acc661dafc31f230/.port/tools/gates/check_test_counts.py";

/** TIER_COLORS are the scatter tier colours, in TIERS order (adjacent pairs checked for colour-blind separation in the mockup). */
export const TIER_COLORS: readonly string[] = [
  PALETTE.rust,
  PALETTE.blue,
  PALETTE.gold,
  PALETTE.purple,
  PALETTE.gray4,
  PALETTE.yellow,
];

/** LABELLED are the modules labelled directly on the plot. */
const LABELLED = [
  "terminal",
  "vt_c",
  "lib_vt",
  "xev_file",
  "core_surface",
  "termio_exec",
  "cli_list_themes",
  "config_struct",
  "pagelist",
];
/** HIT_R2 is the squared nearest-point hover radius in viewBox units. */
const HIT_R2 = 196;

/**
 * The parity scatter: Zig tests vs Rust tests per module on the last day on
 * log(1 + n) axes, with the y = x and y = 2x references, the empty
 * dropped-test zone, tier chips that toggle points, nearest-point
 * hover/tap/keyboard details and the notes on the ≥2× modules.
 */
export default function ScatterPanel({
  points,
  two,
  dates,
}: {
  points: Point[];
  two: Doubled;
  dates: string[];
}) {
  const [on, setOn] = useState<boolean[]>(() => TIERS.map(() => true));
  const boxRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const tip = useChartTip(boxRef);
  const tipId = `tc${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-sc`;
  const [focusPt, setFocusPt] = useState<number | null>(null);

  // Keyboard order: visible points left to right, then bottom to top.
  const keyOrder = useMemo(
    () =>
      points
        .map((p, i) => [p, i] as const)
        .filter(([p]) => on[p.tier])
        .sort(([a], [b]) => a.z - b.z || a.r - b.r)
        .map(([, i]) => i),
    [points, on],
  );

  const active = tip.shown ? Number(tip.shown.slice(1)) : -1;
  const p = active >= 0 ? points[active] : null;

  /** Returns the nearest visible point to a mouse event, or -1. */
  const nearest = (e: MouseEvent<SVGSVGElement>): number => {
    const r = e.currentTarget.getBoundingClientRect();
    const sx = ((e.clientX - r.left) * W) / r.width;
    const sy = ((e.clientY - r.top) * H) / r.height;
    let best = -1;
    let bd = HIT_R2;
    points.forEach((q, i) => {
      if (!on[q.tier]) return;
      const dx = X(q.z) - sx;
      const dy = Y(q.r) - sy;
      const dd = dx * dx + dy * dy;
      if (dd < bd) {
        bd = dd;
        best = i;
      }
    });
    return best;
  };

  /** Returns the viewport position of point i. */
  const ptAnchor = (i: number) => {
    const box = svgRef.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    const k = box.width / W;
    return {
      x: box.left + X(points[i].z) * k,
      y: box.top + Y(points[i].r) * k,
    };
  };

  /** Moves the keyboard cursor to the n-th point in keyboard order. */
  const focusTo = (n: number) => {
    if (!keyOrder.length) return;
    const i = keyOrder[Math.max(0, Math.min(keyOrder.length - 1, n))];
    setFocusPt(i);
    tip.focus(`p${i}`, ptAnchor(i));
  };

  /** Keyboard: arrows step through visible points, Enter/Space pins, Escape clears. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const pos = focusPt === null ? -1 : keyOrder.indexOf(focusPt);
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(pos + 1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(Math.max(0, pos - 1));
    } else if ((e.key === "Enter" || e.key === " ") && focusPt !== null) {
      e.preventDefault();
      tip.tap(`p${focusPt}`, ptAnchor(focusPt));
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  const ratio = p
    ? p.z
      ? `${(p.r / p.z).toFixed(2)}×`
      : "Zig has 0 (helper/seam tests only)"
    : "";
  const rustSum = points.reduce((a, q) => a + q.r, 0);
  const zigSum = points.reduce((a, q) => a + q.z, 0);
  const atOrAbove = points.filter((q) => q.r >= q.z).length;

  return (
    <div className={s.subPanel}>
      <div className={s.subHead}>
        <a className={`${s.subPill} ${s.subPillLink}`} href={TEST_COUNTS_URL}>
          check_test_counts.py @ HEAD
        </a>
        <span className={s.subLabel}>Every module at or above</span>
      </div>
      <div className={s.big}>
        {atOrAbove} / {points.length}
      </div>
      <div className={s.sub}>
        modules with Rust tests ≥ Zig tests · Zig {fmt(zigSum)} vs Rust{" "}
        {fmt(rustSum)} (per-row sum)
      </div>
      <div className={s.chips}>
        {TIERS.map(([name], i) => (
          <button
            key={name}
            type="button"
            className={s.chip}
            aria-pressed={on[i]}
            onClick={() => {
              tip.clear();
              setOn((v) => v.map((x, j) => (j === i ? !x : x)));
            }}
          >
            <i style={{ background: TIER_COLORS[i] }} />
            {name} {points.filter((q) => q.tier === i).length}
          </button>
        ))}
      </div>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the full plot →
      </p>
      <div ref={boxRef} className={s.plotBox}>
        <div className={s.plotScroll}>
          <svg
            ref={svgRef}
            className={s.plotSvg}
            viewBox={`0 0 ${W} ${H}`}
            role="application"
            aria-roledescription="scatter plot"
            // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the plot handles arrows, Enter and Escape itself
            tabIndex={0}
            aria-label="Log-log scatter of Zig tests versus Rust tests per module. Use the arrow keys to step through modules, Enter to pin the details, Escape to close them."
            aria-describedby={tipId}
            onMouseMove={(e) => {
              const i = nearest(e);
              if (i < 0) tip.leave();
              else tip.hover(`p${i}`, { x: e.clientX, y: e.clientY });
            }}
            onMouseLeave={() => tip.leave()}
            onClick={(e) => {
              const i = nearest(e);
              if (i >= 0) tip.tap(`p${i}`, { x: e.clientX, y: e.clientY });
            }}
            onKeyDown={onKey}
            onFocus={(e) => {
              if (focusPt === null && e.currentTarget.matches(":focus-visible"))
                focusTo(0);
            }}
            onBlur={() => {
              setFocusPt(null);
              tip.leave();
            }}
          >
            <ScatterAxes dropped={points.filter((q) => q.r < q.z).length} />
            <g>
              {points.map((q, i) =>
                on[q.tier] ? (
                  <circle
                    key={q.mod.m}
                    cx={X(q.z).toFixed(1)}
                    cy={Y(q.r).toFixed(1)}
                    r={i === active ? 6.5 : 4}
                    fill={TIER_COLORS[q.tier]}
                    fillOpacity={0.85}
                    stroke={PALETTE.card}
                    strokeWidth={1.5}
                  />
                ) : null,
              )}
            </g>
            {points
              .filter((q) => LABELLED.includes(q.mod.m))
              .map((q) => {
                const x = X(q.z);
                const end = x > W - 90;
                return (
                  <text
                    key={q.mod.m}
                    x={x + (end ? -7 : 7)}
                    y={Y(q.r) + 3}
                    textAnchor={end ? "end" : "start"}
                    fill={PALETTE.gray6}
                  >
                    {q.mod.m}
                  </text>
                );
              })}
          </svg>
        </div>
        <div
          ref={tip.tipRef}
          id={tipId}
          role="tooltip"
          className={s.tip}
          hidden={!p}
          style={{ left: tip.pos.left, top: tip.pos.top }}
        >
          {p ? (
            <>
              <b>{p.mod.m}</b>
              <br />
              <span className={s.k}>
                {p.mod.c} · {TIERS[p.tier][0]}
              </span>
              <br />
              Zig {p.z} · Rust <b>{p.r}</b> · {ratio}
              <br />
              <span className={s.k}>first at/above Zig:</span>{" "}
              {p.mod.first < dates.length
                ? dates[p.mod.first]
                : p.z
                  ? "—"
                  : "always"}
            </>
          ) : null}
        </div>
      </div>
      <DoubledNotes points={points} two={two} />
    </div>
  );
}
