//! The vendored `uucode` runtime subset (Option B — VAXIS_C1B_DESIGN.md §1/§2), at the
//! revision the reference pin wires into libvaxis 0.6.0 (uucode 2826a37a with ghostty's
//! `uucode_config.zig`; was 0.1.0 until parity tail S-10b, NON_PORTS.md §G2), restricted to
//! exactly what vaxis's `gwidth`/`unicode`/`Parser`/`TextInput` consume: the four-field
//! (`east_asian_width`, `grapheme_break`, `general_category`, `is_emoji_presentation`)
//! 2-stage width/grapheme table + its typed `get`, the Höhrmann UTF-8 decoder, and the
//! UAX #29 grapheme `Iterator`. The stage1/stage2 tables are reproduced bit-faithfully
//! by `build.rs` from the vendored UCD 17.0.0 corpus (`ucd/`).

pub mod get;
pub mod grapheme;
pub mod tables;
pub mod types;
pub mod utf8;
