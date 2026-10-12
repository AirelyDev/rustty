"use client";

import CommitText from "../ui/CommitText";
import CommitLink from "../ui/CommitLink";
import data from "@/app/porting/data/verification-matrix.json";
import { useId, useRef, useState } from "react";
import useChartTip from "../ui/useChartTip";
import { Durations, Found } from "./Bars";
import Detail from "./Detail";
import Foot from "./Foot";
import Header from "./Header";
import History from "./History";
import Legend from "./Legend";
import Matrix from "./Matrix";
import type { VmData } from "./model";
import Selector from "./Selector";
import TipContent from "./TipContent";
import s from "./VerificationMatrix.module.css";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as VmData;

/** START_PIN is the commit shown first: the current reference commit. */
const START_PIN = DATA.pins.findIndex((p) => p.sha === DATA.hero.ref);

/**
 * "Verification matrix": the headline run count and three tiles, the
 * reference-commit selector, the legend, the tools x hosts matrix for the
 * selected commit with its note, the lane history across all commits, the
 * selected cell's detail, the wall-time bars and the what-the-lanes-found
 * bars. The selected commit and cell link the selector, matrix, history and
 * detail panel.
 */
export default function Body() {
  const boxRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(boxRef);
  const tipId = `vm${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-tip`;
  const [pin, setPin] = useState(START_PIN);
  const [sel, setSel] = useState<number | null>(null);
  /** Selects a commit and clears the selected cell (it belongs to another commit). */
  const choosePin = (i: number) => {
    setPin(i);
    setSel(null);
  };
  /** Jumps to a run's commit and selects its cell. */
  const jump = (k: number) => {
    setPin(DATA.runs[k].pin);
    setSel(k);
  };

  return (
    <div ref={boxRef} className={s.root}>
      <Header data={DATA} />
      <Selector data={DATA} pin={pin} onPin={choosePin} />
      <Legend />
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for all four hosts →
      </p>
      <div className={s.mxw}>
        <Matrix data={DATA} pin={pin} sel={sel} onSelect={setSel} tip={tip} />
      </div>
      <p className={s.note}>
        <CommitText text={DATA.pins[pin].note} />
      </p>
      <div className={s.low}>
        <div className={s.pn}>
          <h4>Lane history across reference commits</h4>
          <p className={s.scrollHint} aria-hidden="true">
            ← scroll sideways for all {DATA.pins.length} commits →
          </p>
          <div className={s.histScroll}>
            <History data={DATA} pin={pin} sel={sel} onJump={jump} tip={tip} />
          </div>
        </div>
        <Detail data={DATA} sel={sel} />
      </div>
      <div className={`${s.low} ${s.even}`}>
        <div className={s.pn}>
          <h4>
            Wall time of the first complete run of every lane, at{" "}
            <CommitLink hash={DATA.durationsRef} /> (hours)
          </h4>
          <Durations data={DATA} />
        </div>
        <div className={s.pn}>
          <h4>What the lanes found</h4>
          <Found data={DATA} />
        </div>
      </div>
      <Foot data={DATA} />
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
