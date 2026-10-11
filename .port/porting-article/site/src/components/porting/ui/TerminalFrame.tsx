import type { ReactNode } from "react";
import s from "./ui.module.css";

/** Renders a terminal-window frame: three window dots, a title (a real command or path) and content. */
export default function TerminalFrame({
  title,
  children,
}: {
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className={s.term}>
      <div className={s.termBar}>
        <i className={s.termDot} aria-hidden="true" />
        <i className={s.termDot} aria-hidden="true" />
        <i className={s.termDot} aria-hidden="true" />
        <span className={s.termTitle} title={title}>
          {title}
        </span>
      </div>
      <div className={s.termBody}>{children}</div>
    </div>
  );
}
