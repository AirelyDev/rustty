import { PALETTE } from "../lib/palette";
import {
  CAT_COLOR,
  ceiling,
  type CostData,
  fmtVal,
  Q0,
  type Scope,
  type Unit,
  WEEK_MS,
  weekBars,
  yTop,
} from "./model";
import {
  barBox,
  dayLabel,
  LOST_END,
  LOST_START,
  WEEKS,
  WK,
  wx,
  wxs,
} from "./weekly-geo";

/** Props for the bar layer of the weekly instrument. */
type Props = { d: CostData; sc: Scope; u: Unit; hatch: string };

/** Y_TICKS is the number of y gridline intervals. */
const Y_TICKS = 4;

/**
 * The bar layer of the weekly instrument: the shaded lost window, y grid,
 * the maxed-week ceiling band with its dashed central line, one stacked bar
 * per quota week (recovered categories, then the hatched modeled block with
 * its low–high whisker and the total), the x axis and the port start/end
 * lines. Each week column and the band label carry a tooltip.
 */
export default function WeeklyBars({ d, sc, u, hatch }: Props) {
  const { X0, X1, B0, B1 } = WK;
  const top = yTop(d, sc, u);
  /** Maps a value to y. */
  const y = (v: number) => B1 - (v / top) * (B1 - B0);
  const ceil = ceiling(d, sc, u);
  const lx0 = wxs(LOST_START);
  const lx1 = wxs(LOST_END);
  const bars = weekBars(d, sc, u);
  return (
    <g>
      <rect
        x={lx0}
        y={B0 - 4}
        width={lx1 - lx0}
        height={B1 - B0 + 4}
        fill="#ffffff"
        fillOpacity={0.025}
      />
      <text x={lx0 + 4} y={B0 + 8} fontSize={10} fill={PALETTE.gray4}>
        Mac Claude transcripts lost → modeled (hatched)
      </text>
      <text x={lx1 + 4} y={B0 + 8} fontSize={10} fill={PALETTE.gray4}>
        transcripts recovered → measured
      </text>
      {Array.from({ length: Y_TICKS + 1 }, (_, i) => {
        const v = (top * i) / Y_TICKS;
        return (
          <g key={v}>
            <line
              x1={X0}
              x2={X1}
              y1={y(v)}
              y2={y(v)}
              stroke={PALETTE.gray2}
              strokeWidth={i ? 0.6 : 1}
            />
            <text
              x={X0 - 6}
              y={y(v) + 3.5}
              fontSize={10}
              fill={PALETTE.gray4}
              textAnchor="end"
            >
              {i ? fmtVal(v, u) : "0"}
            </text>
          </g>
        );
      })}
      <rect
        x={X0}
        y={y(ceil[2])}
        width={X1 - X0}
        height={y(ceil[0]) - y(ceil[2])}
        fill={PALETTE.gold}
        fillOpacity={0.07}
      />
      <line
        x1={X0}
        x2={X1}
        y1={y(ceil[1])}
        y2={y(ceil[1])}
        stroke={PALETTE.gold}
        strokeDasharray="4 4"
        strokeOpacity={0.7}
      />
      {bars.map((b) => {
        const { x, w } = barBox(b.i);
        const [el, ec, eh] = b.ext;
        const wxm = x + w / 2;
        let acc = 0;
        return (
          <g key={b.start}>
            {b.rec.map(([c, v]) => {
              const y1 = y(acc);
              acc += v;
              return (
                <rect
                  key={c}
                  x={x}
                  y={y(acc)}
                  width={w}
                  height={Math.max(0, y1 - y(acc))}
                  fill={CAT_COLOR[c]}
                />
              );
            })}
            {ec > 0 ? (
              <g stroke={PALETTE.gray8} strokeOpacity={0.75}>
                <rect
                  x={x}
                  y={y(acc + ec)}
                  width={w}
                  height={y(acc) - y(acc + ec)}
                  fill={`url(#${hatch})`}
                  stroke={PALETTE.rust}
                  strokeOpacity={1}
                  strokeWidth={0.8}
                />
                <line x1={wxm} x2={wxm} y1={y(acc + el)} y2={y(acc + eh)} />
                <line
                  x1={wxm - 5}
                  x2={wxm + 5}
                  y1={y(acc + eh)}
                  y2={y(acc + eh)}
                />
                <line
                  x1={wxm - 5}
                  x2={wxm + 5}
                  y1={y(acc + el)}
                  y2={y(acc + el)}
                />
              </g>
            ) : null}
            {b.total > 0 ? (
              <text
                x={wxm}
                y={y(b.recSum + Math.max(ec, eh)) - 5}
                fontSize={9.5}
                fill={PALETTE.gray6}
                textAnchor="middle"
              >
                {fmtVal(b.total, u)}
              </text>
            ) : null}
            <rect
              data-tip={`w:${b.i}`}
              x={x - 5}
              y={B0 - 6}
              width={w + 10}
              height={B1 - B0 + 6}
              fill="transparent"
            />
          </g>
        );
      })}
      <g data-tip="band">
        <text x={lx0 + 4} y={B0 + 22} fontSize={10} fill={PALETTE.gold}>
          {`band: maxed week ${sc === "rustty" ? "× 50% " : ""}${fmtVal(ceil[0], u)}–${fmtVal(ceil[2], u)}`}
        </text>
        <rect x={lx0} y={B0 + 12} width={250} height={14} fill="transparent" />
      </g>
      {Array.from({ length: WEEKS + 1 }, (_, i) => {
        const a = Q0 + i * WEEK_MS;
        const t = wx(a);
        return (
          <g key={t}>
            <line x1={t} x2={t} y1={B1} y2={B1 + 4} stroke={PALETTE.gray3} />
            {i < WEEKS && i % 2 === 0 ? (
              <text
                x={wx(a + WEEK_MS / 2)}
                y={384}
                fontSize={9.5}
                fill={PALETTE.gray4}
                textAnchor="middle"
              >
                {dayLabel(a)}
              </text>
            ) : null}
          </g>
        );
      })}
      <text x={X0} y={400} fontSize={9.5} fill={PALETTE.gray3}>
        quota week start (Tue 22:00 UTC)
      </text>
      {["2026-06-21", "2026-10-08"].map((day) => (
        <line
          key={day}
          x1={wxs(day)}
          x2={wxs(day)}
          y1={B0 - 4}
          y2={372}
          stroke={PALETTE.gray4}
          strokeDasharray="2 3"
          strokeOpacity={0.6}
        />
      ))}
    </g>
  );
}
