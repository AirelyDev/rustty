"use client";

import { type KeyboardEvent, useRef } from "react";
import { fmtPct } from "../lib/format";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import ChapterView from "./ChapterView";
import { advPinSummary, full33Summary, scoreboard, suite, tm } from "./model";
import s from "./PerfReplay.module.css";
import type { PrData } from "./types";

/** One chapter of the second track: date, name and short result. */
type Chapter = { d: string; k: string; sub: string };

/** Returns the seven chapters with their short results computed from the data. */
export function chapters(data: PrData): Chapter[] {
  const last = data.stages.length - 1;
  const f33 = full33Summary(data);
  const adv = advPinSummary(data);
  const apc = data.apc.filter((r) => r.config === "apc-parser");
  const pin = data.pin;
  return [
    {
      d: "2026-07-16",
      k: "July final",
      sub: `${scoreboard(data, last).W}/${data.bench.length} · ${fmtPct(suite(data, last).deltaPct)}`,
    },
    { d: "2026-07-30", k: "Re-measure", sub: "provisional" },
    {
      d: "2026-08-28",
      k: "33 configs",
      sub: `${f33.winsAfter}/${f33.n} · geomean ${f33.geoAfter.toFixed(3)}`,
    },
    { d: "2026-08-28", k: "Comptime fixes", sub: "gap classes" },
    {
      d: data.advPin.date,
      k: "Newer reference",
      sub: `${adv.wins}/${adv.n} · ${fmtPct(adv.deltaPct)}`,
    },
    {
      d: "2026-10-07",
      k: "APC + hyperlink",
      sub: `${apc[0].ratio.toFixed(2)}× → ${apc[apc.length - 1].ratio.toFixed(2)}×`,
    },
    {
      d: pin.date ?? "2026-10-08",
      k: "Current reference",
      sub:
        pin.status === "measured"
          ? `${pin.rows.filter((r) => r.rustty[0] < r.ghostty[0]).length}/${pin.rows.length} · ${pin.rev}`
          : "pending",
    },
  ];
}

/** CD0 and CD1 bound the chapter date axis: 07-10 → 10-14. */
const CD0 = tm("2026-07-10T00:00:00");
const CD1 = tm("2026-10-14T00:00:00");

/** Maps a date to x on the 960-wide chapter track. */
function CX(d: string): number {
  return 30 + ((tm(`${d}T12:00:00`) - CD0) / (CD1 - CD0)) * 900;
}

/** NODE_NUDGE separates chapters that share a date (px). */
const NODE_NUDGE = [0, 0, -6, 6, -10, 2, 12];

/** LABEL_X are the chapter labels' x positions, spread evenly under the axis. */
const LABEL_X = [60, 190, 330, 470, 610, 750, 890];

/**
 * The second scrub track (chapters after the day on a date axis) and the
 * selected chapter's view. The nodes are tabs: click, Enter or the arrow keys
 * select a chapter.
 */
export default function Chapters({
  data,
  sel,
  onSel,
  narrow,
  tip,
}: {
  data: PrData;
  sel: number;
  onSel: (i: number) => void;
  narrow: boolean;
  tip: ChartTip;
}) {
  const ch = chapters(data);
  const trackRef = useRef<SVGSVGElement>(null);
  const last = ch.length - 1;

  /** Selects chapter i (clamped) and moves focus to its node. */
  const go = (i: number) => {
    const k = Math.max(0, Math.min(last, i));
    onSel(k);
    trackRef.current?.querySelector<SVGGElement>(`[data-ch="${k}"]`)?.focus();
  };
  /** Arrow keys move between chapters; Enter or Space selects. */
  const onKey = (e: KeyboardEvent<SVGGElement>, i: number) => {
    if (e.key === "ArrowRight") go(i + 1);
    else if (e.key === "ArrowLeft") go(i - 1);
    else if (e.key === "Enter" || e.key === " ") onSel(i);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <>
      <div className={`${s.sec} ${s.secBig}`}>
        Second track · after the day (pick a chapter)
      </div>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for every chapter →
      </p>
      <div className={s.scroll} role="tablist" aria-label="Later perf chapters">
        <svg
          ref={trackRef}
          className={`${s.svg} ${s.wideSvg}`}
          viewBox="0 0 960 74"
          role="none"
        >
          <line
            x1={20}
            x2={950}
            y1={24}
            y2={24}
            stroke="#262927"
            strokeWidth={2}
          />
          {["2026-08-01", "2026-09-01", "2026-10-01"].map((m) => (
            <text key={m} x={CX(m)} y={10} fill={PALETTE.gray4} fontSize={9}>
              {m.slice(0, 7)}
            </text>
          ))}
          {ch.map((c, i) => {
            const pending = i === last && data.pin.status !== "measured";
            const x = CX(c.d) + NODE_NUDGE[i];
            return (
              <g
                key={c.k}
                data-ch={i}
                className={s.chNode}
                role="tab"
                aria-selected={i === sel}
                aria-label={`${c.k}, ${c.d}: ${c.sub}`}
                tabIndex={i === sel ? 0 : -1}
                onClick={() => onSel(i)}
                onKeyDown={(e) => onKey(e, i)}
              >
                <rect
                  x={LABEL_X[i] - 62}
                  y={14}
                  width={124}
                  height={58}
                  fill="transparent"
                />
                <circle
                  cx={x}
                  cy={24}
                  r={i === sel ? 8 : 5.5}
                  fill={pending ? "#000" : PALETTE.rust}
                  stroke={pending ? PALETTE.gold : "#000"}
                  strokeWidth={1.5}
                  strokeDasharray={pending ? "2 2" : undefined}
                />
                <line
                  x1={x}
                  x2={LABEL_X[i]}
                  y1={30}
                  y2={42}
                  stroke={PALETTE.gray3}
                />
                <text
                  x={LABEL_X[i]}
                  y={53}
                  fill={i === sel ? PALETTE.gray8 : PALETTE.gray7}
                  fontSize={9.5}
                  fontWeight={i === sel ? 700 : 400}
                  textAnchor="middle"
                >
                  {c.k}
                </text>
                <text
                  x={LABEL_X[i]}
                  y={66}
                  fill={pending ? PALETTE.gold : PALETTE.gray4}
                  fontSize={9}
                  textAnchor="middle"
                >
                  {c.d.slice(5)} · {c.sub}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <div className={s.chv} role="tabpanel" aria-label={ch[sel].k}>
        <ChapterView data={data} i={sel} narrow={narrow} tip={tip} />
      </div>
    </>
  );
}
