import { PALETTE } from "../lib/palette";
import { type CostData, type CovClass, TRAILER_COLOR, utc } from "./model";
import { barBox, WK, wx } from "./weekly-geo";

/** COV_COLOR is the fill of each data-coverage class. */
export const COV_COLOR: Record<CovClass, string> = {
  rec: "#d97934",
  remnant: "#d9793488",
  vmcodex: "#61aeee",
  prompt: "#3a3f3b",
  none: "#151716",
};

/** COV_NAME describes each data-coverage class. */
export const COV_NAME: Record<CovClass, string> = {
  rec: "Mac Claude transcripts recovered",
  remnant: "Mac Claude remnant calls (partial day)",
  vmcodex: "only Linux VM Claude or Codex recovered",
  prompt: "prompt history and commits only (no token data)",
  none: "no port activity",
};

/** DAY_MS is one day in milliseconds. */
const DAY_MS = 864e5;

/** Returns the trailer-strip segments of a week: [model, commits], largest first. */
export function trailerParts(mc: Record<string, number>): [string, number][] {
  return Object.entries(mc).sort((a, b) => b[1] - a[1]);
}

/**
 * The two strips under the bars: per quota week, the commits by
 * Co-Authored-By model as a 100% bar with the commit count; per UTC day, a
 * data-coverage cell (what survives for that day) with a prompt-count
 * micro-bar below it. Trailer segments repeat their week's tooltip; each
 * day cell has its own.
 */
export default function WeeklyStrips({ d }: { d: CostData }) {
  const label = {
    fontSize: 9,
    fill: PALETTE.gray4,
    textAnchor: "end" as const,
  };
  return (
    <g>
      <text x={WK.X0 - 6} y={321} {...label}>
        trailers
      </text>
      {d.qw.map((q, i) => {
        const { x, w } = barBox(i);
        const parts = trailerParts(q.mc);
        const total = parts.reduce((a, [, v]) => a + v, 0);
        let at = x;
        return (
          <g key={q.s} data-tip={`w:${i}`} data-nokey="">
            {parts.map(([k, v]) => {
              const sw = (w * v) / total;
              const r = (
                <rect
                  key={k}
                  x={at}
                  y={313}
                  width={Math.max(0, sw - 0.5)}
                  height={9}
                  fill={TRAILER_COLOR[k] ?? PALETTE.gray3}
                />
              );
              at += sw;
              return r;
            })}
            {total ? (
              <text
                x={x + w / 2}
                y={333}
                fontSize={8.5}
                fill={PALETTE.gray4}
                textAnchor="middle"
              >
                {total}
              </text>
            ) : null}
          </g>
        );
      })}
      <text x={WK.X0 - 6} y={350} {...label}>
        data
      </text>
      <text x={WK.X0 - 6} y={366} {...label}>
        prompts
      </text>
      {d.cov.map((c, i) => {
        const a = utc(c[0]);
        const xa = wx(a);
        const cw = Math.max(0.5, wx(a + DAY_MS) - xa - 0.6);
        const ph = Math.min(10, c[2] / 5);
        return (
          <g key={c[0]} data-tip={`day:${i}`}>
            <rect
              x={xa}
              y={342}
              width={cw}
              height={12}
              fill={COV_COLOR[c[1]]}
            />
            {c[2] ? (
              <rect
                x={xa}
                y={366 - ph}
                width={cw}
                height={ph}
                fill={PALETTE.gray4}
              />
            ) : null}
            <rect
              x={xa}
              y={340}
              width={cw + 0.6}
              height={28}
              fill="transparent"
            />
          </g>
        );
      })}
    </g>
  );
}
