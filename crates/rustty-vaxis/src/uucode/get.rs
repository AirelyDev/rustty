//! `uucode/src/get.zig` (subset) — the four concrete typed accessors that replace
//! uucode's generic comptime `get(field, cp)` dispatch (VAXIS_C1B_DESIGN.md §6). For
//! these four *basic* fields the Zig `get` just returns `@field(data(cp), name)`; there
//! is no `unshift`/`unpack`/`value` path, so each accessor is the plain `data(cp).<field>`
//! read of the 2-stage lookup. `cp: u32` (Rust has no u21) — callers pass decoded scalars.

use super::tables::data;
use super::types::{EastAsianWidth, GeneralCategory, GraphemeBreak};

/// `uucode.get(.east_asian_width, cp)` — used by gwidth `eawToWidth`.
pub fn east_asian_width(cp: u32) -> EastAsianWidth {
    data(cp).east_asian_width
}

/// `uucode.get(.general_category, cp)` — used by gwidth `eawToWidth`.
pub fn general_category(cp: u32) -> GeneralCategory {
    data(cp).general_category
}

/// `uucode.get(.is_emoji_presentation, cp)` — used by gwidth `.unicode` arbitration.
pub fn is_emoji_presentation(cp: u32) -> bool {
    data(cp).is_emoji_presentation
}

/// `uucode.get(.grapheme_break, cp)` — used by the grapheme `Iterator` (UAX #29).
pub fn grapheme_break(cp: u32) -> GraphemeBreak {
    data(cp).grapheme_break
}
