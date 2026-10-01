//! PORTS: ghostty src/terminal/Screen.zig — the `clone`, `clearHistory`/`clearRows`/`clearCells`/`eraseRows` and `promptClickMove` tests.

use super::*;

// test "Screen: clone"
#[test]
fn screen_clone() {
    let mut s = clone_clear_init(10, 3, 10);
    s.test_write_string(b"1ABCD\n2EFGH").unwrap();
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }
    assert_eq!(5usize, s.cursor.x as usize);
    assert_eq!(1usize, s.cursor.y as usize);

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate::default()),
            None,
        )
        .unwrap();
    {
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }
    assert_eq!(5usize, s2.cursor.x as usize);
    assert_eq!(1usize, s2.cursor.y as usize);

    // Write to s1, should not be in s2
    s.test_write_string(b"\n34567").unwrap();
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH\n34567");
    }
    {
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }
    assert_eq!(5usize, s2.cursor.x as usize);
    assert_eq!(1usize, s2.cursor.y as usize);

    s2.deinit();
    s.deinit();
}

// test "Screen: clone partial"
#[test]
fn screen_clone_partial() {
    let mut s = clone_clear_init(10, 3, 10);
    s.test_write_string(b"1ABCD\n2EFGH").unwrap();
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }
    assert_eq!(5usize, s.cursor.x as usize);
    assert_eq!(1usize, s.cursor.y as usize);

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            None,
        )
        .unwrap();
    {
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"2EFGH");
    }

    // Cursor is shifted since we cloned partial
    assert_eq!(5usize, s2.cursor.x as usize);
    assert_eq!(0usize, s2.cursor.y as usize);

    s2.deinit();
    s.deinit();
}

// test "Screen: clone partial cursor out of bounds"
#[test]
fn screen_clone_partial_cursor_out_of_bounds() {
    let mut s = clone_clear_init(10, 3, 10);
    s.test_write_string(b"1ABCD\n2EFGH").unwrap();
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }
    assert_eq!(5usize, s.cursor.x as usize);
    assert_eq!(1usize, s.cursor.y as usize);

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Active(point::Coordinate { x: 0, y: 0 })),
        )
        .unwrap();
    {
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD");
    }

    // Cursor is shifted since we cloned partial
    assert_eq!(0usize, s2.cursor.x as usize);
    assert_eq!(0usize, s2.cursor.y as usize);

    s2.deinit();
    s.deinit();
}

// test "Screen: clone contains full selection"
#[test]
fn screen_clone_contains_full_selection() {
    let mut s = clone_clear_init(5, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Select a single line
    let cols = s.pages.cols;
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate {
                x: cols - 1,
                y: 1,
            }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate::default()),
            None,
        )
        .unwrap();

    // Our selection should remain valid
    {
        let sel = s2.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap(),
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s2.pages.cols - 1,
                y: 1,
            }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap(),
        );
    }

    s2.deinit();
    s.deinit();
}

// test "Screen: clone contains none of selection"
#[test]
fn screen_clone_contains_none_of_selection() {
    let mut s = clone_clear_init(5, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Select a single line
    let cols = s.pages.cols;
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate {
                x: cols - 1,
                y: 0,
            }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            None,
        )
        .unwrap();

    // Our selection should be null
    assert!(s2.selection.is_none());

    s2.deinit();
    s.deinit();
}

// test "Screen: clone contains selection start cutoff"
#[test]
fn screen_clone_contains_selection_start_cutoff() {
    let mut s = clone_clear_init(5, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Select a single line
    let cols = s.pages.cols;
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate {
                x: cols - 1,
                y: 1,
            }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            None,
        )
        .unwrap();

    // Our selection should remain valid
    {
        let sel = s2.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap(),
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s2.pages.cols - 1,
                y: 0,
            }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap(),
        );
    }

    s2.deinit();
    s.deinit();
}

// test "Screen: clone contains selection end cutoff"
#[test]
fn screen_clone_contains_selection_end_cutoff() {
    let mut s = clone_clear_init(5, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Select a single line
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Active(point::Coordinate { x: 0, y: 1 })),
        )
        .unwrap();

    // Our selection should remain valid
    {
        let sel = s2.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap(),
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s2.pages.cols - 1,
                y: 2,
            }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap(),
        );
    }

    s2.deinit();
    s.deinit();
}

