import data from "@/app/porting/data/unsafe-density.json";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import { linear } from "../lib/scales";
import Hot, { TipBody } from "./Hot";
import s from "./StaticCharts.module.css";
import KeyGroup from "../ui/KeyGroup";

/** BW is the viewBox width of a bar panel. */
const BW = 300;

/** ROW is the height in px of one bar row. */
const ROW = 17;

/** LABEL_W is the width of the name column; VALUE_W the width of the density column. */
const LABEL_W = 112;
const VALUE_W = 34;

/** One bar: name, non-test and test sites, density label and tooltip lines. */
type Bar = {
  name: string;
  nonTest: number;
  test: number;
  density: number;
  tip: string[];
};

/** Builds the crate bars (density per 1,000 physical lines of the crate). */
function crateBars(): Bar[] {
  const rows = [...data.crates.rows, data.crates.other];
  return rows.map((c) => ({
    name: c.crate,
    nonTest: c.nonTest,
    test: c.test,
    density: (c.sites / c.lines) * 1000,
    tip: [
      `${fmtInt(c.sites)} unsafe sites: ${fmtInt(c.nonTest)} in non-test code, ${fmtInt(c.test)} in test code`,
      `${fmtInt(c.lines)} physical lines (${fmtInt(c.codeLines)} code lines)`,
      `${((c.sites / c.lines) * 1000).toFixed(1)} per 1,000 physical lines`,
    ],
  }));
}

/** Builds the engine-area bars (non-test sites per 1,000 code lines of the area). */
function areaBars(): Bar[] {
  const rows = [...data.vtAreas.rows, data.vtAreas.other];
  return rows.map((a) => ({
    name: a.area,
    nonTest: a.nonTest,
    test: a.test,
    density: a.codeLines ? (a.nonTest / a.codeLines) * 1000 : 0,
    tip: [
      `${fmtInt(a.nonTest)} non-test and ${fmtInt(a.test)} test unsafe sites`,
      `${fmtInt(a.codeLines)} code lines`,
      `${a.codeLines ? ((a.nonTest / a.codeLines) * 1000).toFixed(1) : "0"} non-test sites per 1,000 code lines`,
    ],
  }));
}

/**
 * Horizontal stacked bars of unsafe sites: solid for non-test code, hatched
 * for test code, with the panel's density at the right. kind picks the
 * per-crate panel or the terminal engine's per-area panel.
 */
export default function UnsafeBars({ kind }: { kind: "crates" | "areas" }) {
  const bars = kind === "crates" ? crateBars() : areaBars();
  const top = 20;
  const H = top + bars.length * ROW + 8;
  const max = Math.max(...bars.map((b) => b.nonTest + b.test));
  const x = linear([0, max], [LABEL_W, BW - VALUE_W - 4]);
  const hatch = `ud-hatch-${kind}`;
  const unit = kind === "crates" ? "per 1k physical" : "non-test per 1k code";
  return (
    <div className={s.plot}>
      <svg
        viewBox={`0 0 ${BW} ${H}`}
        className={s.svg}
        role="img"
        aria-label={`Unsafe sites by ${kind === "crates" ? "crate" : "terminal-engine area"}: ${bars
          .slice(0, 3)
          .map((b) => `${b.name} ${fmtInt(b.nonTest + b.test)}`)
          .join(", ")}, and more.`}
      >
        <defs>
          <pattern
            id={hatch}
            width="4"
            height="4"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <rect width="4" height="4" fill={PALETTE.rust} fillOpacity={0.25} />
            <line
              x1="0"
              y1="0"
              x2="0"
              y2="4"
              stroke={PALETTE.rust}
              strokeWidth={1.4}
            />
          </pattern>
        </defs>
        <text x={BW} y={11} textAnchor="end" className={s.panelText}>
          {unit}
        </text>
        {bars.map((b, i) => {
          const yy = top + i * ROW;
          return (
            <g key={b.name}>
              <text
                x={LABEL_W - 6}
                y={yy + 11}
                textAnchor="end"
                className={s.axisText}
              >
                {b.name}
              </text>
              <rect
                x={x(0)}
                y={yy + 3}
                width={x(b.nonTest) - x(0)}
                height={ROW - 6}
                fill={PALETTE.rust}
              />
              <rect
                x={x(b.nonTest)}
                y={yy + 3}
                width={x(b.nonTest + b.test) - x(b.nonTest)}
                height={ROW - 6}
                fill={`url(#${hatch})`}
              />
              <text x={BW} y={yy + 11} textAnchor="end" className={s.valText}>
                {b.density.toFixed(1)}
              </text>
            </g>
          );
        })}
      </svg>
      <KeyGroup
        label={`Unsafe sites per ${kind === "crates" ? "crate" : "area"}. Arrow keys move between the bars.`}
      >
        {bars.map((b, i) => (
          <Hot
            key={b.name}
            id={`ud-${kind}-${i}`}
            x={0}
            y={top + i * ROW}
            w={BW}
            h={ROW}
            W={BW}
            H={H}
            label={`${b.name}: ${fmtInt(b.nonTest + b.test)} unsafe sites`}
          >
            <TipBody title={b.name} rows={b.tip} />
          </Hot>
        ))}
      </KeyGroup>
    </div>
  );
}
