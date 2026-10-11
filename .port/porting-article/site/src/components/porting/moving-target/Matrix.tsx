import {
  type FocusEvent,
  type MouseEvent,
  type ReactNode,
  useRef,
} from "react";
import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { useRovingFocus } from "../ui/useRovingFocus";
import type { Advance, MtData } from "./model";
import {
  cellAlpha,
  cellLabel,
  cellVal,
  int,
  matrixMax,
  type Metric,
  rowLabel,
  rowTotal,
  type Totals,
} from "./stats";
import s from "./MovingTarget.module.css";

/** A meta row of the matrix: its label, a cell renderer and the "all moves" total. */
type MetaRow = {
  label: string;
  cell: (a: Advance) => ReactNode;
  total: ReactNode;
};

/** Returns the meta rows (commits, lines, files, wait, catch-up length, Rustty commits, missing comments) with their totals. */
function metaRows(t: Totals): MetaRow[] {
  const dash = <span style={{ color: PALETTE.gray3 }}>—</span>;
  return [
    {
      label: "upstream non-merge commits",
      cell: (a) => int(a.nm),
      total: int(t.nonMerge),
    },
    {
      label: "lines + / −",
      cell: (a) => (
        <>
          <span style={{ color: PALETTE.pass }}>+{int(a.ins)}</span>
          <br />
          <span style={{ color: PALETTE.fail }}>−{int(a.del)}</span>
        </>
      ),
      total: (
        <>
          <span style={{ color: PALETTE.pass }}>+{int(t.matrixIns)}</span>
          <br />
          <span style={{ color: PALETTE.fail }}>−{int(t.matrixDel)}</span>
        </>
      ),
    },
    { label: "files changed", cell: (a) => int(a.files), total: int(t.files) },
    {
      label: "wait: upstream → reference moved",
      cell: (a) => (
        <span style={{ color: PALETTE.gold }}>{a.lag.toFixed(1)} h</span>
      ),
      total: (
        <>
          median
          <br />
          {t.medianLag} h
        </>
      ),
    },
    { label: "until the catch-up finished", cell: (a) => a.dur, total: "" },
    {
      label: "Rustty commits during the catch-up",
      cell: (a) => int(a.era),
      total: int(t.era),
    },
    {
      label: "comments missing on arrival",
      cell: (a) =>
        a.cm === null ? (
          dash
        ) : (
          <span style={{ color: PALETTE.fail }}>{int(a.cm)}</span>
        ),
      total: "",
    },
  ];
}

/** Props for the per-move matrix. */
type MatrixProps = {
  data: MtData;
  totals: Totals;
  sel: number;
  metric: Metric;
  /** Row under the pointer or focus, or null. */
  hoverRow: number | null;
  onHoverRow: (r: number | null) => void;
  onSelect: (i: number) => void;
  tip: ChartTip;
};

/**
 * The per-move matrix: meta rows, then one row per top-level directory whose
 * cell is the lines changed (teal alpha on a log scale) or the commits
 * touching it, with a row total and mini bar. Column headers and cells select
 * the move (linked with the size chart); a cell shows its detail on hover,
 * focus or tap. The cells are a single Tab stop with arrow-key movement.
 */
