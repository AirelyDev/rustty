//! PORTS: ghostty src/terminal/Screen.zig — the `resize` tests.

use super::*;

// test "Screen: resize (no reflow) more rows"
#[test]
fn screen_resize_no_reflow_more_rows() {
    let mut s = mkscreen(10, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    // Resize
    s.resize(Resize {
        cols: 10,
        rows: 10,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize (no reflow) less rows"
#[test]
fn screen_resize_no_reflow_less_rows() {
    let mut s = mkscreen(10, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();
    assert_eq!(5, s.cursor.x);
    assert_eq!(2, s.cursor.y);
    s.resize(Resize {
        cols: 10,
        rows: 2,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Since we shrunk, we should adjust our cursor
    assert_eq!(5, s.cursor.x);
    assert_eq!(1, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }
    s.deinit();
}

// test "Screen: resize (no reflow) less rows trims blank lines"
#[test]
fn screen_resize_no_reflow_less_rows_trims_blank_lines() {
    let mut s = mkscreen(10, 3, 0);
    let str = b"1ABCD";
    s.test_write_string(str).unwrap();

    // Write only a background color into the remaining rows
    for y in 1..s.pages.rows {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        unsafe {
            (*list_cell.cell).set_content_tag(pagepkg::ContentTag::BgColorRgb);
            (*list_cell.cell).set_content_color_rgb(pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0,
            });
        }
    }

    let cursor = s.cursor;
    s.resize(Resize {
        cols: 6,
        rows: 2,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD");
    }
    s.deinit();
}

// test "Screen: resize (no reflow) more rows trims blank lines"
#[test]
fn screen_resize_no_reflow_more_rows_trims_blank_lines() {
    let mut s = mkscreen(10, 3, 0);
    let str = b"1ABCD";
    s.test_write_string(str).unwrap();

    // Write only a background color into the remaining rows
    for y in 1..s.pages.rows {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        unsafe {
            (*list_cell.cell).set_content_tag(pagepkg::ContentTag::BgColorRgb);
            (*list_cell.cell).set_content_color_rgb(pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0,
            });
        }
    }

    let cursor = s.cursor;
    s.resize(Resize {
        cols: 10,
        rows: 7,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD");
    }
    s.deinit();
}

// test "Screen: resize (no reflow) more cols"
#[test]
fn screen_resize_no_reflow_more_cols() {
    let mut s = mkscreen(10, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 20,
        rows: 3,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize (no reflow) less cols"
#[test]
fn screen_resize_no_reflow_less_cols() {
    let mut s = mkscreen(10, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 4,
        rows: 3,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABC\n2EFG\n3IJK");
    }
    s.deinit();
}

// test "Screen: resize (no reflow) more rows with scrollback cursor end"
#[test]
fn screen_resize_no_reflow_more_rows_with_scrollback_cursor_end() {
    let mut s = mkscreen(7, 3, 2);
    let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 7,
        rows: 10,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize (no reflow) more rows no scrollback pull"
#[test]
fn screen_resize_no_reflow_more_rows_no_scrollback_pull() {
    let mut s = mkscreen(7, 3, 2);
    let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();

    // Cursor is at the bottom so this would normally pull scrollback.
    assert_eq!(2 as size::CellCountInt, s.cursor.y);
    s.resize(Resize {
        cols: 7,
        rows: 10,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: false,
    })
    .unwrap();
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize more cols no scrollback pull"
#[test]
fn screen_resize_more_cols_no_scrollback_pull() {
    let mut s = mkscreen(5, 3, 2);
    s.test_write_string(b"1AAAA\n2BBBB\n3CCCCDD\n4E").unwrap();
    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"3CCCC\nDD\n4E");
    }

    // The wrapped line in the active area unwraps, freeing up a row. This
    // would normally pull "2BBBB" back but we should get a blank row at
    // the bottom instead.
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: false,
    })
    .unwrap();
    assert_eq!(2 as size::CellCountInt, s.cursor.x);
    assert_eq!(1 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"3CCCCDD\n4E");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1AAAA\n2BBBB\n3CCCCDD\n4E");
    }
    s.deinit();
}

// test "Screen: resize more cols no scrollback pull wrap straddles scrollback"
#[test]
fn screen_resize_more_cols_no_scrollback_pull_wrap_straddles_scrollback() {
    let mut s = mkscreen(5, 3, 2);
    s.test_write_string(b"1AAAA\n2BBBBXX\n3C\n4D").unwrap();
    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"XX\n3C\n4D");
    }

    // The line isn't fully in scrollback so it is allowed to unwrap
    // back into view, but nothing above it is.
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: false,
    })
    .unwrap();
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"2BBBBXX\n3C\n4D");
    }
    s.deinit();
}

// test "Screen: resize more cols and rows no scrollback pull"
#[test]
fn screen_resize_more_cols_and_rows_no_scrollback_pull() {
    let mut s = mkscreen(5, 3, 2);
    s.test_write_string(b"1AAAA\n2BBBB\n3CCCCDD\n4E").unwrap();

    s.resize(Resize {
        cols: 10,
        rows: 5,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: false,
    })
    .unwrap();
    assert_eq!(1 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"3CCCCDD\n4E");
    }
    s.deinit();
}

// test "Screen: resize less cols no scrollback pull"
#[test]
fn screen_resize_less_cols_no_scrollback_pull() {
    let mut s = mkscreen(10, 3, 2);
    s.test_write_string(b"0Z\n1AAAA\n2BBBBXX\n3C").unwrap();

    // Wrapping needs more rows than we have so the top of the active
    // area still scrolls off as usual.
    s.resize(Resize {
        cols: 5,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: false,
    })
    .unwrap();
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"2BBBB\nXX\n3C");
    }
    s.deinit();
}

