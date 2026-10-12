import type { ReactNode } from "react";
import { EventGlyph } from "./Lanes";
import { PROFILE_H, PROFILE_Y, X } from "./geometry";
import {
  ERA_GROUPS,
  GROUP_NAMES,
  GROUP_COLOURS,
  type HourProfile as Profile,
  KIND_NAMES,
  type Metric,
  MODEL_COLOURS,
  MODEL_NAMES,
  MODEL_TEXTURES,
  rampColour,
} from "./model";
import { COST_SERIES } from "./Ribbons";

/** BAR_SPACE is the profile height available to bars and the line (the rest holds labels). */
const BAR_SPACE = PROFILE_H - 24;

/**
 * Draws the hour-of-day profile under the grid: share of commits per hour
 * (copper bars) and share of prompts per hour (teal line), on one % axis,
 * with the 00–06 overnight band shaded.
 */
export function HourProfile({ profile }: { profile: Profile }) {
  const smax = Math.max(0.01, ...profile.commits, ...profile.prompts);
  const y = (v: number) => PROFILE_Y + PROFILE_H - (BAR_SPACE * v) / smax;
  const cx = (hh: number) => X.grid + hh * X.cw + X.cw / 2;
  const out: ReactNode[] = [];
  for (let hh = 0; hh < 24; hh++) {
    const v = profile.commits[hh];
    const bh = (BAR_SPACE * v) / smax;
    out.push(
      <rect
        key={`b${hh}`}
        x={X.grid + hh * X.cw + 3}
        y={PROFILE_Y + PROFILE_H - bh}
        width={X.cw - 6}
        height={bh}
        rx={2}
        fill="#d97934"
      />,
    );
  }
  const hours: ReactNode[] = [];
  for (let hh = 0; hh < 24; hh += 3) {
    hours.push(
      <text
        key={`h${hh}`}
        x={cx(hh)}
        y={PROFILE_Y + PROFILE_H + 12}
        fill="#858a84"
        fontSize={9}
        textAnchor="middle"
      >
        {String(hh).padStart(2, "0")}
      </text>,
    );
  }
  return (
    <g>
      <rect
        x={X.grid}
        y={PROFILE_Y}
        width={6 * X.cw}
        height={PROFILE_H}
        fill="#121413"
      />
      <text x={X.grid + 3} y={PROFILE_Y + 10} fill="#4a4f4b" fontSize={8}>
        00–06 overnight
      </text>
      <text
        x={X.dateR}
        y={PROFILE_Y + 8}
        fill="#858a84"
        fontSize={9}
        textAnchor="end"
      >
        SHARE BY
      </text>
      <text
        x={X.dateR}
        y={PROFILE_Y + 19}
        fill="#858a84"
        fontSize={9}
        textAnchor="end"
      >
        HOUR
      </text>
      <text
        x={X.dateR}
        y={PROFILE_Y + PROFILE_H}
        fill="#4a4f4b"
        fontSize={8}
        textAnchor="end"
      >
        0%
      </text>
      <text
        x={X.dateR}
        y={PROFILE_Y + 32}
        fill="#4a4f4b"
        fontSize={8}
        textAnchor="end"
      >
        {(smax * 100).toFixed(0)}%
      </text>
      {out}
      <polyline
        fill="none"
        stroke="#64d8c6"
        strokeWidth={2}
        points={profile.prompts.map((v, hh) => `${cx(hh)},${y(v)}`).join(" ")}
      />
      {profile.prompts.map((v, hh) => (
        <circle
          // biome-ignore lint/suspicious/noArrayIndexKey: one point per fixed hour
          key={`p${hh}`}
          cx={cx(hh)}
          cy={y(v)}
          r={2.2}
          fill="#64d8c6"
          stroke="#0b0c0c"
        />
      ))}
      {hours}
    </g>
  );
}

/** Returns the legend title for metric. */
function metricTitle(metric: Metric): string {
  if (metric === "c") return "COMMITS / HOUR";
  if (metric === "l") return "LINES / HOUR";
  return "PROMPTS / HOUR";
}

/**
 * Draws the legend at the bottom right (colour ramp, overlay dots, hatches,
 * models, cost sources, event kinds) and the era-group key under the Gantt.
 */
