import facts from "@/app/porting/data/facts.json";
import data from "@/app/porting/data/stats-card.json";
import Tooltip from "../ui/Tooltip";
import { TipBody } from "./Hot";
import {
  type FactRecord,
  type NotePart,
  type ResolvedFact,
  resolveTiles,
  type TileSpec,
} from "./model";
import s from "./StaticCharts.module.css";

/** PROVENANCE_WORDS turns a fact's provenance code into plain words. */
const PROVENANCE_WORDS: Record<string, string> = {
  MEASURED: "measured",
  DOCUMENTED: "documented in the port's records",
  ESTIMATED: "estimated",
};

/** One tooltip line naming where a fact comes from and as of when; the value links to its source. */
function FactLine({ f, what }: { f: ResolvedFact; what: string }) {
  return (
    <>
      <b>
        {f.href ? (
          <a href={f.href} className="porting-fact" title={f.hrefLabel}>
            {f.display}
          </a>
        ) : (
          f.display
        )}
      </b>{" "}
      {what}: {PROVENANCE_WORDS[f.provenance] ?? "recorded"}, as of {f.asOf}
    </>
  );
}

/** A tile note, its fact placeholders rendered as linked fact displays; a fact without a link fails the build. */
function Note({ parts }: { parts: NotePart[] }) {
  return (
    <>
      {parts.map((p) => {
        if (typeof p === "string") return p;
        if (!p.href)
          throw new Error(`stats-card note: fact ${p.id} has no source link`);
        return (
          <a
            key={p.id}
            href={p.href}
            className="porting-fact"
            title={p.hrefLabel}
          >
            {p.display}
          </a>
        );
      })}
    </>
  );
}

/**
 * The port in numbers: a grid of tiles, every value read from the article's
 * fact table by id. Each tile is a tooltip trigger naming each figure's
 * provenance and "as of" commit or date.
 */
export default function StatsCard() {
  const tiles = resolveTiles(
    data.tiles as TileSpec[],
    facts as unknown as Record<string, FactRecord>,
  );
  return (
    <ul className={s.tiles}>
      {tiles.map((t) => (
        <li key={t.key} className={s.tile}>
          <Tooltip
            id={`stats-${t.key}`}
            content={
              <TipBody
                title={t.label}
                rows={[
                  <FactLine key="m" f={t.main} what="value" />,
                  ...(t.compare
                    ? [
                        <FactLine
                          key="c"
                          f={t.compare}
                          what={t.compare.label}
                        />,
                      ]
                    : []),
                  ...(t.extra
                    ? [<FactLine key="e" f={t.extra} what={t.extra.label} />]
                    : []),
                  ...(t.note ? [<Note key="n" parts={t.note} />] : []),
                ]}
              />
            }
          >
            <span className={s.tileValue}>{t.value}</span>
            <span className={s.tileLabel}>{t.label}</span>
            {t.compare ? (
              <span className={s.tileSub}>
                vs <b>{t.compare.display}</b> {t.compare.label}
              </span>
            ) : null}
            {t.extra ? (
              <span className={s.tileSub}>
                <b>{t.extra.display}</b> {t.extra.label}
              </span>
            ) : null}
          </Tooltip>
        </li>
      ))}
    </ul>
  );
}
