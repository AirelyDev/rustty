import data from "@/app/porting/data/strategy-timeline.json";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import Lanes from "./Lanes";
import { LaneGlyph } from "./marks";
import { lastPerDay, md } from "./model";
import StepChart from "./StepChart";
import s from "./StaticCharts.module.css";

/** LANE_LEFT is the plot's left edge when lane names sit in the gutter (wide layout). */
const LANE_LEFT = 150;

/** The ledger and registry step strips at viewBox width W, aligned to a plot starting at left. */
function Strips({ W, left, idp }: { W: number; left: number; idp: string }) {
  const range = data.range as [string, string];
  const ledger = lastPerDay(data.ledger, "rows");
  return (
    <>
      <StepChart
        W={W}
        H={120}
        left={left}
        range={range}
        yDomain={[200, 320]}
        yTicks={[200, 250, 300]}
        title="rows in the pointer-lifetime ledger"
        idp={`${idp}-led`}
        ariaLabel={`Pointer-lifetime ledger rows: ${ledger[0].rows} at creation on ${md(ledger[0].d)}, ${ledger[ledger.length - 1].rows} on ${md(ledger[ledger.length - 1].d)}.`}
        series={[
          {
            label: "ledger rows",
            colour: PALETTE.gold,
            points: ledger.map((p) => ({ d: p.d, v: p.rows })),
          },
        ]}
        hots={ledger.map((p, i) => ({
          d: p.d,
          label: `Ledger on ${md(p.d)}`,
          title: `${md(p.d)}: ${p.rows} ledger rows`,
          rows: [
            i === 0
              ? "Created retroactively for the code already ported."
              : `+${p.rows - ledger[i - 1].rows} rows`,
            <>
              {p.count} commit{p.count > 1 ? "s" : ""} changed it that day; last{" "}
              <CommitLink hash={p.commit} />
            </>,
          ],
        }))}
      />
      <StepChart
        W={W}
        H={120}
        left={left}
        range={range}
        yDomain={[0, 110]}
        yTicks={[0, 50, 100]}
        title="entries in the check registry"
        idp={`${idp}-reg`}
        ariaLabel={`Check registry entries: ${data.registry[0].n} on day one, ${data.registry[data.registry.length - 1].n} now.`}
        series={[
          {
            label: "registry entries",
            colour: PALETTE.rust,
            points: data.registry.map((p) => ({ d: p.d, v: p.n })),
          },
        ]}
        hots={data.registry
          .filter((p, i, all) => i === 0 || p.n !== all[i - 1].n)
          .map((p) => ({
            d: p.d,
            label: `Check registry on ${md(p.d)}`,
            title: `${md(p.d)}: ${p.n} registry entries`,
            rows: ["From a weekly snapshot of the check script."],
          }))}
      />
    </>
  );
}

/** The phone layout: the changes as a dated list grouped by era, with commit links. */
function PhoneList() {
  const laneIndex = new Map(data.lanes.map((l, i) => [l.key, i]));
  return (
    <ol className={s.eraList}>
      {data.eras.map((era) => (
        <li key={era.start}>
          <div className={s.eraHead}>
            {era.label} · {md(era.start)} to {md(era.end)} ·{" "}
            {era.perDay.toFixed(1)} commits a day
          </div>
          <ol className={s.evList}>
            {data.events
              .filter((e) => e.d >= era.start && e.d <= era.end)
              .map((e) => (
                <li key={`${e.d}-${e.label}`}>
                  <span className={s.evDate}>{md(e.d)}</span>
                  <LaneGlyph i={laneIndex.get(e.lane) ?? 0} />
                  <span>
                    <b>{e.label}.</b> {e.text}
                    {e.commit ? (
                      <>
                        {" "}
                        <CommitLink hash={e.commit} />
                      </>
                    ) : null}
                    {e.d === data.trigger.d && e.lane === "rules" ? (
                      <span className={s.trigger}> {data.trigger.text}</span>
                    ) : null}
                  </span>
                </li>
              ))}
          </ol>
        </li>
      ))}
    </ol>
  );
}

/**
 * How the method changed: dated method changes in five lanes over the three
 * eras, with the pointer-lifetime ledger and the check registry as step
 * strips on the same time axis. Phones get the changes as a list.
 */
export default function StrategyTimeline() {
  return (
    <div className={s.stack}>
      <div className={s.legendLine}>
        {data.lanes.map((l, i) => (
          <span key={l.key}>
            <LaneGlyph i={i} />
            {l.label}
          </span>
        ))}
      </div>
      <div className={s.wideOnly}>
        <Lanes W={960} left={LANE_LEFT} />
        <Strips W={960} left={LANE_LEFT} idp="stw" />
      </div>
      <div className={s.narrowOnly}>
        <Strips W={330} left={34} idp="stn" />
        <PhoneList />
      </div>
    </div>
  );
}
