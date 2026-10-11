import facts from "@/app/porting/data/facts.json";
import { STATIC_CHARTS } from "../static-charts";
import { widget } from "../lib/registry";
import InstrumentCard from "./InstrumentCard";
import LazyMount from "./LazyMount";
import s from "./ui.module.css";
import WidgetMount from "./WidgetMount";

/** FACT_AS_OF indexes each generated fact's "as of" value (commit or date) by fact id. */
const FACT_AS_OF = facts as unknown as Record<string, { asOf: string }>;

/**
 * Renders one porting widget. The server-rendered shell (title, deck, text
 * summary and card chrome with caption) is in the static HTML; the interactive body
 * mounts lazily on the client. asOf names a fact whose "as of" value is
 * appended to the caption; unknown slugs or fact ids fail the static build.
 *
 * Terminal-frame widgets keep the instrument card outside; their bodies draw
 * TerminalFrame windows titled with the real command they show. Static
 * charts (STATIC_CHARTS) are server components rendered in place, so their
 * full content is in the static HTML.
 *
 * The deck (what the widget adds and how to read it) is the visible line
 * under the title; the summary, the widget's text alternative, follows it
 * visually hidden for screen readers. Without a deck the summary is shown.
 */
export default function PortingWidget({
  slug,
  asOf,
}: {
  slug: string;
  asOf?: string;
}) {
  const w = widget(slug);
  let asOfValue: string | null = null;
  if (asOf) {
    const fact = FACT_AS_OF[asOf];
    if (!fact) throw new Error(`unknown fact id for asOf: ${asOf}`);
    if (!fact.asOf) throw new Error(`fact ${asOf} has an empty asOf value`);
    asOfValue = fact.asOf;
  }
  const titleId = `porting-widget-${slug}`;
  const Static = STATIC_CHARTS[slug];
  return (
    <section
      data-porting-widget={slug}
      data-frame={w.frame}
      className={`porting-wide ${s.section}`}
      aria-labelledby={titleId}
    >
      <h3 id={titleId} className={s.title}>
        {w.title}
      </h3>
      {w.deck ? (
        <>
          <p className={s.deck}>{w.deck}</p>
          <p className={s.srOnly}>{w.summary}</p>
        </>
      ) : (
        <p className={s.deck}>{w.summary}</p>
      )}
      <InstrumentCard
        label={w.label}
        source={w.source}
        sourceHref={w.sourceHref}
        wide={false}
        minHeight={w.minHeight}
        caption={
          <>
            {w.caption}
            {asOfValue ? (
              <>
                {" "}
                <span className={s.asOf}>as of {asOfValue}</span>
              </>
            ) : null}
          </>
        }
      >
        {Static ? (
          <Static />
        ) : (
          <LazyMount>
            <WidgetMount slug={slug} />
          </LazyMount>
        )}
      </InstrumentCard>
    </section>
  );
}