// test "Screen: resize (no reflow) less rows with scrollback"
#[test]
fn screen_resize_no_reflow_less_rows_with_scrollback() {
    let mut s = mkscreen(7, 3, 2);
    let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 7,
        rows: 2,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"4ABCD\n5EFGH");
    }
    s.deinit();
}

// https://github.com/mitchellh/ghostty/issues/1030
// test "Screen: resize (no reflow) less rows with empty trailing"
#[test]
fn screen_resize_no_reflow_less_rows_with_empty_trailing() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1\n2\n3\n4\n5\n6\n7\n8";
    s.test_write_string(str).unwrap();
    s.scroll_clear().unwrap();
    s.cursor_absolute(0, 0);
    s.test_write_string(b"A\nB").unwrap();

    let cursor = s.cursor;
    s.resize(Resize {
        cols: 5,
        rows: 2,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"A\nB");
    }
    s.deinit();
}

// test "Screen: resize (no reflow) more rows with soft wrapping"
#[test]
fn screen_resize_no_reflow_more_rows_with_soft_wrapping() {
    let mut s = mkscreen(2, 3, 3);
    let str = b"1A2B\n3C4E\n5F6G";
    s.test_write_string(str).unwrap();

    // Every second row should be wrapped
    for y in 0..6 {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        let wrapped = y % 2 == 0;
        assert_eq!(wrapped, unsafe { (*list_cell.row).wrap() });
    }

    // Resize
    s.resize(Resize {
        cols: 2,
        rows: 10,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1A\n2B\n3C\n4E\n5F\n6G");
    }

    // Every second row should be wrapped
    for y in 0..6 {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        let wrapped = y % 2 == 0;
        assert_eq!(wrapped, unsafe { (*list_cell.row).wrap() });
    }
    s.deinit();
}

// test "Screen: resize more rows no scrollback"
#[test]
fn screen_resize_more_rows_no_scrollback() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();
    let cursor = s.cursor;
    s.resize(Resize {
        cols: 5,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize more rows with empty scrollback"
#[test]
fn screen_resize_more_rows_with_empty_scrollback() {
    let mut s = mkscreen(5, 3, 10);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();
    let cursor = s.cursor;
    s.resize(Resize {
        cols: 5,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize more rows with populated scrollback"
#[test]
fn screen_resize_more_rows_with_populated_scrollback() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }

    // Set our cursor to be on the "4"
    s.cursor_absolute(0, 1);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('4' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        cols: 5,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should still be on the "4"
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('4' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }
    s.deinit();
}

// test "Screen: resize more cols no reflow"
#[test]
fn screen_resize_more_cols_no_reflow() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    let cursor = s.cursor;
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// https://github.com/mitchellh/ghostty/issues/272#issuecomment-1676038963
// test "Screen: resize more cols perfect split"
#[test]
fn screen_resize_more_cols_perfect_split() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD2EFGH3IJKL";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1ABCD2EFGH\n3IJKL");
    }
    s.deinit();
}

// https://github.com/mitchellh/ghostty/issues/1159
// test "Screen: resize (no reflow) more cols with scrollback scrolled up"
#[test]
fn screen_resize_no_reflow_more_cols_with_scrollback_scrolled_up() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1\n2\n3\n4\n5\n6\n7\n8";
    s.test_write_string(str).unwrap();

    // Cursor at bottom
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    s.scroll(Scroll::DeltaRow(-4));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2\n3\n4");
    }

    s.resize(Resize {
        cols: 8,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }

    // Cursor remains at bottom
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// https://github.com/mitchellh/ghostty/issues/1159
// test "Screen: resize (no reflow) less cols with scrollback scrolled up"
#[test]
fn screen_resize_no_reflow_less_cols_with_scrollback_scrolled_up() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1\n2\n3\n4\n5\n6\n7\n8";
    s.test_write_string(str).unwrap();

    // Cursor at bottom
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    s.scroll(Scroll::DeltaRow(-4));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2\n3\n4");
    }

    s.resize(Resize {
        cols: 4,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"6\n7\n8");
    }

    // Cursor remains at bottom
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    // Old implementation doesn't do this but it makes sense to me:
    // {
    //     const contents = try s.dumpStringAlloc(alloc, .{ .viewport = .{} });
    //     defer alloc.free(contents);
    //     try testing.expectEqualStrings("2\n3\n4", contents);
    // }
    s.deinit();
}

// test "Screen: resize more cols no reflow preserves semantic prompt"
#[test]
fn screen_resize_more_cols_no_reflow_preserves_semantic_prompt() {
    let mut s = mkscreen(5, 3, 0);

    // Set one of the rows to be a prompt
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"1ABCD\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"2EFGH").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"\n3IJKL").unwrap();

    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: false,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    let expected = b"1ABCD\n2EFGH\n3IJKL";
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, expected);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, expected);
    }

    // Our one row should still be a semantic prompt, the others should not.
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        assert!(unsafe { (*list_cell.row).semantic_prompt() } == pagepkg::SemanticPrompt::None);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        assert!(unsafe { (*list_cell.row).semantic_prompt() } == pagepkg::SemanticPrompt::Prompt);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        assert!(unsafe { (*list_cell.row).semantic_prompt() } == pagepkg::SemanticPrompt::None);
    }
    s.deinit();
}

// test "Screen: resize more cols with reflow that fits full width"
#[test]
fn screen_resize_more_cols_with_reflow_that_fits_full_width() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    // Verify we soft wrapped
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Let's put our cursor on row 2, where the soft wrap is
    s.cursor_absolute(0, 1);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('2' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    // Resize and verify we undid the soft wrap because we have space now
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }

    // Our cursor should've moved
    assert_eq!(5, s.cursor.x);
    assert_eq!(0, s.cursor.y);
    s.deinit();
}

