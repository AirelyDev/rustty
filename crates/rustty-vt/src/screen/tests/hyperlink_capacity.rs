//! PORTS: ghostty src/terminal/Screen.zig — the hyperlink tests and the `increaseCapacity`/`splitForCapacity` capacity tests.

use super::*;

// test "Screen: hyperlink start/end"
#[test]
fn screen_hyperlink_start_end() {
    let mut s = mkscreen(5, 5, 0);
    assert!(s.cursor.hyperlink_id == 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(0usize, page.hyperlink_set.count());
    }

    s.start_hyperlink(b"http://example.com", None).unwrap();
    assert!(s.cursor.hyperlink_id != 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(1usize, page.hyperlink_set.count());
    }

    s.end_hyperlink();
    assert!(s.cursor.hyperlink_id == 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(0usize, page.hyperlink_set.count());
    }
    s.deinit();
}

// test "Screen: hyperlink accepts its current values"
#[test]
fn screen_hyperlink_accepts_its_current_values() {
    let mut s = mkscreen(5, 5, 0);

    s.start_hyperlink(b"http://example.com", Some(b"current"))
        .unwrap();

    // PORT NOTE: the reference passes `current.uri` / `current.id.explicit`
    // straight back into `s.startHyperlink`. Rust's borrow checker forbids
    // holding `&s.cursor.hyperlink` across the `&mut self` call, so we read
    // the current values through the RAW pointer the cursor already stores
    // (`cursor.hyperlink: Option<*mut Hyperlink>`) — the same aliasing the
    // reference test exercises, expressed in the way this crate allows.
    //
    // PORT NOTE (P3 F2): the slices stay RAW (`*const [u8]`) all the way
    // into `start_hyperlink_raw`. Materializing them as `&[u8]` arguments
    // would strongly protect them for the duration of the call, and the
    // call releases the very allocation they point into — see the PORT NOTE
    // on `Screen::start_hyperlink_raw`. This is the aliasing the reference
    // test exists to exercise, so the test keeps it.
    let link: *mut CursorHyperlink = s.cursor.hyperlink.unwrap();
    let (uri_ptr, uri_len) = unsafe { ((*link).uri.as_ptr(), (*link).uri.len()) };
    let (id_ptr, id_len) = match unsafe { &(*link).id } {
        crate::hyperlink::HyperlinkId::Explicit(v) => (v.as_ptr(), v.len()),
        crate::hyperlink::HyperlinkId::Implicit(_) => unreachable!(),
    };
    let uri: *const [u8] = core::ptr::slice_from_raw_parts(uri_ptr, uri_len);
    let id: *const [u8] = core::ptr::slice_from_raw_parts(id_ptr, id_len);
    unsafe { s.start_hyperlink_raw(uri, Some(id)) }.unwrap();

    let link: *mut CursorHyperlink = s.cursor.hyperlink.unwrap();
    assert_eq!(b"http://example.com".as_slice(), unsafe {
        &(&(*link).uri)[..]
    });
    match unsafe { &(*link).id } {
        crate::hyperlink::HyperlinkId::Explicit(v) => {
            assert_eq!(b"current".as_slice(), &v[..])
        }
        crate::hyperlink::HyperlinkId::Implicit(_) => unreachable!(),
    }

    s.deinit();
}

// test "Screen: implicit hyperlink ID wraps" (success half)
#[test]
fn screen_implicit_hyperlink_id_wraps() {
    let mut s = mkscreen(5, 5, 0);

    s.cursor.hyperlink_implicit_id = size::OffsetInt::MAX;
    s.start_hyperlink(b"http://example.com", None).unwrap();

    assert_eq!(0 as size::OffsetInt, s.cursor.hyperlink_implicit_id);
    let link: *mut CursorHyperlink = s.cursor.hyperlink.unwrap();
    match unsafe { &(*link).id } {
        crate::hyperlink::HyperlinkId::Implicit(v) => {
            assert_eq!(size::OffsetInt::MAX, *v)
        }
        crate::hyperlink::HyperlinkId::Explicit(_) => unreachable!(),
    }

    s.deinit();
}

