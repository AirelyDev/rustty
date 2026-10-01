//! PORTS: ghostty src/terminal/Terminal.zig — the semantic prompt (OSC 133) and `cursorIsAtPrompt` tests, plus the pwd/title round trips.

use super::*;

// test "Terminal: semantic prompt"
#[test]
fn terminal_semantic_prompt() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Prompt
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::FreshLineNewPrompt,
    ))
    .unwrap();
    for c in "hello".bytes() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.y } as usize);
    assert_eq!(5usize, unsafe { (*t.screens.active).cursor.x } as usize);
    {
        let cx = unsafe { (*t.screens.active).cursor.x };
        let cy = unsafe { (*t.screens.active).cursor.y } as u32;
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(screen_pt(cx - 1, cy))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
            (*cell).semantic_content()
        });

        let row = list_cell.row;
        assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
            (*row).semantic_prompt()
        });
    }

    // Start input but end it on EOL
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndPromptStartInputTerminateEol,
    ))
    .unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    // Write some output
    assert_eq!(1usize, unsafe { (*t.screens.active).cursor.y } as usize);
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
    for c in "world".bytes() {
        t.print(c as u32).unwrap();
    }
    {
        let cx = unsafe { (*t.screens.active).cursor.x };
        let cy = unsafe { (*t.screens.active).cursor.y } as u32;
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(screen_pt(cx - 1, cy))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(pagepkg::SemanticContent::Output, unsafe {
            (*cell).semantic_content()
        });

        let row = list_cell.row;
        assert_eq!(pagepkg::SemanticPrompt::None, unsafe {
            (*row).semantic_prompt()
        });
    }

    t.deinit(alloc);
}

// test "Terminal: semantic prompt continuations"
#[test]
fn terminal_semantic_prompt_continuations() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Prompt
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::FreshLineNewPrompt,
    ))
    .unwrap();
    for c in "hello".bytes() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.y } as usize);
    assert_eq!(5usize, unsafe { (*t.screens.active).cursor.x } as usize);
    {
        let cx = unsafe { (*t.screens.active).cursor.x };
        let cy = unsafe { (*t.screens.active).cursor.y } as u32;
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(screen_pt(cx - 1, cy))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
            (*cell).semantic_content()
        });

        let row = list_cell.row;
        assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
            (*row).semantic_prompt()
        });
    }

    // Start input but end it on EOL
    t.carriage_return();
    t.linefeed().unwrap();
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::PromptStart,
        options_unvalidated: b"k=c",
    })
    .unwrap();

    // Write some output
    assert_eq!(1usize, unsafe { (*t.screens.active).cursor.y } as usize);
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
    for c in "world".bytes() {
        t.print(c as u32).unwrap();
    }
    {
        let cx = unsafe { (*t.screens.active).cursor.x };
        let cy = unsafe { (*t.screens.active).cursor.y } as u32;
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(screen_pt(cx - 1, cy))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
            (*cell).semantic_content()
        });

        let row = list_cell.row;
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*row).semantic_prompt()
        });
    }

    t.deinit(alloc);
}

// test "Terminal: index in prompt mode marks new row as prompt continuation"
#[test]
fn terminal_index_in_prompt_mode_marks_new_row_as_prompt_continuation() {
    // This tests the Fish shell workaround: when in prompt mode and we get
    // a newline, assume the new row is a prompt continuation (since Fish
    // doesn't emit OSC133 k=s markers for continuation lines).
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Start a prompt
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    for c in "hello".bytes() {
        t.print(c as u32).unwrap();
    }

    // Verify first row is marked as prompt
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    // Now do a linefeed while still in prompt mode
    t.carriage_return();
    t.linefeed().unwrap();

    // The new row should automatically be marked as prompt continuation
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    // The cursor semantic content should still be prompt
    assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
        (*t.screens.active).cursor.semantic_content
    });

    t.deinit(alloc);
}

