import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import type { Selection } from "./CodeLens";
import { countOf, type Pattern, type Sub, sqrtWidth } from "./model";
import Proof from "./Proof";
import ZigFile from "./ZigFile";
import s from "./TranslationLens.module.css";

/** CLASS_COLOR maps a ledger ownership class to its pill colour (others are grey). */
export const CLASS_COLOR: Record<string, string> = {
  BORROW_FIELD: PALETTE.blue,
  BORROW_PARAM: PALETTE.purple,
  OWNED: PALETTE.rust,
  FFI: PALETTE.yellow,
  SHARED: PALETTE.orange,
  INTRUSIVE: PALETTE.upstream,
  ARENA: PALETTE.gold,
  GHOSTTY_C: PALETTE.pass,
  STATIC: PALETTE.gray5,
  BACKREF: PALETTE.fail,
};

/** Returns the colour of ledger class c. */
export function classColor(c: string): string {
  return CLASS_COLOR[c] ?? "#6b716b";
}

/** SIDE_COLOR colours an occurrence count by side: Rust copper, Zig teal, ledger purple. */
const SIDE_COLOR = {
  rust: PALETTE.rust,
  zig: PALETTE.upstream,
  ledger: PALETTE.purple,
};

/** Renders a ledger class pill (the class name is the text, colour is redundant). */
export function ClassPill({ cls }: { cls: string }) {
  return (
    <span className={s.cls} style={{ background: classColor(cls) }}>
      {cls}
    </span>
  );
}

/** Props for Evidence. */
type EvidenceProps = {
  p: Pattern;
  sub: Sub;
  sel: Selection | null;
  setSel: (x: Selection | null) => void;
  /** Rows in the whole ledger (314). */
  ledgerTotal: number;
};

/**
 * The evidence row for the selected pattern: git grep occurrence counts
 * (sqrt-scaled bars), the pointer-lifetime ledger rows covering its fields,
 * and its proof (Miri harnesses, measured fixes, the cell layout or a note).
 */
export default function Evidence({
  p,
  sub,
  sel,
  setSel,
  ledgerTotal,
}: EvidenceProps) {
  const items = [p.counts.primary, ...p.counts.items];
  const mx = Math.max(...items.map((it) => countOf(it).v));
  return (
    <div className={s.ev}>
      <section className={s.box} aria-label="Occurrences">
        <div className={s.bt}>
          <span>Occurrences</span>
          <b>git grep</b>
        </div>
        {items.map((it, k) => {
          const { side, v } = countOf(it);
          const col = SIDE_COLOR[side];
          return (
            <div key={it.label} className={`${s.cnt} ${k === 0 ? s.prim : ""}`}>
              <span className={s.lab}>
                <span className={s.side} style={{ color: col }}>
                  {side.toUpperCase()}
                </span>{" "}
                {it.label}
                {it.zig_split ? (
                  <span className={s.split}>
                    {" "}
                    ({it.zig_split[0]} Ghostty + {it.zig_split[1]} libxev)
                  </span>
                ) : null}
              </span>
              <span className={s.val} style={{ color: col }}>
                {fmtInt(v)}
              </span>
              <span className={s.bar}>
                <i style={{ width: `${sqrtWidth(v, mx)}%`, background: col }} />
              </span>
            </div>
          );
        })}
      </section>
      <section className={s.box} aria-label="Pointer-lifetime ledger rows">
        <div className={s.bt}>
          <span>Pointer-lifetime ledger rows</span>
          <b>
            {p.ledgerCount} of {ledgerTotal}
          </b>
        </div>
        <p className={s.none} style={{ margin: "0 0 6px" }}>
          Selected: {p.ledgerNote}
          {p.ledgerCount > p.ledger.length
            ? ` (first ${p.ledger.length} shown)`
            : ""}
          .
        </p>
        <section
          className={s.tbl}
          // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be reachable by keyboard
          tabIndex={0}
          aria-label="Ledger rows"
        >
          <table>
            <tbody>
              {p.ledger.map((r) => (
                <tr key={r.join("|")}>
                  <td>
                    <ClassPill cls={r[4]} />{" "}
                    <span className={s.fld}>
                      {r[1]}.{r[2]}
                    </span>{" "}
                    <ZigFile file={r[0]} />
                    <br />
                    <span className={s.zt}>zig: {r[3]}</span>
                    <br />
                    rust: {r[5]}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      </section>
      <Proof p={p} sub={sub} sel={sel} setSel={setSel} />
    </div>
  );
}
