"use client";

import data from "@/app/porting/data/cost.json";
import { useId, useRef, useState } from "react";
import useChartTip from "../ui/useChartTip";
import Codex from "./Codex";
import Context from "./Context";
import s from "./Cost.module.css";
import Eras from "./Eras";
import Header from "./Header";
import Insights from "./Insights";
import type { CostData, Scope, Unit } from "./model";
import Paid from "./Paid";
import TipContent from "./TipContent";
import Tokens from "./Tokens";
import Weekly from "./Weekly";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as CostData;

/**
 * "What it cost": the unit and scope toggles, the paid and API-equivalent
 * headline numbers with four tiles, the weekly quota-week instrument (bars,
 * modeled lost window, ceiling band, limit signals, trailers, coverage),
 * then paid vs list price, token anatomy, efficiency over eras with context
 * and concurrency, the Codex audit meter and the computed insights. One
 * floating tooltip serves every chart (hover, tap to pin, keyboard).
 */
export default function Body() {
  const boxRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(boxRef);
  const uid = `cost${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const tipId = `${uid}-tip`;
  const [u, setU] = useState<Unit>("usd");
  const [sc, setSc] = useState<Scope>("rustty");
  /** Switches the unit and closes any tooltip (its values change). */
  const pickUnit = (v: Unit) => {
    setU(v);
    tip.clear();
  };
  /** Switches the scope and closes any tooltip (its values change). */
  const pickScope = (v: Scope) => {
    setSc(v);
    tip.clear();
  };
  const props = { d: DATA, tip, tipId };
  return (
    <div ref={boxRef} className={s.root}>
      <Header d={DATA} u={u} sc={sc} onUnit={pickUnit} onScope={pickScope} />
      <Weekly {...props} sc={sc} u={u} uid={uid} />
      <div className={s.grid}>
        <Paid {...props} uid={uid} />
        <Tokens {...props} sc={sc} />
        <div className={s.panel}>
          <div className={s.pt}>
            <h4>Efficiency over eras</h4>
            <span>Rustty · hollow = modeled cost</span>
          </div>
          <Eras {...props} />
          <Context {...props} />
        </div>
        <Codex {...props} />
      </div>
      <Insights d={DATA} />
      <div
        ref={tip.tipRef}
        id={tipId}
        role="tooltip"
        className={s.tip}
        hidden={!tip.shown}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {tip.shown ? (
          <TipContent id={tip.shown} d={DATA} sc={sc} u={u} />
        ) : null}
      </div>
    </div>
  );
}
