import StatusMark from "./StatusMark";
import { STATUS_INFO, type Status } from "./model";
import s from "./VerificationMatrix.module.css";

/** The status entries of the legend, in display order. */
const ORDER: Status[] = ["green", "fixed", "incomplete", "pending"];

/**
 * The matrix legend: the four statuses (colour plus shape), the hatched
 * "not available" swatch, the defect badges and the one-time definitions of
 * the sanitizer abbreviations used in the record sentences.
 */
export default function Legend() {
  return (
    <>
      <div className={s.legend}>
        {ORDER.map((k) => (
          <span key={k}>
            <StatusMark st={k} />
            {STATUS_INFO[k].name}
          </span>
        ))}
        <span>
          <i className={s.swHatch} aria-hidden="true" />
          not available on this host
        </span>
        <span className={s.k}>
          badges: <span className={`${s.bd} ${s.bdPort}`}>port n</span>{" "}
          <span className={`${s.bd} ${s.bdGh}`}>Ghostty n</span> defects found
        </span>
      </div>
      <p className={s.abbr}>
        In the records: ASan = AddressSanitizer, LSan = LeakSanitizer, MSan =
        MemorySanitizer, TSan = ThreadSanitizer, HWASan = HWAddressSanitizer,
        GTK = the Linux GUI toolkit. Miri checks Rust's aliasing rules under two
        models, Stacked Borrows and Tree Borrows.
      </p>
    </>
  );
}
