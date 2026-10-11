import { type ReactNode, useState } from "react";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import Tooltip from "../ui/Tooltip";
import type { Selection } from "./CodeLens";
import {
  harnessTally,
  type Pattern,
  perfGroups,
  type Sub,
  zigFieldLine,
} from "./model";
import s from "./TranslationLens.module.css";

/** BIT_COLORS are the cell field colours, in field order. */
const BIT_COLORS = [
  PALETTE.blue,
  PALETTE.rust,
  PALETTE.purple,
  PALETTE.yellow,
  PALETTE.fail,
  PALETTE.upstream,
  PALETTE.pass,
  PALETTE.gray3,
];

/** BIT_TICKS are the bit-axis ticks of the cell layout. */
const BIT_TICKS = [0, 26, 42, 48, 63];

/** Props for Proof. */
type ProofProps = {
  p: Pattern;
  sub: Sub;
  sel: Selection | null;
  setSel: (x: Selection | null) => void;
};

/** Returns the count of the pattern item whose label starts with prefix. */
function count(p: Pattern, prefix: string): number {
  const it = p.counts.items.find((i) => i.label.startsWith(prefix));
  return it?.rust ?? it?.zig ?? 0;
}

/** Renders the isolated Miri harness tables (Stacked and Tree Borrows verdicts). */
function Harnesses({ p }: { p: Pattern }) {
  const t = harnessTally(p.harness);
  return (
    <>
      <div className={s.bt}>
        <span>Isolated Miri harnesses</span>
        <b>
          {p.harness.length} · {t.ub} UB / {t.clean} clean
        </b>
      </div>
      {p.harness.map((h) => (
        <div key={h.name} className={s.h}>
          <div className={s.hn}>
            {h.name} <span>{h.date}</span>
          </div>
          <div className={s.hw}>{h.what}</div>
          <table className={s.grid} aria-label={`${h.name} verdicts`}>
            <colgroup>
              <col />
              <col className={s.vc} />
              <col className={s.vc} />
            </colgroup>
            <thead>
              <tr>
                <th className={s.hd}>candidate</th>
                <th className={s.hd}>SB</th>
                <th className={s.hd}>TB</th>
              </tr>
            </thead>
            <tbody>
              {h.rows.map((r) => (
                <tr key={r[0]}>
                  <td title={r[0]}>{r[0]}</td>
                  {[r[1], r[2]].map((v, i) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: two fixed columns, Stacked then Tree Borrows
                    <td key={i} className={v === "CLEAN" ? s.c : s.x}>
                      {v === "CLEAN" ? "ok" : "UB"}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
      <p className={`${s.none} ${s.small}`}>
        SB = Stacked Borrows, TB = Tree Borrows. Each harness is a small program
        that copies the borrow shape without FFI, so Miri can run it. Rejected
        shapes show as UB.
      </p>
    </>
  );
}

/** Renders the measured perf fix chains (baseline red, each cumulative fix copper, faster than Zig green). */
function PerfChains({ p }: { p: Pattern }) {
  return (
    <>
      <div className={s.bt}>
        <span>Measured effect (no Miri needed)</span>
        <b>Rust vs Zig</b>
      </div>
      {perfGroups(p.perf ?? []).map((g) => (
        <div key={g.bench} className={s.perf}>
          <div className={s.hn}>
            {g.bench}{" "}
            <span>
              {g.unit === "%" ? "% slower than Zig" : "Rust/Zig time per op"}
            </span>
          </div>
          {g.steps.map((st) => {
            const v = st.value;
            const label =
              g.unit === "%"
                ? `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(v)}%`
                : `${v}×`;
            const col =
              v <= 0
                ? PALETTE.pass
                : st.order === 0
                  ? PALETTE.fail
                  : PALETTE.rust;
            return (
              <Tooltip
                key={st.order}
                id={`${g.bench}-${st.order}`}
                content={
                  <>
                    <b>{st.order === 0 ? "starting gap" : `fix ${st.order}`}</b>
                    : {st.fix}
                    {st.hazard ? ` (${st.hazard})` : ""} · {label}
                    <br />
                    source: {st.provenance}
                  </>
                }
              >
                <span className={s.step}>
                  <span className={s.stepTrack}>
                    <i
                      style={{
                        width: `${Math.max(1, (100 * Math.abs(v)) / g.max)}%`,
                        background: col,
                      }}
                    />
                  </span>
                  <span className={s.stepVal}>{label}</span>
                </span>
              </Tooltip>
            );
          })}
        </div>
      ))}
      <p className={`${s.none} ${s.small}`}>
        Top bar of each group: the starting gap. Each bar below is one fix,
        applied cumulatively. Hover or tap a bar for the fix. The const-generic
        width in the excerpt is the first fix under terminal-snapshot.encode.
      </p>
    </>
  );
}

/** Renders the 64-bit cell layout; hovering, focusing or tapping a field highlights its Zig declaration. */
function Bits({ p, sub, sel, setSel }: ProofProps) {
  const [on, setOn] = useState<number | null>(null);
  const bits = p.bits ?? [];
  const b = on === null ? null : bits[on];
  /** Shows field i and highlights its Zig declaration line. */
  const show = (i: number) => {
    setOn(i);
    const n = zigFieldLine(sub, bits[i][0]);
    if (n !== null) setSel({ side: "z", n, how: "bits" });
    else if (sel?.how === "bits") setSel(null);
  };
  return (
    <>
      <div className={s.bt}>
        <span>The 64-bit cell layout</span>
        <b>hover a field</b>
      </div>
      <div className={s.bits}>
        {bits.map(([name, , width], i) => (
          <button
            key={name}
            type="button"
            className={s.bitSeg}
            aria-pressed={on === i}
            aria-label={`${name}: ${width} bits`}
            style={{ flex: width, background: BIT_COLORS[i] }}
            onPointerEnter={(e) => {
              if (e.pointerType === "mouse") show(i);
            }}
            onFocus={() => show(i)}
            onClick={() => show(i)}
          >
            {width >= 6 ? name : ""}
          </button>
        ))}
      </div>
      <div className={s.bitAx} aria-hidden="true">
        {BIT_TICKS.map((t) => (
          <span
            key={t}
            style={{ left: `${(t === 63 ? 100 : (t / 64) * 100).toFixed(2)}%` }}
          >
            {t === 0 ? "bit 0" : t}
          </span>
        ))}
      </div>
      <p className={s.bitInfo} aria-live="polite">
        {b ? (
          <>
            <b>{b[0]}</b> bits {b[1]}–{b[1] + b[2] - 1} ({b[2]}) · {b[3]}
          </>
        ) : (
          "Zig declares these fields in order and the compiler packs them. Rust spells each offset out as a constant."
        )}
      </p>
      <p className={`${s.none} ${s.small}`}>
        No Miri harness: this is plain integer arithmetic with no aliasing. A
        layout test in the port checks every shift against the reference.
      </p>
    </>
  );
}

/**
 * The proof box: Miri harness tables for the raw-pointer and intrusive
 * patterns, measured fix chains for comptime, the cell bit layout for packed
 * structs, and a sentence for the patterns that need no harness.
 */
export default function Proof(props: ProofProps) {
  const { p } = props;
  let body: ReactNode;
  if (p.harness.length) body = <Harnesses p={p} />;
  else if (p.perf) body = <PerfChains p={p} />;
  else if (p.bits) body = <Bits {...props} />;
  else {
    body = (
      <>
        <div className={s.bt}>
          <span>Miri harness</span>
          <b>–</b>
        </div>
        <p className={s.none}>
          {p.id === "errdefer"
            ? `No isolated Miri harness. The pattern does not alias, and the guard only closes file descriptors. Across the codebase, most ported errdefers stay as a comment next to the cleanup written out in the error arm: ${fmtInt(count(p, "Rust comment lines"))} comment lines cite errdefer, and there are only ${fmtInt(count(p, "named"))} named *Errdefer guard types.`
            : "No isolated Miri harness. The point here is byte fidelity, not aliasing: bytes stay bytes. The raw set_title_raw entry point exists because a caller can pass the title's own buffer back in, so the copy must be a memmove."}
        </p>
      </>
    );
  }
  return (
    <section className={s.box} aria-label="Proof">
      {body}
    </section>
  );
}
