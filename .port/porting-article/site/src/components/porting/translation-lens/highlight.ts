/**
 * A small Zig / Rust tokenizer for the excerpt panes. Token kinds are the
 * highlight.js class suffixes, so the panes use the site's code theme
 * (`.hljs-keyword`, `.hljs-string`, … in globals.css); `unsafe` gets its own
 * kind so the page can mark it in red bold.
 */

/** A token kind, or undefined for plain text. */
export type TokenKind =
  | "keyword"
  | "string"
  | "meta"
  | "built_in"
  | "type"
  | "comment"
  | "number"
  | "unsafe";

/** One token: its text and kind. */
export type Token = { t: string; k?: TokenKind };

/** KEYWORDS are the reserved words of each language. */
const KEYWORDS: Record<"zig" | "rust", Set<string>> = {
  zig: new Set(
    "pub fn const var return if else for while switch break continue defer errdefer try catch orelse comptime inline struct union enum packed extern and or undefined null true false test error".split(
      " ",
    ),
  ),
  rust: new Set(
    "pub fn const let mut return if else for while loop match break continue impl struct enum union trait type where use mod crate self Self super as in ref move static true false Some None Ok Err dyn unsafe".split(
      " ",
    ),
  ),
};

/** PRIMITIVES are lower-case primitive type names highlighted as types. */
const PRIMITIVES = new Set(
  "u2 u8 u16 u21 u24 u32 u64 usize i8 i16 i32 i64 isize bool void anyopaque type f32 f64 char str".split(
    " ",
  ),
);

/** TOKEN_RE splits a line: comment, string, char, attribute, Zig builtin, number, word (with macro bang), any other char. */
const TOKEN_RE =
  /(\/\/.*$)|("(?:[^"\\]|\\.)*")|('(?:[^'\\]|\\.)')|(#!?\[[^\]]*\]?)|(@[A-Za-z_]+)|(\b\d[\d_]*(?:\.\d+)?\b)|([A-Za-z_][A-Za-z0-9_]*!?)|([\s\S])/g;

/** Returns the kind of word w in lang. */
function wordKind(w: string, lang: "zig" | "rust"): TokenKind | undefined {
  if (w === "unsafe") return "unsafe";
  if (KEYWORDS[lang].has(w)) return "keyword";
  if (w.endsWith("!")) return "built_in";
  if (/^[A-Z][A-Za-z0-9_]*$/.test(w) || PRIMITIVES.has(w)) return "type";
  return undefined;
}

/**
 * Splits one source line into tokens whose texts concatenate back to the
 * line. A line that is only a comment comes back as a single plain token
 * (the pane colours whole comment lines by their mapping class).
 */
export function tokenize(line: string, lang: "zig" | "rust"): Token[] {
  if (/^\s*\/\//.test(line)) return [{ t: line }];
  const out: Token[] = [];
  /** Appends text with kind k, merging runs of plain text. */
  const push = (t: string, k?: TokenKind) => {
    const last = out[out.length - 1];
    if (!k && last && !last.k) last.t += t;
    else out.push(k ? { t, k } : { t });
  };
  for (const m of line.matchAll(TOKEN_RE)) {
    if (m[1]) push(m[1], "comment");
    else if (m[2] || m[3]) push(m[0], "string");
    else if (m[4]) push(m[4], "meta");
    else if (m[5]) push(m[5], "built_in");
    else if (m[6]) push(m[6], "number");
    else if (m[7]) push(m[7], wordKind(m[7], lang));
    else push(m[0]);
  }
  return out;
}
