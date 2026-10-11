import type { FocusEvent, MouseEvent } from "react";
import type { ChartTip } from "../ui/useChartTip";
import StatusMark from "./StatusMark";
import {
  cellView,
  dur,
  fmt,
  STATUS_INFO,
  toolName,
  type VmData,
} from "./model";
import s from "./VerificationMatrix.module.css";

/** Returns the viewport centre of an element (the keyboard tooltip anchor). */
function centre(el: Element) {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/** Props for the matrix. */
type MatrixProps = {
  data: VmData;
  pin: number;
  /** Index in data.runs of the selected cell's run, or null. */
  sel: number | null;
  onSelect: (k: number) => void;
  tip: ChartTip;
};

/**
 * The matrix of the selected reference commit: one row per tool, one column
 * per host. A cell shows its status (text, colour and shape), defect badges,
 * test count, wall time, date and "+n earlier" when several runs were
 * recorded. Hatched cells are tools the host cannot run; "no record" cells
 * have nothing recorded (not a failure). Hover or focus shows the record; a
 * click or Enter selects the cell for the detail panel.
 */
export default function Matrix({ data, pin, sel, onSelect, tip }: MatrixProps) {
  return (
    <table className={s.mx}>
      <caption className={s.srOnly}>
        Verification runs at reference commit {data.pins[pin].sha}: tools by
        host
      </caption>
      <thead>
        <tr>
          <td />
          {data.hosts.map((h) => (
            <th key={h.key} scope="col">
              {h.name}
              <span className={`${s.sub} ${s.hw}`}>{h.hw}</span>
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {data.tools.map((t) => (
          <tr key={t.key}>
            <th scope="row" className={s.rowHead}>
              {t.name}
            </th>
            {data.hosts.map((h) => {
              const c = cellView(data, pin, t.key, h.key);
              if (c.kind === "na") {
                return (
                  <td key={h.key} className={s.na} title={c.reason}>
                    <span className={s.sub}>not available</span>
                    <span className={s.srOnly}>: {c.reason}</span>
                  </td>
                );
              }
              if (c.kind === "none") {
                return (
                  <td key={h.key} className={s.none}>
                    <span className={s.sub}>no record</span>
                  </td>
                );
              }
              const { run: r } = c;
              const info = STATUS_INFO[r.st];
              const id = `r${c.index}`;
              const pending = r.st === "pending";
              /** Shows the record tooltip at the pointer. */
              const move = (e: MouseEvent) =>
                tip.hover(id, { x: e.clientX, y: e.clientY });
              return (
                <td
                  key={h.key}
                  className={`${s.cell} ${sel === c.index ? s.sel : ""} ${pending ? s.pend : ""}`}
                  style={
                    pending ? undefined : { background: `${info.color}1f` }
                  }
                >
                  <button
                    type="button"
                    className={s.cellBtn}
                    aria-pressed={sel === c.index}
                    aria-label={`${toolName(data, t.key)}, ${h.name}: ${info.cell}${r.targeted ? ", targeted packages only" : ""}${r.tests ? `, ${fmt(r.tests)} tests` : ""}${r.secs ? `, ${dur(r.secs)}` : ""}, ${r.date}`}
                    onClick={(e) => {
                      onSelect(c.index);
                      tip.tap(id, {
                        x: e.clientX || centre(e.currentTarget).x,
                        y: e.clientY || centre(e.currentTarget).y,
                      });
                    }}
                    onMouseMove={move}
                    onMouseLeave={tip.leave}
                    onFocus={(e: FocusEvent<HTMLButtonElement>) =>
                      tip.focus(id, centre(e.currentTarget))
                    }
                    onBlur={tip.leave}
                  >
                    <span className={s.st} style={{ color: info.color }}>
                      <StatusMark st={r.st} />
                      {info.cell}
                      {c.port ? (
                        <span className={`${s.bd} ${s.bdPort}`}>
                          port {c.port}
                        </span>
                      ) : null}
                      {c.gh ? (
                        <span className={`${s.bd} ${s.bdGh}`}>
                          Ghostty {c.gh}
                        </span>
                      ) : null}
                    </span>
                    <span className={s.meta}>
                      {r.targeted ? (
                        <span className={`${s.bd} ${s.bdTarget}`}>
                          targeted
                        </span>
                      ) : null}{" "}
                      {r.tests ? (
                        <span className={s.num}>{fmt(r.tests)} tests</span>
                      ) : null}{" "}
                      <span className={s.sub}>{dur(r.secs)}</span>{" "}
                      <span className={s.sub}>{r.date}</span>
                      {c.earlier ? (
                        <span className={s.sub}> +{c.earlier} earlier</span>
                      ) : null}
                    </span>
                  </button>
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
