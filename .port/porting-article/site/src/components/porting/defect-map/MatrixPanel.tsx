import { type FocusEvent, type MouseEvent, useRef } from "react";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { useRovingFocus } from "../ui/useRovingFocus";
import {
  CLASSES,
  cellAlpha,
  type Filter,
  type Headline,
  type Matrix,
  METHODS,
} from "./model";
import s from "./DefectMap.module.css";

/** Props for the class × method matrix panel. */
type MatrixPanelProps = {
  mx: Matrix;
  head: Headline;
  filter: Filter;
  /** Isolates [method, class] on the map, or clears it with null. */
  onCell: (cell: readonly [string, string] | null) => void;
  tip: ChartTip;
};

/** Converts a #rrggbb colour and alpha to rgba(). */
function rgba(hex: string, a: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a.toFixed(3)})`;
}

/**
 * The class × discovery-method matrix (11 × 7): cell colour is the class hue,
 * alpha grows with the square root of the count. A cell is a button that
 * isolates its defects on the map (activate again to clear); hover or focus
 * lists them. The cells are a single Tab stop with arrow-key movement.
 */
export default function MatrixPanel({
  mx,
  head,
  filter,
  onCell,
  tip,
}: MatrixPanelProps) {
  const bodyRef = useRef<HTMLTableSectionElement>(null);
  // The cells are one Tab stop: arrow keys move between them (up/down by row).
  useRovingFocus(bodyRef, { grid: true });
  /** Returns the viewport centre of an element. */
  const centre = (el: Element) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  };
  return (
    <div className={s.pn}>
      <h4>Class × how it was found</h4>
      <div className={s.mxScroll}>
        <table className={s.mx}>
          <caption className={s.srOnly}>
            Recorded defects by discovery method (estimated) and class
          </caption>
          <thead>
            <tr>
              <th scope="col">
                <span className={s.srOnly}>found by</span>
              </th>
              {CLASSES.map((c) => (
                <th
                  key={c.key}
                  scope="col"
                  className={s.c}
                  style={{ color: PALETTE[c.role] }}
                >
                  {c.short}
                </th>
              ))}
              <th scope="col">all</th>
            </tr>
          </thead>
          <tbody ref={bodyRef}>
            {METHODS.map((m) => (
              <tr key={m.key}>
                <th scope="row" className={s.l}>
                  {m.name}
                </th>
                {CLASSES.map((c) => {
                  const v = mx.get(m.key, c.key);
                  if (!v) {
                    return (
                      <td key={c.key} className={s.z}>
                        <span aria-hidden="true">·</span>
                        <span className={s.srOnly}>0</span>
                      </td>
                    );
                  }
                  const on =
                    filter.cell?.[0] === m.key && filter.cell[1] === c.key;
                  const id = `x${m.key}|${c.key}`;
                  return (
                    <td key={c.key}>
                      <button
                        type="button"
                        className={on ? `${s.cell} ${s.cellOn}` : s.cell}
                        aria-pressed={on}
                        aria-label={`${m.name}, ${c.name}: ${v}`}
                        style={{
                          background: rgba(
                            PALETTE[c.role],
                            cellAlpha(v, mx.max),
                          ),
                          color:
                            v / mx.max > 0.35 ? PALETTE.card : PALETTE.gray8,
                        }}
                        onClick={() => {
                          tip.clear();
                          onCell(on ? null : [m.key, c.key]);
                        }}
                        onMouseMove={(e: MouseEvent) =>
                          tip.hover(id, { x: e.clientX, y: e.clientY })
                        }
                        onMouseLeave={() => tip.leave()}
                        onFocus={(e: FocusEvent<HTMLButtonElement>) =>
                          tip.focus(id, centre(e.currentTarget))
                        }
                        onBlur={() => tip.leave()}
                      >
                        {v}
                      </button>
                    </td>
                  );
                })}
                <th scope="row">{mx.rowTotal(m.key)}</th>
              </tr>
            ))}
            <tr>
              <th scope="row" className={s.l}>
                all
              </th>
              {CLASSES.map((c) => (
                <th key={c.key} scope="row" className={s.c}>
                  {mx.colTotal(c.key)}
                </th>
              ))}
              <th scope="row">{mx.total}</th>
            </tr>
          </tbody>
        </table>
      </div>
      <p className={s.note}>
        Discovery method is an estimate: hand-classified for{" "}
        {head.conf.h + head.conf.m} entries ({head.conf.h} high, {head.conf.m}{" "}
        medium confidence); {head.conf.k} keep a keyword label. Click a cell to
        isolate it on the map. Miri and fuzzing found none of these first: Zig
        has no aliasing model for Miri to check, and Miri only confirmed three
        read-found races on Rust models of the same access pattern.
      </p>
    </div>
  );
}
