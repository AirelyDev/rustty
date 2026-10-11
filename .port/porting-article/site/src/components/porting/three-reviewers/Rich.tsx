import type { ReactNode } from "react";
import CommitLink from "../ui/CommitLink";
import { linkParts, mdParts, type Part } from "./model";

/** Returns the text length a part covers (keys parts by their character offset in the string). */
function partLength(p: Part): number {
  return "code" in p
    ? p.code.length + 2
    : "hash" in p
      ? p.hash.length
      : p.text.length;
}

/** Renders plain text, turning commit hashes into links to the public repository. */
function linked(text: string, base: number): ReactNode[] {
  let at = base;
  return linkParts(text).map((q) => {
    const key = `l${at}`;
    at += partLength(q);
    return "hash" in q ? (
      <CommitLink key={key} hash={q.hash} />
    ) : (
      <span key={key}>{"text" in q ? q.text : ""}</span>
    );
  });
}

/**
 * Renders record text: `backticks` become inline code and, with links, 9-character
 * commit hashes in the plain parts become links to the public repository.
 */
export default function Rich({
  text,
  links = false,
}: {
  text: string;
  links?: boolean;
}) {
  let at = 0;
  return (
    <>
      {mdParts(text).map((p) => {
        const key = `p${at}`;
        const start = at;
        at += partLength(p);
        if ("code" in p) return <code key={key}>{p.code}</code>;
        const plain = "text" in p ? p.text : "";
        return <span key={key}>{links ? linked(plain, start) : plain}</span>;
      })}
    </>
  );
}
