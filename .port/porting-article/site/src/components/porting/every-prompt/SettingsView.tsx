"use client";

import { type CSSProperties, useId, useMemo, useRef, useState } from "react";
import { PALETTE } from "../lib/palette";
import useChartTip from "../ui/useChartTip";
import { useRovingFocus } from "../ui/useRovingFocus";
import s from "./EveryPrompt.module.css";
import {
  type EpData,
  legendKeys,
  modelLabel,
  type Segment,
  shortDate,
  UNKNOWN,
  type Week,
  weekBars,
} from "./model";

/** View of the settings bars: model × effort, or permission mode. */
type View = "settings" | "modes";

/** MODEL_COLOURS colour each model family (shared with the agents-vs-human ribbon where they overlap). */
const MODEL_COLOURS: Record<string, string> = {
  "claude-opus-4-8": PALETTE.gold,
  "claude-fable-5": PALETTE.purple,
  "claude-fable-5-1": PALETTE.purple,
  "claude-opus-5": PALETTE.blue,
  "claude-opus-5-5": PALETTE.pass,
  "claude-sonnet-5": PALETTE.orange,
  "claude-sonnet-5-5": PALETTE.orange,
  "claude-haiku-5-5": PALETTE.yellow,
  "gpt-5.5": PALETTE.upstream,
  "gpt-5.6-sol": PALETTE.upstream,
};

/** MODE_COLOURS colour the permission modes. */
const MODE_COLOURS: Record<string, string> = {
  bypass: PALETTE.rust,
  default: PALETTE.blue,
  plan: PALETTE.purple,
  acceptEdits: PALETTE.yellow,
};

/** MODE_NAMES are the display names of the permission modes. */
const MODE_NAMES: Record<string, string> = {
  bypass: "bypass permissions",
  default: "default (asks first)",
  plan: "plan",
  acceptEdits: "accept edits",
};

/** EFFORT_CLASS gives each effort its texture: high solid, medium lighter, not recorded striped. */
const EFFORT_CLASS: Record<string, string> = {
  high: "",
  medium: s.effMedium,
  [UNKNOWN]: s.effNone,
};

/** Returns a segment's display label. */
function segLabel(seg: Segment, view: View): string {
  if (seg.kind === "unknown")
    return view === "modes" ? "mode unknown" : "settings unknown";
  if (view === "modes") return MODE_NAMES[seg.model ?? ""] ?? seg.model ?? "";
  const m = modelLabel(seg.model ?? "");
  if (seg.kind === "inferred") return `${m}, inferred from commits`;
  return seg.effort === UNKNOWN
    ? `${m}, effort not recorded`
    : `${m}, ${seg.effort} effort`;
}

/** Returns a segment's CSS class (texture) and colour variables. */
function segStyle(
  seg: Segment,
  view: View,
): { className: string; style?: CSSProperties } {
  if (seg.kind === "unknown") return { className: s.segUnknown };
  const c =
    (view === "modes"
      ? MODE_COLOURS[seg.model ?? ""]
      : MODEL_COLOURS[seg.model ?? ""]) ?? PALETTE.gray5;
  if (seg.kind === "inferred") {
    return { className: s.segInferred, style: { "--c": c } as CSSProperties };
  }
  const eff =
    view === "modes" ? "" : (EFFORT_CLASS[seg.effort ?? UNKNOWN] ?? "");
  return {
    className: `${s.segKnown} ${eff}`,
    style: { "--c": c } as CSSProperties,
  };
}

/**
 * Weekly stacked bars of the settings each prompt ran under: model × effort
 * (or permission mode, by toggle). Recorded settings are solid; Mac models
 * inferred from commit trailers are dashed outlines; prompts with no known
 * settings are hatched "unknown". Every week from the first to the last
 * prompt is drawn, empty weeks included. Segments show a tooltip on hover,
 * tap or focus (one Tab stop; arrow keys move between segments). weeks
 * are the weekly settings of the prompts passing the filter (weeksFromTicks),
 * so the bars and the legend counts follow every filter.
 */
