import CommitLink from "../ui/CommitLink";
import CommitText from "../ui/CommitText";
import type { Target } from "./MainChart";
import {
  type AvhData,
  type DayTotals,
  dayIndex,
  dayLabel,
  fmtN,
  KIND_COLOURS,
  MODEL_NAMES,
  pad2,
  weekday,
} from "./model";
import s from "./AgentsVsHuman.module.css";

/** Props shared by the tooltip parts. */
type TipProps = {
  data: AvhData;
  totals: DayTotals;
  /** Whether the target day is inside the range and already revealed by the replay. */
  visible: boolean;
};

/**
 * The day summary: commits, merges, prompts and lines; model mix;
 * API-equivalent cost (or "not recorded" in the lost Mac transcript window);
 * agent threads and other-project prompts; active engineering eras; events
 * with their quotes; and the quiet-stretch reason.
 */
function DaySummary({ data, totals: t, day: i }: TipProps & { day: number }) {
  const dy = data.daily;
  const events = data.events.filter(([d]) => dayIndex(data, d) === i);
  const quiet = data.quiet.find(
    ([a, b]) => i >= dayIndex(data, a) && i <= dayIndex(data, b),
  );
  const eras = data.eras
    .filter(([, a, b]) => i >= dayIndex(data, a) && i <= dayIndex(data, b))
    .map((e) => e[3]);
  const mix = dy.mix[i]
    .map((v, j) => (v ? `${MODEL_NAMES[j]} ${v}` : null))
    .filter(Boolean)
    .join(" · ");
  const cost = dy.costMacClaude[i] + dy.costVmClaude[i] + dy.costCodex[i];
  let costText: string;
  if (dy.lost[i] && cost === 0) {
    costText = "API-equivalent $: not recorded (Mac Claude transcripts lost)";
  } else if (dy.lost[i]) {
    costText = `API-equivalent $${fmtN(cost)} recovered (Mac Claude transcripts lost: lower bound)`;
  } else {
    costText = `API-equivalent $${fmtN(cost)}`;
  }
  return (
    <>
      <div className={s.tm}>
        {t.commits[i]} commits
        {t.merges[i] ? ` (+${t.merges[i]} merges)` : ""} · {t.prompts[i]}{" "}
        prompts · +{fmtN(t.added[i])} / −{fmtN(t.deleted[i])} lines
      </div>
      {mix ? <div className={s.tm}>model: {mix}</div> : null}
      <div className={s.tm}>
        {costText} · {dy.threads[i]} agent threads · {dy.otherPrompts[i]}{" "}
        prompts in other projects
      </div>
      {eras.length ? (
        <div className={`${s.te} ${s.teEras}`}>
          {eras.map((e) => (
            <div key={e}>
              <CommitText text={e} />
            </div>
          ))}
        </div>
      ) : null}
      {events.map(([, time, kind, label, text]) => (
        <div key={`${time}${label}`} className={s.te}>
          <span className={s.teKind} style={{ color: KIND_COLOURS[kind] }}>
            {kind === "pin" ? "" : `${time} `}
            {label}
          </span>{" "}
          — <CommitText text={text} />
        </div>
      ))}
      {quiet ? (
        <div className={s.tq}>
          Quiet {dayLabel(data, dayIndex(data, quiet[0]))}
          {quiet[1] > quiet[0]
            ? `–${dayLabel(data, dayIndex(data, quiet[1]))}`
            : ""}
          : {quiet[2]}
        </div>
      ) : null}
    </>
  );
}

/**
 * Tooltip body for a hovered, tapped or focused target: the hour cell (counts,
 * lines, prompts and the largest commit subject of the hour) followed by the
 * day summary, or the day summary alone outside the grid.
 */
export default function DayTooltip({
  data,
  totals,
  visible,
  target,
}: TipProps & { target: Target }) {
  const i = target.day;
  const head = `${weekday(data, i)} ${dayLabel(data, i)}`;
  if (!visible) {
    return (
      <>
        <div className={s.th}>{head}, 2026</div>
        <div className={s.tm}>not yet replayed</div>
      </>
    );
  }
  if (target.hour === null) {
    return (
      <>
        <div className={s.th}>{head}, 2026</div>
        <DaySummary data={data} totals={totals} visible day={i} />
      </>
    );
  }
  const hh = target.hour;
  const k = i * 24 + hh;
  const g = data.grid;
  const subject = data.subjects[String(k)];
  return (
    <>
      <div className={s.th}>
        {head} · {pad2(hh)}:00–{pad2((hh + 1) % 24)}:00 PT
      </div>
      <div className={`${s.tm} ${s.tmBright}`}>
        {g.commits[k]} commits{g.merges[k] ? ` (+${g.merges[k]} merges)` : ""} ·
        +{fmtN(g.added[k])} / −{fmtN(g.deleted[k])} lines ·{" "}
        <span className={s.teal}>{g.prompts[k]} prompts</span>
      </div>
      {subject ? (
        <div className={s.ts}>
          <CommitText text={subject[1]} />{" "}
          <span className={s.hash}>
            <CommitLink hash={subject[0]} />
          </span>
          {g.commits[k] > 1 ? (
            <span className={s.largest}>
              largest of {g.commits[k]} commits this hour
            </span>
          ) : null}
        </div>
      ) : null}
      <div className={`${s.tm} ${s.dayHead}`}>Day:</div>
      <DaySummary data={data} totals={totals} visible day={i} />
    </>
  );
}
