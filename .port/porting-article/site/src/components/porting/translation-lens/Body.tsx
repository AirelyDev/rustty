"use client";

import data from "@/app/porting/data/translation-lens.json";
import { useId, useRef, useState } from "react";
import useChartTip from "../ui/useChartTip";
import useReducedMotion from "../ui/useReducedMotion";
import ClassPanel from "./ClassPanel";
import CodeLens, { type Selection } from "./CodeLens";
import Evidence from "./Evidence";
import Insights from "./Insights";
import { type TlData, type UnsafeMetric, patternCrates } from "./model";
import ModulePanel from "./ModulePanel";
import PatternHead from "./PatternHead";
import TipContent from "./TipContent";
import s from "./TranslationLens.module.css";
import UnsafePanel from "./UnsafePanel";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as TlData;

/**
 * The "Translation lens" widget: six Zig patterns and their Rust form as
 * real side-by-side code with line mappings, the evidence for each (git grep
 * counts, pointer-lifetime ledger rows, Miri harnesses or measurements), three
 * linked side panels and the computed insights.
 */
export default function Body() {
  const reduced = useReducedMotion();
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const [pi, setPi] = useState(0);
  const [si, setSi] = useState(0);
  const [sel, setSel] = useState<Selection | null>(null);
  const [modMetric, setModMetric] = useState<0 | 1 | 2>(0);
  const [unsMetric, setUnsMetric] = useState<UnsafeMetric>("nt");
  const [selCls, setSelCls] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(rootRef);
  const p = DATA.patterns[pi];
  const sub = p.subs[si];
  const pc = patternCrates(p);

  /** Selects pattern i: first excerpt, no highlight, no class selected. */
  const pick = (i: number) => {
    setPi(i);
    setSi(0);
    setSel(null);
    setSelCls(null);
    tip.clear();
  };
  /** Shows sub-excerpt i of the current pattern. */
  const pickSub = (i: number) => {
    setSi(i);
    setSel(null);
    tip.clear();
  };

  return (
    <div ref={rootRef} className={s.root}>
      <PatternHead
        data={DATA}
        pi={pi}
        si={si}
        uid={uid}
        onPick={pick}
        onSub={pickSub}
      />
      <div
        role="tabpanel"
        id={`${uid}-panel`}
        aria-labelledby={`${uid}-tab${pi}`}
      >
        <CodeLens
          key={`${pi}-${si}`}
          sub={sub}
          sel={sel}
          setSel={setSel}
          tip={tip}
          uid={uid}
          reduced={reduced}
        />
        <Evidence
          p={p}
          sub={sub}
          sel={sel}
          setSel={setSel}
          ledgerTotal={DATA.ledgerClasses.reduce((a, c) => a + c.n, 0)}
        />
        <div className={s.panels}>
          <ModulePanel
            modules={DATA.modules}
            pc={pc}
            metric={modMetric}
            setMetric={setModMetric}
            tip={tip}
          />
          <ClassPanel
            classes={DATA.ledgerClasses}
            p={p}
            selected={selCls}
            setSelected={setSelCls}
          />
          <UnsafePanel
            data={DATA}
            pc={pc}
            metric={unsMetric}
            setMetric={setUnsMetric}
            tip={tip}
          />
        </div>
      </div>
      <Insights data={DATA} />
      <div
        ref={tip.tipRef}
        id={`${uid}-tip`}
        role="tooltip"
        className={s.tip}
        hidden={!tip.shown}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {tip.shown ? (
          <TipContent
            id={tip.shown}
            data={DATA}
            sub={sub}
            pc={pc}
            modMetric={modMetric}
            unsMetric={unsMetric}
          />
        ) : null}
      </div>
    </div>
  );
}
