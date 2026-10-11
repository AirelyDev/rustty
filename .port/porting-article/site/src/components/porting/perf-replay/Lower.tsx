"use client";

import SourceLink from "../ui/SourceLink";
import CommitLink from "../ui/CommitLink";
import { fmtInt, fmtPct } from "../lib/format";
import type { ChartTip } from "../ui/useChartTip";
import { full33Summary, insightNumbers } from "./model";
import s from "./PerfReplay.module.css";
import RatioChart from "./RatioChart";
import { memoryRows } from "./rows";
import type { PrData } from "./types";

/** HARDWARE is the measurement machine, in the article's fixed wording. */
const HARDWARE = "MacBook Pro, Apple M4 Pro, 14 cores, 48 GB";

/** Returns the insertions of a commit's diffstat ("1 file changed, 24 insertions(+)" → 24). */
function insertions(data: PrData, hash: string): number {
  const c = data.commits.find((x) => x.h === hash);
  return Number(c?.stat.match(/(\d+) insertion/)?.[1] ?? 0);
}

/** Memory and binary size on the same log-ratio encoding (Rustty ÷ Ghostty). */
export function MemoryPanel({
  data,
  narrow,
  tip,
}: {
  data: PrData;
  narrow: boolean;
  tip: ChartTip;
}) {
  return (
    <div className={s.panel}>
      <div className={s.sec}>Memory and binary size · Rustty ÷ Ghostty</div>
      <RatioChart
        chart="mem"
        rows={memoryRows(data)}
        d0={0.5}
        d1={2}
        ticks={[0.5, 0.667, 1, 1.5, 2]}
        w={460}
        lw={170}
        rw={112}
        rh={22}
        narrow={narrow}
        tip={tip}
        label="Memory and size ratios, Rustty over Ghostty."
      />
      <p className={s.pnote}>
        * Peak page bytes compare against the old reference commit's page
        geometry only; the alloc-bench RSS is asymmetric (the Rust binary
        carries an allocation-tracking seam).
      </p>
    </div>
  );
}