// test "Screen: implicit hyperlink ID wraps" (allocation-failure rollback half)
//
// A failed allocation must roll the wrapped counter back to its
// original value as well.
#[test]
// WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
// fail point. `tripwire::Module::enabled()` is the union of `cfg!(test)`,
// `cfg!(debug_assertions)` and the `tripwire-test` feature, which this crate
// turns on as a DEV-dependency (Cargo.toml) — reproducing Zig's
// `builtin.is_test`, true at ANY optimize mode. So this runs in BOTH the debug
// and the release test lanes, like the reference test does. No `ignore`.
fn screen_implicit_hyperlink_id_wrap_rolls_back_on_allocation_failure() {
    let mut s = mkscreen(5, 5, 0);

    s.cursor.hyperlink_implicit_id = size::OffsetInt::MAX;
    // var failing = testing.FailingAllocator.init(alloc, .{});
    // failing.fail_index = failing.alloc_index;
    let failing = crate::alloc::testing::FailingAllocator::over_heap(
        crate::alloc::testing::Config::default(),
    );
    failing.fail_index.set(failing.alloc_index.get());
    let c = failing.allocator();
    crate::alloc::testing::with_handle(&c, |failing_alloc| {
        // const original_alloc = s.alloc;
        // defer s.alloc = original_alloc;
        // s.alloc = failing.allocator();
        let original_alloc = s.alloc;
        s.alloc = failing_alloc;
        assert_eq!(
            Err(pagelist::grow::IncreaseCapacityError::OutOfMemory),
            s.start_hyperlink(b"http://example.com", None)
        );
        s.alloc = original_alloc;
    });
    assert!(failing.has_induced_failure.get());

    assert_eq!(size::OffsetInt::MAX, s.cursor.hyperlink_implicit_id);

    s.deinit();
}

// test "Screen: hyperlink reuse"
#[test]
fn screen_hyperlink_reuse() {
    let mut s = mkscreen(5, 5, 0);

    assert!(s.cursor.hyperlink_id == 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(0usize, page.hyperlink_set.count());
    }

    // Use it for the first time
    s.start_hyperlink(b"http://example.com", None).unwrap();
    assert!(s.cursor.hyperlink_id != 0);
    let id = s.cursor.hyperlink_id;

    // Reuse the same hyperlink, expect we have the same ID
    s.start_hyperlink(b"http://example.com", None).unwrap();
    assert_eq!(id, s.cursor.hyperlink_id);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(1usize, page.hyperlink_set.count());
    }

    s.end_hyperlink();
    assert!(s.cursor.hyperlink_id == 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(0usize, page.hyperlink_set.count());
    }
    s.deinit();
}

// test "Screen: hyperlink cursor state on resize"
#[test]
fn screen_hyperlink_cursor_state_on_resize() {
    // This test depends on underlying PageList implementation so
    // it may be invalid one day. It's here to document/verify the
    // current behavior.

    let mut s = mkscreen(5, 10, 0);

    // Start a hyperlink
    s.start_hyperlink(b"http://example.com", None).unwrap();
    assert!(s.cursor.hyperlink_id != 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(1usize, page.hyperlink_set.count());
    }

    // Resize. Any column growth will trigger a page to be reallocated.
    s.resize(Resize {
        cols: 10,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    assert!(s.cursor.hyperlink_id != 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(1usize, page.hyperlink_set.count());
    }

    s.end_hyperlink();
    assert!(s.cursor.hyperlink_id == 0);
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        assert_eq!(0usize, page.hyperlink_set.count());
    }
    s.deinit();
}

// test "Screen: cursorSetHyperlink OOM + URI too large for string alloc"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn screen_cursor_set_hyperlink_oom_uri_too_large_for_string_alloc() {
    let mut s = mkscreen(80, 24, 0);

    // Start a hyperlink with a URI that just barely fits in the string alloc.
    // This will ensure that additional string alloc space is needed for the
    // redundant copy of the URI when the page is re-alloced.
    let uri = vec![b'a'; pagepkg::std_capacity().string_bytes as usize - 8];
    s.start_hyperlink(&uri, None).unwrap();

    // Figure out how many cells should can have hyperlinks in this page,
    // and write twice that number, to guarantee the capacity needs to be
    // increased at some point.
    let base_capacity =
        unsafe { (*crate::pagelist::Node::page((*s.cursor.page_pin).node)).hyperlink_capacity() };
    let base_string_bytes =
        unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).string_bytes };
    for _ in 0..(base_capacity * 2) {
        s.cursor_set_hyperlink().unwrap();
        if s.cursor.x >= s.pages.cols - 1 {
            s.cursor_down_or_scroll().unwrap();
            s.cursor_horizontal_absolute(0);
        } else {
            s.cursor_right(1);
        }
    }

    // Make sure the capacity really did increase.
    assert!(
        base_capacity
            < unsafe {
                (*crate::pagelist::Node::page((*s.cursor.page_pin).node)).hyperlink_capacity()
            }
    );
    // And that our string_bytes increased as well.
    assert!(
        base_string_bytes
            < unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).string_bytes }
    );
    s.deinit();
}

