import s from "./PerfReplay.module.css";
import type { Tip } from "./rows";

/** Renders a tooltip: bold head, plain lines and a muted footer. */
export default function TipContent({ tip }: { tip: Tip }) {
  return (
    <>
      <b>{tip.head}</b>
      {tip.lines.map((l) => (
        <div key={l}>{l}</div>
      ))}
      {tip.foot ? <div className={s.tipFoot}>{tip.foot}</div> : null}
    </>
  );
}
