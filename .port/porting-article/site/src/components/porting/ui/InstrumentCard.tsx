import { Fragment, type CSSProperties, type ReactNode } from "react";
import { sourceSegments } from "../lib/source-links";
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
  /** Data sources, shown in the outlined copper pill (parts joined with " · "). */
  source: string;
  /**
   * Links for parts of source, keyed by the exact text to link (a project, a
   * tool, a commit). Each links on its own; unlisted text stays plain.
   */
  sourceLinks?: Readonly<Record<string, string>>;
  /**
   * Permalink to the research dataset or primary record behind the figure,
   * shown as its own link after the label unless a part of source already
   * links to it.
   */
  sourceHref?: string;
  /** Text of the record link; defaults to "record" for a Markdown file, else "data". */
  sourceHrefLabel?: string;
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

/** Returns the default text of the record link: "record" for a Markdown file, else "data". */
function recordLabel(href: string): string {
  return /\.md(?:#.*)?$/.test(href) ? "record" : "data";
}

/** Renders the instrument-card chrome: source pill, label, record link, optional headline, body and caption. */
export default function InstrumentCard({
  label,
  source,
  sourceLinks,
  sourceHref,
  sourceHrefLabel,
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
        <span className={s.pill}>
          {sourceSegments(source, sourceLinks).map((seg, i) =>
            seg.href ? (
              <a
                // biome-ignore lint/suspicious/noArrayIndexKey: segments are static and never reorder
                key={i}
                className={s.pillLink}
                href={seg.href}
              >
                {seg.text}
              </a>
            ) : (
              // biome-ignore lint/suspicious/noArrayIndexKey: segments are static and never reorder
              <Fragment key={i}>{seg.text}</Fragment>
            ),
          )}
        </span>
        <span className={s.label}>{label}</span>
        {sourceHref &&
        !Object.values(sourceLinks ?? {}).includes(sourceHref) ? (
          <a className={s.record} href={sourceHref}>
            {sourceHrefLabel ?? recordLabel(sourceHref)}
            <span aria-hidden="true"> ↗</span>
          </a>
        ) : null}
      </div>
      {headline ? <div className={s.headline}>{headline}</div> : null}
      <div className={s.body}>{children}</div>
      <p className={s.caption}>{caption}</p>
    </div>
  );
}