// test "Screen: resize more cols with reflow that ends in newline"
#[test]
fn screen_resize_more_cols_with_reflow_that_ends_in_newline() {
    let mut s = mkscreen(6, 3, 0);
    let str = b"1ABCD2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    // Verify we soft wrapped
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD2\nEFGH\n3IJKL");
    }

    // Let's put our cursor on the last row
    s.cursor_absolute(0, 2);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('3' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    // Resize and verify we undid the soft wrap because we have space now
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }

    // Our cursor should still be on the 3
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('3' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }
    s.deinit();
}

// test "Screen: resize more cols with reflow that forces more wrapping"
#[test]
fn screen_resize_more_cols_with_reflow_that_forces_more_wrapping() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    // Let's put our cursor on row 2, where the soft wrap is
    s.cursor_absolute(0, 1);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('2' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    // Verify we soft wrapped
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Resize and verify we undid the soft wrap because we have space now
    s.resize(Resize {
        cols: 7,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD2E\nFGH\n3IJKL");
    }

    // Our cursor should've moved
    assert_eq!(5 as size::CellCountInt, s.cursor.x);
    assert_eq!(0 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize more cols with reflow that unwraps multiple times"
#[test]
fn screen_resize_more_cols_with_reflow_that_unwraps_multiple_times() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD2EFGH3IJKL";
    s.test_write_string(str).unwrap();

    // Let's put our cursor on row 2, where the soft wrap is
    s.cursor_absolute(0, 2);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('3' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    // Verify we soft wrapped
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Resize and verify we undid the soft wrap because we have space now
    s.resize(Resize {
        cols: 15,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD2EFGH3IJKL");
    }

    // Our cursor should've moved
    assert_eq!(10 as size::CellCountInt, s.cursor.x);
    assert_eq!(0 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize more cols with populated scrollback"
#[test]
fn screen_resize_more_cols_with_populated_scrollback() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD5EFGH";
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }

    // // Set our cursor to be on the "5"
    s.cursor_absolute(0, 2);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('5' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD5EFGH");
    }

    // Cursor should still be on the "5"
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('5' as u21, unsafe { (*list_cell.cell).content_codepoint() });
    }
    s.deinit();
}

// test "Screen: resize more cols bounded scrollback keeps viewport valid"
#[test]
fn screen_resize_more_cols_bounded_scrollback_keeps_viewport_valid() {
    // Regression test for issue #12298.
    //
    // This needs to live at the Screen layer rather than PageList because the
    // bad state only appears once Screen forwards the active cursor into the
    // resize path. A direct PageList resize repro does not hit the same bug.
    let mut s = mkscreen(2, 10, 10_000);

    // Build 30 rows of scrollback on top of our 10-row viewport so we have a
    // 40-row screen with history above the active area.
    for _ in 0..30 {
        let _ = s.pages.grow().unwrap();
    }
    s.cursor_reload();
    assert_eq!(40usize, s.pages.scrollbar().total);

    // Fill the entire screen with two-row wrapped runs:
    // - even rows mark the end of a wrapped line
    // - odd rows mark the continuation
    //
    // With 2 columns, each logical line occupies two rows. When we grow to 4
    // columns with reflow enabled, those pairs unwrap back into single rows.
    // That cuts the total row count down and is what stresses the viewport pin.
    let cols = s.pages.cols;
    let mut it = s
        .pages
        .page_iterator(pagelist::iter::Direction::RightDown, scr(), None);
    while let Some(chunk) = it.next() {
        let page: *mut Page = unsafe { crate::pagelist::Node::page(chunk.node) };
        for y in chunk.start..chunk.end {
            let rac = unsafe { (*page).get_row_and_cell(0, y as usize) };
            if y % 2 == 0 {
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            for x in 0..cols {
                let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
                unsafe { (*rac.cell).set_content_codepoint('A' as u21) };
            }
        }
    }

    // Pin the viewport to a history row just above the active area.
    //
    // Before resize:
    // - total rows = 40
    // - active area starts at row 30
    // - viewport is pinned at row 28
    //
    // After unwrap during resize:
    // - total rows shrinks to 20
    // - the old row 28 remaps into what is now the active area
    //
    // The bug was that resize/grow would temporarily keep the viewport as a
    // history pin even after reflow had moved it into the active area, leaving
    // fewer than `rows` visible rows beneath the pin and tripping integrity
    // checks.
    s.pages.scroll(pagelist::scroll::Scroll::Pin(
        s.pages
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 28 }))
            .unwrap(),
    ));
    assert!(s.pages.viewport == pagelist::Viewport::Pin);
    assert!(s.pages.get_bottom_right(point::Tag::Viewport).is_some());

    // Growing columns triggers reflow, which unwraps the synthetic wrapped
    // rows above. This used to panic during the resize path.
    let rows = s.pages.rows;
    s.resize(Resize {
        cols: 4,
        rows,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // After the fix, the viewport is normalized back to the active area as
    // soon as the pinned row lands there, so viewport queries remain valid.
    assert_eq!(4usize, s.pages.cols as usize);
    assert!(s.pages.scrollbar().total < 40);
    assert!(s.pages.viewport == pagelist::Viewport::Active);
    assert!(s.pages.get_bottom_right(point::Tag::Viewport).is_some());
    s.deinit();
}

// test "Screen: resize more cols with reflow"
#[test]
fn screen_resize_more_cols_with_reflow() {
    let mut s = mkscreen(2, 3, 5);
    let str = b"1ABC\n2DEF\n3ABC\n4DEF";
    s.test_write_string(str).unwrap();

    // Let's put our cursor on row 2, where the soft wrap is
    s.cursor_absolute(0, 2);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('E' as u32, unsafe {
            (*list_cell.cell).content_codepoint() as u32
        });
    }

    // Verify we soft wrapped
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"BC\n4D\nEF");
    }

    // Resize and verify we undid the soft wrap because we have space now
    s.resize(Resize {
        cols: 7,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1ABC\n2DEF\n3ABC\n4DEF");
    }

    // Our cursor should've moved
    assert_eq!(2 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize errors preserve state"
//
// WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
// fail point. `tripwire::Module::enabled()` is the union of `cfg!(test)`,
// `cfg!(debug_assertions)` and the `tripwire-test` feature, which this crate
// turns on as a DEV-dependency (Cargo.toml) — reproducing Zig's
// `builtin.is_test`, true at ANY optimize mode. So this runs in BOTH the debug
// and the release test lanes, like the reference test does. No `ignore`.
#[test]
fn screen_resize_errors_preserve_state() {
    // for (std.meta.tags(resize_tw.FailPoint)) |tag| { ... }
    for tag in ResizeFp::ALL {
        let mut s = mkscreen(10, 3, 0);

        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"echo").unwrap();
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.start_hyperlink(b"https://example.com", Some(b"resize"))
            .unwrap();
        s.saved_cursor = Some(SavedCursor {
            x: 1,
            y: 0,
            style: s.cursor.style,
            protected: s.cursor.protected,
            pending_wrap: s.cursor.pending_wrap,
            origin: false,
            charset: s.charset.clone(),
        });

        if crate::options::KITTY_GRAPHICS {
            s.kitty_images.dirty = false;
        }

        // Keep a shallow copy for all non-page state and a byte-for-byte
        // copy of the sole page so reference counts and prompt contents are
        // covered as well.
        assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        // PORT NOTE: `const before = s;` is a Zig by-value struct copy
        // compared with `std.meta.eql`. `Screen` is not `Copy` in Rust and
        // most of its members have no `PartialEq`, so we snapshot exactly
        // the fields the reference then compares, field by field.
        let before_cursor = s.cursor;
        let before_saved_cursor = {
            let sc = s.saved_cursor.as_ref().unwrap();
            (sc.x, sc.y, sc.protected, sc.pending_wrap, sc.origin)
        };
        // The reference compares the WHOLE saved cursor with
        // `std.meta.eql`, which also covers `style` and `charset`; neither
        // has `PartialEq` here, so snapshot them alongside the tuple above.
        let before_saved_style = s.saved_cursor.as_ref().unwrap().style;
        let before_saved_charset = {
            let cs = &s.saved_cursor.as_ref().unwrap().charset;
            (
                cs.gl,
                cs.gr,
                cs.single_shift,
                cs.charsets.g0,
                cs.charsets.g1,
                cs.charsets.g2,
                cs.charsets.g3,
            )
        };
        let before_selection_is_none = s.selection.is_none();
        // `std.meta.eql(before.charset, s.charset)` also covers the
        // charsets table, so compare g0..g3 too.
        let before_charset = (
            s.charset.gl,
            s.charset.gr,
            s.charset.single_shift,
            s.charset.charsets.g0,
            s.charset.charsets.g1,
            s.charset.charsets.g2,
            s.charset.charsets.g3,
        );
        let before_protected_mode = s.protected_mode;
        let before_kitty_keyboard = s.kitty_keyboard;
        let before_semantic_prompt = (s.semantic_prompt.seen, s.semantic_prompt.click);
        let before_dirty = (s.dirty.selection, s.dirty.hyperlink_hover);
        let before_pages_first = s.pages.pages.first;
        let before_pages_last = s.pages.pages.last;
        let before_cols = s.pages.cols;
        let before_rows = s.pages.rows;
        let before_total_rows = s.pages.total_rows;
        let before_viewport = s.pages.viewport;
        let before_viewport_pin = unsafe { *s.pages.viewport_pin };
        let before_tracked_pins = s.pages.count_tracked_pins();
        let before_page: Vec<u8> = unsafe {
            let page: *const Page = crate::pagelist::Node::page(s.pages.pages.first);
            core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
        };
        let before_kitty_dirty = s.kitty_images.dirty;

        resize_tw_error_always(tag);
        assert!(matches!(
            s.resize(Resize {
                cols: 20,
                rows: 4,
                reflow: true,
                prompt_redraw: osc::semantic_prompt::Redraw::True,
                pull_scrollback: true,
            }),
            Err(OutOfMemory)
        ));

        assert_eq!(before_cursor.x, s.cursor.x);
        assert_eq!(before_cursor.y, s.cursor.y);
        assert!(before_cursor.cursor_style == s.cursor.cursor_style);
        assert_eq!(before_cursor.pending_wrap, s.cursor.pending_wrap);
        assert_eq!(before_cursor.protected, s.cursor.protected);
        assert!(before_cursor.style.eql(&s.cursor.style));
        assert_eq!(before_cursor.style_id, s.cursor.style_id);
        assert_eq!(before_cursor.hyperlink_id, s.cursor.hyperlink_id);
        assert_eq!(
            before_cursor.hyperlink_implicit_id,
            s.cursor.hyperlink_implicit_id
        );
        assert_eq!(before_cursor.hyperlink, s.cursor.hyperlink);
        assert!(before_cursor.semantic_content == s.cursor.semantic_content);
        assert_eq!(
            before_cursor.semantic_content_clear_eol,
            s.cursor.semantic_content_clear_eol
        );
        assert!(std::ptr::eq(before_cursor.page_pin, s.cursor.page_pin));
        assert!(std::ptr::eq(before_cursor.page_row, s.cursor.page_row));
        assert!(std::ptr::eq(before_cursor.page_cell, s.cursor.page_cell));
        {
            let sc = s.saved_cursor.as_ref().unwrap();
            assert_eq!(
                before_saved_cursor,
                (sc.x, sc.y, sc.protected, sc.pending_wrap, sc.origin)
            );
            assert!(before_saved_style.eql(&sc.style));
            assert_eq!(
                before_saved_charset,
                (
                    sc.charset.gl,
                    sc.charset.gr,
                    sc.charset.single_shift,
                    sc.charset.charsets.g0,
                    sc.charset.charsets.g1,
                    sc.charset.charsets.g2,
                    sc.charset.charsets.g3,
                )
            );
        }
        assert_eq!(before_selection_is_none, s.selection.is_none());
        assert_eq!(
            before_charset,
            (
                s.charset.gl,
                s.charset.gr,
                s.charset.single_shift,
                s.charset.charsets.g0,
                s.charset.charsets.g1,
                s.charset.charsets.g2,
                s.charset.charsets.g3,
            )
        );
        assert!(before_protected_mode == s.protected_mode);
        assert_eq!(before_kitty_keyboard, s.kitty_keyboard);
        assert_eq!(
            before_semantic_prompt,
            (s.semantic_prompt.seen, s.semantic_prompt.click)
        );
        assert_eq!(before_dirty, (s.dirty.selection, s.dirty.hyperlink_hover));
        assert!(std::ptr::eq(before_pages_first, s.pages.pages.first));
        assert!(std::ptr::eq(before_pages_last, s.pages.pages.last));
        assert_eq!(before_cols, s.pages.cols);
        assert_eq!(before_rows, s.pages.rows);
        assert_eq!(before_total_rows, s.pages.total_rows);
        assert!(before_viewport == s.pages.viewport);
        assert!(before_viewport_pin.eql(unsafe { *s.pages.viewport_pin }));
        assert_eq!(before_tracked_pins, s.pages.count_tracked_pins());
        let after_page: Vec<u8> = unsafe {
            let page: *const Page = crate::pagelist::Node::page(s.pages.pages.first);
            core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
        };
        // if (std.valgrind.runningOnValgrind() > 0) {
        //     // This assertion deliberately compares the complete raw page,
        //     // including semantically irrelevant struct padding.
        //     std.valgrind.memcheck.makeMemDefined(before_page);
        //     std.valgrind.memcheck.makeMemDefined(
        //         s.pages.pages.first.?.page().memory,
        //     );
        // }
        // (the Valgrind makeMemDefined calls have no Rust analogue — we
        // never run under Valgrind; the assertion below is that same
        // complete-raw-page compare, padding included.)
        assert!(
            before_page == after_page,
            "page memory changed after a failed resize"
        );
        if crate::options::KITTY_GRAPHICS {
            assert_eq!(before_kitty_dirty, s.kitty_images.dirty);
        }

        resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
        s.deinit();
    }
}

// test "Screen: resize cursor references when node survives"
#[test]
fn screen_resize_cursor_references_when_node_survives() {
    let mut s = mkscreen(5, 3, 1000);

    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.start_hyperlink(b"https://example.com/", Some(b"resize"))
        .unwrap();
    s.test_write_string(b"abc").unwrap();

    let original_node = unsafe { (*s.cursor.page_pin).node };
    let original_serial = unsafe { (*original_node).serial };
    {
        let page = unsafe { &(*crate::pagelist::Node::page(original_node)) };
        let base = page.memory_ptr();
        assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
        assert_eq!(4, unsafe {
            page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
        });
    }

    // A row-only resize grows the existing page without replacing the
    // cursor's node. The temporary resize references must be released from
    // this page after the cursor state is restored.
    s.resize(Resize {
        cols: 5,
        rows: 4,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    assert!(std::ptr::eq(original_node, unsafe {
        (*s.cursor.page_pin).node
    }));
    assert_eq!(original_serial, unsafe {
        (*(*s.cursor.page_pin).node).serial
    });
    {
        let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
        let base = page.memory_ptr();
        assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
        assert_eq!(4, unsafe {
            page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
        });
    }
    s.deinit();
}

// test "Screen: resize cursor references when node is replaced"
#[test]
fn screen_resize_cursor_references_when_node_is_replaced() {
    let mut s = mkscreen(5, 3, 1000);

    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.start_hyperlink(b"https://example.com/", Some(b"resize"))
        .unwrap();
    s.test_write_string(b"abc").unwrap();

    let original_node = unsafe { (*s.cursor.page_pin).node };
    let original_serial = unsafe { (*original_node).serial };
    {
        let page = unsafe { &(*crate::pagelist::Node::page(original_node)) };
        let base = page.memory_ptr();
        assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
        assert_eq!(4, unsafe {
            page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
        });
    }

    // A column resize with reflow replaces the page and remaps the tracked
    // cursor pin. The old page owns the temporary references, so destroying
    // it must account for them without attempting to release them afterward.
    s.resize(Resize {
        cols: 10,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    let live_node = unsafe { (*s.cursor.page_pin).node };
    assert!(
        !std::ptr::eq(live_node, original_node)
            || unsafe { (*live_node).serial } != original_serial
    );
    {
        let page = unsafe { &(*crate::pagelist::Node::page(live_node)) };
        let base = page.memory_ptr();
        assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
        assert_eq!(4, unsafe {
            page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
        });
    }
    s.deinit();
}

// test "Screen: resize more rows and cols with wrapping"
#[test]
fn screen_resize_more_rows_and_cols_with_wrapping() {
    let mut s = mkscreen(2, 4, 0);
    let str = b"1A2B\n3C4D";
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1A\n2B\n3C\n4D");
    }

    s.resize(Resize {
        cols: 5,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should move due to wrapping
    assert_eq!(3 as size::CellCountInt, s.cursor.x);
    assert_eq!(1 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize less rows no scrollback"
#[test]
fn screen_resize_less_rows_no_scrollback() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    s.cursor_absolute(0, 0);
    let cursor = s.cursor;
    s.resize(Resize {
        cols: 5,
        rows: 1,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"3IJKL");
    }
    s.deinit();
}

// test "Screen: resize less rows moving cursor"
#[test]
fn screen_resize_less_rows_moving_cursor() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    // Put our cursor on the last line
    s.cursor_absolute(1, 2);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('I' as u32, unsafe {
            (*list_cell.cell).content_codepoint() as u32
        });
    }

    // Resize
    s.resize(Resize {
        cols: 5,
        rows: 1,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"3IJKL");
    }

    // Cursor should be on the last line
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(0 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize less rows with empty scrollback"
#[test]
fn screen_resize_less_rows_with_empty_scrollback() {
    let mut s = mkscreen(5, 3, 10);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 5,
        rows: 1,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL");
    }
    s.deinit();
}

// test "Screen: resize less rows with populated scrollback"
#[test]
fn screen_resize_less_rows_with_populated_scrollback() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }

    // Resize
    s.resize(Resize {
        cols: 5,
        rows: 1,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"5EFGH");
    }
    s.deinit();
}

// test "Screen: resize less rows with full scrollback"
#[test]
fn screen_resize_less_rows_with_full_scrollback() {
    let mut s = mkscreen(5, 3, 3);
    let str = b"00000\n1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }

    assert_eq!(4 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    // Resize
    s.resize(Resize {
        cols: 5,
        rows: 2,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should stay in the same relative place (bottom of the
    // screen, same character).
    assert_eq!(4 as size::CellCountInt, s.cursor.x);
    assert_eq!(1 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"00000\n1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH");
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"4ABCD\n5EFGH");
    }
    s.deinit();
}

// test "Screen: resize less cols no reflow"
#[test]
fn screen_resize_less_cols_no_reflow() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1AB\n2EF\n3IJ";
    s.test_write_string(str).unwrap();

    s.cursor_absolute(0, 0);
    let cursor = s.cursor;
    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(cursor.x, s.cursor.x);
    assert_eq!(cursor.y, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize less cols with reflow but row space"
#[test]
fn screen_resize_less_cols_with_reflow_but_row_space() {
    let mut s = mkscreen(5, 3, 1);
    let str = b"1ABCD";
    s.test_write_string(str).unwrap();

    // Put our cursor on the end
    s.cursor_absolute(4, 0);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('D' as u32, unsafe {
            (*list_cell.cell).content_codepoint() as u32
        });
    }

    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1AB\nCD");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1AB\nCD");
    }

    // Cursor should be on the last line
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(1 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize less cols with reflow with trimmed rows"
#[test]
fn screen_resize_less_cols_with_reflow_with_trimmed_rows() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"CD\n5EF\nGH");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"CD\n5EF\nGH");
    }
    s.deinit();
}

// test "Screen: resize less cols with reflow with trimmed rows and scrollback"
#[test]
fn screen_resize_less_cols_with_reflow_with_trimmed_rows_and_scrollback() {
    let mut s = mkscreen(5, 3, 1);
    let str = b"3IJKL\n4ABCD\n5EFGH";
    s.test_write_string(str).unwrap();
    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"CD\n5EF\nGH");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"3IJ\nKL\n4AB\nCD\n5EF\nGH");
    }
    s.deinit();
}

