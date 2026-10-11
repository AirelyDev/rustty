import data from "@/app/porting/data/checks-growth.json";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import Tooltip from "../ui/Tooltip";
import { TipBody } from "./Hot";
import { md, waffle } from "./model";
import StepChart from "./StepChart";
import s from "./StaticCharts.module.css";

/** RANGE is the date range of both step charts. */
const RANGE: [string, string] = ["2026-06-21", "2026-10-10"];

/** FAMILY_COLOURS colour the registry waffle by kind of check (the tier is the fill). */
const FAMILY_COLOURS: Record<string, string> = {
  gate: PALETTE.rust,
  "self-test": PALETTE.upstream,
  detector: PALETTE.gold,
  abi: PALETTE.blue,
};

/** WAFFLE_COLS is the number of cells per waffle row. */
const WAFFLE_COLS = 17;

/** COMMENT_LABELLED is how many of the largest crates are named on the comment bar. */
const COMMENT_LABELLED = 3;

/** The two step charts at a given viewBox width (one for wide screens, one for phones). */
function Steps({ W, idp }: { W: number; idp: string }) {
  const reg = data.registry;
  const scripts = data.scripts;
  return (
    <>
      <StepChart
        W={W}
        H={190}
        range={RANGE}
        yDomain={[0, 110]}
        yTicks={[0, 25, 50, 75, 100]}
        title="entries in the check registry"
        idp={`${idp}-reg`}
        ariaLabel={`Check registry entries from ${reg[0].n} to ${reg[reg.length - 1].n}, of which ${reg[reg.length - 1].self} self-tests.`}
        series={[
          {
            label: "all entries",
            colour: PALETTE.rust,
            points: reg.map((p) => ({ d: p.d, v: p.n })),
          },
          {
            label: "self-tests",
            colour: PALETTE.upstream,
            dash: "4 3",
            points: reg.map((p) => ({ d: p.d, v: p.self })),
          },
        ]}
        hots={reg.map((p, i) => ({
          d: p.d,
          label: `Registry on ${md(p.d)}`,
          title: `${md(p.d)}: ${p.n} entries`,
          rows: [
            `${p.self} of them self-tests`,
            i > 0
              ? `+${p.n - reg[i - 1].n} since ${md(reg[i - 1].d)}`
              : "The registry as first written.",
          ],
        }))}
      />
      <StepChart
        W={W}
        H={150}
        range={RANGE}
        yDomain={[0, 100]}
        yTicks={[0, 50, 100]}
        title="check scripts, and how many test themselves"
        idp={`${idp}-scr`}
        ariaLabel={`Check scripts from ${scripts[0].total} (${scripts[0].selfTest} self-testing) to ${scripts[scripts.length - 1].total} (${scripts[scripts.length - 1].selfTest} self-testing).`}
        series={[
          {
            label: "scripts",
            colour: PALETTE.gray5,
            points: scripts.map((p) => ({ d: p.d, v: p.total })),
          },
          {
            label: "with a self-test",
            colour: PALETTE.gold,
            dash: "4 3",
            points: scripts.map((p) => ({ d: p.d, v: p.selfTest })),
          },
        ]}
        hots={scripts.map((p) => ({
          d: p.d,
          label: `Check scripts on ${md(p.d)}`,
          title: `${md(p.d)}: ${p.total} check scripts`,
          rows: [
            `${p.selfTest} can test themselves (${Math.round((p.selfTest / p.total) * 100)}%)`,
          ],
        }))}
      />
    </>
  );
}