export function Legend({
  metric,
  max,
  ids,
}: {
  metric: Metric;
  max: number;
  ids: string;
}) {
  const lx = X.model;
  const ly = PROFILE_Y + 2;
  const grey = "#9aa09a";
  return (
    <g>
      <text x={lx} y={ly + 8} fill="#858a84" fontSize={9}>
        {metricTitle(metric)} (log)
      </text>
      {Array.from({ length: 10 }, (_, j) => (
        <rect
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed ramp swatches
          key={`r${j}`}
          x={lx + j * 9}
          y={ly + 13}
          width={8}
          height={8}
          rx={1.5}
          fill={rampColour(metric, j / 9)}
        />
      ))}
      <text x={lx} y={ly + 31} fill="#4a4f4b" fontSize={8}>
        1
      </text>
      <text
        x={lx + 90}
        y={ly + 31}
        fill="#4a4f4b"
        fontSize={8}
        textAnchor="end"
      >
        {max.toLocaleString("en-US")}
      </text>
      <circle
        cx={lx + 4}
        cy={ly + 42}
        r={3}
        fill={metric === "p" ? "#d97934" : "#64d8c6"}
      />
      <text x={lx + 11} y={ly + 45} fill={grey} fontSize={8.5}>
        {metric === "p" ? "commits in hour" : "prompts in hour"}
      </text>
      <rect
        x={lx}
        y={ly + 50}
        width={8}
        height={8}
        fill={`url(#${ids}-quiet)`}
        stroke="#3a3f3b"
      />
      <text x={lx + 11} y={ly + 57} fill={grey} fontSize={8.5}>
        quiet stretch
      </text>
      <rect
        x={lx}
        y={ly + 62}
        width={8}
        height={8}
        fill={`url(#${ids}-lost)`}
        stroke="#3a3f3b"
      />
      <text x={lx + 11} y={ly + 69} fill={grey} fontSize={8.5}>
        cost not recorded
      </text>
      {MODEL_NAMES.map((name, j) => {
        const x = lx + 112 + (j % 3) * 86;
        const y = ly + 5 + Math.floor(j / 3) * 12;
        return (
          <g key={name}>
            <rect
              x={x}
              y={y}
              width={8}
              height={8}
              rx={1.5}
              fill={MODEL_COLOURS[j]}
            />
            {MODEL_TEXTURES[j] ? (
              <rect
                x={x}
                y={y}
                width={8}
                height={8}
                rx={1.5}
                fill={`url(#${ids}-${MODEL_TEXTURES[j]})`}
              />
            ) : null}
            <text x={x + 11} y={y + 7} fill={grey} fontSize={8.5}>
              {name}
            </text>
          </g>
        );
      })}
      {COST_SERIES.map(([, colour, name], j) => (
        <g key={name}>
          <rect
            x={lx + 112 + j * 86}
            y={ly + 33}
            width={8}
            height={8}
            rx={1.5}
            fill={colour}
          />
          <text x={lx + 123 + j * 86} y={ly + 40} fill={grey} fontSize={8.5}>
            {name} $
          </text>
        </g>
      ))}
      {KIND_NAMES.map(([kind, name], j) => {
        const x = lx + 112 + (j % 3) * 86;
        const y = ly + 50 + Math.floor(j / 3) * 12;
        return (
          <g key={kind}>
            <EventGlyph kind={kind} x={x + 4} y={y + 3} />
            <text x={x + 11} y={y + 6} fill={grey} fontSize={8.5}>
              {name}
            </text>
          </g>
        );
      })}
      {ERA_GROUPS.map((g, j) => (
        <g key={g}>
          <rect
            x={X.gantt + j * 7}
            y={PROFILE_Y}
            width={5}
            height={5}
            rx={1}
            fill={GROUP_COLOURS[g]}
          />
          <text
            x={X.gantt}
            y={PROFILE_Y + 16 + j * 9}
            fill={GROUP_COLOURS[g]}
            fontSize={8}
          >
            {GROUP_NAMES[g]}
          </text>
        </g>
      ))}
    </g>
  );
}
