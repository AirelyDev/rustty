import s from "./AgentsVsHuman.module.css";
import {
  type AvhData,
  type BrushStats,
  dayLabel,
  fmtN,
  type Metric,
  pct,
  type Tile,
} from "./model";

/** METRICS are the metric toggle buttons: key and label. */
const METRICS: [Metric, string][] = [
  ["c", "Commits"],
  ["l", "Lines changed"],
  ["p", "Prompts"],
];

/**
 * Headline totals with the live replay counters, and the toolbar: metric
 * toggle, reset to all days and the current range.
 */
export function Header({
  data,
  stats,
  metric,
  onMetric,
  onReset,
}: {
  data: AvhData;
  stats: BrushStats;
  metric: Metric;
  onMetric: (m: Metric) => void;
  onReset: () => void;
}) {
  const nDays = data.daily.date.length;
  const whole = stats.a === 0 && stats.b === nDays - 1;
  return (
    <>
      <div className={s.top}>
        <div className={s.big}>
          <b>{fmtN(stats.n)}</b> commits · <i>{fmtN(stats.nPrompts)}</i> prompts
          <small>
            {pct(stats.gt1, stats.n)} of commits landed more than an hour after
            the author’s last prompt
          </small>
        </div>
        <div className={s.ctr} aria-live="polite">
          <span>
            date<b>{dayLabel(data, stats.b)}</b>
          </span>
          <span>
            commits<b>{fmtN(stats.n)}</b>
          </span>
          <span>
            prompts<b>{fmtN(stats.nPrompts)}</b>
          </span>
          <span>
            lines ±<b>{fmtN(stats.lines)}</b>
          </span>
        </div>
      </div>
      <div className={s.ctrl} role="toolbar" aria-label="Heatmap controls">
        {METRICS.map(([m, label]) => (
          <button
            key={m}
            type="button"
            className={s.btn}
            aria-pressed={metric === m}
            onClick={() => onMetric(m)}
          >
            {label}
          </button>
        ))}
        <span className={s.sep} />
        <button type="button" className={s.btn} onClick={onReset}>
          ↻ All {nDays} days
        </button>
        <span className={s.range}>
          {whole ? "All days" : "Zoomed"} · {dayLabel(data, stats.a)} –{" "}
          {dayLabel(data, stats.b)} · {stats.b - stats.a + 1} days
        </span>
      </div>
    </>
  );
}

/** The six insight tiles for the current range and replay position. */
export function InsightTiles({ tiles }: { tiles: Tile[] }) {
  return (
    <div className={s.ins}>
      {tiles.map((t) => (
        <div key={t.text.slice(0, 24)} className={s.tile}>
          <b>{t.value}</b>
          <span>{t.text}</span>
        </div>
      ))}
    </div>
  );
}
