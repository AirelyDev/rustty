"use client";

import data from "@/app/porting/data/three-reviewers.json";
import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { type ReplayState, replayStart, replayStep } from "../lib/replay";
import useChartTip from "../ui/useChartTip";
import useReducedMotion from "../ui/useReducedMotion";
import Bottom from "./Bottom";
import MiriStage from "./MiriStage";
import { bugModels, eventBug, fixDate, headline, insights } from "./model";
import Rail from "./Rail";
import Rich from "./Rich";
import { FixStage, ImplStage, ReviewStage } from "./Stages";
import s from "./ThreeReviewers.module.css";
import type { Model, TrData } from "./types";

/** DATA is the widget's generated data file, typed for the view-model. */
const DATA = data as unknown as TrData;

/** STAGES are the four replay stages: tab label. */
const STAGES = ["Implementer", "Reviewer", "Miri", "Fix"];

/** STEP_MS is the autoplay delay per borrow-stack step (the mockup's 1.7 s). */
const STEP_MS = 1700;

/** LETTERS are the replayed bugs' labels. */
const LETTERS = ["A", "B", "C", "D"];

/**
 * The "Three reviewers" widget: four real port memory-safety bugs, each
 * replayed through the ported code, the reviewer (or what caught it), Miri's
 * borrow stack under Stacked and Tree Borrows with its real output, and the
 * fix; above them, all port memory-safety events on a rail with computed
 * insights, and below, the rule each bug produced and how often its pattern
 * appears now.
 */
