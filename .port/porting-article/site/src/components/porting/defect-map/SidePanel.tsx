import { type FocusEvent, Fragment, type MouseEvent, useRef } from "react";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { useRovingFocus } from "../ui/useRovingFocus";
import {
  type Def,
  type DensityRow,
  type Filter,
  RF_BY,
  RFAST,
  type StatusRow,
} from "./model";
import s from "./DefectMap.module.css";

/** Props for the right-hand panel. */
type SidePanelProps = {
  defs: readonly Def[];
  waffle: readonly number[];
  status: readonly StatusRow[];
  density: readonly DensityRow[];
  reported: number;
  /** The src/terminal note figures. */
  term: {
    lines: number;
    total: number;
    defects: number;
    ghostty: number;
    tmux: number;
    tmuxLines: number;
    tmuxDensity: number;
  };
  filter: Filter;
  selected: number | null;
  dir: string | null;
  tip: ChartTip;
  onSelect: (i: number) => void;
  onRf: (k: string) => void;
  onStatus: (k: string) => void;
  onDir: (n: string | null) => void;
};

/** Returns the viewport centre of an element. */
function centre(el: Element) {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/**
 * The right-hand panel: the ReleaseFast waffle (one square per crash-class
 * entry, one Tab stop with arrow keys; click opens it, a legend row filters the map), the upstream status
 * bar (click a segment to filter) and defects per 10,000 Zig lines for
 * directories with three or more (hover or press a row to highlight its tile).
 */
export default function SidePanel(p: SidePanelProps) {
  const { defs, tip, filter } = p;
  const maxD = Math.max(...p.density.map((r) => r.d));
  const waffleRef = useRef<HTMLDivElement>(null);
  // The waffle squares are one Tab stop; arrow keys move between them.
  useRovingFocus(waffleRef, { cols: 15 });
  /** Tooltip handlers for target id on a button. */
  const tipOn = (id: string) => ({
    onMouseMove: (e: MouseEvent) =>
      tip.hover(id, { x: e.clientX, y: e.clientY }),
    onMouseLeave: () => tip.leave(),
    onFocus: (e: FocusEvent<HTMLElement>) =>
      tip.focus(id, centre(e.currentTarget)),
    onBlur: () => tip.leave(),
  });
  const fixed = p.status.find((r) => r.key === "fixed");
  const fixedG =
    fixed?.items.filter((i) => defs[i].p === "ghostty").length ?? 0;
  return (
    <div className={s.pn}>
      <h4>What a safety-checked crash becomes in ReleaseFast</h4>
      {/* biome-ignore lint/a11y/useSemanticElements: a grid of squares, not a form fieldset */}
      <div
        ref={waffleRef}
        className={s.waffle}
        role="group"
        aria-label="One square per crash-class entry. Arrow keys move between them."
      >
        {p.waffle.map((i) => {
          const d = defs[i];
          const rf = RF_BY[d.rf];
          return (
            <button
              key={i}
              type="button"
              className={p.selected === i ? `${s.sq} ${s.sqOn}` : s.sq}
              aria-label={`ReleaseFast: ${rf.name}. ${d.t}`}
              style={{
                background: PALETTE[rf.role],
                opacity: !filter.rf || filter.rf === d.rf ? 1 : 0.2,
              }}
              onClick={() => p.onSelect(i)}
              {...tipOn(`w${i}`)}
            />
          );
        })}
      </div>
      <div className={s.kv}>
        {RFAST.map((r) => (
          <Fragment key={r.key}>
            <button
              type="button"
              className={filter.rf === r.key ? `${s.kvBtn} ${s.kvOn}` : s.kvBtn}
              aria-pressed={filter.rf === r.key}
              onClick={() => p.onRf(r.key)}
            >
              <i className={s.sw} style={{ background: PALETTE[r.role] }} />
              {r.name}
            </button>
            <span>
              {p.waffle.filter((i) => defs[i].rf === r.key).length} of{" "}
              {p.waffle.length}
            </span>
          </Fragment>
        ))}
      </div>
      <p className={s.note}>
        Ghostty ships ReleaseFast, where Zig's overflow, bounds and cast checks
        are off. Each square is one crash-class entry, classified from its
        ReleaseFast field (estimated; the split between wrong values and crashes
        may be off by about one).
      </p>

      <h4 className={s.later}>Upstream status</h4>
      <div className={s.stack}>
        {p.status.map((r) => (
          <button
            key={r.key}
            type="button"
            className={s.seg}
            aria-pressed={filter.status === r.key}
            aria-label={`${r.name}: ${r.n}`}
            style={{
              flex: Math.max(r.n, 2.5),
              background: PALETTE[r.role],
              opacity: !filter.status || filter.status === r.key ? 1 : 0.25,
            }}
            onClick={() => p.onStatus(r.key)}
            {...tipOn(`s${r.key}`)}
          />
        ))}
      </div>
      <div className={s.kv}>
        {p.status.map((r) => (
          <Fragment key={r.key}>
            <span>
              <i className={s.sw} style={{ background: PALETTE[r.role] }} />
              {r.name}
              {r.key === "fixed"
                ? ` (${fixedG} Ghostty, ${r.n - fixedG} Highway)`
                : ""}
            </span>
            <span>{r.n}</span>
          </Fragment>
        ))}
        <span style={{ color: PALETTE.gray6 }}>reported upstream so far</span>
        <span style={{ color: PALETTE.gray8 }}>{p.reported}</span>
      </div>

      <h4 className={s.later}>Defects per 10,000 lines of Zig</h4>
      <ul className={s.dens}>
        {p.density.map((r) => (
          <li key={r.n}>
            <button
              type="button"
              className={p.dir === r.n ? `${s.densRow} ${s.densOn}` : s.densRow}
              aria-pressed={p.dir === r.n}
              aria-label={`${r.n}: ${r.d.toFixed(1)} defects per 10,000 lines (${r.c} in ${fmtInt(r.v)} lines). Highlight on the map`}
              onMouseEnter={() => p.onDir(r.n)}
              onMouseLeave={() => p.onDir(null)}
              onFocus={() => p.onDir(r.n)}
              onBlur={() => p.onDir(null)}
              onClick={() => p.onDir(p.dir === r.n ? null : r.n)}
            >
              <span>{r.n}</span>
              <span>
                <i style={{ width: `${((r.d / maxD) * 100).toFixed(1)}%` }} />
              </span>
              <span className={s.densVal}>
                {r.d.toFixed(1)}{" "}
                <span style={{ color: PALETTE.gray3 }}>({r.c})</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      <p className={s.densNote}>
        Directories with 3+ defects; (n) = defects. src/terminal holds{" "}
        {Math.round((p.term.lines / p.term.total) * 100)}% of Ghostty's Zig and{" "}
        {p.term.defects} of its {p.term.ghostty} defects; inside it, tmux has{" "}
        {p.term.tmux} in {fmtInt(p.term.tmuxLines)} lines (
        {p.term.tmuxDensity.toFixed(1)}).
      </p>
    </div>
  );
}
