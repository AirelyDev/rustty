import CommitText from "../ui/CommitText";
import CommitLink from "../ui/CommitLink";
import { moveName, type MtData } from "./model";
import { int, topAreas } from "./stats";
import s from "./MovingTarget.module.css";

/**
 * The detail panel for the selected move: the upstream commit and the Rustty
 * commit that moved the reference, how long the catch-up took, the Ghostty size at
 * the pin, the test gap on arrival, the check results recorded in the
 * pin-moving commit's message (restated in plain words), and a ranked bar
 * list of where that range's churn landed.
 */
export default function Detail({ data, sel }: { data: MtData; sel: number }) {
  const a = data.adv[sel];
  const bars = topAreas(data, sel);
  return (
    <div className={s.detail} aria-live="polite">
      <div>
        <h5>
          {moveName(a)}:{" "}
          <span className={`${s.mono} ${s.teal}`}>
            <CommitLink hash={a.from} /> → <CommitLink hash={a.to} />
          </span>
        </h5>
        <dl>
          <dt>upstream commit</dt>
          <dd className={s.mono}>{a.up.replace("T", " ")} (Pacific)</dd>
          <dt>reference moved in Rustty</dt>
          <dd className={s.mono}>
            {a.adv.replace("T", " ")} · <CommitLink hash={a.commit} />
          </dd>
          <dt>catch-up</dt>
          <dd>{a.durTxt}</dd>
          <dt>Ghostty .zig lines at that commit</dt>
          <dd className={s.mono}>{int(a.loc)}</dd>
          {a.ts ? (
            <>
              <dt>test gap on arrival</dt>
              <dd>{a.ts}</dd>
            </>
          ) : null}
          {a.fin !== null ? (
            <>
              <dt>Rustty commits until the catch-up finished</dt>
              <dd className={s.mono}>
                {a.fin} non-merge{a.finNote ? ` (${a.finNote})` : ""}
              </dd>
            </>
          ) : null}
        </dl>
        <div className={s.quote}>
          <CommitText text={a.q} />
        </div>
        <div className={s.quoteNote}>
          check results recorded in the commit message of{" "}
          <CommitLink hash={a.commit} />, in plain words
        </div>
      </div>
      <div>
        <div className={s.label}>Where the range's churn landed</div>
        {bars.map((b) => (
          <div key={b.name} className={s.dbar}>
            <span className={s.barName}>{b.name}</span>
            <span className={s.barTrack}>
              <span
                className={s.barFill}
                style={{ width: `${b.pct.toFixed(1)}%` }}
              />
            </span>
            <span className={s.mono}>{Math.round(b.pct)}%</span>
          </div>
        ))}
        {a.note ? (
          <div className={s.note}>
            <CommitText text={a.note} />
          </div>
        ) : null}
      </div>
    </div>
  );
}
