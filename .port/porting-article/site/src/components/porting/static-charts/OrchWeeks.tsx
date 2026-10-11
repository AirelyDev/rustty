import data from "@/app/porting/data/orchestration.json";
import { fmtInt } from "../lib/format";
import Hot, { TipBody } from "./Hot";
import { ctxSplit, md } from "./model";
import { DAILY, H, kTok, mondayOf, STREAM } from "./orch-shared";
import KeyGroup from "../ui/KeyGroup";

/**
 * One hover column per week over the whole orchestration chart (W-wide
 * viewBox): xs are the weeks' left edges and xEnd the right edge of the last.
 * Each tooltip lists that week's commits by model, context sizes, scripts
 * added and agent threads.
 */
export default function OrchWeeks({
  W,
  idp,
  xs,
  xEnd,
}: {
  W: number;
  idp: string;
  xs: number[];
  xEnd: number;
}) {
  const weeks = data.weeks;
  const ctx = ctxSplit(data.ctx, 100);
  return (
    <KeyGroup
      label={`${weeks.length} weeks of orchestration. Arrow keys move between them.`}
    >
      {weeks.map((w, i) => {
        /** Whether an ISO date falls in this week. */
        const inWeek = (d: string) => mondayOf(d) === w.w;
        const scripts = data.scripts
          .filter((d) => inWeek(d.d))
          .reduce((a, d) => a + d.n, 0);
        const th = data.threads.filter((t) => inWeek(t.d));
        const cl = ctx.claude.find((c) => c.w === w.w);
        const cx = ctx.codex.find((c) => c.w === w.w);
        const total = w.n.reduce((a, b) => a + b, 0);
        return (
          <Hot
            key={w.w}
            id={`${idp}-w-${w.w}`}
            x={xs[i]}
            y={STREAM.top}
            w={(xs[i + 1] ?? xEnd) - xs[i]}
            h={DAILY.bottom - STREAM.top}
            W={W}
            H={H}
            label={`Week of ${md(w.w)}`}
          >
            <TipBody
              title={`Week of ${md(w.w)}: ${fmtInt(total)} commits`}
              rows={[
                ...w.n.flatMap((n, m) =>
                  n ? [`${data.models[m]}: ${fmtInt(n)}`] : [],
                ),
                ...(cl
                  ? [
                      `Claude context: median ${kTok(cl.median)}, 90th percentile ${kTok(cl.p90)} (${fmtInt(cl.calls)} calls)`,
                    ]
                  : []),
                ...(cx
                  ? [
                      `Codex context: median ${kTok(cx.median)} (${fmtInt(cx.calls)} calls)`,
                    ]
                  : []),
                ...(scripts ? [`${scripts} workflow scripts added`] : []),
                ...(th.length
                  ? [
                      `Agent threads: ${fmtInt(th.reduce((a, t) => a + t.claude, 0))} Claude, ${fmtInt(th.reduce((a, t) => a + t.codex, 0))} Codex`,
                    ]
                  : []),
              ]}
            />
          </Hot>
        );
      })}
    </KeyGroup>
  );
}
