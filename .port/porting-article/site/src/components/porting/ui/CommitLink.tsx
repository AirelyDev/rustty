import commits from "@/app/porting/data/commits.json";
import { commitUrl, type RepoName, resolveCommit } from "../lib/links";

/** COMMITS maps each linkable commit token to "<repo>:<sha>" (generated from the research link map). */
const COMMITS = commits as Readonly<Record<string, string>>;

/**
 * Renders a commit hash (9 characters shown) linking to its commit page by
 * full sha, in the repository the commit map resolves it to (Rustty, Ghostty
 * or a third-party project). repo narrows the lookup, or names the repository
 * of a full sha missing from the map. With text, it links that phrase instead
 * of showing the hash, so prose can link "the first commit" or "the fix"
 * without printing a sha; the hash is kept where the sentence is about the
 * commit itself (a baseline, a reference). A hash that resolves nowhere renders
 * as plain code (or as plain text with text);
 * scripts/verify-porting-registry.mjs fails the prose check on any such hash
 * outside its allowlist.
 */
export default function CommitLink({
  hash,
  repo,
  text,
}: {
  hash: string;
  repo?: RepoName;
  text?: string;
}) {
  const c = resolveCommit(COMMITS, hash, repo);
  if (text) return c ? <a href={commitUrl(c)}>{text}</a> : <span>{text}</span>;
  const label = hash.slice(0, 9);
  if (!c) return <code className="porting-commit">{label}</code>;
  return (
    <a href={commitUrl(c)} className="porting-commit">
      {label}
    </a>
  );
}
