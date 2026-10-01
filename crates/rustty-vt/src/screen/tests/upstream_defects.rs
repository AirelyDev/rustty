//! PORTS: ghostty src/terminal/Screen.zig — the recycled-row metadata tests, the UD131 degrade tests, and the Rust-only allocator-routing and clone allocation-failure sweep tests.

use super::*;

// test "Screen: cursorScrollRegionUp recycled row has default metadata"
#[test]
fn screen_cursor_scroll_region_up_recycled_row_has_default_metadata() {
    let mut s = mkscreen(5, 5, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
        .unwrap();

    // Simulate the top region row being part of a soft-wrapped,
    // prompt-marked line. Its Row storage is recycled as the new
    // blank cursor row and must not retain the metadata.
    {
        let rac = s.pages.get_cell(act()).unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
        }
    }

    s.cursor_absolute(1, 2);
    s.cursor_scroll_region_up(2).unwrap();

    {
        let rac = s.pages.get_cell(pactive(0, 2)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"2EFGH\n3IJKL\n\n4MNOP\n5QRST");
    }
    s.deinit();
}

// test "Screen: cursorScrollRegionUp cross-page recycled row has default metadata"
#[test]
fn screen_cursor_scroll_region_up_cross_page_recycled_row_has_default_metadata() {
    let mut s = mkscreen(10, 5, 10);

    // We need to get the active area to span two pages so that the
    // scroll region does too, exercising the slow path (eraseRowBounded).
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

    // The first row of the last page is the Row storage that ends up
    // recycled as the blank region-bottom row: the erased row's
    // storage stays on the first page (receiving this row's content
    // via clone) while this storage is cleared for the blank row.
    {
        let rac = s.pages.get_cell(pactive(0, 3)).unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
        }
    }

    // Region rows 1-3 with the cursor on the region bottom, which is
    // on the second page while the region top is on the first page.
    s.cursor_absolute(0, 3);
    s.cursor_scroll_region_up(2).unwrap();

    {
        let rac = s.pages.get_cell(pactive(0, 3)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1A\n3C\n4D\n\n5E");
    }
    s.deinit();
}

// test "Screen: cursorScrollAbove cross-page recycled row has default metadata"
#[test]
fn screen_cursor_scroll_above_cross_page_recycled_row_has_default_metadata() {
    let mut s = mkscreen(10, 5, 10);

    // Get the cursor page and the last page to differ so that
    // cursorScrollAbove takes the cross-page rotate path.
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();
    s.cursor_absolute(0, 1);
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));
    assert!(!unsafe { (*s.pages.pages.first).next }.is_null());

    // The last row of the cursor page is the Row storage that gets
    // recycled as the new blank row below the cursor after its content
    // is moved down to the next page.
    {
        let rac = s.pages.get_cell(pactive(0, 2)).unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
        }
    }

    s.cursor_scroll_above().unwrap();

    // One row scrolled into history, so the blank row is at active
    // y=1 (just below the cursor's original row).
    {
        let rac = s.pages.get_cell(pactive(0, 1)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2B\n\n3C\n4D\n5E");
    }
    s.deinit();
}

// test "Screen: cursorDownScroll no scrollback recycled row has default metadata"
#[test]
fn screen_cursor_down_scroll_no_scrollback_recycled_row_has_default_metadata() {
    let mut s = mkscreen(5, 3, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    {
        let rac = s.pages.get_cell(act()).unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
        }
    }

    s.cursor_absolute(0, 2);
    s.cursor_down_scroll().unwrap();

    {
        let rac = s.pages.get_cell(pactive(0, 2)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }
    s.deinit();
}

// test "Screen: cursorDownScroll single row no scrollback resets metadata"
#[test]
fn screen_cursor_down_scroll_single_row_no_scrollback_resets_metadata() {
    let mut s = mkscreen(5, 1, 0);
    s.test_write_string(b"1ABCD").unwrap();

    {
        let rac = s.pages.get_cell(act()).unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
        }
    }

    s.cursor_down_scroll().unwrap();

    {
        let rac = s.pages.get_cell(act()).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }
    s.deinit();
}

