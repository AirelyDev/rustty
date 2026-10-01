//! PORTS: ghostty src/terminal/PageList.zig — the `resize` (no reflow) tests
//! and the first `resize` reflow more-cols tests (first half of the resize
//! block).

use super::{cell_bg_color_rgb, cell_codepoint, pin_copy};
use crate::alloc::Global;
use crate::page::Cell;
use crate::pagelist::grow::Scrollbar;
use crate::pagelist::resize::Resize;
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{std_capacity, Options, OutOfMemory, PageList, Viewport};
use crate::point;
use rustty_render_types::term_size as size;

// =====================================================================
// resize (no reflow)
// =====================================================================

// ref: test "PageList resize (no reflow) more rows"
#[test]
fn pagelist_resize_no_reflow_more_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 3)
        },
    )
    .unwrap();
    assert_eq!(3usize, s.total_rows());

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        rows: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should not move because we have no scrollback so
    // we just grew.
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 2 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows with history"
#[test]
fn pagelist_resize_no_reflow_more_rows_with_history() {
    let mut s = PageList::init(Global, Options::new(10, 3)).unwrap();
    s.grow_rows(50).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 50 }), pt);
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(53usize, s.total_rows());

    // Our cursor should move since it's in the scrollback
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 4 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 48 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows"
#[test]
fn pagelist_resize_no_reflow_less_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) one rows"
#[test]
fn pagelist_resize_no_reflow_one_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Resize
    s.resize(Resize {
        rows: Some(1),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(1usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 9 }), pt);
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows cursor on bottom"
#[test]
fn pagelist_resize_no_reflow_less_rows_cursor_on_bottom() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint(y as u32);
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 9 }))
                .unwrap(),
        )
        .unwrap();
    {
        let cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
            point::Point::Active(c) => c,
            _ => unreachable!(),
        };
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: cursor.x,
                y: cursor.y,
            }))
            .unwrap();
        assert_eq!(9u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should move since it's in the scrollback
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 4 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows cursor in scrollback"
#[test]
fn pagelist_resize_no_reflow_less_rows_cursor_in_scrollback() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint(y as u32);
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap(),
        )
        .unwrap();
    {
        let cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
            point::Point::Active(c) => c,
            _ => unreachable!(),
        };
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: cursor.x,
                y: cursor.y,
            }))
            .unwrap();
        assert_eq!(2u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should move since it's in the scrollback
    assert!(s.point_from_pin(point::Tag::Active, pin_copy(p)).is_none());
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 2 }),
        s.point_from_pin(point::Tag::Screen, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows trims blank lines"
#[test]
fn pagelist_resize_no_reflow_less_rows_trims_blank_lines() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 5)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Fill remaining lines with a background color
    for y in 1..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_bg_color_rgb(0xFF, 0, 0);
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();
    {
        let cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
            point::Point::Active(c) => c,
            _ => unreachable!(),
        };
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: cursor.x,
                y: cursor.y,
            }))
            .unwrap();
        assert_eq!('A' as u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        rows: Some(2),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.rows as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should not move since we trimmed
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows trims blank lines cursor in blank line"
#[test]
fn pagelist_resize_no_reflow_less_rows_trims_blank_lines_cursor_in_blank_line() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 5)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Fill remaining lines with a background color
    for y in 1..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_bg_color_rgb(0xFF, 0, 0);
        }
    }

    // Put a tracked pin in a blank line
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        rows: Some(2),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.rows as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should not move since we trimmed
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows trims blank lines erases pages"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_no_reflow_less_rows_trims_blank_lines_erases_pages() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(100, 5)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Resize to take up two pages
    {
        let rows = unsafe { (*page).capacity.rows } + 10;
        s.resize(Resize {
            rows: Some(rows),
            reflow: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Resize down. Every row except the first is blank so we
    // should erase the second page.
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(5usize, s.total_rows());
    assert_eq!(1usize, s.total_pages());

    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows extends blank lines"
#[test]
fn pagelist_resize_no_reflow_more_rows_extends_blank_lines() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 3)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Fill remaining lines with a background color
    for y in 1..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_bg_color_rgb(0xFF, 0, 0);
        }
    }

    // Resize
    s.resize(Resize {
        rows: Some(7),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(7usize, s.rows as usize);
    assert_eq!(7usize, s.total_rows());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows contains viewport"
#[test]
fn pagelist_resize_no_reflow_more_rows_contains_viewport() {
    // When the rows are increased we need to make sure that the viewport
    // doesn't end up below the active area if it's currently in pin mode.

    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1),
            ..Options::new(5, 5)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);

    // Make it so we have scrollback
    s.grow().unwrap();

    assert_eq!(5usize, s.rows as usize);
    assert_eq!(6usize, s.total_rows());

    // Set viewport above active by scrolling up one.
    s.scroll(Scroll::DeltaRow(-1));
    // The viewport should be a pin now.
    assert!(s.viewport == Viewport::Top);

    // Resize
    s.resize(Resize {
        rows: Some(7),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(7usize, s.rows as usize);
    assert_eq!(7usize, s.total_rows());

    // Question: maybe the viewport should actually be in the active
    // here and not pinned to the top.
    assert!(s.viewport == Viewport::Top);

    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols"
#[test]
fn pagelist_resize_no_reflow_less_cols() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols pin in trimmed cols"
#[test]
fn pagelist_resize_no_reflow_less_cols_pin_in_trimmed_cols() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 8, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    assert_eq!(
        point::Point::Active(point::Coordinate { x: 4, y: 2 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols clears graphemes"
#[test]
fn pagelist_resize_no_reflow_less_cols_clears_graphemes() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Add a grapheme.
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(9, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
            (*page)
                .append_grapheme(rac.row, rac.cell, 'A' as u32)
                .unwrap();
        }
    }
    assert_eq!(1usize, unsafe { (*page).grapheme_count() });

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(chunk) = it.next() {
        assert_eq!(0usize, unsafe {
            (*crate::pagelist::Node::page(chunk.node)).grapheme_count()
        });
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols"
#[test]
fn pagelist_resize_no_reflow_more_cols() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(5, 3)
        },
    )
    .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(10usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols with spacer head"
#[test]
fn pagelist_resize_no_reflow_more_cols_with_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 3)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            // try testing.expect(!rac.row.wrap);
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// Regression test for fuzz crash. When we shrink cols and then
// grow back, the page retains capacity from the original size so the grow
// takes the fast path (just bumps page.size.cols). If any row has a
// spacer_head at the old last column, that cell is no longer at the end
// of the wider row, violating page integrity.
// ref: test "PageList resize (no reflow) grow cols fast path with spacer head"
#[test]
fn pagelist_resize_no_reflow_grow_cols_fast_path_with_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 3)
        },
    )
    .unwrap();

    // Shrink to 5 cols. The page keeps capacity for 10 cols.
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);

    // Place a spacer_head at the last column (col 4) on two rows
    // to simulate a wide character that didn't fit at the right edge.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Row 0: 'x' at col 0..3, spacer_head at col 4, wrap = true
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
                (*rac.row).set_wrap(true);
            }
        }

        // Row 1: spacer_head at col 4, wrap = true
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
                (*rac.row).set_wrap(true);
            }
        }
    }

    // Grow back to 10 cols. This must not leave stale spacer_head
    // cells at col 4 (which is no longer the last column).
    s.resize(Resize {
        cols: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);

    // Verify the old spacer_head positions are now narrow.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 0) };
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(!unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 1) };
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(!unsafe { (*rac.row).wrap() });
        }
    }

    s.deinit();
}

