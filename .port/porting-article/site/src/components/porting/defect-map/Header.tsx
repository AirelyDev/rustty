import { PALETTE } from "../lib/palette";
import { shapePath } from "./layout";
import {
  CLASSES,
  type Def,
  type Filter,
  type Headline,
  isFiltered,
  METHODS,
} from "./model";
import s from "./DefectMap.module.css";

/** Props for the header: figures, filter state and the chip handlers. */
type HeaderProps = {
  head: Headline;
  defs: readonly Def[];
  filter: Filter;
  onClass: (k: string) => void;
  onMethod: (k: string) => void;
  onReset: () => void;
};

/**
 * The widget header: the defect-count hero with the class and project split, then
 * the class chips (with reset) and the discovery-method chips with their
 * mark glyphs. A chip click isolates; further clicks add or remove.
 */
export default function Header({
  head,
  defs,
  filter,
  onClass,
  onMethod,
  onReset,
}: HeaderProps) {
  const b = head.byClass;
  /** Counts the defects matching pred. */
  const count = (pred: (d: Def) => boolean) => defs.filter(pred).length;
  return (
    <>
      <div className={s.hero}>
        <span className={s.big}>{head.total}</span>
        <span className={s.sub}>
          defects recorded: {head.live} live ({b.crash} crash, {b.ub} UB,{" "}
          {b.leak} leak, {b.hang} hang, {b.intent} broken intent), {head.latent}{" "}
          latent, {head.test} test weaknesses · {head.ghostty} in Ghostty,{" "}
          {head.deps} in dependencies
        </span>
      </div>
      {/* biome-ignore lint/a11y/useSemanticElements: a chip row, not a form fieldset */}
      <div className={s.bar} role="group" aria-label="Filter by defect class">
        <span className={s.grp} aria-hidden="true">
          class
        </span>
        {CLASSES.map((c) => {
          const colour = PALETTE[c.role];
          const on = filter.cls.has(c.key);
          return (
            <button
              key={c.key}
              type="button"
              aria-pressed={on}
              className={on ? s.chip : `${s.chip} ${s.chipOff}`}
              onClick={() => onClass(c.key)}
            >
              <svg viewBox="0 0 12 12" aria-hidden="true">
                <circle
                  cx="6"
                  cy="6"
                  r="4.5"
                  fill={"hollow" in c ? "none" : colour}
                  stroke={colour}
                  strokeWidth="1.5"
                  strokeDasharray={"dash" in c ? "2 1.5" : undefined}
                />
                {"dot" in c ? (
                  <circle cx="6" cy="6" r="1.6" fill={PALETTE.card} />
                ) : null}
              </svg>
              {c.name}{" "}
              <span className={s.n}>{count((d) => d.c === c.key)}</span>
            </button>
          );
        })}
        <button
          type="button"
          className={s.reset}
          onClick={onReset}
          disabled={!isFiltered(filter)}
        >
          reset
        </button>
      </div>
      {/* biome-ignore lint/a11y/useSemanticElements: a chip row, not a form fieldset */}
      <div
        className={s.bar}
        role="group"
        aria-label="Filter by how the defect was found (estimated)"
      >
        <span className={s.grp} aria-hidden="true">
          found by (estimated)
        </span>
        {METHODS.map((m) => {
          const on = filter.met.has(m.key);
          return (
            <button
              key={m.key}
              type="button"
              aria-pressed={on}
              className={on ? s.chip : `${s.chip} ${s.chipOff}`}
              onClick={() => onMethod(m.key)}
            >
              <svg viewBox="0 0 13 13" aria-hidden="true">
                <path
                  d={shapePath(m.shape, 6.5, 6.5, 4.4)}
                  fill={PALETTE.gray6}
                />
              </svg>
              {m.name}{" "}
              <span className={s.n}>{count((d) => d.m === m.key)}</span>
            </button>
          );
        })}
      </div>
    </>
  );
}
