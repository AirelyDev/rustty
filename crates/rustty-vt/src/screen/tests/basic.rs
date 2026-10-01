//! PORTS: ghostty src/terminal/Screen.zig — the `cursorCopy`, style, cursor-movement, scrollback-limit and read/write tests, plus the `dumpStringAlloc` unknown-point and tripwire-wiring guards.

use super::*;

// test "Screen cursorCopy style deref"
#[test]
fn screen_cursor_copy_style_deref() {
    let s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let mut s = s;

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

    // Bold should create our style
    s2.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert_eq!(1usize, unsafe { (*page).styles.count() });
    assert!(s2.cursor.style.flags.bold);

    // Copy default style, should release our style
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert!(!s2.cursor.style.flags.bold);
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    s.deinit();
    s2.deinit();
}

// PORT NOTE: no reference test. `Screen.clone` returns `!Screen` whose
// inferred error set is `PageList.clone`'s, which carries
// `Page.CloneFromError` (`StyleSetError || HyperlinkError ||
// GraphemeError`) out of `PageList.zig:1173`'s `try dst_page.cloneFrom(..)`.
// The port collapsed all of it into `OutOfMemory` (parity S133-F4); it now
// propagates `pagelist::access::CloneError` unchanged. Reaching a real
// `CloneFrom` failure needs a page whose style set cannot be rehashed into
// the destination, which no reference test constructs either; what IS
// pinned here is that the variant is no longer erased by the signature.
#[test]
fn screen_clone_error_carries_clone_from() {
    use crate::page::CloneFromError;
    use crate::pagelist::access::CloneError;

    // The `?` in `Screen::clone` performs exactly this conversion.
    let err: CloneError = CloneFromError::StyleSetNeedsRehash.into();
    assert!(matches!(
        err,
        CloneError::CloneFrom(CloneFromError::StyleSetNeedsRehash)
    ));
    assert!(!matches!(err, CloneError::OutOfMemory));
}

// PORT NOTE: no reference test. The reference's `errdefer self.cursor = old`
// in `cursorCopy` is exercised by a failing allocator inside
// `manualStyleUpdate`; the port has no swappable allocator there, so the
// `CURSOR_COPY_TW` fail point stands in (same rationale as `grow.rs`'s
// `GROW_TW`). Pins BOTH halves of parity S133-F3: the error variant is no
// longer flattened to `OutOfMemory`, and the cursor is restored WHOLE
// instead of being left half-applied.
#[test]
fn screen_cursor_copy_restores_the_whole_cursor_on_failure() {
    use rustty_util::tripwire::ResetMode;

    let opts = || Options {
        cols: 10,
        rows: 10,
        max_scrollback_bytes: Some(0),
        max_scrollback_lines: None,
        kitty_image_storage_limit: 0,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    };
    let mut s = Screen::init(Allocator, opts()).unwrap();
    let mut s2 = Screen::init(Allocator, opts()).unwrap();

    // Give the two cursors different observable state: `s` is bold at
    // (0, 0); `s2` is plain at (3, 4).
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s2.cursor_absolute(3, 4);
    let before_x = s2.cursor.x;
    let before_y = s2.cursor.y;
    let before_style = s2.cursor.style;
    let before_style_id = s2.cursor.style_id;
    let before_pin = s2.cursor.page_pin;
    assert!(!s2.cursor.style.flags.bold);

    // Fail the `manualStyleUpdate` the reference's errdefer guards, with
    // the variant the port used to throw away.
    CURSOR_COPY_TW.with(|tw| {
        tw.borrow_mut().errorAlways(
            CursorCopyFp::ManualStyleUpdate,
            pagelist::grow::IncreaseCapacityError::OutOfSpace,
        )
    });
    assert_eq!(
        Err(pagelist::grow::IncreaseCapacityError::OutOfSpace),
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true }),
    );
    CURSOR_COPY_TW
        .with(|tw| tw.borrow_mut().end(ResetMode::Reset))
        .unwrap();

    // The whole cursor is back, not a half-applied mixture of the two.
    assert_eq!(before_x, s2.cursor.x);
    assert_eq!(before_y, s2.cursor.y);
    assert_eq!(before_style_id, s2.cursor.style_id);
    assert!(core::ptr::eq(before_pin, s2.cursor.page_pin));
    assert_eq!(before_style.flags.bold, s2.cursor.style.flags.bold);
    assert!(!s2.cursor.style.flags.bold);

    // Disarmed, the same call succeeds and DOES apply the other cursor, so
    // the assertions above are about the failure path.
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert!(s2.cursor.style.flags.bold);
    assert_eq!(0, s2.cursor.x);
    assert_eq!(0, s2.cursor.y);

    s.deinit();
    s2.deinit();
}