// This test is a bit convoluted so I want to explain: what we are trying
// to verify here is that when we increase cols such that our rows per page
// shrinks, we don't fragment our rows across many pages because this ends
// up wasting a lot of memory.
//
// This is particularly important for alternate screen buffers where we
// don't have scrollback so our max size is very small. If we don't do this,
// we end up pruning our pages and that causes resizes to fail!
// ref: test "PageList resize (no reflow) more cols forces less rows per page"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_no_reflow_more_cols_forces_less_rows_per_page() {
    // This test requires initially that our rows fit into one page.
    let cols: size::CellCountInt = 5;
    let rows: size::CellCountInt = 150;
    assert!(
        std_capacity()
            .adjust(crate::page::Adjustment { cols: Some(cols) })
            .unwrap()
            .rows
            >= rows
    );
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(cols, rows)
        },
    )
    .unwrap();

    // Then we need to resize our cols so that our rows per page shrinks.
    // This will force our resize to split our rows across two pages.
    {
        let new_cols = {
            let mut new_cols: size::CellCountInt = 50;
            let mut cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(new_cols),
                })
                .unwrap();
            while cap.rows >= rows {
                new_cols += 50;
                cap = std_capacity()
                    .adjust(crate::page::Adjustment {
                        cols: Some(new_cols),
                    })
                    .unwrap();
            }
            new_cols
        };
        s.resize(Resize {
            cols: Some(new_cols),
            reflow: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(new_cols as usize, s.cols as usize);
        assert_eq!(rows as usize, s.total_rows());
    }

    // Every page except the last should be full
    {
        let mut it = s.pages.first;
        while !it.is_null() {
            let page = it;
            if page == s.pages.last {
                break;
            }
            assert_eq!(
                unsafe { crate::pagelist::Node::capacity(page).rows },
                unsafe { crate::pagelist::Node::rows(page) }
            );
            it = unsafe { (*page).next };
        }
    }

    // Now we need to resize again to a col size that further shrinks
    // our last capacity.
    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
        assert!(page.size.rows == page.capacity.rows);
        let new_cols = {
            let mut new_cols = page.size.cols + 50;
            let mut cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(new_cols),
                })
                .unwrap();
            while cap.rows >= page.size.rows {
                new_cols += 50;
                cap = std_capacity()
                    .adjust(crate::page::Adjustment {
                        cols: Some(new_cols),
                    })
                    .unwrap();
            }
            new_cols
        };

        s.resize(Resize {
            cols: Some(new_cols),
            reflow: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(new_cols as usize, s.cols as usize);
        assert_eq!(rows as usize, s.total_rows());
    }

    // Every page except the last should be full
    {
        let mut it = s.pages.first;
        while !it.is_null() {
            let page = it;
            if page == s.pages.last {
                break;
            }
            assert_eq!(
                unsafe { crate::pagelist::Node::capacity(page).rows },
                unsafe { crate::pagelist::Node::rows(page) }
            );
            it = unsafe { (*page).next };
        }
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols then more cols"
#[test]
fn pagelist_resize_no_reflow_less_cols_then_more_cols() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(5, 3)
        },
    )
    .unwrap();

    // Resize less
    s.resize(Resize {
        cols: Some(2),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows and cols"
#[test]
fn pagelist_resize_no_reflow_less_rows_and_cols() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Resize less
    s.resize(Resize {
        cols: Some(5),
        rows: Some(7),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(7usize, s.rows as usize);

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize less rows and cols cursor at bottom"
#[test]
fn pagelist_resize_less_rows_and_cols_cursor_at_bottom() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    let cursor_pin = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.rows - 1) as u32,
            }))
            .unwrap(),
        )
        .unwrap();

    // Shrink both axes such that the original cursor.y is strictly past the
    // new row count, so resizeWithoutReflow leaves self.rows < c.y + 1.
    s.resize(Resize {
        cols: Some(79),
        rows: Some(20),
        reflow: true,
        cursor: Some(crate::pagelist::resize::Cursor {
            x: 0,
            y: 23,
            pin: Some(cursor_pin),
        }),
        pull_scrollback: true,
    })
    .unwrap();
    assert_eq!(79usize, s.cols as usize);
    assert_eq!(20usize, s.rows as usize);

    // remaining_rows saturates to 0, so the cursor lands on the new bottom row.
    assert_eq!(
        point::Point::Active(point::Coordinate {
            x: 0,
            y: (s.rows - 1) as u32,
        }),
        s.point_from_pin(point::Tag::Active, pin_copy(cursor_pin))
            .unwrap()
    );

    s.untrack_pin(cursor_pin);
    s.deinit();
}