/** The 101-cell registry waffle: one cell per entry, coloured by kind, filled when it must pass. */
function Waffle() {
  const cells = waffle(data.composition, WAFFLE_COLS);
  const f = data.final;
  return (
    <div>
      <div className={s.miniHead}>
        The registry today: {f.entries} entries, {f.hard} must pass,{" "}
        {f.progress} tracked
      </div>
      <div className={s.waffle} aria-hidden="true">
        {cells.map((c, i) => {
          const g = data.composition[c.group];
          const colour = FAMILY_COLOURS[g.family];
          return (
            <span
              // biome-ignore lint/suspicious/noArrayIndexKey: one cell per registry entry, fixed order
              key={i}
              className={s.cell}
              style={
                g.tier === "hard"
                  ? { background: colour, borderColor: colour }
                  : { borderColor: colour }
              }
            />
          );
        })}
      </div>
      <ul className={s.groupList}>
        {data.composition.map((g) => (
          <li key={`${g.family}-${g.tier}`}>
            <Tooltip
              id={`cg-${g.family}-${g.tier}`}
              content={
                <TipBody title={`${g.n} · ${g.label}`} rows={g.examples} />
              }
            >
              <span
                className={s.cell}
                style={
                  g.tier === "hard"
                    ? {
                        background: FAMILY_COLOURS[g.family],
                        borderColor: FAMILY_COLOURS[g.family],
                      }
                    : { borderColor: FAMILY_COLOURS[g.family] }
                }
              />
              <b>{g.n}</b> {g.label}
            </Tooltip>
          </li>
        ))}
      </ul>
      <p className={s.miniNote}>
        Filled = must pass; outlined = tracked toward a target.
      </p>
    </div>
  );
}

/** The comment-fidelity bar (one segment per crate) and the C interface parity pills. */
function Pills() {
  const c = data.comments;
  return (
    <div className={s.pillRow}>
      <div className={s.pillBox}>
        <div className={s.pillHead}>
          <b>{fmtInt(c.total)}</b> distinctive Zig comments carried into Rust,{" "}
          <b>{c.missing}</b> missing
        </div>
        <div className={s.commentBar}>
          {c.crates.map((k, i) => (
            <span
              key={k.crate}
              className={s.commentSeg}
              style={{ flexGrow: k.n }}
            >
              <Tooltip
                id={`cg-c-${k.crate}`}
                content={
                  <TipBody
                    title={k.crate}
                    rows={[
                      `${fmtInt(k.n)} distinctive comments carried, ${k.missing} missing`,
                      `${k.review} matched with a borderline similarity score`,
                    ]}
                  />
                }
              >
                <span className={s.commentFill}>
                  {i < COMMENT_LABELLED ? k.crate.replace("rustty-", "") : ""}
                </span>
              </Tooltip>
            </span>
          ))}
        </div>
        <p className={s.miniNote}>
          {c.crates.length} crates. A distinctive comment is a phrase or
          sentence of five or more words; a further {c.notCarried} comments are
          recorded as not carried, each with its reason.
        </p>
      </div>
      <ul className={s.capi}>
        {data.capi.map((p) => (
          <li key={p.key}>
            <Tooltip
              id={`cg-api-${p.key}`}
              content={
                <TipBody
                  title={p.label}
                  rows={[
                    `Rustty ${fmtInt(p.rust)}, Ghostty ${fmtInt(p.ref)}: ${p.rust === p.ref ? "a full match" : "a gap"}.`,
                  ]}
                />
              }
            >
              <span className={s.capiPill}>
                <b>
                  {fmtInt(p.rust)}/{fmtInt(p.ref)}
                </b>{" "}
                {p.label}
              </span>
            </Tooltip>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Checks as the specification: the check registry and self-testing scripts
 * over time, today's registry by kind, comment fidelity and C interface
 * parity. Checks are described in words; their ids are never shown.
 */
export default function ChecksGrowth() {
  return (
    <div className={s.stack}>
      <div className={s.legendLine}>
        <span>
          <i className={s.swLine} style={{ background: PALETTE.rust }} />
          registry entries
        </span>
        <span>
          <i className={s.swDash} style={{ borderColor: PALETTE.upstream }} />
          self-tests in the registry
        </span>
        <span>
          <i className={s.swLine} style={{ background: PALETTE.gray5 }} />
          check scripts
        </span>
        <span>
          <i className={s.swDash} style={{ borderColor: PALETTE.gold }} />
          scripts with a self-test
        </span>
      </div>
      <div className={s.twoCol}>
        <div>
          <div className={s.wideOnly}>
            <Steps W={560} idp="cgw" />
          </div>
          <div className={s.narrowOnly}>
            <Steps W={330} idp="cgn" />
          </div>
        </div>
        <Waffle />
      </div>
      <Pills />
    </div>
  );
}