// test "Screen: clone contains selection end cutoff reversed"
#[test]
fn screen_clone_contains_selection_end_cutoff_reversed() {
    let mut s = clone_clear_init(5, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Select a single line
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Active(point::Coordinate { x: 0, y: 1 })),
        )
        .unwrap();

    // Our selection should remain valid
    {
        let sel = s2.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap(),
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s2.pages.cols - 1,
                y: 2,
            }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap(),
        );
    }

    s2.deinit();
    s.deinit();
}

// test "Screen: clone contains subset of selection"
#[test]
fn screen_clone_contains_subset_of_selection() {
    let mut s = clone_clear_init(5, 4, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD").unwrap();

    // Select the full screen
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

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
        )
        .unwrap();

    // Our selection should remain valid
    {
        let sel = s2.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap(),
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s2.pages.cols - 1,
                y: 3,
            }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap(),
        );
    }

    s2.deinit();
    s.deinit();
}

// test "Screen: clone clamps clipped selections to mixed-width pages"
#[test]
fn screen_clone_clamps_clipped_selections_to_mixed_width_pages() {
    let mut s = mkscreen(4, 3, 0);

    let first = s.pages.pages.first;
    s.pages
        .split(Pin {
            node: first,
            x: 0,
            y: 2,
            garbage: false,
        })
        .unwrap();
    s.pages
        .split(Pin {
            node: first,
            x: 0,
            y: 1,
            garbage: false,
        })
        .unwrap();
    let middle = unsafe { (*first).next };
    let last = unsafe { (*middle).next };
    // PORT NOTE: `middle.page().size.cols = 2` — a deliberate mid-reflow
    // narrow page, written directly through the node pointer.
    unsafe { (*crate::pagelist::Node::page(middle)).size.cols = 2 };

    s.select(Some(Selection::init(
        Pin {
            node: first,
            x: 0,
            y: 0,
            garbage: false,
        },
        Pin {
            node: last,
            x: 3,
            y: 0,
            garbage: false,
        },
        false,
    )))
    .unwrap();
    let mut linear = s
        .clone(
            Allocator,
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Screen(point::Coordinate { x: 0, y: 1 })),
        )
        .unwrap();
    let linear_end = linear.selection.unwrap().end();
    let _ = linear_end.row_and_cell();
    assert_eq!(1 as size::CellCountInt, linear_end.x);
    linear.deinit();

    s.select(Some(Selection::init(
        Pin {
            node: first,
            x: 3,
            y: 0,
            garbage: false,
        },
        Pin {
            node: last,
            x: 3,
            y: 0,
            garbage: false,
        },
        true,
    )))
    .unwrap();
    let mut rectangle = s
        .clone(
            Allocator,
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            None,
        )
        .unwrap();
    let rectangle_start = rectangle.selection.unwrap().start();
    let _ = rectangle_start.row_and_cell();
    assert_eq!(1 as size::CellCountInt, rectangle_start.x);
    rectangle.deinit();

    s.deinit();
}

// test "Screen: clone contains subset of rectangle selection"
#[test]
fn screen_clone_contains_subset_of_rectangle_selection() {
    let mut s = clone_clear_init(5, 4, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD").unwrap();

    // Select the full screen from x=1 to x=3
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 3, y: 3 }))
            .unwrap(),
        true,
    )))
    .unwrap();

    // Clone
    let mut s2 = s
        .clone(
            Allocator,
            point::Point::Active(point::Coordinate { x: 0, y: 1 }),
            Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
        )
        .unwrap();

    // Our selection should remain valid and be properly clipped
    // preserving the columns of the start and end points of the
    // selection.
    {
        let sel = s2.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 1, y: 0 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap(),
        );
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 3, y: 3 }),
            s2.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap(),
        );
    }

    s2.deinit();
    s.deinit();
}

// test "Screen: clone basic"
#[test]
fn screen_clone_basic() {
    let mut s = clone_clear_init(10, 3, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    {
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 1 })),
            )
            .unwrap();

        // Test our contents rotated
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"2EFGH");

        s2.deinit();
    }

    {
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
            )
            .unwrap();

        // Test our contents rotated
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"2EFGH\n3IJKL");

        s2.deinit();
    }

    s.deinit();
}

// test "Screen: clone empty viewport"
#[test]
fn screen_clone_empty_viewport() {
    let mut s = clone_clear_init(10, 3, 0);

    {
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Viewport(point::Coordinate { x: 0, y: 0 })),
            )
            .unwrap();

        // Test our contents rotated
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"");

        s2.deinit();
    }

    s.deinit();
}

