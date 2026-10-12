import { useEffect, useMemo, useRef, useState } from "react";
import type { ChartTip } from "../ui/useChartTip";
import CodePane from "./CodePane";
import MapGutter, { type Viewport } from "./MapGutter";
import {
  groups,
  lineKinds,
  linkedLines,
  rangeText,
  type Side,
  type Sub,
  segText,
} from "./model";
import s from "./TranslationLens.module.css";

/** The highlighted line: its side, number and what set it (it decides which panes scroll). */
export type Selection = {
  side: Side;
  n: number;
  how: "hover" | "click" | "key" | "map" | "bits";
};

/** Props for CodeLens. */
type CodeLensProps = {
  sub: Sub;
  sel: Selection | null;
  setSel: (s: Selection | null) => void;
  tip: ChartTip;
  uid: string;
  reduced: boolean;
};

/** KIND_NAME names a mapping group kind in the status line and tooltip. */
export const KIND_NAME = {
  code: "code mapping",
  carried: "carried comment",
  echo: "Zig line kept as a comment",
};

/** Scrolls pane so the rows of the given line numbers are visible (no-op when they already are). */
function ensureVisible(
  pane: HTMLDivElement | null,
  ns: number[],
  reduced: boolean,
) {
  if (!pane || !ns.length) return;
  const want = new Set(ns.map(String));
  let first: HTMLElement | null = null;
  let last: HTMLElement | null = null;
  for (const el of pane.querySelectorAll<HTMLElement>("[data-n]")) {
    if (want.has(el.dataset.n ?? "")) {
      first ??= el;
      last = el;
    }
  }
  if (!first || !last) return;
  const top = first.offsetTop;
  const bot = last.offsetTop + last.offsetHeight;
  if (top >= pane.scrollTop && bot <= pane.scrollTop + pane.clientHeight)
    return;
  const target = Math.max(0, top - pane.clientHeight * 0.3);
  pane.scrollTo({ top: target, behavior: reduced ? "auto" : "smooth" });
}

/** Returns a pane's viewport as fractions of its scroll height. */
function viewOf(el: HTMLDivElement | null): Viewport {
  if (!el || !el.scrollHeight) return { top: 0, h: 1 };
  return {
    top: el.scrollTop / el.scrollHeight,
    h: el.clientHeight / el.scrollHeight,
  };
}

/**
 * Side-by-side Zig and Rust excerpts with the MAP gutter. Hovering, tapping
 * or arrowing to a line highlights its counterpart lines (copper on the Rust
 * side, teal on the Zig side) and scrolls the other pane to them; the status
 * line spells the mapping out, which is also what phones and screen readers get.
 */
