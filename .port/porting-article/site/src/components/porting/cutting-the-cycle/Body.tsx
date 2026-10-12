"use client";

import data from "@/app/porting/data/cutting-the-cycle.json";
import {
  type KeyboardEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { type ReplayState, replayStart, replayStep } from "../lib/replay";
import useChartTip from "../ui/useChartTip";
import useReducedMotion from "../ui/useReducedMotion";
import CrateLayer from "./CrateLayer";
import s from "./CuttingTheCycle.module.css";
import CycleLayer from "./CycleLayer";
import { VIEW_H, VIEW_W } from "./geometry";
import Insights from "./Insights";
import Legend from "./Legend";
import { type CcData, replayFrame } from "./model";
import ReplayRow from "./ReplayRow";
import SidePanel from "./SidePanel";
import TipContent from "./TipContent";
import useGraphKeys from "./useGraphKeys";
import useMorph from "./useMorph";
import { buildView } from "./view";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as CcData;
/** STAGES are the four tab names, in order. */
const STAGES = ["The cycle", "The 14 cuts", "The crate graph", "Replay"];
/** REPLAY_MS is the replay speed: milliseconds per day. */
const REPLAY_MS = 90;

/**
 * The "Cutting the cycle" widget: Ghostty's 345-file import cycle, the 14
 * cuts that broke it, the morph into the 61-package crate graph and the
 * day-by-day replay of that graph's growth, with side panels and insights.
 */
export default function Body() {
  const reduced = useReducedMotion();
  const v = useMemo(() => buildView(DATA), []);
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const last = DATA.replay.n - 1;
  const [stage, setStage] = useState(1);
  const [example, setExample] = useState<number | null>(null);
  const [locked, setLocked] = useState<number | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [chain, setChain] = useState(false);
  const [layer, setLayer] = useState(false);
  const [replay, setReplay] = useState<ReplayState>(() =>
    replayStart(reduced, last),
  );
  const rootRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const dotsRef = useRef<SVGGElement>(null);
  const tip = useChartTip(rootRef);
  const morph = useMorph(v, dotsRef, setStage);

  // With reduced motion the replay always rests on its final day.
  useEffect(() => {
    if (reduced) setReplay(replayStart(true, last));
  }, [reduced, last]);

  // Advance the replay one day per tick.
  useEffect(() => {
    if (!replay.playing) return;
    const timer = setInterval(
      () => setReplay((r) => replayStep(r, last)),
      REPLAY_MS,
    );
    return () => clearInterval(timer);
  }, [replay.playing, last]);

  const frame = useMemo(
    () => (stage === 4 ? replayFrame(DATA, replay.index) : null),
    [stage, replay.index],
  );
  /** Selects crate i, or clears the selection when i is already selected (Enter on the graph). */
  const toggleSelected = (i: number) =>
    setSelected((x) => (x === i ? null : i));
  const keys = useGraphKeys(v, stage, frame, svgRef, tip, toggleSelected);
  const shown = tip.shown;
  /** Returns the index in shown when it is a tooltip id with prefix p, else null. */
  const pick = (p: string) =>
    shown?.[0] === p ? Number(shown.slice(1)) : null;
  const file = stage <= 2 ? pick("f") : null;
  const cut = stage === 2 ? (pick("k") ?? locked) : null;
  const hovered = stage >= 3 ? pick("n") : null;
  const edge = stage >= 3 ? pick("e") : null;

  /** Switches to stage n (morphing the dots into crates when leaving the file view); returns whether it switched. */
  const goStage = (n: number): boolean => {
    if (morph.active || n === stage) return false;
    tip.clear();
    setLocked(null);
    setExample(null);
    keys.reset();
    setReplay((r) => ({ ...r, playing: false }));
    if (n === 4) {
      setChain(false);
      setLayer(false);
    }
    if (stage <= 2 && n >= 3 && !reduced) morph.start(n);
    else setStage(n);
    return true;
  };

  /** Keyboard on the tab list: Left/Right move between stages. */
  const onTabKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = ((stage - 1 + d + 4) % 4) + 1;
    if (goStage(n)) document.getElementById(`${uid}-tab${n}`)?.focus();
  };

  /** Shows file i's tooltip at the pointer (mouse hover). */
  const onFileEnter = (i: number, x: number, y: number) =>
    tip.hover(`f${i}`, { x, y });
  /** Pins or unpins file i's tooltip (tap or click). */
  const onFileTap = (i: number, x: number, y: number) =>
    tip.tap(`f${i}`, { x, y });
  /** Shows crate i's tooltip at the pointer (mouse hover). */
  const onCrateEnter = (i: number, x: number, y: number) =>
    tip.hover(`n${i}`, { x, y });
  /** Pins crate i's tooltip and, in the crate graph, selects it (tap or click). */
  const onCrateTap = (i: number, x: number, y: number) => {
    tip.tap(`n${i}`, { x, y });
    if (stage === 3) setSelected(i);
  };
  /** Shows crate edge i's tooltip at the pointer (mouse hover). */
  const onEdgeEnter = (i: number, x: number, y: number) =>
    tip.hover(`e${i}`, { x, y });
  /** Pins or unpins crate edge i's tooltip (tap or click). */
  const onEdgeTap = (i: number, x: number, y: number) =>
    tip.tap(`e${i}`, { x, y });
  /** Plays the replay from the paused day (from the start at the end); with reduced motion shows the final day. */
  const onPlay = () =>
    setReplay((r) =>
      reduced
        ? replayStart(true, last)
        : { index: r.index >= last ? 0 : r.index, playing: true },
    );
  /** Pauses the replay on the current day. */
  const onPause = () => setReplay((r) => ({ ...r, playing: false }));
  /** Jumps the replay to day index i and stops it. */
  const onSeek = (i: number) => setReplay({ index: i, playing: false });

  const fileView = stage <= 2 || morph.active;
  return (
    <div ref={rootRef} className={s.root}>
      <div className={s.head}>
        <div className={s.big}>
          {DATA.zig.stats.scc} <span>→</span> {DATA.crates.nodes.length}
        </div>
        <div className={s.sub}>
          Zig files in one import cycle → acyclic Cargo packages,{" "}
          {v.lay.maxDepth} levels deep
        </div>
      </div>
      <div
        className={s.tabs}
        role="tablist"
        aria-label="Stages"
        onKeyDown={onTabKey}
      >
        {STAGES.map((name, i) => (
          <button
            key={name}
            id={`${uid}-tab${i + 1}`}
            type="button"
            role="tab"
            className={s.tab}
            aria-selected={stage === i + 1}
            aria-controls={`${uid}-panel`}
            tabIndex={stage === i + 1 ? 0 : -1}
            onClick={() => goStage(i + 1)}
          >
            <i>{i + 1}</i>
            {name}
          </button>
        ))}
      </div>
      <div
        className={s.body}
        role="tabpanel"
        id={`${uid}-panel`}
        aria-labelledby={`${uid}-tab${stage}`}
      >
        <div className={s.main}>
          <p className={s.scrollHint} aria-hidden="true">
            ← scroll sideways for the full graph →
          </p>
          <div className={s.scroll}>
            <svg
              ref={svgRef}
              className={s.svg}
              viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
              role="application"
              aria-roledescription={fileView ? "import graph" : "crate graph"}
              // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application"; the graph handles arrows, Enter and Escape itself
              tabIndex={0}
              aria-label={
                fileView
                  ? `The ${DATA.zig.stats.scc} Zig files of the import cycle, by directory. Use the arrow keys to step through files clockwise, Enter to pin the details, Escape to close them.`
                  : "The crate graph in depth columns. Use the arrow keys to step through crates, Enter to pin the details and select the crate, Escape to close them."
              }
              aria-describedby={`${uid}-tip`}
              onKeyDown={keys.onKeyDown}
              onFocus={keys.onFocus}
              onBlur={keys.onBlur}
            >
              {fileView ? (
                <CycleLayer
                  v={v}
                  stage={stage}
                  file={file}
                  cut={cut}
                  example={example}
                  morphing={morph.active}
                  dotsRef={dotsRef}
                  onEnter={onFileEnter}
                  onLeave={tip.leave}
                  onTap={onFileTap}
                />
              ) : (
                <CrateLayer
                  v={v}
                  stage={stage}
                  selected={selected}
                  hovered={hovered}
                  edge={edge}
                  showChain={chain}
                  showLayer={layer}
                  frame={frame}
                  onEnterCrate={onCrateEnter}
                  onTapCrate={onCrateTap}
                  onEnterEdge={onEdgeEnter}
                  onTapEdge={onEdgeTap}
                  onLeave={tip.leave}
                />
              )}
            </svg>
          </div>
          <Legend stage={stage} />
          {frame ? (
            <ReplayRow
              replay={replay}
              last={last}
              frame={frame}
              d0={DATA.replay.d0}
              packages={DATA.crates.nodes.length}
              onPlay={onPlay}
              onPause={onPause}
              onSeek={onSeek}
            />
          ) : null}
        </div>
        <div className={s.side}>
          <SidePanel
            v={v}
            stage={stage}
            tip={tip}
            example={example}
            setExample={setExample}
            locked={locked}
            setLocked={setLocked}
            selected={selected}
            setSelected={setSelected}
            chain={chain}
            setChain={setChain}
            layer={layer}
            setLayer={setLayer}
          />
        </div>
      </div>
      <Insights v={v} />
      <div
        ref={tip.tipRef}
        id={`${uid}-tip`}
        role="tooltip"
        className={s.tip}
        hidden={!shown}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {shown ? <TipContent v={v} id={shown} frame={frame} /> : null}
      </div>
    </div>
  );
}