// test "Screen: resize less cols with reflow previously wrapped"
#[test]
fn screen_resize_less_cols_with_reflow_previously_wrapped() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"3IJKL4ABCD5EFGH";
    s.test_write_string(str).unwrap();

    // Check
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }

    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    // {
    //     const contents = try s.testString(alloc, .viewport);
    //     defer alloc.free(contents);
    //     const expected = "CD\n5EF\nGH";
    //     try testing.expectEqualStrings(expected, contents);
    // }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"ABC\nD5E\nFGH");
    }
    s.deinit();
}

// test "Screen: resize less cols with reflow and scrollback"
#[test]
fn screen_resize_less_cols_with_reflow_and_scrollback() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1A\n2B\n3C\n4D\n5E";
    s.test_write_string(str).unwrap();

    // Put our cursor on the end
    s.cursor_absolute(1, s.pages.rows - 1);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('E' as u32, unsafe {
            (*list_cell.cell).content_codepoint() as u32
        });
    }

    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3C\n4D\n5E");
    }

    // Cursor should be on the last line
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize less cols with reflow previously wrapped and scrollback"
#[test]
fn screen_resize_less_cols_with_reflow_previously_wrapped_and_scrollback() {
    let mut s = mkscreen(5, 3, 2);
    let str = b"1ABCD2EFGH3IJKL4ABCD5EFGH";
    s.test_write_string(str).unwrap();

    // Check
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
    }

    // Put our cursor on the end
    s.cursor_absolute(s.pages.cols - 1, s.pages.rows - 1);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('H' as u32, unsafe {
            (*list_cell.cell).content_codepoint() as u32
        });
    }

    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"CD5\nEFG\nH");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1AB\nCD2\nEFG\nH3I\nJKL\n4AB\nCD5\nEFG\nH");
    }

    // Cursor should be on the last line
    assert_eq!(0 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate {
                x: s.cursor.x,
                y: s.cursor.y as u32,
            }))
            .unwrap();
        assert_eq!('H' as u32, unsafe {
            (*list_cell.cell).content_codepoint() as u32
        });
    }
    s.deinit();
}

