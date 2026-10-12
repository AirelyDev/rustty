import CommitLink from "../ui/CommitLink";
import data from "@/app/porting/data/strategy-timeline.json";
import { PALETTE } from "../lib/palette";
import { linear } from "../lib/scales";
import Hot, { TipBody } from "./Hot";
import { laneStyle, shapePath } from "./marks";
import { dayOffset, md, placeLabels } from "./model";
import s from "./StaticCharts.module.css";
import KeyGroup from "../ui/KeyGroup";

/** ERA_TOP and ERA_H place the era band row; TRIGGER_Y the trigger marker. */
const ERA_TOP = 0;
const ERA_H = 30;
const TRIGGER_Y = 42;

/** LANE_TOP is where the first lane starts; LANE_H is each lane's height. */
const LANE_TOP = 54;
const LANE_H = 52;

/** CHAR_W is the approximate width in px of one 10px character of a marker label. */
const CHAR_W = 6.1;

/** ERA_FILLS shade the three era bands, darkest first. */
const ERA_FILLS = [PALETTE.gray1, "#1d211f", "#232826"];

/** MONTH_STARTS get a faint vertical grid line. */
const MONTH_STARTS = ["2026-07-01", "2026-08-01", "2026-09-01", "2026-10-01"];

/**
 * The method-change lanes in a W-wide viewBox: era bands with their commit
 * cadence, the trigger marker, then one lane per kind of change with a marker
 * per change and labels placed in two rows (a label that fits nowhere keeps
 * its marker and tooltip). Plot x runs from left to W − 34, matching the step
 * strips drawn under it.
 */
