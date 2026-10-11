"use client";

import data from "@/app/porting/data/test-contract.json";
import { useMemo } from "react";
import useReducedMotion from "../ui/useReducedMotion";
import GridPanel from "./GridPanel";
import {
  allDayStats,
  doubled,
  expandMods,
  gridInsights,
  gridOrder,
  replayEvents,
  scatterPoints,
  syncTable,
  type TcData,
} from "./model";
import MovingPanel from "./MovingPanel";
import ScatterPanel from "./ScatterPanel";
import SkipPanel from "./SkipPanel";
import s from "./TestContract.module.css";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as TcData;

/**
 * "The test contract" widget: the square-by-square replay of the Zig test
 * contract over 110 days, the per-module parity scatter at the end, the
 * contract growing across eight upstream syncs, and the silent-skip census.
 */
export default function Body() {
  const reduced = useReducedMotion();
  const mods = useMemo(() => expandMods(DATA), []);
  const stats = useMemo(() => allDayStats(DATA, mods), [mods]);
  const order = useMemo(() => gridOrder(mods), [mods]);
  const events = useMemo(() => replayEvents(DATA, mods), [mods]);
  const tiles = useMemo(() => gridInsights(DATA, mods), [mods]);
  const points = useMemo(() => scatterPoints(DATA, mods), [mods]);
  const two = useMemo(() => doubled(DATA, mods), [mods]);
  const table = useMemo(() => syncTable(DATA, mods), [mods]);

  return (
    <div className={s.root}>
      <GridPanel
        data={DATA}
        order={order}
        stats={stats}
        events={events}
        tiles={tiles}
        reduced={reduced}
      />
      <div className={s.two}>
        <ScatterPanel points={points} two={two} dates={DATA.dates} />
        <MovingPanel data={DATA} stats={stats} table={table} />
      </div>
      <SkipPanel skip={DATA.skip} />
    </div>
  );
}
