"use client";

import { useEffect, useRef } from "react";
import { type CodeMarks, diffKind, indentOf, lineKey } from "./model";
import s from "./ThreeReviewers.module.css";
import type { Seg } from "./types";

/**
 * Renders code excerpts with real line numbers: a header per file (a "⋮" gap
 * between excerpts of the same file), comment lines dimmed, and, in the Miri
 * stage, the step's lines highlighted (red at the UB) and earlier ones shaded.
 * The highlighted line is scrolled into view inside the box, not the page.
 */
export function CodeBlock({
  segs,
  marks,
  label,
}: {
  segs: Seg[];
  marks?: CodeMarks;
  label: string;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const hiKey = marks ? [...marks.hi].join("|") : "";

  // Keep the current line in view inside the scroll box.
  useEffect(() => {
    const box = boxRef.current;
    const hl = box?.querySelector<HTMLElement>("[data-hi]");
    if (box && hl && hiKey) {
      box.scrollTop = Math.max(0, hl.offsetTop - box.clientHeight / 2);
    }
  }, [hiKey]);

  let last: string | null = null;
  return (
    <div ref={boxRef} className={s.code} title={label}>
      {segs.map((sg, si) => {
        const head =
          sg.file && sg.file !== last ? (
            <div className={s.f}>{sg.file}</div>
          ) : (
            <div className={s.gap} aria-hidden="true">
              ⋮
            </div>
          );
        if (sg.file) last = sg.file;
        const file = sg.file || last || "";
        const ind = indentOf(sg.lines);
        return (
          <div key={`${si}-${sg.file}`} className={s.seg}>
            {head}
            {sg.lines.map(([n, t]) => {
              const key = lineKey(file, n);
              const hi = marks?.hi.has(key);
              let cls = s.l;
              if (/^\s*\/\//.test(t)) cls += ` ${s.cm}`;
              if (hi) cls += ` ${marks?.bad ? s.hibad : s.hi}`;
              else if (marks?.seen.has(key)) cls += ` ${s.seen}`;
              return (
                <div key={n} className={cls} data-hi={hi ? "" : undefined}>
                  <span className={s.ln}>{n}</span>
                  <span className={s.tx}>
                    {t.startsWith("…") ? t : t.slice(ind)}
                  </span>
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

/** Renders unified-diff hunks: added lines green, deleted red, hunk headers blue; "⋮" between hunks. */
export function DiffBlock({
  hunks,
  label,
}: {
  hunks: string[][];
  label: string;
}) {
  return (
    <div className={s.code} title={label}>
      {hunks.map((hk, i) => (
        <div key={`${i}-${hk[0]}`}>
          {i ? (
            <div className={s.gap} aria-hidden="true">
              ⋮
            </div>
          ) : null}
          {hk.map((ln, j) => {
            const k = diffKind(ln);
            const cls = k === "ctx" ? s.l : `${s.l} ${s[k]}`;
            return (
              <div key={`${j}-${ln}`} className={cls}>
                <span className={s.ln} />
                <span className={s.tx}>{ln}</span>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