// ref: test "PageList resize less rows and cols cursor near top pushed to scrollback"
#[test]
fn pagelist_resize_less_rows_and_cols_cursor_near_top_pushed_to_scrollback() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Fill every active row with non-blank content so that shrinking rows
    // can't trim trailing blank lines and instead pushes the top rows into
    // scrollback.
    {
        let mut it = s.row_iterator(
            crate::pagelist::iter::Direction::RightDown,
            point::Point::Active(point::Coordinate::default()),
            None,
        );
        while let Some(p) = it.next() {
            let rac = p.row_and_cell();
            let cells = unsafe { (*crate::pagelist::Node::page(p.node)).get_cells(rac.row) };
            let len = cells.len();
            for x in 0..len {
                unsafe {
                    (*cells)[x] = cell_codepoint(('A' as u32) + (x as u32 % 26));
                }
            }
        }
    }

    // Cursor near the top of the active area. After we shrink rows the active
    // area top moves down past this pin, so it ends up in scrollback.
    let cursor_pin = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Shrink both axes with reflow. resizeWithoutReflow shrinks self.rows
    // first, leaving the cursor pin above the new active area, then resizeCols
    // walks .left_up from the cursor pin toward the active-area top.
    s.resize(Resize {
        cols: Some(79),
        rows: Some(20),
        reflow: true,
        cursor: Some(crate::pagelist::resize::Cursor {
            x: 0,
            y: 0,
            pin: Some(cursor_pin),
        }),
        pull_scrollback: true,
    })
    .unwrap();
    assert_eq!(79usize, s.cols as usize);
    assert_eq!(20usize, s.rows as usize);

    // The active area is anchored to the bottom, so shrinking rows pushed the
    // top-of-screen cursor into scrollback: it no longer resolves to an
    // active-area coordinate, but it remains a valid screen pin.
    assert!(s
        .point_from_pin(point::Tag::Active, pin_copy(cursor_pin))
        .is_none());
    assert!(s
        .point_from_pin(point::Tag::Screen, pin_copy(cursor_pin))
        .is_some());

    // Integrity must hold after the resize.
    s.assert_integrity();

    s.untrack_pin(cursor_pin);
    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows and less cols"
