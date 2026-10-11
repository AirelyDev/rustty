import { runHeading, type VmData } from "./model";
import s from "./VerificationMatrix.module.css";

/** The tooltip body for a run id `r<index>`: tool, host, reference commit, date and the record sentence. */
export default function TipContent({ id, data }: { id: string; data: VmData }) {
  const r = data.runs[Number(id.slice(1))];
  if (!r) return null;
  return (
    <>
      <b>{runHeading(data, r)}</b> ·{" "}
      <span className={s.mono}>
        {data.pins[r.pin].sha} · {r.date}
      </span>
      <br />
      {r.txt}
    </>
  );
}
