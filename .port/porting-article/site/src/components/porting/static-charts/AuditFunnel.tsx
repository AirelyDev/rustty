import data from "@/app/porting/data/audit-funnel.json";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import Tooltip from "../ui/Tooltip";
import { TipBody } from "./Hot";
import Meter from "./Meter";
import { funnelView } from "./model";
import s from "./StaticCharts.module.css";

/** STAGE_ACTORS names who produced each funnel stage, with its bar colour. */
const STAGE_ACTORS = [
  { who: "Codex", colour: PALETTE.blue },
  { who: "Claude", colour: PALETTE.rust },
  { who: "Claude", colour: PALETTE.rust },
  { who: "Claude", colour: PALETTE.rust },
  { who: "Claude", colour: PALETTE.pass },
];

/** SEVERITY_NOTES describe Codex's severity scale in plain words. */
const SEVERITY_NOTES: Record<string, string> = {
  P1: "most severe",
  P2: "high",
  P3: "medium",
  P4: "low",
};

/** SEVERITY_COLOURS shade the severity bar from most to least severe. */
const SEVERITY_COLOURS = [
  PALETTE.fail,
  PALETTE.orange,
  PALETTE.yellow,
  PALETTE.gray5,
];

/**
 * A second opinion from Codex: the funnel from registry rows to verified
 * fixes, the severity split of the distinct findings, and Codex's weekly-limit
 * meter during the two audit runs with the audit's events marked.
 */
export default function AuditFunnel() {
  const rows = funnelView(data.stages);
  const sev = Object.entries(data.severity);
  const sevTotal = sev.reduce((a, [, n]) => a + n, 0);
  const rem = data.remediation;
  return (
    <div className={s.stack}>
      <div className={s.funnelGrid}>
        <ol className={s.funnel}>
          {rows.map((r, i) => (
            <li key={r.label}>
              <Tooltip
                id={`funnel-${i}`}
                content={
                  <TipBody
                    title={`${fmtInt(r.n)} · ${r.label}`}
                    rows={[
                      r.note,
                      i > 0
                        ? `${Math.round(r.ofPrev * 1000) / 10}% of the stage before`
                        : "The start of the funnel.",
                      r.provenance === "documented"
                        ? "As reported in Codex's final answer and the remediation commit."
                        : "Counted from the vetting state file.",
                    ]}
                  />
                }
              >
                <span className={s.fRow}>
                  <span className={s.fLabel}>
                    {r.label}
                    <span className={s.fWho}>{STAGE_ACTORS[i].who}</span>
                  </span>
                  <span className={s.fTrack}>
                    <span
                      className={s.fBar}
                      style={{
                        width: `${Math.max(r.frac * 100, 0.8)}%`,
                        background: STAGE_ACTORS[i].colour,
                      }}
                    />
                    <span className={s.fNum}>{fmtInt(r.n)}</span>
                  </span>
                </span>
              </Tooltip>
            </li>
          ))}
        </ol>
        <div className={s.sevBox}>
          <div className={s.miniHead}>
            The {sevTotal} distinct findings by Codex's severity
          </div>
          <div className={s.sevBar}>
            {sev.map(([k, n], i) => (
              <span
                key={k}
                className={s.sevSeg}
                style={{ flexGrow: n, background: SEVERITY_COLOURS[i] }}
              >
                <Tooltip
                  id={`sev-${k}`}
                  content={
                    <TipBody
                      title={`${k} (${SEVERITY_NOTES[k]}): ${n} findings`}
                      rows={[
                        `${Math.round((n / sevTotal) * 100)}% of the ${sevTotal} distinct findings, grouped by root cause.`,
                      ]}
                    />
                  }
                >
                  <span className={s.sevText}>
                    {k} {n}
                  </span>
                </Tooltip>
              </span>
            ))}
          </div>
          <p className={s.miniNote}>
            P1 is the most severe, P4 the least. Claude agreed with Codex's
            severity on 144 of 147.
          </p>
          <p className={s.miniNote}>
            Fixed in one remediation commit, <CommitLink hash={rem.commit} /> (
            {rem.d}): {fmtInt(rem.files)} files, {fmtInt(rem.tests)} tests
            passing, {fmtInt(rem.provingTests)} new tests that prove a fix.
          </p>
        </div>
      </div>
      <div className={s.miniHead}>
        Codex weekly limit used during the audit: median reading per hour (UTC),
        band = every reading in the hour
      </div>
      <div className={s.wideOnly}>
        <Meter W={960} H={230} rows={2} idp="mw" />
      </div>
      <div className={s.narrowOnly}>
        <Meter W={330} H={250} rows={4} idp="mn" />
      </div>
    </div>
  );
}