#[test]
fn pagelist_resize_no_reflow_more_rows_and_less_cols() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Resize less
    s.resize(Resize {
        cols: Some(5),
        rows: Some(20),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(20usize, s.rows as usize);
    assert_eq!(20usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize more rows and cols doesn't fit in single std page"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_more_rows_and_cols_doesnt_fit_in_single_std_page() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Resize to a size that requires more than one page to fit our rows.
    let new_cols = 600;
    let new_rows = 600;
    let cap = std_capacity()
        .adjust(crate::page::Adjustment {
            cols: Some(new_cols),
        })
        .unwrap();
    assert!((cap.rows as usize) < new_rows as usize);

    s.resize(Resize {
        cols: Some(new_cols),
        rows: Some(new_rows),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(new_cols as usize, s.cols as usize);
    assert_eq!(new_rows as usize, s.rows as usize);
    assert_eq!(new_rows as usize, s.total_rows());

    s.deinit();
}

// ref: test "PageList resize (no reflow) empty screen"
#[test]
fn pagelist_resize_no_reflow_empty_screen() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(5, 5)
        },
    )
    .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(10),
        rows: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);
    assert_eq!(10usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(10usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols forces smaller cap"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_no_reflow_more_cols_forces_smaller_cap() {
    // We want a cap that forces us to have less rows
    let cap = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(100) })
        .unwrap();
    let cap2 = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(500) })
        .unwrap();
    assert_eq!(500 as size::CellCountInt, cap2.cols);
    assert!(cap2.rows < cap.rows);

    // Create initial cap, fits in one page
    let mut s = PageList::init(Global, Options::new(cap.cols, cap.rows)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Resize to our large cap
    let rows = s.total_rows();
    s.resize(Resize {
        cols: Some(cap2.cols),
        reflow: false,
        ..Default::default()
    })
    .unwrap();

    // Our total rows should be the same, and contents should be the same.
    assert_eq!(rows, s.total_rows());
    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(cap2.cols as usize, cells.len());
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr()).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows adds blank rows if cursor at bottom"
#[test]
fn pagelist_resize_no_reflow_more_rows_adds_blank_rows_if_cursor_at_bottom() {
    let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();

    // Grow to 5 total rows, simulating 3 active + 2 scrollback
    s.grow_rows(2).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.total_rows() {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint(y as u32);
            }
        }
    }

    // Active should be on row 3
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.rows - 2) as u32,
            }))
            .unwrap(),
        )
        .unwrap();
    let original_cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
        point::Point::Active(c) => c,
        _ => unreachable!(),
    };
    {
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: original_cursor.x,
                y: original_cursor.y,
            }))
            .unwrap();
        assert_eq!(3u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize_without_reflow(Resize {
        rows: Some(10),
        reflow: false,
        cursor: Some(crate::pagelist::resize::Cursor {
            x: 0,
            y: s.rows - 2,
            pin: None,
        }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.rows as usize);

    // Our cursor should not change
    assert_eq!(
        point::Point::Active(original_cursor),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    // 12 because we have our 10 rows in the active + 2 in the scrollback
    // because we're preserving the cursor.
    assert_eq!(12usize, s.total_rows());

    // Active should be at the same place it was.
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    // Go through our active, we should get only 3,4,5
    for y in 0..3usize {
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        let expected: u32 = (y + 2) as u32;
        assert_eq!(expected, unsafe { (*get.cell).content_codepoint() });
    }

    s.untrack_pin(p);
    s.deinit();
}

// PORT-LOCAL test (parity S123-F2): the reference has no `test {}` block for
// this either. `resizeWithoutReflow`'s cols branch opens with
// `const old_total_rows = self.total_rows; defer assert(self.total_rows ==
// old_total_rows);` (PageList.zig:2802-2803) — a `defer`, so it fires on EVERY
// exit from that block including the error return out of
// `resizeWithoutReflowGrowCols`. The port evaluated it only on the success
// path. This drives the error exit and asserts the same two invariants the
// reference's `defer` + `errdefer` guarantee there.
#[test]
fn pagelist_resize_no_reflow_grow_cols_oom_preserves_total_rows_and_limits() {
    use crate::pagelist::resize::{ResizeFp, RESIZE_TW};
    use rustty_util::tripwire::ResetMode;

    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    let rows_before = s.total_rows();
    let cols_before = s.cols;
    let old_limits = s.limits;

    // 80 -> 120 has no spare capacity in the existing page, so the grow takes
    // the slow path and must allocate.
    RESIZE_TW.with(|tw| {
        tw.borrow_mut()
            .errorAlways(ResizeFp::GrowColsCreatePage, OutOfMemory)
    });
    assert_eq!(
        Err(OutOfMemory),
        s.resize_without_reflow(Resize {
            cols: Some(120),
            reflow: false,
            ..Default::default()
        })
    );
    RESIZE_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

    assert_eq!(rows_before, s.total_rows());
    assert_eq!(cols_before, s.cols);
    assert_eq!(old_limits, s.limits);
    s.deinit();
}

// PORT-LOCAL test: the reference has no `test {}` block for this, it covers the
// analogous `errdefer` rollbacks with `std.testing.failing_allocator`. The
// behavior under test IS the reference's: `resizeWithoutReflow` installs the new
// `limits` minimums up front and guards them with `errdefer self.limits =
// old_limits` (PageList.zig:2337), so an `Allocator.Error` out of ANY fallible
// path below it — including the two row-growth `try self.grow()` loops
// (PageList.zig:2440 and :2470) — must leave the OLD limits installed.
//
// Release lane: this test injects a failure through a `rustty_util::tripwire`
// fail point and deliberately carries NO `#[cfg_attr(not(debug_assertions),
// ignore)]`. `tripwire::Module::enabled()` is `cfg!(any(test, debug_assertions,
// feature = "tripwire-test"))` (W57-B03/W59-B04), which reproduces Zig's
// `builtin.is_test` — true at ANY optimize mode — so the fail point trips in the
// `--release` lane too and this test RUNS there. Verified green in both profiles.
#[test]
fn pagelist_resize_no_reflow_grow_rows_oom_restores_limits() {
    use crate::pagelist::grow::{GrowFp, GROW_TW};
    use rustty_util::tripwire::ResetMode;

    // Arm 1: cursor is NOT at the bottom, so we take the `cursor:` block's
    // `for (0..delta) |_| _ = try self.grow()` loop.
    {
        let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

        // Fill the only page to capacity so that the very next `grow()` has to
        // allocate a new page — the single fallible point in `grow()`.
        let page_cap = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.last)).pause_integrity_checks(true) };
        while unsafe { crate::pagelist::Node::rows(s.pages.last) } < page_cap {
            assert!(s.grow().unwrap().is_none());
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.last)).pause_integrity_checks(false) };

        // The new row count must not fit in a single page, otherwise the
        // recomputed minimums are identical and the rollback is unobservable.
        let new_rows = page_cap + 1;
        let old_limits = s.limits;
        assert_ne!(old_limits, {
            let mut l = old_limits;
            l.resize(s.cols, new_rows);
            l
        });

        GROW_TW.with(|tw| tw.borrow_mut().errorAlways(GrowFp::CreatePage, OutOfMemory));
        assert_eq!(
            Err(OutOfMemory),
            s.resize_without_reflow(Resize {
                rows: Some(new_rows),
                reflow: false,
                cursor: Some(crate::pagelist::resize::Cursor {
                    x: 0,
                    y: 0,
                    pin: None,
                }),
                ..Default::default()
            })
        );
        GROW_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

        assert_eq!(old_limits, s.limits);
        s.deinit();
    }

    // Arm 2: no cursor, so we take the "pulls down scrollback" `for
    // (count..rows) |_| _ = try self.grow()` loop.
    {
        let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

        // Same requirement as above: grow past a single page's capacity so the
        // loop eventually reaches the allocating path AND the minimums change.
        let new_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows } + 1;
        let old_limits = s.limits;

        unsafe { (*crate::pagelist::Node::page(s.pages.last)).pause_integrity_checks(true) };
        GROW_TW.with(|tw| tw.borrow_mut().errorAlways(GrowFp::CreatePage, OutOfMemory));
        assert_eq!(
            Err(OutOfMemory),
            s.resize_without_reflow(Resize {
                rows: Some(new_rows),
                reflow: false,
                ..Default::default()
            })
        );
        GROW_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
        unsafe { (*crate::pagelist::Node::page(s.pages.last)).pause_integrity_checks(false) };

        assert_eq!(old_limits, s.limits);
        s.deinit();
    }
}

