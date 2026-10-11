import { moveName, type MtData, tipAt } from "./model";
import { timeOfTipId } from "./TimeChart";
import { cellVal, int, rowLabel } from "./stats";
import s from "./MovingTarget.module.css";

/** Formats a ratio as "0.12×". */
function ratio(x: number): string {
  return `${x.toFixed(2)}×`;
}

/** One label/value line of a tooltip. */
function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className={s.tipRow}>
      <span>{k}</span>
      <span className={s.mono}>{v}</span>
    </div>
  );
}

/**
 * The tooltip body for a target id: `t<n>` a moment on the size chart,
 * `c<row>|<move>` a matrix cell.
 */
export default function TipContent({ id, data }: { id: string; data: MtData }) {
  if (id.startsWith("t")) {
    const x = tipAt(data, timeOfTipId(id));
    return (
      <>
        <b className={s.mono}>{x.date}</b>
        <Row k="Ghostty main" v={int(x.main)} />
        <Row k={`pinned ${x.pinSha}`} v={int(x.pin)} />
        <Row k="reference trails main by" v={int(x.trail)} />
        {x.rs !== null && x.ratio !== null ? (
          <>
            <Row k="Rustty .rs lines" v={int(x.rs)} />
            <Row k="Rustty ÷ pinned Ghostty" v={ratio(x.ratio)} />
          </>
        ) : null}
      </>
    );
  }
  const [r, c] = id.slice(1).split("|").map(Number);
  const row = data.rows[r];
  const a = data.adv[c];
  const cell = row.v[c];
  const head = (
    <>
      <b>{rowLabel(row)}</b> · {moveName(a)}
    </>
  );
  if (!cell) {
    return (
      <>
        {head}
        <br />
        <span className={s.k}>untouched in this range</span>
      </>
    );
  }
  const share = (cellVal(cell, "churn") / (a.ins + a.del)) * 100;
  return (
    <>
      {head}
      <Row k="lines + / −" v={`+${int(cell[0])} / −${int(cell[1])}`} />
      <Row k="share of range churn" v={`${share.toFixed(1)}%`} />
      <Row k="files" v={String(cell[2])} />
      <Row k="non-merge commits touching" v={`${cell[3]} of ${a.nm}`} />
      {row.other ? (
        <div className={`${s.k} ${s.tipOther}`}>
          {data.otherNames.join(", ")}
        </div>
      ) : null}
    </>
  );
}
