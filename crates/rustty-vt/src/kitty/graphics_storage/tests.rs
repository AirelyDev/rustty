//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig — the `test {}`
//! blocks. The shared helpers live here; the tests are in `tests/`.

// PORT NOTE: the ported tests mirror the reference verbatim:
//   - `s.placements.get(.{...}) != null` → `s.placements.get(&...).is_some()`
//     (clippy `unnecessary_get_then_check` would prefer `contains_key`, but we
//     keep `get(...).is_some()` to mirror the reference `get(...) != null`).
//   - `var s: ImageStorage = .{}; s.total_limit = 5000;` → a `Default::default()`
//     followed by a field assignment (clippy `field_reassign_with_default`).
// Both forms are preserved per R1; the lints are allowed on the test module only.
#![allow(
    clippy::unnecessary_get_then_check,
    clippy::field_reassign_with_default
)]

use super::*;
use crate::kitty::graphics_image::{Data, Metadata};
use crate::size;
use crate::terminal::Terminal;

mod placement_delete;
mod scroll_animation;
mod storage;

// PORT NOTE: the tests construct `terminal.Terminal.init(alloc, .{ .cols, .rows })`
// relying on the reference `Terminal.Options` field defaults (`max_scrollback_bytes`,
// `colors = .default`, `default_modes = .{}`, kitty limits). Rust `Options` has
// no field defaults, so this helper supplies them verbatim, mirroring the
// reference `Options` declaration (see terminal.rs's test `opts` helper).
fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> crate::terminal::Options {
    crate::terminal::Options {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: crate::terminal::Colors::default(),
        default_modes: crate::modes::ModePacked::default(),
        default_cursor_style: crate::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        default_xt_checksum: Default::default(),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: Limits::DIRECT,
    }
}

// Our pin for the placement
// fn trackPin(t: *terminal.Terminal, pt: point.Coordinate) !*PageList.Pin {
//     return try t.screens.active.pages.trackPin(t.screens.active.pages.pin(.{
//         .active = pt,
//     }).?);
// }
fn track_pin(
    t: &mut Terminal,
    pt: point::Coordinate,
) -> Result<*mut PageList::Pin, crate::pagelist::OutOfMemory> {
    let s = unsafe { &mut *t.screens.active };
    let pin = s.pages.pin(point::Point::Active(pt)).unwrap();
    s.pages.track_pin(pin)
}