// test "Screen: clone one line viewport"
#[test]
fn screen_clone_one_line_viewport() {
    let mut s = clone_clear_init(10, 3, 0);
    s.test_write_string(b"1ABC").unwrap();

    {
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Viewport(point::Coordinate { x: 0, y: 0 })),
            )
            .unwrap();

        // Test our contents
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABC");

        s2.deinit();
    }

    s.deinit();
}

// test "Screen: clone empty active"
#[test]
fn screen_clone_empty_active() {
    let mut s = clone_clear_init(10, 3, 0);

    {
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 0 })),
            )
            .unwrap();

        // Test our contents rotated
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"");

        s2.deinit();
    }

    s.deinit();
}

// test "Screen: clone one line active with extra space"
#[test]
fn screen_clone_one_line_active_with_extra_space() {
    let mut s = clone_clear_init(10, 3, 0);
    s.test_write_string(b"1ABC").unwrap();

    {
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                None,
            )
            .unwrap();

        // Test our contents rotated
        let contents = s2
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABC");

        s2.deinit();
    }

    s.deinit();
}

// test "Screen: clear history with no history"
#[test]
fn screen_clear_history_with_no_history() {
    let mut s = clone_clear_init(10, 3, 3);
    s.test_write_string(b"4ABCD\n5EFGH\n6IJKL").unwrap();
    assert!(s.pages.viewport == pagelist::Viewport::Active);
    s.erase_history(None);
    assert!(s.pages.viewport == pagelist::Viewport::Active);
    {
        // Test our contents rotated
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
    }
    {
        // Test our contents rotated
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
    }

    s.deinit();
}

// test "Screen: clear history"
#[test]
fn screen_clear_history() {
    let mut s = clone_clear_init(10, 3, 3);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH\n6IJKL")
        .unwrap();
    assert!(s.pages.viewport == pagelist::Viewport::Active);

    // Scroll to top
    s.scroll(Scroll::Top);
    {
        // Test our contents rotated
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    s.erase_history(None);
    assert!(s.pages.viewport == pagelist::Viewport::Active);
    {
        // Test our contents rotated
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
    }
    {
        // Test our contents rotated
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
    }

    s.deinit();
}

// test "Screen: clear above cursor"
#[test]
fn screen_clear_above_cursor() {
    let mut s = clone_clear_init(10, 10, 3);
    s.test_write_string(b"4ABCD\n5EFGH\n6IJKL").unwrap();
    s.clear_rows(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        Some(point::Point::Active(point::Coordinate {
            x: 0,
            y: (s.cursor.y - 1) as u32,
        })),
        false,
    );
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"\n\n6IJKL");
    }
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"\n\n6IJKL");
    }

    assert_eq!(5usize, s.cursor.x as usize);
    assert_eq!(2usize, s.cursor.y as usize);

    s.deinit();
}

// test "Screen: clear above cursor with history"
#[test]
fn screen_clear_above_cursor_with_history() {
    let mut s = clone_clear_init(10, 3, 3);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
    s.test_write_string(b"4ABCD\n5EFGH\n6IJKL").unwrap();
    s.clear_rows(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        Some(point::Point::Active(point::Coordinate {
            x: 0,
            y: (s.cursor.y - 1) as u32,
        })),
        false,
    );
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"\n\n6IJKL");
    }
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL\n\n\n6IJKL");
    }

    assert_eq!(5usize, s.cursor.x as usize);
    assert_eq!(2usize, s.cursor.y as usize);

    s.deinit();
}

// test "Screen clearRows active one line"
#[test]
fn screen_clear_rows_active_one_line() {
    let mut s = clone_clear_init(80, 24, 1000);

    s.test_write_string(b"hello, world").unwrap();
    s.clear_rows(
        point::Point::Active(point::Coordinate::default()),
        None,
        false,
    );
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"");

    s.deinit();
}

// test "Screen clearRows active multi line"
#[test]
fn screen_clear_rows_active_multi_line() {
    let mut s = clone_clear_init(80, 24, 1000);

    s.test_write_string(b"hello\nworld").unwrap();
    s.clear_rows(
        point::Point::Active(point::Coordinate::default()),
        None,
        false,
    );
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"");

    s.deinit();
}

// test "Screen clearRows active styled line"
#[test]
fn screen_clear_rows_active_styled_line() {
    let mut s = clone_clear_init(80, 24, 1000);

    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"hello world").unwrap();
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();

    // We should have one style
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    s.clear_rows(
        point::Point::Active(point::Coordinate::default()),
        None,
        false,
    );

    // We should have none because active cleared it
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(str, b"");

    s.deinit();
}

