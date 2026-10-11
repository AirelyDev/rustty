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
import {
  type DayStats,
  fmt,
  pinLabel,
  type SyncTable as SyncTableData,
  stepPath,
  type TcData,
} from "./model";
import MovingAxes, { H, M, PH, PW, W, Y } from "./MovingAxes";
import SyncTable from "./SyncTable";
import s from "./TestContract.module.css";
import useChartTip from "../ui/useChartTip";

/**
 * The moving contract: step lines of the Zig test blocks in src/, the mapped
 * subset and the daily Rust #[test] count, numbered sync ticks, a crosshair
 * tooltip (hover, tap, arrow keys) and the per-sync table.
 */
export default function MovingPanel({
  data,
  stats,
  table,
}: {
  data: TcData;
  stats: DayStats[];
  table: SyncTableData;
}) {
  const nd = data.dates.length;
  /** Maps a day index to x. */
  const X = (i: number) => M.l + (i / (nd - 1)) * PW;
  const boxRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const tip = useChartTip(boxRef);
  const tipId = `tc${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-mv`;
  const [focusDay, setFocusDay] = useState<number | null>(null);

  const series = useMemo(() => {
    const src = stats.map((d) => data.pins[d.pin].src);
    const mapped = stats.map((d) => data.pins[d.pin].mapped);
    return { src, mapped, rust: data.rust.all, met: stats.map((d) => d.met) };
  }, [data, stats]);
  const paths = useMemo(() => {
    /** Maps a day index to x. */
    const x = (i: number) => M.l + (i / (nd - 1)) * PW;
    return {
      mapped: stepPath(series.mapped, x, Y),
      src: stepPath(series.src, x, Y),
      rust: stepPath(series.rust, x, Y),
    };
  }, [series, nd]);

  const day = tip.shown ? Number(tip.shown.slice(1)) : -1;
  const last = nd - 1;
  const lastPin = data.pins[data.pins.length - 1];

  /** Returns the day under a mouse event. */
  const dayAt = (e: MouseEvent<SVGSVGElement>): number => {
    const r = e.currentTarget.getBoundingClientRect();
    const sx = ((e.clientX - r.left) * W) / r.width;
    const i = Math.round(((sx - M.l) / PW) * (nd - 1));
    return Math.max(0, Math.min(nd - 1, i));
  };

  /** Returns the viewport anchor of day i (on the Rust line). */
  const dayAnchor = (i: number) => {
    const box = svgRef.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    const k = box.width / W;
    return { x: box.left + X(i) * k, y: box.top + Y(series.rust[i]) * k };
  };

  /** Moves the keyboard cursor to day i. */
  const focusTo = (i: number) => {
    const d = Math.max(0, Math.min(nd - 1, i));
    setFocusDay(d);
    tip.focus(`d${d}`, dayAnchor(d));
  };

  /** Keyboard: arrows move a day (Shift: a week), Home/End jump, Enter/Space pins, Escape clears. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const cur = focusDay ?? 0;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(cur + (e.shiftKey ? 7 : 1));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(cur - (e.shiftKey ? 7 : 1));
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      focusTo(e.key === "Home" ? 0 : nd - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      tip.tap(`d${cur}`, dayAnchor(cur));
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  const k = day >= 0 ? stats[day].pin : 0;

  return (
    <div className={s.subPanel}>
      <div className={s.subHead}>
        <span className={s.subPill}>
          ghostty git grep · {data.pins.length} reference commits
        </span>
        <span className={s.subLabel}>The contract moved</span>
      </div>
      <div className={s.big}>
        {fmt(data.pins[0].src)} → {fmt(lastPin.src)}
      </div>
      <div className={s.sub}>
        Zig test blocks in Ghostty <code>src/</code> across{" "}
        {data.pins.length - 1} upstream syncs (+
        {fmt(lastPin.src - data.pins[0].src)})
      </div>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the full chart →
      </p>
      <div ref={boxRef} className={s.plotBox}>
        <div className={s.plotScroll}>
          <svg
            ref={svgRef}
            className={s.plotSvg}
            viewBox={`0 0 ${W} ${H}`}
            role="application"
            aria-roledescription="line chart"
            // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
            tabIndex={0}
            aria-label="Step lines of the Zig test count per reference commit and the Rust test count per day. Use the arrow keys to move between days (Shift for a week), Enter to pin the details, Escape to close them."
            aria-describedby={tipId}
            onMouseMove={(e) => {
              const i = dayAt(e);
              tip.hover(`d${i}`, { x: e.clientX, y: e.clientY });
            }}
            onMouseLeave={() => tip.leave()}
            onClick={(e) => {
              const i = dayAt(e);
              tip.tap(`d${i}`, { x: e.clientX, y: e.clientY });
            }}
            onKeyDown={onKey}
            onFocus={(e) => {
              if (
                focusDay === null &&
                e.currentTarget.matches(":focus-visible")
              )
                focusTo(last);
            }}
            onBlur={() => {
              setFocusDay(null);
              tip.leave();
            }}
          >
            <MovingAxes data={data} />
            <path
              d={paths.mapped}
              fill="none"
              stroke={PALETTE.upstream}
              strokeWidth={1.5}
              strokeDasharray="5 3"
              strokeLinejoin="round"
            />
            <path
              d={paths.src}
              fill="none"
              stroke={PALETTE.upstream}
              strokeWidth={2}
              strokeLinejoin="round"
            />
            <path
              d={paths.rust}
              fill="none"
              stroke={PALETTE.rust}
              strokeWidth={2}
              strokeLinejoin="round"
            />
            <text
              x={X(last) - 2}
              y={Y(series.rust[last]) - 6}
              textAnchor="end"
              fill={PALETTE.rust}
            >
              Rust {fmt(series.rust[last])}
            </text>
            <text
              x={X(last) - 2}
              y={Y(lastPin.src) - 6}
              textAnchor="end"
              fill={PALETTE.upstream}
            >
              Zig {fmt(lastPin.src)}
            </text>
            <text
              x={X(last) - 2}
              y={Y(lastPin.mapped) + 12}
              textAnchor="end"
              fill={PALETTE.upstream}
              opacity={0.7}
            >
              mapped {fmt(lastPin.mapped)}
            </text>
            {day >= 0 ? (
              <line
                x1={X(day)}
                x2={X(day)}
                y1={M.t}
                y2={M.t + PH}
                stroke={PALETTE.gray6}
                opacity={0.5}
              />
            ) : null}
          </svg>
        </div>
        <div
          ref={tip.tipRef}
          id={tipId}
          role="tooltip"
          className={s.tip}
          hidden={day < 0}
          style={{ left: tip.pos.left, top: tip.pos.top }}
        >
          {day >= 0 ? (
            <>
              <b>{data.dates[day]}</b>{" "}
              <span className={s.k}>
                {pinLabel(k)} · {data.pins[k].sha}
              </span>
              <br />
              <span style={{ color: PALETTE.rust }}>■</span> Rust #[test]{" "}
              <b>{fmt(series.rust[day])}</b>
              <br />
              <span style={{ color: PALETTE.upstream }}>■</span> Zig test blocks{" "}
              <b>{fmt(series.src[day])}</b>
              <br />
              <span style={{ color: PALETTE.upstream }}>┅</span> mapped Zig{" "}
              {fmt(series.mapped[day])} · covered {fmt(series.met[day])}
              <br />
              <span className={s.k}>Rust / Zig:</span>{" "}
              {(series.rust[day] / series.src[day]).toFixed(2)}×
            </>
          ) : null}
        </div>
      </div>
      <SyncTable dates={data.dates} table={table} />
    </div>
  );
}
