import data from "@/app/porting/data/audit-funnel.json";
import { PALETTE } from "../lib/palette";
import { linear } from "../lib/scales";
import Hot, { TipBody } from "./Hot";
import { dayOffset, placeLabels } from "./model";
import s from "./StaticCharts.module.css";
import KeyGroup from "../ui/KeyGroup";

/** START and END bound the meter's time axis (UTC hours). */
const START = "2026-07-31T17:00";
const END = "2026-08-02T16:00";

/** BUCKET is how many hourly readings one hover column covers. */
const BUCKET = 3;

/** LABEL_ROW is the height in px of one event-label row. */
const LABEL_ROW = 14;

/** CHAR_W is the approximate width in px of one 10px monospace character. */
const CHAR_W = 6;

/** One hourly meter row: [hour, median %, min %, max %, cumulative list-price $]. */
type MeterRow = [string, number, number, number, number];

/** Formats an hour key "2026-08-01T06" as "08-01 06:00". */
function hourLabel(t: string): string {
  return `${t.slice(5, 10)} ${t.slice(11, 13)}:00`;
}

/**
 * Codex's weekly-limit meter during the two audit runs, drawn in a W × H
 * viewBox with the page-wide treatment it shares with the cost widget: run
 * bands named "run 1" and "run 2", the hourly median reading as a line with
 * its min–max band, the unexplained readings of 08-01 00:00–03:59 shaded and
 * annotated, and the seven audit events as ticks whose labels sit in `rows`
 * rows above the plot (a label that fits no row keeps its tick and tooltip).
 * One hover column covers three hours.
 */
