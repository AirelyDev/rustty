import data from "@/app/porting/data/orchestration.json";
import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import { md } from "./model";
import OrchChart from "./OrchChart";
import { MODEL_COLOURS, MODEL_TEXTURE } from "./orch-shared";
import s from "./StaticCharts.module.css";

/**
 * How the orchestration changed: which models wrote the commits, how large
 * each call's context grew, and the switch from hand-written workflow
 * scripts to agent workers in worktrees.
 */
export default function Orchestration() {
  const t = data.totals;
  return (
    <div className={s.stack}>
      <div className={s.kpis}>
        <span>
          <b>{t.scriptsCommitted}</b>committed workflow scripts, none after{" "}
          {md(data.scripts[data.scripts.length - 1].d)}
        </span>
        <span>
          <b>{fmtInt(t.subagents)}</b>subagents in {t.worktrees} worktrees in
          September
        </span>
        <span>
          <b>{t.subShare}%</b>of September's calls made by subagents
        </span>
      </div>
      <div className={s.legendLine}>
        {data.models.map((m, i) => (
          <span key={m}>
            <i
              className={s.swBox}
              style={{
                background: MODEL_COLOURS[i],
                backgroundImage: MODEL_TEXTURE[i]
                  ? "repeating-linear-gradient(-45deg, rgb(11 12 12 / 60%) 0 1.4px, transparent 1.4px 3.5px)"
                  : undefined,
              }}
            />
            {m}
            {MODEL_TEXTURE[i] ? " (striped)" : ""}
          </span>
        ))}
      </div>
      <div className={s.legendLine}>
        <span>
          <i className={s.swLine} style={{ background: PALETTE.rust }} />
          Claude median context (hollow: under 100 calls that week)
        </span>
        <span>
          <i className={s.swDiamond} style={{ borderColor: PALETTE.blue }} />
          Codex median context
        </span>
        <span>
          <i className={s.swBox} style={{ background: PALETTE.rust }} />
          scripts added
        </span>
        <span>
          <i className={s.swBox} style={{ background: PALETTE.upstream }} />
          Claude threads
        </span>
        <span>
          <i className={s.swBox} style={{ background: PALETTE.blue }} />
          Codex threads
        </span>
      </div>
      <div className={s.wideOnly}>
        <OrchChart W={960} idp="orw" labels />
      </div>
      <div className={s.narrowOnly}>
        <OrchChart W={330} idp="orn" labels={false} />
      </div>
    </div>
  );
}
