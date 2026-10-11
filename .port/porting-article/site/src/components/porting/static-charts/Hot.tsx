import type { ReactNode } from "react";
import Tooltip from "../ui/Tooltip";
import s from "./StaticCharts.module.css";

/**
 * A transparent hover, tap and keyboard target laid over an SVG mark. The
 * box is given in the SVG's viewBox units (x, y, w, h inside a W × H
 * viewBox) and placed in percent, so it follows the SVG when it scales.
 * label is the target's accessible name; children are the tooltip body.
 */
export default function Hot({
  id,
  x,
  y,
  w,
  h,
  W,
  H,
  label,
  children,
}: {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  W: number;
  H: number;
  label: string;
  children: ReactNode;
}) {
  /** Converts a viewBox length to a percentage of the matching viewBox side. */
  const pct = (v: number, of: number) => `${(v / of) * 100}%`;
  return (
    <span
      className={s.hot}
      style={{
        left: pct(x, W),
        top: pct(y, H),
        width: pct(w, W),
        height: pct(h, H),
      }}
    >
      <Tooltip id={id} content={children}>
        <span className={s.srOnly}>{label}</span>
      </Tooltip>
    </span>
  );
}

/** A tooltip body: a bold title line, then one line per row. */
export function TipBody({
  title,
  rows,
}: {
  title: ReactNode;
  rows: ReactNode[];
}) {
  return (
    <>
      <span className={s.tipTitle}>{title}</span>
      {rows.map((r, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static rows in a fixed order
        <span key={i} className={s.tipRow}>
          {r}
        </span>
      ))}
    </>
  );
}
