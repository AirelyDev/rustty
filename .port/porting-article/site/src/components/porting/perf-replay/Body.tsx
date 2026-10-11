"use client";

import data from "@/app/porting/data/perf-replay.json";
import {
  type KeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { PALETTE } from "../lib/palette";
import { type ReplayState, replayStart, replayStep } from "../lib/replay";
import ReplayControls from "../ui/ReplayControls";
import TerminalFrame from "../ui/TerminalFrame";
import useChartTip from "../ui/useChartTip";
import useReducedMotion from "../ui/useReducedMotion";
import Chapters from "./Chapters";
import Dumbbell from "./Dumbbell";
import Hero from "./Hero";
import { useNarrow, useTween } from "./hooks";
import { Insights, MemoryPanel, Method } from "./Lower";
import { cmdTitle, finalOrder, hhmm } from "./model";
import s from "./PerfReplay.module.css";
import { LtoPanel, Negatives } from "./SideBranches";
import StepPanel from "./StepPanel";
import Timeline, { TICK_COLOUR } from "./Timeline";
import TipContent from "./TipContent";
import { tipFor } from "./tips";
import type { PrData } from "./types";
import Waterfalls from "./Waterfalls";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as PrData;

/** STEP_MS is the autoplay delay per replay step (the mockup's 1.7 s). */
const STEP_MS = 1700;

/** TICK_LEGEND names the commit tick colours of the timeline. */
const TICK_LEGEND: [keyof typeof TICK_COLOUR, string][] = [
  ["perf", "perf fix"],
  ["revert", "revert"],
  ["merge", "merge"],
  ["records", "records"],
  ["parity", "parity fix"],
];

/**
 * "From 0 of 14 to 14 of 14 in one day": the step-by-step replay of every
 * recorded benchmark sweep of 15–16 July against same-session Zig (in a
 * terminal frame titled with the real recorded command), its timeline, the
 * key-benchmark waterfalls, the two side branches (LTO, falsified
 * optimisations), the chapters after the day, memory and size, and the
 * computed insights.
 */
export default function Body() {
  const reduced = useReducedMotion();
  const narrow = useNarrow();
  const last = DATA.stages.length - 1;
  const order = useMemo(() => finalOrder(DATA), []);
  const [replay, setReplay] = useState<ReplayState>(() =>
    replayStart(reduced, last),
  );
  const [bench, setBench] = useState(order[0]);
  const [chapter, setChapter] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(rootRef);
  const cur = replay.index;
  const tween = useTween(cur, reduced);

  // With reduced motion the replay rests on its final step.
  useEffect(() => {
    if (reduced) setReplay(replayStart(true, last));
  }, [reduced, last]);

  // Advance one step per tick while playing.
  useEffect(() => {
    if (!replay.playing) return;
    const t = setTimeout(() => setReplay((r) => replayStep(r, last)), STEP_MS);
    return () => clearTimeout(t);
  }, [replay, last]);

  /** Jumps to step i (clamped) and stops autoplay. */
  const seek = (i: number) =>
    setReplay({ index: Math.max(0, Math.min(last, i)), playing: false });

  /** Left/Right anywhere in the replay frame (outside the scrubber and charts) step the replay. */
  const onFrameKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    if (e.key === "ArrowRight") seek(cur + 1);
    else if (e.key === "ArrowLeft") seek(cur - 1);
    else return;
    e.preventDefault();
  };

  const shown = tip.shown ? tipFor(DATA, tip.shown, cur) : null;
  const st = DATA.stages[cur];

  return (
    <div ref={rootRef} className={s.root}>
      <Hero data={DATA} />
      <TerminalFrame title={cmdTitle(DATA, cur, bench)}>
        {/* biome-ignore lint/a11y/noStaticElementInteractions: arrow keys from focused controls inside the frame step the replay */}
        <div onKeyDown={onFrameKey}>
          <div className={s.main}>
            <div>
              <div className={s.legend}>
                <span>
                  <i className={s.lgDot} style={{ background: PALETTE.rust }} />
                  Rustty faster
                </span>
                <span>
                  <i className={s.lgDot} style={{ background: PALETTE.fail }} />
                  slower
                </span>
                <span>
                  <i
                    className={s.lgDot}
                    style={{ background: PALETTE.gray5 }}
                  />
                  tie (±1.5%)
                </span>
                <span>
                  <i className={s.lgRing} />
                  07-15 start
                </span>
                <span>
                  <i className={`${s.lgRing} ${s.lgGold}`} />
                  moved outside spread
                </span>
                <span>
                  <i className={s.lgBar} />
                  max/min spread (ratio range)
                </span>
                <span>
                  <i className={s.lgZig} />
                  Zig = 1.0
                </span>
              </div>
              <Dumbbell
                data={DATA}
                order={order}
                tween={tween}
                narrow={narrow}
                tip={tip}
                onBench={setBench}
              />
            </div>
            <StepPanel data={DATA} cur={cur} />
          </div>
          <div className={s.ctrl}>
            <button
              type="button"
              className={s.btn}
              aria-label="Previous step"
              onClick={() => seek(cur - 1)}
            >
              ‹
            </button>
            <button
              type="button"
              className={s.btn}
              aria-label="Next step"
              onClick={() => seek(cur + 1)}
            >
              ›
            </button>
            <ReplayControls
              index={cur}
              lastIndex={last}
              playing={replay.playing}
              onPlay={() =>
                setReplay((r) =>
                  reduced
                    ? replayStart(true, last)
                    : // Resume from the paused step; start over only from the end.
                      { index: r.index >= last ? 0 : r.index, playing: true },
                )
              }
              onPause={() => setReplay((r) => ({ ...r, playing: false }))}
              onSeek={seek}
              label={`step ${cur + 1}/${last + 1} · ${hhmm(st.t)}`}
            />
          </div>
          <p className={s.scrollHint} aria-hidden="true">
            ← scroll sideways for the full timeline →
          </p>
          <div className={s.scroll}>
            <Timeline data={DATA} cur={cur} tip={tip} onStep={seek} />
          </div>
          <div className={s.legend}>
            {TICK_LEGEND.map(([k, t]) => (
              <span key={k}>
                <i
                  className={s.lgTick}
                  style={{ background: TICK_COLOUR[k] }}
                />
                {t}
              </span>
            ))}
            <span>× measured worse</span>
          </div>
        </div>
      </TerminalFrame>

      <div className={s.sec}>
        Who moved the key benchmarks · ms per official sweep, dashed =
        same-session Zig · steps after the replay cursor are dimmed
      </div>
      <Waterfalls data={DATA} cur={cur} tip={tip} />

      <div className={s.two}>
        <LtoPanel data={DATA} tip={tip} />
        <Negatives data={DATA} />
      </div>

      <Chapters
        data={DATA}
        sel={chapter}
        onSel={setChapter}
        narrow={narrow}
        tip={tip}
      />

      <div className={s.two}>
        <MemoryPanel data={DATA} narrow={narrow} tip={tip} />
        <Insights data={DATA} />
      </div>
      <Method data={DATA} />

      <div
        ref={tip.tipRef}
        role="tooltip"
        className={s.tip}
        hidden={!shown}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {shown ? <TipContent tip={shown} /> : null}
      </div>
    </div>
  );
}