// test "Screen: increaseCapacity cursor style ref count preserved"
#[test]
fn screen_increase_capacity_cursor_style_ref_count_preserved() {
    let mut s = mkscreen(5, 5, 0);
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"1ABCD").unwrap();

    // We should have one page and it should be our cursor page
    assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    assert!(std::ptr::eq(s.pages.pages.first, unsafe {
        (*s.cursor.page_pin).node
    }));

    let old_style = s.cursor.style;

    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
        // 5 chars + cursor = 6 refs
        assert_eq!(6, unsafe {
            let base = page.memory_ptr();
            page.styles.ref_count(base, s.cursor.style_id)
        });
    }

    // This forces the page to change via increaseCapacity.
    let new_node = s
        .increase_capacity(
            unsafe { (*s.cursor.page_pin).node },
            Some(pagelist::grow::IncreaseCapacity::GraphemeBytes),
        )
        .unwrap();

    // Cursor's page_pin should now point to the new node
    assert!(std::ptr::eq(unsafe { (*s.cursor.page_pin).node }, new_node));

    // Verify cursor's page_cell and page_row are correctly reloaded from the pin
    let page_rac = unsafe { (*s.cursor.page_pin).row_and_cell() };
    assert!(std::ptr::eq(s.cursor.page_row, page_rac.row));
    assert!(std::ptr::eq(s.cursor.page_cell, page_rac.cell));

    // Style should be preserved
    assert!(old_style.eql(&s.cursor.style));
    assert!(s.cursor.style_id != style::DEFAULT_ID);

    // After increaseCapacity, the 5 chars are cloned (5 refs) and
    // the cursor's style is re-added (1 ref) = 6 total.
    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
        let ref_count = unsafe {
            let base = page.memory_ptr();
            page.styles.ref_count(base, s.cursor.style_id)
        };
        assert_eq!(6, ref_count);
    }
    s.deinit();
}

// test "Screen: increaseCapacity cursor hyperlink ref count preserved"
#[test]
fn screen_increase_capacity_cursor_hyperlink_ref_count_preserved() {
    let mut s = mkscreen(5, 5, 0);
    s.start_hyperlink(b"https://example.com/", None).unwrap();
    s.test_write_string(b"1ABCD").unwrap();

    // We should have one page and it should be our cursor page
    assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    assert!(std::ptr::eq(s.pages.pages.first, unsafe {
        (*s.cursor.page_pin).node
    }));

    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
        // Cursor has the hyperlink active = 1 count in hyperlink_set
        assert_eq!(1usize, page.hyperlink_set.count());
        assert!(s.cursor.hyperlink_id != 0);
        assert!(s.cursor.hyperlink.is_some());
    }

    // This forces the page to change via increaseCapacity.
    let _ = s
        .increase_capacity(
            unsafe { (*s.cursor.page_pin).node },
            Some(pagelist::grow::IncreaseCapacity::GraphemeBytes),
        )
        .unwrap();

    // Hyperlink should be preserved with correct URI
    assert!(s.cursor.hyperlink.is_some());
    assert!(s.cursor.hyperlink_id != 0);
    assert_eq!(
        b"https://example.com/".as_slice(),
        unsafe { &(*s.cursor.hyperlink.unwrap()).uri }.as_slice()
    );

    // After increaseCapacity, the hyperlink is re-added to the new page.
    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
        assert_eq!(1usize, page.hyperlink_set.count());
    }
    s.deinit();
}

