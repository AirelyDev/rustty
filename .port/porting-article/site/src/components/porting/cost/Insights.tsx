import s from "./Cost.module.css";
import { type CostData, fmtMoney, fmtTok, maxedWeek, pct } from "./model";
import {
  codexPerPoint,
  CTX_MIN_CALLS,
  concurrencyPeak,
  contextGrowth,
  delegation,
  eraStats,
  kindTable,
  tokenMix,
  topModels,
  unitRange,
} from "./stats";

/**
 * The eight computed insights, each number taken from the data (Rustty
 * scope): what a maxed week buys, which model dominated, why cache reads are
 * the bill, how delegation changed unit cost, context and concurrency, the
 * cheapest era, the Codex meter, and the check on the 50% assumption.
 */
export default function Insights({ d }: { d: CostData }) {
  const week = maxedWeek(d);
  const [m1, m2, m3] = topModels(d);
  const opus48 = d.qw.reduce((a, q) => a + (q.mc["Opus 4.8"] ?? 0), 0);
  const mix = tokenMix(d, "rustty");
  const cache = mix.find((r) => r.key === "cache_read");
  const out = mix.find((r) => r.key === "output");
  const del = delegation(d, "rustty").delegated;
  const kinds = kindTable(d, "rustty");
  const sub = kinds.find((k) => k.key === "claude/subagent");
  const main = kinds.find((k) => k.key === "claude/main");
  const ctx = contextGrowth(d);
  const p90 = Math.max(
    ...d.ctx
      .filter((c) => c.tool === "claude" && c.n >= CTX_MIN_CALLS)
      .map((c) => c.p90),
  );
  const peak = concurrencyPeak(d);
  const eras = eraStats(d);
  const orch = eras[2];
  const units = unitRange(d);
  const codex = codexPerPoint(d);
  if (!cache || !out || !sub || !main) return null;
  return (
    <div className={s.ins}>
      <div className={s.lbl}>Computed insights</div>
      <ol>
        <li>
          A maxed quota week bought <b>{fmtMoney(d.ceil.usd[1])}</b> of
          list-price usage in the central case ({fmtMoney(d.ceil.usd[0])}–
          {fmtMoney(d.ceil.usd[2])}). Against the plan's{" "}
          <b>{fmtMoney(week.perWeek)}/week</b> that is{" "}
          <b>{Math.round(week.central)}×</b> ({Math.round(week.range[0])}–
          {Math.round(week.range[1])}×). In tokens, that week was{" "}
          <b>{fmtTok(d.ceil.tok[1])}</b> billable tokens, of which only{" "}
          <b>{fmtTok(d.ceil.out[1])}</b> were output.
        </li>
        <li>
          <b>{m1.name} dominated recovered list-price usage</b>:{" "}
          <b>{fmtMoney(m1.usd)}</b>, {pct(m1.share, 0)} of the{" "}
          {fmtMoney(d.api.recovered)} measured. Next were {m2.name} at{" "}
          {fmtMoney(m2.usd)} ({pct(m2.share, 0)}) and {m3.name} at{" "}
          {fmtMoney(m3.usd)} ({pct(m3.share, 0)}). In the lost window, commit
          trailers point to <b>Opus 4.8</b>, with {opus48} commits through
          07-23.
        </li>
        <li>
          <b>Cache reads dominate the list-price total.</b> They are{" "}
          {pct(cache.shareTok)} of tokens but {pct(cache.shareUsd, 0)} of
          list-price dollars, because a cache-read token costs{" "}
          <b>${cache.perM.toFixed(2)}/M</b> and an output token{" "}
          <b>${out.perM.toFixed(1)}/M</b>, about{" "}
          {Math.round(out.perM / cache.perM)}× more. Output is{" "}
          {pct(out.shareTok)} of tokens and {pct(out.shareUsd)} of list-price
          dollars.
        </li>
        <li>
          <b>Delegation made calls cheaper.</b> {pct(del)} of list-price usage
          ran in subagents or workflow runs. A Claude subagent call averaged{" "}
          <b>${sub.perCall.toFixed(3)}</b>, against{" "}
          <b>${main.perCall.toFixed(3)}</b> on the main thread (−
          {Math.round((1 - sub.perCall / main.perCall) * 100)}%), even with a
          larger median context ({fmtTok(sub.ctx ?? 0)} vs{" "}
          {fmtTok(main.ctx ?? 0)}): <b>${sub.perM.toFixed(2)}</b> vs{" "}
          <b>${main.perM.toFixed(2)}</b> per million tokens.
        </li>
        <li>
          <b>Context per call grew {ctx.ratio.toFixed(2)}×</b>: the median went
          from {fmtTok(ctx.from.med)} tokens (week of {ctx.from.w.slice(5)}) to{" "}
          {fmtTok(ctx.to.med)} (week of {ctx.to.w.slice(5)}, the orchestrator
          era), with p90 near {(p90 / 1e6).toFixed(1)}M and a maximum of ~
          {fmtTok(ctx.max)}. Peak concurrency was{" "}
          <b>{peak.ratio.toFixed(1)}×</b> on {peak.d.slice(5)}, with{" "}
          {peak.threads} threads.
        </li>
        <li>
          <b>The orchestrator era used the least list-price usage per unit:</b>{" "}
          <b>${orch.perCommit[1].toFixed(1)} per commit</b> (range $
          {orch.perCommit[0].toFixed(1)}–${orch.perCommit[2].toFixed(1)}) and{" "}
          {orch.commitsPerPrompt.toFixed(1)} commits per human prompt. The
          earlier eras are mostly modeled (${eras[0].perCommit[1].toFixed(1)}{" "}
          and ${eras[1].perCommit[1].toFixed(1)} per commit, central). In the
          recovered quota weeks the measured list-price usage ran $
          {units[0].toFixed(1)}–$
          {units[1].toFixed(1)} per commit.
        </li>
        <li>
          <b>Codex's weekly meter</b> reached 99% within the ~
          {d.codexAudit.hours}-hour audit, at roughly{" "}
          <b>${Math.round(codex.perPoint)} per meter point</b>. So one Pro week
          of capacity was about ${((codex.perPoint * 100) / 1000).toFixed(1)}k
          at list price, nearly all of it on the Rustty audit.
        </li>
        <li>
          <b>The 50% assumption holds up.</b> Rustty was{" "}
          {(d.share * 100).toFixed(1)}% of the four complete recovered quota
          weeks. Per-week prompt shares give {fmtMoney(d.sensitivity)} instead
          of {fmtMoney(d.api.modeled[1])} for the lost window, because quiet
          weeks such as 08-10 and 08-17 had only a 6–8% Rustty prompt share.
        </li>
      </ol>
    </div>
  );
}
