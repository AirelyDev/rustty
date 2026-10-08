//! PORTS: ghostty src/terminal/Terminal.zig — the tests, shared test helpers and the first test group (print-string error, pwd/title, scrollback limits).

// PORT NOTE: no reference test. `Screen.testWriteString`'s inferred error
// set carries `PageList.IncreaseCapacityError` through
// `try self.cursorSetHyperlink()`; the port's `PrintStringError` had no
// `OutOfSpace`, so all four of those sites did
// `.map_err(|_| OutOfMemory)?` and the one distinguishable error was lost
// (parity S134-F2). The four sites now use a bare `?`, which routes through
// this conversion — pinned here. Reaching `OutOfSpace` from a real page
// needs a page filled to its hyperlink-map limit, which no reference test
// constructs either; what IS pinned is that the variant survives the
// conversion the `?` uses.
#[test]
fn print_string_error_preserves_out_of_space() {
    use crate::pagelist::grow::IncreaseCapacityError;
    assert_eq!(
        PrintStringError::OutOfSpace,
        PrintStringError::from(IncreaseCapacityError::OutOfSpace),
    );
    assert_eq!(
        PrintStringError::OutOfMemory,
        PrintStringError::from(IncreaseCapacityError::OutOfMemory),
    );
}
use super::*;
use crate::alloc::Global;

// test "Terminal: setPwd preserves a sentinel on allocation failure"
#[test]
fn terminal_set_pwd_preserves_a_sentinel_on_allocation_failure() {
    // var failing = testing.FailingAllocator.init(testing.allocator, .{});
    // const alloc = failing.allocator();
    let failing = crate::alloc::testing::FailingAllocator::over_heap(
        crate::alloc::testing::Config::default(),
    );
    let c = failing.allocator();
    crate::alloc::testing::with_handle(&c, |alloc| {
        let mut t = Terminal::init(alloc, opts(5, 1)).unwrap();
        // defer t.deinit(alloc);

        // try t.pwd.ensureTotalCapacityPrecise(alloc, 3);
        t.pwd.ensure_total_capacity_precise(alloc, 3).unwrap();
        // failing.fail_index = failing.alloc_index;
        failing.fail_index.set(failing.alloc_index.get());
        // PORT NOTE: the growth tries `remap` first. The reference's inner
        // `testing.allocator` (DebugAllocator) cannot remap a 3-byte
        // size-class slot to the grown capacity, so its remap is null and
        // the growth reaches the failing `alloc`. This test's inner heap
        // remaps with `realloc`, which may move, so the remap is refused
        // here explicitly to reproduce that answer.
        failing.resize_fail_index.set(failing.resize_index.get());
        assert_eq!(Err(OutOfMemory), t.set_pwd(b"pwd"));
        assert!(failing.has_induced_failure.get());
        assert!(t.get_pwd().is_none());

        t.deinit(alloc);
    });
}

// test "Terminal: setPwd accepts its current value"
#[test]
fn terminal_set_pwd_accepts_its_current_value() {
    let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

    t.set_pwd(b"file:///tmp").unwrap();

    // PORT NOTE: the reference writes `t.setPwd(t.getPwd().?)`. Borrowck
    // forbids holding the `&[u8]` from `get_pwd` across the `&mut self`
    // call, so we re-derive the SAME aliasing source as a RAW pointer and
    // go through `set_pwd_raw`. This is exactly the aliasing the vt-c FFI
    // seam can produce. Passing it as a `&[u8]` to `set_pwd` instead would
    // be Stacked-Borrows UB — the argument is strongly protected for the
    // duration of the call, so writing into it is illegal regardless of
    // memmove-vs-memcpy (proven under Miri, WS8 T7).
    let (ptr, len) = {
        let cur = t.get_pwd().unwrap();
        (cur.as_ptr(), cur.len())
    };
    unsafe { t.set_pwd_raw(ptr, len) }.unwrap();

    assert_eq!(b"file:///tmp".as_slice(), t.get_pwd().unwrap());

    t.deinit(Global);
}