export default function Body() {
  const reduced = useReducedMotion();
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const [bug, setBug] = useState(0);
  const [stage, setStage] = useState(0);
  const [model, setModel] = useState<Model>("sb");
  const [replay, setReplay] = useState<ReplayState>({
    index: 0,
    playing: false,
  });
  const rootRef = useRef<HTMLDivElement>(null);
  const tabsRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(rootRef);
  const b = DATA.bugs[bug];
  const last = b.miri[model].length - 1;
  const hl = headline(DATA);

  // Reduced motion: replays open on their final (UB) step.
  useEffect(() => {
    setReplay(replayStart(reduced, last));
  }, [reduced, last]);

  // Auto-advance one step per tick while playing.
  useEffect(() => {
    if (!replay.playing) return;
    const t = setTimeout(() => setReplay((r) => replayStep(r, last)), STEP_MS);
    return () => clearTimeout(t);
  }, [replay, last]);

  /** Resets the replay for a new bug, stage or model (final step under reduced motion). */
  const resetFor = (steps: number) =>
    setReplay(replayStart(reduced, steps - 1));
  /** Selects bug i (bug cards, rail dots, A–D markers). */
  const pickBug = (i: number) => {
    setBug(i);
    resetFor(DATA.bugs[i].miri[model].length);
  };
  /** Shows stage i (clamped) and focuses its tab when asked. */
  const pickStage = (i: number, focus = false) => {
    const k = Math.max(0, Math.min(STAGES.length - 1, i));
    setStage(k);
    resetFor(b.miri[model].length);
    if (focus)
      tabsRef.current
        ?.querySelector<HTMLButtonElement>(`[data-i="${k}"]`)
        ?.focus();
  };
  /** Switches the aliasing model of the Miri replay. */
  const pickModel = (m: Model) => {
    setModel(m);
    resetFor(b.miri[m].length);
  };
  /** Left/Right on the tab bar change stage. */
  const onTabKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "ArrowRight") pickStage(stage + 1, true);
    if (e.key === "ArrowLeft") pickStage(stage - 1, true);
  };

  const shownEv = tip.shown?.startsWith("ev:")
    ? DATA.timeline[Number(tip.shown.slice(3))]
    : null;

  return (
    <div ref={rootRef} className={s.root}>
      <div className={s.hl}>
        <span className={s.big}>
          {hl.miri} <small>of</small> {hl.total}
        </span>
        <span className={s.sub}>
          port-introduced memory-safety, concurrency and ABI defects were found
          by Miri or a Miri-proven audit ({hl.events} curated events, {hl.first}{" "}
          → {hl.last.slice(5)})
        </span>
      </div>

      <fieldset className={s.bugs} aria-label="Choose a bug">
        {DATA.bugs.map((x, i) => (
          <button
            key={x.key}
            type="button"
            className={s.bug}
            aria-pressed={i === bug}
            onClick={() => pickBug(i)}
          >
            <span className={s.bugD}>
              {LETTERS[i]} · fixed {fixDate(x)} · {x.crate}
            </span>
            <span className={s.bugT}>{x.short}</span>
            <span className={s.bugM}>
              <Rich text={x.title} />
            </span>
            <span className={s.chips}>
              {bugModels(x).sb ? (
                <span className={`${s.chip} ${s.chipSb}`}>SB UB</span>
              ) : null}
              {bugModels(x).tb ? (
                <span className={`${s.chip} ${s.chipTb}`}>TB UB</span>
              ) : null}
              <span
                className={
                  x.review.kind === "none" ? s.chip : `${s.chip} ${s.chipRv}`
                }
              >
                {x.caughtBy}
              </span>
            </span>
          </button>
        ))}
      </fieldset>

      <div className={s.sec}>
        <span>
          Where it sits: all {DATA.timeline.length} events, by what found them
        </span>
        <span>
          dot area = defects in the event · filled = production code · hollow =
          test or bench
        </span>
      </div>
      <Rail data={DATA} bug={bug} onBug={pickBug} tip={tip} />
      <div className={s.ins}>
        {insights(DATA).map((t) => (
          <div key={t.value}>
            <b>{t.value}</b>
            {t.text}
          </div>
        ))}
      </div>
      <div
        ref={tabsRef}
        className={s.stagebar}
        role="tablist"
        aria-label="Replay stage"
        onKeyDown={onTabKey}
      >
        {STAGES.map((name, i) => (
          <span key={name} className={s.tabItem}>
            {i ? (
              <span className={s.arrow} aria-hidden="true">
                →
              </span>
            ) : null}
            <button
              type="button"
              role="tab"
              id={`${uid}-tab${i}`}
              data-i={i}
              className={s.tab}
              aria-selected={i === stage}
              aria-controls={`${uid}-stage`}
              tabIndex={i === stage ? 0 : -1}
              onClick={() => pickStage(i)}
            >
              <span className={s.tabN}>{i + 1}</span>
              {name}
            </button>
          </span>
        ))}
        <span className={s.spacer} />
        <button
          type="button"
          className={s.btn}
          aria-label="Previous stage"
          onClick={() => pickStage(stage - 1)}
        >
          ‹
        </button>
        <button
          type="button"
          className={s.btn}
          aria-label="Next stage"
          onClick={() => pickStage(stage + 1)}
        >
          ›
        </button>
      </div>
      <div
        role="tabpanel"
        id={`${uid}-stage`}
        aria-labelledby={`${uid}-tab${stage}`}
      >
        {stage === 0 ? <ImplStage bug={b} /> : null}
        {stage === 1 ? <ReviewStage bug={b} /> : null}
        {stage === 2 ? (
          <MiriStage
            bug={b}
            model={model}
            step={replay.index}
            playing={replay.playing}
            onModel={pickModel}
            onSeek={(i) =>
              setReplay({
                index: Math.max(0, Math.min(last, i)),
                playing: false,
              })
            }
            onPlay={() =>
              setReplay((r) =>
                reduced
                  ? replayStart(true, last)
                  : // Resume from the paused step; start over only from the end.
                    { index: r.index >= last ? 0 : r.index, playing: true },
              )
            }
            onPause={() => setReplay((r) => ({ ...r, playing: false }))}
          />
        ) : null}
        {stage === 3 ? <FixStage bug={b} /> : null}
      </div>

      <Bottom bug={b} />

      <div
        ref={tip.tipRef}
        role="tooltip"
        className={s.tip}
        hidden={!shownEv}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {shownEv ? (
          <>
            <div className={s.tipK}>
              {shownEv.d}
              {shownEv.ref ? ` · ${shownEv.ref}` : ""} · {shownEv.tool}
            </div>
            <b>{shownEv.area}</b> — {shownEv.n} defect{shownEv.n > 1 ? "s" : ""}
            <br />
            {shownEv.sum}
            <div className={`${s.tipK} ${s.tipFoot}`}>
              {shownEv.cls} · {shownEv.scope}
              {eventBug(DATA, shownEv) >= 0
                ? ` · replayed as ${LETTERS[eventBug(DATA, shownEv)]}`
                : ""}
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}
