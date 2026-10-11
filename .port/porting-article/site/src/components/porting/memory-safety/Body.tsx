"use client";

import data from "@/app/porting/data/memory-safety.json";
import { useId, useMemo, useRef, useState } from "react";
import useChartTip from "../ui/useChartTip";
import Detail from "./Detail";
import HeatTable from "./HeatTable";
import Header from "./Header";
import { type Group, headline, heat as heatOf, type MsData } from "./model";
import RuleList from "./RuleList";
import Swimlane from "./Swimlane";
import TipContent from "./TipContent";
import s from "./MemorySafety.module.css";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as MsData;

/**
 * "Memory-safety bugs the port introduced, and what caught them": the defect-count
 * header, the swimlane timeline (rows by detector or crate area) with the
 * eight rule flags and the cumulative curves, the crate x bug-class table that
 * filters the lanes, and the event detail panel.
 */
export default function Body() {
  const boxRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const tip = useChartTip(boxRef);
  const tipId = `ms${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-tip`;
  const [group, setGroup] = useState<Group>("lane");
  const [filter, setFilter] = useState<readonly [string, string] | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [hoverRule, setHoverRule] = useState<number | null>(null);
  const head = useMemo(() => headline(DATA.events), []);
  const heat = useMemo(() => heatOf(DATA.events), []);

  return (
    <div ref={boxRef} className={s.root}>
      <Header head={head} group={group} onGroup={setGroup} />
      <Swimlane
        data={DATA}
        group={group}
        filter={filter}
        selected={selected}
        hoverRule={hoverRule}
        tip={tip}
        tipId={tipId}
        onSelect={setSelected}
        onRule={setHoverRule}
        svgRef={svgRef}
      />
      <RuleList rules={DATA.rules} hover={hoverRule} onHover={setHoverRule} />
      <div className={s.low}>
        <div className={s.panel}>
          <h4>Crate area × bug class (defects)</h4>
          <div className={s.heatScroll}>
            <HeatTable
              heat={heat}
              filter={filter}
              onFilter={setFilter}
              tip={tip}
            />
          </div>
          <p className={s.note}>
            Click a cell to highlight its events. Aliasing and
            pointer-provenance undefined behaviour make up {head.aliasOrProv} of
            the {head.defects} defects ({head.alias} aliasing, {head.prov}{" "}
            provenance; {head.aliasOrProvProd} of them in production code), and
            every crate area has some: Rust's borrow model, not Zig's code, is
            the dominant source.
          </p>
        </div>
        <div className={s.panel}>
          <Detail event={selected === null ? null : DATA.events[selected]} />
        </div>
      </div>
      <div
        ref={tip.tipRef}
        id={tipId}
        role="tooltip"
        className={s.tip}
        hidden={!tip.shown}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {tip.shown ? (
          <TipContent id={tip.shown} data={DATA} heat={heat} />
        ) : null}
      </div>
    </div>
  );
}