// test "Terminal: setTitle preserves a sentinel on allocation failure"
#[test]
fn terminal_set_title_preserves_a_sentinel_on_allocation_failure() {
    let failing = crate::alloc::testing::FailingAllocator::over_heap(
        crate::alloc::testing::Config::default(),
    );
    let c = failing.allocator();
    crate::alloc::testing::with_handle(&c, |alloc| {
        let mut t = Terminal::init(alloc, opts(5, 1)).unwrap();
        // defer t.deinit(alloc);

        // try t.title.ensureTotalCapacityPrecise(alloc, 5);
        t.title.ensure_total_capacity_precise(alloc, 5).unwrap();
        // failing.fail_index = failing.alloc_index;
        failing.fail_index.set(failing.alloc_index.get());
        // PORT NOTE: the growth tries `remap` first. The reference's inner
        // `testing.allocator` (DebugAllocator) cannot remap a 5-byte
        // size-class slot to the grown capacity, so its remap is null and
        // the growth reaches the failing `alloc`. This test's inner heap
        // remaps with `realloc`, which may move, so the remap is refused
        // here explicitly to reproduce that answer.
        failing.resize_fail_index.set(failing.resize_index.get());
        assert_eq!(Err(OutOfMemory), t.set_title(b"title"));
        assert!(failing.has_induced_failure.get());
        assert!(t.get_title().is_none());

        t.deinit(alloc);
    });
}

// test "Terminal: setTitle accepts its current value"
#[test]
fn terminal_set_title_accepts_its_current_value() {
    let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

    t.set_title(b"Rustty").unwrap();

    // See the PORT NOTE in `terminal_set_pwd_accepts_its_current_value`.
    let (ptr, len) = {
        let cur = t.get_title().unwrap();
        (cur.as_ptr(), cur.len())
    };
    unsafe { t.set_title_raw(ptr, len) }.unwrap();

    assert_eq!(b"Rustty".as_slice(), t.get_title().unwrap());

    t.deinit(Global);
}

// PORT NOTE: the reference tests call `init(alloc, .{ .cols = .., .rows = .. })`
// and rely on `Options` field defaults (`max_scrollback_bytes = 10_000`,
// `colors = .default`, `default_modes = .{}`, kitty limits). Rust `Options`
// has no field defaults, so this helper supplies them verbatim, mirroring the
// reference `Options` declaration. `colors = .default` expands to
// `Colors{ background/foreground/cursor = .unset, palette = .default }`
// (`Colors::default()` inlined here for tests only).
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

// Same as `opts` but with a caller-supplied `max_scrollback_bytes`, mirroring
// reference inits like `.{ .rows, .cols, .max_scrollback_bytes = 0 }`.
pub(super) fn opts_sb(
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    max_scrollback_bytes: usize,
) -> Options {
    Options {
        max_scrollback_bytes: Some(max_scrollback_bytes),
        ..opts(cols, rows)
    }
}

// Screen-point getCell helper mirroring the reference
// `t.screens.active.pages.getCell(.{ .screen = .{ .x, .y } })`.
pub(super) fn screen_pt(x: size::CellCountInt, y: u32) -> point::Point {
    point::Point::Screen(point::Coordinate { x, y })
}

// Active-point helper mirroring the reference
// `.{ .active = .{ .x, .y } }` literals used in the dirty-bit assertions.
pub(super) fn active_pt(x: size::CellCountInt, y: u32) -> point::Point {
    point::Point::Active(point::Coordinate { x, y })
}

// Viewport-point helper mirroring the reference
// `t.screens.active.pages.getCell(.{ .viewport = .{ .x, .y } })` literals.
pub(super) fn viewport_pt(x: size::CellCountInt, y: u32) -> point::Point {
    point::Point::Viewport(point::Coordinate { x, y })
}

// ref: test "Terminal setScrollbackMaxBytes"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn terminal_set_scrollback_max_bytes() {
    let mut t = Terminal::init(
        Global,
        Options {
            max_scrollback_bytes: None,
            ..opts(80, 3)
        },
    )
    .unwrap();

    let primary: *mut crate::screen::Screen =
        t.screens.get(crate::screen_set::Key::Primary).unwrap();
    let page_rows: usize =
        unsafe { crate::pagelist::Node::capacity((*primary).pages.pages.first).rows as usize };

    // Build several complete pages of history so lowering the byte limit has
    // existing allocations to prune immediately.
    for _ in 0..(4 * page_rows) {
        t.linefeed().unwrap();
    }
    let old_page_size = unsafe { (*primary).pages.page_size };
    t.set_scrollback_max_bytes(Some(1));
    unsafe {
        assert_eq!(1usize, (*primary).pages.limits.bytes.explicit);
        assert!((*primary).pages.page_size < old_page_size);
        assert!(
            (*primary).pages.page_size <= (*primary).pages.limits.max(crate::pagelist::Key::Bytes)
        );
        assert!(!(*primary).no_scrollback);
    }

    // Zero switches Screen behavior as well as PageList accounting, discards
    // all retained history, and prevents future linefeeds from recreating it.
    t.set_scrollback_max_bytes(Some(0));
    unsafe {
        assert_eq!(0usize, (*primary).pages.limits.bytes.explicit);
        assert!((*primary).no_scrollback);
        assert_eq!((*primary).pages.rows as usize, (*primary).pages.total_rows);
        assert!((*primary).pages.viewport == crate::pagelist::Viewport::Active);
    }

    for _ in 0..page_rows {
        t.linefeed().unwrap();
    }
    unsafe {
        assert_eq!((*primary).pages.rows as usize, (*primary).pages.total_rows);
    }

    // Re-enabling unlimited scrollback affects subsequent output.
    t.set_scrollback_max_bytes(None);
    unsafe {
        assert_eq!(usize::MAX, (*primary).pages.limits.bytes.explicit);
        assert!(!(*primary).no_scrollback);
    }
    for _ in 0..page_rows {
        t.linefeed().unwrap();
    }
    unsafe {
        assert!((*primary).pages.total_rows > (*primary).pages.rows as usize);
    }

    t.deinit(Global);
}

