import { PALETTE } from "../lib/palette";
import type { Headline } from "./model";
import { SCOPE_COLOURS } from "./Lanes";
import s from "./MemorySafety.module.css";

/** The facet toggle options. */
export const GROUPS = [
  ["lane", "rows: detector"],
  ["crate", "rows: crate area"],
] as const;

/**
 * The widget header: the defect-count hero figure with its three tiles, then the
 * facet toggle (rows by detector or crate area) and the legend.
 */
export default function Header({
  head,
  group,
  onGroup,
}: {
  head: Headline;
  group: string;
  onGroup: (g: "lane" | "crate") => void;
}) {
  return (
    <>
      <div className={s.hero}>
        <span>
          <span className={s.big}>{head.defects}</span>{" "}
          <span className={s.hs}>
            defects in {head.events} curated events (a floor, not a census)
          </span>
        </span>
        <span className={s.hs}>
          <b>{head.miri}</b>found by Miri or a Miri-proven audit
        </span>
        <span className={s.hs}>
          <b>{head.prodEvents}</b>events in production code ({head.prodDefects}{" "}
          defects)
        </span>
        <span className={s.hs}>
          <b>
            {head.aliasOrProv} of {head.defects}
          </b>
          defects are aliasing or pointer-provenance undefined behaviour
        </span>
      </div>
      <div className={s.ctl}>
        {/* biome-ignore lint/a11y/useSemanticElements: a pill toggle row, not a form fieldset */}
        <div className={s.seg} role="group" aria-label="Row grouping">
          {GROUPS.map(([k, label]) => (
            <button
              key={k}
              type="button"
              aria-pressed={group === k}
              className={group === k ? s.segOn : undefined}
              onClick={() => onGroup(k)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className={s.legend}>
          <span>
            <i
              className={s.swProd}
              style={{ background: SCOPE_COLOURS.prod }}
            />
            production code
          </span>
          <span>
            <i
              className={s.swTest}
              style={{ background: SCOPE_COLOURS.test }}
            />
            test-only
          </span>
          <span>
            <i
              className={s.swBench}
              style={{ background: SCOPE_COLOURS.bench }}
            />
            benchmark
          </span>
          <span style={{ color: PALETTE.gray4 }}>
            area ∝ defects in the event
          </span>
        </div>
      </div>
    </>
  );
}
