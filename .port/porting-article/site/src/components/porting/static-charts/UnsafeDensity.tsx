import CommitLink from "../ui/CommitLink";
import data from "@/app/porting/data/unsafe-density.json";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import { linear } from "../lib/scales";
import Hot, { TipBody } from "./Hot";
import { dayOffset, md, paddedExtent } from "./model";
import s from "./StaticCharts.module.css";
import UnsafeBars from "./UnsafeBars";
import KeyGroup from "../ui/KeyGroup";

/** PW and PH are the viewBox size of the density-over-time panel. */
const PW = 300;
const PH = 250;

/** AXIS_START and AXIS_END bound the time axis of the density panel. */
const AXIS_START = "2026-07-01";
const AXIS_END = "2026-10-20";

/** MONTH_NAMES are the axis labels of the density panel's month ticks. */
const MONTH_NAMES: Record<string, string> = {
  "07": "Jul",
  "08": "Aug",
  "09": "Sep",
  "10": "Oct",
};

/**
 * Unsafe density over time: the census at each snapshot as a line of dots,
 * labelled per 1,000 physical lines, over bars of the physical line count.
 * Two snapshots share 08-25, so the second is nudged right.
 */
function DensityPanel() {
  const left = 34;
  const right = PW - 12;
  const top = 26;
  const bottom = 158;
  const [lo, hi] = paddedExtent(
    data.series.map((p) => p.density),
    0.5,
  );
  const x = linear([0, dayOffset(AXIS_END, AXIS_START)], [left, right]);
  const y = linear([lo, hi], [bottom, top]);
  const maxLines = Math.max(...data.series.map((p) => p.lines));
  const lineH = linear([0, maxLines], [0, 52]);
  const pts = data.series.map((p, i, all) => ({
    ...p,
    cx:
      x(dayOffset(p.d, AXIS_START)) + (i > 0 && all[i - 1].d === p.d ? 12 : 0),
    cy: y(p.density),
  }));
  const ticks: number[] = [];
  for (let v = lo; v <= hi + 1e-9; v += 1) ticks.push(v);
  const months = ["2026-07-01", "2026-08-01", "2026-09-01", "2026-10-01"];
  const barBase = PH - 18;
  return (
    <div className={s.plot}>
      <svg
        viewBox={`0 0 ${PW} ${PH}`}
        className={s.svg}
        role="img"
        aria-label={`Unsafe sites per 1,000 physical lines: ${data.series.map((p) => `${p.density} on ${md(p.d)}`).join(", ")}.`}
      >
        <text x={left} y={12} className={s.panelText}>
          per 1,000 physical lines
        </text>
        {ticks.map((v) => (
          <g key={v}>
            <line
              x1={left}
              x2={right}
              y1={y(v)}
              y2={y(v)}
              stroke={PALETTE.gray2}
              strokeDasharray="2 4"
            />
            <text
              x={left - 4}
              y={y(v) + 3}
              textAnchor="end"
              className={s.axisText}
            >
              {v}
            </text>
          </g>
        ))}
        <path
          d={pts.map((p, i) => `${i ? "L" : "M"}${p.cx},${p.cy}`).join("")}
          fill="none"
          stroke={PALETTE.rust}
          strokeWidth={2}
        />
        {pts.map((p, i) => (
          <g key={p.commit}>
            <circle
              cx={p.cx}
              cy={p.cy}
              r={4}
              fill={PALETTE.rust}
              stroke={PALETTE.card}
              strokeWidth={1.5}
            />
            <text
              x={p.cx}
              y={p.cy + (i >= 2 ? 15 : -8)}
              textAnchor={i === pts.length - 1 ? "end" : "middle"}
              className={s.valText}
            >
              {p.density.toFixed(2)}
            </text>
            <rect
              x={p.cx - 4}
              y={barBase - lineH(p.lines)}
              width={8}
              height={lineH(p.lines)}
              fill={PALETTE.gray3}
            />
          </g>
        ))}
        <text
          x={left}
          y={barBase - lineH(maxLines) - 6}
          className={s.panelText}
        >
          physical lines ×{data.physicalGrowth} since 07-10
        </text>
        <line
          x1={left}
          x2={right}
          y1={barBase}
          y2={barBase}
          stroke={PALETTE.gray3}
        />
        {months.map((m) => (
          <text
            key={m}
            x={x(dayOffset(m, AXIS_START))}
            y={PH - 4}
            textAnchor="middle"
            className={s.axisText}
          >
            {MONTH_NAMES[m.slice(5, 7)]}
          </text>
        ))}
      </svg>
      <KeyGroup label="Unsafe density at each census. Arrow keys move between them.">
        {pts.map((p) => (
          <Hot
            key={p.commit}
            id={`ud-p-${p.commit}`}
            x={p.cx - 11}
            y={Math.min(p.cy, barBase - lineH(p.lines)) - 12}
            w={22}
            h={barBase - Math.min(p.cy, barBase - lineH(p.lines)) + 12}
            W={PW}
            H={PH}
            label={`Census on ${md(p.d)}`}
          >
            <TipBody
              title={`Census at ${p.commit.slice(0, 9)} (${md(p.d)})`}
              rows={[
                `${fmtInt(p.sites)} unsafe sites in ${fmtInt(p.lines)} physical lines`,
                `${p.density.toFixed(2)} per 1,000 physical lines`,
                <>
                  census commit <CommitLink hash={p.commit} />
                </>,
              ]}
            />
          </Hot>
        ))}
      </KeyGroup>
    </div>
  );
}

/**
 * Unsafe, measured: the density over time, unsafe sites per crate split into
 * test and non-test code, and the terminal engine's non-test unsafe by area.
 * Each panel names its own density basis.
 */
export default function UnsafeDensity() {
  const last = data.series[data.series.length - 1];
  return (
    <div className={s.stack}>
      <div className={s.kpis}>
        <span>
          <b>{fmtInt(last.sites)}</b>unsafe sites at head
        </span>
        <span>
          <b>{last.density.toFixed(2)}</b>per 1,000 physical lines
        </span>
        <span>
          <b>
            ~{data.testShare.heuristic}% ({data.testShare.strict}%)
          </b>
          in test code (heuristic; stricter rule in brackets)
        </span>
      </div>
      <div className={s.threeCol}>
        <div>
          <div className={s.miniHead}>Density at each census</div>
          <DensityPanel />
        </div>
        <div>
          <div className={s.miniHead}>Unsafe sites per crate</div>
          <UnsafeBars kind="crates" />
        </div>
        <div>
          <div className={s.miniHead}>Terminal engine, by area</div>
          <UnsafeBars kind="areas" />
        </div>
      </div>
      <p className={s.miniNote}>
        Solid = sites in non-test code, hatched = sites in test code (split at
        each file's first test module). The line and the crate bars use physical
        lines; the area bars use code lines.
      </p>
    </div>
  );
}
