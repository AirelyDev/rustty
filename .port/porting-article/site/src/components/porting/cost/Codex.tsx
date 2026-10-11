import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import s from "./Cost.module.css";
import { type CostData, fmtMoney, fmtTok, placeLabels, utc } from "./model";
import { codexPerPoint, DROP_HOUR } from "./stats";
import TipSvg from "./TipSvg";

/** Meter frame: time domain, plot box and the right-axis dollar maximum. */
const M = {
  t0: utc("2026-07-31T14"),
  t1: utc("2026-08-02T16"),
  X0: 34,
  X1: 420,
  Y0: 66,
  Y1: 236,
  usdMax: 2500,
};
/** LABEL_ROWS are the y baselines of the event-label rows. */
const LABEL_ROWS = [12, 25, 38, 51];
/** LABEL_CHAR is the estimated width of one 9px label character. */
const LABEL_CHAR = 5.6;

/** Maps a UTC instant to x. */
function mx(t: number): number {
  return M.X0 + ((t - M.t0) / (M.t1 - M.t0)) * (M.X1 - M.X0);
}
/** Maps a meter percentage to y. */
function my(v: number): number {
  return M.Y1 - (v / 100) * (M.Y1 - M.Y0);
}
/** Maps cumulative dollars to y (right axis). */
function mc(v: number): number {
  return M.Y1 - (v / M.usdMax) * (M.Y1 - M.Y0);
}
/** Maps an hour stamp to the x of the middle of that hour. */
function hx(h: string): number {
  return mx(utc(h) + 18e5);
}

/** Places the event labels in four rows (shared greedy placer, time order); a label that fits no row goes in the last row. */
function placeCodexLabels(d: CostData): { row: number; end: boolean }[] {
  const placed = placeLabels(
    d.cev.map((e, i) => ({ i, x: mx(utc(e.t)), label: e.label })),
    { rows: LABEL_ROWS.length, charW: LABEL_CHAR, pad: 0, width: 462, gap: 3 },
  );
  return d.cev.map((e, i) => {
    const p = placed.get(i);
    return p
      ? { row: p.row, end: p.end }
      : { row: LABEL_ROWS.length - 1, end: mx(utc(e.t)) > 300 };
  });
}

/**
 * The Codex audit against its weekly meter: the hourly median reading with a
 * min–max band, cumulative Rustty Codex usage at list price (dashed, right
 * axis), the seven audit events, the unexplained 08-01 04:00 drop and a
 * short account of run 2. Below: three mini stats.
 */