// test "Screen: resize less cols with scrollback keeps cursor row"
#[test]
fn screen_resize_less_cols_with_scrollback_keeps_cursor_row() {
    let mut s = mkscreen(5, 3, 5);
    let str = b"1A\n2B\n3C\n4D\n5E";
    s.test_write_string(str).unwrap();

    // Lets do a scroll and clear operation
    s.scroll_clear().unwrap();

    // Move our cursor to the beginning
    s.cursor_absolute(0, 0);

    s.resize(Resize {
        cols: 3,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }

    // Cursor should be on the last line
    assert_eq!(0 as size::CellCountInt, s.cursor.x);
    assert_eq!(0 as size::CellCountInt, s.cursor.y);
    s.deinit();
}

// test "Screen: resize more rows, less cols with reflow with scrollback"
#[test]
fn screen_resize_more_rows_less_cols_with_reflow_with_scrollback() {
    let mut s = mkscreen(5, 3, 3);
    let str = b"1ABCD\n2EFGH3IJKL\n4MNOP";
    s.test_write_string(str).unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL\n4MNOP");
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n4MNOP");
    }

    s.resize(Resize {
        cols: 2,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"BC\nD\n2E\nFG\nH3\nIJ\nKL\n4M\nNO\nP");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1A\nBC\nD\n2E\nFG\nH3\nIJ\nKL\n4M\nNO\nP");
    }
    s.deinit();
}

