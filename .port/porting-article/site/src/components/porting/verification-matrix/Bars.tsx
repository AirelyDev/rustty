import { PALETTE } from "../lib/palette";
import {
  durationRows,
  dur,
  durFraction,
  foundByTool,
  hostName,
  type VmData,
} from "./model";
import s from "./VerificationMatrix.module.css";

/**
 * Wall time of the last green full run per lane, in hours: one bar per lane,
 * copper for macOS and blue for the Linux VM (the host is also named in each
 * label).
 */
export function Durations({ data }: { data: VmData }) {
  return (
    <>
      <div className={s.bars}>
        {durationRows(data).map((d) => (
          <div key={`${d.name}|${d.host}`} className={s.bar}>
            <span className={s.barName}>
              {d.name} <span className={s.k}>· {hostName(data, d.host)}</span>
            </span>
            <span className={s.barTrack}>
              <span
                className={s.barFill}
                style={{
                  width: `${Math.max(1, durFraction(data, d.secs) * 100)}%`,
                  background: d.host === "mac" ? PALETTE.rust : PALETTE.blue,
                }}
              />
            </span>
            <span className={`${s.mono} ${s.barVal}`}>
              {(d.secs / 3600).toFixed(1)} h
            </span>
          </div>
        ))}
      </div>
      <p className={s.barKey}>
        <span style={{ color: PALETTE.rust }}>■ macOS</span>{" "}
        <span style={{ color: PALETTE.blue }}>■ Linux VM</span>
        <span className={s.k}>
          {" "}
          · {dur(durationRows(data)[0].secs)} is the longest
        </span>
      </p>
    </>
  );
}

/**
 * What the lanes found: a stacked bar per tool of defects in Rustty's own code
 * (red) and in Ghostty or its dependencies (teal), summed over the recorded
 * runs, with the counts written out ("5 + 1").
 */
export function Found({ data }: { data: VmData }) {
  const rows = foundByTool(data);
  const max = Math.max(...rows.map((r) => r.port + r.gh));
  return (
    <>
      <div className={s.bars}>
        {rows.map((r) => (
          <div key={r.key} className={s.bar}>
            <span className={s.barName}>{r.name}</span>
            <span className={s.barTrack}>
              <span
                className={s.barFill}
                style={{
                  width: `${(r.port / max) * 100}%`,
                  background: PALETTE.fail,
                }}
              />
              <span
                className={s.barFill}
                style={{
                  width: `${(r.gh / max) * 100}%`,
                  background: PALETTE.upstream,
                }}
              />
            </span>
            <span className={`${s.mono} ${s.barVal}`}>
              {r.port}
              {r.gh ? ` + ${r.gh}` : ""}
            </span>
          </div>
        ))}
      </div>
      <p className={s.barKey}>
        <span style={{ color: PALETTE.fail }}>■ port defects</span>{" "}
        <span style={{ color: PALETTE.upstream }}>■ Ghostty-side</span>
      </p>
      <p className={s.barNote}>{data.foundNote}</p>
    </>
  );
}
