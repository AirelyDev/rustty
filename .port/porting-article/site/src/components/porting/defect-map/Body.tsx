"use client";

import data from "@/app/porting/data/defect-map.json";
import { useCallback, useId, useMemo, useRef, useState } from "react";
import useChartTip from "../ui/useChartTip";
import Detail, { type DetailTexts } from "./Detail";
import Header from "./Header";
import { layoutMap } from "./layout";
import MapView from "./MapView";
import MatrixPanel from "./MatrixPanel";
import {
  ALL_FILTER,
  CLASS_KEYS,
  type DmData,
  type DmDetails,
  densityRows,
  type Filter,
  headline,
  matrix,
  METHOD_KEYS,
  statusRows,
  terminalShare,
  toggleChip,
  waffle,
} from "./model";
import SidePanel from "./SidePanel";
import TipContent from "./TipContent";
import s from "./DefectMap.module.css";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as DmData;

/** Loads the detail texts (mechanism, ReleaseFast, fix, upstream) keyed by ledger number. */
async function loadDetails(): Promise<Map<number, NonNullable<DetailTexts>>> {
  const mod = (await import(
    "@/app/porting/data/defect-map-details.json"
  )) as unknown as {
    default: DmDetails;
  };
  return new Map(
    mod.default.d.map(([n, mech, rfast, fix, us]) => [
      n,
      { mech, rfast, fix, us },
    ]),
  );
}

/**
 * "Defects on the map of Ghostty": the defect-count header and filter chips, the
 * treemap of Ghostty's Zig source with one mark per recorded defect, the
 * detail panel (its long texts load on the first selection), the class ×
 * method matrix, the ReleaseFast waffle, the upstream status bar and the
 * density list.
 */
export default function Body() {
  const boxRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(boxRef);
  const tipId = `dm${useId().replace(/[^a-zA-Z0-9_-]/g, "")}-tip`;
  const [filter, setFilter] = useState<Filter>(ALL_FILTER);
  const [selected, setSelected] = useState<number | null>(null);
  const [dir, setDir] = useState<string | null>(null);
  const [details, setDetails] = useState<Map<
    number,
    NonNullable<DetailTexts>
  > | null>(null);
  const [failed, setFailed] = useState(false);
  const loading = useRef(false);
  const layout = useMemo(() => layoutMap(DATA, CLASS_KEYS), []);
  const head = useMemo(() => headline(DATA), []);
  const mx = useMemo(() => matrix(DATA.defs), []);
  const waf = useMemo(() => waffle(DATA.defs), []);
  const status = useMemo(() => statusRows(DATA.defs), []);
  const density = useMemo(() => densityRows(DATA), []);
  const term = useMemo(() => terminalShare(DATA), []);

  /** Opens defect i in the detail panel, fetching the detail texts on first use. */
  const select = useCallback((i: number) => {
    setSelected(i);
    setFailed(false);
    if (loading.current) return;
    loading.current = true;
    loadDetails().then(setDetails, () => {
      loading.current = false;
      setFailed(true);
    });
  }, []);

  const def = selected === null ? null : DATA.defs[selected];
  return (
    <div ref={boxRef} className={s.root}>
      <Header
        head={head}
        defs={DATA.defs}
        filter={filter}
        onClass={(k) =>
          setFilter((f) => ({ ...f, cls: toggleChip(f.cls, k, CLASS_KEYS) }))
        }
        onMethod={(k) =>
          setFilter((f) => ({ ...f, met: toggleChip(f.met, k, METHOD_KEYS) }))
        }
        onReset={() => setFilter(ALL_FILTER)}
      />
      <MapView
        data={DATA}
        layout={layout}
        filter={filter}
        selected={selected}
        dir={dir}
        tip={tip}
        tipId={tipId}
        onSelect={select}
      />
      <Detail
        def={def}
        texts={def ? (details?.get(def.n) ?? null) : null}
        failed={failed}
        tmux={{ n: term.tmux, lines: term.tmuxLines }}
      />
      <div className={s.panels}>
        <MatrixPanel
          mx={mx}
          head={head}
          filter={filter}
          onCell={(cell) => setFilter((f) => ({ ...f, cell }))}
          tip={tip}
        />
        <SidePanel
          defs={DATA.defs}
          waffle={waf}
          status={status}
          density={density}
          reported={DATA.reported}
          term={term}
          filter={filter}
          selected={selected}
          dir={dir}
          tip={tip}
          onSelect={select}
          onRf={(k) =>
            setFilter((f) => ({
              ...f,
              rf: f.rf === k ? null : k,
              cls: ALL_FILTER.cls,
            }))
          }
          onStatus={(k) =>
            setFilter((f) => ({ ...f, status: f.status === k ? null : k }))
          }
          onDir={setDir}
        />
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
          <TipContent id={tip.shown} data={DATA} mx={mx} status={status} />
        ) : null}
      </div>
    </div>
  );
}
