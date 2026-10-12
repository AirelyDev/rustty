"use client";

import {
  type KeyboardEvent,
  type MouseEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { useInView } from "react-intersection-observer";
import { PALETTE } from "../lib/palette";
import { type ReplayState, replayStart, replayStep } from "../lib/replay";
import ReplayControls from "../ui/ReplayControls";
import {
  type DayStats,
  eventsAt,
  fmt,
  type GridOrder,
  gridHit,
  gridLabels,
  gridLayout,
  gridPaths,
  modAtSquare,
  moduleOutline,
  type ReplayEvent,
  replayDelay,
  type SquareClass,
  squareAt,
  type TcData,
  type Tile,
} from "./model";
import {
  GridLegend,
  GridStats,
  GridNote,
  ModuleTable,
  ModuleTip,
  Sparkline,
} from "./GridTable";
import s from "./TestContract.module.css";
import useChartTip from "../ui/useChartTip";

/** SQUARE_FILL maps each square colour class to its fill (alternate shades per module). */
const SQUARE_FILL: Record<SquareClass, string> = {
  future0: "#1d201f",
  future1: "#1a1d1c",
  open0: PALETTE.upstream,
  open1: "#4fb3a4",
  openNew: "#b8e9e0",
  cov0: PALETTE.rust,
  cov1: "#c96d2e",
  covNew: PALETTE.gold,
};

/** LABEL_BAND is the height in px above the squares reserved for module labels. */
const LABEL_BAND = 14;

/** DEFAULT_WIDTH is the grid box width assumed before the first measurement (desktop card). */
const DEFAULT_WIDTH = 958;

/**
 * The replay panel: one square per mapped Zig test, coloured by whether its
 * module's Rust count reached it on the selected day, with live counters, a
 * day scrubber over the covered-share sparkline, the event line, the legend
 * and three insight tiles. Auto-plays once when scrolled into view (reduced
 * motion: stays on the final day).
 */
export default function GridPanel({
  data,
  order,
  stats,
  events,
  tiles,
  reduced,
}: {
  data: TcData;
  order: GridOrder;
  stats: DayStats[];
  events: ReplayEvent[];
  tiles: Tile[];
  reduced: boolean;
}) {
  const last = data.dates.length - 1;
  const [replay, setReplay] = useState<ReplayState>({
    index: last,
    playing: false,
  });
  const day = replay.index;
  const st = stats[day];

  const boxRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const tip = useChartTip(boxRef);
  const tipId = `tc${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-grid`;

  // Track the box width; the grid's cell size and rows follow it.
  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // Auto-play once when the panel scrolls into view; reduced motion keeps the final day.
  const { ref: viewRef, inView } = useInView({
    threshold: 0.35,
    triggerOnce: true,
  });
  useEffect(() => {
    if (!inView) return;
    const start = replayStart(reduced, last);
    setReplay({ ...start, playing: !reduced });
  }, [inView, reduced, last]);

  // Advance one day per tick: slow for the first four weeks, then fast.
  useEffect(() => {
    if (!replay.playing) return;
    const t = setTimeout(
      () => setReplay((r) => replayStep(r, last)),
      replayDelay(replay.index),
    );
    return () => clearTimeout(t);
  }, [replay, last]);

  const layout = useMemo(
    () => gridLayout(order.total, Math.max(120, width)),
    [order.total, width],
  );
  const paths = useMemo(
    () => gridPaths(data, order, layout, day),
    [data, order, layout, day],
  );
  const labels = useMemo(() => gridLabels(order, layout), [order, layout]);

  const hovered = tip.shown ? Number(tip.shown.slice(1)) : -1;
  const [focusMod, setFocusMod] = useState<number | null>(null);

  /** Returns the module under a pointer event, or -1. */
  const modAt = (e: MouseEvent<SVGSVGElement>): number => {
    const r = e.currentTarget.getBoundingClientRect();
    const idx = gridHit(
      layout,
      e.clientX - r.left,
      e.clientY - r.top - LABEL_BAND,
    );
    return idx < 0 ? -1 : modAtSquare(order, idx);
  };

  /** Returns the viewport anchor of module mi (its middle square). */
  const modAnchor = (mi: number) => {
    const box = svgRef.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    const o = order.mods[mi];
    const [x, y] = squareAt(layout, o.start + Math.floor(o.zmax / 2));
    return { x: box.left + x, y: box.top + LABEL_BAND + y };
  };

  /** Moves the keyboard cursor to module mi and shows its tooltip. */
  const focusTo = (mi: number) => {
    setFocusMod(mi);
    tip.focus(`m${mi}`, modAnchor(mi));
  };

  /** Keyboard: arrows step through modules, Home/End jump, Enter/Space pins, Escape clears. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const n = order.mods.length;
    const cur = focusMod ?? 0;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(Math.min(n - 1, cur + 1));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(Math.max(0, cur - 1));
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      focusTo(e.key === "Home" ? 0 : n - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      setFocusMod(cur);
      tip.tap(`m${cur}`, modAnchor(cur));
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  const k = st.pin;
  const recent = eventsAt(events, day);
  const o = hovered >= 0 ? order.mods[hovered] : null;
  const label = `${data.dates[day]} · day ${day + 1} · ${data.revs[day]}`;

  return (
    <div ref={viewRef} className={s.panel}>
      <GridStats data={data} st={st} day={day} />
      <div ref={boxRef} className={s.gridBox}>
        <svg
          ref={svgRef}
          className={s.gridSvg}
          width={layout.width}
          height={layout.height + LABEL_BAND}
          viewBox={`0 0 ${layout.width} ${layout.height + LABEL_BAND}`}
          role="application"
          aria-roledescription="square grid"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the grid handles arrows, Enter and Escape itself
          tabIndex={0}
          aria-label={`Grid of ${fmt(order.total)} squares, one per mapped Zig test, grouped by module, coloured by whether a Rust counterpart was counted on ${data.dates[day]}. Use the arrow keys to step through modules, Enter to pin the details, Escape to close them.`}
          aria-describedby={tipId}
          onPointerMove={(e) => {
            if (e.pointerType !== "mouse") return;
            const mi = modAt(e);
            if (mi < 0) tip.leave();
            else tip.hover(`m${mi}`, { x: e.clientX, y: e.clientY });
          }}
          onPointerLeave={(e) => {
            if (e.pointerType === "mouse") tip.leave();
          }}
          onClick={(e) => {
            const mi = modAt(e);
            if (mi >= 0) tip.tap(`m${mi}`, { x: e.clientX, y: e.clientY });
          }}
          onKeyDown={onKey}
          onFocus={(e) => {
            if (focusMod === null && e.currentTarget.matches(":focus-visible"))
              focusTo(0);
          }}
          onBlur={() => {
            setFocusMod(null);
            tip.leave();
          }}
        >
          {labels.map(([x, name]) => (
            <text key={name} x={x} y={9} className={s.gridLabel}>
              {name}
            </text>
          ))}
          <g transform={`translate(0 ${LABEL_BAND})`}>
            {(Object.keys(SQUARE_FILL) as SquareClass[]).map((c) =>
              paths[c] ? (
                <path key={c} d={paths[c]} fill={SQUARE_FILL[c]} />
              ) : null,
            )}
            {hovered >= 0 ? (
              <path
                d={moduleOutline(order, layout, hovered)}
                fill="none"
                stroke={PALETTE.gray8}
                strokeWidth={1}
              />
            ) : null}
          </g>
        </svg>
        <div
          ref={tip.tipRef}
          id={tipId}
          role="tooltip"
          className={s.tip}
          hidden={!o}
          style={{ left: tip.pos.left, top: tip.pos.top }}
        >
          {o ? <ModuleTip o={o} k={k} day={day} dates={data.dates} /> : null}
        </div>
      </div>
      <Sparkline data={data} stats={stats} day={day} />
      <ReplayControls
        index={day}
        lastIndex={last}
        playing={replay.playing}
        onPlay={() =>
          setReplay((r) =>
            reduced
              ? replayStart(true, last)
              : // Resume from the paused day; start over only from the end.
                { index: r.index >= last ? 0 : r.index, playing: true },
          )
        }
        onPause={() => setReplay((r) => ({ ...r, playing: false }))}
        onSeek={(i) => setReplay({ index: i, playing: false })}
        label={label}
      />
      <div className={s.events}>
        {recent.length
          ? recent.map((e, i) => (
              <span key={`${e.day}-${e.lead}`}>
                {i ? " · " : ""}
                {e.lead ? <em>{e.lead}</em> : null}
                {e.text}
              </span>
            ))
          : " "}
      </div>
      <GridLegend />
      <div className={s.insights}>
        {tiles.map((t) => (
          <div key={t.value} className={s.ins}>
            <b>{t.value}</b>
            {t.text}
          </div>
        ))}
      </div>
      <ModuleTable order={order} dates={data.dates} />
      <GridNote total={order.total} />
    </div>
  );
}
