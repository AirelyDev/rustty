import Tooltip from "../ui/Tooltip";
import s from "./EveryPrompt.module.css";
import { splitGlossed } from "./log-model";

/**
 * Renders prompt text as plain text nodes, with each internal id from the
 * glossary (UD131, R7, …) kept in its own spelling as a focusable term whose
 * tooltip gives the plain words. Nothing in the text is interpreted as markup.
 */
export default function Glossed({
  text,
  glossary,
  rowId,
  boundary,
}: {
  text: string;
  glossary: Record<string, string>;
  /** The prompt's id, to keep tooltip ids unique. */
  rowId: string;
  /** The element the term tooltips stay inside (see Tooltip). */
  boundary?: () => HTMLElement | null;
}) {
  return (
    <>
      {splitGlossed(text, glossary).map((p, k) =>
        p.id ? (
          <Tooltip
            // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional and never reorder
            key={k}
            id={`${rowId}-${k}`}
            boundary={boundary}
            content={
              <>
                <b>{p.id}</b>: {glossary[p.id]}
              </>
            }
          >
            <span className={s.term}>{p.t}</span>
          </Tooltip>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional and never reorder
          <span key={k}>{p.t}</span>
        ),
      )}
    </>
  );
}
