/**
 * GitHub permalink helpers for the porting article: commit pages and blob or
 * tree permalinks at a full sha. Commit tokens resolve through commits.json,
 * which scripts/porting/build_data.py generates from the research link map.
 * Pure functions, so scripts/verify-porting-helpers.mjs can run them under
 * Node's type stripping.
 */

/** Repository owner/name for each short code used in commits.json. */
export const REPO_PATHS: Readonly<Record<string, string>> = {
  r: "AirelyDev/rustty",
  g: "ghostty-org/ghostty",
};

/** Repository names a caller may pass instead of a commits.json code. */
export type RepoName = "rustty" | "ghostty";

/** Maps a RepoName to its commits.json code. */
const REPO_CODE: Readonly<Record<RepoName, string>> = {
  rustty: "r",
  ghostty: "g",
};

/** A commit resolved to its repository and sha. */
export type ResolvedCommit = {
  /** owner/name of the GitHub repository. */
  repo: string;
  /** The sha as the link map gives it (40 characters for Rustty and Ghostty). */
  sha: string;
};

/** FULL_SHA_RE matches a full 40-character lowercase hex sha. */
const FULL_SHA_RE = /^[0-9a-f]{40}$/;

/** Splits one commits.json value ("r:<sha>" or "owner/name:<sha>") into repo and sha. */
function parseEntry(entry: string): ResolvedCommit {
  const i = entry.lastIndexOf(":");
  const code = entry.slice(0, i);
  return { repo: REPO_PATHS[code] ?? code, sha: entry.slice(i + 1) };
}

/**
 * Resolves a commit token against the commit map: an exact key first, then
 * any key whose full sha starts with the token (for shorter or longer
 * abbreviations of the same commit). A full sha with an explicit repo needs no
 * map entry. Returns null when the token resolves nowhere.
 */
export function resolveCommit(
  commits: Readonly<Record<string, string>>,
  hash: string,
  repo?: RepoName,
): ResolvedCommit | null {
  const want = repo ? REPO_PATHS[REPO_CODE[repo]] : null;
  const exact = commits[hash];
  if (exact) {
    const c = parseEntry(exact);
    if (!want || c.repo === want) return c;
  }
  if (hash.length >= 7) {
    for (const entry of Object.values(commits)) {
      const c = parseEntry(entry);
      if (c.sha.startsWith(hash) && c.sha.length === 40) {
        if (!want || c.repo === want) return c;
      }
    }
  }
  if (repo && FULL_SHA_RE.test(hash)) {
    return { repo: REPO_PATHS[REPO_CODE[repo]], sha: hash };
  }
  return null;
}

/** Returns the GitHub commit page URL of a resolved commit. */
export function commitUrl(c: ResolvedCommit): string {
  return `https://github.com/${c.repo}/commit/${c.sha}`;
}

/**
 * Returns a GitHub permalink to a file (blob) or directory (tree) at a
 * resolved commit. lines is "12" or "574-610" and becomes "#L12" or
 * "#L574-L610"; it is ignored for directories.
 */
export function sourceUrl(
  c: ResolvedCommit,
  path: string,
  lines?: string,
  tree = false,
): string {
  const clean = path.replace(/^\/+|\/+$/g, "");
  const base = `https://github.com/${c.repo}/${tree ? "tree" : "blob"}/${c.sha}/${clean}`;
  if (tree || !lines) return base;
  const [a, b] = lines.split("-");
  return b ? `${base}#L${a}-L${b}` : `${base}#L${a}`;
}

/** HEX_TOKEN_RE finds 7–40-character lowercase hex tokens on word boundaries. */
export const HEX_TOKEN_RE = /\b[0-9a-f]{7,40}\b/g;

/** One piece of a split text: plain text, or a commit token with its resolution. */
export type TextPiece =
  | { kind: "text"; text: string }
  | { kind: "commit"; text: string; commit: ResolvedCommit };

/**
 * Splits text into plain runs and commit tokens that resolve in the commit
 * map. Hex-looking words that resolve nowhere stay plain text, so colour
 * literals, numbers and unresolvable hashes are never linked.
 */
export function splitCommits(
  commits: Readonly<Record<string, string>>,
  text: string,
): TextPiece[] {
  const out: TextPiece[] = [];
  let last = 0;
  for (const m of text.matchAll(HEX_TOKEN_RE)) {
    const entry = commits[m[0]];
    if (!entry) continue;
    const at = m.index ?? 0;
    if (at > last) out.push({ kind: "text", text: text.slice(last, at) });
    out.push({ kind: "commit", text: m[0], commit: parseEntry(entry) });
    last = at + m[0].length;
  }
  if (last < text.length) out.push({ kind: "text", text: text.slice(last) });
  return out;
}
