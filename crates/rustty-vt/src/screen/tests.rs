//! PORTS: ghostty src/terminal/Screen.zig — the `test` blocks, split by topic
//! into `tests/<topic>.rs`; this root holds the shared helpers.

// PORT NOTE: these are the Screen foundation tests that exercise ONLY the
// methods filled in vt-11a (init/cursor movement/cursorCopy/setAttribute).
// The read/write + `cursorCopy x/y` tests depend on `dumpStringAlloc`
// (DEFERRED), so they land with the dump methods in a later sub-batch.
use super::*;
use crate::selection_codepoints;
use crate::style;

mod basic;
mod clear_prompt;
mod hyperlink_capacity;
mod resize;
mod scroll;
mod select;
mod upstream_defects;

// PORT NOTE: small helpers to keep the resize tests close to the Zig form.
// `mkscreen` mirrors `try init(alloc, .{ ... })`; `dump`/`dump_unwrapped`
// mirror `try s.dumpStringAlloc(alloc, <point>)`.
pub(super) fn mkscreen(
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    max_scrollback_bytes: usize,
) -> Screen {
    Screen::init(
        Allocator,
        Options {
            cols,
            rows,
            max_scrollback_bytes: Some(max_scrollback_bytes),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap()
}

pub(super) fn vp() -> point::Point {
    point::Point::Viewport(point::Coordinate::default())
}
pub(super) fn scr() -> point::Point {
    point::Point::Screen(point::Coordinate::default())
}
pub(super) fn act() -> point::Point {
    point::Point::Active(point::Coordinate::default())
}
pub(super) fn dump(s: &Screen, pt: point::Point) -> Vec<u8> {
    s.dump_string_alloc(Allocator, pt).unwrap()
}

// PORT NOTE: helpers for the selection tests below.
//   `spin(s, pt)` mirrors `s.pages.pin(pt).?`.
//   `pfp_screen(s, p)` mirrors `s.pages.pointFromPin(.screen, p).?`.
//   `pfp_active(s, p)` mirrors `s.pages.pointFromPin(.active, p).?`.
//   `selstr(s, sel, trim)` mirrors `try s.selectionString(alloc, .{ .sel, .trim })`.
pub(super) fn spin(s: &Screen, pt: point::Point) -> Pin {
    s.pages.pin(pt).unwrap()
}
pub(super) fn pscreen(x: size::CellCountInt, y: u32) -> point::Point {
    point::Point::Screen(point::Coordinate { x, y })
}
pub(super) fn pactive(x: size::CellCountInt, y: u32) -> point::Point {
    point::Point::Active(point::Coordinate { x, y })
}
pub(super) fn pfp_screen(s: &Screen, p: Pin) -> point::Point {
    s.pages.point_from_pin(point::Tag::Screen, p).unwrap()
}
pub(super) fn pfp_active(s: &Screen, p: Pin) -> point::Point {
    s.pages.point_from_pin(point::Tag::Active, p).unwrap()
}
pub(super) fn selstr(s: &mut Screen, sel: Selection, trim: bool) -> Vec<u8> {
    s.selection_string(Allocator, SelectionString { sel, trim })
        .unwrap()
}

// PORT NOTE: `init` helper mirroring the reference's
// `try init(alloc, .{ .cols, .rows, .max_scrollback_bytes })`. The kitty image
// limit fields are not exercised by these tests (kitty graphics is deferred
// to vt-12); they take the same defaults used by every other test in this
// module.
pub(super) fn clone_clear_init(
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    max_scrollback_bytes: usize,
) -> Screen {
    Screen::init(
        Allocator,
        Options {
            cols,
            rows,
            max_scrollback_bytes: Some(max_scrollback_bytes),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap()
}
