"use client";

import type { ReactNode } from "react";
import { fmtInt, fmtPct } from "../lib/format";
import CommitLink from "../ui/CommitLink";
import CommitText from "../ui/CommitText";
import type { ChartTip } from "../ui/useChartTip";
import Comptime from "./Comptime";
import {
  advPinSummary,
  full33Summary,
  insightNumbers,
  scoreboard,
  suite,
} from "./model";
import s from "./PerfReplay.module.css";
import RatioChart from "./RatioChart";
import {
  advPinRows,
  apcRows,
  full33Rows,
  julyRows,
  pinHasRanges,
  pinRows,
} from "./rows";
import type { PrData } from "./types";

/** JULY_PIN is Ghostty's original reference commit (the July comparisons). */
const JULY_PIN = "4749c4e93";

/** MAIN_REV is the Ghostty main commit of the 33-config matrix. */
const MAIN_REV = "4540d499a";

/** APC_FIX is the commit that spelled the APC group test as the reference's NEON loop. */
const APC_FIX = "5a7f2d1c8";

/** Chapter heading: a big mono figure and a paragraph. */
function Head({ big, children }: { big: string; children: ReactNode }) {
  return (
    <div className={s.chhead}>
      <span className={s.big}>{big}</span>
      <p>{children}</p>
    </div>
  );
}

/** Props shared by the chapter views. */
type ViewProps = { data: PrData; i: number; narrow: boolean; tip: ChartTip };

