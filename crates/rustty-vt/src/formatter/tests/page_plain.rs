//! formatter tests: `PageFormatter` plain output (wrapping, trailing state,
//! x/y ranges, styling).

use super::*;

// test "Page plain single line"
#[test]
fn page_plain_single_line() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello, world");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());

    // Test our point map.
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello, world");
    let page_ref = unsafe { &*page };
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 12, state.cells);

    // Verify our point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..output.len() {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain single line soft-wrapped unwrapped"
#[test]
fn page_plain_single_line_soft_wrapped_unwrapped() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello!");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Plain,
            unwrap: true,
            ..Options::plain()
        },
    );

    // Test our point map.
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output
    // Note: we don't test the trailing state, which may have bugs
    // with unwrap...
    let _ = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello!");

    // Verify our point map
    assert_eq!(output.len(), point_map.len());
    assert_eq!(coord(0, 0), point_map[0]);
    assert_eq!(coord(1, 0), point_map[1]);
    assert_eq!(coord(2, 0), point_map[2]);
    assert_eq!(coord(0, 1), point_map[3]);
    assert_eq!(coord(1, 1), point_map[4]);
    assert_eq!(coord(2, 1), point_map[5]);
    t.deinit(Global);
}

// test "Page plain single wide char"
#[test]
fn page_plain_single_wide_char() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice("1A⚡".as_bytes());
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };

    // Full string
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "1A⚡".as_bytes());
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 4, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 2..output.len() {
            assert_eq!(coord(2, 0), point_map[i]);
        }
    }

    // Wide only (from start)
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.start_x = 2;
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "⚡".as_bytes());
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 4, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..output.len() {
            assert_eq!(coord(2, 0), point_map[i]);
        }
    }

    // Wide only (from tail)
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.start_x = 3;
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "⚡".as_bytes());
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 4, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..output.len() {
            assert_eq!(coord(2, 0), point_map[i]);
        }
    }
    t.deinit(Global);
}

// test "Page plain single wide char soft-wrapped unwrapped"
#[test]
fn page_plain_single_wide_char_soft_wrapped_unwrapped() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(3, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice("1A⚡".as_bytes());
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };

    // Full string
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.opts.unwrap = true;
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "1A⚡".as_bytes());
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 2, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 2..output.len() {
            assert_eq!(coord(0, 1), point_map[i]);
        }
    }

    // Full string (ending on spacer head)
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.opts.unwrap = true;
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.end_x = Some(2);
        formatter.end_y = Some(0);
        let _ = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "1A⚡".as_bytes());

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 2..output.len() {
            assert_eq!(coord(0, 1), point_map[i]);
        }
    }

    // Wide only (from start)
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.opts.unwrap = true;
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.start_x = 2;
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "⚡".as_bytes());
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 2, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..output.len() {
            assert_eq!(coord(0, 1), point_map[i]);
        }
    }

    // Wide only (from tail)
    {
        builder.clear();
        let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.opts.unwrap = true;
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.start_y = 1;
        formatter.start_x = 1;
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "⚡".as_bytes());
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 2, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..output.len() {
            assert_eq!(coord(0, 1), point_map[i]);
        }
    }
    t.deinit(Global);
}

// test "Page plain multiline"
#[test]
fn page_plain_multiline() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\nworld");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(4, 0), point_map[5]); // \n
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
    }
    t.deinit(Global);
}

// test "Page plain multiline rectangle"
#[test]
fn page_plain_multiline_rectangle() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_x = 1;
    formatter.end_x = Some(3);
    formatter.rectangle = true;
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"ell\norl");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..3 {
        assert_eq!(coord((i + 1) as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(3, 0), point_map[3]); // \n
    for i in 0..3 {
        assert_eq!(coord((i + 1) as size::CellCountInt, 1), point_map[4 + i]);
    }
    t.deinit(Global);
}

// test "Page plain multi blank lines"
#[test]
fn page_plain_multi_blank_lines() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\n\r\n\r\nworld");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\n\n\nworld");
    assert_eq!(page_ref.size.rows as usize - 3, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(4, 0), point_map[5]); // \n after row 0
    assert_eq!(coord(0, 1), point_map[6]); // \n after blank row 1
    assert_eq!(coord(0, 2), point_map[7]); // \n after blank row 2
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 3), point_map[8 + i]);
    }
    t.deinit(Global);
}

