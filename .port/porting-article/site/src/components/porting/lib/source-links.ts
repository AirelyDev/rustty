/**
 * Splits a widget's source line (for example "git log · tokei") into plain and
 * linked segments, so each project, tool or commit it names can link to its own
 * page instead of the whole line pointing at one dataset. Pure, so
 * scripts/verify-porting-helpers.mjs can run it under Node's type stripping.
 */

/** One piece of the source line: plain text, or text that links to href. */
export type SourceSegment = { text: string; href?: string };

/**
 * WORD_CHAR is a letter, digit, hyphen or underscore: a link key must not start
 * or end inside a word, and a hyphenated name ("rustty-wterm") is one word, so
 * "wterm" does not match inside it.
 */
const WORD_CHAR = /[A-Za-z0-9_-]/;

/**
 * Returns the index of the first occurrence of key in text that starts and
 * ends on a word boundary, or -1. A key may itself contain spaces or
 * punctuation ("git log --all"); only its outer edges must sit on boundaries.
 */
function findKey(text: string, key: string): number {
  let from = 0;
  for (;;) {
    const at = text.indexOf(key, from);
    if (at < 0) return -1;
    const before = at > 0 ? text[at - 1] : "";
    const after = at + key.length < text.length ? text[at + key.length] : "";
    if (!WORD_CHAR.test(before) && !WORD_CHAR.test(after)) return at;
    from = at + 1;
  }
}

/**
 * Splits source at every key of links, in reading order, linking the text of
 * each key to its URL. A key that is missing from source, or that overlaps an
 * earlier key, throws, so a typo in the registry fails the build instead of
 * silently dropping a link.
 */
export function sourceSegments(
  source: string,
  links: Readonly<Record<string, string>> = {},
): SourceSegment[] {
  const found = Object.entries(links)
    .map(([key, href]) => ({ key, href, at: findKey(source, key) }))
    .sort((a, b) => a.at - b.at);
  const segments: SourceSegment[] = [];
  let pos = 0;
  for (const { key, href, at } of found) {
    if (at < 0)
      throw new Error(`source link key not found in source: "${key}"`);
    if (at < pos) throw new Error(`source link key overlaps another: "${key}"`);
    if (at > pos) segments.push({ text: source.slice(pos, at) });
    segments.push({ text: key, href });
    pos = at + key.length;
  }
  if (pos < source.length) segments.push({ text: source.slice(pos) });
  return segments;
}
