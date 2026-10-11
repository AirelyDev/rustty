import type { CSSProperties } from "react";
import { pinStrip, STATUS_INFO, type VmData } from "./model";
import s from "./VerificationMatrix.module.css";

/**
 * The reference-commit selector: eight buttons (commit, date and label) each
 * with a strip of one marker per recorded run, coloured and shaped by status.
 * Selecting a commit shows its matrix.
 */
export default function Selector({
  data,
  pin,
  onPin,
}: {
  data: VmData;
  pin: number;
  onPin: (i: number) => void;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a row of toggle buttons, not a form fieldset
    <div className={s.pins} role="group" aria-label="Reference commit">
      {data.pins.map((p, i) => {
        const strip = pinStrip(data, i);
        const counts = (["green", "fixed", "incomplete", "pending"] as const)
          .map((k) => [k, strip.filter((x) => x === k).length] as const)
          .filter(([, n]) => n > 0)
          .map(([k, n]) => `${n} ${STATUS_INFO[k].name}`)
          .join(", ");
        return (
          <button
            key={p.sha}
            type="button"
            className={`${s.pin} ${i === pin ? s.pinOn : ""}`}
            aria-pressed={i === pin}
            aria-label={`${p.sha}, ${p.date}, ${p.label}: ${counts || "no runs recorded"}`}
            onClick={() => onPin(i)}
          >
            <span className={`${s.mono} ${s.pinSha}`}>{p.sha}</span>
            <span className={s.pinDate}>
              {p.date} · {p.label}
            </span>
            <span className={s.dots} aria-hidden="true">
              {strip.length === 0 ? (
                <i className={s.dotNone} />
              ) : (
                data.runs
                  .filter((r) => r.pin === i)
                  .map((r) => (
                    <i
                      key={`${r.tool}|${r.host}|${r.date}`}
                      className={`${s.dot} ${s[`dot_${STATUS_INFO[r.st].shape}`]}`}
                      style={
                        { "--mark": STATUS_INFO[r.st].color } as CSSProperties
                      }
                    />
                  ))
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}
