//! Tests for `stream_terminal` (ghostty src/terminal/stream_terminal.zig
//! tests), split by topic under `tests/`. Shared fixtures live here.

use super::*;
use crate::alloc::Global;
use crate::color::{DynamicPalette, DynamicRGB};
use crate::modes as modespkg;
use crate::terminal::Colors;
use crate::terminal::Options;
use crate::{kitty, size};

mod basics;
mod clipboard_dnd_paste;
mod dispatch;
mod effects;

// PORT NOTE: the reference tests construct `Terminal: try .init(testing.allocator,
// .{ .cols, .rows })` and drive bytes through a `Stream`. Rustty's `Options`
// has no field defaults, so we supply them via this helper (matching
// `terminal.rs`'s test `opts`). `max_scrollback_bytes` defaults to 10_000 except
// where a test overrides it.
pub(super) fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
    Options {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors {
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor: DynamicRGB::UNSET,
            palette: DynamicPalette::default_palette(),
        },
        default_modes: modespkg::ModePacked::default(),
        default_cursor_style: crate::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        default_xt_checksum: Default::default(),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    }
}

// -- Handler.resize tests (oracle 89b103dd5) ------------------------------
//
// PORT NOTE: the reference tests hang a `var` on a local `S` struct to count
// / capture `writePty` calls. Rust's `write_pty` is a bare `fn` pointer, so
// the counters live in thread-locals (same idiom as `CAPTURE`/`cap_*`).
thread_local! {
    static WRITE_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) fn calls_reset() {
    WRITE_CALLS.with(|c| c.set(0));
    CAPTURE.with(|c| *c.borrow_mut() = None);
}

pub(super) fn calls_get() -> usize {
    WRITE_CALLS.with(|c| c.get())
}

pub(super) fn count_write_pty(_: &mut Handler, data: &[u8]) {
    WRITE_CALLS.with(|c| c.set(c.get() + 1));
    CAPTURE.with(|c| *c.borrow_mut() = Some(data.to_vec()));
}

pub(super) fn cell_9x18() -> crate::terminal::ResizeCellSize {
    crate::terminal::ResizeCellSize {
        width: 9,
        height: 18,
    }
}

// -- vt-12b-7b response/query effect tests --------------------------------
//
// PORT NOTE (vt-12b-7b): the reference tests capture the `write_pty` output
// into a local-struct `var` static (`S.written`/`S.last_response`) inside a
// `fn writePty(...)` callback. Rust's `write_pty` is a bare `fn` pointer (no
// closures/captures), so the captured bytes are routed through a
// `thread_local!` cell. The `CAPTURE` helpers below are the analog of the
// reference's per-test `S` struct: `cap_write_pty` is the `writePty` callback
// (it stores the latest response, freeing/replacing the prior one — matching
// the reference's `if (old) free; dupe` pattern); `cap_take`/`cap_reset` read
// and clear it. Effect callbacks that return fixed strings
// (`enquiry`/`xtversion`/`getSize`/`colorScheme`/`da`/`titleChanged`) become
// free `fn`s. The `title_changed`/`bell` counters likewise live in a cell.

thread_local! {
    static CAPTURE: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
    static COUNTER: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) fn cap_reset() {
    CAPTURE.with(|c| *c.borrow_mut() = None);
    COUNTER.with(|c| c.set(0));
}

pub(super) fn cap_take() -> Option<Vec<u8>> {
    CAPTURE.with(|c| c.borrow().clone())
}

// fn writePty(_: *Handler, data: [:0]const u8) void { ...dupe into S.written... }
pub(super) fn cap_write_pty(_: &mut Handler, data: &[u8]) {
    CAPTURE.with(|c| *c.borrow_mut() = Some(data.to_vec()));
}
