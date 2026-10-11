import { CLASS_SHORT, CLASSES, CRATES, type Heat, type MsData } from "./model";
import s from "./MemorySafety.module.css";

/**
 * The tooltip body for a target id: `e<i>` an event, `r<i>` a rule flag,
 * `h<crate>|<class>` a heat-table cell.
 */
export default function TipContent({
  id,
  data,
  heat,
}: {
  id: string;
  data: MsData;
  heat: Heat;
}) {
  if (id.startsWith("e")) {
    const e = data.events[Number(id.slice(1))];
    return (
      <>
        <b className={s.mono}>{e.d}</b> · {e.n} defect{e.n > 1 ? "s" : ""}
        <br />
        {e.sum}
        <br />
        <span className={s.k}>
          {e.tool} · {e.area} · {e.scope}
        </span>
      </>
    );
  }
  if (id.startsWith("r")) {
    const i = Number(id.slice(1));
    const r = data.rules[i];
    return (
      <>
        <b>Rule {i + 1}</b> ·{" "}
        <span className={s.mono}>
          {r.d} · {r.ref}
        </span>
        <br />
        {r.t}
      </>
    );
  }
  const [c, k] = id.slice(1).split("|");
  const { n, events } = heat.cell(c, k);
  const crate = CRATES.find((x) => x[0] === c);
  const cls = CLASSES.find((x) => x[0] === k);
  return (
    <>
      <b>{crate?.[1] ?? c}</b>
      <br />
      {cls?.[1] ?? CLASS_SHORT[k]}: <span className={s.mono}>{n}</span> defects
      in {events} events
    </>
  );
}
