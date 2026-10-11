import {
  type KeyboardEvent,
  type PointerEvent,
  type RefObject,
  memo,
} from "react";
import { tokenize } from "./highlight";
import { type CodeLine, type LineKind, type Side, isComment } from "./model";
import s from "./TranslationLens.module.css";

/** Props for CodePane. */
type CodePaneProps = {
  side: Side;
  lines: CodeLine[];
  /** Line classes (carried, echo, note, Rust-only). */
  kinds: Map<number, LineKind>;
  /** Highlighted line numbers. */
  hl: Set<number>;
  /** Line under the keyboard cursor, or null. */
  cursor: number | null;
  file: string;
  range: string;
  uid: string;
  rowsRef: RefObject<HTMLDivElement | null>;
  /** Called with a line number on mouse hover, click/tap, or keyboard move (n null clears). */
  onLine: (n: number | null, how: "hover" | "click" | "key") => void;
  onScroll: () => void;
};

/** Returns the line number of the row under a pointer event, or null. */
function lineAt(e: PointerEvent<HTMLDivElement>): number | null {
  const row = (e.target as HTMLElement).closest("[data-n]");
  return row ? Number(row.getAttribute("data-n")) : null;
}

/** Renders one source line's tokens with the site's highlight classes (unsafe in red bold). */
const LineText = memo(function LineText({
  text,
  lang,
}: {
  text: string;
  lang: "zig" | "rust";
}) {
  return (
    <span className={s.t}>
      {text
        ? tokenize(text, lang).map((tk, i) =>
            tk.k ? (
              <span
                // biome-ignore lint/suspicious/noArrayIndexKey: tokens of a fixed line never reorder
                key={i}
                className={tk.k === "unsafe" ? s.unsafe : `hljs-${tk.k}`}
              >
                {tk.t}
              </span>
            ) : (
              tk.t
            ),
          )
        : " "}
    </span>
  );
});

/**
 * One code pane: a header with the language and file range, and the lines as
 * a listbox. Hover or tap a line to highlight its counterpart; with keyboard
 * focus, Up/Down/Home/End move a cursor through the lines and Escape clears.
 */
export default function CodePane({
  side,
  lines,
  kinds,
  hl,
  cursor,
  file,
  range,
  uid,
  rowsRef,
  onLine,
  onScroll,
}: CodePaneProps) {
  const lang = side === "z" ? "zig" : "rust";
  const nums = lines
    .filter((l): l is NonNullable<CodeLine> => !!l)
    .map((l) => l[0]);

  const rows: { l: CodeLine; key: string }[] = [];
  let last = 0;
  for (const l of lines) {
    if (l) last = l[0];
    rows.push({ l, key: l ? String(l[0]) : `gap-after-${last}` });
  }

  /** Moves the keyboard cursor with the arrow, Home and End keys; Escape clears it. */
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const at = cursor === null ? -1 : nums.indexOf(cursor);
    let k: number | null = null;
    if (e.key === "ArrowDown") k = Math.min(nums.length - 1, at + 1);
    else if (e.key === "ArrowUp") k = Math.max(0, at - 1);
    else if (e.key === "Home") k = 0;
    else if (e.key === "End") k = nums.length - 1;
    else if (e.key === "Escape" && cursor !== null) {
      e.preventDefault();
      onLine(null, "key");
      return;
    }
    if (k === null) return;
    e.preventDefault();
    onLine(nums[k], "key");
  };

  return (
    <div className={`${s.pane} ${side === "z" ? s.zig : s.rust}`}>
      <div className={s.pHdr}>
        <span className={s.lang}>
          {side === "z" ? "ZIG · GHOSTTY" : "RUST · RUSTTY"}
        </span>
        <span className={s.path}>
          <span dir="ltr">
            {file} : {range}
          </span>
        </span>
      </div>
      <div
        ref={rowsRef}
        className={s.rows}
        role="listbox"
        tabIndex={0}
        aria-label={`${side === "z" ? "Zig" : "Rust"} source, ${file}. Up and Down move through the lines and highlight their counterparts.`}
        aria-activedescendant={
          cursor !== null ? `${uid}-${side}${cursor}` : undefined
        }
        onKeyDown={onKey}
        onScroll={onScroll}
        onPointerOver={(e) => {
          if (e.pointerType === "mouse") {
            const n = lineAt(e);
            if (n !== null) onLine(n, "hover");
          }
        }}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") onLine(null, "hover");
        }}
        onClick={(e) => {
          const row = (e.target as HTMLElement).closest("[data-n]");
          if (row) onLine(Number(row.getAttribute("data-n")), "click");
        }}
      >
        {rows.map(({ l, key }) => {
          if (!l) {
            return (
              <div key={key} className={`${s.ln} ${s.gap}`} aria-hidden="true">
                ⋯ lines omitted ⋯
              </div>
            );
          }
          const [n, t, red] = l;
          const k = kinds.get(n) ?? "";
          const cls = [
            s.ln,
            isComment(t) ? s.cm : "",
            red ? s.red : "",
            hl.has(n) ? s.hl : "",
            cursor === n ? s.cursor : "",
          ].join(" ");
          return (
            // biome-ignore lint/a11y/useFocusableInteractive: the listbox keeps focus and points at its options with aria-activedescendant
            <div
              key={key}
              id={`${uid}-${side}${n}`}
              role="option"
              aria-selected={hl.has(n)}
              className={cls}
              data-n={n}
              data-k={k || undefined}
            >
              <span className={s.n}>{n}</span>
              <LineText text={t} lang={lang} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