// PORT NOTE (parity S133-F2): no reference test. Pins that
// `Options.default` carries the artifact field default for the kitty image
// storage limit (320MB for `.ghostty`) and that `Screen.init` hands it to
// the image storage, rather than a zero-byte budget.
#[test]
fn screen_options_default_kitty_image_storage_limit() {
    use crate::build_options::Artifact;
    assert_eq!(
        Options::kitty_image_storage_limit_default(Artifact::Ghostty),
        320 * 1000 * 1000
    );
    assert_eq!(
        Options::kitty_image_storage_limit_default(Artifact::Lib),
        10 * 1000 * 1000
    );
    assert_eq!(crate::options::ARTIFACT, Artifact::Ghostty);
    assert_eq!(
        Options::default().kitty_image_storage_limit,
        320 * 1000 * 1000
    );
    let mut s = Screen::init(Allocator, Options::default()).unwrap();
    if crate::options::KITTY_GRAPHICS {
        assert_eq!(s.kitty_images.total_limit, 320 * 1000 * 1000);
    }
    s.deinit();
}

// test "Screen clearCells empty range"
// PORT NOTE (parity 0cb004734): test hunk of the upstream empty-cell-clear
// guard in clearCells.
#[test]
fn screen_clear_cells_empty_range() {
    // var s = try Screen.init(testing.allocator, .default);
    // defer s.deinit();
    let mut s = Screen::init(Allocator, Options::default()).unwrap();

    // const page = s.cursor.page_pin.node.page();
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    // const row = s.cursor.page_row;
    let row: *mut Row = s.cursor.page_row;
    // const cells = page.getCells(row);
    let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
    // s.clearCells(page, row, cells[0..0]);
    let empty: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells as *mut Cell, 0);
    s.clear_cells(page, row, empty);

    s.deinit();
}

// -- P3-W1 T4 step 2: stored page widths (oracle e727a3658, a55850c98) ----

// ref: test "Screen clearRows uses stored page width"
#[test]
fn screen_clear_rows_uses_stored_page_width() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 2,
            rows: 1,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            ..Options::default()
        },
    )
    .unwrap();

    s.test_write_string(b"AB").unwrap();
    let node = s.pages.pages.first;
    // Model an incomplete reflow: the DESIRED width is now wider than the
    // width the stored page actually has.
    s.pages.cols = 4;

    s.clear_rows(
        point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        None,
        false,
    );

    let page: *mut Page = unsafe { crate::pagelist::Node::page(node) };
    let row0: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
    let cells: *mut [Cell] = unsafe { (*page).get_cells(row0) };
    assert_eq!(2usize, cells.len());
    for i in 0..cells.len() {
        let cell = unsafe { (cells as *mut Cell).add(i) };
        assert!(unsafe { (*cell).is_empty() });
    }

    // Restore the real width before deinit.
    s.pages.cols = 2;
    s.deinit();
}

// ref: test "Screen: cursorCellEndOfPrev across mixed-width pages"
#[test]
fn screen_cursor_cell_end_of_prev_across_mixed_width_pages() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 4,
            rows: 2,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            ..Options::default()
        },
    )
    .unwrap();

    s.test_write_string(b"ABCDE").unwrap();
    let first = s.pages.pages.first;
    s.pages
        .split(crate::pagelist::Pin {
            node: first,
            y: 1,
            x: 0,
            garbage: false,
        })
        .unwrap();
    s.cursor_reload();
    let second = unsafe { (*first).next };
    // Model an incomplete reflow: the stored first page is narrower than
    // the desired screen width.
    s.pages.pause_integrity_checks(true);
    unsafe { (*crate::pagelist::Node::page(first)).size.cols = 2 };

    assert!(core::ptr::eq(second, unsafe { (*s.cursor.page_pin).node }));
    let expected = crate::pagelist::Pin {
        node: first,
        y: 0,
        x: 1,
        garbage: false,
    }
    .row_and_cell()
    .cell;
    assert!(core::ptr::eq(expected, s.cursor_cell_end_of_prev()));

    unsafe { (*crate::pagelist::Node::page(first)).size.cols = 4 };
    s.pages.pause_integrity_checks(false);
    s.deinit();
}

