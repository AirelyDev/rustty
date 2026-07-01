//! `sprite/draw/` — the per-glyph sprite drawing modules.
//!
//! PORT NOTE: this is the Rust module root for ghostty's `src/font/sprite/draw/`
//! directory. Only the shared `common` helpers are ported in the sprite-scaffold
//! batch; the individual glyph draw modules (`box.zig`, `block.zig`, `braille`,
//! etc.) are ported in later batches, so they are not yet declared here.

pub mod common;
