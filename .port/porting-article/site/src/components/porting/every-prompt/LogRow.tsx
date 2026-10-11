import { memo, useLayoutEffect, useRef, useState } from "react";
import { fmtInt } from "../lib/format";
import CommitLink from "../ui/CommitLink";
import Tooltip from "../ui/Tooltip";
import s from "./EveryPrompt.module.css";
import Glossed from "./Glossed";
import {
  BASIS_LABEL,
  badges,
  fmtPacific,
  OTHER_REPO_NOTE,
  type Row,
} from "./log-model";
import { LANE_COLOURS, LANE_NAMES, modelLabel } from "./model";

/** COLLAPSED_LINES is how many lines of text a collapsed row shows. */
const COLLAPSED_LINES = 6;

/** COMMITS_SHOWN is how many commit links a row shows before "+N more". */
const COMMITS_SHOWN = 5;

/** Props of one log row. */
type LogRowProps = {
  row: Row;
  glossary: Record<string, string>;
  /** Whether the whole text is shown (otherwise it is cut after six lines). */
  expanded: boolean;
  /** Expands or collapses the row with this id. */
  onToggle: (id: string) => void;
  /** Whether this is the row the reader jumped to. */
  highlighted: boolean;
  /** The element the row's tooltips stay inside (the log's scroll box). */
  boundary?: () => HTMLElement | null;
};

/** Returns the model of a row in words, marking a model inferred from commit trailers. */
function modelText(r: Row): string {
  if (!r.model) return "model unknown";
  const m = modelLabel(r.model);
  return r.settingsSource === "commit-trailer"
    ? `${m} (inferred from commits)`
    : m;
}

/** Returns the effort of a row in words. */
function effortText(r: Row): string {
  if (r.effort) return `effort ${r.effort}`;
  return r.settingsSource === "unknown" || r.settingsSource === "commit-trailer"
    ? "effort unknown"
    : "effort not recorded";
}

/**
 * One prompt of the log: Pacific date-time, lane, model, effort, mode and
 * length; the verbatim text (plain text nodes, cut after six lines with an
 * expand button); its badges; and the commits it led to as permalinks (five,
 * then "+N more"), or a quiet note where the evidence was ambiguous.
 */
function LogRow({
  row: r,
  glossary,
  expanded,
  onToggle,
  highlighted,
  boundary,
}: LogRowProps) {
  const textRef = useRef<HTMLDivElement>(null);
  const [overflows, setOverflows] = useState(false);
  const [allCommits, setAllCommits] = useState(false);

  // Offer the expand/collapse button only when the text is longer than six
  // lines (measured the same way whether the row is collapsed or not).
  useLayoutEffect(() => {
    const el = textRef.current;
    if (!el) return;
    /** Measures whether the text runs past the collapsed height. */
    const check = () => {
      const lh = Number.parseFloat(getComputedStyle(el).lineHeight) || 18.75;
      setOverflows(el.scrollHeight > COLLAPSED_LINES * lh + 2);
    };
    check();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const bs = badges(r);
  const ambiguous = r.commitsBasis === "ambiguous";
  const otherRepo = r.commitsBasis === "other-repo";
  const commits = allCommits ? r.commits : r.commits.slice(0, COMMITS_SHOWN);
  const more = r.commits.length - COMMITS_SHOWN;
  const textId = `ep-text-${r.id}`;
  return (
    <article
      className={s.row}
      data-highlighted={highlighted ? "" : undefined}
      aria-label={`Prompt of ${fmtPacific(r.ts)} Pacific, ${LANE_NAMES[r.lane]}`}
    >
      <div className={s.rowMeta}>
        <time dateTime={r.ts}>{fmtPacific(r.ts)}</time>
        <span className={s.rowLane}>
          <i
            className={s.sw}
            style={{ background: LANE_COLOURS[r.lane] }}
            aria-hidden="true"
          />
          {LANE_NAMES[r.lane]}
        </span>
        <span>{modelText(r)}</span>
        <span>{effortText(r)}</span>
        <span>mode {r.mode ?? "unknown"}</span>
        <span>{fmtInt(r.chars)} chars</span>
      </div>
      <div
        ref={textRef}
        id={textId}
        className={s.rowText}
        data-collapsed={expanded ? undefined : ""}
      >
        <Glossed
          text={r.text}
          glossary={glossary}
          rowId={r.id}
          boundary={boundary}
        />
      </div>
      {overflows ? (
        <button
          type="button"
          className={s.linkBtn}
          aria-expanded={expanded}
          aria-controls={textId}
          onClick={() => onToggle(r.id)}
        >
          {expanded ? "show less" : "show the whole prompt"}
        </button>
      ) : null}
      {bs.length || r.commits.length || ambiguous || otherRepo ? (
        <div className={s.rowFoot}>
          {bs.map((b) => (
            <span key={b} className={s.badge} data-badge={b}>
              {b}
            </span>
          ))}
          {ambiguous ? (
            <span className={s.quiet}>commits ambiguous</span>
          ) : otherRepo ? (
            <span className={s.quiet}>{OTHER_REPO_NOTE}</span>
          ) : r.commits.length ? (
            <span className={s.rowCommits}>
              <Tooltip
                id={`ep-basis-${r.id}`}
                boundary={boundary}
                content={`Commits ${BASIS_LABEL[r.commitsBasis] ?? "linked to this prompt"}.`}
              >
                <span className={s.ruleMark}>
                  {r.commits.length === 1
                    ? "1 commit"
                    : `${fmtInt(r.commits.length)} commits`}
                </span>
              </Tooltip>
              {commits.map((c) => (
                <CommitLink key={c} hash={c} repo="rustty" />
              ))}
              {more > 0 ? (
                <button
                  type="button"
                  className={s.linkBtn}
                  aria-expanded={allCommits}
                  onClick={() => setAllCommits((x) => !x)}
                >
                  {allCommits ? "fewer" : `+${fmtInt(more)} more`}
                </button>
              ) : null}
            </span>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

/** LogRow re-renders only when its own props change (rows re-render on every scroll). */
export default memo(LogRow);