/// `e727a3658`'s SECOND hunk — `clear_cells` comparing `cells.len` against
/// `page.size.cols` instead of `self.pages.cols` — is not covered by the
/// oracle's own `clearRows` test (there, clearing the whole narrow row makes
/// both branches agree, because `update_row_*_flag` also scans
/// `page.size.cols`). The observable divergence is in the OTHER reflow
/// direction: a stored page WIDER than the desired width. Then a partial
/// clear whose length happens to equal the DESIRED width takes the
/// "we cleared everything" fast path and drops the row's `styled` flag while
/// styled cells still exist beyond the desired width — corrupting the row
/// flags rather than tripping an assert.
#[test]
fn screen_clear_cells_uses_stored_page_width_for_row_flags() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 4,
            rows: 1,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            ..Options::default()
        },
    )
    .unwrap();

    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"ABCD").unwrap();

    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    let row: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
    assert!(unsafe { (*row).styled() }, "the row starts styled");
    assert_eq!(4, unsafe { (*page).size.cols });

    // Park the cursor inside the narrower desired width so the screen's own
    // integrity assertions stay satisfied for the modelled state.
    s.cursor_horizontal_absolute(0);

    // Model an incomplete reflow the other way: the DESIRED width is now
    // NARROWER than the stored page.
    s.pages.cols = 2;

    // Clear exactly the first two cells — a partial clear whose length
    // equals the desired width but NOT the stored width.
    let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
    let partial: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells as *mut Cell, 2);
    s.clear_cells(page, row, partial);

    assert!(
        unsafe { (*row).styled() },
        "cells 2 and 3 are still styled, so the row flag must stay set; \
         comparing cells.len against the DESIRED width drops it"
    );

    s.pages.cols = 4;
    s.deinit();
}

// ref: test "Screen write regrows compacted page capacity"
#[test]
fn screen_write_regrows_compacted_page_capacity() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            ..Options::default()
        },
    )
    .unwrap();

    // Compact the active page so every managed capacity dimension is
    // zero, then reload the cursor since its cached row/cell pointers
    // point into the replaced page.
    {
        let node = s
            .pages
            .compact(unsafe { (*s.cursor.page_pin).node })
            .unwrap()
            .unwrap();
        let cap = unsafe { crate::pagelist::Node::capacity(node) };
        assert_eq!(0, cap.styles);
        assert_eq!(0, cap.grapheme_bytes);
        assert_eq!(0, cap.string_bytes);
        assert_eq!(0, cap.hyperlink_bytes);
        s.cursor_reload();
    }

    // Styled write: exercises the manual_style_update single-retry
    // path. Prior to increase_capacity handling zero dimensions, the
    // retry would fail and the style would be dropped.
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"A").unwrap();

    // Grapheme write: exercises the append_grapheme single-retry path.
    // We can't use test_write_string here because it appends graphemes
    // directly on the page without the capacity retry.
    s.test_write_string(b"a").unwrap();
    let prev = s.cursor_cell_left(1);
    s.append_grapheme(prev, 0x0301).unwrap();

    // Hyperlink: exercises the start_hyperlink retry loop, which used
    // to loop forever when capacity growth from zero didn't grow.
    s.start_hyperlink(b"https://example.com/", None).unwrap();
    s.test_write_string(b"B").unwrap();
    s.end_hyperlink();

    // Verify the content landed on the page.
    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert!(unsafe { (*page).styles.count() } >= 1);
    assert!(unsafe { (*page).hyperlink_set.count() } >= 1);
    assert!(unsafe { (*page).grapheme_count() } >= 1);

    s.deinit();
}

/// `P3-W1 T6` step 4 (oracle 8d663a76e). A "no panic" test is blind to a
/// DOUBLE release, so this asserts the ref count reaches EXACTLY zero, from
/// an exactly-known starting value, for both the uniform-run case (one
/// `release_multiple` call) and the alternating case (which degrades to one
/// call per cell and is where an off-by-one run length would show up).
#[test]
fn screen_clear_cells_releases_styles_per_run() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 8,
            rows: 1,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            ..Options::default()
        },
    )
    .unwrap();

    // One uniform run of four styled cells.
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"AAAA").unwrap();
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();

    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    let row: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
    let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
    let base = unsafe { (*page).memory_ptr() };

    let id = unsafe { (*(cells as *mut Cell)).style_id() };
    assert_ne!(
        crate::style::DEFAULT_ID,
        id,
        "the first cell must be styled"
    );
    assert_eq!(
        4,
        usize::from(unsafe { (*page).styles.ref_count(base, id) })
    );

    // Clear exactly the four styled cells.
    let run: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells as *mut Cell, 4);
    s.clear_cells(page, row, run);

    assert_eq!(
        0,
        usize::from(unsafe { (*page).styles.ref_count(base, id) }),
        "one release_multiple per run must land the ref count on exactly \
         zero -- not below it (double release) and not above (short run)"
    );

    s.deinit();
}

