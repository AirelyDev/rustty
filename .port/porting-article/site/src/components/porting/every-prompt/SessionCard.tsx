import { memo } from "react";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import s from "./EveryPrompt.module.css";
import { type Card, type LogSession, rowDomId } from "./log-model";
import { AREA_COMMAND_ONLY, LANE_NAMES, modelLabel } from "./model";
import { AREA_COLOURS, sessionSpan } from "./SessionTimeline";
import type { Session } from "./session-model";

/** COMMITS_SHOWN is how many commit links a card shows before "+N more". */
const COMMITS_SHOWN = 5;

/** Props of one session card header. */
type SessionCardProps = {
  card: Card;
  /** The session as the summary has it (title, area, span, counts, markers). */
  session: Session | undefined;
  /** The session as the log has it (branches, models, commits), when present. */
  info: LogSession | undefined;
  /** The window's first day ("YYYY-MM-DD"), for dates. */
  start: string;
  open: boolean;
  onToggle: (key: string) => void;
  /** Whether this is the card the reader jumped to. */
  highlighted: boolean;
};

/** Returns a session's duration in words: "2 h 51 min" within a day, else "spanned N days". */
function duration(x: Session): string {
  if (x.spannedDays > 1) return `spanned ${x.spannedDays} days`;
  const m = x.lastDay * 1440 + x.lastMin - (x.firstDay * 1440 + x.firstMin);
  if (m < 1) return "a single moment";
  const h = Math.floor(m / 60);
  return h ? `${h} h ${m % 60} min` : `${m} min`;
}

/**
 * The header of one session card: a disclosure button with the title (and a
 * "title by Claude" marker for titles Claude wrote), then area, founding and
 * article badges, Pacific dates and duration, how many of its prompts the
 * filter shows, its linked commits (five, then "+N more"), branches and
 * models. All text renders as text nodes.
 */
function SessionCard({
  card,
  session: x,
  info,
  start,
  open,
  onToggle,
  highlighted,
}: SessionCardProps) {
  const title = x?.title ?? info?.title ?? card.key;
  const area = x?.area ?? info?.area ?? "";
  const byClaude = (x?.titleSource ?? info?.titleSource) === "claude-written";
  const commits = info?.commits ?? [];
  const shown = card.rows.length;
  // The disclosure controls the card's prompts (list items after the card).
  const controls = open
    ? card.rows.map((r) => rowDomId(r.id)).join(" ")
    : undefined;
  return (
    <div className={`${s.card} ${highlighted ? s.cardHi : ""}`}>
      <button
        type="button"
        className={s.cardBtn}
        aria-expanded={open}
        aria-controls={controls}
        data-card={card.key}
        onClick={() => onToggle(card.key)}
      >
        <span className={s.cardChevron} aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className={s.cardTitle}>{title}</span>
        <span className={s.cardCount}>
          {shown === card.total
            ? `${fmtInt(card.total)} prompt${card.total === 1 ? "" : "s"}`
            : `${fmtInt(shown)} of ${fmtInt(card.total)} prompts shown`}
        </span>
      </button>
      <div className={s.cardMeta}>
        {byClaude ? <span className={s.byClaude}>title by Claude</span> : null}
        {area && area !== AREA_COMMAND_ONLY ? (
          <span className={s.cardArea}>
            <i
              className={s.sw}
              style={{ background: AREA_COLOURS[area] ?? PALETTE.gray5 }}
              aria-hidden="true"
            />
            {area}
          </span>
        ) : (
          <span className={s.cardArea}>only commands or shell escapes</span>
        )}
        {x?.founding ? <span className={s.badge}>founding</span> : null}
        {x?.articleWork ? <span className={s.badge}>article work</span> : null}
        {x ? (
          <span>
            {sessionSpan(start, x)} · {duration(x)}
          </span>
        ) : null}
        {x ? <span>{LANE_NAMES[x.lane]}</span> : null}
        {info?.branches.length ? (
          <span>
            branch{info.branches.length === 1 ? "" : "es"}{" "}
            {info.branches.join(", ")}
          </span>
        ) : null}
        {info?.models.length ? (
          <span>{info.models.map(modelLabel).join(", ")}</span>
        ) : null}
        {commits.length ? (
          <span className={s.rowCommits}>
            {commits.length} commit{commits.length === 1 ? "" : "s"}:{" "}
            {commits.slice(0, COMMITS_SHOWN).map((c) => (
              <CommitLink key={c} hash={c} repo="rustty" />
            ))}
            {commits.length > COMMITS_SHOWN
              ? ` +${commits.length - COMMITS_SHOWN} more`
              : ""}
          </span>
        ) : null}
        {info?.commitsAmbiguous ? (
          <span className={s.cardQuiet}>
            {info.commitsAmbiguous} more commit
            {info.commitsAmbiguous === 1 ? "" : "s"} not linked (ambiguous)
          </span>
        ) : null}
      </div>
    </div>
  );
}

export default memo(SessionCard);