// test "Page plain trailing blank lines"
#[test]
fn page_plain_trailing_blank_lines() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld\r\n\r\n");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output. We expect there to be no trailing newlines because
    // we can't differentiate trailing blank lines as being meaningful because
    // the page formatter can't see the cursor position.
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\nworld");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(4, 0), point_map[5]); // \n
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
    }
    t.deinit(Global);
}

// test "Page plain trailing whitespace"
#[test]
fn page_plain_trailing_whitespace() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello   \r\nworld   ");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output. We expect there to be no trailing newlines because
    // we can't differentiate trailing blank lines as being meaningful because
    // the page formatter can't see the cursor position.
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\nworld");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(4, 0), point_map[5]); // \n
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
    }
    t.deinit(Global);
}

// test "Page plain trailing whitespace no trim"
#[test]
fn page_plain_trailing_whitespace_no_trim() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello   \r\nworld  ");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Plain,
            trim: false,
            ..Options::plain()
        },
    );
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output. We expect there to be no trailing newlines because
    // we can't differentiate trailing blank lines as being meaningful because
    // the page formatter can't see the cursor position.
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello   \nworld  ");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 7, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..8 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(7, 0), point_map[8]); // \n
    for i in 0..7 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[9 + i]);
    }
    t.deinit(Global);
}

// test "Page plain with prior trailing state rows"
#[test]
fn page_plain_with_prior_trailing_state_rows() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.trailing_state = Some(TrailingState { rows: 2, cells: 0 });
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"\n\nhello");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    assert_eq!(coord(0, 0), point_map[0]); // \n first blank row
    assert_eq!(coord(0, 1), point_map[1]); // \n second blank row
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[2 + i]);
    }
    t.deinit(Global);
}

// test "Page plain with prior trailing state cells no wrapped line"
#[test]
fn page_plain_with_prior_trailing_state_cells_no_wrapped_line() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.trailing_state = Some(TrailingState { rows: 0, cells: 3 });
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Blank cells are reset when row is not a wrap continuation
    assert_eq!(output, b"hello");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain with prior trailing state cells with wrap continuation"
#[test]
fn page_plain_with_prior_trailing_state_cells_with_wrap_continuation() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"world");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };

    // Surgically modify the first row to be a wrap continuation
    let row = page_ref.get_row(0);
    unsafe { (*row).set_wrap_continuation(true) };

    let mut formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Plain,
            unwrap: true,
            ..Options::plain()
        },
    );
    formatter.trailing_state = Some(TrailingState { rows: 0, cells: 3 });
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Blank cells are preserved when row is a wrap continuation with unwrap enabled
    assert_eq!(output, b"   world");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map - 3 spaces from prior trailing state + "world"
    assert_eq!(output.len(), point_map.len());
    // The 3 blank cells can't go back beyond (0,0) so they all map to (0,0)
    assert_eq!(coord(0, 0), point_map[0]); // space
    assert_eq!(coord(0, 0), point_map[1]); // space
    assert_eq!(coord(0, 0), point_map[2]); // space
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[3 + i]);
    }
    t.deinit(Global);
}

// test "Page plain soft-wrapped without unwrap"
#[test]
fn page_plain_soft_wrapped_without_unwrap() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world test");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Without unwrap, wrapped lines show as separate lines
    assert_eq!(output, b"hello worl\nd test");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 6, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..10 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(9, 0), point_map[10]); // \n
    for i in 0..6 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[11 + i]);
    }
    t.deinit(Global);
}

// test "Page plain soft-wrapped with unwrap"
#[test]
fn page_plain_soft_wrapped_with_unwrap() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world test");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Plain,
            unwrap: true,
            ..Options::plain()
        },
    );
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // With unwrap, wrapped lines are joined together
    assert_eq!(output, b"hello world test");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 6, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..10 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    for i in 0..6 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[10 + i]);
    }
    t.deinit(Global);
}

