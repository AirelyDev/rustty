"use client";

import data from "@/app/porting/data/moving-target.json";
import { useId, useMemo, useRef, useState } from "react";
import useChartTip from "../ui/useChartTip";
import Detail from "./Detail";
import Header from "./Header";
import Insights from "./Insights";
import Matrix from "./Matrix";
import { maxLag, type MtData } from "./model";
import { type Metric, totals as totalsOf } from "./stats";
import TimeChart from "./TimeChart";
import TipContent from "./TipContent";
import { PALETTE } from "../lib/palette";
import s from "./MovingTarget.module.css";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as MtData;

/** METRICS are the matrix cell options: lines changed or commits touching. */
const METRICS: [Metric, string][] = [
  ["churn", "lines changed"],
  ["commits", "commits touching"],
];

/**
 * "Chasing a moving target": the hero count and five tiles, the size chart
 * (Ghostty main, the pinned reference and Rustty over time, with the reference
 * moves), the per-move matrix with its metric toggle, the selected move's
 * detail panel and the three insight tiles. The selected move links the
 * chart, the matrix and the detail panel.
 */
export default function Body() {
  const boxRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(boxRef);
  const tipId = `mt${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-tip`;
  const [sel, setSel] = useState(DATA.adv.length - 1);
  const [metric, setMetric] = useState<Metric>("churn");
  const [hoverRow, setHoverRow] = useState<number | null>(null);
  const totals = useMemo(() => totalsOf(DATA), []);
  const lag = useMemo(() => maxLag(DATA), []);

  return (
    <div ref={boxRef} className={s.root}>
      <Header t={totals} lag={lag} moves={DATA.adv.length} />
      <div className={s.sec}>
        <h4>Size over time</h4>
        <div className={s.legend}>
          <span>
            <i className={s.swDash} style={{ borderColor: PALETTE.upstream }} />
            Ghostty main (weekly)
          </span>
          <span>
            <i className={s.swLine} style={{ borderColor: PALETTE.upstream }} />
            pinned reference
          </span>
          <span>
            <i
              className={s.swArea}
              style={{ background: "rgba(100,216,198,.18)" }}
            />
            lines the reference trails main
          </span>
          <span>
            <i className={s.swLine} style={{ borderColor: PALETTE.rust }} />
            Rustty .rs lines
          </span>
        </div>
      </div>
      <TimeChart
        data={DATA}
        sel={sel}
        onSelect={setSel}
        tip={tip}
        tipId={tipId}
      />
      <div className={s.sec} style={{ marginTop: 14 }}>
        <h4>Per move: what moved upstream, and what it cost</h4>
        {/* biome-ignore lint/a11y/useSemanticElements: a pill toggle row, not a form fieldset */}
        <div className={s.seg} role="group" aria-label="Matrix values">
          {METRICS.map(([k, label]) => (
            <button
              key={k}
              type="button"
              aria-pressed={metric === k}
              className={metric === k ? s.segOn : undefined}
              onClick={() => setMetric(k)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for all {DATA.adv.length} moves →
      </p>
      <div className={s.mx}>
        <Matrix
          data={DATA}
          totals={totals}
          sel={sel}
          metric={metric}
          hoverRow={hoverRow}
          onHoverRow={setHoverRow}
          onSelect={setSel}
          tip={tip}
        />
      </div>
      <Detail data={DATA} sel={sel} />
      <Insights data={DATA} />
      <div
        ref={tip.tipRef}
        id={tipId}
        role="tooltip"
        className={s.tip}
        hidden={!tip.shown}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {tip.shown ? <TipContent id={tip.shown} data={DATA} /> : null}
      </div>
    </div>
  );
}