// test "Screen cursorCopy style deref new page"
#[test]
fn screen_cursor_copy_style_deref_new_page() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(2048),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    // We need to get the cursor on a new page.
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s2.pages.pages.first).rows };

    // Fill the scrollback with blank lines until
    // there are only 5 rows left on the first page.
    unsafe { (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 5) {
        s2.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(false) };

    s2.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
        .unwrap();

    // This should be PAGE 1
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

    // It should be the last page in the list.
    assert!(std::ptr::eq(
        unsafe { crate::pagelist::Node::page(s2.pages.pages.last) } as *const _,
        page as *const _
    ));
    // It should have a previous page.
    assert!(!unsafe { (*(*s2.cursor.page_pin).node).prev }.is_null());

    // The cursor should be at 2, 9
    assert!(s2.cursor.x == 2);
    assert!(s2.cursor.y == 9);

    // Bold should create our style in page 1.
    s2.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert_eq!(1usize, unsafe { (*page).styles.count() });
    assert!(s2.cursor.style.flags.bold);

    // Copy the cursor for the first screen. This should release
    // the style from page 1 and move the cursor back to page 0.
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert!(!s2.cursor.style.flags.bold);
    assert_eq!(0usize, unsafe { (*page).styles.count() });
    // The page after the page the cursor is now in should be page 1.
    assert!(std::ptr::eq(page, unsafe {
        crate::pagelist::Node::page((*(*s2.cursor.page_pin).node).next)
    }));
    // The cursor should be at 0, 0
    assert!(s2.cursor.x == 0);
    assert!(s2.cursor.y == 0);

    s.deinit();
    s2.deinit();
}

// test "Screen cursorCopy style copy"
#[test]
fn screen_cursor_copy_style_copy() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert!(s2.cursor.style.flags.bold);
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    s.deinit();
    s2.deinit();
}

// test "Screen cursorCopy hyperlink deref"
#[test]
fn screen_cursor_copy_hyperlink_deref() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

    // Create a hyperlink for the cursor.
    s2.start_hyperlink(b"https://example.com/", None).unwrap();
    assert_eq!(1usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id != 0);

    // Copy a cursor with no hyperlink, should release our hyperlink.
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id == 0);

    s.deinit();
    s2.deinit();
}

// test "Screen cursorCopy hyperlink deref new page"
#[test]
fn screen_cursor_copy_hyperlink_deref_new_page() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(2048),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    // We need to get the cursor on a new page.
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s2.pages.pages.first).rows };

    // Fill the scrollback with blank lines until
    // there are only 5 rows left on the first page.
    unsafe { (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 5) {
        s2.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(false) };

    s2.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
        .unwrap();

    // This should be PAGE 1
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

    // It should be the last page in the list.
    assert!(std::ptr::eq(
        unsafe { crate::pagelist::Node::page(s2.pages.pages.last) } as *const _,
        page as *const _
    ));
    // It should have a previous page.
    assert!(!unsafe { (*(*s2.cursor.page_pin).node).prev }.is_null());

    // The cursor should be at 2, 9
    assert!(s2.cursor.x == 2);
    assert!(s2.cursor.y == 9);

    // Create a hyperlink for the cursor, should be in page 1.
    s2.start_hyperlink(b"https://example.com/", None).unwrap();
    assert_eq!(1usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id != 0);

    // Copy the cursor for the first screen. This should release
    // the hyperlink from page 1 and move the cursor back to page 0.
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id == 0);
    // The page after the page the cursor is now in should be page 1.
    assert!(std::ptr::eq(page, unsafe {
        crate::pagelist::Node::page((*(*s2.cursor.page_pin).node).next)
    }));
    // The cursor should be at 0, 0
    assert!(s2.cursor.x == 0);
    assert!(s2.cursor.y == 0);

    s.deinit();
    s2.deinit();
}

