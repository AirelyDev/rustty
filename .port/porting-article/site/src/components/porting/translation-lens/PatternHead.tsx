import CommitLink from "../ui/CommitLink";
import type { KeyboardEvent } from "react";
import { fmtInt } from "../lib/format";
import type { TlData } from "./model";
import s from "./TranslationLens.module.css";

/** Props for PatternHead. */
type PatternHeadProps = {
  data: TlData;
  /** Selected pattern index. */
  pi: number;
  /** Selected sub-excerpt index. */
  si: number;
  /** Id prefix for the tabs and their panel. */
  uid: string;
  onPick: (i: number) => void;
  onSub: (i: number) => void;
};

/**
 * Headline (the pattern's primary Rust count and what was counted), the six
 * pattern tabs (arrow keys cycle them), the pattern title and explanation, the
 * excerpt stat chips and, for the intrusive pattern, the sub-excerpt toggle.
 */
export default function PatternHead({
  data,
  pi,
  si,
  uid,
  onPick,
  onSub,
}: PatternHeadProps) {
  const pats = data.patterns;
  const p = pats[pi];
  const st = p.subs[si].stats;

  /** Left/Right on the tab list move to the previous/next pattern (wrapping) and focus its tab. */
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = (pi + d + pats.length) % pats.length;
    onPick(n);
    document.getElementById(`${uid}-tab${n}`)?.focus();
  };

  const chips: [string, string, string][] = [
    [
      "te",
      "Zig excerpt",
      `${st.zig_code} code + ${st.zig_comment} comment lines`,
    ],
    [
      "cu",
      "Rust excerpt",
      `${st.rust_code} code + ${st.rust_comment} comment lines`,
    ],
    ["", "code ratio", `${(st.rust_code / st.zig_code).toFixed(2)}×`],
    ["gr", "Zig comments carried", `${st.carried} / ${st.zig_comments}`],
    ["", "Zig lines kept as Rust comments", String(st.echoes)],
    ["go", "port-note lines", String(st.port_note_lines)],
    ["re", "unsafe in Rust excerpt", String(st.unsafe_tokens)],
  ];

  return (
    <>
      <div className={s.headline}>
        <span className={s.big}>{fmtInt(p.counts.primary.rust)}</span>
        <span className={s.bigSub}>
          {p.counts.primary.label} ·{" "}
          <b>
            Rustty at <CommitLink hash={data.commits.rust} />
          </b>
        </span>
      </div>
      <div
        className={s.tabs}
        role="tablist"
        aria-label="Translation patterns"
        onKeyDown={onKey}
      >
        {pats.map((x, i) => (
          <button
            key={x.id}
            id={`${uid}-tab${i}`}
            type="button"
            role="tab"
            className={s.tab}
            aria-selected={i === pi}
            aria-controls={`${uid}-panel`}
            tabIndex={i === pi ? 0 : -1}
            onClick={() => onPick(i)}
          >
            {x.short}
            <small>{fmtInt(x.counts.primary.rust)} Rust sites</small>
          </button>
        ))}
      </div>
      <h4 className={s.pTitle}>{p.title}</h4>
      <p className={s.why}>{p.why}</p>
      <div className={s.chips}>
        {chips.map(([c, label, v]) => (
          <span key={label} className={s.chip} data-c={c || undefined}>
            {label} <b>{v}</b>
          </span>
        ))}
      </div>
      {p.subs.length > 1 ? (
        <fieldset className={s.subs} aria-label="Excerpt">
          {p.subs.map((x, i) => (
            <button
              key={x.key}
              type="button"
              className={s.sub}
              aria-pressed={i === si}
              onClick={() => onSub(i)}
            >
              {x.label}
            </button>
          ))}
        </fieldset>
      ) : null}
    </>
  );
}
