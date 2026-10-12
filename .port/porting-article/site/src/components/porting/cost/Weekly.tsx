import type { CSSProperties } from "react";
import type { ChartTip } from "../ui/useChartTip";
import s from "./Cost.module.css";
import Hatch from "./Hatch";
import {
  CAT_COLOR,
  CAT_NAME,
  CATS,
  type CostData,
  fmtMoney,
  fmtVal,
  type Scope,
  TRAILER_COLOR,
  type Unit,
  weekTotals,
} from "./model";
import TipSvg from "./TipSvg";
import WeeklyBars from "./WeeklyBars";
import WeeklyLane, { EVENT_STYLE } from "./WeeklyLane";
import WeeklyStrips, { COV_COLOR } from "./WeeklyStrips";
import { WK } from "./weekly-geo";

/** Props for the weekly panel. */
type Props = {
  d: CostData;
  sc: Scope;
  u: Unit;
  tip: ChartTip;
  tipId: string;
  /** Unique id prefix for SVG patterns. */
  uid: string;
};

/** TRAILER_KEYS are the trailer-strip legend entries. */
const TRAILER_KEYS = [
  "Opus 4.8",
  "Opus 5",
  "Opus 5.5",
  "Fable 5",
  "Sonnet 5.5",
  "none",
];

/** A legend swatch: a small coloured square, optionally styled. */
function Sw({ bg, style }: { bg: string; style?: CSSProperties }) {
  return <i className={s.sw} style={{ background: bg, ...style }} />;
}

/**
 * The full-width weekly instrument: its title (unit-dependent), the
 * recovered + modeled totals, the SVG (bars, band, limit lane, trailer and
 * coverage strips) in a sideways-scrolling container on phones, the legend
 * and the reading note.
 */
export default function Weekly({ d, sc, u, tip, tipId, uid }: Props) {
  const tot = weekTotals(d, sc, u);
  const hatch = `${uid}-hatch`;
  return (
    <div className={s.panel}>
      <div className={s.pt}>
        <h4>
          {u === "usd"
            ? "Weekly API-equivalent cost by quota week"
            : "Weekly billable tokens by quota week"}
        </h4>
        <span>
          {`${sc === "rustty" ? "Rustty" : "all projects"} · recovered ${fmtVal(tot.rec, u)} + modeled ${fmtVal(tot.ext[1], u)} (${fmtVal(tot.ext[0], u)}–${fmtVal(tot.ext[2], u)})${sc === "all" ? " · all-projects model assumes every lost week maxed" : ""}`}
        </span>
      </div>
      <p className={s.scrollHint} aria-hidden="true">
        ← scroll sideways for all 17 weeks →
      </p>
      <div className={s.wkWrap}>
        <TipSvg
          tip={tip}
          tipId={tipId}
          className={s.wk}
          viewBox={`0 0 ${WK.W} ${WK.H}`}
          label="Weekly usage by quota week, with modeled lost-window weeks, the measured ceiling band, limit signals, commit trailer models and data coverage per day"
        >
          <Hatch id={hatch} />
          <WeeklyBars d={d} sc={sc} u={u} hatch={hatch} />
          <WeeklyStrips d={d} />
          <WeeklyLane d={d} sc={sc} />
        </TipSvg>
      </div>
      <div className={s.leg}>
        {CATS.map((c) => (
          <span key={c}>
            <Sw bg={CAT_COLOR[c]} />
            {CAT_NAME[c] ?? c}
          </span>
        ))}
        <span>
          <Sw bg="repeating-linear-gradient(45deg,#d97934 0 2px,#d9793422 2px 5px)" />
          Mac Claude, modeled
        </span>
        <span>
          <Sw bg="#ffbc7a33" style={{ border: "1px dashed #ffbc7a" }} />
          maxed-week band
        </span>
        <span className={s.legHead}>markers:</span>
        {(
          [
            "weekly",
            "session",
            "usage_credits_exhausted",
            "prompt",
            "slash",
          ] as const
        ).map((k) => (
          <span key={k}>
            <i
              className={`${s.mk} ${s[`mk_${EVENT_STYLE[k].shape}`]}`}
              style={{ color: EVENT_STYLE[k].color }}
            />
            {EVENT_STYLE[k].name}
          </span>
        ))}
        <span className={s.legHead}>trailer strip:</span>
        {TRAILER_KEYS.map((k) => (
          <span key={k}>
            <Sw bg={TRAILER_COLOR[k]} />
            {k === "none" ? "no trailer" : k}
          </span>
        ))}
        <span className={s.legHead}>data strip:</span>
        <span>
          <Sw bg={COV_COLOR.rec} />
          transcripts recovered
        </span>
        <span>
          <Sw bg={COV_COLOR.vmcodex} />
          VM / Codex only
        </span>
        <span>
          <Sw bg={COV_COLOR.prompt} />
          prompts + commits only
        </span>
      </div>
      <p className={s.note}>
        Bars are <b>quota weeks</b>, the subscription's own 7-day window (resets
        Tuesday 3 pm Pacific, 22:00 UTC). Hatched = modeled Mac Claude in the
        lost window, spread by day from the Monday-week model (its totals are
        unchanged). The whisker shows the low and high ceilings. The <b>band</b>{" "}
        is the measured size of a maxed week: weeks 09-29, 09-08 and 09-22 (
        {d.ceil.usd.map(fmtMoney).join(" / ")}, all projects), halved in
        Rustty-only view. In token view the hatched bars use those weeks' token
        totals. Markers above the bars are limit signals: the author's own
        words, Claude Code's limit messages and slash commands. In Rustty-only
        view, other-project signals are dimmed; they count because the quota is
        account-wide.
      </p>
    </div>
  );
}