// test "Terminal: index in input mode does not mark new row as prompt"
#[test]
fn terminal_index_in_input_mode_does_not_mark_new_row_as_prompt() {
    // Input mode should NOT trigger prompt continuation on newline
    // (only prompt mode does, not input mode)
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Start a prompt then switch to input
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    for c in "$ ".bytes() {
        t.print(c as u32).unwrap();
    }
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndPromptStartInput,
    ))
    .unwrap();
    for c in "echo \\".bytes() {
        t.print(c as u32).unwrap();
    }

    // Linefeed while in input mode
    t.carriage_return();
    t.linefeed().unwrap();

    // The new row should be marked as prompt continuation
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    // Our cursor should still be in input
    assert_eq!(pagepkg::SemanticContent::Input, unsafe {
        (*t.screens.active).cursor.semantic_content
    });

    t.deinit(alloc);
}

// test "Terminal: index in output mode does not mark new row as prompt"
#[test]
fn terminal_index_in_output_mode_does_not_mark_new_row_as_prompt() {
    // Output mode should NOT trigger prompt continuation
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Complete prompt cycle: prompt -> input -> output
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    for c in "$ ".bytes() {
        t.print(c as u32).unwrap();
    }
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndPromptStartInput,
    ))
    .unwrap();
    for c in "ls".bytes() {
        t.print(c as u32).unwrap();
    }
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndInputStartOutput,
    ))
    .unwrap();

    // Linefeed while in output mode
    t.carriage_return();
    t.linefeed().unwrap();

    // The new row should NOT be marked as a prompt
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::None, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    t.deinit(alloc);
}

// test "Terminal: OSC133C at x=0 on prompt row clears prompt mark"
#[test]
fn terminal_osc133c_at_x0_on_prompt_row_clears_prompt_mark() {
    // This tests the second Fish heuristic: when Fish emits a newline
    // then immediately sends OSC133C (start output) at column 0, we
    // should clear the prompt continuation mark we just set.
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Start a prompt
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    for c in "$ echo \\".bytes() {
        t.print(c as u32).unwrap();
    }

    // Simulate Fish behavior: newline first (which marks next row as prompt)
    t.carriage_return();
    t.linefeed().unwrap();

    // Verify the new row is marked as prompt continuation
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    // Now Fish sends OSC133C at column 0 (cursor is still at x=0)
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndInputStartOutput,
    ))
    .unwrap();

    // The prompt continuation should be cleared
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::None, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    t.deinit(alloc);
}

// test "Terminal: OSC133C at x>0 on prompt row does not clear prompt mark"
#[test]
fn terminal_osc133c_at_xgt0_on_prompt_row_does_not_clear_prompt_mark() {
    // If we're not at column 0, we shouldn't clear the prompt mark
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Start a prompt on a row
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    for c in "$ ".bytes() {
        t.print(c as u32).unwrap();
    }

    // Move to a new line and mark it as prompt continuation manually
    t.carriage_return();
    t.linefeed().unwrap();
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::PromptStart,
        options_unvalidated: b"k=c",
    })
    .unwrap();
    for c in "> ".bytes() {
        t.print(c as u32).unwrap();
    }

    // Verify the row is marked as prompt continuation
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    // Now send OSC133C but cursor is NOT at column 0
    assert!(unsafe { (*t.screens.active).cursor.x } > 0);
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndInputStartOutput,
    ))
    .unwrap();

    // The prompt continuation should NOT be cleared (we're not at x=0)
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    t.deinit(alloc);
}

// test "Terminal: multiple newlines in prompt mode marks all rows"
#[test]
fn terminal_multiple_newlines_in_prompt_mode_marks_all_rows() {
    // Multiple newlines should each mark their row as prompt continuation
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Start a prompt
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    for c in "line1".bytes() {
        t.print(c as u32).unwrap();
    }

    // Multiple newlines
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "line2".bytes() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "line3".bytes() {
        t.print(c as u32).unwrap();
    }

    // First row should be prompt
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    // Second and third rows should be prompt continuation
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 2)).unwrap();
        assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
            (*list_cell.row).semantic_prompt()
        });
    }

    t.deinit(alloc);
}

