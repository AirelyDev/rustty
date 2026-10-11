"use client";

import {
  type KeyboardEvent,
  type MouseEvent,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import useChartTip from "../ui/useChartTip";
import { useRovingFocus } from "../ui/useRovingFocus";
import s from "./EveryPrompt.module.css";
import {
  AREA_COMMAND_ONLY,
  dayDate,
  dayIndexOf,
  type EpData,
  type Filter,
  LANE_NAMES,
  shortDate,
} from "./model";
import { type Session, sessionBar } from "./session-model";
import { LBL, monthStarts, PW, W } from "./Timeline";

/**
 * PERFORMANCE_PINK keeps performance out of the warm copper/gold/orange
 * cluster (engine, verification/CI, tooling); 8.6:1 on the plot background.
 */
const PERFORMANCE_PINK = "#f290c4";

/**
 * WEBSITE_LAVENDER marks the website sessions (prompts typed in the website
 * folders): the hue farthest from every other area colour (ΔE ≥ 26), 9.1:1 on
 * the plot background.
 */
const WEBSITE_LAVENDER = "#b4a7f5";

/**
 * AREA_COLOURS colour the eleven session areas; each is also named in the
 * legend and the card. All are at least 5.8:1 against the plot background.
 */
export const AREA_COLOURS: Record<string, string> = {
  engine: PALETTE.rust,
  "renderer and fonts": PALETTE.blue,
  "macOS app": PALETTE.purple,
  "GTK/Linux": PALETTE.pass,
  performance: PERFORMANCE_PINK,
  "upstream catch-up": PALETTE.upstream,
  "verification/CI": PALETTE.gold,
  defects: PALETTE.fail,
  tooling: PALETTE.orange,
  website: WEBSITE_LAVENDER,
  article: PALETTE.gray7,
};

/** COMMAND_ONLY_COLOUR draws the muted command-only sessions (5.3:1 on the plot background). */
const COMMAND_ONLY_COLOUR = PALETTE.gray4;

/** Y0 is the top of the first row; ROW_H a packed row's height. */
const Y0 = 4;
const ROW_H = 14;
/** THICK is the bar thickness per prompt bucket (1, 2–4, 5–14, 15–49, 50+ prompts). */
const THICK = [3, 5, 7, 9, 11];
/** TIP_GAP is the space between a bar (or the pointer) and the card below it. */
const TIP_GAP = 10;
/** PHASE_LINE is the colour of the phase dividers. */
const PHASE_LINE = "#3a3f3b";

/** Props of the session timeline. */
type SessionTimelineProps = {
  data: EpData;
  sessions: Session[];
  /** Keys of the sessions the filter keeps (others are dimmed). */
  kept: Set<string>;
  filter: Filter;
  /** Selects a session (or clears it when it is already selected). */
  onSelect: (s: Session) => void;
  /** Sets or clears the area filter. */
  onArea: (area: string | undefined) => void;
  /** Shows or hides the command-only sessions (hidden by default). */
  onCommandOnly: () => void;
};

/** Returns the Pacific "Jun 20 21:42" of a day index and minute of day. */
function when(start: string, day: number, minute: number): string {
  const hh = String(Math.floor(minute / 60)).padStart(2, "0");
  const mm = String(minute % 60).padStart(2, "0");
  return `${shortDate(dayDate(start, day))} ${hh}:${mm}`;
}

/** Returns the span of a session in words: "Jun 20 21:42 → Jun 21 00:33". */
export function sessionSpan(start: string, x: Session): string {
  const a = when(start, x.firstDay, x.firstMin);
  if (x.firstDay === x.lastDay && x.firstMin === x.lastMin) return a;
  const b =
    x.firstDay === x.lastDay
      ? when(start, x.lastDay, x.lastMin).split(" ").pop()
      : when(start, x.lastDay, x.lastMin);
  return `${a} → ${b}`;
}

/** Returns a session's accessible name: title, area, span, prompts and markers. */
function sessionLabel(start: string, x: Session, selected: boolean): string {
  const parts = [
    x.title,
    x.titleSource === "claude-written" ? "title by Claude" : "",
    `area ${x.area}`,
    x.founding ? "founding session" : "",
    sessionSpan(start, x),
    x.spannedDays > 1 ? `spanned ${x.spannedDays} days` : "",
    `${x.prompts} prompt${x.prompts === 1 ? "" : "s"}`,
    selected ? "selected" : "",
  ];
  return parts.filter(Boolean).join(", ");
}

/**
 * The session timeline above the prompt timeline, on the same day scale:
 * one bar per session from its first to its last prompt, packed into rows
 * so overlapping sessions stack, coloured by area (named in the legend and
 * the card), thicker for more prompts. Founding sessions carry a diamond;
 * command-only sessions are thin grey ticks outside the areas. Bars are one
 * Tab stop (arrow keys move between them); hover or focus shows the card,
 * a click or Enter selects the session, which filters everything below.
 */
export default function SessionTimeline({
  data,
  sessions,
  kept,
  filter,
  onSelect,
  onArea,
  onCommandOnly,
}: SessionTimelineProps) {
  const nDays = data.commitsPerDay.length;
  const colW = PW / nDays;
  /** Maps a fractional day to x. */
  const X = (d: number) => LBL + d * colW;
  const start = data.window[0];
  const purgeDay = dayIndexOf(start, data.purgedBefore);
  const nRows = Math.max(1, ...sessions.map((x) => x.row + 1));
  const plotH = nRows * ROW_H;
  const yAxis = Y0 + plotH + 13;
  const H = yAxis + 4;
  const boxRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const tip = useChartTip(boxRef);
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const tipId = `ep${uid}-stip`;
  const hatchId = `ep${uid}-shatch`;
  const [focused, setFocused] = useState<string | null>(null);
  useRovingFocus(svgRef, { selector: "[data-sess]" });

  // Sessions in time order (the keyboard order); command-only ones drawn first, beneath.
  const ordered = useMemo(
    () =>
      [...sessions].sort(
        (a, b) =>
          a.firstDay - b.firstDay ||
          a.firstMin - b.firstMin ||
          a.key.localeCompare(b.key),
      ),
    [sessions],
  );
  const byKey = useMemo(
    () => new Map(sessions.map((x) => [x.key, x])),
    [sessions],
  );

  /** Returns the bar geometry of a session (the packing in the data keeps neighbours apart). */
  const geo = (x: Session) => {
    const b = sessionBar(x, nDays, PW);
    const cy = Y0 + x.row * ROW_H + ROW_H / 2;
    const th = x.commandsOnly ? 2 : (THICK[x.promptsBucket] ?? THICK[0]);
    return { x: LBL + b.x, w: b.w, hitX: LBL + b.hitX, hitW: b.hitW, cy, th };
  };

  // The card opens below its bar (or the pointer), never over the legend
  // above: anchors are the bar's bottom-left, and the card is kept inside
  // the box horizontally only.
  const at = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const [tipPos, setTipPos] = useState({ left: 0, top: 0 });
  /** Returns the viewport anchor of a bar element (its bottom-left corner), remembered for the card. */
  const anchorOf = (el: Element) => {
    const r = el.getBoundingClientRect();
    at.current = { x: r.left, y: r.bottom };
    return at.current;
  };

  /** Selects a session on Enter or Space. */
  const onKey = (e: KeyboardEvent<SVGGElement>, x: Session) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(x);
      tip.focus(x.key, anchorOf(e.currentTarget));
    }
  };

  /** Selects a session on click and pins its card. */
  const onClick = (e: MouseEvent<SVGGElement>, x: Session) => {
    onSelect(x);
    tip.tap(x.key, anchorOf(e.currentTarget));
  };

  const shown = tip.shown ? (byKey.get(tip.shown) ?? null) : null;
  useLayoutEffect(() => {
    if (!shown || !boxRef.current || !tip.tipRef.current) return;
    const box = boxRef.current.getBoundingClientRect();
    const tw = tip.tipRef.current.offsetWidth;
    const left = Math.max(0, Math.min(at.current.x - box.left, box.width - tw));
    setTipPos({ left, top: at.current.y - box.top + TIP_GAP });
  }, [shown, tip.tipRef]);
  const ranged = filter.from > 0 || filter.to < nDays - 1;
  const counts = data.totals.sessionsByArea ?? {};
  const areas = data.areas ?? [];
  const axisDays = [0, ...monthStarts(start, nDays), nDays - 1];
  const commandOnly = data.totals.sessionsCommandOnly ?? 0;
  const founding = data.totals.foundingSessions ?? 0;
  const showCo = filter.commandOnly === true;
  const foundingCommandOnly = sessions.filter(
    (x) => x.founding && x.commandsOnly,
  ).length;

  return (
    <div className={s.sessions}>
      <div className={s.sessionsHead}>
        <h4 id={`ep${uid}-sh`}>Sessions</h4>
        <p className={s.sessionsKey}>
          bar: first to last prompt · thicker: more prompts ·{" "}
          <span className={s.foundingKey} aria-hidden="true">
            ◆
          </span>{" "}
          founding session ({founding}
          {foundingCommandOnly
            ? `, ${foundingCommandOnly} of them only commands or shell escapes`
            : ""}
          ) ·{" "}
          <button
            type="button"
            className={s.coToggle}
            aria-pressed={showCo}
            onClick={onCommandOnly}
          >
            <span className={s.cmdKey} aria-hidden="true" />{" "}
            {showCo ? "hide" : `+${commandOnly}`} command-only sessions
          </button>{" "}
          <span className={s.cmdNote}>
            (only Claude Code commands or shell escapes; no area)
          </span>
        </p>
      </div>
      <fieldset className={s.areaLegend}>
        <legend className={s.srOnly}>Filter by area</legend>
        {areas.map((a) => (
          <button
            key={a}
            type="button"
            className={s.areaBtn}
            aria-pressed={filter.area === a}
            onClick={() => onArea(filter.area === a ? undefined : a)}
          >
            <span
              className={s.areaSw}
              style={{ background: AREA_COLOURS[a] ?? PALETTE.gray5 }}
            />
            {a} <span className={s.legendN}>{counts[a] ?? 0}</span>
          </button>
        ))}
      </fieldset>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for every session →
      </p>
      <div ref={boxRef} className={s.plotBox}>
        <section
          className={s.plotScroll}
          aria-label="Session timeline, scrolls sideways"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be focusable to scroll by keyboard
          tabIndex={0}
        >
          {/* biome-ignore lint/a11y/useSemanticElements: an <svg> cannot be a <fieldset>; the group names the bars inside it */}
          <svg
            ref={svgRef}
            className={s.sessSvg}
            viewBox={`0 0 ${W} ${H}`}
            role="group"
            aria-label="Session timeline"
            aria-describedby={`ep${uid}-shelp`}
            // Chrome puts an SVG with a label in the Tab order; the bars are the stop (roving focus).
            tabIndex={-1}
          >
            <title>Session timeline</title>
            <desc id={`ep${uid}-shelp`}>
              {`One bar per session from ${shortDate(dayDate(start, 0))} to ${shortDate(dayDate(start, nDays - 1))}, on the same day scale as the prompts below. Arrow keys move between sessions, Enter selects one (only its prompts are shown below; Enter again clears it), Escape closes the card.`}
            </desc>
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
            <rect x={X(0)} y={Y0} width={PW} height={plotH} fill="#101211" />
            <rect
              x={X(0)}
              y={Y0}
              width={X(purgeDay) - X(0)}
              height={plotH}
              fill={`url(#${hatchId})`}
              opacity={0.25}
            />
            {data.phases.slice(1).map((p) => (
              <line
                key={p.key}
                x1={X(p.from)}
                x2={X(p.from)}
                y1={Y0}
                y2={Y0 + plotH}
                stroke={PHASE_LINE}
                strokeDasharray="2 3"
              />
            ))}
            <text x={4} y={Y0 + 12} className={s.laneText}>
              sessions
            </text>
            <text x={4} y={Y0 + 25} className={s.laneSub}>
              {fmtInt(sessions.length)} · {nRows} rows
            </text>
            <text x={4} y={Y0 + 37} className={s.laneSub}>
              purged: hatched
            </text>
            {ordered
              .filter(
                (x) =>
                  x.area === AREA_COMMAND_ONLY &&
                  (showCo || filter.session === x.key),
              )
              .map((x) => {
                const g = geo(x);
                const selected = filter.session === x.key;
                return (
                  <g key={x.key}>
                    <rect
                      x={g.x}
                      y={g.cy - g.th / 2}
                      width={g.w}
                      height={g.th}
                      fill={selected ? PALETTE.gray8 : COMMAND_ONLY_COLOUR}
                      opacity={kept.has(x.key) ? 1 : 0.35}
                    />
                    {selected ? (
                      // A selected command-only session (from a link) is ringed so it can be found.
                      <rect
                        x={g.x - 3}
                        y={g.cy - 5}
                        width={g.w + 6}
                        height={10}
                        rx={2}
                        fill="none"
                        stroke={PALETTE.gray8}
                        strokeWidth={1.4}
                      />
                    ) : null}
                  </g>
                );
              })}
            {ordered
              .filter((x) => x.area !== AREA_COMMAND_ONLY)
              .map((x) => {
                const g = geo(x);
                const on = kept.has(x.key);
                const selected = filter.session === x.key;
                return (
                  // biome-ignore lint/a11y/useSemanticElements: an SVG mark cannot be a <button>; it takes focus, Enter and Space itself
                  <g
                    key={x.key}
                    data-sess=""
                    role="button"
                    tabIndex={-1}
                    aria-pressed={selected}
                    aria-label={sessionLabel(start, x, selected)}
                    aria-describedby={tip.shown === x.key ? tipId : undefined}
                    className={s.sessBar}
                    onPointerEnter={(e) => {
                      if (e.pointerType === "mouse") {
                        at.current = { x: e.clientX, y: e.clientY + 8 };
                        tip.hover(x.key, at.current);
                      }
                    }}
                    onPointerLeave={(e) => {
                      if (e.pointerType === "mouse") tip.leave();
                    }}
                    onFocus={(e) => {
                      setFocused(x.key);
                      tip.focus(x.key, anchorOf(e.currentTarget));
                    }}
                    onBlur={() => {
                      setFocused(null);
                      tip.leave();
                    }}
                    onKeyDown={(e) => onKey(e, x)}
                    onClick={(e) => onClick(e, x)}
                  >
                    <rect
                      x={g.hitX}
                      y={g.cy - ROW_H / 2}
                      width={g.hitW}
                      height={ROW_H}
                      fill="transparent"
                    />
                    <rect
                      x={g.x}
                      y={g.cy - g.th / 2}
                      width={g.w}
                      height={g.th}
                      rx={1}
                      fill={AREA_COLOURS[x.area] ?? PALETTE.gray5}
                      opacity={on ? 1 : 0.22}
                      stroke={selected ? PALETTE.gray8 : "none"}
                      strokeWidth={selected ? 1.4 : 0}
                    />
                    {focused === x.key ? (
                      <rect
                        x={g.x - 2.5}
                        y={g.cy - g.th / 2 - 2.5}
                        width={g.w + 5}
                        height={g.th + 5}
                        rx={2}
                        fill="none"
                        stroke={PALETTE.upstream}
                        strokeWidth={1.4}
                      />
                    ) : null}
                  </g>
                );
              })}
            {ordered
              .filter(
                (x) =>
                  x.founding &&
                  (x.area !== AREA_COMMAND_ONLY ||
                    showCo ||
                    filter.session === x.key),
              )
              .map((x) => {
                const g = geo(x);
                const r = 4;
                return (
                  <path
                    key={`f${x.key}`}
                    d={`M${g.x} ${g.cy - r}l${r} ${r}l${-r} ${r}l${-r} ${-r}Z`}
                    fill={PALETTE.gray8}
                    stroke="#0b0c0c"
                    strokeWidth={1}
                    pointerEvents="none"
                  />
                );
              })}
            {ranged ? (
              <g pointerEvents="none">
                <rect
                  x={X(0)}
                  y={Y0}
                  width={X(filter.from) - X(0)}
                  height={plotH}
                  className={s.dim}
                />
                <rect
                  x={X(filter.to + 1)}
                  y={Y0}
                  width={X(nDays) - X(filter.to + 1)}
                  height={plotH}
                  className={s.dim}
                />
              </g>
            ) : null}
            {axisDays.map((d) => (
              <text
                key={d}
                x={d === nDays - 1 ? X(d + 1) : X(d)}
                y={yAxis}
                textAnchor={d === nDays - 1 ? "end" : "start"}
                className={s.axisText}
              >
                {shortDate(dayDate(start, d))}
              </text>
            ))}
          </svg>
        </section>
        <div
          ref={tip.tipRef}
          id={tipId}
          role="tooltip"
          className={s.tip}
          hidden={!shown}
          data-pinned={tip.pinned ? "" : undefined}
          style={{ left: tipPos.left, top: tipPos.top }}
        >
          {shown ? (
            <SessionTip
              start={start}
              x={shown}
              selected={filter.session === shown.key}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}

/** The card of one session: title, area, markers, span, prompts and commits. */
function SessionTip({
  start,
  x,
  selected,
}: {
  start: string;
  x: Session;
  selected: boolean;
}) {
  return (
    <>
      <div className={s.th}>{x.title}</div>
      {x.titleSource === "claude-written" ? (
        <div className={s.byClaude}>title by Claude</div>
      ) : null}
      <div className={s.tm}>
        <span
          className={s.sw}
          style={{ background: AREA_COLOURS[x.area] ?? PALETTE.gray5 }}
        />
        {x.area}
        {x.founding ? " · founding session" : ""}
      </div>
      <div className={s.tm}>{sessionSpan(start, x)}</div>
      <div className={s.tm}>
        {x.spannedDays > 1 ? `spanned ${x.spannedDays} days · ` : ""}
        {x.prompts} prompt{x.prompts === 1 ? "" : "s"} · {x.commits} commit
        {x.commits === 1 ? "" : "s"} linked
      </div>
      <div className={s.tm}>{LANE_NAMES[x.lane]}</div>
      <div className={s.tnote}>
        {selected
          ? "Selected: only its prompts show below. Click or Enter again to clear."
          : "Click or Enter: show only this session's prompts below."}
      </div>
    </>
  );
}
