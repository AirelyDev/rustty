/**
 * Link guard for the porting article: every fact carries a source link, and
 * prose names commits and source files only through CommitLink and
 * SourceLink. Pure functions over the article text and generated data, so
 * scripts/verify-porting-guard.mjs can test them on small fixtures.
 */

/**
 * HASH_ALLOWLIST holds the commit-like hashes that resolve in no repository
 * and stay plain code: an iTerm2-Color-Schemes release tag and a uucode
 * version hash quoted in commit subjects, and a tmux master hash cited in a
 * defect entry. The two memory-only quotes of the link map stay plain quotes
 * and need no allowance here.
 */
export const HASH_ALLOWLIST = new Set(["97e244c", "2826a37a", "72e83b00"]);

/** SOURCE_EXTS are the file extensions a backticked prose path is checked for. */
const SOURCE_EXT = "zig|rs|swift|h|c|sh|py|toml|md";

/** CODE_SPAN_RE finds inline code spans. */
const CODE_SPAN_RE = /`([^`\n]+)`/g;

/** HEX_RE matches a whole 7–40-character lowercase hex token. */
const HEX_RE = /^[0-9a-f]{7,40}$/;

/**
 * PATH_RE matches a whole source path or file name with a checked extension,
 * optionally followed by a line suffix (":12", ":12-40", ":12:5", "#L12",
 * "#L12-L40").
 */
const PATH_RE = new RegExp(`^[\\w./-]*\\w\\.(?:${SOURCE_EXT})(?::\\d+(?:[-:]\\d+)?|#L\\d+(?:-L?\\d+)?)?$`);

/** Returns the prose of an MDX document: fenced code blocks, markdown links and JSX tags removed. */
export function linkableProse(mdx) {
  return mdx
    .replace(/^```[\s\S]*?^```[^\n]*$/gm, "")
    .replace(/\[(?:[^\]\\]|\\.)*\]\([^)\s]*\)/g, "")
    .replace(/<[A-Za-z][^>]*>/g, "");
}

/**
 * Returns the guard failures for an article: facts without an href, prose
 * code spans that hold a commit hash or a source path outside the link
 * components, and CommitLink/SourceLink commits the commit map cannot resolve.
 * facts maps id → {href}; commits is the commits.json map.
 */
export function linkFailures({ mdx, facts, commits }) {
  const failures = [];
  for (const [id, f] of Object.entries(facts)) {
    if (!f.href) failures.push(`fact ${id} has no href`);
  }
  for (const [, id] of mdx.matchAll(/<Fact id="([a-z0-9_]+)"/g)) {
    if (id in facts && !facts[id].href) failures.push(`<Fact id="${id}" /> has no href`);
  }
  for (const [, code] of linkableProse(mdx).matchAll(CODE_SPAN_RE)) {
    if (HEX_RE.test(code) && !HASH_ALLOWLIST.has(code)) {
      failures.push(`prose has a bare commit hash \`${code}\`; use <CommitLink hash="${code}" />`);
    }
    if (PATH_RE.test(code)) {
      failures.push(`prose has a bare source path \`${code}\`; use <SourceLink />`);
    }
  }
  const shas = Object.values(commits).map((v) => v.slice(v.lastIndexOf(":") + 1));
  const known = (h) => h in commits || shas.some((s) => s.length === 40 && s.startsWith(h));
  for (const [, h] of mdx.matchAll(/<CommitLink hash="([0-9a-f]+)"/g)) {
    if (!known(h) && !HASH_ALLOWLIST.has(h)) failures.push(`<CommitLink hash="${h}" /> resolves in no repository`);
  }
  for (const [, h] of mdx.matchAll(/<SourceLink at="([0-9a-f]+)"/g)) {
    if (!known(h)) failures.push(`<SourceLink at="${h}" /> resolves in no repository`);
  }
  return failures;
}
