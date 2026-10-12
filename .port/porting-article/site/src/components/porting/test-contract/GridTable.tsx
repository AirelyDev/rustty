import SourceLink from "../ui/SourceLink";
import CommitLink from "../ui/CommitLink";
import { PALETTE } from "../lib/palette";
import {
  type DayStats,
  fmt,
  type GridMod,
  type GridOrder,
  md,
  pinLabel,
  sparkPath,
  type TcData,
} from "./model";
import s from "./TestContract.module.css";

/** SPARK_W is the sparkline's viewBox width (stretched to the scrubber width). */
const SPARK_W = 600;
/** SPARK_H is the sparkline's viewBox height. */
const SPARK_H = 34;

/** Tooltip body for one grid module on day `day` under pin k. */
export function ModuleTip({
  o,
  k,
  day,
  dates,
}: {
  o: GridMod;
  k: number;
  day: number;
  dates: string[];
}) {
  return (
    <>
      <b>{o.m}</b> <span className={s.k}>{o.c}</span>
      <br />
      Zig tests now: <b>{o.z[k]}</b>{" "}
      <span className={s.k}>
        ({o.z[0]} → {o.z[o.z.length - 1]} across syncs)
      </span>
      <br />
      Rust tests on {md(dates[day])}: <b>{o.R[day]}</b>
      <br />
      <span className={s.k}>first at/above Zig:</span>{" "}
      {o.first < dates.length ? md(dates[o.first]) : "—"}
      {o.R[day] < o.z[k] ? (
        <>
          <br />
          <span style={{ color: PALETTE.upstream }}>
            short by {o.z[k] - o.R[day]}
          </span>
        </>
      ) : null}
    </>
  );
}

/** Covered-share sparkline over the scrubber, with sync ticks and the current-day cursor. */
export function Sparkline({
  data,
  stats,
  day,
}: {
  data: TcData;
  stats: DayStats[];
  day: number;
}) {
  const last = data.dates.length - 1;
  const spark = sparkPath(stats, SPARK_W, SPARK_H);
  return (
    <div className={s.spark} aria-hidden="true">
      <svg
        viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
        preserveAspectRatio="none"
        className={s.sparkSvg}
        aria-hidden="true"
      >
        <path
          d={`${spark} L${SPARK_W},${SPARK_H} L0,${SPARK_H}Z`}
          fill={PALETTE.rust}
          opacity={0.18}
        />
        <path
          d={spark}
          fill="none"
          stroke={PALETTE.rust}
          strokeWidth={1.5}
          vectorEffect="non-scaling-stroke"
        />
        {data.pins.slice(1).map((p) => {
          const x = (data.dates.indexOf(p.date) / last) * SPARK_W;
          return (
            <line
              key={p.sha}
              x1={x}
              x2={x}
              y1={0}
              y2={SPARK_H}
              stroke={PALETTE.upstream}
              strokeWidth={1}
              vectorEffect="non-scaling-stroke"
              opacity={0.8}
            />
          );
        })}
        <line
          x1={(day / last) * SPARK_W}
          x2={(day / last) * SPARK_W}
          y1={0}
          y2={SPARK_H}
          stroke={PALETTE.gray8}
          strokeWidth={1}
          vectorEffect="non-scaling-stroke"
          opacity={0.6}
        />
      </svg>
    </div>
  );
}

/** The grid's colour legend and interaction hint. */
export function GridLegend() {
  return (
    <>
      <div className={s.legend}>
        <span>
          <i style={{ background: PALETTE.upstream }} />
          Zig test, no Rust counterpart counted yet
        </span>
        <span>
          <i style={{ background: PALETTE.rust }} />
          covered (module's Rust count reaches this square)
        </span>
        <span>
          <i style={{ background: PALETTE.gold }} />
          arrived with an upstream sync in the last 3 days
        </span>
        <span>
          <i className={s.legendFuture} />
          arrives with a later upstream sync
        </span>
      </div>
      <p className={s.hint}>
        Hover or tap a module for its counts. With the keyboard, focus the grid
        and use the arrow keys, Enter to pin and Escape to close.
      </p>
    </>
  );
}

/** Visually hidden table of every grid module and the day it first met its Zig count (screen readers). */
export function ModuleTable({
  order,
  dates,
}: {
  order: GridOrder;
  dates: string[];
}) {
  const last = dates.length - 1;
  return (
    <table className={s.srOnly}>
      <caption>
        First day each module reached its Zig test count (grid order)
      </caption>
      <thead>
        <tr>
          <th scope="col">module</th>
          <th scope="col">crate</th>
          <th scope="col">Zig tests now</th>
          <th scope="col">Rust tests</th>
          <th scope="col">first at or above</th>
        </tr>
      </thead>
      <tbody>
        {order.mods.map((m) => (
          <tr key={m.m}>
            <td>{m.m}</td>
            <td>{m.c}</td>
            <td>{m.z[m.z.length - 1]}</td>
            <td>{m.R[last]}</td>
            <td>{m.first < dates.length ? dates[m.first] : "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Method note under the grid. */
export function GridNote({ total }: { total: number }) {
  return (
    <p className={s.note}>
      Each square is one Zig test block counted by the repo's per-module
      test-count check (
      <SourceLink
        at="58aa62a81"
        path=".port/tools/gates/check_test_counts.py"
      />
      , 211 modules, {fmt(total)} tests at the current reference commit). For
      each day 06-21 → 10-10, the last first-parent commit before midnight
      Pacific was sampled and <code>#[test]</code> lines were counted in each
      module's Rust files. Square <i>i</i> of a module is copper when that
      module's Rust count is greater than <i>i</i>. This is a count match, not a
      per-name match. The name-level check (4,287 of 4,287 live reference tests
      matched) came later. The Zig side is recounted at each reference commit in
      the Ghostty checkout. The two Zig dependency packages were taken at the
      versions each reference commit pins. Squares are ordered by the day their
      module first reached its Zig count.
    </p>
  );
}

/** Replay headline (covered / contract) and the five live counters for day `day`. */
export function GridStats({
  data,
  st,
  day,
}: {
  data: TcData;
  st: DayStats;
  day: number;
}) {
  const k = st.pin;
  return (
    <>
      <div className={s.big}>
        {fmt(st.met)} / {fmt(st.need)}
      </div>
      <div className={s.sub}>
        Zig tests with a counted Rust counterpart ·{" "}
        {((100 * st.met) / st.need).toFixed(1)}% · contract of the {pinLabel(k)}
      </div>
      <div className={s.stats} aria-live="off">
        <div className={s.stat}>
          <b>{md(data.dates[day])}</b>
          <span>
            day {day + 1} · <CommitLink hash={data.revs[day]} />
          </span>
        </div>
        <div className={`${s.stat} ${s.statCu}`}>
          <b>{fmt(data.rust.all[day])}</b>
          <span>Rust #[test], all crates</span>
        </div>
        <div className={`${s.stat} ${s.statCu}`}>
          <b>{fmt(data.rust.vt[day])}</b>
          <span>terminal engine (rustty-vt)</span>
        </div>
        <div className={`${s.stat} ${s.statTeal}`}>
          <b>{fmt(st.need)}</b>
          <span>Zig tests in the contract</span>
        </div>
        <div className={`${s.stat} ${s.statGold}`}>
          <b>
            {st.below}
            {st.below ? ` (−${fmt(st.deficit)})` : ""}
          </b>
          <span>modules below their Zig count</span>
        </div>
      </div>
    </>
  );
}
