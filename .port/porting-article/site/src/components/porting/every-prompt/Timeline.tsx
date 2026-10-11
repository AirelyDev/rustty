"use client";

import {
  type KeyboardEvent,
  type PointerEvent,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import useChartTip from "../ui/useChartTip";
import s from "./EveryPrompt.module.css";
import {
  type DayCounts,
  dayDate,
  dayIndexOf,
  type EpData,
  type Filter,
  LANE_COLOURS,
  LANE_NAMES,
  LANE_SHORT,
  shortDate,
  type Tick,
} from "./model";

/** W and H are the timeline's viewBox size. */
export const W = 1000;
const H = 258;
/** LBL is the width of the lane-label column; PR the right padding. */
export const LBL = 112;
export const PR = 6;
/** PW is the plot width. */
export const PW = W - LBL - PR;
/** Y_PHASE, Y_PURGE and Y_LANES are the tops of the phase strip, purge strip and first lane. */
const Y_PHASE = 0;
const Y_PURGE = 20;
const Y_LANES = 40;
/** LANE_H is a lane's height and LANE_GAP the gap between lanes. */
const LANE_H = 46;
const LANE_GAP = 4;
/** Y_COMMITS and COMMITS_H place the commits-per-day bars under the lanes. */
const Y_COMMITS = Y_LANES + 3 * (LANE_H + LANE_GAP) + 6;
const COMMITS_H = 36;
/** Y_AXIS is the baseline of the date labels. */
const Y_AXIS = Y_COMMITS + COMMITS_H + 16;
/** LOG2_MAX is the log2 length drawn at full lane height (about 2,000 characters). */
const LOG2_MAX = 11;
/** PHASE_FILLS alternate the phase band fills. */
const PHASE_FILLS = ["#161917", "#1d211f", "#161917"];

/** Props of the timeline strip. */
type TimelineProps = {
  data: EpData;
  ticks: Tick[];
  /** Log row indices of the prompts passing the filter. */
  shown: Set<number>;
  filter: Filter;
  /** Per-day, per-lane counts of all prompts. */
  counts: DayCounts[];
  /** Sets the day range (brush or range inputs). */
  onRange: (from: number, to: number) => void;
};

/**
 * The timeline strip: one tick per prompt in three lanes (Mac Claude Code,
 * VM Claude Code, Codex) at its Pacific time of day within its day column,
 * height by length on a log scale; phase bands above, the purged-transcript
 * window hatched, commits per day beneath. Dragging with a mouse brushes a
 * day range; two range inputs do the same from the keyboard. A day tooltip
 * shows on hover, tap or keyboard focus (arrow keys).
 */
export default function Timeline({
  data,
  ticks,
  shown,
  filter,
  counts,
  onRange,
}: TimelineProps) {
  const nDays = data.commitsPerDay.length;
  const colW = PW / nDays;
  /** Maps a day index to the x of its column's left edge. */
  const X = (d: number) => LBL + d * colW;
  const purgeDay = dayIndexOf(data.window[0], data.purgedBefore);
  const maxCommits = Math.max(1, ...data.commitsPerDay);
  const boxRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const tip = useChartTip(boxRef);
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const tipId = `ep${uid}-tip`;
  const hatchId = `ep${uid}-hatch`;
  const [focusDay, setFocusDay] = useState<number | null>(null);
  const drag = useRef<{ start: number; moved: boolean } | null>(null);
  const suppressClick = useRef(false);
  const [preview, setPreview] = useState<[number, number] | null>(null);

  // The tick marks only change with the filter; drawn as one path per lane and state.
  const paths = useMemo(() => {
    const out = LANE_COLOURS.map(() => ({ on: "", off: "" }));
    const cw = PW / nDays;
    for (const t of ticks) {
      const x = LBL + t.day * cw + (t.minute / 1440) * cw;
      const base = Y_LANES + t.lane * (LANE_H + LANE_GAP) + LANE_H;
      const h = 3 + (Math.min(t.log2chars, LOG2_MAX) / LOG2_MAX) * (LANE_H - 7);
      const seg = `M${x.toFixed(1)} ${base}v${(-h).toFixed(1)}`;
      if (shown.has(t.i)) out[t.lane].on += seg;
      else out[t.lane].off += seg;
    }
    return out;
  }, [ticks, shown, nDays]);

  /** Returns the day under a pointer event's x. */
  const dayAt = (clientX: number): number => {
    const r = svgRef.current?.getBoundingClientRect();
    if (!r) return 0;
    const sx = ((clientX - r.left) * W) / r.width;
    return Math.max(0, Math.min(nDays - 1, Math.floor((sx - LBL) / colW)));
  };

  /** Returns the viewport anchor of day d (top of its column, right edge). */
  const dayAnchor = (d: number) => {
    const box = svgRef.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    const k = box.width / W;
    return { x: box.left + (X(d) + colW) * k, y: box.top + Y_LANES * k };
  };

  /** Moves the keyboard cursor to day d and shows its tooltip. */
  const focusTo = (d: number) => {
    const day = Math.max(0, Math.min(nDays - 1, d));
    setFocusDay(day);
    tip.focus(`d${day}`, dayAnchor(day));
  };

  /** Keyboard: arrows move a day (Shift: a week), Home/End jump, Enter/Space pins, Escape clears. */
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const cur = focusDay ?? filter.from;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      focusTo(cur + (e.shiftKey ? 7 : 1));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      focusTo(cur - (e.shiftKey ? 7 : 1));
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      focusTo(e.key === "Home" ? 0 : nDays - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      tip.tap(`d${cur}`, dayAnchor(cur));
    } else if (e.key === "Escape") {
      tip.clear();
    }
  };

  /** Starts a brush drag with a mouse or pen (touch scrolls and taps instead). */
  const onDown = (e: PointerEvent<SVGSVGElement>) => {
    if (e.pointerType === "touch" || e.button !== 0) return;
    drag.current = { start: dayAt(e.clientX), moved: false };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  /** Extends the brush while dragging; otherwise shows the hovered day. */
  const onMove = (e: PointerEvent<SVGSVGElement>) => {
    const d = dayAt(e.clientX);
    const g = drag.current;
    if (g) {
      if (d !== g.start) g.moved = true;
      if (g.moved) {
        const a = Math.min(g.start, d);
        const b = Math.max(g.start, d);
        // Re-render only when the previewed range changes.
        if (preview?.[0] !== a || preview?.[1] !== b) {
          setPreview([a, b]);
          tip.leave();
        }
        return;
      }
    }
    if (e.pointerType === "mouse")
      tip.hover(`d${d}`, { x: e.clientX, y: e.clientY });
  };

  /** Ends a drag: a moved drag sets the day range and swallows the click. */
  const onUp = (e: PointerEvent<SVGSVGElement>) => {
    const g = drag.current;
    drag.current = null;
    if (g?.moved) {
      const d = dayAt(e.clientX);
      suppressClick.current = true;
      setPreview(null);
      onRange(Math.min(g.start, d), Math.max(g.start, d));
    }
  };

  const brush = preview ?? [filter.from, filter.to];
  const whole = brush[0] === 0 && brush[1] === nDays - 1;
  const day = tip.shown ? Number(tip.shown.slice(1)) : -1;
  const start = data.window[0];
  const axisDays = [0, ...monthStarts(start, nDays), nDays - 1];

  return (
    <div className={s.timeline}>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for the whole timeline →
      </p>
      <div ref={boxRef} className={s.plotBox}>
        {/* At phone width the plot scrolls sideways; the box takes focus so
            arrow keys can scroll it (the plot itself uses arrows to brush). */}
        <section
          className={s.plotScroll}
          aria-label="Timeline, scrolls sideways"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be focusable to scroll by keyboard
          tabIndex={0}
        >
          <svg
            ref={svgRef}
            className={s.plotSvg}
            viewBox={`0 0 ${W} ${H}`}
            role="application"
            aria-roledescription="timeline"
            // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the timeline handles arrows, Enter and Escape itself
            tabIndex={0}
            aria-label={`One tick per prompt from ${shortDate(dayDate(start, 0))} to ${shortDate(dayDate(start, nDays - 1))}, in three lanes: ${LANE_NAMES.join(", ")}; commits per day beneath. Use the arrow keys to move between days (Shift for a week), Enter to pin the details, Escape to close them. The range inputs below the timeline narrow the days.`}
            aria-describedby={tipId}
            onPointerDown={onDown}
            onPointerMove={onMove}
            onPointerUp={onUp}
            onPointerCancel={() => {
              drag.current = null;
              setPreview(null);
            }}
            onPointerLeave={() => tip.leave()}
            onClick={(e) => {
              if (suppressClick.current) {
                suppressClick.current = false;
                return;
              }
              const d = dayAt(e.clientX);
              tip.tap(`d${d}`, { x: e.clientX, y: e.clientY });
            }}
            onKeyDown={onKey}
            onFocus={(e) => {
              if (
                focusDay === null &&
                e.currentTarget.matches(":focus-visible")
              ) {
                focusTo(filter.from);
              }
            }}
            onBlur={() => {
              setFocusDay(null);
              tip.leave();
            }}
          >
            <defs>
              <pattern
                id={hatchId}
                width="6"
                height="6"
                patternUnits="userSpaceOnUse"
                patternTransform="rotate(45)"
              >
                <line
                  x1="0"
                  y1="0"
                  x2="0"
                  y2="6"
                  stroke="#4a4f4b"
                  strokeWidth="2"
                />
              </pattern>
            </defs>
            {data.phases.map((p, k) => (
              <g key={p.key}>
                <rect
                  x={X(p.from)}
                  y={Y_PHASE}
                  width={X(p.to + 1) - X(p.from)}
                  height={16}
                  fill={PHASE_FILLS[k % PHASE_FILLS.length]}
                />
                <text
                  x={X(p.from) + 5}
                  y={Y_PHASE + 12}
                  className={s.phaseText}
                >
                  {p.label}
                </text>
                {k > 0 ? (
                  <line
                    x1={X(p.from)}
                    x2={X(p.from)}
                    y1={Y_PHASE}
                    y2={Y_COMMITS + COMMITS_H}
                    stroke="#3a3f3b"
                    strokeDasharray="2 3"
                  />
                ) : null}
              </g>
            ))}
            <rect
              x={X(0)}
              y={Y_PURGE}
              width={X(purgeDay) - X(0)}
              height={15}
              fill={`url(#${hatchId})`}
            />
            <text x={X(0) + 5} y={Y_PURGE + 11} className={s.purgeText}>
              transcripts purged — model and effort unknown
            </text>
            {LANE_NAMES.map((name, lane) => {
              const y = Y_LANES + lane * (LANE_H + LANE_GAP);
              return (
                <g key={name}>
                  <rect
                    x={X(0)}
                    y={y}
                    width={PW}
                    height={LANE_H}
                    fill="#101211"
                  />
                  {lane === 0 ? (
                    <rect
                      x={X(0)}
                      y={y}
                      width={X(purgeDay) - X(0)}
                      height={LANE_H}
                      fill={`url(#${hatchId})`}
                      opacity={0.35}
                    />
                  ) : null}
                  <rect
                    x={4}
                    y={y + 15}
                    width={8}
                    height={8}
                    rx={2}
                    fill={LANE_COLOURS[lane]}
                  />
                  <text x={17} y={y + 22} className={s.laneText}>
                    {LANE_SHORT[lane]}
                  </text>
                  <text x={17} y={y + 35} className={s.laneSub}>
                    {lane === 2 ? "app" : "Claude Code"}
                  </text>
                  <path
                    d={paths[lane].off}
                    stroke={LANE_COLOURS[lane]}
                    strokeOpacity={0.16}
                  />
                  <path
                    d={paths[lane].on}
                    stroke={LANE_COLOURS[lane]}
                    strokeWidth={1.2}
                  />
                </g>
              );
            })}
            <text x={4} y={Y_COMMITS + 14} className={s.laneText}>
              commits
            </text>
            <text x={4} y={Y_COMMITS + 27} className={s.laneSub}>
              per day · max {maxCommits}
            </text>
            {data.commitsPerDay.map((c, d) =>
              c > 0 ? (
                <rect
                  // biome-ignore lint/suspicious/noArrayIndexKey: one bar per day index
                  key={d}
                  x={X(d) + 0.6}
                  y={Y_COMMITS + COMMITS_H - (c / maxCommits) * COMMITS_H}
                  width={Math.max(0.8, colW - 1.2)}
                  height={(c / maxCommits) * COMMITS_H}
                  fill={d >= brush[0] && d <= brush[1] ? "#858a84" : "#2c302d"}
                />
              ) : null,
            )}
            {axisDays.map((d) => (
              <text
                key={d}
                x={d === nDays - 1 ? X(d + 1) : X(d)}
                y={Y_AXIS}
                textAnchor={d === nDays - 1 ? "end" : "start"}
                className={s.axisText}
              >
                {shortDate(dayDate(start, d))}
              </text>
            ))}
            {!whole ? (
              <g pointerEvents="none">
                <rect
                  x={X(0)}
                  y={Y_LANES - 2}
                  width={X(brush[0]) - X(0)}
                  height={Y_COMMITS + COMMITS_H - Y_LANES + 4}
                  className={s.dim}
                />
                <rect
                  x={X(brush[1] + 1)}
                  y={Y_LANES - 2}
                  width={X(nDays) - X(brush[1] + 1)}
                  height={Y_COMMITS + COMMITS_H - Y_LANES + 4}
                  className={s.dim}
                />
                <rect
                  x={X(brush[0])}
                  y={Y_LANES - 2}
                  width={X(brush[1] + 1) - X(brush[0])}
                  height={Y_COMMITS + COMMITS_H - Y_LANES + 4}
                  className={s.brush}
                />
              </g>
            ) : null}
            {day >= 0 ? (
              <rect
                x={X(day)}
                y={Y_LANES - 2}
                width={colW}
                height={Y_COMMITS + COMMITS_H - Y_LANES + 4}
                className={s.dayFocus}
                pointerEvents="none"
              />
            ) : null}
          </svg>
        </section>
        <div
          ref={tip.tipRef}
          id={tipId}
          role="tooltip"
          className={s.tip}
          hidden={day < 0}
          data-pinned={tip.pinned ? "" : undefined}
          style={{ left: tip.pos.left, top: tip.pos.top }}
        >
          {day >= 0 ? (
            <DayTip
              data={data}
              day={day}
              counts={counts[day]}
              shown={
                ticks.filter((t) => t.day === day && shown.has(t.i)).length
              }
              purgeDay={purgeDay}
            />
          ) : null}
        </div>
      </div>
      <RangeInputs data={data} filter={filter} onRange={onRange} />
    </div>
  );
}

/** Returns the day indices of the first of each month after the window's first day. */
export function monthStarts(start: string, nDays: number): number[] {
  const out: number[] = [];
  for (let d = 1; d < nDays - 4; d++) {
    if (dayDate(start, d).getUTCDate() === 1) out.push(d);
  }
  return out;
}

/** WEEKDAYS are short English weekday names, Sunday first. */
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** The tooltip of one day: prompts per lane, prompts passing the filter, commits, settings note. */
function DayTip({
  data,
  day,
  counts,
  shown,
  purgeDay,
}: {
  data: EpData;
  day: number;
  counts: DayCounts;
  shown: number;
  purgeDay: number;
}) {
  const date = dayDate(data.window[0], day);
  const phase = data.phases.find((p) => day >= p.from && day <= p.to);
  return (
    <>
      <div className={s.th}>
        {WEEKDAYS[date.getUTCDay()]} {shortDate(date)}, 2026
        {phase ? <span className={s.thPhase}> · {phase.label}</span> : null}
      </div>
      <div className={s.tm}>
        {counts.total} prompt{counts.total === 1 ? "" : "s"}
        {counts.total !== shown ? ` (${shown} match the filter)` : ""}
      </div>
      <ul className={s.tipLanes}>
        {LANE_NAMES.map((name, lane) => (
          <li key={name}>
            <span className={s.sw} style={{ background: LANE_COLOURS[lane] }} />
            {name}: {counts.lanes[lane]}
          </li>
        ))}
      </ul>
      <div className={s.tm}>
        {data.commitsPerDay[day]} commit
        {data.commitsPerDay[day] === 1 ? "" : "s"} that day
      </div>
      {day < purgeDay && counts.lanes[0] > 0 ? (
        <div className={s.tnote}>
          Mac transcripts purged: model and effort unknown, or inferred from
          commits
        </div>
      ) : null}
    </>
  );
}

/** The keyboard alternative to the brush: labelled from/to range inputs and a reset button. */
function RangeInputs({
  data,
  filter,
  onRange,
}: {
  data: EpData;
  filter: Filter;
  onRange: (from: number, to: number) => void;
}) {
  const uid = useId();
  const nDays = data.commitsPerDay.length;
  const start = data.window[0];
  const whole = filter.from === 0 && filter.to === nDays - 1;
  return (
    <div className={s.ranges}>
      <label className={s.rangeLabel} htmlFor={`${uid}-from`}>
        From <b>{shortDate(dayDate(start, filter.from))}</b>
      </label>
      <input
        id={`${uid}-from`}
        type="range"
        className={s.range}
        min={0}
        max={nDays - 1}
        value={filter.from}
        aria-valuetext={shortDate(dayDate(start, filter.from))}
        onChange={(e) => {
          const v = Number(e.currentTarget.value);
          onRange(Math.min(v, filter.to), filter.to);
        }}
      />
      <label className={s.rangeLabel} htmlFor={`${uid}-to`}>
        To <b>{shortDate(dayDate(start, filter.to))}</b>
      </label>
      <input
        id={`${uid}-to`}
        type="range"
        className={s.range}
        min={0}
        max={nDays - 1}
        value={filter.to}
        aria-valuetext={shortDate(dayDate(start, filter.to))}
        onChange={(e) => {
          const v = Number(e.currentTarget.value);
          onRange(filter.from, Math.max(v, filter.from));
        }}
      />
      <button
        type="button"
        className={s.btn}
        disabled={whole}
        onClick={() => onRange(0, nDays - 1)}
      >
        All {nDays} days
      </button>
    </div>
  );
}
