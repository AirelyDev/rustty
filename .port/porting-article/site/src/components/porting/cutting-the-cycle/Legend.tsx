"use client";

import SourceLink from "../ui/SourceLink";
import { PALETTE } from "../lib/palette";
import s from "./CuttingTheCycle.module.css";
import { TIER_COLOR, TIER_SHORT } from "./geometry";

/** NON_FILE_TIERS are the tiers no Zig file of the cycle became (shown from stage 3 on). */
const NON_FILE_TIERS = new Set([
  "Ported Zig dependencies",
  "FFI -sys bindings",
]);

/** Legend under the graph: tier colours, the stage-2 edge colours and what dot area means. */
export default function Legend({ stage }: { stage: number }) {
  return (
    <div className={s.legend}>
      {Object.keys(TIER_COLOR)
        .filter((k) => stage >= 3 || !NON_FILE_TIERS.has(k))
        .map((k) => (
          <span key={k}>
            <i
              style={
                k === "FFI -sys bindings"
                  ? {
                      background: "transparent",
                      border: `1px solid ${PALETTE.gray4}`,
                    }
                  : { background: TIER_COLOR[k] }
              }
            />
            {TIER_SHORT[k]}
          </span>
        ))}
      {stage === 2 ? (
        <>
          <span>
            <i className={s.legendLine} style={{ background: PALETTE.fail }} />
            planned cut
          </span>
          <span>
            <i className={s.legendLine} style={{ background: PALETTE.gold }} />
            other upward edge
          </span>
          <span>
            <i className={s.legendDash} />
            imports{" "}
            <SourceLink
              at="4749c4e93"
              path="src/global.zig"
              text="global.zig"
            />
          </span>
        </>
      ) : null}
      <span className={s.legendNote}>
        {stage <= 2
          ? "dots: Zig files, coloured by the crate tier they became; size by importers"
          : `area ∝ ${stage === 4 ? "physical lines on the date" : "code lines"}`}
      </span>
    </div>
  );
}