// test "Screen: increaseCapacity cursor with both style and hyperlink preserved"
#[test]
fn screen_increase_capacity_cursor_with_both_style_and_hyperlink_preserved() {
    let mut s = mkscreen(5, 5, 0);

    // Set both a non-default style AND an active hyperlink.
    // Write one character first with bold to mark the row as styled,
    // then start the hyperlink and write more characters.
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.start_hyperlink(b"https://example.com/", None).unwrap();
    s.test_write_string(b"1ABCD").unwrap();

    // We should have one page and it should be our cursor page
    assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    assert!(std::ptr::eq(s.pages.pages.first, unsafe {
        (*s.cursor.page_pin).node
    }));

    let old_style = s.cursor.style;

    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
        // 5 chars + cursor = 6 refs for bold style
        assert_eq!(6, unsafe {
            let base = page.memory_ptr();
            page.styles.ref_count(base, s.cursor.style_id)
        });
        // Cursor has the hyperlink active = 1 count in hyperlink_set
        assert_eq!(1usize, page.hyperlink_set.count());
        assert!(s.cursor.style_id != style::DEFAULT_ID);
        assert!(s.cursor.hyperlink_id != 0);
        assert!(s.cursor.hyperlink.is_some());
    }

    // This forces the page to change via increaseCapacity.
    let _ = s
        .increase_capacity(
            unsafe { (*s.cursor.page_pin).node },
            Some(pagelist::grow::IncreaseCapacity::GraphemeBytes),
        )
        .unwrap();

    // Style should be preserved
    assert!(old_style.eql(&s.cursor.style));
    assert!(s.cursor.style_id != style::DEFAULT_ID);

    // Hyperlink should be preserved with correct URI
    assert!(s.cursor.hyperlink.is_some());
    assert!(s.cursor.hyperlink_id != 0);
    assert_eq!(
        b"https://example.com/".as_slice(),
        unsafe { &(*s.cursor.hyperlink.unwrap()).uri }.as_slice()
    );

    // After increaseCapacity, both style and hyperlink are re-added to the new page.
    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
        let ref_count = unsafe {
            let base = page.memory_ptr();
            page.styles.ref_count(base, s.cursor.style_id)
        };
        assert_eq!(6, ref_count);
        assert_eq!(1usize, page.hyperlink_set.count());
    }
    s.deinit();
}

