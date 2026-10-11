import { useRef } from "react";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import {
  type ModuleRow,
  moduleDots,
  moduleTotals,
  type PatternCrates,
} from "./model";
import s from "./TranslationLens.module.css";
import useRowKeys from "./useRowKeys";

/** METRICS are the module metric toggles: index into [code, comments, tests] and label. */
export const METRICS: [0 | 1 | 2, string][] = [
  [0, "code lines"],
  [1, "comment lines"],
  [2, "tests"],
];

/** Chart geometry (viewBox units): width, row height, top margin, label edge, ratio column. */
const W = 300;
const ROW_H = 13;
const TOP = 16;
const L = 92;
const R = W - 34;

/** Props for ModulePanel. */
type ModulePanelProps = {
  modules: ModuleRow[];
  pc: PatternCrates;
  metric: 0 | 1 | 2;
  setMetric: (m: 0 | 1 | 2) => void;
  tip: ChartTip;
};

/**
 * Zig → Rust lines per module: a log-scale dumbbell over the module groups
 * with the Rust/Zig ratio on the right (green below 1×). Modules holding the
 * pattern's crates get a copper band and a thicker connector.
 */
export default function ModulePanel({
  modules,
  pc,
  metric,
  setMetric,
  tip,
}: ModulePanelProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const dots = moduleDots(modules, metric, pc);
  const keys = useRowKeys(dots.length, "mod:", svgRef, tip);
  const vals = dots.flatMap((d) => [d.z, d.r]);
  const lo = Math.floor(Math.log10(Math.max(1, Math.min(...vals))));
  const hi = Math.ceil(Math.log10(Math.max(...vals)));
  /** X position of value v on the log axis. */
  const x = (v: number) =>
    L + ((Math.log10(Math.max(v, 1)) - lo) / (hi - lo)) * (R - L);
  const H = TOP + dots.length * ROW_H + 18;
  const [tz, tr] = moduleTotals(modules, metric);
  const decades = Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);

  return (
    <section className={s.box} aria-label="Zig to Rust lines per module">
      <div className={s.bt}>
        <span>Zig → Rust lines per module</span>
        <b>
          {fmtInt(tz)} → {fmtInt(tr)} ({(tr / tz).toFixed(2)}×)
        </b>
      </div>
      <fieldset className={`${s.tog} ${s.subs}`} aria-label="Module metric">
        {METRICS.map(([m, label]) => (
          <button
            key={m}
            type="button"
            aria-pressed={metric === m}
            onClick={() => setMetric(m)}
          >
            {label}
          </button>
        ))}
      </fieldset>
      <svg
        ref={svgRef}
        className={s.chart}
        viewBox={`0 0 ${W} ${H}`}
        role="application"
        aria-roledescription="dumbbell chart"
        aria-label={`Zig and Rust ${METRICS[metric][1]} per module, log scale. Use the arrow keys to step through modules, Enter to pin the details, Escape to close them.`}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
        tabIndex={0}
        onKeyDown={keys.onKeyDown}
        onBlur={keys.onBlur}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") tip.leave();
        }}
      >
        {decades.map((e) => (
          <g key={e}>
            <line
              x1={x(10 ** e)}
              x2={x(10 ** e)}
              y1={TOP - 4}
              y2={H - 14}
              stroke="#1d201e"
            />
            <text
              x={x(10 ** e)}
              y={H - 3}
              fill="#4a4f4b"
              fontSize="8"
              textAnchor="middle"
            >
              {e >= 3 ? `${10 ** (e - 3)}k` : 10 ** e}
            </text>
          </g>
        ))}
        <text x={L} y="9" fill={PALETTE.upstream} fontSize="8">
          ● Zig
        </text>
        <text x={L + 38} y="9" fill={PALETTE.rust} fontSize="8">
          ● Rust
        </text>
        <text x={R} y="9" fill="#858a84" fontSize="8" textAnchor="end">
          log scale
        </text>
        {dots.map((d, i) => {
          const y = TOP + i * ROW_H + 6;
          const ratio = d.r / d.z;
          return (
            // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; keyboard access is on the role=application svg
            <g
              key={d.row.module}
              data-row={i}
              onPointerMove={(e) => {
                if (e.pointerType === "mouse")
                  tip.hover(`mod:${i}`, { x: e.clientX, y: e.clientY });
              }}
              onClick={(e) =>
                tip.tap(`mod:${i}`, { x: e.clientX, y: e.clientY })
              }
            >
              <rect
                x="0"
                y={y - 6}
                width={W}
                height={ROW_H}
                fill={d.on ? "rgba(217,121,52,.09)" : "transparent"}
                stroke={keys.cursor === i ? "#f4f1ed" : "none"}
                strokeWidth={0.6}
              />
              <text
                x={L - 6}
                y={y + 3}
                fill={d.on ? "#f4f1ed" : "#858a84"}
                fontSize="8.5"
                textAnchor="end"
                fontWeight={d.on ? 700 : 400}
              >
                {d.row.module.replace("dep: ", "").slice(0, 15)}
              </text>
              <line
                x1={x(d.z)}
                x2={x(d.r)}
                y1={y}
                y2={y}
                stroke={d.on ? PALETTE.rust : "#3a3e3b"}
                strokeWidth={d.on ? 2 : 1.4}
              />
              <circle cx={x(d.z)} cy={y} r="3" fill={PALETTE.upstream} />
              <circle cx={x(d.r)} cy={y} r="3" fill={PALETTE.rust} />
              <text
                x={W - 2}
                y={y + 3}
                fill={
                  ratio < 1 ? PALETTE.pass : d.on ? PALETTE.gold : "#858a84"
                }
                fontSize="8"
                textAnchor="end"
              >
                {ratio.toFixed(2)}×
              </text>
            </g>
          );
        })}
      </svg>
    </section>
  );
}
