import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import s from "./Cost.module.css";
import Hatch from "./Hatch";
import {
  type CostData,
  fmtMoney,
  leverage,
  maxedWeek,
  paidRows,
} from "./model";
import TipSvg from "./TipSvg";

/** Log-scale frame of the paid chart: plot left/right and the $ domain. */
const P = { L0: 150, L1: 372, lo: Math.log10(30), hi: Math.log10(300000) };
/** ROW_H is the vertical pitch of the rows; ROW_Y0 the first row's y. */
const ROW_H = 30;
/** ROW_Y0 is the first row's centre y. */
const ROW_Y0 = 34;

/** Maps dollars to x on the log scale. */
function px(v: number): number {
  return P.L0 + ((Math.log10(v) - P.lo) / (P.hi - P.lo)) * (P.L1 - P.L0);
}

/** Formats a value compactly: "$23.8k", "$422". */
function fk(v: number): string {
  return v >= 1000 ? `$${(v / 1000).toFixed(1)}k` : `$${Math.round(v)}`;
}

/** Props for the paid panel. */
type Props = { d: CostData; tip: ChartTip; tipId: string; uid: string };

/**
 * Paid vs list price on a log scale: the two subscription shares, the paid
 * central and range, then the recovered (measured), lost-window (modeled,
 * hatched, low–high) and API-equivalent total (a reference value) rows, with
 * the leverage bracket between paid and the API-equivalent total. Below:
 * three mini stats (a maxed week against the plan's weekly price, the Codex
 * audit, the unknown extra-usage credits).
 */
export default function Paid({ d, tip, tipId, uid }: Props) {
  const rows = paidRows(d);
  const lev = leverage(d);
  const week = maxedWeek(d);
  const hatch = `${uid}-phatch`;
  const yb = ROW_Y0 + rows.length * ROW_H + 14;
  const a = px(d.paid.central);
  const b = px(d.api.total[1]);
  return (
    <div className={s.panel}>
      <div className={s.pt}>
        <h4>Paid vs list price</h4>
        <span>log scale · Rustty</span>
      </div>
      <TipSvg
        tip={tip}
        tipId={tipId}
        viewBox={`0 0 460 ${yb + 36}`}
        label="Subscription dollars paid versus API-equivalent list price, on a log scale"
      >
        <Hatch id={hatch} />
        {[100, 1000, 10000, 100000].map((v) => (
          <g key={v}>
            <line
              x1={px(v)}
              x2={px(v)}
              y1={18}
              y2={yb - 22}
              stroke={PALETTE.gray2}
            />
            <text
              x={px(v)}
              y={12}
              fontSize={9.5}
              fill={PALETTE.gray4}
              textAnchor="middle"
            >
              {fmtMoney(v)}
            </text>
          </g>
        ))}
        {rows.map((r, i) => {
          const y = ROW_Y0 + i * ROW_H;
          const end = r.range ? r.range[1] : r.v;
          return (
            <g key={r.name} data-tip={`paid:${i}`}>
              <rect
                x={0}
                y={y - 13}
                width={460}
                height={27}
                fill="transparent"
              />
              <text
                x={0}
                y={y - 1}
                fontSize={10.5}
                fill={PALETTE.gray7}
                className={s.sans}
              >
                {r.name}
              </text>
              <text x={0} y={y + 10} fontSize={9} fill={PALETTE.gray4}>
                {r.sub}
              </text>
              {r.range ? (
                <rect
                  x={px(r.range[0])}
                  y={y - 6}
                  width={Math.max(3, px(r.range[1]) - px(r.range[0]))}
                  height={12}
                  rx={2}
                  fill={r.kind === "hatch" ? `url(#${hatch})` : r.color}
                  fillOpacity={r.kind === "hatch" ? 1 : 0.35}
                  stroke={r.color}
                />
              ) : null}
              <line
                x1={P.L0}
                x2={px(r.v)}
                y1={y}
                y2={y}
                stroke={r.color}
                strokeOpacity={0.35}
              />
              <circle cx={px(r.v)} cy={y} r={4.5} fill={r.color} />
              <text x={px(end) + 8} y={y + 3.5} fontSize={10.5} fill={r.color}>
                {fk(r.v)}
                {r.range ? (
                  <tspan fill={PALETTE.gray4} fontSize={9}>
                    {` ${fk(r.range[0])}–${fk(r.range[1])}`}
                  </tspan>
                ) : null}
              </text>
            </g>
          );
        })}
        <g data-tip="lev">
          <path
            d={`M${a} ${yb - 6}V${yb}H${b}V${yb - 6}`}
            fill="none"
            stroke={PALETTE.gray8}
            strokeOpacity={0.6}
          />
          <text
            x={(a + b) / 2}
            y={yb + 16}
            fontSize={12}
            fill={PALETTE.gray8}
            textAnchor="middle"
            fontWeight={700}
          >
            {`~${Math.round(lev.central)}× list price per dollar paid`}
          </text>
          <text
            x={(a + b) / 2}
            y={yb + 30}
            fontSize={9.5}
            fill={PALETTE.gray4}
            textAnchor="middle"
          >
            {`≈${Math.round(lev.low)}–${Math.round(lev.high)}× with consistent bounds`}
          </text>
          <rect x={a} y={yb - 8} width={b - a} height={42} fill="transparent" />
        </g>
      </TipSvg>
      <div className={s.mini}>
        <div>
          <b>{`${Math.round(week.range[0])}–${Math.round(week.range[1])}×`}</b>a
          maxed week ({fmtMoney(d.ceil.usd[0])}–{fmtMoney(d.ceil.usd[2])} at
          list price) against the plan's {fmtMoney(week.perWeek)} per week ($
          {d.paid.price} × 12 ÷ 52)
        </div>
        <div>
          <b>{`${fmtMoney(d.paid.codexMonth.rustty)} in ~${d.codexAudit.hours} h`}</b>
          Rustty's Codex audit at list price; the whole $200 Pro month carried{" "}
          {fmtMoney(d.paid.codexMonth.rustty + d.paid.codexMonth.other)} of
          Codex usage, {(d.paid.codexShare * 100).toFixed(1)}% of it Rustty
        </div>
        <div>
          <b>unknown</b>extra-usage credits:{" "}
          <span className={s.mono}>/usage-credits</span> run twice on 07-13,
          credits ran out on 09-08. Not included.
        </div>
      </div>
    </div>
  );
}