// test "Screen: increaseCapacity non-cursor page returns early"
#[test]
fn screen_increase_capacity_non_cursor_page_returns_early() {
    // Test that calling increaseCapacity on a page that is NOT the cursor's
    // page properly delegates to pages.increaseCapacity without doing the
    // extra cursor accounting (style/hyperlink re-adding).
    let mut s = mkscreen(80, 24, 10000);

    // Set up a custom style and hyperlink on the cursor
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.start_hyperlink(b"https://example.com/", None).unwrap();
    s.test_write_string(b"Hello").unwrap();

    // Store cursor state before growing pages
    let old_style = s.cursor.style;
    let old_style_id = s.cursor.style_id;
    let old_hyperlink = s.cursor.hyperlink;
    let old_hyperlink_id = s.cursor.hyperlink_id;

    // The cursor is on the first (and only) page
    assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));

    // Grow pages until we have multiple pages. The cursor's pin stays on
    // the first page since we're just adding rows.
    let first_page_node = s.pages.pages.first;
    unsafe { (*crate::pagelist::Node::page(first_page_node)).pause_integrity_checks(true) };
    let n_grow = unsafe {
        crate::pagelist::Node::capacity(first_page_node).rows
            - crate::pagelist::Node::rows(first_page_node)
    };
    for _ in 0..n_grow {
        let _ = s.pages.grow().unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(first_page_node)).pause_integrity_checks(false) };
    let _ = s.pages.grow().unwrap();

    // Now we have two pages
    assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    let second_page = s.pages.pages.last;

    // Cursor should still be on the first page (where it was created)
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));
    assert!(!std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        second_page
    ));

    let second_page_styles_cap = unsafe { crate::pagelist::Node::capacity(second_page).styles };
    let cursor_page_styles_cap =
        unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).styles };

    // Call increaseCapacity on the second page (NOT the cursor's page)
    let new_second_page = s
        .increase_capacity(second_page, Some(pagelist::grow::IncreaseCapacity::Styles))
        .unwrap();

    // The second page should have increased capacity
    assert_eq!(second_page_styles_cap * 2, unsafe {
        crate::pagelist::Node::capacity(new_second_page).styles
    });

    // The cursor's page (first page) should be unchanged
    assert_eq!(cursor_page_styles_cap, unsafe {
        crate::pagelist::Node::capacity((*s.cursor.page_pin).node).styles
    });

    // Cursor state should be completely unchanged since we didn't touch its page
    assert!(old_style.eql(&s.cursor.style));
    assert_eq!(old_style_id, s.cursor.style_id);
    assert_eq!(old_hyperlink, s.cursor.hyperlink);
    assert_eq!(old_hyperlink_id, s.cursor.hyperlink_id);

    // Verify hyperlink is still valid
    assert!(s.cursor.hyperlink.is_some());
    assert_eq!(
        b"https://example.com/".as_slice(),
        unsafe { &(*s.cursor.hyperlink.unwrap()).uri }.as_slice()
    );
    s.deinit();
}

// test "Screen: cursorDown to page with insufficient capacity"
#[test]
fn screen_cursor_down_to_page_with_insufficient_capacity() {
    // Regression test for https://github.com/ghostty-org/ghostty/issues/10282
    //
    // This test exposes a use-after-realloc bug in cursorDown (and similar
    // cursor movement functions). The bug pattern:
    //
    // 1. cursorDown creates a by-value copy of the pin via page_pin.down(n)
    // 2. cursorChangePin is called, which may trigger increaseCapacity
    //    if the target page's style map is full
    // 3. increaseCapacity frees the old page and creates a new one
    // 4. The local pin copy still points to the freed page
    // 5. rowAndCell() on the stale pin accesses freed memory

    // Small screen to make page boundary crossing easy to set up
    let mut s = mkscreen(10, 3, 1);

    // Scroll down enough to create a second page
    let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
    let rem = unsafe { (*start_page).capacity.rows };
    unsafe { (*start_page).pause_integrity_checks(true) };
    for _ in 0..rem {
        s.cursor_down_or_scroll().unwrap();
    }
    unsafe { (*start_page).pause_integrity_checks(false) };

    // Cursor should now be on a new page
    let new_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert!(!std::ptr::eq(start_page, new_page));

    // Fill new_page's style map to capacity. When we move INTO this page
    // with a style set, increaseCapacity will be triggered.
    {
        unsafe { (*new_page).pause_integrity_checks(true) };

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
                let base = (*new_page).memory_ptr();
                (*new_page).styles.add(base, st)
            };
            if res.is_ok() {
                n += 1;
            } else {
                break;
            }
        }

        unsafe { (*new_page).pause_integrity_checks(false) };
        unsafe { (*new_page).assert_integrity() };
    }

    // Move cursor to start of active area and set a style
    s.cursor_absolute(0, 0);
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert!(s.cursor.style.flags.bold);
    assert!(s.cursor.style_id != style::DEFAULT_ID);

    // Find the row just before the page boundary
    let mut found = false;
    for row in 0..(s.pages.rows - 1) {
        s.cursor_absolute(0, row);
        let cur_node = unsafe { (*s.cursor.page_pin).node };
        if let Some(next_pin) = unsafe { (*s.cursor.page_pin).down(1) } {
            if !std::ptr::eq(next_pin.node, cur_node) {
                // Cursor is at 'row', moving down crosses to new_page
                assert!(std::ptr::eq(
                    unsafe { crate::pagelist::Node::page(next_pin.node) },
                    new_page as *const Page
                ));

                // This cursorDown triggers the bug: the local page_pin copy
                // becomes stale after increaseCapacity, causing rowAndCell()
                // to access freed memory.
                s.cursor_down(1);

                // If the fix is applied, verify correct state
                assert!(s.cursor.y == row + 1);
                assert!(s.cursor.style.flags.bold);

                found = true;
                break;
            }
        }
    }
    // PORT NOTE: the reference's `else { try testing.expect(false); }` arm
    // on the `for ... else` (the "Didn't find boundary" failure) is mirrored
    // by this assert on the loop-completed-without-break flag.
    if !found {
        // Didn't find boundary
        assert!(found);
    }
    s.deinit();
}

