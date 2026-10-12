import type { FocusEvent, MouseEvent } from "react";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import {
  CLASS_SHORT,
  CLASSES,
  CRATE_SHORT,
  CRATES,
  heatAlpha,
  type Heat,
} from "./model";
import s from "./MemorySafety.module.css";

/** Props for the crate x class table. */
type HeatTableProps = {
  heat: Heat;
  filter: readonly [string, string] | null;
  onFilter: (f: readonly [string, string] | null) => void;
  tip: ChartTip;
};

/**
 * The crate area x bug class table (defects): red alpha grows with the square
 * root of the defect count. A cell is a button; activating it highlights its
 * events in the lanes (others dim) and activating it again clears that.
 */
export default function HeatTable({
  heat,
  filter,
  onFilter,
  tip,
}: HeatTableProps) {
  /** Returns the viewport centre of an element. */
  const centre = (el: Element) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  };
  return (
    <table className={s.heat}>
      <caption className={s.srOnly}>
        Defects by crate area and bug class
      </caption>
      <thead>
        <tr>
          <th scope="col">
            <span className={s.srOnly}>crate area</span>
          </th>
          {CLASSES.map(([k]) => (
            <th key={k} scope="col">
              {CLASS_SHORT[k]}
            </th>
          ))}
          <th scope="col">all</th>
        </tr>
      </thead>
      <tbody>
        {CRATES.map(([c]) => (
          <tr key={c}>
            <th scope="row" className={s.rowHead}>
              {CRATE_SHORT[c]}
            </th>
            {CLASSES.map(([k]) => {
              const { n, events } = heat.cell(c, k);
              if (!n) {
                return (
                  <td key={k} className={s.zero}>
                    <span aria-hidden="true">·</span>
                    <span className={s.srOnly}>0</span>
                  </td>
                );
              }
              const on = filter?.[0] === c && filter[1] === k;
              const dark = n / heat.max > 0.45;
              return (
                <td key={k}>
                  <button
                    type="button"
                    className={on ? `${s.cell} ${s.cellOn}` : s.cell}
                    aria-pressed={on}
                    aria-label={`${CRATE_SHORT[c]}, ${CLASS_SHORT[k]}: ${n} defects in ${events} events`}
                    style={{
                      background: `rgba(224,108,117,${heatAlpha(n, heat.max)})`,
                      color: dark ? PALETTE.card : PALETTE.gray8,
                    }}
                    onClick={() => {
                      tip.clear();
                      onFilter(on ? null : [c, k]);
                    }}
                    onMouseMove={(e: MouseEvent) =>
                      tip.hover(`h${c}|${k}`, { x: e.clientX, y: e.clientY })
                    }
                    onMouseLeave={() => tip.leave()}
                    onFocus={(e: FocusEvent<HTMLButtonElement>) =>
                      tip.focus(`h${c}|${k}`, centre(e.currentTarget))
                    }
                    onBlur={() => tip.leave()}
                  >
                    {n}
                  </button>
                </td>
              );
            })}
            <th scope="row" className={s.total}>
              {heat.rowTotals[c]}
            </th>
          </tr>
        ))}
        <tr>
          <th scope="row" className={s.rowHead}>
            all
          </th>
          {CLASSES.map(([k]) => (
            <th key={k} scope="row" className={s.total}>
              {heat.colTotals[k]}
            </th>
          ))}
          <th scope="row" className={s.total}>
            {heat.total}
          </th>
        </tr>
      </tbody>
    </table>
  );
}