export default function Lanes({ W, left }: { W: number; left: number }) {
  const right = W - 34;
  const [start, end] = data.range;
  const x = linear([0, dayOffset(end, start)], [left, right]);
  /** Maps an ISO date to x. */
  const xd = (d: string) => x(dayOffset(d, start));
  const H = LANE_TOP + data.lanes.length * LANE_H + 4;
  const laneIndex = new Map(data.lanes.map((l, i) => [l.key, i]));
  const marks = data.events.map((e, i, all) => {
    const same = all
      .slice(0, i)
      .filter((o) => o.lane === e.lane && o.d === e.d).length;
    return { ...e, li: laneIndex.get(e.lane) ?? 0, mx: xd(e.d) + same * 9 };
  });
  const slots = new Array<number>(marks.length).fill(-1);
  data.lanes.forEach((l) => {
    const idx = marks
      .map((m, i) => (m.lane === l.key ? i : -1))
      .filter((i) => i >= 0);
    const placed = placeLabels(
      idx.map((i) => ({
        x: marks[i].mx - 3,
        w: marks[i].label.length * CHAR_W,
      })),
      { rows: 2, gap: 6 },
    );
    idx.forEach((i, k) => {
      slots[i] =
        marks[i].mx - 3 + marks[i].label.length * CHAR_W <= W ? placed[k] : -1;
    });
  });
  const tx = xd(data.trigger.d);
  return (
    <div className={s.plot}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className={s.svg}
        role="img"
        aria-label={`${data.events.length} method changes from ${md(start)} to ${md(end)} in ${data.lanes.length} lanes: ${data.lanes.map((l) => l.label).join(", ")}.`}
      >
        {data.eras.map((e, i) => (
          <g key={e.start}>
            <rect
              x={xd(e.start)}
              y={ERA_TOP}
              width={xd(e.end) + (x(1) - x(0)) - xd(e.start)}
              height={H - ERA_TOP}
              fill={ERA_FILLS[i]}
            />
            <text x={xd(e.start) + 5} y={ERA_TOP + 12} className={s.eraText}>
              {e.label}
            </text>
            <text x={xd(e.start) + 5} y={ERA_TOP + 25} className={s.axisText}>
              {e.perDay.toFixed(1)} commits a day
            </text>
          </g>
        ))}
        {MONTH_STARTS.map((d) => (
          <line
            key={d}
            x1={xd(d)}
            x2={xd(d)}
            y1={ERA_H + 2}
            y2={H}
            stroke={PALETTE.gray2}
            strokeDasharray="1 5"
          />
        ))}
        <path
          d={`M${tx - 5},${TRIGGER_Y - 5}h10l-5,7Z`}
          fill="none"
          stroke={PALETTE.gray6}
          strokeWidth={1.2}
        />
        <text
          x={tx - 9}
          y={TRIGGER_Y + 2}
          textAnchor="end"
          className={s.axisText}
        >
          {data.trigger.label}
        </text>
        {data.lanes.map((l, i) => {
          const y0 = LANE_TOP + i * LANE_H;
          return (
            <g key={l.key}>
              <line x1={0} x2={W} y1={y0} y2={y0} stroke={PALETTE.gray2} />
              <text
                x={left - 10}
                y={y0 + 14}
                textAnchor="end"
                className={s.laneText}
              >
                {l.label}
              </text>
            </g>
          );
        })}
        {marks.map((m, i) => {
          const { colour, shape } = laneStyle(m.li);
          const y0 = LANE_TOP + m.li * LANE_H;
          return (
            <g key={`${m.d}-${m.label}`}>
              <path
                d={shapePath(shape, m.mx, y0 + 11, 4.5)}
                fill={shape === "cross" ? "none" : colour}
                stroke={shape === "cross" ? colour : PALETTE.card}
                strokeWidth={shape === "cross" ? 2 : 1}
              />
              {slots[i] >= 0 ? (
                <text
                  x={m.mx - 3}
                  y={y0 + 28 + slots[i] * 13}
                  className={s.markText}
                >
                  {m.label}
                </text>
              ) : null}
            </g>
          );
        })}
      </svg>
      <KeyGroup
        label={`Method changes: ${data.eras.length} eras, the trigger and ${marks.length} events. Arrow keys move between them.`}
      >
        {data.eras.map((e) => (
          <Hot
            key={e.start}
            id={`stl-era-${e.start}`}
            x={xd(e.start)}
            y={ERA_TOP}
            w={xd(e.end) - xd(e.start)}
            h={ERA_H}
            W={W}
            H={H}
            label={`Era: ${e.label}`}
          >
            <TipBody
              title={`${e.label}: ${md(e.start)} to ${md(e.end)}`}
              rows={[
                `${e.commits.toLocaleString("en-US")} commits in ${e.days} days, ${e.perDay.toFixed(1)} a day`,
              ]}
            />
          </Hot>
        ))}
        <Hot
          id="stl-trigger"
          x={tx - 40}
          y={TRIGGER_Y - 8}
          w={48}
          h={14}
          W={W}
          H={H}
          label="Trigger marker"
        >
          <TipBody title={md(data.trigger.d)} rows={[data.trigger.text]} />
        </Hot>
        {marks.map((m, i) => {
          const y0 = LANE_TOP + m.li * LANE_H;
          const wLabel = slots[i] >= 0 ? m.label.length * CHAR_W : 0;
          return (
            <Hot
              key={`${m.d}-${m.label}`}
              id={`stl-ev-${i}`}
              x={m.mx - 7}
              y={y0 + 4}
              w={Math.max(14, wLabel + 4)}
              h={slots[i] >= 0 ? 14 + (slots[i] + 1) * 13 : 16}
              W={W}
              H={H}
              label={`${md(m.d)}: ${m.label}`}
            >
              <TipBody
                title={`${md(m.d)} · ${m.label}`}
                rows={[
                  m.text,
                  ...(m.commit
                    ? [
                        <>
                          commit <CommitLink hash={m.commit} />
                        </>,
                      ]
                    : []),
                ]}
              />
            </Hot>
          );
        })}
      </KeyGroup>
    </div>
  );
}