export default function CodeLens({
  sub,
  sel,
  setSel,
  tip,
  uid,
  reduced,
}: CodeLensProps) {
  const zRef = useRef<HTMLDivElement>(null);
  const rRef = useRef<HTMLDivElement>(null);
  const [zView, setZView] = useState<Viewport>({ top: 0, h: 1 });
  const [rView, setRView] = useState<Viewport>({ top: 0, h: 1 });
  const gs = useMemo(() => groups(sub), [sub]);
  const kinds = useMemo(() => lineKinds(sub), [sub]);
  const link = sel ? linkedLines(sub, sel.side, sel.n) : null;
  const zHl = new Set(link?.z ?? []);
  const rHl = new Set(link?.r ?? []);

  // Measure the viewports once the panes are laid out.
  useEffect(() => {
    setZView(viewOf(zRef.current));
    setRView(viewOf(rRef.current));
  }, []);

  // Scroll the counterpart lines into view (both panes for map and bit-layout hovers).
  useEffect(() => {
    if (!sel || !link) return;
    if (!link.groups.length) {
      if (sel.how === "key")
        ensureVisible(
          sel.side === "z" ? zRef.current : rRef.current,
          [sel.n],
          reduced,
        );
      return;
    }
    const both = sel.how === "map" || sel.how === "bits" || sel.how === "key";
    if (sel.side === "r" || both) ensureVisible(zRef.current, link.z, reduced);
    if (sel.side === "z" || both) ensureVisible(rRef.current, link.r, reduced);
  }, [sel?.side, sel?.n, sel?.how]);

  /** Sets or clears the highlight from a pane event; a mouse leave clears only mouse-driven highlights. */
  const onLine =
    (side: Side) => (n: number | null, how: "hover" | "click" | "key") => {
      if (n === null) {
        if (how === "key" || sel?.how !== "key") setSel(null);
        return;
      }
      if (how === "hover" && sel?.how === "key") return;
      setSel({ side, n, how });
    };
  /** Highlights ribbon group gi and shows its tooltip (mouse hover). */
  const onEnter = (gi: number, x: number, y: number) => {
    setSel({ side: "z", n: gs[gi].z[0], how: "map" });
    tip.hover(`map:${gi}`, { x, y });
  };
  /** Highlights ribbon group gi and pins its tooltip (tap or click). */
  const onTap = (gi: number, x: number, y: number) => {
    setSel({ side: "z", n: gs[gi].z[0], how: "map" });
    tip.tap(`map:${gi}`, { x, y });
  };

  let status =
    "Hover or tap a line, or focus a pane and use the arrow keys, to see its counterpart.";
  if (sel && link) {
    const own = sel.side === "z" ? "Zig" : "Rust";
    const other = sel.side === "z" ? link.r : link.z;
    const kindSet = [
      ...new Set(link.groups.map((g) => KIND_NAME[gs[g].k])),
    ].join(", ");
    if (other.length) {
      status = `${own} ${sel.n} ↔ ${sel.side === "z" ? "Rust" : "Zig"} ${rangeText(other)} (${kindSet})`;
    } else {
      const k = sel.side === "r" ? kinds.r.get(sel.n) : undefined;
      status = `${own} ${sel.n}: ${k === "only" ? "Rust-only code" : k === "note" ? "port note or Rust-only comment" : "no mapped line"}`;
    }
  }

  return (
    <>
      <div className={s.code}>
        <CodePane
          side="z"
          lines={sub.zig}
          kinds={kinds.z}
          hl={zHl}
          cursor={sel?.how === "key" && sel.side === "z" ? sel.n : null}
          file={sub.zigFile}
          range={segText(sub.zig)}
          uid={uid}
          rowsRef={zRef}
          onLine={onLine("z")}
          onScroll={() => setZView(viewOf(zRef.current))}
        />
        <MapGutter
          sub={sub}
          groups={gs}
          active={link?.groups ?? []}
          zView={zView}
          rView={rView}
          onEnter={onEnter}
          onTap={onTap}
          onLeave={() => {
            tip.leave();
            if (sel?.how === "map") setSel(null);
          }}
        />
        <CodePane
          side="r"
          lines={sub.rust}
          kinds={kinds.r}
          hl={rHl}
          cursor={sel?.how === "key" && sel.side === "r" ? sel.n : null}
          file={sub.rustFile}
          range={segText(sub.rust)}
          uid={uid}
          rowsRef={rRef}
          onLine={onLine("r")}
          onScroll={() => setRView(viewOf(rRef.current))}
        />
      </div>
      <p className={s.status} aria-live="polite">
        {status}
      </p>
      <div className={s.legend}>
        <span>
          <i style={{ background: "rgba(100,216,198,.35)" }} />
          highlighted Zig line
        </span>
        <span>
          <i style={{ background: "rgba(217,121,52,.45)" }} />
          its Rust lines
        </span>
        <span>
          <i style={{ background: "#98c379" }} />
          comment carried verbatim
        </span>
        <span>
          <i style={{ background: "#64d8c6" }} />
          Zig line kept as a comment
        </span>
        <span>
          <i style={{ background: "#ffbc7a" }} />
          port note (Rust only)
        </span>
        <span>
          <i style={{ background: "rgba(217,121,52,.25)" }} />
          Rust-only code
        </span>
        <span>
          <b className={s.unsafe}>unsafe</b> keyword
        </span>
        <span>• identifier removed (note below)</span>
      </div>
      <p className={s.note}>
        • after a line number: an internal tracking identifier (a review or
        finding number) was removed from that comment; the rest of the line is
        verbatim. Every other line is the source as it stands at the commit.
      </p>
    </>
  );
}
