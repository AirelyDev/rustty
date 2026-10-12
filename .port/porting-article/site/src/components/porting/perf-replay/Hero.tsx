import { fmtPct } from "../lib/format";
import { hero } from "./model";
import s from "./PerfReplay.module.css";
import type { PrData } from "./types";

/** The hero row: 0/14 → 14/14, the window, suite and geomean, perf commits and the two headline speed-ups. */
export default function Hero({ data }: { data: PrData }) {
  const h = hero(data);
  return (
    <div className={s.hero}>
      <div>
        <div className={s.big}>
          <span className={s.red}>
            {h.startW}/{h.n}
          </span>{" "}
          <span className={s.arrow}>→</span>{" "}
          <span className={s.cu}>
            {h.endW}/{h.n}
          </span>
        </div>
        <div className={s.sub}>
          benchmarks where Rustty is faster · {h.from} → {h.to} · suite{" "}
          {fmtPct(h.suiteStart)} → {fmtPct(h.suiteEnd)} · geomean{" "}
          {h.geoStart.toFixed(3)} → {h.geoEnd.toFixed(3)}
        </div>
      </div>
      <div className={s.minis}>
        <div className={s.mini}>
          <b>{h.perfCommits}</b> perf commits on 07-16
        </div>
        <div className={s.mini}>
          <b>{h.tParser.toFixed(2)}×</b> terminal-parser
        </div>
        <div className={s.mini}>
          <b>{h.clone.toFixed(2)}×</b> screen-clone
        </div>
      </div>
    </div>
  );
}
