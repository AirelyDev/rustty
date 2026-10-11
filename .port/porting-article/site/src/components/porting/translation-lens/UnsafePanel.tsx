import { useRef } from "react";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import {
  type PatternCrates,
  type TlData,
  type UnsafeMetric,
  unsafeRows,
  unsafeValue,
} from "./model";
import s from "./TranslationLens.module.css";
import useRowKeys from "./useRowKeys";

/** UNSAFE_METRICS are the crate-panel toggles. */
export const UNSAFE_METRICS: [UnsafeMetric, string][] = [
  ["nt", "non-test / kLOC"],
  ["all", "all / kLOC"],
  ["sites", "total sites"],
];

/** HEUR_COLOR colours the same-line heuristic categories ("other" is grey). */
const HEUR_COLOR: Record<string, string> = {
  "raw-pointer deref / arithmetic": PALETTE.rust,
  "FFI / foreign call": PALETTE.yellow,
  "slice from raw parts": PALETTE.purple,
  "intrusive link (field address)": PALETTE.upstream,
  "container_of / provenance": PALETTE.blue,
};

/** Chart geometry (viewBox units): width, row height, label edge, bar end. */
const W = 300;
const ROW_H = 12;
const L = 104;
const R = W - 38;

/** Props for UnsafePanel. */
type UnsafePanelProps = {
  data: TlData;
  pc: PatternCrates;
  metric: UnsafeMetric;
  setMetric: (m: UnsafeMetric) => void;
  tip: ChartTip;
};

/**
 * unsafe per crate (20 or more sites, -sys crates left out) for the chosen
 * metric, the pattern's crates highlighted with a ◆ count of pattern sites,
 * and the stacked bar classifying same-line unsafe blocks outside tests.
 */
export default function UnsafePanel({
  data,
  pc,
  metric,
  setMetric,
  tip,
}: UnsafePanelProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const rows = unsafeRows(data.unsafeCrates, metric);
  const keys = useRowKeys(rows.length, "uns:", svgRef, tip);
  const top = new Set(pc.top);
  const mx = unsafeValue(rows[0], metric);
  const H = rows.length * ROW_H + 6;
  const h = data.unsafeHeur;

  return (
    <section className={s.box} aria-label="unsafe per crate">
      <div className={s.bt}>
        <span>unsafe per crate</span>
        <b>{fmtInt(data.unsafeTotal)} sites</b>
      </div>
      <fieldset className={`${s.tog} ${s.subs}`} aria-label="unsafe metric">
        {UNSAFE_METRICS.map(([m, label]) => (
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
        aria-roledescription="bar chart"
        aria-label={`unsafe sites per crate, ${UNSAFE_METRICS.find((m) => m[0] === metric)?.[1]}. Use the arrow keys to step through crates, Enter to pin the details, Escape to close them.`}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the chart handles arrows, Enter and Escape itself
        tabIndex={0}
        onKeyDown={keys.onKeyDown}
        onBlur={keys.onBlur}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") tip.leave();
        }}
      >
        {rows.map((c, i) => {
          const y = i * ROW_H + 2;
          const v = unsafeValue(c, metric);
          const on = top.has(c.crate);
          const w = Math.max(1, ((R - L) * v) / mx);
          return (
            // biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcut; keyboard access is on the role=application svg
            <g
              key={c.crate}
              data-row={i}
              onPointerMove={(e) => {
                if (e.pointerType === "mouse")
                  tip.hover(`uns:${i}`, { x: e.clientX, y: e.clientY });
              }}
              onClick={(e) =>
                tip.tap(`uns:${i}`, { x: e.clientX, y: e.clientY })
              }
            >
              <rect
                x="0"
                y={y}
                width={W}
                height={ROW_H}
                fill={on ? "rgba(217,121,52,.09)" : "transparent"}
                stroke={keys.cursor === i ? "#f4f1ed" : "none"}
                strokeWidth={0.6}
              />
              <text
                x={L - 5}
                y={y + 9}
                fontSize="8.5"
                textAnchor="end"
                fill={on ? "#f4f1ed" : "#858a84"}
                fontWeight={on ? 700 : 400}
              >
                {c.crate.replace("rustty-", "")}
              </text>
              <rect
                x={L}
                y={y + 2}
                width={w}
                height="8"
                rx="1.5"
                fill={
                  on
                    ? PALETTE.rust
                    : c.tier === "ffi-asan"
                      ? "#6b5a48"
                      : "#4a4f4b"
                }
              />
              <text x={L + w + 4} y={y + 9} fontSize="8" fill="#c1c8c0">
                {metric === "sites" ? fmtInt(v) : v.toFixed(1)}
              </text>
              {on ? (
                <text
                  x={W - 1}
                  y={y + 9}
                  fontSize="7.5"
                  textAnchor="end"
                  fill={PALETTE.gold}
                >
                  ◆{pc.all[c.crate]}
                </text>
              ) : null}
            </g>
          );
        })}
      </svg>
      <div className={s.heur}>
        <div className={s.bt} style={{ margin: "0 0 4px" }}>
          <span>What same-line unsafe blocks do</span>
          <b>{fmtInt(h.total)}</b>
        </div>
        <div className={s.heurBar} aria-hidden="true">
          {h.cats.map(([k, n]) => (
            <i
              key={k}
              style={{ flex: n, background: HEUR_COLOR[k] ?? "#3a3e3b" }}
            />
          ))}
        </div>
        <ul className={s.heurList}>
          {h.cats.map(([k, n]) => (
            <li key={k}>
              <span
                style={{ color: HEUR_COLOR[k] ?? "#6b716b" }}
                aria-hidden="true"
              >
                ■
              </span>{" "}
              {k} {fmtInt(n)} ({((100 * n) / h.total).toFixed(0)}%)
            </li>
          ))}
        </ul>
        <p className={s.none} style={{ fontSize: 10.5, marginTop: 4 }}>
          Heuristic: <code>unsafe {"{"}</code> lines outside test files and test
          modules, classified by what else is on the same line. A block that
          spans several lines counts as "other".
        </p>
      </div>
    </section>
  );
}