/// Companion to the run test: ALTERNATING styles, where every run has
/// length one. This is the case an off-by-one in the run scan would break
/// while the uniform case still passed.
#[test]
fn screen_clear_cells_releases_alternating_styles_exactly_once_each() {
    let mut s = Screen::init(
        Allocator,
        Options {
            cols: 8,
            rows: 1,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            ..Options::default()
        },
    )
    .unwrap();

    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"A").unwrap();
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
    s.set_attribute(crate::sgr::Attribute::Italic).unwrap();
    s.test_write_string(b"B").unwrap();
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"C").unwrap();
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();

    let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    let row: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
    let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
    let base = unsafe { (*page).memory_ptr() };
    let cells_base = cells as *mut Cell;

    let bold_id = unsafe { (*cells_base).style_id() };
    let italic_id = unsafe { (*cells_base.add(1)).style_id() };
    assert_ne!(bold_id, italic_id);
    assert_eq!(bold_id, unsafe { (*cells_base.add(2)).style_id() });
    assert_eq!(
        2,
        usize::from(unsafe { (*page).styles.ref_count(base, bold_id) })
    );
    assert_eq!(
        1,
        usize::from(unsafe { (*page).styles.ref_count(base, italic_id) })
    );

    let run: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells_base, 3);
    s.clear_cells(page, row, run);

    assert_eq!(
        0,
        usize::from(unsafe { (*page).styles.ref_count(base, bold_id) })
    );
    assert_eq!(
        0,
        usize::from(unsafe { (*page).styles.ref_count(base, italic_id) })
    );

    s.deinit();
}

// test "Screen clearRows protected"
#[test]
fn screen_clear_rows_protected() {
    let mut s = clone_clear_init(80, 24, 1000);

    s.test_write_string(b"UNPROTECTED").unwrap();
    s.cursor.protected = true;
    s.test_write_string(b"PROTECTED").unwrap();
    s.cursor.protected = false;
    s.test_write_string(b"UNPROTECTED").unwrap();
    s.test_write_string(b"\n").unwrap();
    s.cursor.protected = true;
    s.test_write_string(b"PROTECTED").unwrap();
    s.cursor.protected = false;
    s.test_write_string(b"UNPROTECTED").unwrap();
    s.cursor.protected = true;
    s.test_write_string(b"PROTECTED").unwrap();
    s.cursor.protected = false;

    s.clear_rows(
        point::Point::Active(point::Coordinate::default()),
        None,
        true,
    );

    let str = s
        .dump_string_alloc(
            Allocator,
            point::Point::Screen(point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(
        str,
        b"           PROTECTED\nPROTECTED           PROTECTED".as_slice()
    );

    s.deinit();
}

// test "Screen eraseRows history"
#[test]
fn screen_erase_rows_history() {
    let mut s = clone_clear_init(5, 5, 1000);

    s.test_write_string(b"1\n2\n3\n4\n5\n6").unwrap();

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"2\n3\n4\n5\n6");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"1\n2\n3\n4\n5\n6");
    }

    s.erase_history(None);

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"2\n3\n4\n5\n6");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"2\n3\n4\n5\n6");
    }

    s.deinit();
}

// test "Screen eraseRows history with more lines"
#[test]
fn screen_erase_rows_history_with_more_lines() {
    let mut s = clone_clear_init(5, 5, 1000);

    s.test_write_string(b"A\nB\nC\n1\n2\n3\n4\n5\n6").unwrap();

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"2\n3\n4\n5\n6");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"A\nB\nC\n1\n2\n3\n4\n5\n6");
    }

    s.erase_history(None);

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"2\n3\n4\n5\n6");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"2\n3\n4\n5\n6");
    }

    s.deinit();
}

// test "Screen eraseRows active partial"
#[test]
fn screen_erase_rows_active_partial() {
    let mut s = clone_clear_init(5, 5, 0);

    s.test_write_string(b"1\n2\n3").unwrap();

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"1\n2\n3");
    }

    s.erase_active(1);

    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"3");
    }
    {
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"3");
    }

    s.deinit();
}

