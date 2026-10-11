import commits from "@/app/porting/data/commits.json";
import { resolveCommit, type RepoName, sourceUrl } from "../lib/links";

/** COMMITS maps each linkable commit token to "<repo>:<sha>" (generated from the research link map). */
const COMMITS = commits as Readonly<Record<string, string>>;

/**
 * Renders a source path in code style, linking to its GitHub permalink at a
 * full sha: `at` is a commit token from the commit map (or a full sha with
 * repo), `path` the repository path, `lines` an optional "12" or "574-610"
 * range, and `tree` links a directory. `text` replaces the shown path (a bare
 * file name or a crate name). An unresolvable commit fails the static build.
 */
export default function SourceLink({
  at,
  path,
  lines,
  text,
  repo,
  tree = false,
}: {
  at: string;
  path: string;
  lines?: string;
  text?: string;
  repo?: RepoName;
  tree?: boolean;
}) {
  const c = resolveCommit(COMMITS, at, repo);
  if (!c) throw new Error(`SourceLink: unknown commit ${at} for ${path}`);
  return (
    <a href={sourceUrl(c, path, lines, tree)} className="porting-source">
      <code>{text ?? path}</code>
    </a>
  );
}
