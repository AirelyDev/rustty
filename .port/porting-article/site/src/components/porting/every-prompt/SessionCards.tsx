import { fmtInt } from "../lib/format";
import s from "./EveryPrompt.module.css";

/** The log's two views: session cards (the default) or the flat list of prompts. */
export type LogMode = "sessions" | "prompts";

/** The toggle between the session cards and the flat prompt list. */
export function ModeToggle({
  mode,
  onMode,
}: {
  mode: LogMode;
  onMode: (m: LogMode) => void;
}) {
  return (
    <fieldset className={s.toggle}>
      <legend className={s.srOnly}>Show the log as</legend>
      <button
        type="button"
        className={s.btn}
        aria-pressed={mode === "sessions"}
        onClick={() => onMode("sessions")}
      >
        Sessions
      </button>
      <button
        type="button"
        className={s.btn}
        aria-pressed={mode === "prompts"}
        onClick={() => onMode("prompts")}
      >
        All prompts
      </button>
    </fieldset>
  );
}

/**
 * The collapsed group of command-only sessions (sessions holding only
 * Claude Code commands or shell escapes) at the end of the session cards: a
 * disclosure button with their count. When the category filter hides every
 * prompt they hold, it says so and offers to show every category.
 */
export function CommandOnlyGroup({
  cards,
  prompts,
  total,
  open,
  onToggle,
  onShowAll,
}: {
  /** Command-only sessions with prompts passing the filter. */
  cards: number;
  /** Their prompts passing the filter. */
  prompts: number;
  /** All command-only sessions in the data. */
  total: number;
  open: boolean;
  onToggle: () => void;
  /** Shows every category (null when every category is already shown). */
  onShowAll: (() => void) | null;
}) {
  if (cards === 0) {
    return (
      <div className={s.groupHead}>
        <span className={s.groupNote}>
          {fmtInt(total)} command-only sessions (only Claude Code commands or
          shell escapes) are hidden with the commands.
        </span>{" "}
        {onShowAll ? (
          <button type="button" className={s.linkBtn} onClick={onShowAll}>
            show all
          </button>
        ) : null}
      </div>
    );
  }
  return (
    <div className={s.groupHead}>
      <button
        type="button"
        className={s.cardBtn}
        aria-expanded={open}
        onClick={onToggle}
      >
        <span className={s.cardChevron} aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className={s.cardTitle}>
          {fmtInt(cards)} command-only session{cards === 1 ? "" : "s"}
        </span>
        <span className={s.cardCount}>
          {fmtInt(prompts)} prompt{prompts === 1 ? "" : "s"} · only Claude Code
          commands or shell escapes
        </span>
      </button>
    </div>
  );
}
