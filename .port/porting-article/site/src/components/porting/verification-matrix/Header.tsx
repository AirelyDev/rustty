import CommitLink from "../ui/CommitLink";
import { dur, fmt, heroByPin, TOOL_ABBR, type VmData } from "./model";
import s from "./VerificationMatrix.module.css";

/** Returns a lane's short name: the sanitizer abbreviation (or Valgrind) and its host. */
function laneName(tool: string, host: string): string {
  return `${TOOL_ABBR[tool] ?? "Valgrind"} ${host === "mac" ? "macOS" : "Linux"}`;
}

/**
 * The widget header: the headline count of test runs under a sanitizer or
 * Valgrind, each lane at the latest reference commit it completed at (named
 * per commit), then three tiles (the Miri Stacked Borrows sweep of the first
 * complete matrix, the Miri speed-up, the Ghostty-side defects found).
 */
export default function Header({ data }: { data: VmData }) {
  const h = data.hero;
  const first = data.pins.findIndex((p) => p.sha === h.miriRef);
  const sb = data.runs.find((r) => r.pin === first && r.tool === "sb");
  return (
    <div className={s.hero}>
      <span>
        <span className={s.big}>{fmt(h.runs)}</span>
        <span className={s.hs}>
          test runs under a sanitizer or Valgrind, each lane at the latest
          reference commit it completed at:{" "}
          {heroByPin(data).map(([pin, lanes], i) => (
            <span key={pin}>
              {i ? "; " : ""}
              {lanes.map((l) => laneName(l.tool, l.host)).join(", ")} at{" "}
              <span className={s.mono}>
                <CommitLink hash={pin} />
              </span>
            </span>
          ))}
        </span>
      </span>
      <span className={s.hs}>
        <b className={s.mono}>{fmt(h.miriSecs)} s</b>
        Miri Stacked Borrows over {h.crates} crates, green at{" "}
        <span className={s.mono}>
          <CommitLink hash={h.miriRef} />
        </span>
        {sb ? ` (${sb.date}, ${dur(sb.secs)})` : ""}
      </span>
      <span className={s.hs}>
        <b className={s.mono}>{h.speedup}×</b>
        faster Miri with one process per test ({fmt(h.before)} s →{" "}
        {fmt(h.after)} s)
      </span>
      <span className={s.hs}>
        <b className={s.mono}>{h.ghostty}</b>
        Ghostty-side defects found by sanitizers (Valgrind, LeakSanitizer,
        ThreadSanitizer); port bugs below
      </span>
    </div>
  );
}
