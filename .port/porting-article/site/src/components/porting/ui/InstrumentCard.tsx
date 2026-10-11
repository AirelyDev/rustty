import type { CSSProperties, ReactNode } from "react";
import s from "./ui.module.css";

/** Reserved card heights in px at 1100 px (desktop) and 390 px (phone). */
export type CardMinHeight = {
  desktop: number;
  phone: number;
  reduced?: { desktop: number; phone: number };
};

/** Props for InstrumentCard. */
type InstrumentCardProps = {
  /** Uppercase tracked label in the header. */
  label: string;
  /** Data source, shown in the outlined copper pill. */
  source: string;
  /** Permalink the source pill opens (the research dataset or primary record). */
  sourceHref?: string;
  /** Optional mono headline figure under the header. */
  headline?: ReactNode;
  /** Footer caption: what is plotted and where it comes from. */
  caption: ReactNode;
  /**
   * When true the card breaks out of the prose column (porting-wide). Inside
   * another porting-wide block (a PortingWidget section) the nested rule in
   * PortingPage.module.css neutralizes the breakout so margins apply once.
   */
  wide?: boolean;
  /**
   * Reserved card height per layout, so lazy bodies do not shift the page.
   * `reduced` applies under prefers-reduced-motion, where replays mount at
   * their (possibly taller) final state.
   */
  minHeight?: CardMinHeight;
  /** Card body. */
  children?: ReactNode;
};

/** Renders the instrument-card chrome: source pill, label, optional headline, body and caption. */
export default function InstrumentCard({
  label,
  source,
  sourceHref,
  headline,
  caption,
  wide = true,
  minHeight,
  children,
}: InstrumentCardProps) {
  const style = minHeight
    ? ({
        "--card-min-desktop": `${minHeight.desktop}px`,
        "--card-min-phone": `${minHeight.phone}px`,
        ...(minHeight.reduced
          ? {
              "--card-min-desktop-reduced": `${minHeight.reduced.desktop}px`,
              "--card-min-phone-reduced": `${minHeight.reduced.phone}px`,
            }
          : {}),
      } as CSSProperties)
    : undefined;
  return (
    <div className={wide ? `${s.card} porting-wide` : s.card} style={style}>
      <div className={s.header}>
        {sourceHref ? (
          <a className={`${s.pill} ${s.pillLink}`} href={sourceHref}>
            {source}
          </a>
        ) : (
          <span className={s.pill}>{source}</span>
        )}
        <span className={s.label}>{label}</span>
      </div>
      {headline ? <div className={s.headline}>{headline}</div> : null}
      <div className={s.body}>{children}</div>
      <p className={s.caption}>{caption}</p>
    </div>
  );
}