// test "Screen setAttribute increases capacity when style map is full"
#[test]
fn screen_set_attribute_increases_capacity_when_style_map_is_full() {
    // Tests that setAttribute succeeds when the style map is full by
    // increasing page capacity. When capacity is at max and increaseCapacity
    // returns OutOfSpace, manualStyleUpdate will split the page instead.

    // Use a small screen with multiple rows
    let mut s = mkscreen(10, 5, 10);

    // Write content to multiple rows
    s.test_write_string(b"line1\nline2\nline3\nline4\nline5")
        .unwrap();

    // Get the page and fill its style map to capacity
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    let original_styles_capacity = unsafe { (*page).capacity.styles };

    // Fill the style map to capacity using the StyleSet's layout capacity
    // which accounts for the load factor
    {
        unsafe { (*page).pause_integrity_checks(true) };

        let max_items = unsafe { (*page).styles.layout.cap };
        let mut n: usize = 1;
        while n < max_items {
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
                let base = (*page).memory_ptr();
                (*page).styles.add(base, st)
            };
            if res.is_err() {
                break;
            }
            n += 1;
        }

        unsafe { (*page).pause_integrity_checks(false) };
        unsafe { (*page).assert_integrity() };
    }

    // Now try to set a new unique attribute that would require a new style slot
    // This should succeed by increasing capacity (or splitting if at max capacity)
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();

    // The style should have been applied (bold flag set)
    assert!(s.cursor.style.flags.bold);

    // The cursor should have a valid non-default style_id
    assert!(s.cursor.style_id != style::DEFAULT_ID);

    // Either the capacity increased or the page was split/changed
    let current_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    let capacity_increased = unsafe { (*current_page).capacity.styles } > original_styles_capacity;
    let page_changed = !std::ptr::eq(current_page, page);
    assert!(capacity_increased || page_changed);
    s.deinit();
}