/** The computed insights, with figures taken from the data. */
export function Insights({ data }: { data: PrData }) {
  const n = insightNumbers(data);
  const f = full33Summary(data);
  const enc = data.comptime[0].steps;
  const churn = data.comptime[1].steps;
  const pct = (x: number) => Math.round(x * 100);
  const neg = (x: number, d = 0) => `−${Math.abs(x).toFixed(d)}`;
  return (
    <div className={s.panel}>
      <div className={s.sec}>Computed insights</div>
      <ol className={s.ins}>
        <li>
          <b>
            <CommitLink hash="be02801" />
          </b>{" "}
          (+{insertions(data, "be02801")} lines in{" "}
          <SourceLink
            at="be02801"
            path="crates/rustty-vt/src/parser.rs"
            text="parser.rs"
          />
          ) is the largest single-commit move: terminal-parser{" "}
          {n.parserFrom.toFixed(1)} → {n.parserTo.toFixed(1)} ms,{" "}
          <b>{neg(n.parser, 1)} ms</b>, {n.parserShareOfZig.toFixed(1)}% of the
          whole Zig suite.
          <small>
            Rounds 4+5 against a same-session control of the round-3 binary:
            suite {neg(n.bundle)} ms, of which terminal-parser{" "}
            {pct(n.shares.parser)}%, terminal-stream {pct(n.shares.stream)}% (
            <CommitLink hash="6ca1015" /> + <CommitLink hash="52116f3" />
            ), screen-clone.clone {pct(n.shares.clone)}% (
            <CommitLink hash="745b3c9" />
            ). Only <CommitLink hash="be02801" /> touched the parser in that
            window.
          </small>
        </li>
        <li>
          The best isolated full-sweep step is{" "}
          <b>
            <CommitLink hash={n.best.commit} />
          </b>{" "}
          (boxed dispatch payloads): suite <b>{neg(n.best.delta)} ms</b>.
          <small>
            Consecutive cumulative stage binaries in one session; next best{" "}
            <CommitLink hash={n.next.commit} /> {neg(n.next.delta)} ms.
          </small>
        </li>
        <li>
          Reverting LTO (
          <b>
            <CommitLink hash="9e1837b" />
          </b>
          , 6 lines of{" "}
          <SourceLink at="58aa62a81" path="Cargo.toml" lines="502-517" />) took
          terminal-parser from{" "}
          <b>
            {Math.round(n.ltoChainEnd)} → {Math.round(n.ltoShipped)} ms
          </b>{" "}
          on identical code; with fat LTO the 14-benchmark suite took{" "}
          {fmtInt(n.ltoChainSuite)} ms.
          <small>
            Same code <CommitLink hash="d211a84" />, fat-LTO profile against
            overflow-checks-only. The profile comparison covers four benchmarks,
            so no whole-suite saving is given.
          </small>
        </li>
        <li>
          Turning overflow checks off was worth only{" "}
          <b>{fmtPct(n.profileOnly)}</b> of suite. Re-measuring the unchanged
          pre-fix code in the next session moved the suite ratio from{" "}
          <b>
            {fmtPct(n.sessionFrom)} to {fmtPct(n.sessionTo)}
          </b>
          .
          <small>
            Session effects dwarfed the build profile; hence same-session Zig in
            every comparison and, later, a check against a stored Rustty binary.
          </small>
        </li>
        <li>
          Compile-time-flattening fixes cut Rustty's 33-config suite by{" "}
          <b>
            {fmtInt(f.rustBefore - f.rustAfter)} ms (
            {fmtPct((f.rustAfter / f.rustBefore - 1) * 100)})
          </b>{" "}
          and the worst gaps from {fmtPct(enc[0].value, 0)} to{" "}
          {fmtPct(enc[enc.length - 1].value, 0)} (snapshot encode) and{" "}
          {churn[0].value}× to {churn[churn.length - 1].value}× (hyperlink
          churn), yet the geomean ratio went {f.geoBefore.toFixed(3)} →{" "}
          {f.geoAfter.toFixed(3)} because Zig's same-session numbers moved too.
          <small>
            Wins {f.winsBefore} → {f.winsAfter} of {f.n}.
          </small>
        </li>
        <li>
          Hazard classes ranked by worst recorded gap:{" "}
          <b>missing cross-crate #[inline]</b> (6.1× per op), scalar early-exit
          scans (5.3× on a micro), comptime param → runtime enum (3.9×),
          comptime const recomputed (+34%), #[inline] hint lost (+27%), bench
          config mismatch (~24% at round 5, 8.2% re-measured later).
          <small>Full ranking in the compile-time-flattening chapter.</small>
        </li>
        <li>
          Noise floor: on the final July sweep the widest max/min spread on a
          large config is <b>{n.noise.spread.toFixed(3)}</b>; the same unchanged
          Zig binary measured terminal-parser at 256.2–286.4 ms across four
          sessions (<b>11.8%</b>).
          <small>
            So a stored number is a cross-session comparison; every ratio here
            is same-session.
          </small>
        </li>
      </ol>
    </div>
  );
}

/** The methodology footnote: hardware, builds, same-session Zig, and wall clock vs ns/op slopes. */
export function Method({ data }: { data: PrData }) {
  return (
    <p className={s.method}>
      <b>Method:</b> one {HARDWARE}; runs strictly sequential, byte-identical
      corpora for both binaries. Rust built with{" "}
      <code>cargo build -p rustty-bench --profile release-fast</code> (overflow
      checks off, no LTO, nothing else), Zig with{" "}
      <code>-Doptimize=ReleaseFast</code>, and every ratio compares runs from
      the same session. Wall clock includes process spawn: Rustty's process
      floor is about 3 ms lower than Ghostty's, so on benchmarks under ~15 ms a
      wall-clock "win" can hide a per-operation loss (hyperlink-map lookup: −15%
      wall clock, 7.5× slower per operation). For short configs the project
      compares the slope of a loops sweep, <code>(t(N) − t(1)) / (N − 1)</code>,
      in ns per op. The {data.floor.length} spawn-floor configs (marked floor)
      are shown but their percentages mean little. Step times are commit times
      of the code measured, not sweep times. The terminal title shows the
      recorded command of the hovered benchmark and the results file of the step
      (the corpus path prefix is shortened to …/corpora).
    </p>
  );
}
