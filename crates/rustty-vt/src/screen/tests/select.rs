//! PORTS: ghostty src/terminal/Screen.zig — the `selectAll`/`selectLine`/`selectWord`/`selectOutput`, `selectionString`, `lineIterator` and `select` tracking tests.

use super::*;

// test "Screen: selectAll"
#[test]
fn screen_select_all() {
    let mut s = mkscreen(10, 10, 0);

    {
        s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();
        let sel = s.select_all().unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 2), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    {
        s.test_write_string(b"\nFOO\n BAR\n BAZ\n QWERTY\n 12345678")
            .unwrap();
        let sel = s.select_all().unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(8, 7), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine"
#[test]
fn screen_select_line() {
    let mut s = mkscreen(10, 10, 0);
    s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

    // Outside of active area
    // try testing.expect(s.selectLine(.{ .x = 13, .y = 0 }) == null);
    // try testing.expect(s.selectLine(.{ .x = 0, .y = 5 }) == null);

    // Going forward
    {
        let pin = spin(&s, pactive(0, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going backward
    {
        let pin = spin(&s, pactive(7, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going forward and backward
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Outside active area
    {
        let pin = spin(&s, pactive(9, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine across soft-wrap"
#[test]
fn screen_select_line_across_soft_wrap() {
    let mut s = mkscreen(5, 10, 0);
    s.test_write_string(b" 12 34012   \n 123").unwrap();

    // Going forward
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine across full soft-wrap"
#[test]
fn screen_select_line_across_full_soft_wrap() {
    let mut s = mkscreen(5, 5, 0);
    s.test_write_string(b"1ABCD2EFGH\n3IJKL").unwrap();

    {
        let pin = spin(&s, pactive(2, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(4, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine across soft-wrap ignores blank lines"
#[test]
fn screen_select_line_across_soft_wrap_ignores_blank_lines() {
    let mut s = mkscreen(5, 10, 0);
    s.test_write_string(b" 12 34012             \n 123")
        .unwrap();

    // Going forward
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going backward
    {
        let pin = spin(&s, pactive(1, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going forward and backward
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine disabled whitespace trimming"
#[test]
fn screen_select_line_disabled_whitespace_trimming() {
    let mut s = mkscreen(5, 10, 0);
    s.test_write_string(b" 12 34012   \n 123").unwrap();

    // Going forward
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: None,
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(4, 2), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Non-wrapped
    {
        let pin = spin(&s, pactive(1, 3));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: None,
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pscreen(0, 3), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(4, 3), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine with scrollback"
#[test]
fn screen_select_line_with_scrollback() {
    let mut s = mkscreen(2, 3, 5);
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

    // Selecting first line
    {
        let pin = spin(&s, pactive(0, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(1, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Selecting last line
    {
        let pin = spin(&s, pactive(0, 2));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 2), pfp_active(&s, sel.start()));
        assert_eq!(pactive(1, 2), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic prompt boundary"
#[test]
fn screen_select_line_semantic_prompt_boundary() {
    let mut s = mkscreen(5, 10, 0);
    s.test_write_string(b"ABCDE\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"A    ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"> ").unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"ABCDE\nA    \n> ");
    }

    // Selecting output stops at the prompt even if soft-wrapped
    {
        let pin = spin(&s, pactive(1, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        let expected = b"A";
        assert_eq!(contents.as_slice(), expected);
        sel.deinit(&mut s);
    }
    {
        let pin = spin(&s, pactive(1, 2));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 2), pfp_active(&s, sel.start()));
        assert_eq!(pactive(0, 2), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic prompt to input boundary"
#[test]
fn screen_select_line_semantic_prompt_to_input_boundary() {
    let mut s = mkscreen(10, 5, 0);

    // Write prompt followed by user input on same row: "$>command"
    // Using non-whitespace to avoid whitespace trimming affecting the test
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"$>").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"command").unwrap();

    // Selecting from prompt should only select prompt
    {
        let pin = spin(&s, pactive(0, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(1, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Selecting from input should only select input
    {
        let pin = spin(&s, pactive(5, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(2, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(8, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic input to output boundary"
#[test]
fn screen_select_line_semantic_input_to_output_boundary() {
    let mut s = mkscreen(10, 5, 0);

    // Row 0: user input
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"ls -la\n").unwrap();
    // Row 1: command output
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"file.txt").unwrap();

    // Selecting from input should only select input
    {
        let pin = spin(&s, pactive(2, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"ls -la");
        sel.deinit(&mut s);
    }

    // Selecting from output should only select output
    {
        let pin = spin(&s, pactive(2, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"file.txt");
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic mid-row boundary"
#[test]
fn screen_select_line_semantic_mid_row_boundary() {
    let mut s = mkscreen(10, 5, 0);

    // Single row with output then prompt then input: "out$>cmd"
    // Using non-whitespace to avoid whitespace trimming affecting the test
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"out").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"$>").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"cmd").unwrap();

    // Selecting from output should stop at prompt
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(2, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Selecting from prompt should only select prompt
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(3, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(4, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Selecting from input should only select input
    {
        let pin = spin(&s, pactive(6, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(5, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(7, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic boundary soft-wrap with mid-row transition"
#[test]
fn screen_select_line_semantic_boundary_soft_wrap_with_mid_row_transition() {
    let mut s = mkscreen(5, 5, 0);

    // Row 0: prompt "$ " + input "cmd" (soft-wraps)
    // Row 1: input continues "12" + output "out"
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"$ ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"cmd12").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"out").unwrap();

    // Verify layout
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"$ cmd\n12out");
    }

    // Selecting from input on row 0 should get all input across soft-wrap
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"cmd12");
        sel.deinit(&mut s);
    }

    // Selecting from input on row 1 should get all input across soft-wrap
    {
        let pin = spin(&s, pactive(0, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"cmd12");
        sel.deinit(&mut s);
    }

    // Selecting from output should only get output
    {
        let pin = spin(&s, pactive(3, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"out");
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic boundary disabled"
#[test]
fn screen_select_line_semantic_boundary_disabled() {
    let mut s = mkscreen(10, 5, 0);

    // Write prompt followed by input
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"$ ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"command").unwrap();

    // With semantic_prompt_boundary = false, should select entire line
    {
        let pin = spin(&s, pactive(0, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: false,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"$ command");
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic boundary first cell of row"
#[test]
fn screen_select_line_semantic_boundary_first_cell_of_row() {
    let mut s = mkscreen(5, 5, 0);

    // Row 0: input that soft-wraps
    // Row 1: output starts at first cell
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"12345").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"ABCDE").unwrap();

    // Verify soft-wrap happened
    {
        let pin = spin(&s, pactive(0, 0));
        let row = pin.row_and_cell().row;
        assert!(unsafe { (*row).wrap() });
    }

    // Selecting from input should stop before output on row 1
    {
        let pin = spin(&s, pactive(2, 0));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
        assert_eq!(pactive(4, 0), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Selecting from output should only get output
    {
        let pin = spin(&s, pactive(2, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert_eq!(pactive(0, 1), pfp_active(&s, sel.start()));
        assert_eq!(pactive(4, 1), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectLine semantic boundary across mixed-width pages"
#[test]
fn screen_select_line_semantic_boundary_across_mixed_width_pages() {
    let mut s = mkscreen(4, 2, 0);

    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"ABCD").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"E").unwrap();

    let first = s.pages.pages.first;
    s.pages
        .split(Pin {
            node: first,
            x: 0,
            y: 1,
            garbage: false,
        })
        .unwrap();
    let second = unsafe { (*first).next };
    // PORT NOTE: `first.page().size.cols = 2` — a deliberate mid-reflow
    // narrow page, written directly through the node pointer.
    unsafe { (*crate::pagelist::Node::page(first)).size.cols = 2 };
    s.pages.pause_integrity_checks(true);

    assert_eq!(2 as size::CellCountInt, unsafe {
        crate::pagelist::Node::cols(first)
    });
    assert_eq!(4 as size::CellCountInt, unsafe {
        crate::pagelist::Node::cols(second)
    });

    let sel = s
        .select_line(SelectLine {
            pin: Pin {
                node: first,
                x: 1,
                y: 0,
                garbage: false,
            },
            whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
            semantic_prompt_boundary: true,
        })
        .unwrap();
    assert!(Pin {
        node: first,
        x: 0,
        y: 0,
        garbage: false
    }
    .eql(sel.start()));
    assert!(Pin {
        node: first,
        x: 1,
        y: 0,
        garbage: false
    }
    .eql(sel.end()));
    sel.deinit(&mut s);

    s.pages.pause_integrity_checks(false);
    s.deinit();
}

// test "Screen: selectLine semantic all same content"
#[test]
fn screen_select_line_semantic_all_same_content() {
    let mut s = mkscreen(5, 5, 0);

    // All prompt content that soft-wraps
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"prompt text").unwrap();

    // Verify soft-wrap
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"promp\nt tex\nt");
    }

    // Should select all prompt content across soft-wraps
    {
        let pin = spin(&s, pactive(2, 1));
        let sel = s
            .select_line(SelectLine {
                pin,
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"prompt text");
        sel.deinit(&mut s);
    }

    s.deinit();
}

// PORT NOTE: the selectWord tests use the default word boundary codepoints
// inline; mirrored here as a local const matching the reference array.
// Default boundary codepoints for word selection
const SELECT_WORD_BOUNDARY: [u32; 20] = [
    0,
    b' ' as u32,
    b'\t' as u32,
    b'\'' as u32,
    b'"' as u32,
    '│' as u32,
    b'`' as u32,
    b'|' as u32,
    b':' as u32,
    b';' as u32,
    b',' as u32,
    b'(' as u32,
    b')' as u32,
    b'[' as u32,
    b']' as u32,
    b'{' as u32,
    b'}' as u32,
    b'<' as u32,
    b'>' as u32,
    b'$' as u32,
];

// test "Screen: selectWord"
#[test]
fn screen_select_word() {
    let mut s = mkscreen(10, 10, 0);
    s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

    let boundary_codepoints = &SELECT_WORD_BOUNDARY;

    // Outside of active area
    // try testing.expect(s.selectWord(.{ .x = 9, .y = 0 }) == null);
    // try testing.expect(s.selectWord(.{ .x = 0, .y = 5 }) == null);

    // Going forward
    {
        let pin = spin(&s, pactive(0, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going backward
    {
        let pin = spin(&s, pactive(2, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going forward and backward
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Whitespace
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(3, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Whitespace single char
    {
        let pin = spin(&s, pactive(0, 1));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(0, 1), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(0, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // End of screen
    {
        let pin = spin(&s, pactive(1, 2));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(0, 2), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 2), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectWord at hard line breaks"
#[test]
fn screen_select_word_at_hard_line_breaks() {
    struct Case {
        cols: size::CellCountInt,
        text: &'static [u8],
    }
    let cases = [
        Case {
            cols: 5,
            text: b"abcde\nfghij",
        },
        Case {
            cols: 5,
            text: b"     \n     ",
        },
        Case {
            cols: 1,
            text: b"a\nb",
        },
        Case {
            cols: 1,
            text: b" \n ",
        },
    ];
    for case in cases {
        let mut s = mkscreen(case.cols, 2, 0);
        s.test_write_string(case.text).unwrap();

        for y in 0..2u32 {
            for x in 0..case.cols {
                let pin = spin(&s, pactive(x, y));
                let sel = s.select_word(pin, &[0, ' ' as u32]).unwrap();
                assert_eq!(pscreen(0, y), pfp_screen(&s, sel.start()));
                assert_eq!(pscreen(case.cols - 1, y), pfp_screen(&s, sel.end()));
                sel.deinit(&mut s);
            }
        }

        s.deinit();
    }
}

// test "Screen: selectWord across soft-wrap at right edge"
#[test]
fn screen_select_word_across_soft_wrap_at_right_edge() {
    let mut s = mkscreen(5, 3, 0);
    s.test_write_string(b"abcdefghij\nklmno").unwrap();

    for y in 0..2u32 {
        for x in 0..5 {
            let pin = spin(&s, pactive(x, y));
            let sel = s.select_word(pin, &[0, ' ' as u32]).unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }
    }

    s.deinit();
}

// test "Screen: selectWord wide characters"
#[test]
fn screen_select_word_wide_characters() {
    struct Case {
        text: &'static str,
        cols: size::CellCountInt,
        boundary_codepoints: &'static [u32],
        start: usize,
        end: usize,
        expected: &'static str,
    }
    let cases = [
        Case {
            text: "日本語",
            cols: 10,
            boundary_codepoints: &[0, ' ' as u32],
            start: 0,
            end: 5,
            expected: "日本語",
        },
        Case {
            text: "日本語",
            cols: 10,
            boundary_codepoints: &[' ' as u32],
            start: 0,
            end: 5,
            expected: "日本語",
        },
        Case {
            text: "a日b語c",
            cols: 10,
            boundary_codepoints: &[0, ' ' as u32],
            start: 0,
            end: 6,
            expected: "a日b語c",
        },
        Case {
            text: " 日本語 ",
            cols: 10,
            boundary_codepoints: &[0, ' ' as u32],
            start: 1,
            end: 6,
            expected: "日本語",
        },
        Case {
            text: "日本語",
            cols: 4,
            boundary_codepoints: &[0, ' ' as u32],
            start: 0,
            end: 5,
            expected: "日本語",
        },
        Case {
            text: "日本語",
            cols: 5,
            boundary_codepoints: &[0, ' ' as u32],
            start: 0,
            end: 6,
            expected: "日本語",
        },
        Case {
            text: "日本\n語文",
            cols: 4,
            boundary_codepoints: &[0, ' ' as u32],
            start: 0,
            end: 3,
            expected: "日本",
        },
        Case {
            text: "日本\n語文",
            cols: 4,
            boundary_codepoints: &[0, ' ' as u32],
            start: 4,
            end: 7,
            expected: "語文",
        },
        Case {
            text: "a語b",
            cols: 10,
            boundary_codepoints: &[0, '語' as u32],
            start: 0,
            end: 0,
            expected: "a",
        },
        Case {
            text: "a語b",
            cols: 10,
            boundary_codepoints: &[0, '語' as u32],
            start: 1,
            end: 2,
            expected: "語",
        },
        Case {
            text: "a語b",
            cols: 10,
            boundary_codepoints: &[0, '語' as u32],
            start: 3,
            end: 3,
            expected: "b",
        },
        Case {
            text: "abcd語ef",
            cols: 5,
            boundary_codepoints: &[0, '語' as u32],
            start: 0,
            end: 3,
            expected: "abcd",
        },
        Case {
            text: "abcd語ef",
            cols: 5,
            boundary_codepoints: &[0, '語' as u32],
            start: 4,
            end: 6,
            expected: "語",
        },
        Case {
            text: "abcd語ef",
            cols: 5,
            boundary_codepoints: &[0, '語' as u32],
            start: 7,
            end: 8,
            expected: "ef",
        },
    ];

    for case in cases {
        let mut s = mkscreen(case.cols, 4, 0);
        s.test_write_string(case.text.as_bytes()).unwrap();

        let cols = case.cols as usize;
        // Selecting any cell in the word should select the whole word.
        for offset in case.start..=case.end {
            let x = (offset % cols) as size::CellCountInt;
            let y = (offset / cols) as u32;
            let pin = spin(&s, pactive(x, y));
            let sel = s.select_word(pin, case.boundary_codepoints).unwrap();
            assert_eq!(
                pscreen(
                    (case.start % cols) as size::CellCountInt,
                    (case.start / cols) as u32
                ),
                pfp_screen(&s, sel.start())
            );
            assert_eq!(
                pscreen(
                    (case.end % cols) as size::CellCountInt,
                    (case.end / cols) as u32
                ),
                pfp_screen(&s, sel.end())
            );

            let contents = selstr(&mut s, sel, false);
            assert_eq!(String::from_utf8(contents).unwrap(), case.expected);
            sel.deinit(&mut s);
        }

        s.deinit();
    }
}

// test "Screen: selectWord across soft-wrap"
#[test]
fn screen_select_word_across_soft_wrap() {
    let mut s = mkscreen(5, 10, 0);
    s.test_write_string(b" 1234012\n 123").unwrap();

    let boundary_codepoints = &SELECT_WORD_BOUNDARY;

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b" 1234\n012\n 123");
    }

    // Going forward
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going backward
    {
        let pin = spin(&s, pactive(1, 1));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going forward and backward
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectWord whitespace across soft-wrap"
#[test]
fn screen_select_word_whitespace_across_soft_wrap() {
    let mut s = mkscreen(5, 10, 0);
    s.test_write_string(b"1       1\n 123").unwrap();

    let boundary_codepoints = &SELECT_WORD_BOUNDARY;

    // Going forward
    {
        let pin = spin(&s, pactive(1, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going backward
    {
        let pin = spin(&s, pactive(1, 1));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    // Going forward and backward
    {
        let pin = spin(&s, pactive(3, 0));
        let sel = s.select_word(pin, boundary_codepoints).unwrap();
        assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
        assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
        sel.deinit(&mut s);
    }

    s.deinit();
}

// test "Screen: selectWord with character boundary"
#[test]
fn screen_select_word_with_character_boundary() {
    let boundary_codepoints = &SELECT_WORD_BOUNDARY;

    let cases: [&[u8]; 17] = [
        " 'abc' \n123".as_bytes(),
        " \"abc\" \n123".as_bytes(),
        " │abc│ \n123".as_bytes(),
        " `abc` \n123".as_bytes(),
        " |abc| \n123".as_bytes(),
        " :abc: \n123".as_bytes(),
        " ;abc; \n123".as_bytes(),
        " ,abc, \n123".as_bytes(),
        " (abc( \n123".as_bytes(),
        " )abc) \n123".as_bytes(),
        " [abc[ \n123".as_bytes(),
        " ]abc] \n123".as_bytes(),
        " {abc{ \n123".as_bytes(),
        " }abc} \n123".as_bytes(),
        " <abc< \n123".as_bytes(),
        " >abc> \n123".as_bytes(),
        " $abc$ \n123".as_bytes(),
    ];

    for case in cases {
        let mut s = mkscreen(20, 10, 0);
        s.test_write_string(case).unwrap();

        // Inside character forward
        {
            let pin = spin(&s, pactive(2, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Inside character backward
        {
            let pin = spin(&s, pactive(4, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Inside character bidirectional
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // On quote
        // NOTE: this behavior is not ideal, so we can change this one day,
        // but I think its also not that important compared to the above.
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }
}

// test "Screen: selectOutput"
#[test]
fn screen_select_output() {
    let mut s = mkscreen(10, 15, 0);

    // Build content with cell-level semantic content:
    // Row 0-1: output1 (output)
    // Row 2: prompt2 (prompt)
    // Row 3: input2 (input)
    // Row 4-7: output2 (output, with overflow causing wrap)
    // Row 8: "$ " (prompt) + "input3" (input)
    // Row 9-11: output3 (output)
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"output1\n").unwrap();
    s.test_write_string(b"output1\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"prompt2\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"input2\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"output2output2output2output2\n")
        .unwrap();
    s.test_write_string(b"output2\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Prompt(
        osc::semantic_prompt::PromptKind::Initial,
    ));
    s.test_write_string(b"$ ").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Input(
        SemanticContentInput::ClearExplicit,
    ));
    s.test_write_string(b"input3\n").unwrap();
    s.cursor_set_semantic_content(SemanticContentArg::Output);
    s.test_write_string(b"output3\n").unwrap();
    s.test_write_string(b"output3\n").unwrap();
    s.test_write_string(b"output3").unwrap();

    // First output block (rows 0-1), should select those rows
    {
        let pin = spin(&s, pactive(1, 1));
        let sel = s.select_output(pin).unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(contents.as_slice(), b"output1\noutput1");
        sel.deinit(&mut s);
    }
    // Second output block (rows 4-7)
    {
        let pin = spin(&s, pactive(3, 7));
        let sel = s.select_output(pin).unwrap();
        let contents = selstr(&mut s, sel, false);
        assert_eq!(
            contents.as_slice(),
            b"output2output2output2output2\noutput2"
        );
        sel.deinit(&mut s);
    }
    // Third output block (rows 9-11)
    {
        let pin = spin(&s, pactive(2, 10));
        let sel = s.select_output(pin).unwrap();
        assert_eq!(pactive(0, 9), pfp_active(&s, sel.start()));
        assert_eq!(pactive(6, 11), pfp_active(&s, sel.end()));
        sel.deinit(&mut s);
    }
    // Click on prompt should return null
    {
        let pin = spin(&s, pactive(1, 8));
        assert!(s.select_output(pin).is_none());
    }
    // Click on input should return null
    {
        let pin = spin(&s, pactive(5, 8));
        assert!(s.select_output(pin).is_none());
    }

    s.deinit();
}

// test "Screen: selectionString basic"
#[test]
fn screen_selection_string_basic() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(0, 1)), spin(&s, pscreen(2, 2)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = b"2EFGH\n3IJ";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString start outside of written area"
#[test]
fn screen_selection_string_start_outside_of_written_area() {
    let mut s = mkscreen(5, 10, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(0, 5)), spin(&s, pscreen(2, 6)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = b"";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString end outside of written area"
#[test]
fn screen_selection_string_end_outside_of_written_area() {
    let mut s = mkscreen(5, 10, 0);
    let str = b"1ABCD\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(0, 2)), spin(&s, pscreen(2, 6)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = b"3IJKL";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString trim space"
#[test]
fn screen_selection_string_trim_space() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1AB  \n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(2, 1)), false);

    {
        let contents = selstr(&mut s, sel, true);
        let expected = b"1AB\n2EF";
        assert_eq!(contents.as_slice(), expected);
    }

    // No trim
    {
        let contents = selstr(&mut s, sel, false);
        let expected = b"1AB  \n2EF";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString trim empty line"
#[test]
fn screen_selection_string_trim_empty_line() {
    let mut s = mkscreen(5, 5, 0);
    let str = b"1AB  \n\n2EFGH\n3IJKL";
    s.test_write_string(str).unwrap();

    let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(2, 2)), false);

    {
        let contents = selstr(&mut s, sel, true);
        let expected = b"1AB\n\n2EF";
        assert_eq!(contents.as_slice(), expected);
    }

    // No trim
    {
        let contents = selstr(&mut s, sel, false);
        let expected = b"1AB  \n\n2EF";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString soft wrap"
#[test]
fn screen_selection_string_soft_wrap() {
    let mut s = mkscreen(5, 3, 0);
    let str = b"1ABCD2EFGH3IJKL";
    s.test_write_string(str).unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(0, 1)), spin(&s, pscreen(2, 2)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = b"2EFGH3IJ";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString wide char"
#[test]
fn screen_selection_string_wide_char() {
    let mut s = mkscreen(5, 3, 0);
    let str = "1A⚡".as_bytes();
    s.test_write_string(str).unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(3, 0)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = str;
        assert_eq!(contents.as_slice(), expected);
    }

    {
        let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(2, 0)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = str;
        assert_eq!(contents.as_slice(), expected);
    }

    {
        let sel = Selection::init(spin(&s, pscreen(3, 0)), spin(&s, pscreen(3, 0)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = "⚡".as_bytes();
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString wide char with header"
#[test]
fn screen_selection_string_wide_char_with_header() {
    let mut s = mkscreen(5, 3, 0);
    let str = "1ABC⚡".as_bytes();
    s.test_write_string(str).unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(4, 0)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = str;
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// https://github.com/mitchellh/ghostty/issues/289
// test "Screen: selectionString empty with soft wrap"
#[test]
fn screen_selection_string_empty_with_soft_wrap() {
    let mut s = mkscreen(5, 2, 0);

    // Let me describe the situation that caused this because this
    // test is not obvious. By writing an emoji below, we introduce
    // one cell with the emoji and one cell as a "wide char spacer".
    // We then soft wrap the line by writing spaces.
    //
    // By selecting only the tail, we'd select nothing and we had
    // a logic error that would cause a crash.
    s.test_write_string("👨".as_bytes()).unwrap();
    s.test_write_string(b"      ").unwrap();

    {
        let sel = Selection::init(spin(&s, pscreen(1, 0)), spin(&s, pscreen(2, 0)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = "👨".as_bytes();
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString with zero width joiner"
#[test]
fn screen_selection_string_with_zero_width_joiner() {
    let mut s = mkscreen(10, 1, 0);
    let str = "👨\u{200d}".as_bytes(); // this has a ZWJ
    s.test_write_string(str).unwrap();

    // Integrity check
    {
        let pin = spin(&s, pscreen(0, 0));
        let cell = pin.row_and_cell().cell;
        assert_eq!(0x1F468u32, unsafe { (*cell).codepoint() });
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*cell).wide() });
        let cps = unsafe {
            (*crate::pagelist::Node::page(pin.node))
                .lookup_grapheme(cell)
                .unwrap()
        };
        assert_eq!(1usize, cps.len());
    }

    // The real test
    {
        let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(1, 0)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = "👨\u{200d}".as_bytes();
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: selectionString, rectangle, basic"
#[test]
fn screen_selection_string_rectangle_basic() {
    let mut s = mkscreen(30, 5, 0);
    let str = "Lorem ipsum dolor\nsit amet, consectetur\nadipiscing elit, sed do\neiusmod tempor incididunt\nut labore et dolore".as_bytes();
    let sel = Selection::init(spin(&s, pscreen(2, 1)), spin(&s, pscreen(6, 3)), true);
    let expected = "t ame\nipisc\nusmod".as_bytes();
    s.test_write_string(str).unwrap();

    let contents = selstr(&mut s, sel, true);
    assert_eq!(contents.as_slice(), expected);

    s.deinit();
}

// test "Screen: selectionString, rectangle, w/EOL"
#[test]
fn screen_selection_string_rectangle_w_eol() {
    let mut s = mkscreen(30, 5, 0);
    let str = "Lorem ipsum dolor\nsit amet, consectetur\nadipiscing elit, sed do\neiusmod tempor incididunt\nut labore et dolore".as_bytes();
    let sel = Selection::init(spin(&s, pscreen(12, 0)), spin(&s, pscreen(26, 4)), true);
    let expected = "dolor\nnsectetur\nlit, sed do\nor incididunt\n dolore".as_bytes();
    s.test_write_string(str).unwrap();

    let contents = selstr(&mut s, sel, true);
    assert_eq!(contents.as_slice(), expected);

    s.deinit();
}

// test "Screen: selectionString, rectangle, more complex w/breaks"
#[test]
fn screen_selection_string_rectangle_more_complex_w_breaks() {
    let mut s = mkscreen(30, 8, 0);
    let str = "Lorem ipsum dolor\nsit amet, consectetur\nadipiscing elit, sed do\neiusmod tempor incididunt\nut labore et dolore\n\nmagna aliqua. Ut enim\nad minim veniam, quis".as_bytes();
    let sel = Selection::init(spin(&s, pscreen(11, 2)), spin(&s, pscreen(26, 7)), true);
    let expected = "elit, sed do\npor incididunt\nt dolore\n\na. Ut enim\nniam, quis".as_bytes();
    s.test_write_string(str).unwrap();

    let contents = selstr(&mut s, sel, true);
    assert_eq!(contents.as_slice(), expected);

    s.deinit();
}

// test "Screen: selectionString multi-page"
#[test]
fn screen_selection_string_multi_page() {
    let mut s = mkscreen(10, 3, 2048);

    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };

    // Lazy way to seek to the first page boundary.
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 1) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };

    s.test_write_string(b"123456789\n!@#$%^&*(\n123456789")
        .unwrap();

    {
        let sel = Selection::init(spin(&s, pactive(0, 0)), spin(&s, pactive(2, 2)), false);
        let contents = selstr(&mut s, sel, true);
        let expected = b"123456789\n!@#$%^&*(\n123";
        assert_eq!(contents.as_slice(), expected);
    }

    s.deinit();
}

// test "Screen: lineIterator"
#[test]
fn screen_line_iterator() {
    let mut s = mkscreen(5, 5, 0);
    let str = b"1ABCD\n2EFGH";
    s.test_write_string(str).unwrap();

    // Test the line iterator
    // PORT NOTE: the reference holds the iterator's `*const Screen` and
    // interleaves `iter.next()` with `selectionString` (a `*mut Screen`
    // method). Rust's `LineIterator<'a>` borrows `&Screen`, so we collect
    // both selections first (dropping the iterator borrow) before calling
    // the `&mut self` `selection_string`. The order of operations and
    // assertions is identical.
    let start = spin(&s, vp());
    let (sel0, sel1) = {
        let mut iter = s.line_iterator(start);
        let a = iter.next().unwrap();
        let b = iter.next().unwrap();
        (a, b)
    };
    {
        let actual = selstr(&mut s, sel0, false);
        assert_eq!(actual.as_slice(), b"1ABCD");
    }
    {
        let actual = selstr(&mut s, sel1, false);
        assert_eq!(actual.as_slice(), b"2EFGH");
    }

    s.deinit();
}

// test "Screen: lineIterator soft wrap"
#[test]
fn screen_line_iterator_soft_wrap() {
    let mut s = mkscreen(5, 5, 0);
    let str = b"1ABCD2EFGH\n3ABCD";
    s.test_write_string(str).unwrap();

    // Test the line iterator
    // PORT NOTE: see `screen_line_iterator` — selections are collected
    // before the `&mut self` `selection_string` calls due to Rust's
    // borrow of `&Screen` by `LineIterator`.
    let start = spin(&s, vp());
    let (sel0, sel1) = {
        let mut iter = s.line_iterator(start);
        let a = iter.next().unwrap();
        let b = iter.next().unwrap();
        (a, b)
    };
    {
        let actual = selstr(&mut s, sel0, false);
        assert_eq!(actual.as_slice(), b"1ABCD2EFGH");
    }
    {
        let actual = selstr(&mut s, sel1, false);
        assert_eq!(actual.as_slice(), b"3ABCD");
    }
    // try testing.expect(iter.next() == null);

    s.deinit();
}

// test "Screen: select untracked"
#[test]
fn screen_select_untracked() {
    let mut s = mkscreen(10, 10, 0);
    s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

    assert!(s.selection.is_none());
    let tracked = s.pages.count_tracked_pins();
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
            .unwrap(),
        false,
    )))
    .unwrap();
    assert_eq!(tracked + 2, s.pages.count_tracked_pins());
    s.select(None).unwrap();
    assert_eq!(tracked, s.pages.count_tracked_pins());
    s.deinit();
}

// test "Screen: select replaces existing pins"
#[test]
fn screen_select_replaces_existing_pins() {
    let mut s = mkscreen(10, 10, 0);
    s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

    let tracked = s.pages.count_tracked_pins();
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
            .unwrap(),
        false,
    )))
    .unwrap();
    assert_eq!(tracked + 2, s.pages.count_tracked_pins());

    // Replacing the selection must untrack the prior selection's pins
    // rather than leak them.
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
            .unwrap(),
        false,
    )))
    .unwrap();
    assert_eq!(tracked + 2, s.pages.count_tracked_pins());
    s.deinit();
}

// test "Screen: reselecting tracked selection preserves its pins"
#[test]
fn screen_reselecting_tracked_selection_preserves_its_pins() {
    let mut s = mkscreen(10, 2, 0);

    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // `Selection` is `Copy`, so this hands our own tracked selection back
    // to `select` by value — exactly the aliasing upstream fixes.
    let current = s.selection.unwrap();
    s.select(Some(current)).unwrap();

    assert_eq!(selection::Order::Forward, s.selection.unwrap().order(&s));

    s.deinit();
}
