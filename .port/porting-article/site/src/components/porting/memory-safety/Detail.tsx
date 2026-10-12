import CommitText from "../ui/CommitText";
import { isProd, type MsEvent, refText, scopeKind } from "./model";
import { SCOPE_COLOURS } from "./Lanes";
import s from "./MemorySafety.module.css";

/**
 * The event detail panel: date, commit or ledger entries, detector, bug class,
 * crate, scope and defect count of the selected event, or the hint and the two
 * largest events when none is selected.
 */
export default function Detail({ event }: { event: MsEvent | null }) {
  if (!event) {
    return (
      <div className={s.detail}>
        <h4>Event detail</h4>
        <p className={s.hint}>
          Click a bubble (or press Enter on the focused timeline). The largest:
          the first full Miri sweep of the terminal engine on 07-11 (2,037
          tests) found 9 tests of one aliasing bug in search, fixed about 7
          hours later; on 09-28 isolated harnesses proved 8 cross-thread and
          re-entrant references to the whole renderer or surface were undefined
          behaviour before the fix and clean after, under both Stacked and Tree
          Borrows.
        </p>
      </div>
    );
  }
  const colour = SCOPE_COLOURS[scopeKind(event.scope)];
  return (
    <div className={s.detail} aria-live="polite">
      <h4>Event detail</h4>
      <h5>{event.sum}</h5>
      <dl>
        <dt>date</dt>
        <dd className={s.mono}>{event.d}</dd>
        <dt>reference</dt>
        <dd className={s.mono}>
          <CommitText text={refText(event)} />
        </dd>
        <dt>detector</dt>
        <dd>{event.tool}</dd>
        <dt>bug class</dt>
        <dd>{event.bc}</dd>
        <dt>where</dt>
        <dd>{event.area}</dd>
        <dt>scope</dt>
        <dd>
          <span style={{ color: colour }} aria-hidden="true">
            {isProd(event) ? "●" : "■"}
          </span>{" "}
          {event.scope}
        </dd>
        <dt>defects</dt>
        <dd className={s.mono}>{event.n}</dd>
      </dl>
    </div>
  );
}
