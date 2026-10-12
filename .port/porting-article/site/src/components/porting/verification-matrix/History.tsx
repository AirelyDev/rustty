import { type KeyboardEvent, type MouseEvent, useRef } from "react";
import type { ChartTip } from "../ui/useChartTip";
import { useRovingFocus } from "../ui/useRovingFocus";
import {
  histHeight,
  histX,
  histY,
  HIST,
  historyDots,
  runHeading,
  STATUS_INFO,
  type HistDot,
  type VmData,
} from "./model";
import s from "./VerificationMatrix.module.css";

/** Returns the SVG marker for a dot: a disc, diamond or triangle sized by r, or a dashed ring when pending. */
function Mark({ d }: { d: HistDot }) {
  const info = STATUS_INFO[d.st];
  const common = { fill: info.color, fillOpacity: 0.9, stroke: "#0b0c0c" };
  if (info.shape === "diamond") {
    const r = d.r + 1;
    return (
      <polygon
        {...common}
        points={`${d.x},${d.y - r} ${d.x + r},${d.y} ${d.x},${d.y + r} ${d.x - r},${d.y}`}
      />
    );
  }
  if (info.shape === "triangle") {
    const r = d.r + 1;
    return (
      <polygon
        {...common}
        points={`${d.x},${d.y - r} ${d.x + r},${d.y + r * 0.8} ${d.x - r},${d.y + r * 0.8}`}
      />
    );
  }
  if (info.shape === "ring") {
    return (
      <circle
        cx={d.x}
        cy={d.y}
        r={d.r}
        fill="none"
        stroke="#858a84"
        strokeDasharray="2 1.5"
      />
    );
  }
  return <circle {...common} cx={d.x} cy={d.y} r={d.r} />;
}

/** Props for the lane history. */
type HistoryProps = {
  data: VmData;
  pin: number;
  sel: number | null;
  /** Jumps to a run's commit and selects its cell. */
  onJump: (k: number) => void;
  tip: ChartTip;
};

/**
 * Lane history: one row per tool, one column per reference commit, one marker
 * per recorded run (shape and colour = status, size = defects found, dashed
 * ring = pending). The selected commit is boxed. Click, tap or Enter on a
 * marker jumps to that commit and cell.
 */
export default function History({ data, pin, sel, onJump, tip }: HistoryProps) {
  const dots = historyDots(data);
  const cw = (HIST.width - HIST.left - HIST.pad) / data.pins.length;
  const svgRef = useRef<SVGSVGElement>(null);
  // The run markers are one Tab stop; arrow keys move between them.
  useRovingFocus(svgRef, { selector: '[role="button"]' });
  return (
    // biome-ignore lint/a11y/useSemanticElements: an SVG plot grouping its focusable markers, not a form fieldset
    <svg
      ref={svgRef}
      className={s.hist}
      viewBox={`0 0 ${HIST.width} ${histHeight(data.tools.length)}`}
      role="group"
      aria-label="Lane history: every recorded run by tool and reference commit. Arrow keys move between the runs."
    >
      {data.pins.map((p, i) => (
        <text
          key={p.sha}
          x={histX(data, i)}
          y={14}
          textAnchor="middle"
          style={{ fill: i === pin ? "#f4f1ed" : undefined }}
        >
          {p.sha.slice(0, 5)}
        </text>
      ))}
      <rect
        x={HIST.left + pin * cw + 1}
        y={HIST.top - 6}
        width={cw - 2}
        height={data.tools.length * HIST.row + 4}
        fill="none"
        stroke="#3a3e3b"
        rx={4}
      />
      {data.tools.map((t, ti) => (
        <g key={t.key}>
          <text
            x={HIST.left - 8}
            y={histY(ti) + 3}
            textAnchor="end"
            style={{ fill: "#c1c8c0" }}
          >
            {t.name}
          </text>
          <line
            x1={HIST.left}
            x2={HIST.width - HIST.pad}
            y1={histY(ti)}
            y2={histY(ti)}
            stroke="#191c1a"
          />
        </g>
      ))}
      {dots.map((d) => {
        const r = data.runs[d.k];
        if (r.st === "pending") {
          return (
            <g key={d.k}>
              <title>{`${runHeading(data, r)}: pending`}</title>
              <Mark d={d} />
            </g>
          );
        }
        const id = `r${d.k}`;
        /** Shows the record tooltip at the pointer. */
        const move = (e: MouseEvent) =>
          tip.hover(id, { x: e.clientX, y: e.clientY });
        /** Jumps to the run on Enter or Space. */
        const key = (e: KeyboardEvent<SVGGElement>) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onJump(d.k);
          }
        };
        return (
          // biome-ignore lint/a11y/useSemanticElements: an SVG marker, a native button cannot be drawn here
          <g
            key={d.k}
            role="button"
            tabIndex={0}
            className={`${s.hd} ${sel === d.k ? s.hdSel : ""}`}
            aria-label={`${runHeading(data, r)}, ${data.pins[d.pin].sha}, ${r.date}: ${STATUS_INFO[r.st].name}. Jump to this cell`}
            onClick={() => onJump(d.k)}
            onKeyDown={key}
            onMouseMove={move}
            onMouseLeave={tip.leave}
            onFocus={(e) => {
              const b = e.currentTarget.getBoundingClientRect();
              tip.focus(id, {
                x: b.left + b.width / 2,
                y: b.top + b.height / 2,
              });
            }}
            onBlur={tip.leave}
          >
            <Mark d={d} />
          </g>
        );
      })}
    </svg>
  );
}