// This seems like it should work fine but for some reason in practice
// in the initial implementation I found this bug! This is a regression
// test for that.
// test "Screen: resize more rows then shrink again"
#[test]
fn screen_resize_more_rows_then_shrink_again() {
    let mut s = mkscreen(5, 3, 10);
    let str = b"1ABC";
    s.test_write_string(str).unwrap();

    // Grow
    s.resize(Resize {
        cols: 5,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }

    // Shrink
    s.resize(Resize {
        cols: 5,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }

    // Grow again
    s.resize(Resize {
        cols: 5,
        rows: 10,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, str);
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    s.deinit();
}

// test "Screen: resize less cols to eliminate wide char"
#[test]
fn screen_resize_less_cols_to_eliminate_wide_char() {
    let mut s = mkscreen(2, 1, 0);
    let str = "😀".as_bytes();
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        assert_eq!('😀' as u21, unsafe {
            (*list_cell.cell).content_codepoint()
        });
    }

    // Resize to 1 column can't fit a wide char. So it should be deleted.
    s.resize(Resize {
        cols: 1,
        rows: 1,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        assert_eq!(0 as u21, unsafe { (*list_cell.cell).content_codepoint() });
        assert_eq!(pagepkg::Wide::Narrow, unsafe { (*list_cell.cell).wide() });
    }
    s.deinit();
}

// test "Screen: resize less cols to wrap wide char"
#[test]
fn screen_resize_less_cols_to_wrap_wide_char() {
    let mut s = mkscreen(3, 3, 0);
    let str = "x😀".as_bytes();
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        assert_eq!('😀' as u21, unsafe {
            (*list_cell.cell).content_codepoint()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    s.resize(Resize {
        cols: 2,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, "x\n😀".as_bytes());
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
            (*list_cell.cell).wide()
        });
        assert!(unsafe { (*list_cell.row).wrap() });
    }
    s.deinit();
}

// test "Screen: resize less cols to eliminate wide char with row space"
#[test]
fn screen_resize_less_cols_to_eliminate_wide_char_with_row_space() {
    let mut s = mkscreen(2, 2, 0);
    let str = "😀".as_bytes();
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        assert_eq!('😀' as u21, unsafe {
            (*list_cell.cell).content_codepoint()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    s.resize(Resize {
        cols: 1,
        rows: 2,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"");
    }
    s.deinit();
}

// test "Screen: resize less cols reflows cursor after wrapped text"
#[test]
fn screen_resize_less_cols_reflows_cursor_after_wrapped_text() {
    let mut s = mkscreen(50, 7, 0);

    for _ in 0..30 {
        s.test_write_string(b"a").unwrap();
    }

    assert_eq!(0, s.cursor.y);
    assert_eq!(30, s.cursor.x);

    s.resize(Resize {
        cols: 25,
        rows: 7,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    assert_eq!(1, s.cursor.y);
    assert_eq!(5, s.cursor.x);
    s.deinit();
}

// test "Screen: resize less cols reflows cursor after empty cells"
#[test]
fn screen_resize_less_cols_reflows_cursor_after_empty_cells() {
    let mut s = mkscreen(10, 3, 0);

    s.test_write_string(b"abc").unwrap();
    s.cursor_right(6);

    assert_eq!(0, s.cursor.y);
    assert_eq!(9, s.cursor.x);

    s.resize(Resize {
        cols: 5,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();

    assert_eq!(1, s.cursor.y);
    assert_eq!(4, s.cursor.x);
    s.deinit();
}

// test "Screen: resize more cols with wide spacer head"
#[test]
fn screen_resize_more_cols_with_wide_spacer_head() {
    let mut s = mkscreen(3, 2, 0);
    let str = "  😀".as_bytes();
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, "  \n😀".as_bytes());
    }

    // So this is the key point: we end up with a wide spacer head at
    // the end of row 1, then the emoji, then a wide spacer tail on row 2.
    // We should expect that if we resize to more cols, the wide spacer
    // head is replaced with the emoji.
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
            (*list_cell.cell).wide()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    s.resize(Resize {
        cols: 4,
        rows: 2,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        assert_eq!('😀' as u21, unsafe {
            (*list_cell.cell).content_codepoint()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 3, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }
    s.deinit();
}

// test "Screen: resize more cols with wide spacer head multiple lines"
#[test]
fn screen_resize_more_cols_with_wide_spacer_head_multiple_lines() {
    let mut s = mkscreen(3, 3, 0);
    let str = "xxxyy😀".as_bytes();
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, "xxx\nyy\n😀".as_bytes());
    }

    // Similar to the "wide spacer head" test, but this time we'er going
    // to increase our columns such that multiple rows are unwrapped.
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
            (*list_cell.cell).wide()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 2 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    s.resize(Resize {
        cols: 8,
        rows: 2,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, str);
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 5, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        assert_eq!('😀' as u21, unsafe {
            (*list_cell.cell).content_codepoint()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 6, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }
    s.deinit();
}

// test "Screen: resize more cols requiring a wide spacer head"
#[test]
fn screen_resize_more_cols_requiring_a_wide_spacer_head() {
    let mut s = mkscreen(2, 2, 0);
    let str = "xx😀".as_bytes();
    s.test_write_string(str).unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, "xx\n😀".as_bytes());
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    // This resizes to 3 columns, which isn't enough space for our wide
    // char to enter row 1. But we need to mark the wide spacer head on the
    // end of the first row since we're wrapping to the next row.
    s.resize(Resize {
        cols: 3,
        rows: 2,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::False,
        pull_scrollback: true,
    })
    .unwrap();
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, "xx\n😀".as_bytes());
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
            (*list_cell.cell).wide()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        assert_eq!('😀' as u21, unsafe {
            (*list_cell.cell).content_codepoint()
        });
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 1 }))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }
    s.deinit();
}

// test "Screen: resize more cols with cursor at prompt"
#[test]
fn screen_resize_more_cols_with_cursor_at_prompt() {
    let mut s = mkscreen(10, 3, 5);

    s.test_write_string(b"ABCDE\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(SemanticContentInput::ClearEol));
    s.test_write_string(b"echo").unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"ABCDE\n> echo");
    }

    s.resize(Resize {
        cols: 20,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::True,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(6, s.cursor.x);
    assert_eq!(1, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"ABCDE");
    }
    s.deinit();
}

// test "Screen: resize more cols with cursor not at prompt"
#[test]
fn screen_resize_more_cols_with_cursor_not_at_prompt() {
    let mut s = mkscreen(10, 3, 5);

    s.test_write_string(b"ABCDE\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(SemanticContentInput::ClearEol));
    s.test_write_string(b"echo\n").unwrap();
    s.test_write_string(b"output").unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"ABCDE\n> echo\noutput");
    }

    s.resize(Resize {
        cols: 20,
        rows: 3,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::True,
        pull_scrollback: true,
    })
    .unwrap();

    // Cursor should not move
    assert_eq!(6, s.cursor.x);
    assert_eq!(2, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"ABCDE\n> echo\noutput");
    }
    s.deinit();
}

// test "Screen: resize with prompt_redraw last clears only one line"
#[test]
fn screen_resize_with_prompt_redraw_last_clears_only_one_line() {
    let mut s = mkscreen(10, 4, 5);

    s.test_write_string(b"ABCDE\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"> ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"hello\n").unwrap();
    s.test_write_string(b"world").unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"ABCDE\n> hello\nworld");
    }

    // Cursor is at end of "world" line with semantic_content = .input
    s.resize(Resize {
        cols: 20,
        rows: 4,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::Last,
        pull_scrollback: true,
    })
    .unwrap();

    // With .last, only the current line where cursor is should be cleared
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"ABCDE\n> hello");
    }
    s.deinit();
}