// test "Screen cursorCopy hyperlink copy"
#[test]
fn screen_cursor_copy_hyperlink_copy() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    // Create a hyperlink for the cursor.
    s.start_hyperlink(b"https://example.com/", None).unwrap();
    assert_eq!(1usize, unsafe {
        (*crate::pagelist::Node::page((*s.cursor.page_pin).node))
            .hyperlink_set
            .count()
    });
    assert!(s.cursor.hyperlink_id != 0);

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

    assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id == 0);

    // Copy the cursor with the hyperlink.
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
        .unwrap();
    assert_eq!(1usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id != 0);

    s.deinit();
    s2.deinit();
}

// test "Screen cursorCopy hyperlink copy disabled"
#[test]
fn screen_cursor_copy_hyperlink_copy_disabled() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    // Create a hyperlink for the cursor.
    s.start_hyperlink(b"https://example.com/", None).unwrap();
    assert_eq!(1usize, unsafe {
        (*crate::pagelist::Node::page((*s.cursor.page_pin).node))
            .hyperlink_set
            .count()
    });
    assert!(s.cursor.hyperlink_id != 0);

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

    assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id == 0);

    // Copy the cursor with the hyperlink.
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: false })
        .unwrap();
    assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
    assert!(s2.cursor.hyperlink_id == 0);

    s.deinit();
    s2.deinit();
}

// test "Screen style basics"
#[test]
fn screen_style_basics() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    // Set a new style
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert!(s.cursor.style_id != 0);
    assert_eq!(1usize, unsafe { (*page).styles.count() });
    assert!(s.cursor.style.flags.bold);

    // Set another style, we should still only have one since it was unused
    s.set_attribute(crate::sgr::Attribute::Italic).unwrap();
    assert!(s.cursor.style_id != 0);
    assert_eq!(1usize, unsafe { (*page).styles.count() });
    assert!(s.cursor.style.flags.italic);

    s.deinit();
}

// test "Screen style reset to default"
#[test]
fn screen_style_reset_to_default() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    // Set a new style
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert!(s.cursor.style_id != 0);
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    // Reset to default
    s.set_attribute(crate::sgr::Attribute::ResetBold).unwrap();
    assert!(s.cursor.style_id == 0);
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    s.deinit();
}

// test "Screen style reset with unset"
#[test]
fn screen_style_reset_with_unset() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    // Set a new style
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert!(s.cursor.style_id != 0);
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    // Reset to default
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
    assert!(s.cursor.style_id == 0);
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    s.deinit();
}

// test "Screen: cursorDown across pages preserves style"
#[test]
fn screen_cursor_down_across_pages_preserves_style() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 3,
            max_scrollback_bytes: Some(1),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    // Scroll down enough to go to another page
    let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
    let rem = unsafe { (*start_page).capacity.rows };
    unsafe { (*start_page).pause_integrity_checks(true) };
    for _ in 0..rem {
        s.cursor_down_or_scroll().unwrap();
    }
    unsafe { (*start_page).pause_integrity_checks(false) };

    // We need our page to change for this test o make sense. If this
    // assertion fails then the bug is in the test: we should be scrolling
    // above enough for a new page to show up.
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(!std::ptr::eq(start_page, page));
    }

    // Scroll back to the previous page
    s.cursor_up(1);
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(std::ptr::eq(start_page, page));
    }

    // Go back up, set a style
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    // Go back down into the next page and we should have that style
    s.cursor_down(1);
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    s.deinit();
}

// test "Screen: cursorUp across pages preserves style"
#[test]
fn screen_cursor_up_across_pages_preserves_style() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 3,
            max_scrollback_bytes: Some(1),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
    let rem = unsafe { (*start_page).capacity.rows };
    unsafe { (*start_page).pause_integrity_checks(true) };
    for _ in 0..rem {
        s.cursor_down_or_scroll().unwrap();
    }
    unsafe { (*start_page).pause_integrity_checks(false) };

    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(!std::ptr::eq(start_page, page));
    }

    // Go back up, set a style
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    // Go back down into the prev page and we should have that style
    s.cursor_up(1);
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(std::ptr::eq(start_page, page));

        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    s.deinit();
}

