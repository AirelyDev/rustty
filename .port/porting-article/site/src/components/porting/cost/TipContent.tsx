import s from "./Cost.module.css";
import {
  CAT_COLOR,
  CAT_NAME,
  ceiling,
  type CostData,
  fmtMoney,
  fmtVal,
  pct,
  Q0,
  type Scope,
  type Unit,
  WEEK_MS,
  weekBars,
} from "./model";
import PanelTip from "./PanelTip";
import { dayLabel } from "./weekly-geo";
import { COV_NAME } from "./WeeklyStrips";

/** Props shared by every tooltip body. */
export type TipProps = { id: string; d: CostData; sc: Scope; u: Unit };

/** The tooltip of quota week i: recovered values by category, the modeled range, prompts, commits, trailers and delegation. */
function WeekTip({
  i,
  d,
  sc,
  u,
}: {
  i: number;
  d: CostData;
  sc: Scope;
  u: Unit;
}) {
  const b = weekBars(d, sc, u)[i];
  const q = d.qw[i];
  if (!b || !q) return null;
  const [el, ec, eh] = b.ext;
  const mc =
    Object.entries(q.mc)
      .sort((x, y) => y[1] - x[1])
      .map(([k, v]) => `${k === "none" ? "no trailer" : k} ${v}`)
      .join(", ") || "none";
  const dl = q.dl[sc === "rustty" ? 0 : 1];
  return (
    <>
      <b>Quota week from {dayLabel(Q0 + i * WEEK_MS)} 22:00 UTC</b> (
      {sc === "rustty" ? "Rustty" : "all projects"})
      <table className={s.tipTable}>
        <tbody>
          {b.rec.map(([c, v]) => (
            <tr key={c}>
              <td>
                <i className={s.sw} style={{ background: CAT_COLOR[c] }} />
                {CAT_NAME[c] ?? c}
              </td>
              <td>{fmtVal(v, u)}</td>
            </tr>
          ))}
          {ec > 0 ? (
            <>
              <tr>
                <td>Mac Claude, modeled (central)</td>
                <td>{fmtVal(ec, u)}</td>
              </tr>
              <tr className={s.dim}>
                <td>low – high</td>
                <td>
                  {fmtVal(el, u)} – {fmtVal(eh, u)}
                </td>
              </tr>
            </>
          ) : null}
          <tr>
            <td>
              <b>total</b>
            </td>
            <td>
              <b>{fmtVal(b.total, u)}</b>
            </td>
          </tr>
        </tbody>
      </table>
      Lost-transcript share of week: {pct(q.lf, 0)}
      <br />
      Rustty prompts {q.p} · other-project prompts {q.po} · non-merge commits{" "}
      {q.c}
      <br />
      Commit trailers: {mc}
      {dl !== null ? (
        <>
          <br />
          Delegated (subagents + workflows): {pct(dl, 0)} of recovered $
        </>
      ) : null}
    </>
  );
}

/** The ceiling band's tooltip: the three measured maxed weeks. */
function BandTip({ d, sc, u }: { d: CostData; sc: Scope; u: Unit }) {
  const c = ceiling(d, sc, u);
  return (
    <>
      <b>Measured ceiling of a maxed week</b>
      {sc === "rustty" ? " × 50% (Rustty share)" : " (all projects)"}
      <br />
      low {fmtVal(c[0], u)} (week 09-29, weekly limit on day 3, then locked out)
      <br />
      central {fmtVal(c[1], u)} (week 09-08, weekly limit about 15 h before
      reset)
      <br />
      high {fmtVal(c[2], u)} (week 09-22, the largest complete week)
    </>
  );
}

/** A coverage day's tooltip: what survives, prompts, commits, recovered and modeled $. */
function DayTip({ i, d }: { i: number; d: CostData }) {
  const c = d.cov[i];
  if (!c) return null;
  return (
    <>
      <b>{c[0]}</b> · {COV_NAME[c[1]]}
      <br />
      Rustty prompts {c[2]} · other-project prompts {c[6]} · commits {c[3]}
      <br />
      recovered {fmtMoney(c[4])}
      {c[5] ? ` · modeled ${fmtMoney(c[5])} (central)` : ""}
    </>
  );
}

/** A limit signal's tooltip: when, what kind, which project, threads, and the words. */
function EventTip({ i, d }: { i: number; d: CostData }) {
  const e = d.ev[i];
  if (!e) return null;
  const system = e.k !== "prompt" && e.k !== "slash";
  const kind = system
    ? "Claude Code limit message"
    : e.k === "slash"
      ? "slash command"
      : "author's words";
  return (
    <>
      <b>{e.t.replace("T", " ")} UTC</b> · {kind} ·{" "}
      {e.r ? "Rustty" : "another project"}
      {e.n && e.n > 10 ? (
        <>
          <br />
          {e.n} agent threads received it
        </>
      ) : e.n && e.n > 1 ? (
        <>
          <br />
          {e.n} messages
        </>
      ) : null}
      <br />
      <span className={s.quote}>
        {e.k === "prompt" ? `“${e.s}”` : (e.f ?? e.s)}
      </span>
    </>
  );
}

/** The tooltip body for a mark id ("w:3", "band", "day:12", "ev:4", or a panel id). */
export default function TipContent({ id, d, sc, u }: TipProps) {
  const [kind, a] = id.split(":");
  const i = Number(a);
  if (kind === "w") return <WeekTip i={i} d={d} sc={sc} u={u} />;
  if (kind === "band") return <BandTip d={d} sc={sc} u={u} />;
  if (kind === "day") return <DayTip i={i} d={d} />;
  if (kind === "ev") return <EventTip i={i} d={d} />;
  return <PanelTip id={id} d={d} sc={sc} u={u} />;
}
