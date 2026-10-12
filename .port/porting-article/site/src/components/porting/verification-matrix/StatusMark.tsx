import type { CSSProperties } from "react";
import { STATUS_INFO, type Status } from "./model";
import s from "./VerificationMatrix.module.css";

/**
 * A small status marker drawn with its own shape as well as its colour: a
 * disc for green, a diamond for found-then-fixed, a triangle for did not
 * complete and a dashed hollow ring for pending.
 */
export default function StatusMark({ st }: { st: Status }) {
  const info = STATUS_INFO[st];
  return (
    <i
      aria-hidden="true"
      className={`${s.mark} ${s[`mark_${info.shape}`]}`}
      style={{ "--mark": info.color } as CSSProperties}
    />
  );
}