// test "Terminal: OSC133A click_events=1 sets click to click_events"
#[test]
fn terminal_osc133a_click_events_1_sets_click_to_click_events() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Verify default state is none
    assert_eq!(crate::screen::SemanticClick::None, unsafe {
        (*t.screens.active).semantic_prompt.click
    });

    // OSC 133;A with click_events=1
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"click_events=1",
    })
    .unwrap();

    assert_eq!(
        crate::screen::SemanticClick::ClickEvents(osc::semantic_prompt::ClickEvents::Absolute),
        unsafe { (*t.screens.active).semantic_prompt.click }
    );

    t.deinit(alloc);
}

// test "Terminal: OSC133A click_events=2 sets click to click_events (relative)"
#[test]
fn terminal_osc133a_click_events_2_sets_click_to_click_events_relative() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Verify default state is none
    assert_eq!(crate::screen::SemanticClick::None, unsafe {
        (*t.screens.active).semantic_prompt.click
    });

    // OSC 133;A with click_events=2
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"click_events=2",
    })
    .unwrap();

    assert_eq!(
        crate::screen::SemanticClick::ClickEvents(osc::semantic_prompt::ClickEvents::Relative),
        unsafe { (*t.screens.active).semantic_prompt.click }
    );

    t.deinit(alloc);
}

// test "Terminal: OSC133A click_events=0 does not set click_events"
#[test]
fn terminal_osc133a_click_events_0_does_not_set_click_events() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // OSC 133;A with click_events=0
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"click_events=0",
    })
    .unwrap();

    // Should remain none since click_events=0 doesn't activate anything
    assert_eq!(crate::screen::SemanticClick::None, unsafe {
        (*t.screens.active).semantic_prompt.click
    });

    t.deinit(alloc);
}

// test "Terminal: OSC133A cl option sets click to cl value"
#[test]
fn terminal_osc133a_cl_option_sets_click_to_cl_value() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // OSC 133;A with cl=m (multiple)
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"cl=m",
    })
    .unwrap();

    assert_eq!(
        crate::screen::SemanticClick::Cl(osc::semantic_prompt::Click::Multiple),
        unsafe { (*t.screens.active).semantic_prompt.click }
    );

    t.deinit(alloc);
}

// test "Terminal: OSC133A cl=line sets click to line"
#[test]
fn terminal_osc133a_cl_line_sets_click_to_line() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"cl=line",
    })
    .unwrap();

    assert_eq!(
        crate::screen::SemanticClick::Cl(osc::semantic_prompt::Click::Line),
        unsafe { (*t.screens.active).semantic_prompt.click }
    );

    t.deinit(alloc);
}

// test "Terminal: OSC133A click_events=1 takes priority over cl"
#[test]
fn terminal_osc133a_click_events_1_takes_priority_over_cl() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // OSC 133;A with both click_events=1 and cl=m
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"click_events=1;cl=m",
    })
    .unwrap();

    // click_events should take priority
    assert_eq!(
        crate::screen::SemanticClick::ClickEvents(osc::semantic_prompt::ClickEvents::Absolute),
        unsafe { (*t.screens.active).semantic_prompt.click }
    );

    t.deinit(alloc);
}

// test "Terminal: OSC133A click_events=0 falls back to cl"
#[test]
fn terminal_osc133a_click_events_0_falls_back_to_cl() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // OSC 133;A with click_events=0 and cl=v
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"click_events=0;cl=v",
    })
    .unwrap();

    // Should fall back to cl since click_events is disabled
    assert_eq!(
        crate::screen::SemanticClick::Cl(osc::semantic_prompt::Click::ConservativeVertical),
        unsafe { (*t.screens.active).semantic_prompt.click }
    );

    t.deinit(alloc);
}

