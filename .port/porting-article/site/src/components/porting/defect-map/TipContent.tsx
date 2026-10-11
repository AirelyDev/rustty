import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import { DEP_NAMES, projectName } from "./layout";
import {
  CLASS_BY,
  clip,
  type DmData,
  leafStats,
  type Matrix,
  METHOD_BY,
  RF_BY,
  STATUS_BY,
  type StatusRow,
} from "./model";
import s from "./DefectMap.module.css";

/** Props for the tooltip body. */
type TipContentProps = {
  id: string;
  data: DmData;
  mx: Matrix;
  status: readonly StatusRow[];
};

/** Renders a coloured class name. */
function ClassName({ c }: { c: string }) {
  return (
    <span style={{ color: PALETTE[CLASS_BY[c].role] }}>{CLASS_BY[c].name}</span>
  );
}

/**
 * The tooltip body for a target id: `m<i>` a defect mark, `w<i>` a waffle
 * square, `l<name>` a tile, `x<method>|<class>` a matrix cell, `s<key>` an
 * upstream-status segment.
 */
export default function TipContent({ id, data, mx, status }: TipContentProps) {
  const kind = id[0];
  const rest = id.slice(1);
  if (kind === "m" || kind === "w") {
    const d = data.defs[Number(rest)];
    if (kind === "w") {
      const rf = RF_BY[d.rf];
      return (
        <>
          ReleaseFast:{" "}
          <span style={{ color: PALETTE[rf.role] }}>{rf.name}</span> (estimated)
          <br />
          <b>{d.t}</b>
        </>
      );
    }
    return (
      <>
        <ClassName c={d.c} /> · {METHOD_BY[d.m].name} (estimated)
        <br />
        <b>{d.t}</b>
        <br />
        <span className={`${s.mono} ${s.k}`}>
          {d.f || DEP_NAMES[d.leaf] || ""}
        </span>
      </>
    );
  }
  if (kind === "l") {
    const st = leafStats(data, rest);
    return (
      <>
        <b className={s.mono}>{DEP_NAMES[rest] ?? rest}</b>
        {st.lines ? (
          <>
            <br />
            <span className={s.mono}>{fmtInt(st.lines)}</span> lines of Zig
          </>
        ) : null}
        <br />
        {st.n ? (
          st.byClass.map(([c, k], j) => (
            <span key={c}>
              {j ? " · " : ""}
              {k} <ClassName c={c} />
            </span>
          ))
        ) : (
          <span className={s.k}>no recorded defect</span>
        )}
        {st.n && st.lines ? (
          <>
            <br />
            <span className={s.k}>
              {((st.n / st.lines) * 1e4).toFixed(1)} per 10k lines
            </span>
          </>
        ) : null}
      </>
    );
  }
  if (kind === "x") {
    const [m, c] = rest.split("|");
    const items = mx.items(m, c);
    return (
      <>
        <b>{METHOD_BY[m].name}</b> × <ClassName c={c} />:{" "}
        <span className={s.mono}>{items.length}</span>
        <br />
        <span className={s.k}>
          {items.slice(0, 4).map((i) => (
            <span key={i}>
              • {clip(data.defs[i].t)}
              <br />
            </span>
          ))}
          {items.length > 4 ? `…and ${items.length - 4} more` : null}
        </span>
      </>
    );
  }
  const row = status.find((r) => r.key === rest);
  if (!row) return null;
  return (
    <>
      <b>{STATUS_BY[row.key].name}</b>: <span className={s.mono}>{row.n}</span>
      {row.key !== "nr" ? (
        <span className={s.k}>
          {row.items.map((i) => (
            <span key={i}>
              <br />• {clip(data.defs[i].t)}
              {data.defs[i].p !== "ghostty"
                ? ` (${projectName(data.defs[i].p)})`
                : ""}
            </span>
          ))}
          {row.key === "fixed" ? (
            <>
              <br />
              Highway's fix is in its 1.4.0 release; the Highway version Ghostty
              pins does not include it yet.
            </>
          ) : null}
        </span>
      ) : null}
    </>
  );
}