/** Renders the selected chapter of the second track. */
export default function ChapterView({ data, i, narrow, tip }: ViewProps) {
  const chart = { narrow, tip };
  if (i === 0) {
    const last = data.stages.length - 1;
    const ins = insightNumbers(data);
    return (
      <>
        <Head
          big={`${scoreboard(data, last).W}/${data.bench.length} · ${fmtPct(suite(data, last).deltaPct)}`}
        >
          Final July sweep (code at{" "}
          <CommitLink hash={data.stages[last].commit} />) against Ghostty at the
          original reference commit <CommitLink hash={JULY_PIN} />. Error bars
          are the ratio range from each binary's max/min of 7 runs: the widest
          spread on a large config is {ins.noise.spread.toFixed(3)} (
          {ins.noise.bench}), so the thinnest large-config margin (
          {ins.thinnest.bench}, {fmtPct(ins.thinnest.pct)}) is outside the
          noise. Hollow = 07-15 start.
        </Head>
        <RatioChart
          chart="july"
          rows={julyRows(data)}
          d0={0.25}
          d1={4}
          ticks={[0.25, 0.333, 0.5, 0.667, 1, 1.5, 2, 3, 4]}
          label="Final July ratios against the 07-15 start."
          {...chart}
        />
      </>
    );
  }
  if (i === 1) {
    return (
      <>
        <Head big={data.provisional.wins}>
          {data.provisional.date}: a paired re-measure of the comparable
          configurations was Rustty-faster on all{" "}
          {data.provisional.wins.split("/")[1]}, but the host load was far above
          the 2.0 idle gate, so the harness stamped the run provisional and
          exited with "unusable measurement". No values are plotted. The same
          work found that checks relative to Ghostty cannot catch a Rustty
          self-regression: screen-clone.clone could get 110.6% slower before the
          check fired, which is why a second check compares against a stored
          Rustty binary.
        </Head>
        <div className={`${s.pend} ${s.pendMuted}`}>
          Provisional run: not plotted on purpose.
        </div>
      </>
    );
  }
  if (i === 2) {
    const f = full33Summary(data);
    const tp = data.full33.find((r) => r.id.startsWith("terminal-parser"));
    const nr = data.full33.find(
      (r) => r.id === "terminal-resize.cols-no-reflow",
    );
    return (
      <>
        <Head big={`${f.winsBefore} → ${f.winsAfter} of ${f.n}`}>
          Full matrix against Ghostty main <CommitLink hash={MAIN_REV} />{" "}
          (2026-08-28), before and after the compile-time-flattening fixes.
          Rustty's own suite fell {fmtInt(f.rustBefore)} → {fmtInt(f.rustAfter)}{" "}
          ms ({fmtPct((f.rustAfter / f.rustBefore - 1) * 100)}) on identical
          corpora, yet the geomean ratio moved {f.geoBefore.toFixed(3)} →{" "}
          {f.geoAfter.toFixed(3)} because the same-session Zig suite also fell{" "}
          {fmtInt(f.zigBefore)} → {fmtInt(f.zigAfter)} ms with no Zig code
          change (terminal-resize.cols-no-reflow alone:{" "}
          {nr ? `${fmtInt(nr.bZ[0])} → ${fmtInt(nr.fZ[0])}` : ""} ms) and Zig's
          short configs got faster. Caveat: Ghostty main was then much slower
          than its pinned self on terminal-parser (
          {tp ? Math.round(tp.fZ[0]) : ""} vs ~286 ms) and wcwidth. Thin grey
          bar = before-fix range; thick = after-fix range. Configs under 20 ms
          are marked floor.
        </Head>
        <RatioChart
          chart="full33"
          rows={full33Rows(data)}
          d0={0.1}
          d1={1.6}
          ticks={[0.125, 0.25, 0.5, 1, 1.5]}
          rh={15}
          label="33 configurations against Ghostty main, before and after the fixes."
          {...chart}
        />
      </>
    );
  }
  if (i === 3) return <Comptime data={data} tip={tip} />;
  if (i === 4) {
    const a = advPinSummary(data);
    return (
      <>
        <Head big={`${a.wins}/${a.n} · ${fmtPct(a.deltaPct)}`}>
          {data.advPin.date}, Ghostty's benchmark tool rebuilt at reference
          commit <CommitLink hash={data.advPin.rev} /> (ReleaseFast), idle host.
          Sum of medians {fmtInt(a.ghostty)} → {fmtInt(a.rustty)} ms (second run
          −27.6%). Dot = first run, bar = range across the two independent runs
          (per-run raw repetitions were not kept, so the two runs stand in for
          the spread). Thinnest margin {a.thinnest} {a.thinRuns[0].toFixed(3)} /{" "}
          {a.thinRuns[1].toFixed(3)}: upstream hand-vectorised memset in this
          window (about 2.8× on this config) and Rustty deliberately does not
          port that, which is the likely explanation, not a measured one.
          Workloads changed with the reference commit, so do not compare these
          ratios with July's.
        </Head>
        <RatioChart
          chart="adv"
          rows={advPinRows(data)}
          d0={0.2}
          d1={1.25}
          ticks={[0.25, 0.333, 0.5, 0.667, 1]}
          label="Ratios at the advanced reference commit, two runs."
          {...chart}
        />
      </>
    );
  }
  if (i === 5) {
    const apc = data.apc.filter((r) => r.config === "apc-parser");
    return (
      <>
        <Head
          big={`${apc[0].ratio.toFixed(2)}× → ${apc[apc.length - 1].ratio.toFixed(2)}×`}
        >
          The first paired numbers at the advanced reference commit showed three
          short configs (outside the 14-config suite) where Rustty was slower.
          Root cause: the APC scan was a scalar port of a vector loop. Commit{" "}
          <CommitLink hash={APC_FIX} /> spells the group test as the reference's
          NEON loop and adds the missing hyperlink inlining. Child CPU time or
          per-loop slope, not wall clock. Two remain Rustty-slower.
        </Head>
        <RatioChart
          chart="apc"
          rows={apcRows(data)}
          d0={0.5}
          d1={2.2}
          ticks={[0.5, 0.667, 1, 1.5, 2]}
          lw={280}
          rh={18}
          label="APC scan and hyperlink configs, three stages each."
          {...chart}
        />
      </>
    );
  }
  const pin = data.pin;
  const measured = pin.status === "measured";
  const ranged = measured && pinHasRanges(data);
  const won = pin.rows.filter((r) => r.rustty[0] < r.ghostty[0]).length;
  const sumG = pin.rows.reduce((a, r) => a + r.ghostty[0], 0);
  const sumR = pin.rows.reduce((a, r) => a + r.rustty[0], 0);
  return (
    <>
      <Head
        big={
          measured
            ? `${won}/${pin.rows.length} · ${fmtPct((sumR / sumG - 1) * 100)}`
            : "measurement pending"
        }
      >
        <CommitText
          text={
            measured
              ? `Re-run of the benchmark suite at the current reference commit ${pin.rev} (Rustty ${pin.head}, ${pin.date ?? ""}), Ghostty's benchmark tool rebuilt with ReleaseFast in the same session, idle host. Sum of medians ${fmtInt(sumG)} → ${fmtInt(sumR)} ms. ${ranged ? "Median ratio per config with the max/min range." : "The run kept medians only, so each config shows its median ratio without a range."}${pin.note ? ` ${pin.note}` : ""}`
              : `Slot for the re-run of all ${data.bench.length} configurations at the current reference commit ${pin.rev} (Rustty head ${pin.head}). No perf run exists at this reference commit yet; nothing below is a value.`
          }
        />
      </Head>
      {measured ? null : (
        <div className={s.pend}>
          Measurement pending · same harness: 9 paired runs + warm-up, idle gate
          2.0, ratio and max/min spread per config, sum of medians, geomean.
          Will plot here when recorded.
        </div>
      )}
      <RatioChart
        chart="pin"
        rows={pinRows(data)}
        d0={0.2}
        d1={1.5}
        ticks={[0.25, 0.5, 1]}
        label={
          measured
            ? "Ratios at the current reference commit."
            : "Current reference commit: pending rows, no values."
        }
        {...chart}
      />
    </>
  );
}