// test "Screen: promptClickMove line right basic"
#[test]
fn screen_prompt_click_move_line_right_basic() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Move cursor back to start of input (column 2, the 'h')
    s.cursor_absolute(2, 0);

    // Click on first 'l' (column 4), should require 2 right movements (h->e->l)
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 2);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove line right cursor not on input"
#[test]
fn screen_prompt_click_move_line_right_cursor_not_on_input() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);

    // Move cursor back to prompt area (column 0, the '>')
    s.cursor_absolute(0, 0);

    // Cursor is on prompt, not input - should return zero
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result, PromptClickMove::ZERO);

    s.deinit();
}

// test "Screen: promptClickMove line right click on same position"
#[test]
fn screen_prompt_click_move_line_right_click_on_same_position() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Move cursor to column 4
    s.cursor_absolute(4, 0);

    // Click on same position - no movement needed
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result, PromptClickMove::ZERO);

    s.deinit();
}

// test "Screen: promptClickMove line right skips non-input cells"
#[test]
fn screen_prompt_click_move_line_right_skips_non_input_cells() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write: "> h" then output "X" then input "llo"
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"h").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"X").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"llo").unwrap();

    // Move cursor to column 2 (the 'h')
    s.cursor_absolute(2, 0);

    // Click on 'l' at column 5 - should skip the 'X' output cell
    // Movement: h (start) -> l (col 4) -> l (col 5) = 2 right movements
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 5, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 2);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove line right soft-wrapped line"
#[test]
fn screen_prompt_click_move_line_right_soft_wrapped_line() {
    let mut s = clone_clear_init(10, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input that wraps
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    // Write 8 chars of input, first row has 2 for prompt + 8 input = 10 cols
    s.test_write_string(b"abcdefgh").unwrap();
    // Continue on next row (soft-wrapped)
    s.test_write_string(b"ij").unwrap();

    // Verify soft wrap occurred
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"> abcdefgh\nij");
    }

    // Move cursor to column 2 (the 'a')
    s.cursor_absolute(2, 0);

    // Click on 'j' at column 1, row 1 - should count all input cells
    // Movement: a->b->c->d->e->f->g->h->i->j = 9 right movements
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 1, y: 1 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 9);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove disabled when click is none"
#[test]
fn screen_prompt_click_move_disabled_when_click_is_none() {
    let mut s = clone_clear_init(20, 5, 0);

    // Click mode is .none by default (disabled)
    assert_eq!(s.semantic_prompt.click, SemanticClick::None);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Move cursor to start of input
    s.cursor_absolute(2, 0);

    // Click should return zero since click mode is disabled
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result, PromptClickMove::ZERO);

    s.deinit();
}

// test "Screen: promptClickMove line right stops at hard wrap"
#[test]
fn screen_prompt_click_move_line_right_stops_at_hard_wrap() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write prompt and input on first line
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();
    // Hard wrap (newline)
    s.test_write_string(b"\n").unwrap();
    s.test_write_string(b"world").unwrap();

    // Move cursor to column 2 (the 'h')
    s.cursor_absolute(2, 0);

    // Click on 'w' at column 0, row 1 - but line mode stops at hard wrap
    // Should only move to end of first line: h->e->l->l->o = 4 movements
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    // Should stop at end of first line, not cross hard wrap
    assert_eq!(result.right, 5);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove line right stops at non-continuation row"
#[test]
fn screen_prompt_click_move_line_right_stops_at_non_continuation_row() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Row 0: PROMPT "> hello"
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello\n").unwrap();

    // Row 1: CONTINUATION "world"
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Continuation,
    ));
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"world\n").unwrap();

    // Row 2: NEW PROMPT "> again"
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"again").unwrap();

    // Verify content
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"> hello\nworld\n> again");
    }

    // Move cursor to 'w' at column 0, row 1
    s.cursor_absolute(0, 1);

    // Click on 'a' at column 2, row 2 - but row 2 is a new prompt
    // Should stop at end of "world": w->o->r->l->d = 4 movements
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    // Should stop at 'd' (end of world), not cross to new prompt
    assert_eq!(result.right, 5);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove line left basic"
#[test]
fn screen_prompt_click_move_line_left_basic() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Cursor is at column 7 (after 'o'), move it to column 6 (the 'o')
    s.cursor_absolute(6, 0);

    // Click on 'h' (column 2), should require 4 left movements (o->l->l->e->h)
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.left, 4);
    assert_eq!(result.right, 0);

    s.deinit();
}

