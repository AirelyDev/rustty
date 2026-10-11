import CommitText from "../ui/CommitText";
import { fmtPct } from "../lib/format";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import { hhmm, listSubject, stageCommits } from "./model";
import s from "./PerfReplay.module.css";
import type { PrData, StageKind } from "./types";

/** KIND names and colours each step kind (badge text, so colour is not the only cue). */
export const KIND: Record<StageKind, [string, string]> = {
  ship: ["official ship sweep", PALETTE.rust],
  stage: ["stage binary (cumulative)", PALETTE.blue],
  session: ["session control (no code change)", PALETTE.purple],
  profile: ["build-profile experiment", PALETTE.yellow],
};

/** Formats a signed ms delta with U+2212: "−301 ms", "+4 ms". */
function deltaMs(d: number): string {
  const r = Math.round(d);
  return `${r > 0 ? "+" : r < 0 ? "−" : ""}${Math.abs(r)} ms`;
}

/** Shortens a diffstat: "6 files changed, 1020 insertions(+)" → "6 files, 1020 +". */
function shortStat(stat: string): string {
  return stat.replace(
    / changed|insertions|insertion|deletions|deletion|\(|\)/g,
    "",
  );
}

/**
 * The step panel beside the replay: commit time of the code measured, the
 * step kind, wins/ties/losses, suite and its change, geomean, the step's
 * description and commit, the benchmarks that moved and every commit since
 * the previous step.
 */
export default function StepPanel({
  data,
  cur,
}: {
  data: PrData;
  cur: number;
}) {
  const st = data.stages[cur];
  const prev = cur ? data.stages[cur - 1] : null;
  const [kind, colour] = KIND[st.kind];
  const tot = st.W + st.T + st.L;
  const commits = stageCommits(data, cur);
  return (
    <div className={s.step}>
      <div className={s.k}>code as of (commit time)</div>
      <div className={s.clock}>{hhmm(st.t)}</div>
      <span className={s.badge} style={{ borderColor: colour, color: colour }}>
        {kind}
      </span>
      <div className={s.k}>wins / ties / losses</div>
      <div className={s.wtl} aria-hidden="true">
        <span
          style={{ width: `${(st.W / tot) * 100}%`, background: PALETTE.rust }}
        />
        <span
          style={{ width: `${(st.T / tot) * 100}%`, background: PALETTE.gray4 }}
        />
        <span
          style={{ width: `${(st.L / tot) * 100}%`, background: PALETTE.fail }}
        />
      </div>
      <span className={s.num}>
        {st.W} / {st.T} / {st.L}
      </span>
      <div className={s.k}>suite (sum of 14 medians)</div>
      <span
        className={s.num}
        style={{ color: st.suiteD < 0 ? PALETTE.rust : PALETTE.fail }}
      >
        {fmtPct(st.suiteD)}
      </span>{" "}
      <span className={s.small}>
        Rust {Math.round(st.suiteR)} ms · Zig {Math.round(st.suiteZ)} ms
        {prev ? ` · Δ ${deltaMs(st.suiteR - prev.suiteR)}` : ""}
      </span>
      <div className={s.k}>geomean Rust ÷ Zig</div>
      <span className={s.num}>{st.geo.toFixed(3)}</span>
      <div className={s.k}>step</div>
      <div className={s.subj}>{st.label}</div>
      <span className={`${s.hash} ${s.mono}`}>
        <CommitLink hash={st.commit} />
      </span>{" "}
      <span className={s.small}>{st.subject}</span>
      {st.note ? (
        <div className={s.note}>
          <CommitText text={st.note} />
        </div>
      ) : null}
      {cur ? (
        <div className={s.k}>
          moved outside spread
          {st.kind === "session" && st.moved.length ? " (session effect)" : ""}
        </div>
      ) : null}
      {st.moved.map(([b, pct]) => (
        <span key={b} className={s.chip}>
          {b} {fmtPct(pct)}
        </span>
      ))}
      {cur && !st.moved.length ? (
        <span className={s.small}>none (changes within noise)</span>
      ) : null}
      {commits.length ? (
        <>
          <div className={s.k}>
            commits since previous step ({commits.length})
          </div>
          <ul className={s.clist}>
            {commits.map((c) => (
              <li key={c.h}>
                <CommitLink hash={c.h} /> {c.t.slice(11, 16)} {listSubject(c.s)}
                <br />
                <span className={s.dim}>
                  {shortStat(c.stat)}
                  {c.files.length ? ` · ${c.files.slice(0, 2).join(", ")}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}