// test "Page plain soft-wrapped 3 lines without unwrap"
#[test]
fn page_plain_soft_wrapped_3_lines_without_unwrap() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world this is a test");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Without unwrap, wrapped lines show as separate lines
    assert_eq!(output, b"hello worl\nd this is\na test");
    assert_eq!(page_ref.size.rows as usize - 2, state.rows);
    assert_eq!(page_ref.size.cols as usize - 6, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..10 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(9, 0), point_map[10]); // \n
    for i in 0..9 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[11 + i]);
    }
    assert_eq!(coord(8, 1), point_map[20]); // \n
    for i in 0..6 {
        assert_eq!(coord(i as size::CellCountInt, 2), point_map[21 + i]);
    }
    t.deinit(Global);
}

// test "Page plain soft-wrapped 3 lines with unwrap"
#[test]
fn page_plain_soft_wrapped_3_lines_with_unwrap() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world this is a test");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Plain,
            unwrap: true,
            ..Options::plain()
        },
    );
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // With unwrap, wrapped lines are joined together
    assert_eq!(output, b"hello world this is a test");
    assert_eq!(page_ref.size.rows as usize - 2, state.rows);
    assert_eq!(page_ref.size.cols as usize - 6, state.cells);

    // Verify point map - unwrapped text spans 3 rows
    assert_eq!(output.len(), point_map.len());
    for i in 0..10 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    for i in 0..10 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[10 + i]);
    }
    for i in 0..6 {
        assert_eq!(coord(i as size::CellCountInt, 2), point_map[20 + i]);
    }
    t.deinit(Global);
}

// test "Page plain start_y subset"
#[test]
fn page_plain_start_y_subset() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld\r\ntest");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_y = 1;
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"world\ntest");
    assert_eq!(page_ref.size.rows as usize - 2, state.rows);
    assert_eq!(page_ref.size.cols as usize - 4, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[i]);
    }
    assert_eq!(coord(4, 1), point_map[5]); // \n
    for i in 0..4 {
        assert_eq!(coord(i as size::CellCountInt, 2), point_map[6 + i]);
    }
    t.deinit(Global);
}

// test "Page plain end_y subset"
#[test]
fn page_plain_end_y_subset() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld\r\ntest");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.end_y = Some(1);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\nworld");
    assert_eq!(1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(4, 0), point_map[5]); // \n
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
    }
    t.deinit(Global);
}

// test "Page plain start_y and end_y range"
#[test]
fn page_plain_start_y_and_end_y_range() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld\r\ntest\r\nfoo");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_y = 1;
    formatter.end_y = Some(2);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"world\ntest");
    assert_eq!(1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 4, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[i]);
    }
    assert_eq!(coord(4, 1), point_map[5]); // \n
    for i in 0..4 {
        assert_eq!(coord(i as size::CellCountInt, 2), point_map[6 + i]);
    }
    t.deinit(Global);
}

// test "Page plain start_y out of bounds"
#[test]
fn page_plain_start_y_out_of_bounds() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_y = 30;
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"");
    assert_eq!(0, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map is empty
    assert_eq!(0, point_map.len());
    t.deinit(Global);
}

// test "Page plain end_y greater than rows"
#[test]
fn page_plain_end_y_greater_than_rows() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.end_y = Some(30);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Should clamp to page.size.rows and work normally
    assert_eq!(output, b"hello");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain end_y less than start_y"
#[test]
fn page_plain_end_y_less_than_start_y() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_y = 5;
    formatter.end_y = Some(2);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"");
    assert_eq!(0, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map is empty
    assert_eq!(0, point_map.len());
    t.deinit(Global);
}

// test "Page plain start_x on first row only"
#[test]
fn page_plain_start_x_on_first_row_only() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_x = 6;
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"world");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 11, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord((i + 6) as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain end_x on last row only"
#[test]
fn page_plain_end_x_on_last_row_only() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"first line\r\nsecond line\r\nthird line");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.end_y = Some(2);
    formatter.end_x = Some(4);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"first line\nsecond line\nthird");
    assert_eq!(1, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..10 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(9, 0), point_map[10]); // \n
    for i in 0..11 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[11 + i]);
    }
    assert_eq!(coord(10, 1), point_map[22]); // \n
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 2), point_map[23 + i]);
    }
    t.deinit(Global);
}