export default function Meter({
  W,
  H,
  rows,
  idp,
}: {
  W: number;
  H: number;
  rows: number;
  idp: string;
}) {
  const meter = data.meter as MeterRow[];
  const note = data.meterNote;
  const top = 8 + rows * LABEL_ROW + 6;
  const left = 34;
  const right = W - 10;
  const bottom = H - 24;
  const x = linear([0, dayOffset(END, START)], [left, right]);
  /** Maps a UTC timestamp to x. */
  const xt = (t: string) => x(dayOffset(t, START));
  const y = linear([0, 100], [bottom, top]);
  const pts = meter.map(([h, med, lo, hi]) => ({
    h,
    med,
    lo,
    hi,
    cx: xt(`${h}:30`),
  }));
  const line = pts
    .map((p, i) => `${i ? "L" : "M"}${p.cx},${y(p.med)}`)
    .join("");
  const band = `${pts.map((p, i) => `${i ? "L" : "M"}${p.cx},${y(p.hi)}`).join("")}${[
    ...pts,
  ]
    .reverse()
    .map((p) => `L${p.cx},${y(p.lo)}`)
    .join("")}Z`;
  const evX = data.events.map((e) => xt(e.t));
  // Each label starts just right of its tick, pulled left where it would leave the viewBox.
  const labX = data.events.map((e, i) =>
    Math.min(evX[i] + 3, W - 2 - e.label.length * CHAR_W),
  );
  const slots = placeLabels(
    data.events.map((e, i) => ({
      x: labX[i] - 3,
      w: e.label.length * CHAR_W + 6,
    })),
    { rows, gap: 4 },
  );
  const buckets: (typeof pts)[] = [];
  for (let i = 0; i < pts.length; i += BUCKET)
    buckets.push(pts.slice(i, i + BUCKET));
  const nx0 = xt(`${note.from}:00`);
  const nx1 = xt(`${note.drop}:00`);
  const noteRight = nx1 + 6 + note.lines[0].length * CHAR_W < right;
  const first = pts[0];
  const last = pts[pts.length - 1];
  const dayTicks = ["2026-08-01T00:00", "2026-08-02T00:00"];
  return (
    <div className={s.plot}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className={s.svg}
        role="img"
        aria-label={`Codex weekly limit used, median reading per hour with its range: ${first.med}% at the start of run 1, ${last.med}% at the last call of run 2. ${note.text}`}
      >
        {data.runs.map((r) => (
          <g key={r.label}>
            <rect
              x={xt(r.start)}
              y={top}
              width={xt(r.end) - xt(r.start)}
              height={bottom - top}
              fill={PALETTE.gray1}
            />
            <text x={xt(r.start) + 4} y={top + 12} className={s.axisText}>
              {r.label}
            </text>
          </g>
        ))}
        <rect
          x={nx0}
          y={top}
          width={nx1 - nx0}
          height={bottom - top}
          fill={PALETTE.yellow}
          fillOpacity={0.08}
          stroke={PALETTE.yellow}
          strokeOpacity={0.35}
          strokeDasharray="2 3"
        />
        {[0, 25, 50, 75, 100].map((v) => (
          <g key={v}>
            <line
              x1={left}
              x2={right}
              y1={y(v)}
              y2={y(v)}
              stroke={PALETTE.gray2}
              strokeDasharray={v === 0 ? undefined : "2 4"}
            />
            <text
              x={left - 4}
              y={y(v) + 3}
              textAnchor="end"
              className={s.axisText}
            >
              {v}%
            </text>
          </g>
        ))}
        {dayTicks.map((t) => (
          <g key={t}>
            <line
              x1={xt(t)}
              x2={xt(t)}
              y1={bottom}
              y2={bottom + 4}
              stroke={PALETTE.gray3}
            />
            <text
              x={xt(t)}
              y={bottom + 15}
              textAnchor="middle"
              className={s.axisText}
            >
              {t.slice(5, 10)}
            </text>
          </g>
        ))}
        {data.events.map((e, i) => (
          <g key={e.t}>
            <line
              x1={evX[i]}
              x2={evX[i]}
              y1={slots[i] >= 0 ? 8 + slots[i] * LABEL_ROW + 4 : top}
              y2={bottom}
              stroke={PALETTE.gray4}
              strokeDasharray="1 3"
            />
            {slots[i] >= 0 ? (
              <text
                x={labX[i]}
                y={8 + slots[i] * LABEL_ROW + 9}
                className={s.evText}
              >
                {e.label}
              </text>
            ) : null}
          </g>
        ))}
        <path d={band} fill={PALETTE.blue} fillOpacity={0.16} />
        <path d={line} fill="none" stroke={PALETTE.blue} strokeWidth={2} />
        {pts.map((p) => (
          <circle
            key={p.h}
            cx={p.cx}
            cy={y(p.med)}
            r={2.2}
            fill={PALETTE.blue}
          />
        ))}
        {note.lines.map((l, i) => (
          <text
            key={l}
            x={noteRight ? nx1 + 6 : nx0 - 4}
            y={y(40) + i * 12}
            textAnchor={noteRight ? "start" : "end"}
            className={s.noteText}
          >
            {l}
          </text>
        ))}
        <text x={first.cx + 4} y={y(first.med) - 8} className={s.valText}>
          {first.med}%
        </text>
        <text
          x={last.cx - 4}
          y={y(last.med) - 8}
          textAnchor="end"
          className={s.valText}
        >
          {last.med}%
        </text>
      </svg>
      <KeyGroup label="Weekly-limit readings by hour, then the marked events. Arrow keys move between them.">
        {buckets.map((b) => {
          const half = (x(1 / 24) - x(0)) / 2;
          const x0 = b[0].cx - half;
          const x1 = b[b.length - 1].cx + half;
          const flagged = b.some((p) => p.h >= note.from && p.h <= note.to);
          return (
            <Hot
              key={b[0].h}
              id={`${idp}-h-${b[0].h}`}
              x={x0}
              y={top}
              w={x1 - x0}
              h={bottom - top}
              W={W}
              H={H}
              label={`Weekly limit readings from ${hourLabel(b[0].h)} UTC`}
            >
              <TipBody
                title={`${hourLabel(b[0].h)} to ${b[b.length - 1].h.slice(11, 13)}:59 UTC`}
                rows={[
                  ...b.map(
                    (p) =>
                      `${p.h.slice(11, 13)}:00 · median reading ${p.med}% (readings ${p.lo}–${p.hi}%)`,
                  ),
                  ...(flagged ? [note.text] : []),
                ]}
              />
            </Hot>
          );
        })}
        {data.events.map((e, i) => (
          <Hot
            key={e.t}
            id={`${idp}-e-${i}`}
            x={slots[i] >= 0 ? labX[i] - 4 : evX[i] - 4}
            y={slots[i] >= 0 ? 8 + slots[i] * LABEL_ROW - 2 : top - 12}
            w={slots[i] >= 0 ? e.label.length * CHAR_W + 10 : 10}
            h={LABEL_ROW}
            W={W}
            H={H}
            label={`Event: ${e.label}`}
          >
            <TipBody
              title={`${e.t.slice(5, 10)} ${e.t.slice(11)} UTC`}
              rows={[e.text]}
            />
          </Hot>
        ))}
      </KeyGroup>
    </div>
  );
}