// test "Screen: cursorAbsolute across pages preserves style"
#[test]
fn screen_cursor_absolute_across_pages_preserves_style() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 3,
            max_scrollback_bytes: Some(1),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
    let rem = unsafe { (*start_page).capacity.rows };
    unsafe { (*start_page).pause_integrity_checks(true) };
    for _ in 0..rem {
        s.cursor_down_or_scroll().unwrap();
    }
    unsafe { (*start_page).pause_integrity_checks(false) };

    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(!std::ptr::eq(start_page, page));
    }

    // Go back up, set a style
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    // Go back down into the prev page and we should have that style
    s.cursor_absolute(1, 1);
    {
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(std::ptr::eq(start_page, page));

        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    s.deinit();
}

// test "Screen: cursorAbsolute to page with insufficient capacity"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn screen_cursor_absolute_to_page_with_insufficient_capacity() {
    // This test checks for a very specific edge case
    // which previously resulted in memory corruption.
    //
    // The conditions for this edge case are as such:
    // - The cursor has an associated style or other managed memory.
    // - The cursor moves to a different page.
    // - The new page is at capacity and must have its capacity adjusted.
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 3,
            max_scrollback_bytes: Some(1),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    // Scroll down enough to go to another page
    let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
    let rem = unsafe { (*start_page).capacity.rows };
    unsafe { (*start_page).pause_integrity_checks(true) };
    for _ in 0..rem {
        s.cursor_down_or_scroll().unwrap();
    }
    unsafe { (*start_page).pause_integrity_checks(false) };

    let new_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };

    // We need our page to change for this test to make sense. If this
    // assertion fails then the bug is in the test: we should be scrolling
    // above enough for a new page to show up.
    assert!(!std::ptr::eq(start_page, new_page));

    // Add styles to the start page until it reaches capacity.
    {
        // Pause integrity checks because they're slow and
        // we're not testing this, this is just setup.
        unsafe { (*start_page).pause_integrity_checks(true) };

        let mut n: u32 = 1;
        loop {
            let st = style::Style {
                bg_color: style::Color::Rgb(crate::color::RGB {
                    r: (n & 0xFF) as u8,
                    g: ((n >> 8) & 0xFF) as u8,
                    b: ((n >> 16) & 0xFF) as u8,
                }),
                fg_color: style::Color::None,
                underline_color: style::Color::None,
                flags: style::Flags::default(),
            };
            let res = unsafe {
                let base = (*start_page).memory_ptr();
                (*start_page).styles.add(base, st)
            };
            if res.is_ok() {
                n += 1;
            } else {
                break;
            }
        }

        unsafe { (*start_page).pause_integrity_checks(false) };
        unsafe { (*start_page).assert_integrity() };
    }

    // Set a style on the cursor.
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    {
        let styleval = unsafe {
            let base = (*new_page).memory_ptr();
            &*(*new_page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    // Go back up into the start page and we should still have that style.
    s.cursor_absolute(1, 1);
    {
        let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        // The page we're on now should NOT equal start_page, since its
        // capacity should have been adjusted, which invalidates our ptr.
        assert!(!std::ptr::eq(start_page, cur_page));
        // To make sure we DID change pages we check we're not on new_page.
        assert!(!std::ptr::eq(new_page, cur_page));

        let styleval = unsafe {
            let base = (*cur_page).memory_ptr();
            &*(*cur_page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
    }

    unsafe { (*crate::pagelist::Node::page((*s.cursor.page_pin).node)).assert_integrity() };
    unsafe { (*new_page).assert_integrity() };

    s.deinit();
}

// PORT NOTE: these tests were DEFERRED from vt-11a because they require
// `dumpStringAlloc`, which lands here in vt-11b alongside the formatter
// production code.

// ref: test "Screen forwards optional scrollback limits"
#[test]
fn screen_forwards_optional_scrollback_limits() {
    let max_lines: usize = 123;
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: None,
            max_scrollback_lines: Some(max_lines),
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    assert_eq!(usize::MAX, s.pages.limits.bytes.explicit);
    assert_eq!(max_lines, s.pages.limits.lines.explicit);
    assert!(!s.no_scrollback);

    s.deinit();
}

// test "Screen reset cursor pin is not garbage" (oracle `d30a9c424`)
#[test]
fn screen_reset_cursor_pin_is_not_garbage() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    s.test_write_string(b"hello, world").unwrap();

    // The page reset marks every tracked pin garbage but the screen
    // keeps using the cursor pin, so it must come back clean: anything
    // that copies it (e.g. Kitty image placements) would otherwise be
    // born garbage and reaped.
    s.reset();
    assert!(!unsafe { (*s.cursor.page_pin).garbage });

    s.deinit();
}

// PORT NOTE (rustty-only guard, `P3-W3` T2 Step 3): `10bc43420` makes the
// byte limit OPTIONAL, so `Some(0)` ("disable scrollback") and `None`
// ("unlimited") are now two DIFFERENT encodings where the pinned tree had
// only one `usize` and no way to say "unlimited" at all. A two-case test
// cannot tell the new encoding from the old, so this asserts all THREE
// behaviours against the same growth workload.
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn screen_scrollback_bytes_zero_none_and_finite_are_distinct() {
    fn sopts(bytes: Option<usize>) -> Options {
        Options {
            cols: 80,
            rows: 1,
            max_scrollback_bytes: bytes,
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // (a) Some(0) DISABLES scrollback: the screen reports no_scrollback and
    //     the pagelist keeps only its structural minimum.
    let mut zero = Screen::init(Allocator, sopts(Some(0))).unwrap();
    assert!(zero.no_scrollback);
    assert_eq!(0usize, zero.pages.limits.bytes.explicit);

    let page_rows: usize =
        unsafe { crate::pagelist::Node::capacity(zero.pages.pages.first) }.rows as usize;
    let grow: usize = 20 * page_rows;
    zero.pages.grow_rows(grow).unwrap();
    assert!(zero.pages.page_size <= zero.pages.limits.max(Key::Bytes));
    let zero_rows = zero.pages.total_rows;

    // (b) None is UNLIMITED: scrollback is enabled and growth is unbounded.
    let mut unlimited = Screen::init(Allocator, sopts(None)).unwrap();
    assert!(!unlimited.no_scrollback);
    assert_eq!(usize::MAX, unlimited.pages.limits.bytes.explicit);
    unlimited.pages.grow_rows(grow).unwrap();
    assert_eq!(grow + 1, unlimited.pages.total_rows);
    let unlimited_rows = unlimited.pages.total_rows;

    // (c) Some(n) PRUNES at n: scrollback is enabled but bounded.
    let bounded_limit = 2 * zero.pages.limits.bytes.min;
    let mut bounded = Screen::init(Allocator, sopts(Some(bounded_limit))).unwrap();
    assert!(!bounded.no_scrollback);
    assert_eq!(bounded_limit, bounded.pages.limits.bytes.explicit);
    bounded.pages.grow_rows(grow).unwrap();
    assert!(bounded.pages.page_size <= bounded.pages.limits.max(Key::Bytes));
    let bounded_rows = bounded.pages.total_rows;

    // The three encodings must be strictly ordered; any two of them
    // collapsing would mean the optional encoding is not being honored.
    assert!(zero_rows < bounded_rows, "{zero_rows} < {bounded_rows}");
    assert!(
        bounded_rows < unlimited_rows,
        "{bounded_rows} < {unlimited_rows}"
    );

    bounded.deinit();
    unlimited.deinit();
    zero.deinit();
}

// test "Screen read and write"
#[test]
fn screen_read_and_write() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    assert_eq!(s.cursor.style_id, 0 as style::Id);

    s.test_write_string(b"hello, world").unwrap();
    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"hello, world");

    s.deinit();
}

// (no reference test) PORT-ADDITIVE proving test (W29-B02): invalid UTF-8
// must surface as `PrintStringError::InvalidUtf8` — the Zig
// `try std.unicode.Utf8View.init` error — not `OutOfMemory`, and must
// write nothing (validation happens before any codepoint is written).
#[test]
fn screen_test_write_string_invalid_utf8() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    assert_eq!(
        s.test_write_string(b"foo\xFFbar"),
        Err(PrintStringError::InvalidUtf8)
    );

    // Nothing was written: validation fails before iteration begins.
    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"");

    // Valid input still writes (regression arm).
    s.test_write_string(b"hello, world").unwrap();
    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"hello, world");

    s.deinit();
}

