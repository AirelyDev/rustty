import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import { type CostData, niceCeil } from "./model";
import { type EraStat, eraStats } from "./stats";
import TipSvg from "./TipSvg";

/** ERA_COLOR is each era's colour, in order. */
export const ERA_COLOR = ["#ffbc7a", "#c678dd", "#d97934"];
/** ERA_SHORT are the era legend labels. */
const ERA_SHORT = [
  "before the written rules",
  "rules, VM, Codex",
  "upstream catch-ups, orchestrator",
];

/** A metric of the era chart: label, [low, central, high] per era, and whether it is a dollar cost. */
export type EraMetric = {
  name: string;
  of: (e: EraStat) => [number, number, number];
  cost: boolean;
};

/** ERA_METRICS are the four rows of the era chart. */
export const ERA_METRICS: EraMetric[] = [
  { name: "$ per commit", of: (e) => e.perCommit, cost: true },
  { name: "$ per prompt", of: (e) => e.perPrompt, cost: true },
  { name: "$ per 1k code lines added", of: (e) => e.perKLines, cost: true },
  {
    name: "commits per prompt",
    of: (e) => [e.commitsPerPrompt, e.commitsPerPrompt, e.commitsPerPrompt],
    cost: false,
  },
];

/** Plot left and right of the era chart. */
const X0 = 150;
/** X1 is the plot's right edge. */
const X1 = 420;

/**
 * Efficiency over the three eras: for $ per commit, per prompt, per 1k code
 * lines added and commits per prompt, one dot per era with a low–high
 * whisker; a hollow dot means the era's cost is mostly modeled.
 */
export default function Eras({
  d,
  tip,
  tipId,
}: {
  d: CostData;
  tip: ChartTip;
  tipId: string;
}) {
  const eras = eraStats(d);
  return (
    <TipSvg
      tip={tip}
      tipId={tipId}
      viewBox="0 0 460 236"
      label="List-price cost per commit, per prompt and per thousand code lines, and commits per prompt, by era"
    >
      <text x={0} y={9} fontSize={9.5} fill={PALETTE.gray4}>
        eras:
      </text>
      {ERA_SHORT.map((n, i) => (
        <text
          key={n}
          x={[36, 208, 320][i]}
          y={9}
          fontSize={9.5}
          fill={ERA_COLOR[i]}
        >
          {`● ${n}`}
        </text>
      ))}
      {ERA_METRICS.map((m, mi) => {
        const y0 = 24 + mi * 53;
        const vals = eras.map(m.of);
        const mx = niceCeil(Math.max(...vals.map((v) => v[2])) * 1.05);
        /** Maps a metric value to x. */
        const x = (v: number) => X0 + (v / mx) * (X1 - X0);
        return (
          <g key={m.name}>
            <text x={0} y={y0 + 20} fontSize={10.5} fill={PALETTE.gray7}>
              {m.name}
            </text>
            <line
              x1={X0}
              x2={X1}
              y1={y0 + 44}
              y2={y0 + 44}
              stroke={PALETTE.gray2}
            />
            <text x={X1 + 4} y={y0 + 47} fontSize={8.5} fill={PALETTE.gray3}>
              {m.cost ? `$${mx}` : mx.toFixed(0)}
            </text>
            {eras.map((e, i) => {
              const v = vals[i];
              const y = y0 + 8 + i * 13;
              const hollow = m.cost && e.mostlyModeled;
              return (
                <g key={e.name} data-tip={`era:${mi}:${i}`}>
                  <rect
                    x={X0}
                    y={y - 5}
                    width={X1 - X0}
                    height={12}
                    fill="transparent"
                  />
                  {m.cost && v[2] - v[0] > 0.01 ? (
                    <line
                      x1={x(v[0])}
                      x2={x(v[2])}
                      y1={y}
                      y2={y}
                      stroke={ERA_COLOR[i]}
                      strokeOpacity={0.55}
                      strokeWidth={2}
                    />
                  ) : null}
                  <circle
                    cx={x(v[1])}
                    cy={y}
                    r={4}
                    fill={hollow ? PALETTE.card : ERA_COLOR[i]}
                    stroke={ERA_COLOR[i]}
                    strokeWidth={1.6}
                  />
                  <text
                    x={x(m.cost ? v[2] : v[1]) + 7}
                    y={y + 3.5}
                    fontSize={9.5}
                    fill={PALETTE.gray5}
                  >
                    {m.cost ? `$${v[1].toFixed(1)}` : v[1].toFixed(1)}
                  </text>
                </g>
              );
            })}
          </g>
        );
      })}
    </TipSvg>
  );
}
