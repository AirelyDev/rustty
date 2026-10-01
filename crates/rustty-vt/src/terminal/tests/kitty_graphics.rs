//! PORTS: ghostty src/terminal/Terminal.zig — the kitty graphics state tests on the scroll / erase-display / screen-switch routes.

use super::*;

// =========================================================================
// Kitty graphics state on the scroll / erase-display / screen-switch routes.
//
// PORT NOTE: NOT ports of reference `test {}` blocks — the reference has no
// tests for these writes. They are the parity gate for the
// `kitty_images.dirty = true` sets in `index` / `scroll_up` / `insert_lines`
// / `delete_lines` / `switch_screen` (Terminal.zig:2264, 2471, 2720, 2886,
// 4509) and for the `kitty_images.delete(.{ .all = true })` calls in
// `erase_display` (Terminal.zig:3329-3337 and 3387-3397). All of those were
// omitted behind a stale "vt-12 deferral" PORT NOTE even though Kitty
// graphics is ENABLED (`options::KITTY_GRAPHICS`) and `ImageStorage` is
// live. Every case below is one the pre-fix tree got wrong: the routes are
// chosen so the flag can NOT come from the `Screen`-level helpers
// (`cursor_down_scroll` / `cursor_scroll_above` / `scroll`), i.e. they are
// the region / margin routes, not the full-screen scrollback ones.
// =========================================================================

fn kitty_dirty(t: &Terminal) -> bool {
    // SAFETY: `screens.active` is a stable heap pointer owned by `screens`.
    unsafe { (*t.screens.active).kitty_images.dirty }
}

/// Clear the `dirty` flag on EVERY screen, which is what a renderer does
/// once it has drawn the images.
fn clear_kitty_dirty(t: &mut Terminal) {
    for slot in t.screens.all.iter() {
        if let Some(s) = *slot {
            // SAFETY: stable heap pointers owned by `screens`.
            unsafe { (*s).kitty_images.dirty = false };
        }
    }
}

// Terminal.zig:2264 — index() inside a top/bottom scrolling region.
#[test]
fn terminal_kitty_dirty_index_top_bottom_region() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(4, 1);
    clear_kitty_dirty(&mut t);
    t.index().unwrap();

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:2264 — index() inside left/right margins.
#[test]
fn terminal_kitty_dirty_index_left_right_margin() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.scrolling_region.left = 3;
    t.scrolling_region.right = 5;
    t.set_cursor_pos(3, 4);
    clear_kitty_dirty(&mut t);
    t.index().unwrap();

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:2471 — scrollUp's scrollback branch with a partial region.
#[test]
fn terminal_kitty_dirty_scroll_up_top_partial_region() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    clear_kitty_dirty(&mut t);
    t.scroll_up(1).unwrap();

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:2471/2886 — scrollUp with left/right margins routes through
// deleteLines, which has its own dirty write.
#[test]
fn terminal_kitty_dirty_scroll_up_left_right_margin() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.scrolling_region.left = 3;
    t.scrolling_region.right = 5;
    t.set_cursor_pos(1, 4);
    clear_kitty_dirty(&mut t);
    t.scroll_up(1).unwrap();

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:2720 — insertLines.
#[test]
fn terminal_kitty_dirty_insert_lines() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(2, 1);
    clear_kitty_dirty(&mut t);
    t.insert_lines(1);

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:2886 — deleteLines.
#[test]
fn terminal_kitty_dirty_delete_lines() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(2, 1);
    clear_kitty_dirty(&mut t);
    t.delete_lines(1);

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:2320 -> 2423 -> 2720 — reverseIndex at the top of the region
// inherits the dirty write from scrollDown/insertLines, exactly as the
// reference does (it has no write of its own).
#[test]
fn terminal_kitty_dirty_reverse_index_at_region_top() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(2, 1);
    clear_kitty_dirty(&mut t);
    t.reverse_index();

    assert!(kitty_dirty(&t));

    t.deinit(Global);
}

// Terminal.zig:4509 — switchScreen marks the DESTINATION screen's images
// dirty on every switch, including re-entry into an already-created screen
// whose flag a renderer has since cleared.
#[test]
fn terminal_kitty_dirty_switch_screen_marks_destination() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    // First switch: the alternate screen is lazily created here.
    clear_kitty_dirty(&mut t);
    t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
    assert!(kitty_dirty(&t), "alternate screen, first entry");

    // Re-entry into the ALREADY-CREATED primary (the primary always
    // exists, so this arm can never be excused as "fresh screens have no
    // images yet").
    clear_kitty_dirty(&mut t);
    t.switch_screen(crate::screen_set::Key::Primary).unwrap();
    assert!(kitty_dirty(&t), "primary screen, re-entry");

    // Re-entry into the already-created alternate.
    clear_kitty_dirty(&mut t);
    t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
    assert!(kitty_dirty(&t), "alternate screen, re-entry");

    t.deinit(Global);
}

/// Put one image plus one (non-virtual) placement into the active screen's
/// Kitty image storage, the state `kitty_images.delete(.{ .all = true })`
/// is supposed to tear down.
fn seed_kitty_image(t: &mut Terminal) {
    use crate::kitty::graphics_image::Image;
    use crate::kitty::graphics_storage::{Location, Placement};

    // SAFETY: `screens.active` is a stable heap pointer owned by `screens`
    // and we hold `&mut t`. Each access re-derives its reference from the
    // raw base pointer so no alias outlives its statement (the storage's
    // `s: *mut Screen` PORT NOTE shape).
    let sp = t.screens.active;
    unsafe { &mut (*sp).kitty_images }
        .add_image(
            Global,
            sp,
            Image {
                id: 1,
                width: 4,
                height: 4,
                ..Image::default()
            },
        )
        .unwrap();
    let pin = {
        let pages = unsafe { &mut (*sp).pages };
        let pin = pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        pages.track_pin(pin).unwrap()
    };
    unsafe { &mut (*sp).kitty_images }
        .add_placement(
            Global,
            sp,
            1,
            0,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();

    assert_eq!(1, unsafe { &(*sp).kitty_images }.images.len());
    assert_eq!(1, unsafe { &(*sp).kitty_images }.placements.len());
}

fn kitty_image_counts(t: &Terminal) -> (usize, usize) {
    // SAFETY: `screens.active` is a stable heap pointer owned by `screens`.
    let s = unsafe { &(*t.screens.active).kitty_images };
    (s.images.len(), s.placements.len())
}

// Terminal.zig:3387-3397 — ED complete clears all Kitty graphics state.
#[test]
fn terminal_erase_display_complete_deletes_kitty_images() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
    seed_kitty_image(&mut t);

    t.erase_display(csi::EraseDisplay::Complete, false);

    assert_eq!((0, 0), kitty_image_counts(&t));

    t.deinit(Global);
}

// Terminal.zig:3329-3337 — ED scroll-complete clears all Kitty graphics
// state too.
#[test]
fn terminal_erase_display_scroll_complete_deletes_kitty_images() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
    seed_kitty_image(&mut t);

    t.erase_display(csi::EraseDisplay::ScrollComplete, false);

    assert_eq!((0, 0), kitty_image_counts(&t));

    t.deinit(Global);
}

// Terminal.zig:5512-ish — DECCOLM inherits the clear through
// eraseDisplay(.complete).
#[test]
fn terminal_deccolm_deletes_kitty_images() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
    t.modes.set(modespkg::Mode::EnableMode3, true);
    seed_kitty_image(&mut t);

    t.deccolm(Global, DeccolmMode::Cols132).unwrap();

    assert_eq!((0, 0), kitty_image_counts(&t));

    t.deinit(Global);
}