// test "Screen read and write newline"
#[test]
fn screen_read_and_write_newline() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    assert_eq!(s.cursor.style_id, 0 as style::Id);

    s.test_write_string(b"hello\nworld").unwrap();
    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"hello\nworld");

    s.deinit();
}

// test "Screen read and write scrollback"
#[test]
fn screen_read_and_write_scrollback() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 2,
            max_scrollback_bytes: Some(1000),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    s.test_write_string(b"hello\nworld\ntest").unwrap();
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"hello\nworld\ntest");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"world\ntest");
    }

    s.deinit();
}

// test "Screen read and write no scrollback small"
#[test]
fn screen_read_and_write_no_scrollback_small() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 2,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    s.test_write_string(b"hello\nworld\ntest").unwrap();
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"world\ntest");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"world\ntest");
    }

    s.deinit();
}

// test "Screen read and write no scrollback large"
#[test]
fn screen_read_and_write_no_scrollback_large() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 2,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();

    for i in 0..1_000u32 {
        // PORT NOTE: `std.fmt.bufPrint(&buf, "{}\n", .{i})`.
        let str = format!("{}\n", i);
        s.test_write_string(str.as_bytes()).unwrap();
    }
    s.test_write_string(b"1000").unwrap();

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"999\n1000");
    }

    s.deinit();
}

