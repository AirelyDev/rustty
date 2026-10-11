import { fmt, TOOL_ABBR, type VmData } from "./model";
import s from "./VerificationMatrix.module.css";

/**
 * The reading note under the matrix: what "port" and "Ghostty" count, the
 * headline sum written out lane by lane with each lane's reference commit,
 * and the state of the current-pin lanes if any is still pending.
 */
export default function Foot({ data }: { data: VmData }) {
  const parts = data.hero.lanes
    .map(
      (r) =>
        `${TOOL_ABBR[r.tool] ?? "Valgrind"}${["lsan", "tsan"].includes(r.tool) ? ` (${r.host === "mac" ? "macOS" : "Linux VM"})` : ""} ${fmt(r.tests)} at ${r.pin}`,
    )
    .join(" + ");
  const pending = data.runs.some(
    (r) => r.pin === data.pins.length - 1 && r.st === "pending",
  );
  return (
    <p className={s.foot}>
      Found counts: "port" = defects in Rustty's own code, "Ghostty" = recorded
      Ghostty or dependency defects; a test-only flake fixed by a run counts as
      neither. {fmt(data.hero.runs)} = {parts}.
      {pending
        ? " Cells marked pending have no recorded result yet; they are not results."
        : ""}
    </p>
  );
}