// test "Terminal: OSC133A no click options leaves click as none"
#[test]
fn terminal_osc133a_no_click_options_leaves_click_as_none() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // OSC 133;A with no click-related options
    t.semantic_prompt(osc::SemanticPrompt {
        action: osc::semantic_prompt::Action::FreshLineNewPrompt,
        options_unvalidated: b"aid=123",
    })
    .unwrap();

    assert_eq!(crate::screen::SemanticClick::None, unsafe {
        (*t.screens.active).semantic_prompt.click
    });

    t.deinit(alloc);
}

// test "Terminal: cursorIsAtPrompt"
#[test]
fn terminal_cursor_is_at_prompt() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

    assert!(!t.cursor_is_at_prompt());
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    assert!(t.cursor_is_at_prompt());
    for c in "$ ".bytes() {
        t.print(c as u32).unwrap();
    }

    // Input is also a prompt
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndPromptStartInput,
    ))
    .unwrap();
    assert!(t.cursor_is_at_prompt());
    for c in "ls".bytes() {
        t.print(c as u32).unwrap();
    }

    // But once we say we're starting output, we're not a prompt
    // (cursor is not at x=0, so the Fish heuristic doesn't trigger)
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::EndInputStartOutput,
    ))
    .unwrap();
    // Still a prompt because this line has a prompt
    assert!(t.cursor_is_at_prompt());
    t.linefeed().unwrap();
    assert!(!t.cursor_is_at_prompt());

    // Until we know we're at a prompt again
    t.linefeed().unwrap();
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    assert!(t.cursor_is_at_prompt());

    t.deinit(alloc);
}

// test "Terminal: cursorIsAtPrompt alternate screen"
#[test]
fn terminal_cursor_is_at_prompt_alternate_screen() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(3, 2)).unwrap();

    assert!(!t.cursor_is_at_prompt());
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    assert!(t.cursor_is_at_prompt());

    // Secondary screen is never a prompt
    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    assert!(!t.cursor_is_at_prompt());
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    assert!(!t.cursor_is_at_prompt());

    t.deinit(alloc);
}

// PORT NOTE: pwd/title round-trip tests. The reference `Terminal.zig` has no
// dedicated tests for setPwd/getPwd/setTitle/getTitle, so these exercise the
// ported `Vec<u8>` + trailing-0 sentinel contract (getters return the
// NUL-stripped byte slice or None) with independent expected values.
#[test]
fn terminal_set_get_pwd() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Empty by default.
    assert!(t.get_pwd().is_none());

    // Set a pwd and read it back NUL-stripped.
    t.set_pwd(b"/home/user").unwrap();
    assert_eq!(Some(b"/home/user".as_slice()), t.get_pwd());
    // The backing buffer carries a trailing NUL sentinel.
    assert_eq!(b"/home/user\x00".as_slice(), t.pwd.as_slice());

    // Setting again clears the prior value first.
    t.set_pwd(b"/tmp").unwrap();
    assert_eq!(Some(b"/tmp".as_slice()), t.get_pwd());

    // Setting empty clears it back to None (no sentinel appended).
    t.set_pwd(b"").unwrap();
    assert!(t.get_pwd().is_none());
    assert!(t.pwd.is_empty());

    t.deinit(alloc);
}

#[test]
fn terminal_set_get_title() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    // Empty by default.
    assert!(t.get_title().is_none());

    // Set a title and read it back NUL-stripped.
    t.set_title(b"hello world").unwrap();
    assert_eq!(Some(b"hello world".as_slice()), t.get_title());
    // The backing buffer carries a trailing NUL sentinel.
    assert_eq!(b"hello world\x00".as_slice(), t.title.as_slice());

    // Setting again clears the prior value first.
    t.set_title(b"x").unwrap();
    assert_eq!(Some(b"x".as_slice()), t.get_title());

    // Setting empty clears it back to None (no sentinel appended).
    t.set_title(b"").unwrap();
    assert!(t.get_title().is_none());
    assert!(t.title.is_empty());

    t.deinit(alloc);
}
