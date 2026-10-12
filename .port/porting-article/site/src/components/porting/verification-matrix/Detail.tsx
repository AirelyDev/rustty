import CommitText from "../ui/CommitText";
import CommitLink from "../ui/CommitLink";
import {
  defectTotals,
  dur,
  fmt,
  hostName,
  runHeading,
  STATUS_INFO,
  type VmData,
} from "./model";
import s from "./VerificationMatrix.module.css";

/**
 * The cell detail panel: for the selected run its reference commit, date,
 * status, tests, wall time, record, defects found and source; with nothing
 * selected, a prompt and the defect totals across the recorded runs.
 */
export default function Detail({
  data,
  sel,
}: {
  data: VmData;
  sel: number | null;
}) {
  if (sel === null) {
    const t = defectTotals(data);
    return (
      <div className={`${s.pn} ${s.det}`}>
        <h4>Cell detail</h4>
        <p className={s.detHint}>
          Select a cell for the run's record: test count, wall time, what it
          found and where it is written down. Lanes found{" "}
          <b className={s.port}>{t.port}</b> defects in Rustty's own code and{" "}
          <b className={s.gh}>{t.gh}</b> Ghostty-side defects across the
          recorded runs (port counts are per run as recorded; some runs record
          findings rather than defects).
        </p>
      </div>
    );
  }
  const r = data.runs[sel];
  return (
    <div className={`${s.pn} ${s.det}`}>
      <h4>Cell detail</h4>
      <h5>{runHeading(data, r)}</h5>
      <dl>
        <dt>reference</dt>
        <dd className={s.mono}>
          <CommitLink hash={data.pins[r.pin].sha} />
        </dd>
        <dt>host</dt>
        <dd>{hostName(data, r.host)}</dd>
        <dt>date</dt>
        <dd className={s.mono}>2026-{r.date}</dd>
        <dt>status</dt>
        <dd style={{ color: STATUS_INFO[r.st].color }}>
          {STATUS_INFO[r.st].name}
        </dd>
        {r.targeted ? (
          <>
            <dt>scope</dt>
            <dd>targeted: only the packages the move touched</dd>
          </>
        ) : null}
        {r.tests ? (
          <>
            <dt>tests</dt>
            <dd className={s.mono}>{fmt(r.tests)}</dd>
          </>
        ) : null}
        {r.secs ? (
          <>
            <dt>wall time</dt>
            <dd className={s.mono}>
              {fmt(r.secs)} s ({dur(r.secs)})
            </dd>
          </>
        ) : null}
        <dt>record</dt>
        <dd>
          <CommitText text={r.txt} />
        </dd>
        {r.port || r.gh ? (
          <>
            <dt>found</dt>
            <dd>
              {r.port ? <span className={s.port}>{r.port} port </span> : null}
              {r.gh ? <span className={s.gh}>{r.gh} Ghostty-side</span> : null}
            </dd>
          </>
        ) : null}
        <dt>source</dt>
        <dd className={s.mono}>
          <CommitText text={r.src} />
        </dd>
      </dl>
    </div>
  );
}