// test "Screen setAttribute splits page on OutOfSpace at max styles"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'the 2026-07-31 adversarial re-audit'): too slow for either Miri lane, but NOT a hang. RUN TO COMPLETION under Miri with `-C debug-assertions=off`, single worker, no bound: `ok ... finished in 1656.89s` (27.6 min), 0 UB / 0 leaks. Removing the integrity assertions does not rescue it. CORRECTION to the WS19 T6 wording: the cost is NOT `page-mapping/LZ4 volume` -- this test never touches LZ4, and an instrumented run attributes 13.0s (0.8%) to the increase_capacity page-mapping phase and 1650.7s (99.6%) to filling the RefCountedSet style map with 53248 entries. Covered natively + by ASan."
)]
#[test]
fn screen_set_attribute_splits_page_on_out_of_space_at_max_styles() {
    let mut s = mkscreen(10, 10, 0);

    // Write content to multiple rows so we have something to split
    s.test_write_string(b"line1\nline2\nline3\nline4\nline5")
        .unwrap();

    // Remember the original node
    let original_node = unsafe { (*s.cursor.page_pin).node };

    // Increase the page's style capacity to max by repeatedly calling increaseCapacity
    // Use Screen.increaseCapacity to properly maintain cursor state
    let max_styles = size::CellCountInt::MAX;
    while unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).styles } < max_styles
    {
        if s.increase_capacity(
            unsafe { (*s.cursor.page_pin).node },
            Some(pagelist::grow::IncreaseCapacity::Styles),
        )
        .is_err()
        {
            break;
        }
    }

    // Get the page reference after increaseCapacity - cursor may have moved
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert_eq!(max_styles, unsafe { (*page).capacity.styles });

    // Fill the style map to capacity using the StyleSet's layout capacity
    // which accounts for the load factor
    {
        unsafe { (*page).pause_integrity_checks(true) };

        let max_items = unsafe { (*page).styles.layout.cap };
        let mut n: usize = 1;
        while n < max_items {
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
                let base = (*page).memory_ptr();
                (*page).styles.add(base, st)
            };
            if res.is_err() {
                break;
            }
            n += 1;
        }

        unsafe { (*page).pause_integrity_checks(false) };
        unsafe { (*page).assert_integrity() };
    }

    // Track the node before setAttribute
    let node_before_set = unsafe { (*s.cursor.page_pin).node };

    // Now try to set a new unique attribute that would require a new style slot
    // At max capacity, increaseCapacity will return OutOfSpace, triggering page split
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();

    // The style should have been applied (bold flag set)
    assert!(s.cursor.style.flags.bold);

    // The cursor should have a valid non-default style_id
    assert!(s.cursor.style_id != style::DEFAULT_ID);

    // The page should have been split
    let page_was_split = !std::ptr::eq(unsafe { (*s.cursor.page_pin).node }, node_before_set)
        || !unsafe { (*node_before_set).next }.is_null()
        || !unsafe { (*node_before_set).prev }.is_null()
        || !std::ptr::eq(unsafe { (*s.cursor.page_pin).node }, original_node);
    assert!(page_was_split);
    s.deinit();
}

// The cursor style and hyperlink IDs are only meaningful within the page
// the cursor pin points at. scrollClear can move the active area onto a
// later page while the cursor pin stays with its content on an earlier
// page (now scrollback), so the reset in cursorReload must migrate both
// references to the destination page. It previously replaced the pin
// directly and then released the old style ID on the new page.
// test "Screen scrollClear across pages migrates cursor style and hyperlink"
#[test]
fn screen_scroll_clear_across_pages_migrates_cursor_style_and_hyperlink() {
    let mut s = mkscreen(10, 10, usize::MAX);

    // Fill the first page so the active area spans two pages.
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 5) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    s.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
        .unwrap();
    assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));

    // Move the cursor to the top of the active area, which is on the
    // first page, and give it a style and a hyperlink there.
    s.cursor_absolute(0, 0);
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.start_hyperlink(b"https://example.com/", None).unwrap();

    let old_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    let old_style_id = s.cursor.style_id;
    let old_hyperlink_id = s.cursor.hyperlink_id;
    assert!(old_style_id != style::DEFAULT_ID);
    assert!(old_hyperlink_id != 0);

    // All ten active rows are non-empty, so this moves the active area
    // fully onto the second page while the cursor pin stays with its
    // old row, which is now scrollback.
    s.scroll_clear().unwrap();

    // The cursor was moved to the new active top-left on the second
    // page with its style and hyperlink references rebuilt there.
    let new_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert!(!std::ptr::eq(new_page, old_page));
    assert!(s.cursor.style_id != style::DEFAULT_ID);
    assert!(s.cursor.hyperlink_id != 0);
    assert!(
        unsafe {
            (*new_page)
                .styles
                .ref_count((*new_page).memory_ptr(), s.cursor.style_id)
        } > 0
    );
    assert!(
        unsafe {
            (*new_page)
                .hyperlink_set
                .ref_count((*new_page).memory_ptr(), s.cursor.hyperlink_id)
        } > 0
    );

    // The cursor's references on the old page were released. Nothing
    // else referenced either entry, so both are dead there now.
    assert!(
        unsafe {
            (*old_page)
                .styles
                .ref_count((*old_page).memory_ptr(), old_style_id)
        } == 0
    );
    assert!(
        unsafe {
            (*old_page)
                .hyperlink_set
                .ref_count((*old_page).memory_ptr(), old_hyperlink_id)
        } == 0
    );

    // Printing attaches the migrated style and hyperlink to a cell.
    s.test_write_string(b"B").unwrap();

    s.deinit();
}