// test "Screen: resize with prompt_redraw last multiline prompt clears only last line"
#[test]
fn screen_resize_with_prompt_redraw_last_multiline_prompt_clears_only_last_line() {
    let mut s = mkscreen(20, 5, 5);

    // Create a 3-line prompt: 1 initial + 2 continuation lines
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"line1\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Continuation,
    ));
    s.test_write_string(b"line2\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Continuation,
    ));
    s.test_write_string(b"line3").unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"line1\nline2\nline3");
    }

    // Cursor is at end of line3 (the last continuation line)
    s.resize(Resize {
        cols: 30,
        rows: 5,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::Last,
        pull_scrollback: true,
    })
    .unwrap();

    // With .last, only line3 (where cursor is) should be cleared
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"line1\nline2");
    }
    s.deinit();
}

// S134-F1: `clear_prompt_for_redraw`'s `Redraw::True` arm logs
// `log.warn("cursor on prompt line but promptIterator found no prompt",
// .{})` (Screen.zig:2264) when the cursor is FLAGGED semantic-prompt
// content but no row is actually tagged with a `SemanticPrompt` marker
// for `prompt_iterator` to find — the exact "cursor.semantic_content
// says prompt, but the row's own tag was never set" mismatch the
// function's own doc comment describes (some shells mark input areas but
// not every continuation line). Reproduced directly, without going
// through `cursor_set_semantic_content` (which sets BOTH fields
// consistently and would never trigger this path).
#[test]
fn screen_resize_prompt_redraw_true_logs_when_no_prompt_row_is_tagged() {
    let mut s = mkscreen(20, 5, 5);
    s.test_write_string(b"line1").unwrap();

    // Flag the cursor as prompt content WITHOUT tagging any row's
    // `SemanticPrompt`, so `prompt_iterator` finds nothing.
    s.cursor.semantic_content = pagepkg::SemanticContent::Prompt;

    let cap = crate::log_capture::start();
    s.resize(Resize {
        cols: 30,
        rows: 5,
        reflow: true,
        prompt_redraw: osc::semantic_prompt::Redraw::True,
        pull_scrollback: true,
    })
    .unwrap();
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "screen"
                && *msg == "cursor on prompt line but promptIterator found no prompt"),
        "expected the no-prompt-found diagnostic, got: {messages:?}"
    );
    drop(cap);

    s.deinit();
}
