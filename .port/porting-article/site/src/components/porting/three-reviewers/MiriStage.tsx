"use client";

import ReplayControls from "../ui/ReplayControls";
import { CodeBlock } from "./Code";
import { codeMarks, stackAt, stateNote, termHighlighter } from "./model";
import Rich from "./Rich";
import Term from "./Term";
import s from "./ThreeReviewers.module.css";
import type { Bug, Model, StackItem } from "./types";

/** Props for MiriStage; Body owns the replay state. */
type MiriStageProps = {
  bug: Bug;
  model: Model;
  step: number;
  playing: boolean;
  onModel: (m: Model) => void;
  onSeek: (step: number) => void;
  onPlay: () => void;
  onPause: () => void;
};

/** Renders one borrow-stack item or borrow-tree node: tag, permission or state, creator, protector. */
function Item({ it, model }: { it: StackItem; model: Model }) {
  const cls = [s.it, it.st ? s[it.st] : ""].join(" ");
  const permColor =
    {
      Unique: "#d97934",
      SharedRW: "#64d8c6",
      Reserved: "#61aeee",
      Frozen: "#e6c07b",
      Disabled: "#e06c75",
      "Disabled?": "#e06c75",
    }[it.perm] ?? "#858a84";
  return (
    <li
      className={cls}
      style={{ marginLeft: model === "tb" ? (it.depth ?? 0) * 16 : 0 }}
    >
      <span className={s.tag}>&lt;{it.id}&gt;</span>
      <span className={s.perm} style={{ color: permColor }}>
        {it.perm}
      </span>
      <span className={s.lb}>
        <Rich text={it.label} />
        {it.st ? (
          <span className={s.stateNote}> · {stateNote(it, model)}</span>
        ) : null}
      </span>
      {it.prot ? <span className={s.pr}>protected</span> : <span />}
    </li>
  );
}

/**
 * Stage 3: the code Miri executed (current line orange, the UB access red,
 * visited lines shaded) beside the borrow stack (Stacked Borrows, top first)
 * or borrow tree (Tree Borrows), with step controls, the SB/TB toggle and
 * Miri's real output in a terminal titled with the command that produced it.
 */
export default function MiriStage({
  bug,
  model,
  step,
  playing,
  onModel,
  onSeek,
  onPlay,
  onPause,
}: MiriStageProps) {
  const m = bug.miri;
  const steps = m[model];
  const k = Math.min(step, steps.length - 1);
  const cur = steps[k];
  const frame = stackAt(bug, model, k);
  const marks = codeMarks(steps, k);
  const acc = frame.access;
  const last = steps.length - 1;
  const label = `Step ${k + 1} of ${steps.length}${acc ? (acc.ok ? ": access allowed" : ": undefined behaviour") : ""}`;
  return (
    <>
      <div className={s.grid}>
        <div className={s.panel}>
          <div className={s.ph}>
            <span>The code Miri executed</span>
            <i>{model === "sb" ? "Stacked Borrows" : "Tree Borrows"}</i>
          </div>
          {m.map ? (
            <p className={s.note}>
              <Rich text={m.map} />
            </p>
          ) : null}
          <CodeBlock
            segs={m.trace}
            marks={marks}
            label="The code Miri executed"
          />
        </div>
        <div className={s.panel}>
          <div className={s.ph}>
            <span>
              {model === "sb" ? "Borrow stack (top first)" : "Borrow tree"}
            </span>
            <i>
              step {k + 1} / {steps.length}
            </i>
          </div>
          <div className={s.stepctl}>
            <fieldset className={s.modelSet} aria-label="Aliasing model">
              <button
                type="button"
                className={s.btn}
                aria-pressed={model === "sb"}
                onClick={() => onModel("sb")}
              >
                Stacked
              </button>
              <button
                type="button"
                className={s.btn}
                aria-pressed={model === "tb"}
                onClick={() => onModel("tb")}
              >
                Tree
              </button>
            </fieldset>
            <span className={s.spacer} />
            <button
              type="button"
              className={s.btn}
              aria-label="Previous step"
              disabled={k === 0}
              onClick={() => onSeek(k - 1)}
            >
              ‹
            </button>
            <button
              type="button"
              className={s.btn}
              aria-label="Next step"
              disabled={k === last}
              onClick={() => onSeek(k + 1)}
            >
              ›
            </button>
          </div>
          <ReplayControls
            index={k}
            lastIndex={last}
            playing={playing}
            onPlay={onPlay}
            onPause={onPause}
            onSeek={onSeek}
            label={label}
          />
          <div className={s.loc}>
            Location tracked: <Rich text={m.loc} />
          </div>
          {acc ? (
            <div className={`${s.acc} ${acc.ok ? s.accOk : s.accUb}`}>
              {acc.ok ? "✓" : "✗ UB"} <Rich text={acc.text} />
            </div>
          ) : null}
          <ul
            className={s.stack}
            aria-label={
              model === "sb" ? "Borrow stack, top first" : "Borrow tree"
            }
          >
            {frame.items.map((it, i) => (
              <Item key={`${k}-${i}-${it.id}`} it={it} model={model} />
            ))}
          </ul>
          <div className={s.dots} aria-hidden="true">
            {steps.map((x, i) => (
              <span
                key={`${i}-${x.say.length}`}
                className={
                  i <= k ? (x.acc && !x.acc.ok ? s.ub : s.on) : undefined
                }
              />
            ))}
          </div>
          <div className={s.say}>
            <Rich text={cur.say} />
          </div>
        </div>
      </div>
      <Term
        cmd={model === "sb" ? m.cmd : m.cmdTb}
        text={model === "sb" ? m.logSb : m.logTb}
        highlight={termHighlighter(cur.at)}
      />
    </>
  );
}
