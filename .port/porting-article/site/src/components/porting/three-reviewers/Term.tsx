"use client";

import { useEffect, useRef } from "react";
import TerminalFrame from "../ui/TerminalFrame";
import { termLineKind } from "./model";
import s from "./ThreeReviewers.module.css";

/**
 * Renders real command output in the shared terminal frame titled with the
 * command: Miri errors red, passing results green, and the lines matching
 * `highlight` (the current step's file:line) marked and scrolled into view.
 */
export default function Term({
  cmd,
  text,
  highlight,
}: {
  cmd: string;
  text: string;
  highlight?: (line: string) => boolean;
}) {
  const bodyRef = useRef<HTMLPreElement>(null);
  const lines = text.split("\n");
  const marked = lines.map((l) => !!highlight?.(l));
  const firstHi = marked.indexOf(true);

  // Scroll the first highlighted line into view inside the output box.
  useEffect(() => {
    const box = bodyRef.current;
    const hl = box?.querySelector<HTMLElement>("[data-hi]");
    if (box && hl && firstHi >= 0) {
      box.scrollTop = Math.max(0, hl.offsetTop - 40);
    }
  }, [firstHi, text]);

  return (
    <div className={s.termWrap}>
      <TerminalFrame title={`$ ${cmd}`}>
        <pre ref={bodyRef} className={s.tbody}>
          {lines.map((l, i) => {
            const kind = termLineKind(l);
            let cls = kind === "error" ? s.tErr : kind === "ok" ? s.tOk : "";
            if (marked[i]) cls = `${cls} ${s.tHi}`;
            return (
              <span
                key={`${i}-${l.slice(0, 12)}`}
                className={cls || undefined}
                data-hi={marked[i] ? "" : undefined}
              >
                {l}
                {i < lines.length - 1 ? "\n" : ""}
              </span>
            );
          })}
        </pre>
      </TerminalFrame>
    </div>
  );
}
