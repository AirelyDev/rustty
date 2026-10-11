import { classColor } from "./Evidence";
import type { LedgerClass, Pattern } from "./model";
import ZigFile from "./ZigFile";
import s from "./TranslationLens.module.css";

/** SAMPLE_ROWS is how many rows a selected class lists. */
const SAMPLE_ROWS = 8;

/** Props for ClassPanel. */
type ClassPanelProps = {
  classes: LedgerClass[];
  p: Pattern;
  selected: string | null;
  setSelected: (c: string | null) => void;
};

/**
 * The ledger's ownership classes as bars. Classes present in the selected
 * pattern's ledger rows are bold, with a white outline for the pattern's
 * share; clicking a class lists its first rows.
 */
export default function ClassPanel({
  classes,
  p,
  selected,
  setSelected,
}: ClassPanelProps) {
  const inPat = new Map<string, number>();
  for (const r of p.ledger) inPat.set(r[4], (inPat.get(r[4]) ?? 0) + 1);
  const mx = Math.max(...classes.map((c) => c.n));
  const total = classes.reduce((a, c) => a + c.n, 0);
  const sel = classes.find((c) => c.cls === selected) ?? null;

  return (
    <section className={s.box} aria-label="Ledger ownership classes">
      <div className={s.bt}>
        <span>Ledger ownership classes</span>
        <b>
          {total} rows · {classes.length} classes
        </b>
      </div>
      {classes.map((c) => {
        const n = inPat.get(c.cls);
        const on = n !== undefined;
        return (
          <button
            key={c.cls}
            type="button"
            className={`${s.clsRow} ${on ? s.pat : ""}`}
            aria-pressed={selected === c.cls}
            aria-label={`${c.cls}: ${c.n} rows${on ? `, ${n} in the selected pattern's rows` : ""}`}
            onClick={() => setSelected(selected === c.cls ? null : c.cls)}
          >
            <span
              className={s.nm}
              style={{ color: on ? "#f4f1ed" : classColor(c.cls) }}
            >
              {c.cls}
            </span>
            <span className={s.clsTrack}>
              <i
                style={{
                  width: `${Math.max(1.5, (100 * c.n) / mx)}%`,
                  background: classColor(c.cls),
                  opacity: on || !selected || selected === c.cls ? 1 : 0.45,
                }}
              />
              {on ? (
                <i
                  style={{
                    width: `${Math.max(1.5, (100 * (n ?? 0)) / mx)}%`,
                    border: "1px solid #f4f1ed",
                  }}
                />
              ) : null}
            </span>
            <span className={s.clsV}>{c.n}</span>
          </button>
        );
      })}
      <div className={s.samp}>
        {sel ? (
          <>
            <div className={s.bt} style={{ marginBottom: 4 }}>
              <span>{sel.cls}</span>
              <b>
                {sel.n} rows
                {sel.n > SAMPLE_ROWS ? ` · ${SAMPLE_ROWS} shown` : ""}
              </b>
            </div>
            <div className={s.tbl} style={{ maxHeight: 200 }}>
              <table>
                <tbody>
                  {sel.sample.map((r) => (
                    <tr key={r.join("|")}>
                      <td>
                        <span className={s.fld}>
                          {r[1]}.{r[2]}
                        </span>{" "}
                        <ZigFile file={r[0]} />
                        <br />
                        <span className={s.zt}>{r[3]}</span>
                        <br />→ {r[4]}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        ) : (
          <p className={`${s.none} ${s.small}`}>
            Click a class to see sample rows. A white outline marks the share of
            that class covered by the selected pattern's ledger rows.
          </p>
        )}
      </div>
    </section>
  );
}