export default function Codex({
  d,
  tip,
  tipId,
}: {
  d: CostData;
  tip: ChartTip;
  tipId: string;
}) {
  const m = d.meter;
  const last = m[m.length - 1];
  const per = codexPerPoint(d);
  const lab = placeCodexLabels(d);
  const dropX = hx(DROP_HOUR);
  const band = `${m.map((r, i) => `${i ? "L" : "M"}${hx(r[0])} ${my(r[3])}`).join("")}${[
    ...m,
  ]
    .reverse()
    .map((r) => `L${hx(r[0])} ${my(r[2])}`)
    .join("")}Z`;
  const text = { fontSize: 9.5, fill: PALETTE.gray5 };
  return (
    <div className={s.panel}>
      <div className={s.pt}>
        <h4>Codex audit: the weekly meter</h4>
        <span>07-31 → 08-02 · ChatGPT Pro</span>
      </div>
      <TipSvg
        tip={tip}
        tipId={tipId}
        viewBox="0 0 460 340"
        label="Codex weekly usage meter and cumulative list-price cost during the audit"
      >
        {[0, 25, 50, 75, 100].map((v) => (
          <g key={v}>
            <line
              x1={M.X0}
              x2={M.X1}
              y1={my(v)}
              y2={my(v)}
              stroke={PALETTE.gray2}
              strokeWidth={0.6}
            />
            <text
              x={M.X0 - 4}
              y={my(v) + 3}
              fontSize={9}
              fill={PALETTE.blue}
              textAnchor="end"
            >
              {`${v}%`}
            </text>
          </g>
        ))}
        {[0, 1000, 2000].map((v) => (
          <text
            key={v}
            x={M.X1 + 4}
            y={mc(v) + 3}
            fontSize={9}
            fill={PALETTE.rust}
          >
            {v ? `$${v / 1000}k` : "$0"}
          </text>
        ))}
        <path d={band} fill={PALETTE.blue} fillOpacity={0.14} />
        <polyline
          points={m.map((r) => `${hx(r[0])},${my(r[1])}`).join(" ")}
          fill="none"
          stroke={PALETTE.blue}
          strokeWidth={2}
        />
        <polyline
          points={m.map((r) => `${hx(r[0])},${mc(r[4])}`).join(" ")}
          fill="none"
          stroke={PALETTE.rust}
          strokeWidth={1.6}
          strokeDasharray="4 3"
        />
        <rect
          x={mx(utc(d.meterNote.from))}
          y={M.Y0}
          width={mx(utc(d.meterNote.drop)) - mx(utc(d.meterNote.from))}
          height={M.Y1 - M.Y0}
          fill={PALETTE.yellow}
          fillOpacity={0.08}
          stroke={PALETTE.yellow}
          strokeOpacity={0.35}
          strokeDasharray="2 3"
        />
        <path d={`M${dropX} ${my(8)}V${my(58)}`} stroke={PALETTE.gray5} />
        {d.meterNote.lines.map((l, i) => (
          <text
            key={l}
            x={dropX - 3}
            y={my(66) + i * 11}
            fontSize={9}
            fill={PALETTE.gray5}
            textAnchor="end"
          >
            {l}
          </text>
        ))}
        <text
          x={hx(last[0])}
          y={my(last[1]) - 6}
          fontSize={11}
          fill={PALETTE.blue}
          textAnchor="end"
          fontWeight={700}
        >
          {`${last[1]}%`}
        </text>
        <text
          x={hx(m[0][0])}
          y={my(m[0][1]) - 6}
          fontSize={10}
          fill={PALETTE.blue}
        >
          {`${m[0][1]}%`}
        </text>
        <text
          x={hx(last[0]) - 4}
          y={mc(last[4]) + 14}
          fontSize={10}
          fill={PALETTE.rust}
          textAnchor="end"
        >
          {fmtMoney(last[4])}
        </text>
        {d.cev.map((e, i) => {
          const x = mx(utc(e.t));
          const ly = LABEL_ROWS[lab[i].row];
          return (
            <g key={e.t} data-tip={`cev:${i}`}>
              <line
                x1={x}
                x2={x}
                y1={ly + 3}
                y2={M.Y1}
                stroke={PALETTE.gray6}
                strokeOpacity={0.28}
                strokeDasharray="2 2"
              />
              <text
                x={lab[i].end ? x - 2 : x + 2}
                y={ly}
                fontSize={9}
                fill={PALETTE.gray6}
                textAnchor={lab[i].end ? "end" : "start"}
              >
                {e.label}
              </text>
              <rect
                x={x - 4}
                y={ly - 10}
                width={8}
                height={M.Y1 - ly + 10}
                fill="transparent"
              />
            </g>
          );
        })}
        {m.map((r, i) => (
          <rect
            key={r[0]}
            data-tip={`meter:${i}`}
            x={hx(r[0]) - 4}
            y={M.Y0}
            width={8}
            height={M.Y1 - M.Y0}
            fill="transparent"
          />
        ))}
        {[
          "2026-08-01T00",
          "2026-08-01T12",
          "2026-08-02T00",
          "2026-08-02T12",
        ].map((h) => (
          <g key={h}>
            <line
              x1={mx(utc(h))}
              x2={mx(utc(h))}
              y1={M.Y1}
              y2={M.Y1 + 4}
              stroke={PALETTE.gray3}
            />
            <text
              x={mx(utc(h))}
              y={M.Y1 + 15}
              fontSize={9}
              fill={PALETTE.gray4}
              textAnchor="middle"
            >
              {`${h.slice(5, 10)} ${h.slice(11)}h`}
            </text>
          </g>
        ))}
        <text x={M.X0} y={M.Y1 + 30} fontSize={9.5} fill={PALETTE.blue}>
          — weekly meter (median per hour; band = min–max)
        </text>
        <text x={M.X0} y={M.Y1 + 43} fontSize={9.5} fill={PALETTE.rust}>
          - - cumulative Rustty Codex usage at list price
        </text>
        <text x={M.X0} y={M.Y1 + 58} {...text}>
          Run 2 used gpt-5.5: about 9.8k calls at high effort,
        </text>
        <text x={M.X0} y={M.Y1 + 70} {...text}>
          then about 13.2k at medium after the author's prompt.
        </text>
        <text x={M.X0} y={M.Y1 + 82} {...text}>
          It stopped at 99%, with 0.35% of the 3.1M-line scope
        </text>
        <text x={M.X0} y={M.Y1 + 94} {...text}>
          credited in Codex's own strict ledger.
        </text>
      </TipSvg>
      <div className={s.mini}>
        <div>
          <b>{`~$${Math.round(per.perPoint)} per point`}</b>from the 1% reading
          on 08-01 04:00 (after the unexplained drop) to {last[1]}%, the meter
          consumed {fmtMoney(per.usd)} of list-price usage
        </div>
        <div>
          <b>{`${d.codexAudit.calls.toLocaleString("en-US")} calls`}</b>
          {d.codexAudit.threads} threads, {fmtTok(d.codexAudit.tokens)} tokens (
          {fmtTok(d.codexAudit.cacheRead)} cache reads), {fmtMoney(last[4])} at
          list price
        </div>
        <div>
          <b>99% Rustty</b>in that window Rustty used {fmtMoney(last[4])} of
          list-price Codex usage and other projects{" "}
          {fmtMoney(d.codexAudit.otherUsd)}, so the whole audit counts as
          Rustty's (no 50% split)
        </div>
      </div>
    </div>
  );
}
