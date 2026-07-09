//! rustty-vaxis — pure-Rust port of the vendored `libvaxis` 0.5.1 subset (R3).
//!
//! CHUNK 1a — the CORE VALUE TYPES only. Module-per-Zig-file layout:
//! - [`cell`] — `Cell.zig` (`Style`/`Color`/`Segment`/`Cell` + SGR bits)
//! - [`key`] — `Key.zig` (`Key` + constants + matchers + `Modifiers`/`KittyFlags`)
//! - [`mouse`] — `Mouse.zig`
//! - [`grapheme_cache`] — `GraphemeCache.zig`
//! - [`image`] — `Image.zig` STRUCT SHAPE (zigimg/Window bodies are R5 seams)
//! - [`Winsize`] — the `main.zig:38` terminal-size struct
//!
//! CHUNK 1b adds the width/grapheme surface:
//! - [`gwidth`] — `gwidth.zig` (grapheme-width Method/eawToWidth/gwidth, 11-test pin)
//! - [`unicode`] — `unicode.zig` (Old-API grapheme iterator wrapper)
//! - [`uucode`] — the vendored `uucode` 0.1.0 runtime subset (types/get/grapheme/utf8
//!   + the stage1/stage2 tables `build.rs` reproduces from the vendored UCD 16.0.0)
//!
//! PORT STATUS: done (chunk 1a — value types; chunk 1b — gwidth/unicode + the uucode
//! width/grapheme tables; rendering/tty/loop are later chunks).

pub mod cell;
pub mod grapheme_cache;
pub mod gwidth;
pub mod image;
pub mod key;
pub mod mouse;
pub mod unicode;
pub mod uucode;

/// The size of the terminal screen
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Winsize {
    pub rows: u16,
    pub cols: u16,
    pub x_pixel: u16,
    pub y_pixel: u16,
}
