import { ERA_METRICS } from "./Eras";
import { fmtMoney, fmtTok, leverage, paidRows, pct } from "./model";
import {
  BUSY_MIN_HOURS,
  delegation,
  eraStats,
  type ThreadKind,
  tokenMix,
} from "./stats";
import type { TipProps } from "./TipContent";
import { KIND_LABEL } from "./Tokens";

/** The paid chart's leverage tooltip: central and the consistent bounds, with the low end at the top of the paid range. */
function LeverageTip({ d }: Pick<TipProps, "d">) {
  const lev = leverage(d);
  const [lo, mid, hi] = d.api.total;
  return (
    <>
      <b>~{Math.round(lev.central)}× list price per dollar paid</b>
      <br />
      {fmtMoney(mid)} API-equivalent ÷ {fmtMoney(d.paid.central)} paid.
      Consistent bounds: {fmtMoney(lo)} ÷ ${d.paid.cycles * d.paid.price * 0.5}{" "}
      (five billing cycles at 50%, if the plan renews on the 28th) ≈{" "}
      {Math.round(lev.low)}× to {fmtMoney(hi)} ÷ {fmtMoney(d.paid.central)} ≈{" "}
      {Math.round(lev.high)}×; about {Math.round(lev.lowAtTop)}× if the paid
      side is the full ~${d.paid.high}. The API-equivalent is a reference value,
      not money spent.
    </>
  );
}

/** The tooltip of a token kind: tokens, dollars and $ per million tokens. */
function MixTip({ i, d, sc }: { i: number } & Pick<TipProps, "d" | "sc">) {
  const r = tokenMix(d, sc)[i];
  if (!r) return null;
  return (
    <>
      <b>{r.name}</b>
      <br />
      {fmtTok(r.tok)} tokens = {pct(r.shareTok)} of tokens
      <br />
      {fmtMoney(r.usd)} = {pct(r.shareUsd)} of list-price cost
      <br />
      {fmtMoney(r.perM)} per million tokens
    </>
  );
}

/** The tooltip of an era point: the metric with its range, the cost split and the era's outputs. */
function EraTip({ mi, i, d }: { mi: number; i: number } & Pick<TipProps, "d">) {
  const e = eraStats(d)[i];
  const m = ERA_METRICS[mi];
  if (!e || !m) return null;
  const v = m.of(e);
  const f = (x: number) => (m.cost ? `$${x.toFixed(2)}` : x.toFixed(2));
  return (
    <>
      <b>{e.name}</b> ({e.start} → {e.end})
      <br />
      {m.name}: {f(v[1])}
      {m.cost ? ` (range ${f(v[0])}–${f(v[2])})` : ""}
      <br />
      list-price cost {fmtMoney(e.cost[1])} = recovered {fmtMoney(e.recovered)}{" "}
      + modeled {fmtMoney(e.cost[1] - e.recovered)}
      <br />
      {e.commits} non-merge commits · {e.prompts} prompts ·{" "}
      {e.codeLines.toLocaleString("en-US")} code lines added
      {m.cost && e.mostlyModeled ? (
        <>
          <br />
          <i>
            Mostly modeled: in this era $ per unit mainly reflects the
            maxed-subscription assumption.
          </i>
        </>
      ) : null}
    </>
  );
}

/** Tooltip bodies of the lower panels: paid rows, leverage, token kinds, delegation, eras, context, concurrency, the Codex meter and events. */
export default function PanelTip({ id, d, sc }: TipProps) {
  const [kind, a, b] = id.split(":");
  const i = Number(a);
  if (kind === "paid") {
    const r = paidRows(d)[i];
    return r ? (
      <>
        <b>{r.name}</b> · {fmtMoney(r.v)}
        <br />
        {r.how}
      </>
    ) : null;
  }
  if (kind === "lev") return <LeverageTip d={d} />;
  if (kind === "mix") return <MixTip i={i} d={d} sc={sc} />;
  if (kind === "del") {
    const row = delegation(d, sc).rows[i];
    const k = b as ThreadKind;
    return row ? (
      <>
        <b>
          {row.name} · {KIND_LABEL[k]}
        </b>
        <br />
        {fmtMoney(row.parts[k])} = {pct(row.parts[k] / row.total)} of {row.name}
      </>
    ) : null;
  }
  if (kind === "era") return <EraTip mi={i} i={Number(b)} d={d} />;
  if (kind === "ctx") {
    const c = d.ctx[i];
    return c ? (
      <>
        <b>week of {c.w}</b> · {c.tool === "codex" ? "Codex" : "Claude"},{" "}
        {c.n.toLocaleString("en-US")} calls
        <br />
        median {fmtTok(c.med)} · p90 {fmtTok(c.p90)}
        {c.tool === "claude" ? ` · max ${fmtTok(c.mx)}` : ""}
        {c.n < 100 ? (
          <>
            <br />
            <i>few calls; noisy</i>
          </>
        ) : null}
      </>
    ) : null;
  }
  if (kind === "act") {
    const r = d.act[i];
    return r ? (
      <>
        <b>{r.d}</b> ·{" "}
        {r.s.replace("mac/", "Mac ").replace("rvm/", "Linux VM ")}
        <br />
        {r.th} threads · {r.b.toFixed(2)} busy h · {r.a.toFixed(2)} thread h
        <br />
        concurrency {(r.a / r.b).toFixed(1)}×
        {r.b < BUSY_MIN_HOURS ? (
          <>
            <br />
            <i>under 0.5 busy hours: small-denominator artifact, not a peak</i>
          </>
        ) : null}
      </>
    ) : null;
  }
  if (kind === "meter") {
    const m = d.meter[i];
    return m ? (
      <>
        <b>{m[0].replace("T", " ")}:00 UTC</b>
        <br />
        weekly meter: median {m[1]}% (readings {m[2]}–{m[3]}%)
        <br />
        cumulative Rustty Codex: {fmtMoney(m[4])}
        {m[0] >= d.meterNote.from && m[0] <= d.meterNote.to ? (
          <>
            <br />
            <i>{d.meterNote.text}</i>
          </>
        ) : null}
      </>
    ) : null;
  }
  if (kind === "cev") {
    const e = d.cev[i];
    return e ? (
      <>
        <b>{e.t.replace("T", " ")} UTC</b>
        <br />
        {e.text}
      </>
    ) : null;
  }
  return null;
}
