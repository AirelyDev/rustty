import { fmtInt } from "../lib/format";
import Tooltip from "../ui/Tooltip";
import s from "./EveryPrompt.module.css";
import {
  dayDate,
  type EpData,
  LANE_SHORT,
  shortDate,
  type Tick,
  type TileValues,
} from "./model";

/** A repeated prompt text and how often it was typed. */
export type Repeated = { text: string; count: number };

/** Props of the tile grid. */
type TilesProps = {
  data: EpData;
  values: TileValues;
  /** Tile values over the chosen categories (median length, under ten words). */
  kindValues: TileValues;
  /** The chosen categories in words ("instructions and decisions"). */
  kindLabel: string;
  /** Whether the filter is the unfiltered view. */
  whole: boolean;
  /** The longest prompt passing the filter, or null. */
  longest: Tick | null;
  /** Top repeated prompts from the loaded log, or null before it loads. */
  repeated: Repeated[] | null;
  /** Shows the longest prompt in the log (sets the search to its id). */
  onLongest: (i: number) => void;
};

/** Formats a share in [0, 1] as a whole percentage, or "—" when undefined. */
function pct(x: number | null): string {
  return x === null ? "—" : `${Math.round(x * 100)}%`;
}

/** Formats a median length: whole numbers plain, halves with one decimal, "—" when undefined. */
function chars(x: number | null): string {
  if (x === null) return "—";
  return Number.isInteger(x) ? fmtInt(x) : x.toFixed(1);
}

/**
 * The live tiles for the current filter: prompts, median length, share under
 * ten words, decision answers, commits per prompt, the longest prompt (with a
 * jump to it in the log) and the most repeated prompts; plus the whole log's
 * redaction count, which does not change with the filter.
 */
export default function Tiles({
  data,
  values: v,
  kindValues: kv,
  kindLabel,
  whole,
  longest,
  repeated,
  onLongest,
}: TilesProps) {
  const t = data.totals;
  return (
    <div className={s.tiles}>
      <div className={s.tile}>
        <b>{fmtInt(v.prompts)}</b>
        <span>
          {whole
            ? "prompts typed"
            : `of ${fmtInt(t.prompts)} prompts match the filter`}
        </span>
      </div>
      <div className={s.tile}>
        <b>{chars(kv.medianChars)}</b>
        <span>median length in characters, over {kindLabel}</span>
      </div>
      <div className={s.tile}>
        <b>{pct(kv.underTenWordsShare)}</b>
        <span>under ten words, over {kindLabel}</span>
      </div>
      <div className={s.tile}>
        <b>{fmtInt(v.decisions)}</b>
        <span>
          decision answers{" "}
          <Tooltip id="ep-decision-rule" content={data.rules.decision}>
            <span className={s.ruleMark}>rule</span>
          </Tooltip>
        </span>
      </div>
      <div className={s.tile}>
        <b>
          {v.commitsPerPrompt === null ? "—" : v.commitsPerPrompt.toFixed(1)}
        </b>
        <span>
          commits per prompt (all commits on these days ÷ prompts
          {v.commitsPerPrompt === null && v.prompts > 0
            ? "; for day ranges only, as commits are not split by lane or kind of prompt"
            : ""}
          )
        </span>
      </div>
      <div className={s.tile}>
        <b>{fmtInt(t.redactions)}</b>
        <span>
          redactions in {fmtInt(t.redactedPrompts)} prompts (whole log, not
          filtered)
        </span>
      </div>
      <div className={s.tile}>
        <b>{longest ? fmtInt(longest.chars) : "—"}</b>
        <span>
          {longest ? (
            <>
              characters, the longest prompt (
              {shortDate(dayDate(data.window[0], longest.day))},{" "}
              {LANE_SHORT[longest.lane]}){" "}
              <button
                type="button"
                className={s.linkBtn}
                onClick={() => onLongest(longest.i)}
              >
                show it in the log
              </button>
            </>
          ) : (
            "no prompt matches"
          )}
        </span>
      </div>
      <div className={`${s.tile} ${s.tileWide}`}>
        <b className={s.tileSmallHead}>most repeated</b>
        <span className={s.tileSub}>over {kindLabel}</span>
        {repeated === null ? (
          <span>open the log to compute</span>
        ) : repeated.length === 0 ? (
          <span>no prompt is repeated</span>
        ) : (
          <ol className={s.repeated}>
            {repeated.map((r) => (
              <li key={r.text}>
                <q>{r.text}</q> ×{r.count}
              </li>
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}