// ref: test "Terminal setScrollbackMaxLines"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn terminal_set_scrollback_max_lines() {
    let mut t = Terminal::init(
        Global,
        Options {
            max_scrollback_bytes: None,
            max_scrollback_lines: None,
            ..opts(80, 3)
        },
    )
    .unwrap();

    let primary: *mut crate::screen::Screen =
        t.screens.get(crate::screen_set::Key::Primary).unwrap();
    let page_rows: usize =
        unsafe { crate::pagelist::Node::capacity((*primary).pages.pages.first).rows as usize };

    for _ in 0..(4 * page_rows) {
        t.linefeed().unwrap();
    }
    let old_total_rows = unsafe { (*primary).pages.total_rows };
    t.set_scrollback_max_lines(Some(page_rows));
    unsafe {
        assert_eq!(page_rows, (*primary).pages.limits.lines.explicit);
        assert!((*primary).pages.total_rows < old_total_rows);
        assert!(!(*primary)
            .pages
            .limits
            .exceeded(&(*primary).pages, crate::pagelist::Key::Lines));
    }

    let limited_total_rows = unsafe { (*primary).pages.total_rows };
    t.set_scrollback_max_lines(None);
    unsafe {
        assert_eq!(usize::MAX, (*primary).pages.limits.lines.explicit);
        assert_eq!(limited_total_rows, (*primary).pages.total_rows);
    }
    for _ in 0..(3 * page_rows) {
        t.linefeed().unwrap();
    }
    unsafe {
        assert!((*primary).pages.total_rows - (*primary).pages.rows as usize > page_rows);
    }

    t.deinit(Global);
}

// ref: test "Terminal setScrollback only affects primary screen"
#[test]
fn terminal_set_scrollback_only_affects_primary_screen() {
    let mut t = Terminal::init(Global, opts(80, 3)).unwrap();

    let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
    let primary: *mut crate::screen::Screen =
        t.screens.get(crate::screen_set::Key::Primary).unwrap();
    let alternate: *mut crate::screen::Screen =
        t.screens.get(crate::screen_set::Key::Alternate).unwrap();

    t.set_scrollback_max_bytes(Some(123));
    t.set_scrollback_max_lines(Some(456));

    unsafe {
        assert_eq!(123usize, (*primary).pages.limits.bytes.explicit);
        assert_eq!(456usize, (*primary).pages.limits.lines.explicit);
        assert!(!(*primary).no_scrollback);

        assert_eq!(0usize, (*alternate).pages.limits.bytes.explicit);
        assert_eq!(usize::MAX, (*alternate).pages.limits.lines.explicit);
        assert!((*alternate).no_scrollback);
    }
    assert!(core::ptr::eq(alternate, t.screens.active));

    t.deinit(Global);
}

// ref: test "Terminal forwards optional scrollback limits"
#[test]
fn terminal_forwards_optional_scrollback_limits() {
    let max_lines: usize = 123;
    let mut t = Terminal::init(
        Global,
        Options {
            max_scrollback_bytes: None,
            max_scrollback_lines: Some(max_lines),
            ..opts(80, 24)
        },
    )
    .unwrap();

    assert_eq!(usize::MAX, unsafe {
        (*t.screens.active).pages.limits.bytes.explicit
    });
    assert_eq!(max_lines, unsafe {
        (*t.screens.active).pages.limits.lines.explicit
    });

    t.deinit(Global);
}

mod attrs;
mod cursor;
mod edit_chars;
mod edit_lines;
mod kitty_graphics;
mod misc;
mod print;
mod resize;
mod scroll;
mod semantic_prompt;
mod upstream_defects;
