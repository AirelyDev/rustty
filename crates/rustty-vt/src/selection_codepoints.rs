// This file contains various default word boundaries used for
// selection logic. We put it in a separate file so that different
// subsystems can import it without introducing a number of
// dependencies.

// PORT NOTE: Rust has no `u21`. Zig's `u21` (used for codepoints) occupies the
// same 4-byte storage as `u32` and codepoint values fit in 21 bits, so we reuse
// the crate's `u21 = u32` alias (defined in `page.rs`) for these codepoint
// tables. The `'x' as u21` casts reproduce Zig's character-literal coercion to
// `u21`.
use crate::page::u21;

/// Default boundary characters for word selection: ` \t'"│`|:;,()[]{}<>$`
pub const DEFAULT_WORD_BOUNDARIES: [u21; 20] = [
    0,           // null
    ' ' as u21,  // space
    '\t' as u21, // tab
    '\'' as u21, // single quote
    '"' as u21,  // double quote
    '│' as u21,  // U+2502 box drawing
    '`' as u21,  // backtick
    '|' as u21,  // pipe
    ':' as u21,  // colon
    ';' as u21,  // semicolon
    ',' as u21,  // comma
    '(' as u21,  // left paren
    ')' as u21,  // right paren
    '[' as u21,  // left bracket
    ']' as u21,  // right bracket
    '{' as u21,  // left brace
    '}' as u21,  // right brace
    '<' as u21,  // less than
    '>' as u21,  // greater than
    '$' as u21,  // dollar
];

/// Default whitespace characters trimmed from line selections.
pub const DEFAULT_LINE_WHITESPACE: [u21; 3] = [0, ' ' as u21, '\t' as u21];
