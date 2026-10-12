import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import Tooltip from "../ui/Tooltip";
import s from "./EveryPrompt.module.css";
import { dayDate, type EpData, shortDate } from "./model";
import { AREA_COLOURS } from "./SessionTimeline";
import type { Session, SessionTileValues } from "./session-model";

/** Props of the session tiles. */
type SessionTilesProps = {
  data: EpData;
  values: SessionTileValues;
  /** Whether the filter keeps every session. */
  whole: boolean;
  /** Selects a session (shows only its prompts). */
  onSelect: (x: Session) => void;
  /** Narrows the day range to one day. */
  onDay: (day: number) => void;
};

/** WEEKDAYS are short English weekday names, Sunday first. */
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** Formats a median: whole numbers plain, halves with one decimal, "—" when undefined. */
function med(x: number | null): string {
  if (x === null) return "—";
  return Number.isInteger(x) ? fmtInt(x) : x.toFixed(1);
}

/**
 * The session tiles, above the prompt tiles: sessions kept by the filter,
 * sessions per area (a small bar list), the longest session by prompts, the
 * busiest parallel day and the median prompts per session.
 */
export default function SessionTiles({
  data,
  values: v,
  whole,
  onSelect,
  onDay,
}: SessionTilesProps) {
  const total = data.totals.sessions ?? 0;
  const max = Math.max(1, ...v.byArea.map((a) => a.n));
  const start = data.window[0];
  const busy = v.busiestDay;
  const busyDate = busy ? dayDate(start, busy.day) : null;
  const long = v.longestPrompts;
  return (
    <div className={s.tiles}>
      <div className={s.tile}>
        <b>{fmtInt(v.sessions)}</b>
        <span>
          {whole ? "sessions" : `of ${fmtInt(total)} sessions in the filter`}
          {v.sessions > 0
            ? `, ${fmtInt(v.commandOnly)} of them only commands or shell escapes${v.founding ? `, ${v.founding} founding` : ""}`
            : ""}
        </span>
      </div>
      <div className={`${s.tile} ${s.tileAreas}`}>
        <b className={s.tileSmallHead}>sessions per area</b>
        <ul className={s.areaBars}>
          {v.byArea.map((a) => (
            <li key={a.area}>
              <span className={s.areaName}>{a.area}</span>
              <span className={s.areaTrack}>
                <span
                  className={s.areaFill}
                  style={{
                    width: `${(a.n / max) * 100}%`,
                    background: AREA_COLOURS[a.area] ?? PALETTE.gray5,
                  }}
                />
              </span>
              <span className={s.areaN}>{a.n}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className={s.tile}>
        <b>{long ? fmtInt(long.prompts) : "—"}</b>
        <span>
          {long ? (
            <>
              prompts, the longest session: “{long.title}”
              {long.spannedDays > 1
                ? ` (spanned ${long.spannedDays} days)`
                : ""}{" "}
              <button
                type="button"
                className={s.linkBtn}
                onClick={() => onSelect(long)}
              >
                show its prompts
              </button>
            </>
          ) : (
            "no session matches"
          )}
        </span>
      </div>
      <div className={s.tile}>
        <b>{busy ? busy.open : "—"}</b>
        <span>
          {busy && busyDate ? (
            <>
              sessions with an instruction or decision on the busiest day
              {v.busiestTies.length ? "s (a tie)" : ""},{" "}
              {[busy.day, ...v.busiestTies].map((day, k, all) => {
                const d = dayDate(start, day);
                return (
                  <span key={day}>
                    {k > 0 ? (k === all.length - 1 ? " and " : ", ") : ""}
                    {WEEKDAYS[d.getUTCDay()]} {shortDate(d)}{" "}
                    <button
                      type="button"
                      className={s.linkBtn}
                      onClick={() => onDay(day)}
                    >
                      {all.length > 1 ? "show it" : "show that day"}
                    </button>
                  </span>
                );
              })}{" "}
              <Tooltip
                id="ep-sessions-rule"
                content={data.rules.sessions ?? ""}
              >
                <span className={s.ruleMark}>rule</span>
              </Tooltip>
            </>
          ) : (
            "no session at work on these days"
          )}
        </span>
      </div>
      <div className={s.tile}>
        <b>{med(v.medianPrompts)}</b>
        <span>median prompts per session</span>
      </div>
    </div>
  );
}