export default function Matrix({
  data,
  totals,
  sel,
  metric,
  hoverRow,
  onHoverRow,
  onSelect,
  tip,
}: MatrixProps) {
  const max = matrixMax(data, metric);
  const maxTot = Math.max(...data.rows.map((r) => rowTotal(r, metric)));
  const meta = metaRows(totals);
  const bodyRef = useRef<HTMLTableSectionElement>(null);
  // The cells are one Tab stop: arrow keys move between them (up/down by row).
  useRovingFocus(bodyRef, { grid: true });
  /** Returns the viewport centre of an element. */
  const centre = (el: Element) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  };
  return (
    <>
      <table className={s.matrix}>
        <caption className={s.srOnly}>
          Per reference move: what changed upstream by top-level directory, and
          how fast Rustty moved
        </caption>
        <thead>
          <tr>
            <th scope="col" className={s.rowHead}>
              <span className={s.srOnly}>measure</span>
            </th>
            {data.adv.map((a, i) => (
              <th
                key={a.n}
                scope="col"
                className={i === sel ? s.on : undefined}
              >
                <button
                  type="button"
                  className={s.colBtn}
                  aria-pressed={i === sel}
                  onClick={() => onSelect(i)}
                >
                  move {a.n}
                  <br />
                  <span className={s.sha}>{a.to}</span>
                </button>
              </th>
            ))}
            <th scope="col" className={s.leftCol}>
              all {data.adv.length}
            </th>
          </tr>
        </thead>
        <tbody ref={bodyRef}>
          {meta.map((m) => (
            <tr key={m.label} className={s.meta}>
              <th scope="row" className={s.rowHead}>
                {m.label}
              </th>
              {data.adv.map((a, i) => (
                <td key={a.n} className={i === sel ? s.colOn : undefined}>
                  {m.cell(a)}
                </td>
              ))}
              <td className={s.leftCol}>{m.total}</td>
            </tr>
          ))}
          <tr className={s.sep}>
            <td colSpan={data.adv.length + 2} />
          </tr>
          {data.rows.map((r, ri) => (
            <tr key={r.a} className={hoverRow === ri ? s.hl : undefined}>
              <th scope="row" className={s.rowHead}>
                {rowLabel(r)}
              </th>
              {data.adv.map((a, i) => {
                const cv = cellVal(r.v[i], metric);
                const al = cellAlpha(cv, max);
                const id = `c${ri}|${i}`;
                const cls = `${s.cell} ${cv ? "" : s.zero} ${i === sel ? s.colOn : ""}`;
                const style = {
                  background: cv
                    ? `rgba(100,216,198,${al.toFixed(3)})`
                    : "#0f1110",
                  color: al > 0.55 ? PALETTE.card : undefined,
                };
                /** Highlights the row and shows the cell tooltip at the pointer. */
                const enter = (e: MouseEvent) => {
                  onHoverRow(ri);
                  tip.hover(id, { x: e.clientX, y: e.clientY });
                };
                /** Clears the row highlight and the transient tooltip. */
                const leave = () => {
                  onHoverRow(null);
                  tip.leave();
                };
                if (!cv) {
                  return (
                    // biome-ignore lint/a11y/useKeyWithClickEvents: untouched cells repeat what their column header does
                    <td
                      key={a.n}
                      className={cls}
                      style={style}
                      onMouseMove={enter}
                      onMouseLeave={leave}
                      onClick={() => onSelect(i)}
                    >
                      <span aria-hidden="true">·</span>
                      <span className={s.srOnly}>untouched</span>
                    </td>
                  );
                }
                return (
                  <td key={a.n} className={cls} style={style}>
                    <button
                      type="button"
                      className={s.cellBtn}
                      aria-label={`${rowLabel(r)}, move ${a.n}: ${int(r.v[i]?.[0] ?? 0)} inserted, ${int(r.v[i]?.[1] ?? 0)} deleted, ${r.v[i]?.[3]} commits touching`}
                      onClick={() => onSelect(i)}
                      onMouseMove={enter}
                      onMouseLeave={leave}
                      onFocus={(e: FocusEvent<HTMLButtonElement>) => {
                        onHoverRow(ri);
                        tip.focus(id, centre(e.currentTarget));
                      }}
                      onBlur={leave}
                    >
                      {cellLabel(cv, metric)}
                    </button>
                  </td>
                );
              })}
              <td className={s.bar}>
                <span
                  style={{
                    width: `${Math.max(2, (rowTotal(r, metric) / maxTot) * 44)}px`,
                  }}
                />
                {int(rowTotal(r, metric))}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className={s.note}>
        The all-moves lines include move {totals.driftN}'s +
        {int(totals.driftIns)} / −{int(totals.driftDel)} (the 2-commit drift);
        the header tile leaves it out.
      </p>
    </>
  );
}
