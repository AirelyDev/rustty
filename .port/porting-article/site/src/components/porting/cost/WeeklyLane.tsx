import { PALETTE } from "../lib/palette";
import {
  type CostData,
  type LimitEvent,
  placeEventLabels,
  type Scope,
} from "./model";
import { WK, wxs } from "./weekly-geo";

/** LANE_ROWS are the y baselines of the four label rows above the markers. */
const LANE_ROWS = [18, 34, 50, 66];
/** LANE_Y is the y of the markers (slash commands sit 6 px lower). */
const LANE_Y = 86;

/** EVENT_STYLE gives each limit-signal kind its colour and marker shape (colour is never the only encoding). */
export const EVENT_STYLE: Record<
  LimitEvent["k"],
  { color: string; shape: string; name: string }
> = {
  weekly: { color: PALETTE.fail, shape: "diamond", name: "weekly limit" },
  session: { color: PALETTE.orange, shape: "circle", name: "session limit" },
  usage_credits_exhausted: {
    color: PALETTE.yellow,
    shape: "triangle",
    name: "credits out",
  },
  prompt: { color: PALETTE.rust, shape: "ring", name: "author's words" },
  slash: { color: PALETTE.gray5, shape: "square", name: "slash command" },
};

/** Draws one limit-signal marker at (x, y) in the kind's shape. */
function Marker({ e, x, y }: { e: LimitEvent; x: number; y: number }) {
  const { color, shape } = EVENT_STYLE[e.k];
  if (shape === "diamond") {
    return (
      <path
        d={`M${x} ${y - 6}L${x + 5} ${y}L${x} ${y + 6}L${x - 5} ${y}Z`}
        fill={color}
      />
    );
  }
  if (shape === "circle") return <circle cx={x} cy={y} r={4.5} fill={color} />;
  if (shape === "triangle") {
    return (
      <path
        d={`M${x} ${y - 5}L${x + 5} ${y + 4}L${x - 5} ${y + 4}Z`}
        fill={color}
      />
    );
  }
  if (shape === "ring") {
    return (
      <circle
        cx={x}
        cy={y}
        r={3.6}
        fill={PALETTE.card}
        stroke={e.r ? color : PALETTE.gray5}
        strokeWidth={1.6}
      />
    );
  }
  return <rect x={x - 2.5} y={y - 2.5} width={5} height={5} fill={color} />;
}

/**
 * The limit lane above the bars: one marker per limit signal (weekly limit
 * diamond, session limit disc, credits-out triangle, author's words ring,
 * slash-command square), labels placed greedily in four rows with a leader
 * line, the author's words in italics. In the port-only scope, signals from
 * other projects are dimmed (they still count: the quota is account-wide).
 */
export default function WeeklyLane({ d, sc }: { d: CostData; sc: Scope }) {
  const placed = placeEventLabels(d.ev, wxs, WK.W);
  return (
    <g>
      {d.ev.map((e, i) => {
        const x = wxs(e.t);
        const y = LANE_Y + (e.k === "slash" ? 6 : 0);
        const { color } = EVENT_STYLE[e.k];
        const p = placed.get(i);
        const prompt = e.k === "prompt";
        return (
          <g
            key={`${e.t}-${e.k}-${i}`}
            data-tip={`ev:${i}`}
            opacity={sc === "rustty" && !e.r ? 0.3 : 1}
          >
            {p ? (
              <>
                <line
                  x1={x}
                  x2={x}
                  y1={LANE_ROWS[p.row] + 3}
                  y2={y - 6}
                  stroke={color}
                  strokeOpacity={0.45}
                />
                <text
                  x={p.end ? x - 2 : x + 2}
                  y={LANE_ROWS[p.row]}
                  fontSize={9.5}
                  fill={prompt ? (e.r ? PALETTE.gold : PALETTE.gray5) : color}
                  textAnchor={p.end ? "end" : "start"}
                  fontStyle={prompt ? "italic" : undefined}
                >
                  {p.label}
                </text>
              </>
            ) : null}
            <Marker e={e} x={x} y={y} />
            <rect
              x={x - 6}
              y={y - 8}
              width={12}
              height={16}
              fill="transparent"
            />
          </g>
        );
      })}
    </g>
  );
}
