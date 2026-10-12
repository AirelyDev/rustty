import { fmtInt } from "../lib/format";
import { PALETTE } from "../lib/palette";
import CommitLink from "../ui/CommitLink";
import CommitText from "../ui/CommitText";
import SourceLink from "../ui/SourceLink";
import { DEP_NAMES, projectName } from "./layout";
import { CLASS_BY, type Def, METHOD_BY, RF_BY, STATUS_BY } from "./model";
import s from "./DefectMap.module.css";

/** FIXES_URL is the public list of Ghostty bugs Rustty fixes instead of porting, at the snapshot commit. */
const FIXES_URL =
  "https://github.com/AirelyDev/rustty/blob/58aa62a81e7db5f8762b0158acc661dafc31f230/docs/ghostty-bug-fixes.md";

/** MAP_COMMIT is the Ghostty reference commit the map's files are measured at. */
const MAP_COMMIT = "246f70287";

/** The four detail texts of a defect, or null while the detail file loads. */
export type DetailTexts = {
  mech: string;
  rfast: string;
  fix: string;
  us: string;
} | null;

/** Props for the detail panel. */
type DetailProps = {
  def: Def | null;
  texts: DetailTexts;
  /** Whether loading the detail texts failed. */
  failed: boolean;
  /** The tmux leaf's defects and Zig lines, for the hint. */
  tmux: { n: number; lines: number };
};

/**
 * The detail panel: the selected defect's title and tags (class, discovery
 * method, severity, project, upstream status, public fixes list, ReleaseFast
 * consequence), then where it is, the mechanism, what ReleaseFast does, what
 * Rustty does instead, the upstream note and the Rustty commit. With nothing
 * selected it shows a hint.
 */
export default function Detail({ def, texts, failed, tmux }: DetailProps) {
  if (!def) {
    return (
      <div className={s.detail}>
        <div className={s.hintHead}>Click a mark</div>
        <p className={s.hint}>
          Each mark opens its ledger entry: what Ghostty does wrong, what that
          becomes in a ReleaseFast build, what Rustty does instead, the upstream
          status and the Rustty commit. Try the dense{" "}
          <SourceLink at={MAP_COMMIT} path="src/terminal/tmux" tree /> tile (
          {tmux.n} entries in {fmtInt(tmux.lines)} lines) or{" "}
          <SourceLink at={MAP_COMMIT} path="src/termio/Exec.zig" />.
        </p>
      </div>
    );
  }
  const c = CLASS_BY[def.c];
  const colour = PALETTE[c.role];
  const st = STATUS_BY[def.s];
  const rf = def.rf ? RF_BY[def.rf] : null;
  const where = def.f || DEP_NAMES[def.leaf] || def.leaf;
  return (
    <div className={s.detail} aria-live="polite">
      <h5>
        <span
          style={{ color: "hollow" in c ? PALETTE.gray6 : colour }}
          aria-hidden="true"
        >
          ●
        </span>{" "}
        {def.t}
      </h5>
      <div className={s.meta}>
        <span className={s.tag} style={{ borderColor: colour, color: colour }}>
          {c.name}
        </span>
        <span className={s.tag}>
          found by {METHOD_BY[def.m].name}
          {def.md ? ` · ${def.md}` : ""} (estimated)
        </span>
        <span className={s.tag}>severity {def.sev}</span>
        <span className={s.tag}>{projectName(def.p)}</span>
        <span
          className={s.tag}
          style={{
            borderColor: PALETTE[st.role],
            color: def.s === "nr" ? PALETTE.gray5 : PALETTE[st.role],
          }}
        >
          {st.name}
        </span>
        {def.doc ? (
          <a className={s.tag} href={FIXES_URL}>
            in the public fixes list
          </a>
        ) : null}
        {rf ? (
          <span
            className={s.tag}
            style={{ borderColor: PALETTE[rf.role], color: PALETTE[rf.role] }}
          >
            ReleaseFast: {rf.name} (estimated)
          </span>
        ) : null}
      </div>
      <dl className={s.grid}>
        <dt>where</dt>
        <dd className={s.mono}>
          {def.p === "ghostty" && def.f ? (
            <SourceLink at={MAP_COMMIT} path={def.f} />
          ) : (
            where
          )}
        </dd>
        <dt>mechanism</dt>
        <dd>
          {texts ? (
            <CommitText text={texts.mech} />
          ) : failed ? (
            "could not load details"
          ) : (
            "loading…"
          )}
        </dd>
        {texts?.rfast ? (
          <>
            <dt>ReleaseFast</dt>
            <dd>
              <CommitText text={texts.rfast} />
            </dd>
          </>
        ) : null}
        {texts?.fix ? (
          <>
            <dt>Rustty instead</dt>
            <dd>
              <CommitText text={texts.fix} />
            </dd>
          </>
        ) : null}
        {texts?.us && def.s !== "nr" ? (
          <>
            <dt>upstream</dt>
            <dd>
              <CommitText text={texts.us} />
            </dd>
          </>
        ) : null}
        <dt>Rustty commit</dt>
        <dd className={s.mono}>
          <CommitLink hash={def.k} /> · {def.dt}
        </dd>
      </dl>
    </div>
  );
}