// test "Screen: selectLine does not join lines across a recycled row"
#[test]
fn screen_select_line_does_not_join_lines_across_a_recycled_row() {
    let mut s = mkscreen(6, 5, 0);

    // A soft-wrapped line across rows 0 and 1: row 0 gets wrap=true.
    s.test_write_string(b"AAAAAAA").unwrap();

    // Scroll a region of rows 0-2 up by one: row 0 is discarded and
    // its Row storage recycled as the blank row 2.
    s.cursor_absolute(0, 2);
    s.cursor_scroll_region_up(2).unwrap();

    // Write unrelated single-line words on the recycled row and below.
    s.cursor_absolute(0, 2);
    s.test_write_string(b"world").unwrap();
    s.cursor_absolute(0, 3);
    s.test_write_string(b"hello").unwrap();

    // Selecting the line "world" must not extend into "hello": these
    // are separate hard lines. A stale wrap flag on the recycled row
    // would join them.
    {
        let pin = spin(&s, pactive(0, 2));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents, b"world");
        sel.deinit(&mut s);
    }
    s.deinit();
}

// UD131 (R13): the IL/DL/scroll row-copy retry loop
// (`Screen::clone_partial_row_grow_capacity`) when the destination page's
// style set is genuinely FULL of LIVE styles at the `u16` capacity ceiling
// and the row pulled across the page boundary carries a style the set does
// not have. The reference `@panic("increaseCapacity OutOfSpace")` here
// (Screen.zig:747); measured reachable from real VT input (IL/DL, a scroll,
// or a scroll-region shift crossing a page boundary). The port finishes the
// copy: the unstorable style degrades to the default while the codepoint,
// the storable grapheme and hyperlink, and the row flags are kept.
//
// Reaching the loop: the same loop is driven by real VT in the W2 orphan
// test (`ud80_delete_lines_..._reclaims_them`); here it is called directly
// so the full-LIVE-set precondition can be fabricated. Two pages are made
// with `split` so each keeps a small `size.rows` (a full 53,247-style set
// fits on wide rows), which keeps the page clones cheap. The first style is
// genuinely pinned, so the loop re-clones once (reclaiming nothing) then
// degrades — the path W2's one-shot flag bounds. A regression of that flag
// (task R13-X2's control: resetting it whenever the node changed) fails
// this test in under a second on the loop's debug re-clone counter instead
// of re-cloning forever.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud131_clone_partial_row_loop_full_live_set_degrades_new_style_to_default() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 50000,
            rows: 8,
            max_scrollback_bytes: Some(1 << 30),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    s.pages
        .split(Pin {
            node: s.pages.pages.first,
            y: 4,
            x: 0,
            garbage: false,
        })
        .unwrap();
    assert_eq!(2, s.pages.total_pages());
    let dst_node = s.pages.pages.first;
    let src_node = s.pages.pages.last;
    assert!(!std::ptr::eq(dst_node, src_node));

    let mk = |cp: u32| -> Cell {
        let mut c = Cell::zeroed();
        c.set_content_tag(crate::page::ContentTag::Codepoint);
        c.set_content_codepoint(cp);
        c
    };

    // Source row on the second page: a NEW style ('A'), a grapheme ('B'), a
    // hyperlink ('C'), a plain cell ('D'), and the row flags.
    let src_page: *mut Page = unsafe { pagelist::Node::page(src_node) };
    let src_row: *const Row = unsafe { (*src_page).get_row(0) };
    let link_id = unsafe {
        (*src_page).insert_hyperlink(hyperlink::Hyperlink {
            id: hyperlink::HyperlinkId::Implicit(0),
            uri: b"https://ud131.example/".to_vec(),
        })
    }
    .unwrap();
    let new_style = style::Style {
        flags: style::Flags {
            bold: true,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let base = (*src_page).memory_ptr();
        let new_id = (*src_page).styles.add(base, new_style).unwrap();
        let row = (*src_page).get_row(0);
        (*row).set_wrap(true);
        (*row).set_semantic_prompt(crate::page::SemanticPrompt::Prompt);

        let c0 = (*src_page).get_row_and_cell(0, 0).cell;
        *c0 = mk('A' as u32);
        (*c0).set_style_id(new_id);
        (*row).set_styled(true);

        let c1 = (*src_page).get_row_and_cell(1, 0).cell;
        *c1 = mk('B' as u32);
        (*src_page).set_graphemes(row, c1, &[0x0301u32]).unwrap();

        let c2 = (*src_page).get_row_and_cell(2, 0).cell;
        *c2 = mk('C' as u32);
        (*src_page).set_hyperlink(row, c2, link_id).unwrap();

        let c3 = (*src_page).get_row_and_cell(3, 0).cell;
        *c3 = mk('D' as u32);
    }

    // Destination page: style capacity grown to the ceiling, then a full
    // LIVE set on rows 1..4 (row 0 blank = clone target), first style pinned.
    while unsafe { pagelist::Node::capacity(s.pages.pages.first).styles } < size::CellCountInt::MAX
    {
        let _ = s
            .pages
            .increase_capacity(
                s.pages.pages.first,
                Some(pagelist::grow::IncreaseCapacity::Styles),
            )
            .unwrap();
    }
    let dst_node = s.pages.pages.first;
    let (live, _pinned) =
        unsafe { (*pagelist::Node::page(dst_node)).test_fill_full_live_pinned(1..4, true) };
    assert!(live > 50_000, "the set is full of live styles: {live}");
    assert!(pagelist::grow::styles_hold_pinned(unsafe {
        pagelist::Node::page(dst_node)
    }));

    let out = s.clone_partial_row_grow_capacity(dst_node, 0, src_page, src_row, 0, 50000);
    let page: *mut Page = unsafe { pagelist::Node::page(out) };

    unsafe {
        let row = (*page).get_row(0);
        assert!((*row).wrap(), "the row wrap flag is kept");
        assert_eq!(
            (*row).semantic_prompt(),
            crate::page::SemanticPrompt::Prompt,
            "the row semantic prompt is kept"
        );

        let c0 = (*page).get_row_and_cell(0, 0).cell;
        assert_eq!((*c0).codepoint(), 'A' as u32);
        assert_eq!(
            (*c0).style_id(),
            style::DEFAULT_ID,
            "the unstorable style degrades to the default"
        );

        let c1 = (*page).get_row_and_cell(1, 0).cell;
        assert_eq!((*c1).codepoint(), 'B' as u32);
        assert!((*c1).has_grapheme(), "the grapheme is kept");
        assert_eq!((*page).lookup_grapheme(c1).unwrap(), &[0x0301u32]);

        let c2 = (*page).get_row_and_cell(2, 0).cell;
        assert_eq!((*c2).codepoint(), 'C' as u32);
        assert!((*c2).hyperlink(), "the hyperlink is kept");

        let c3 = (*page).get_row_and_cell(3, 0).cell;
        assert_eq!((*c3).codepoint(), 'D' as u32);
        assert_eq!((*c3).style_id(), style::DEFAULT_ID);

        assert!(
            (*page).styles.count() > 50_000,
            "the live styles are kept: {}",
            (*page).styles.count()
        );
        assert!(pagelist::grow::styles_hold_pinned(page));
    }

    let mut node_ = s.pages.pages.first;
    while !node_.is_null() {
        unsafe { (*pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }
    s.deinit();
}

// UD131 (R13), the per-dimension degrade through the IL/DL/scroll loop
// (`Screen::clone_partial_row_grow_capacity`): the twin of
// `pagelist::tests::ud131_erase_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions`,
// with the same mixed source row (`ud131_test_support::build_mixed_src`),
// the same control comparison, and the same once-per-copy log check. See
// that test for the row and the negative controls.
fn ud131_mixed_screen(fill: bool) -> Screen {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: crate::page::ud131_test_support::mixed_cols(fill),
            rows: 6,
            max_scrollback_bytes: Some(1 << 30),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    )
    .unwrap();
    s.pages
        .split(Pin {
            node: s.pages.pages.first,
            y: 3,
            x: 0,
            garbage: false,
        })
        .unwrap();
    assert_eq!(2, s.pages.total_pages());
    // Room on the SOURCE page for the mixed row's graphemes and links.
    for (dim, n) in [
        (pagelist::grow::IncreaseCapacity::GraphemeBytes, 8),
        (pagelist::grow::IncreaseCapacity::HyperlinkBytes, 8),
        (pagelist::grow::IncreaseCapacity::StringBytes, 4),
    ] {
        for _ in 0..n {
            let _ = s
                .pages
                .increase_capacity(s.pages.pages.last, Some(dim))
                .unwrap();
        }
    }
    if fill {
        while unsafe { pagelist::Node::capacity(s.pages.pages.first).styles }
            < size::CellCountInt::MAX
        {
            let _ = s
                .pages
                .increase_capacity(
                    s.pages.pages.first,
                    Some(pagelist::grow::IncreaseCapacity::Styles),
                )
                .unwrap();
        }
        let (live, _) = unsafe {
            (*pagelist::Node::page(s.pages.pages.first)).test_fill_full_live_pinned(1..3, false)
        };
        assert!(live > 50_000, "the set is full of live styles: {live}");
    }
    s
}

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud131_clone_partial_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions() {
    use crate::page::ud131_test_support as t;

    // Control: an ordinary destination stores everything.
    let mut ctl = ud131_mixed_screen(false);
    let ctl_cols = ctl.pages.cols as usize;
    let ctl_src = unsafe { pagelist::Node::page(ctl.pages.pages.last) };
    let ctl_row = unsafe { t::build_mixed_src(ctl_src, 0) };
    let cap = crate::log_capture::start();
    let ctl_out = ctl.clone_partial_row_grow_capacity(
        ctl.pages.pages.first,
        0,
        ctl_src,
        ctl_row,
        0,
        ctl_cols,
    );
    assert!(
        !cap.messages().iter().any(|(_, _, m)| m.contains("UD131")),
        "the control copy does not degrade"
    );
    drop(cap);
    let ctl_page = unsafe { pagelist::Node::page(ctl_out) };

    // Degraded: the destination's style set is full of live styles.
    let mut deg = ud131_mixed_screen(true);
    let deg_cols = deg.pages.cols as usize;
    let deg_src = unsafe { pagelist::Node::page(deg.pages.pages.last) };
    let deg_row = unsafe { t::build_mixed_src(deg_src, 0) };
    let start_cap = unsafe { pagelist::Node::capacity(deg.pages.pages.first) };
    let styles_before = unsafe {
        (*pagelist::Node::page(deg.pages.pages.first))
            .styles
            .count()
    };

    let cap = crate::log_capture::start();
    let out = deg.clone_partial_row_grow_capacity(
        deg.pages.pages.first,
        0,
        deg_src,
        deg_row,
        0,
        deg_cols,
    );
    let first_copy = cap.messages();
    let out = deg.clone_partial_row_grow_capacity(out, 0, deg_src, deg_row, 0, deg_cols);
    let both_copies = cap.messages();
    drop(cap);
    let page = unsafe { pagelist::Node::page(out) };

    unsafe {
        t::assert_only_new_styles_degraded(ctl_page, page, 0);

        let c = (*page).capacity;
        assert!(
            c.grapheme_bytes > start_cap.grapheme_bytes,
            "grapheme bytes grew"
        );
        assert!(
            c.hyperlink_bytes > start_cap.hyperlink_bytes,
            "hyperlink bytes grew"
        );
        assert!(c.string_bytes > start_cap.string_bytes, "string bytes grew");
        assert_eq!(c.styles, size::CellCountInt::MAX);
        assert_eq!((*page).styles.count(), styles_before);

        (*page).assert_integrity();
        (*ctl_page).assert_integrity();
    }

    let expected = (
        log::Level::Warn,
        crate::page::LOG_SCOPE.to_string(),
        "row copy at a page capacity ceiling: 5 cell part(s) degraded to default \
         (styles=5, hyperlinks=0, graphemes=0); see UD131"
            .to_string(),
    );
    let ud131 = |v: &[crate::log_capture::CapturedRecord]| {
        v.iter()
            .filter(|(_, _, m)| m.contains("UD131"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ud131(&first_copy),
        vec![expected.clone()],
        "one record for one copy"
    );
    assert_eq!(
        ud131(&both_copies),
        vec![expected.clone(), expected],
        "one record per copy"
    );

    ctl.deinit();
    deg.deinit();
}

// Parity tail S7-3 routing evidence (Rust-only; the reference routes by
// construction): the Screen's own storage (`ScreenSet`'s
// `alloc.create(Screen)`) and the cursor hyperlink (`alloc.create(Hyperlink)`
// + `source.dupe(alloc)`) reach the screen's allocator, and are returned.
#[test]
fn screen_internals_route_through_the_allocator_handle() {
    use crate::alloc::testing::{with_handle, CountingAllocator};
    use crate::screen_set::ScreenSet;

    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        let opts = || Options {
            cols: 5,
            rows: 5,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        };

        // A bare Screen: its PageList's allocations.
        let mut s = Screen::init(alloc, opts()).unwrap();
        let screen_calls = counting.alloc_calls.get();
        let screen_live = counting.live_bytes.get();
        assert!(screen_calls > 0);

        // The cursor hyperlink: the struct and both duplicated slices.
        let uri = b"http://example.com";
        let id = b"id";
        s.start_hyperlink(uri, Some(id)).unwrap();
        assert_eq!(
            screen_calls + 3,
            counting.alloc_calls.get(),
            "alloc.create(Hyperlink) + dupe(uri) + dupe(id)"
        );
        assert_eq!(
            screen_live + core::mem::size_of::<CursorHyperlink>() + uri.len() + id.len(),
            counting.live_bytes.get()
        );
        s.end_hyperlink();
        assert_eq!(screen_live, counting.live_bytes.get());

        // Screen.clone: its PageList clone plus the pin remap
        // (`TrackedPinsRemap.init(alloc)`). Baseline: the same PageList
        // clone without a remap.
        let top = point::Point::Screen(point::Coordinate::default());
        let before = counting.alloc_calls.get();
        let mut plain = pagelist::access::Clone {
            top,
            bot: None,
            tracked_pins: None,
        };
        let mut pages = s.pages.clone(alloc, &mut plain).unwrap();
        let pages_calls = counting.alloc_calls.get() - before;
        pages.deinit();
        drop(pages);
        assert_eq!(screen_live, counting.live_bytes.get());

        let before = counting.alloc_calls.get();
        let mut cloned = s.clone(alloc, top, None).unwrap();
        assert_eq!(
            pages_calls + 1,
            counting.alloc_calls.get() - before,
            "Screen.clone = its PageList clone + the pin remap, through alloc"
        );
        cloned.deinit();
        drop(cloned);
        assert_eq!(
            screen_live,
            counting.live_bytes.get(),
            "the clone frees everything it took"
        );

        s.deinit();
        drop(s);
        assert_eq!(0, counting.live_bytes.get());

        // A ScreenSet: one more allocation, of exactly the Screen struct.
        let before = counting.alloc_calls.get();
        let mut set = ScreenSet::init(alloc, opts()).unwrap();
        assert_eq!(
            before + screen_calls + 1,
            counting.alloc_calls.get(),
            "alloc.create(Screen) is one more allocation"
        );
        assert_eq!(
            screen_live + core::mem::size_of::<Screen>(),
            counting.live_bytes.get(),
            "the Screen's own storage comes from the screen's allocator"
        );
        set.deinit(alloc);
    });
    assert_eq!(0, counting.live_bytes.get(), "deinit returns every byte");
}

// P7-W1 T5 fix round 1 (Rust-only): `Screen.clone` fails at every
// allocation of its allocator in turn: the PageList clone, the pin remap,
// and the three fallback `trackPin`s (the cursor outside the cloned rows,
// and a selection clipped at both ends). Every failure returns the error
// with nothing live and no pool assert (`errdefer pages.deinit()`); since
// T5 a PageList dropped without `deinit` aborts on its node pool's
// `live == free.len` assert.
#[test]
fn screen_clone_allocation_failure_sweep_returns_everything() {
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};

    let mut s = clone_clear_init(5, 4, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD").unwrap();
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
            .unwrap(),
        false,
    )))
    .unwrap();

    let mut failures = 0;
    for n in 0.. {
        let counting = CountingAllocator::over_heap();
        // SAFETY: `counting` outlives `failing` (declared first).
        let failing = unsafe {
            FailingAllocator::init(
                counting.allocator(),
                Config {
                    fail_index: n,
                    ..Config::default()
                },
            )
        };
        let c = failing.allocator();
        let ok = with_handle(&c, |alloc| {
            match s.clone(
                alloc,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
            ) {
                Ok(mut s2) => {
                    assert!(s2.selection.is_some());
                    s2.deinit();
                    true
                }
                Err(pagelist::access::CloneError::OutOfMemory) => false,
                Err(e) => panic!("unexpected {e:?}"),
            }
        });
        assert_eq!(
            0,
            counting.live_bytes.get(),
            "allocation {n} failing leaked {} bytes",
            counting.live_bytes.get()
        );
        if !failing.has_induced_failure.get() {
            assert!(ok);
            break;
        }
        assert!(!ok, "allocation {n} failed, so the clone must fail");
        failures += 1;
    }
    // The PageList clone's own allocations, the remap, and the tracked
    // pins beyond the preheated two all fail somewhere in the sweep.
    assert!(failures > 5, "{failures}");

    s.deinit();
}
