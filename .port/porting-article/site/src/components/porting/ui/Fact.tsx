import facts from "@/app/porting/data/facts.json";

/** One generated fact as stored in facts.json. */
type FactRecord = {
  display: string;
  value: number | string;
  provenance: string;
  source: string;
  asOf: string;
  href: string;
  hrefLabel: string;
};

/** FACTS indexes generated facts by id. */
const FACTS = facts as unknown as Record<string, FactRecord>;

/** LONG_FACT_CHARS is the display length above which a fact may wrap (phone widths). */
const LONG_FACT_CHARS = 24;

/**
 * Renders a fact-checked number from facts.json as a link to its source (same
 * tab), titled with the source label, provenance and as-of value. Unknown ids
 * and facts without a link fail the static build.
 */
export default function Fact({ id }: { id: string }) {
  const f = FACTS[id];
  if (!f) throw new Error(`unknown fact id: ${id}`);
  if (!f.href) throw new Error(`fact ${id} has no source link`);
  const long = f.display.length > LONG_FACT_CHARS;
  return (
    <a
      href={f.href}
      className={long ? "porting-fact porting-fact-long" : "porting-fact"}
      title={`${f.hrefLabel} · ${f.provenance.toLowerCase()} · as of ${f.asOf}`}
    >
      {f.display}
    </a>
  );
}
