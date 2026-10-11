import { PALETTE } from "../lib/palette";
import type { ChartTip } from "../ui/useChartTip";
import s from "./Cost.module.css";
import { type CostData, fmtMoney, fmtTok, pct, type Scope } from "./model";
import { delegation, kindTable, type ThreadKind, tokenMix } from "./stats";
import TipSvg from "./TipSvg";

/** Plot left and right of both token charts. */
const X0 = 78;
/** X1 is the plot's right edge. */
const X1 = 452;

/** KIND_COLOR is the fill of each thread kind in the delegation bars. */
export const KIND_COLOR: Record<ThreadKind, string> = {
  main: "#9aa09a",
  subagent: "#d97934",
  workflow: "#ffbc7a",
};

/** KIND_LABEL names each thread kind. */
export const KIND_LABEL: Record<ThreadKind, string> = {
  main: "main thread",
  subagent: "subagents",
  workflow: "workflow runs",
};

/** Props for the token-anatomy panel. */
type Props = { d: CostData; sc: Scope; tip: ChartTip; tipId: string };

/**
 * Token anatomy for the current scope: 100% bars of tokens and of dollars by
 * token kind (cache read, cache write, uncached input, output) joined by
 * ribbons; 100% dollar bars by main thread, subagents and workflow runs for
 * Claude, Codex and both, with the delegated share; and the unit-cost table
 * (calls, $ per call, median context, $ per million tokens).
 */
export default function Tokens({ d, sc, tip, tipId }: Props) {
  const mix = tokenMix(d, sc);
  const tt = mix.reduce((a, r) => a + r.tok, 0);
  const tu = mix.reduce((a, r) => a + r.usd, 0);
  const w = X1 - X0;
  const del = delegation(d, sc);
  let xa = X0;
  let xb = X0;
  return (
    <div className={s.panel}>
      <div className={s.pt}>
        <h4>Token anatomy</h4>
        <span>
          {sc === "rustty" ? "Rustty" : "all projects"} · hover or tap for $ per
          million
        </span>
      </div>
      <TipSvg
        tip={tip}
        tipId={tipId}
        viewBox="0 0 460 150"
        label="Share of tokens versus share of list-price dollars by token type"
      >
        <text x={0} y={30} fontSize={10.5} fill={PALETTE.gray7}>
          tokens
        </text>
        <text x={0} y={42} fontSize={9} fill={PALETTE.gray4}>
          {fmtTok(tt)}
        </text>
        <text x={0} y={114} fontSize={10.5} fill={PALETTE.gray7}>
          dollars
        </text>
        <text x={0} y={126} fontSize={9} fill={PALETTE.gray4}>
          {fmtMoney(tu)}
        </text>
        {mix.map((r, i) => {
          const wa = w * r.shareTok;
          const wb = w * r.shareUsd;
          const a = xa;
          const b = xb;
          xa += wa;
          xb += wb;
          return (
            <g key={r.key} data-tip={`mix:${i}`}>
              <path
                d={`M${a} 46 L${a + wa} 46 L${b + wb} 96 L${b} 96Z`}
                fill={r.color}
                fillOpacity={0.16}
              />
              <rect
                x={a}
                y={22}
                width={Math.max(1.2, wa - 1)}
                height={24}
                fill={r.color}
              />
              <rect
                x={b}
                y={96}
                width={Math.max(1.2, wb - 1)}
                height={24}
                fill={r.color}
              />
              {wa > 44 ? (
                <text
                  x={a + 4}
                  y={38}
                  fontSize={10}
                  fill={PALETTE.card}
                  fontWeight={700}
                >
                  {pct(r.shareTok)}
                </text>
              ) : i === mix.length - 1 ? (
                <text
                  x={X1}
                  y={14}
                  fontSize={9.5}
                  fill={r.color}
                  textAnchor="end"
                >
                  {`${r.name} ${pct(r.shareTok)}`}
                </text>
              ) : null}
              {wb > 44 ? (
                <text
                  x={b + 4}
                  y={112}
                  fontSize={10}
                  fill={PALETTE.card}
                  fontWeight={700}
                >
                  {pct(r.shareUsd)}
                </text>
              ) : null}
              {wb > 20 ? (
                <text
                  x={b + 2}
                  y={134 + (i % 2) * 11}
                  fontSize={9}
                  fill={r.color}
                >
                  {r.name}
                </text>
              ) : null}
            </g>
          );
        })}
      </TipSvg>
      <TipSvg
        tip={tip}
        tipId={tipId}
        viewBox="0 0 460 104"
        label="List-price usage by main thread, subagents and workflow runs"
      >
        <text x={0} y={10} fontSize={9.5} fill={PALETTE.gray4}>
          share of list-price $ ·
        </text>
        {(["main", "subagent", "workflow"] as const).map((k, i) => (
          <text
            key={k}
            x={134 + i * 80}
            y={10}
            fontSize={9.5}
            fill={KIND_COLOR[k]}
          >
            {`■ ${KIND_LABEL[k]}`}
          </text>
        ))}
        {del.rows.map((row, ri) => {
          const y = 20 + ri * 27;
          let x = X0;
          return (
            <g key={row.name}>
              <text x={0} y={y + 13} fontSize={10.5} fill={PALETTE.gray7}>
                {row.name}
              </text>
              {(["main", "subagent", "workflow"] as const).map((k) => {
                if (!row.parts[k]) return null;
                const ww = (w * row.parts[k]) / row.total;
                const x0 = x;
                x += ww;
                return (
                  <g key={k} data-tip={`del:${ri}:${k}`}>
                    <rect
                      x={x0}
                      y={y}
                      width={ww - 1}
                      height={18}
                      fill={KIND_COLOR[k]}
                    />
                    {ww > 36 ? (
                      <text
                        x={x0 + 4}
                        y={y + 13}
                        fontSize={10}
                        fill={PALETTE.card}
                        fontWeight={700}
                      >
                        {pct(row.parts[k] / row.total, 0)}
                      </text>
                    ) : null}
                  </g>
                );
              })}
              {ri === 2 ? (
                <text
                  x={X1}
                  y={y + 30}
                  fontSize={10}
                  fill={PALETTE.gray8}
                  textAnchor="end"
                >
                  {`delegated: ${pct(del.delegated)}`}
                </text>
              ) : null}
            </g>
          );
        })}
      </TipSvg>
      <table className={s.table}>
        <thead>
          <tr>
            <th scope="col">{sc === "rustty" ? "Rustty" : "all projects"}</th>
            <th scope="col">calls</th>
            <th scope="col">$ / call</th>
            <th scope="col">median context</th>
            <th scope="col">$ / M tokens</th>
          </tr>
        </thead>
        <tbody>
          {kindTable(d, sc).map((r) => (
            <tr key={r.key}>
              <td>{r.name}</td>
              <td>{r.calls.toLocaleString("en-US")}</td>
              <td>${r.perCall.toFixed(3)}</td>
              <td>{r.ctx === null ? "–" : fmtTok(r.ctx)}</td>
              <td>${r.perM.toFixed(2)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className={s.note}>
        Workflow runs are tagged only in recent Claude Code transcripts, so
        earlier workflow calls are counted as subagents or main. Unit costs mix
        models.
      </p>
    </div>
  );
}
