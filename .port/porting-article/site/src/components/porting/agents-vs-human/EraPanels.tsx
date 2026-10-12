import {
  type AvhData,
  dayLabel,
  type EraAutonomy,
  fmtLag,
  fmtN,
  LAG_LABELS,
} from "./model";
import s from "./AgentsVsHuman.module.css";

/** BW is the width of one histogram bucket in the small-multiple SVG. */
const BW = 24;

/** Returns the bar colour of lag bucket b: teal under 1 h, grey 1–4 h, copper beyond. */
function bucketColour(b: number): string {
  if (b < 2) return "#64d8c6";
  if (b < 4) return "#9aa09a";
  return "#d97934";
}

/**
 * Autonomy small multiples, one per human era: a histogram of the lag from
 * the author's last prompt to each commit in 7 buckets with a dashed median
 * line, the era's commit count and median. Hovering (or focusing) a card
 * outlines that era on the grid; clicking (or Enter/Space) zooms to it.
 */
export function AutonomyPanel({
  data,
  eras,
  hl,
  onHighlight,
  onZoom,
}: {
  data: AvhData;
  eras: EraAutonomy[];
  hl: number | null;
  onHighlight: (era: number | null) => void;
  onZoom: (era: number) => void;
}) {
  return (
    <div className={s.sm}>
      {eras.map((x) => {
        const mx = Math.max(1, ...x.hist);
        const cls = [s.smc, x.commits ? "" : s.dim, hl === x.era ? s.on : ""]
          .filter(Boolean)
          .join(" ");
        return (
          <button
            type="button"
            key={x.label}
            className={cls}
            aria-label={`Zoom to era ${x.label}: ${fmtN(x.commits)} commits, median lag ${fmtLag(x.medianLag)}`}
            onMouseEnter={() => onHighlight(x.era)}
            onMouseLeave={() => onHighlight(null)}
            onFocus={() => onHighlight(x.era)}
            onBlur={() => onHighlight(null)}
            onClick={() => onZoom(x.era)}
          >
            <span className={s.smTitle}>
              {x.label}
              <em>
                {dayLabel(data, x.start)}–{dayLabel(data, x.end)} ·{" "}
                {fmtN(x.commits)} commits · median {fmtLag(x.medianLag)}
                {x.noPrompt ? ` · ${x.noPrompt} no recorded prompt` : ""}
              </em>
            </span>
            <svg viewBox={`0 0 ${7 * BW} 64`} aria-hidden="true">
              {x.hist.map((v, b) => {
                const hh = (44 * v) / mx;
                return (
                  <rect
                    key={LAG_LABELS[b]}
                    x={b * BW + 2}
                    y={48 - hh}
                    width={BW - 4}
                    height={Math.max(v ? 1 : 0, hh)}
                    rx={2}
                    fill={bucketColour(b)}
                  >
                    <title>
                      {LAG_LABELS[b]}: {v} commits
                    </title>
                  </rect>
                );
              })}
              {x.medianPos !== null ? (
                <line
                  x1={x.medianPos * BW}
                  x2={x.medianPos * BW}
                  y1={2}
                  y2={48}
                  stroke="#f4f1ed"
                  strokeDasharray="2 2"
                />
              ) : null}
              <text x={0} y={60} fill="#4a4f4b" fontSize={8}>
                &lt;30m
              </text>
              <text
                x={3.5 * BW}
                y={60}
                fill="#4a4f4b"
                fontSize={8}
                textAnchor="middle"
              >
                2–4h
              </text>
              <text
                x={7 * BW}
                y={60}
                fill="#4a4f4b"
                fontSize={8}
                textAnchor="end"
              >
                &gt;24h
              </text>
            </svg>
          </button>
        );
      })}
    </div>
  );
}

/** One leverage bar with its value text. */
function Bar({
  value,
  max,
  colour,
  text,
}: {
  value: number;
  max: number;
  colour: string;
  text: string;
}) {
  return (
    <div className={s.levCell}>
      <div
        className={s.levBar}
        style={{
          background: colour,
          width: `${((64 * value) / max).toFixed(1)}px`,
          minWidth: value ? 2 : 0,
        }}
      />
      <span className={s.levText}>{text}</span>
    </div>
  );
}

/**
 * Leverage by era as two separate bar columns (no dual axis): commits per
 * prompt, and commits per hands-on hour with the hands-on hours.
 */
export function LeverageBars({
  eras,
  hl,
}: {
  eras: EraAutonomy[];
  hl: number | null;
}) {
  const m1 = Math.max(1, ...eras.map((r) => r.commitsPerPrompt));
  const m2 = Math.max(1, ...eras.map((r) => r.commitsPerHour));
  return (
    <div>
      <div className={s.lev}>
        <span />
        <span className={s.levHead}>COMMITS PER PROMPT</span>
        <span className={s.levHead}>PER HANDS-ON HOUR</span>
        {eras.map((r) => (
          <div key={r.label} className={s.levRow}>
            <span
              className={s.levLabel}
              style={{ color: hl === r.era ? "#f4f1ed" : undefined }}
            >
              {r.label}
            </span>
            <Bar
              value={r.commitsPerPrompt}
              max={m1}
              colour="#d97934"
              text={
                r.prompts
                  ? `${r.commitsPerPrompt.toFixed(1)}× · ${r.prompts} prompts`
                  : "–"
              }
            />
            <Bar
              value={r.commitsPerHour}
              max={m2}
              colour="#ffbc7a"
              text={
                r.handsOn
                  ? `${Math.round(r.commitsPerHour)} · ${r.handsOn.toFixed(1)} h`
                  : "–"
              }
            />
          </div>
        ))}
      </div>
      <p className={s.levNote}>
        Hands-on hours: prompts grouped when less than 30 min apart, each
        group's span + 5 min. The swing from GTK bring-up on the Linux VM (the
        author testing by hand, under one commit per prompt) to parity closure
        (about nine commits per prompt) is the agents taking over the loop.
      </p>
    </div>
  );
}