// test "Page plain start_x and end_x multiline"
#[test]
fn page_plain_start_x_and_end_x_multiline() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world\r\ntest case\r\nfoo bar");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_x = 6;
    formatter.end_y = Some(2);
    formatter.end_x = Some(2);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // First row: "world" (start_x=6 to end of row)
    // Second row: "test case" (full row)
    // Third row: "foo" (start to end_x=2, inclusive)
    assert_eq!(output, b"world\ntest case\nfoo");
    assert_eq!(1, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord((i + 6) as size::CellCountInt, 0), point_map[i]);
    }
    assert_eq!(coord(10, 0), point_map[5]); // \n
    for i in 0..9 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
    }
    assert_eq!(coord(8, 1), point_map[15]); // \n
    for i in 0..3 {
        assert_eq!(coord(i as size::CellCountInt, 2), point_map[16 + i]);
    }
    t.deinit(Global);
}

// test "Page plain start_x out of bounds"
#[test]
fn page_plain_start_x_out_of_bounds() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_x = 100;
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"");
    assert_eq!(0, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map is empty
    assert_eq!(0, point_map.len());
    t.deinit(Global);
}

// test "Page plain end_x greater than cols"
#[test]
fn page_plain_end_x_greater_than_cols() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.end_x = Some(100);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain end_x less than start_x single row"
#[test]
fn page_plain_end_x_less_than_start_x_single_row() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_x = 10;
    formatter.end_y = Some(0);
    formatter.end_x = Some(5);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"");
    assert_eq!(0, state.rows);
    assert_eq!(0, state.cells);

    // Verify point map is empty
    assert_eq!(0, point_map.len());
    t.deinit(Global);
}

// test "Page plain start_y non-zero ignores trailing state"
#[test]
fn page_plain_start_y_non_zero_ignores_trailing_state() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_y = 1;
    formatter.trailing_state = Some(TrailingState { rows: 5, cells: 10 });
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Should NOT output the 5 newlines from trailing_state because start_y is non-zero
    assert_eq!(output, b"world");
    assert_eq!(page_ref.size.rows as usize - 1, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 1), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain start_x non-zero ignores trailing state"
#[test]
fn page_plain_start_x_non_zero_ignores_trailing_state() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_x = 6;
    formatter.trailing_state = Some(TrailingState { rows: 2, cells: 8 });
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // Should NOT output the 2 newlines or 8 spaces from trailing_state because start_x is non-zero
    assert_eq!(output, b"world");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 11, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..5 {
        assert_eq!(coord((i + 6) as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}

// test "Page plain start_y and start_x zero uses trailing state"
#[test]
fn page_plain_start_y_and_start_x_zero_uses_trailing_state() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    formatter.start_y = 0;
    formatter.start_x = 0;
    formatter.trailing_state = Some(TrailingState { rows: 2, cells: 0 });
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    // SHOULD output the 2 newlines from trailing_state because both start_y and start_x are 0
    assert_eq!(output, b"\n\nhello");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 5, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    assert_eq!(coord(0, 0), point_map[0]); // \n first blank row
    assert_eq!(coord(0, 1), point_map[1]); // \n second blank row
    for i in 0..5 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[2 + i]);
    }
    t.deinit(Global);
}

// test "Page plain single line with styling"
#[test]
fn page_plain_single_line_with_styling() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello, \x1b[1mworld\x1b[0m");
    }

    // Verify we have only a single page
    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    // Create the formatter
    let page = unsafe { last_page(pages) };
    let page_ref = unsafe { &*page };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });

    // Verify output
    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello, world");
    assert_eq!(page_ref.size.rows as usize, state.rows);
    assert_eq!(page_ref.size.cols as usize - 12, state.cells);

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    for i in 0..12 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    t.deinit(Global);
}
