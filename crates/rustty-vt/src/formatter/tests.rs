//! Tests for `formatter` (ghostty src/terminal/formatter.zig tests), split
//! by topic under `tests/`. Shared fixtures live here.

// PORT NOTE: the reference test bodies use index loops (`for (0..N) |i|`) and
// index `point_map.items[offset + i]`. We keep the faithful indexed form
// rather than the iterator form clippy would prefer.
#![allow(clippy::needless_range_loop)]

use super::*;
use crate::alloc::Global;
use crate::terminal::{Colors, Options as TermOptions, Terminal};

// PORT NOTE: the reference tests call `Terminal.init(alloc, .{ .cols, .rows })`
// relying on `Options` field defaults. Rust `Options` has no field defaults,
// so this helper supplies them verbatim (mirroring terminal.rs's test `opts`).
pub(super) fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> TermOptions {
    TermOptions {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors {
            background: crate::color::DynamicRGB::UNSET,
            foreground: crate::color::DynamicRGB::UNSET,
            cursor: crate::color::DynamicRGB::UNSET,
            palette: crate::color::DynamicPalette::default_palette(),
        },
        default_modes: crate::modes::ModePacked::default(),
        default_cursor_style: crate::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        default_xt_checksum: Default::default(),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
    }
}

// PORT NOTE: `&pages.pages.last.?.data` — deref the raw last node to a
// `*const Page`. The caller guarantees the node outlives the formatter.
pub(super) unsafe fn last_page(pages: &PageList) -> *const Page {
    unsafe { &(*crate::pagelist::Node::page(pages.pages.last)) }
}

// PORT NOTE: `std.mem.trimStart(u8, s, pat)` — strip leading bytes that are
// any of the bytes in `pat` (Zig trims the *set* of bytes).
pub(super) fn trim_start<'a>(s: &'a [u8], pat: &[u8]) -> &'a [u8] {
    let mut i = 0;
    while i < s.len() && pat.contains(&s[i]) {
        i += 1;
    }
    &s[i..]
}

// PORT NOTE: `std.mem.indexOf(u8, haystack, needle)`.
pub(super) fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// PORT NOTE: `Coordinate{ .x, .y }` helper for assertions.
pub(super) fn coord(x: size::CellCountInt, y: u32) -> Coordinate {
    Coordinate { x, y }
}

// PORT NOTE: `Pin{ .node, .x, .y }` helper; the reference `Pin` has no
// `garbage` field, so we default it to false (untracked pins are never
// garbage).
pub(super) fn pin(
    node: *mut crate::pagelist::Node,
    x: size::CellCountInt,
    y: size::CellCountInt,
) -> Pin {
    Pin {
        node,
        x,
        y,
        garbage: false,
    }
}

mod html_codepoint_pinmap;
mod page_plain;
mod page_vt_pagelist;
mod terminal_screen;