// ref: test "PageList resize reflow more cols no wrapped rows"
#[test]
fn pagelist_resize_reflow_more_cols_no_wrapped_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(5, 3)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(10),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(10usize, cells.len());
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr()).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols wrapped rows"
#[test]
fn pagelist_resize_reflow_more_cols_wrapped_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 4)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            if y % 2 == 0 {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Active should still be on top
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // First row should be unwrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(4usize, cells.len());
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr()).content_codepoint()
        });
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr().add(2)).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize reflow invalidates viewport offset cache"
#[test]
fn pagelist_resize_reflow_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(2, 4)).unwrap();
    s.grow_rows(20).unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        for y in 0..s.rows as usize {
            if y % 2 == 0 {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Scroll to a pinned viewport in history
    let pin_y = 10;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: pin_y }))
            .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    // Resize with reflow - unwrapping rows changes total_rows
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);

    // Verify scrollbar cache was invalidated during reflow
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList resize reflow more cols creates multiple pages"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_reflow_more_cols_creates_multiple_pages() {
    // We want a wide viewport so our row limit is rather small. This will
    // force the reflow below to create multiple pages, which we assert.
    let cap = {
        let mut current: size::CellCountInt = 100;
        loop {
            let cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(current),
                })
                .unwrap();
            if cap.rows < 100 {
                break cap;
            }
            current += 100;
        }
    };

    let mut s = PageList::init(Global, Options::new(cap.cols, cap.rows)).unwrap();

    // Wrap every other row so every line is wrapped for reflow
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            if y % 2 == 0 {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('A' as u32);
            }
        }
    }

    // Resize
    let newcap = cap
        .adjust(crate::page::Adjustment {
            cols: Some(cap.cols + 100),
        })
        .unwrap();
    assert!(newcap.rows < cap.rows);
    s.resize(Resize {
        cols: Some(newcap.cols),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(newcap.cols as usize, s.cols as usize);
    assert_eq!(cap.rows as usize, s.total_rows());

    {
        let mut count: usize = 0;
        let mut it = s.pages.first;
        while !it.is_null() {
            count += 1;

            // All pages should have the new capacity
            assert_eq!(newcap.cols, unsafe {
                crate::pagelist::Node::capacity(it).cols
            });
            assert_eq!(newcap.rows, unsafe {
                crate::pagelist::Node::capacity(it).rows
            });

            it = unsafe { (*it).next };
        }

        // We should have more than one page, meaning we created at least
        // one page. This is the critical aspect of this test so if this
        // ever goes false we need to adjust this test.
        assert!(count > 1);
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols wrap across page boundary"
#[test]
fn pagelist_resize_reflow_more_cols_wrap_across_page_boundary() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // At this point, we have some rows on the first page, and some on the second.
    // We can now wrap across the boundary condition.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows as usize - 1 };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        let page2 = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let y = 0;
        {
            let rac = unsafe { (*page2).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page2).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // We expect one fewer rows since we unwrapped a row.
    let end_rows = s.total_rows() - 1;

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(end_rows, s.total_rows());

    {
        // PAGE 1 ROW 6280, ACTIVE 8
        let p = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 8 }))
            .unwrap();
        let row = p.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p.cells(crate::pagelist::pin::CellSubset::All);
        assert!(!unsafe { (*(cells as *const Cell)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(1)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(2)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(3)).has_text() });
    }
    {
        // PAGE 1 ROW 6281, ACTIVE 9
        let p = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 9 }))
            .unwrap();
        let row = p.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p.cells(crate::pagelist::pin::CellSubset::All);
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols wrap across page boundary cursor in second page"
#[test]
fn pagelist_resize_reflow_more_cols_wrap_across_page_boundary_cursor_in_second_page() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // At this point, we have some rows on the first page, and some on the second.
    // We can now wrap across the boundary condition.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows as usize - 1 };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        let page2 = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let y = 0;
        {
            let rac = unsafe { (*page2).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page2).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in wrapped row on the last page
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 9 }))
                .unwrap(),
        )
        .unwrap();
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.last));

    // We expect one fewer rows since we unwrapped a row.
    let end_rows = s.total_rows() - 1;

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(end_rows, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 9 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 9 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });

        let cells = p2.cells(crate::pagelist::pin::CellSubset::All);
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }

    s.untrack_pin(p);
    s.deinit();
}
