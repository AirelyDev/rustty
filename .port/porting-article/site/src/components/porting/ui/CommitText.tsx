import commits from "@/app/porting/data/commits.json";
import { commitUrl, splitCommits } from "../lib/links";

/** COMMITS maps each linkable commit token to "<repo>:<sha>" (generated from the research link map). */
const COMMITS = commits as Readonly<Record<string, string>>;

/**
 * Renders text with every commit hash the commit map knows turned into a
 * commit link (commit subjects, defect texts). Unknown hex words stay text.
 */
export default function CommitText({ text }: { text: string }) {
  const pieces = splitCommits(COMMITS, text);
  if (pieces.length === 1 && pieces[0].kind === "text") return <>{text}</>;
  let at = 0;
  return (
    <>
      {pieces.map((p) => {
        const key = `c${at}`;
        at += p.text.length;
        return p.kind === "text" ? (
          <span key={key}>{p.text}</span>
        ) : (
          <a key={key} href={commitUrl(p.commit)} className="porting-commit">
            {p.text}
          </a>
        );
      })}
    </>
  );
}