// test "Screen: promptClickMove line left skips non-input cells"
#[test]
fn screen_prompt_click_move_line_left_skips_non_input_cells() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write: "> h" then output "X" then input "llo"
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"h").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"X").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"llo").unwrap();

    // Move cursor to column 6 (the 'o')
    s.cursor_absolute(6, 0);

    // Click on 'h' at column 2 - should skip the 'X' output cell
    // Movement: o->l->l->h = 3 left movements (skipping X)
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.left, 3);
    assert_eq!(result.right, 0);

    s.deinit();
}

// test "Screen: promptClickMove line left soft-wrapped line"
#[test]
fn screen_prompt_click_move_line_left_soft_wrapped_line() {
    let mut s = clone_clear_init(10, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input that wraps
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    // Write 8 chars of input, first row has 2 for prompt + 8 input = 10 cols
    s.test_write_string(b"abcdefgh").unwrap();
    // Continue on next row (soft-wrapped)
    s.test_write_string(b"ij").unwrap();

    // Verify soft wrap occurred
    {
        let contents = s
            .dump_string_alloc(
                Allocator,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(contents, b"> abcdefgh\nij");
    }

    // Cursor is at column 2, row 1 (after 'j'). Move to 'j' at column 1.
    s.cursor_absolute(1, 1);

    // Click on 'a' at column 2, row 0 - should count all input cells backwards
    // Movement: j->i->h->g->f->e->d->c->b->a = 9 left movements
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.left, 9);
    assert_eq!(result.right, 0);

    s.deinit();
}

// test "Screen: promptClickMove line left stops at hard wrap"
#[test]
fn screen_prompt_click_move_line_left_stops_at_hard_wrap() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write prompt and input on first line, then hard wrap
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();
    // Hard wrap (newline)
    s.test_write_string(b"\n").unwrap();
    s.test_write_string(b"world").unwrap();

    // Move cursor to 'd' at column 4, row 1 (an actual input cell)
    s.cursor_absolute(4, 1);

    // Click on 'h' at column 2, row 0 - but line mode stops at hard wrap
    // Should only move to start of second line, not cross to row 0
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    // Should stop at start of second line: d->l->r->o->w = 4 movements
    assert_eq!(result.left, 4);
    assert_eq!(result.right, 0);

    s.deinit();
}

// test "Screen: promptClickMove click right of input same line"
#[test]
fn screen_prompt_click_move_click_right_of_input_same_line() {
    // Set up: "> hello" where "> " is prompt and "hello" is input
    // Clicking to the right of the 'o' should move cursor past the input

    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Move cursor to start of input (column 2, the 'h')
    s.cursor_absolute(2, 0);

    // Click beyond the input (column 15) - should move to one past the 'o'
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 15, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 5);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove click right of input cursor at end"
#[test]
fn screen_prompt_click_move_click_right_of_input_cursor_at_end() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Cursor is already at column 7 (one past 'o') after writing
    // Click beyond the input (column 15) - no movement needed since
    // cursor is already at the end position
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 15, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 0);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove click right of input on lower line"
#[test]
fn screen_prompt_click_move_click_right_of_input_on_lower_line() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Move cursor to start of input (column 2, the 'h')
    s.cursor_absolute(2, 0);

    // Click on a lower line (row 1) - should move to end of input
    // This is outside the prompt area so should clamp to end
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 5, y: 1 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    // From 'h', we need to pass e, l, l, o (4 cells) + 1 past end = 5
    assert_eq!(result.right, 5);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove click right of input cursor at end lower line"
#[test]
fn screen_prompt_click_move_click_right_of_input_cursor_at_end_lower_line() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Cursor is at column 7 after writing (one past 'o')
    // Click on a lower line (row 1) - cursor already at end, no movement needed
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 5, y: 1 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 0);
    assert_eq!(result.left, 0);

    s.deinit();
}

// test "Screen: promptClickMove click right of input cursor on last char"
#[test]
fn screen_prompt_click_move_click_right_of_input_cursor_on_last_char() {
    let mut s = clone_clear_init(20, 5, 0);

    // Enable line click mode
    s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

    // Write a prompt and input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello").unwrap();

    // Move cursor to last input char (column 6, the 'o')
    s.cursor_absolute(6, 0);

    // Click beyond the input (column 15)
    let click_pin = s
        .pages
        .pin(point::Point::Active(point::Coordinate { x: 15, y: 0 }))
        .unwrap();
    let result = s.prompt_click_move(click_pin);

    assert_eq!(result.right, 1);
    assert_eq!(result.left, 0);

    s.deinit();
}
