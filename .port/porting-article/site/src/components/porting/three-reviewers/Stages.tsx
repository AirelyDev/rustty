"use client";

import CommitLink from "../ui/CommitLink";
import { CodeBlock, DiffBlock } from "./Code";
import { latency, modelsText } from "./model";
import Rich from "./Rich";
import Term from "./Term";
import s from "./ThreeReviewers.module.css";
import type { Bug } from "./types";

/** Renders the bug's life in git (first written → fixed) with the latency, mechanism, scope and models. */
function LifePanel({ bug }: { bug: Bug }) {
  return (
    <div className={s.panel}>
      <div className={s.ph}>
        <span>Life of the bug</span>
        <i>git log</i>
      </div>
      <ul className={s.life}>
        {bug.life.map((l) => (
          <li key={`${l.hash}-${l.what}`}>
            <span className={s.lifeW}>{l.what}</span>
            <br />
            <span className={s.lifeC}>
              {l.date} · <CommitLink hash={l.hash} />
            </span>
            <br />
            {l.subject}
          </li>
        ))}
      </ul>
      <dl className={s.kv}>
        <dt>first → fixed</dt>
        <dd>
          <b>{latency(bug.life)}</b>
        </dd>
        <dt>mechanism</dt>
        <dd>
          <Rich text={bug.mechanism} />
        </dd>
        <dt>scope</dt>
        <dd>{bug.scope}</dd>
        <dt>models</dt>
        <dd>UB under {modelsText(bug)} (re-run on 2026-10-08)</dd>
      </dl>
    </div>
  );
}

/** Stage 1: the ported Rust at the commit before the fix (or the harness model of an uncommitted pass), beside the bug's life. */
export function ImplStage({ bug }: { bug: Bug }) {
  const c = bug.impl.commit;
  return (
    <div className={s.grid}>
      <div className={s.panel}>
        <div className={s.ph}>
          <span>Implementer: the ported Rust</span>
          <i>
            {c ? (
              <>
                <CommitLink hash={c.hash} /> · {c.date}
              </>
            ) : (
              "never committed"
            )}
          </i>
        </div>
        <p className={s.note}>
          <Rich text={bug.impl.note} links />
        </p>
        <CodeBlock segs={bug.impl.segs} label="The ported Rust" />
      </div>
      <LifePanel bug={bug} />
    </div>
  );
}

/** Stage 2: the reviewer's critique verbatim, or, when no reviewer caught it, what did. */
export function ReviewStage({ bug }: { bug: Bug }) {
  const r = bug.review;
  const none = r.kind === "none";
  return (
    <div className={s.grid}>
      <div className={s.panel}>
        <div className={s.ph}>
          <span>
            {none
              ? "Reviewer: none. What caught it"
              : "Reviewer: the critique, verbatim"}
          </span>
          <i>{bug.caughtBy}</i>
        </div>
        <div className={s.verdict}>
          <span className={none ? s.icNone : s.icRv} aria-hidden="true">
            {none ? "○" : "●"}
          </span>
          <span>
            <Rich text={r.head} />
          </span>
        </div>
        {r.quotes.map((q) => (
          <figure key={q.who} className={none ? `${s.q} ${s.qNone}` : s.q}>
            <figcaption className={s.who}>{q.who}</figcaption>
            <blockquote>
              <p>
                <Rich text={q.text} />
              </p>
            </blockquote>
            <div className={s.src}>
              <Rich text={q.src} links />
            </div>
          </figure>
        ))}
      </div>
      <LifePanel bug={bug} />
    </div>
  );
}

/** Stage 4: the fix commit with its diff (or before/after), the verdict matrix and the Miri-clean runs. */
export function FixStage({ bug }: { bug: Bug }) {
  const f = bug.fix;
  return (
    <div className={s.grid}>
      <div className={s.panel}>
        <div className={s.ph}>
          <span>
            Fix: <CommitLink hash={f.commit.hash} />
          </span>
          <i>{f.commit.date}</i>
        </div>
        <p className={s.note}>
          <b>{f.commit.subject}</b>
          <br />
          <Rich text={f.stat} />
        </p>
        {f.beforeAfter ? (
          <>
            <p className={s.note}>
              Before: the harness model of the first pass. After: the shipped
              loop shapes through the hook’s own pointer.
            </p>
            <CodeBlock
              segs={[f.beforeAfter.before]}
              label="Before: harness model of the first pass"
            />
            <div className={s.gap8} />
            <CodeBlock
              segs={f.beforeAfter.after}
              label="After: the shipped code"
            />
          </>
        ) : f.diff ? (
          <DiffBlock
            hunks={f.diff}
            label={`Diff of the fix commit ${f.commit.hash}`}
          />
        ) : null}
      </div>
      <div className={s.panel}>
        <div className={s.ph}>
          <span>Miri verdicts, before and after</span>
          <i>re-run 2026-10-08</i>
        </div>
        <table className={s.mx}>
          <thead>
            <tr>
              <th scope="col">shape</th>
              <th scope="col">SB</th>
              <th scope="col">TB</th>
            </tr>
          </thead>
          <tbody>
            {f.matrix.map(([shape, sb, tb]) => (
              <tr key={shape}>
                <td>
                  <Rich text={shape} />
                </td>
                <td className={`${s.v} ${s[`v${sb}`] ?? ""}`}>{sb}</td>
                <td className={`${s.v} ${s[`v${tb}`] ?? ""}`}>{tb}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <Term cmd={f.cleanCmd} text={f.cleanSb} />
        {f.cleanTb ? <Term cmd={f.cleanCmdTb} text={f.cleanTb} /> : null}
      </div>
    </div>
  );
}