// test "Screen cursorCopy x/y"
#[test]
fn screen_cursor_copy_x_y() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    s.cursor_absolute(2, 3);
    assert!(s.cursor.x == 2);
    assert!(s.cursor.y == 3);

    let mut s2 = Screen::init(
        Allocator,
        Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: false })
        .unwrap();
    assert!(s2.cursor.x == 2);
    assert!(s2.cursor.y == 3);
    s2.test_write_string(b"Hello").unwrap();

    {
        let str = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"\n\n\n  Hello");
    }

    s.deinit();
    s2.deinit();
}

// PORT NOTE: no reference test; this pins the `error.UnknownPoint` arm of
// `dumpStringAlloc`/`dumpStringAllocUnwrapped` (Screen.zig:3616, :3635),
// which is reachable for a `.history` point when there is no scrollback
// (PageList.getBottomRight(.history) returns null because the active
// top-left has no row above it).
#[test]
fn screen_dump_string_alloc_history_unknown_point() {
    let mut s = mkscreen(10, 3, 0);

    // No scrollback: there is no history region, so both helpers report
    // `UnknownPoint` rather than collapsing onto the OOM variant.
    assert_eq!(
        Err(DumpStringAllocError::UnknownPoint),
        s.dump_string_alloc(
            Allocator,
            point::Point::History(point::Coordinate::default())
        )
    );
    assert_eq!(
        Err(DumpStringAllocError::UnknownPoint),
        s.dump_string_alloc_unwrapped(
            Allocator,
            point::Point::History(point::Coordinate::default())
        )
    );
    s.deinit();

    // Positive control: once rows have scrolled into history, the same
    // point resolves and the dump succeeds.
    let mut s = mkscreen(10, 3, 1000);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP").unwrap();
    assert_eq!(
        Ok(b"1ABCD".to_vec()),
        s.dump_string_alloc(
            Allocator,
            point::Point::History(point::Coordinate::default())
        )
    );
    assert_eq!(
        Ok(b"1ABCD".to_vec()),
        s.dump_string_alloc_unwrapped(
            Allocator,
            point::Point::History(point::Coordinate::default())
        )
    );
    s.deinit();
}

// PORT PARITY GUARD (no Zig twin — it needs none): Zig's
// `tripwire.zig: pub const enabled = builtin.is_test` is true for every test
// build at EVERY optimize mode, so the reference's allocation-failure tests
// inject in `zig build test -Doptimize=ReleaseFast` exactly as in Debug. This
// crate reproduces that cross-crate by enabling `rustty-util/tripwire-test` as
// a DEV-dependency, which is what keeps the fail-point tests in this module
// out of the release lane's `ignored` bucket. Asserting it directly means the
// wiring cannot be silently dropped (a `cargo test --release` run would
// otherwise just report the tests as "ignored" and stay green).
//
// (f1948d505 removed the `selectionString_tw` module this guard originally
// named, and parity tail S7-3 removed the `StartHyperlinkFp` stand-in that
// replaced it; PageList's `init_tw` is the same idiom and shares the wiring.)
#[test]
fn tripwire_fail_points_enabled_in_test_builds_at_any_profile() {
    assert!(
        rustty_util::tripwire::Module::<crate::pagelist::InitFp, OutOfMemory>::enabled(),
        "tripwire fail points must be armed in ALL test builds (Zig: \
         `enabled = builtin.is_test`); if this fires in `--release`, the \
         `rustty-util/tripwire-test` dev-dependency feature was lost"
    );
}
