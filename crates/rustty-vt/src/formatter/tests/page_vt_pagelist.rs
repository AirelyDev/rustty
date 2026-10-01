//! formatter tests: `PageFormatter` VT output and `PageListFormatter`
//! (multi-page, offsets, rectangles).

use super::*;

// test "Page VT single line plain text"
#[test]
fn page_vt_single_line_plain_text() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello");

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page VT single line with bold"
#[test]
fn page_vt_single_line_with_bold() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1mhello\x1b[0m");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"\x1b[0m\x1b[1mhello\x1b[0m");

    // Verify point map - style sequences should point to first character they style
    assert_eq!(output.len(), point_map.len());
    // \x1b[0m = 4 bytes, \x1b[1m = 4 bytes, total 8 bytes of style sequences
    // All style bytes should map to the first styled character at (0, 0)
    for i in 0..8 {
        assert_eq!(coord(0, 0), point_map[i]);
    }
    // Then "hello" maps to its respective positions
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[8 + i]);
    }
    t.deinit(Global);
}

// test "Page VT multiple styles"
#[test]
fn page_vt_multiple_styles() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1mhello \x1b[3mworld\x1b[0m");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        b"\x1b[0m\x1b[1mhello \x1b[0m\x1b[1m\x1b[3mworld\x1b[0m"
    );

    // Verify point map matches output length
    assert_eq!(output.len(), point_map.len());
    t.deinit(Global);
}

// test "Page VT with foreground color"
#[test]
fn page_vt_with_foreground_color() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[31mred\x1b[0m");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"\x1b[0m\x1b[38;5;1mred\x1b[0m");

    // Verify point map - style sequences should point to first character they style
    assert_eq!(output.len(), point_map.len());
    // \x1b[0m = 4 bytes, \x1b[38;5;1m = 9 bytes, total 13 bytes of style sequences
    // All style bytes should map to the first styled character at (0, 0)
    for i in 0..13 {
        assert_eq!(coord(0, 0), point_map[i]);
    }
    // Then "red" maps to its respective positions
    for i in 0..3 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[13 + i]);
    }
    t.deinit(Global);
}

// test "Page VT with background and foreground colors"
#[test]
fn page_vt_with_background_and_foreground_colors() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Vt,
            background: Some(crate::color::RGB {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }),
            foreground: Some(crate::color::RGB {
                r: 0xab,
                g: 0xcd,
                b: 0xef,
            }),
            ..Options::vt()
        },
    );
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();

    // Should emit OSC 10 for foreground, OSC 11 for background, then the text
    assert_eq!(
        output,
        b"\x1b]10;rgb:ab/cd/ef\x1b\\\x1b]11;rgb:12/34/56\x1b\\hello"
    );
    t.deinit(Global);
}

// test "Page VT multi-line with styles"
#[test]
fn page_vt_multi_line_with_styles() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1mfirst\x1b[0m\r\n\x1b[3msecond\x1b[0m");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // Note: style is reset before newline to prevent background colors from
    // bleeding to the next line's leading cells.
    assert_eq!(
        output,
        b"\x1b[0m\x1b[1mfirst\x1b[0m\r\n\x1b[0m\x1b[3msecond\x1b[0m"
    );

    // Verify point map matches output length
    assert_eq!(output.len(), point_map.len());
    t.deinit(Global);
}

// test "Page VT duplicate style not emitted twice"
#[test]
fn page_vt_duplicate_style_not_emitted_twice() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1mhel\x1b[1mlo\x1b[0m");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"\x1b[0m\x1b[1mhello\x1b[0m");

    // Verify point map matches output length
    assert_eq!(output.len(), point_map.len());
    t.deinit(Global);
}

// test "PageList plain single line"
#[test]
fn pagelist_plain_single_line() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello, world");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello, world");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(
            pin(node, i as size::CellCountInt, 0),
            pin_map.get(i).unwrap()
        );
    }
    t.deinit(Global);
}

// test "PageList plain spanning two pages"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_plain_spanning_two_pages() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
    {
        let mut s = t.vt_stream();
        // Fill the first page almost completely
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"page one");
    }
    // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
    // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
    // allocation, so it pops any raw pointer derived from that allocation
    // earlier; reading through a cached one afterwards is Stacked-Borrows UB
    // (memory: rust-mut-args-strongly-protected). This test used to cache it
    // across its `vt_stream()`/search writes. The violation was real and
    // deterministic, and had never been observed because a blanket
    // "multi-page PageList intractable" annotation kept the test out of Miri
    // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
    // made it runnable. Test-code only: no production path caches this.
    let active_ptr = t.screens.active;
    let pages_ptr = || -> *const PageList { unsafe { &raw const (*active_ptr).pages } };
    // Verify we're still on one page
    assert!(unsafe { core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
    // Add one more newline to push content to a second page
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\r\n");
    }
    assert!(unsafe { !core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
    // Write content on the second page
    {
        let mut s = t.vt_stream();
        s.next_slice(b"page two");
    }

    // Format the entire PageList
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr(), Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let full_output = builder.as_slice();
    let output = trim_start(full_output, b"\n");
    assert_eq!(output, b"page one\npage two");

    // Verify pin map
    assert_eq!(full_output.len(), pin_map.count());
    let first_node = unsafe { (*pages_ptr()).pages.first };
    let last_node = unsafe { (*pages_ptr()).pages.last };
    let trimmed_count = full_output.len() - output.len();

    // First part (trimmed blank lines) maps to first node
    for i in 0..trimmed_count {
        assert_eq!(first_node, pin_map.get(i).unwrap().node);
    }

    // "page one" (8 chars) maps to first node
    for i in 0..8 {
        let idx = trimmed_count + i;
        assert_eq!(first_node, pin_map.get(idx).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
    }

    // \n - maps to last node as it represents the transition to new page
    assert_eq!(last_node, pin_map.get(trimmed_count + 8).unwrap().node);

    // "page two" (8 chars) maps to last node
    for i in 0..8 {
        let idx = trimmed_count + 9 + i;
        assert_eq!(last_node, pin_map.get(idx).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
    }
    t.deinit(Global);
}

// test "PageList soft-wrapped line spanning two pages without unwrap"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_soft_wrapped_line_spanning_two_pages_without_unwrap() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
    {
        let mut s = t.vt_stream();
        // Fill the first page with soft-wrapped content
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"hello world test");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    // Verify we're on two pages due to wrapping
    assert!(unsafe { !core::ptr::eq((*pages_ptr).pages.first, (*pages_ptr).pages.last) });

    // Format without unwrap - should show line breaks
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let full_output = builder.as_slice();
    let output = trim_start(full_output, b"\n");
    assert_eq!(output, b"hello worl\nd test");

    // Verify pin map
    assert_eq!(full_output.len(), pin_map.count());
    let first_node = unsafe { (*pages_ptr).pages.first };
    let last_node = unsafe { (*pages_ptr).pages.last };
    let trimmed_count = full_output.len() - output.len();

    // First part (trimmed blank lines) maps to first node
    for i in 0..trimmed_count {
        assert_eq!(first_node, pin_map.get(i).unwrap().node);
    }

    // First line maps to first node
    for i in 0..10 {
        let idx = trimmed_count + i;
        assert_eq!(first_node, pin_map.get(idx).unwrap().node);
    }

    // \n - maps to last node as it represents the transition to new page
    assert_eq!(last_node, pin_map.get(trimmed_count + 10).unwrap().node);

    // "d test" (6 chars) maps to last node
    for i in 0..6 {
        let idx = trimmed_count + 11 + i;
        assert_eq!(last_node, pin_map.get(idx).unwrap().node);
    }
    t.deinit(Global);
}

// test "PageList soft-wrapped line spanning two pages with unwrap"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_soft_wrapped_line_spanning_two_pages_with_unwrap() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
    {
        let mut s = t.vt_stream();
        // Fill the first page with soft-wrapped content
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"hello world test");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    // Verify we're on two pages due to wrapping
    assert!(unsafe { !core::ptr::eq((*pages_ptr).pages.first, (*pages_ptr).pages.last) });

    // Format with unwrap - should join the wrapped lines
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(
        pages_ptr,
        Options {
            emit: Format::Plain,
            unwrap: true,
            ..Options::plain()
        },
    );
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let full_output = builder.as_slice();
    let output = trim_start(full_output, b"\r\n");
    assert_eq!(output, b"hello world test");

    // Verify pin map
    assert_eq!(full_output.len(), pin_map.count());
    let first_node = unsafe { (*pages_ptr).pages.first };
    let last_node = unsafe { (*pages_ptr).pages.last };
    let trimmed_count = full_output.len() - output.len();

    // First part (trimmed blank lines) maps to first node
    for i in 0..trimmed_count {
        assert_eq!(first_node, pin_map.get(i).unwrap().node);
    }

    // First line from first page
    for i in 0..10 {
        let idx = trimmed_count + i;
        assert_eq!(first_node, pin_map.get(idx).unwrap().node);
    }

    // "d test" (6 chars) from last page
    for i in 0..6 {
        let idx = trimmed_count + 10 + i;
        assert_eq!(last_node, pin_map.get(idx).unwrap().node);
    }
    t.deinit(Global);
}

// test "PageList VT spanning two pages"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_vt_spanning_two_pages() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
    {
        let mut s = t.vt_stream();
        // Fill the first page almost completely
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"\x1b[1mpage one");
    }
    // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
    // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
    // allocation, so it pops any raw pointer derived from that allocation
    // earlier; reading through a cached one afterwards is Stacked-Borrows UB
    // (memory: rust-mut-args-strongly-protected). This test used to cache it
    // across its `vt_stream()`/search writes. The violation was real and
    // deterministic, and had never been observed because a blanket
    // "multi-page PageList intractable" annotation kept the test out of Miri
    // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
    // made it runnable. Test-code only: no production path caches this.
    let active_ptr = t.screens.active;
    let pages_ptr = || -> *const PageList { unsafe { &raw const (*active_ptr).pages } };
    // Verify we're still on one page
    assert!(unsafe { core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
    // Add one more newline to push content to a second page
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\r\n");
    }
    assert!(unsafe { !core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
    // New content is still styled
    {
        let mut s = t.vt_stream();
        s.next_slice(b"page two");
    }

    // Format the entire PageList with VT
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr(), Options::vt());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let full_output = builder.as_slice();
    let output = trim_start(full_output, b"\r\n");
    assert_eq!(
        output,
        b"\x1b[0m\x1b[1mpage one\x1b[0m\r\n\x1b[0m\x1b[1mpage two\x1b[0m"
    );

    // Verify pin map
    assert_eq!(full_output.len(), pin_map.count());
    let first_node = unsafe { (*pages_ptr()).pages.first };
    let last_node = unsafe { (*pages_ptr()).pages.last };

    // Just verify we have entries for both pages in the pin map
    let mut first_count: usize = 0;
    let mut last_count: usize = 0;
    for byte_i in 0..pin_map.count() {
        let p = pin_map.get(byte_i).unwrap();
        if p.node == first_node {
            first_count += 1;
        }
        if p.node == last_node {
            last_count += 1;
        }
    }
    assert!(first_count > 0);
    assert!(last_count > 0);
    t.deinit(Global);
}

// test "PageList plain with x offset on single page"
#[test]
fn pagelist_plain_with_x_offset_on_single_page() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world\r\ntest case\r\nfoo bar");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let node = unsafe { (*pages_ptr).pages.first };
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.top_left = Some(pin(node, 6, 0));
    formatter.bottom_right = Some(pin(node, 2, 2));
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"world\ntest case\nfoo");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    for byte_i in 0..pin_map.count() {
        let p = pin_map.get(byte_i).unwrap();
        assert_eq!(node, p.node);
    }

    // "world" starts at x=6, y=0
    for i in 0..5 {
        assert_eq!((6 + i) as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(0, pin_map.get(i).unwrap().y);
    }
    t.deinit(Global);
}

// test "PageList plain with x offset spanning two pages"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_plain_with_x_offset_spanning_two_pages() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
    {
        let mut s = t.vt_stream();
        // Fill first page almost completely
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"hello world");
    }
    // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
    // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
    // allocation, so it pops any raw pointer derived from that allocation
    // earlier; reading through a cached one afterwards is Stacked-Borrows UB
    // (memory: rust-mut-args-strongly-protected). This test used to cache it
    // across its `vt_stream()`/search writes. The violation was real and
    // deterministic, and had never been observed because a blanket
    // "multi-page PageList intractable" annotation kept the test out of Miri
    // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
    // made it runnable. Test-code only: no production path caches this.
    let active_ptr = t.screens.active;
    let pages_ptr = || -> *const PageList { unsafe { &raw const (*active_ptr).pages } };
    // Verify we're still on one page
    assert!(unsafe { core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
    // Push to second page
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\r\n");
    }
    assert!(unsafe { !core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
    {
        let mut s = t.vt_stream();
        s.next_slice(b"foo bar test");
    }
    let first_node = unsafe { (*pages_ptr()).pages.first };
    let last_node = unsafe { (*pages_ptr()).pages.last };
    let first_node_size_rows = unsafe { crate::pagelist::Node::rows(first_node) };

    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr(), Options::plain());
    formatter.top_left = Some(pin(first_node, 6, first_node_size_rows - 1));
    formatter.bottom_right = Some(pin(last_node, 2, 1));
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let full_output = builder.as_slice();
    let output = trim_start(full_output, b"\n");
    assert_eq!(output, b"world\nfoo");

    // Verify pin map
    assert_eq!(full_output.len(), pin_map.count());
    let trimmed_count = full_output.len() - output.len();

    // "world" (5 chars) from first page
    for i in 0..5 {
        let idx = trimmed_count + i;
        assert_eq!(first_node, pin_map.get(idx).unwrap().node);
        assert_eq!((6 + i) as size::CellCountInt, pin_map.get(idx).unwrap().x);
    }

    // \n - maps to last node as it represents the transition to new page
    assert_eq!(last_node, pin_map.get(trimmed_count + 5).unwrap().node);

    // "foo" (3 chars) from last page
    for i in 0..3 {
        let idx = trimmed_count + 6 + i;
        assert_eq!(last_node, pin_map.get(idx).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
    }
    t.deinit(Global);
}

// test "PageList plain with start_x only"
#[test]
fn pagelist_plain_with_start_x_only() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let node = unsafe { (*pages_ptr).pages.first };
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.top_left = Some(pin(node, 6, 0));
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"world");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    for i in 0..5 {
        assert_eq!(node, pin_map.get(i).unwrap().node);
        assert_eq!((6 + i) as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(0, pin_map.get(i).unwrap().y);
    }
    t.deinit(Global);
}

// test "PageList plain with end_x only"
#[test]
fn pagelist_plain_with_end_x_only() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world\r\ntest");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let node = unsafe { (*pages_ptr).pages.first };
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.bottom_right = Some(pin(node, 2, 1));
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello world\ntes");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());

    // "hello world" (11 chars) on y=0
    for i in 0..11 {
        assert_eq!(node, pin_map.get(i).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(0, pin_map.get(i).unwrap().y);
    }

    // \n
    assert_eq!(node, pin_map.get(11).unwrap().node);

    // "tes" (3 chars) on y=1
    for i in 0..3 {
        assert_eq!(node, pin_map.get(12 + i).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(12 + i).unwrap().x);
        assert_eq!(1, pin_map.get(12 + i).unwrap().y);
    }
    t.deinit(Global);
}

// test "PageList plain rectangle basic"
#[test]
fn pagelist_plain_rectangle_basic() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(30, 5)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Lorem ipsum dolor\r\n");
        s.next_slice(b"sit amet, consectetur\r\n");
        s.next_slice(b"adipiscing elit, sed do\r\n");
        s.next_slice(b"eiusmod tempor incididunt\r\n");
        s.next_slice(b"ut labore et dolore");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.top_left = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(2, 1))) };
    formatter.bottom_right = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(6, 3))) };
    formatter.rectangle = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    let expected = "t ame\nipisc\nusmod";
    assert_eq!(output, expected.as_bytes());
    t.deinit(Global);
}

// test "PageList plain rectangle with EOL"
#[test]
fn pagelist_plain_rectangle_with_eol() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(30, 5)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Lorem ipsum dolor\r\n");
        s.next_slice(b"sit amet, consectetur\r\n");
        s.next_slice(b"adipiscing elit, sed do\r\n");
        s.next_slice(b"eiusmod tempor incididunt\r\n");
        s.next_slice(b"ut labore et dolore");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.top_left = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(12, 0))) };
    formatter.bottom_right = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(26, 4))) };
    formatter.rectangle = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    let expected = "dolor\nnsectetur\nlit, sed do\nor incididunt\n dolore";
    assert_eq!(output, expected.as_bytes());
    t.deinit(Global);
}

// test "PageList plain rectangle more complex with breaks"
#[test]
fn pagelist_plain_rectangle_more_complex_with_breaks() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(30, 8)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Lorem ipsum dolor\r\n");
        s.next_slice(b"sit amet, consectetur\r\n");
        s.next_slice(b"adipiscing elit, sed do\r\n");
        s.next_slice(b"eiusmod tempor incididunt\r\n");
        s.next_slice(b"ut labore et dolore\r\n");
        s.next_slice(b"\r\n");
        s.next_slice(b"magna aliqua. Ut enim\r\n");
        s.next_slice(b"ad minim veniam, quis");
    }
    let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
    let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
    formatter.top_left = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(11, 2))) };
    formatter.bottom_right = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(26, 7))) };
    formatter.rectangle = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    let expected = "elit, sed do\npor incididunt\nt dolore\n\na. Ut enim\nniam, quis";
    assert_eq!(output, expected.as_bytes());
    t.deinit(Global);
}