export default function SettingsView({
  data,
  weeks,
  whole,
  kindLabel,
}: {
  data: EpData;
  /** Weekly settings of the prompts passing the filter. */
  weeks: Week[];
  /** Whether the filter is the default view. */
  whole: boolean;
  /** The categories shown, in words ("instructions and decisions"). */
  kindLabel: string;
}) {
  const [view, setView] = useState<View>("settings");
  const bars = useMemo(
    () => weekBars(weeks, data.models, view),
    [weeks, data, view],
  );
  const legend = useMemo(
    () => legendKeys(weeks, data.models, view),
    [weeks, data, view],
  );
  const max = Math.max(1, ...bars.map((b) => b.total));
  const boxRef = useRef<HTMLDivElement>(null);
  const barsRef = useRef<HTMLDivElement>(null);
  const tip = useChartTip(boxRef);
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const tipId = `ep${uid}-set`;
  useRovingFocus(barsRef, { selector: "[data-seg]" });

  const [wi, key] = tip.shown ? tip.shown.split("~") : ["", ""];
  const bar = tip.shown ? bars[Number(wi)] : null;
  const seg = bar?.segments.find((x) => x.key === key) ?? null;

  /** Returns the viewport anchor of a segment element (its top-right corner). */
  const anchorOf = (el: Element) => {
    const r = el.getBoundingClientRect();
    return { x: r.right - 4, y: r.top };
  };

  return (
    <div className={s.settings} ref={boxRef}>
      <div className={s.settingsHead}>
        <h4>Settings per week</h4>
        <fieldset className={s.toggle}>
          <legend className={s.srOnly}>Show</legend>
          <button
            type="button"
            className={s.btn}
            aria-pressed={view === "settings"}
            onClick={() => setView("settings")}
          >
            Model and effort
          </button>
          <button
            type="button"
            className={s.btn}
            aria-pressed={view === "modes"}
            onClick={() => setView("modes")}
          >
            Permission mode
          </button>
        </fieldset>
      </div>
      <ul className={s.legend}>
        {legend.map((g) => {
          const st = segStyle(g, view);
          return (
            <li key={g.key}>
              <span
                className={`${s.legendSw} ${st.className}`}
                style={st.style}
              />
              {segLabel(g, view)} <span className={s.legendN}>{g.count}</span>
            </li>
          );
        })}
      </ul>
      <p className={s.settingsNote}>
        {whole
          ? `Over ${kindLabel}, as shown above.`
          : `Over the prompts shown (${kindLabel} matching the filter); bars are drawn to this view's scale.`}
      </p>
      <div ref={barsRef} className={s.weeks}>
        {bars.map((b, i) => (
          <div key={b.week} className={s.week}>
            <span className={s.weekLabel}>
              week of {shortDate(new Date(`${b.week}T00:00:00Z`))}
            </span>
            <div className={s.track}>
              {b.total === 0 ? (
                <span className={s.empty}>no prompts</span>
              ) : (
                <div
                  className={s.bar}
                  style={{ width: `${(b.total / max) * 100}%` }}
                >
                  {b.segments.map((g) => {
                    const st = segStyle(g, view);
                    const id = `${i}~${g.key}`;
                    const label = `Week of ${shortDate(new Date(`${b.week}T00:00:00Z`))}: ${segLabel(g, view)}, ${g.count} prompt${g.count === 1 ? "" : "s"}`;
                    return (
                      <button
                        key={g.key}
                        type="button"
                        data-seg=""
                        className={`${s.seg} ${st.className}`}
                        style={{ ...st.style, flexGrow: g.count }}
                        aria-label={label}
                        aria-describedby={tip.shown === id ? tipId : undefined}
                        onPointerEnter={(e) => {
                          if (e.pointerType === "mouse")
                            tip.hover(id, { x: e.clientX, y: e.clientY });
                        }}
                        onPointerLeave={(e) => {
                          if (e.pointerType === "mouse") tip.leave();
                        }}
                        onFocus={(e) =>
                          tip.focus(id, anchorOf(e.currentTarget))
                        }
                        onBlur={() => tip.leave()}
                        onClick={(e) => tip.tap(id, anchorOf(e.currentTarget))}
                      >
                        {g.count / max >= 0.07 ? g.count : ""}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
            <span className={s.weekTotal}>{b.total}</span>
          </div>
        ))}
      </div>
      <div
        ref={tip.tipRef}
        id={tipId}
        role="tooltip"
        className={s.tip}
        hidden={!seg}
        data-pinned={tip.pinned ? "" : undefined}
        style={{ left: tip.pos.left, top: tip.pos.top }}
      >
        {seg && bar ? (
          <>
            <div className={s.th}>
              week of {shortDate(new Date(`${bar.week}T00:00:00Z`))}
            </div>
            <div className={s.tm}>
              {segLabel(seg, view)}: {seg.count} of {bar.total} prompts
            </div>
            {seg.kind === "inferred" ? (
              <div className={s.tnote}>
                No settings recorded; model taken from the Co-Authored-By
                trailer of the commits linked to the prompt. Effort unknown.
              </div>
            ) : null}
            {seg.kind === "unknown" ? (
              <div className={s.tnote}>
                {view === "modes"
                  ? "No permission mode recorded. This includes the prompts whose model is inferred from commits, and a few whose model is recorded but not their mode."
                  : "No transcript or session record of the settings, and no linked commit names the model."}
              </div>
            ) : null}
          </>
        ) : null}
      </div>
      <p className={s.hint}>
        Solid: recorded in session transcripts; by model, solid is high effort,
        faded medium effort and striped effort not recorded. Dashed outline:
        model inferred from commits. Hatched: unknown. Bars are drawn to scale;
        every week from the first prompt to the last is shown.
      </p>
    </div>
  );
}
