//! PORTS: ghostty src/input/Binding.zig — the `Action` parse, clone and format tests.

use super::*;

// rustty-only (R13, UPSTREAM-DEFECTS.md UD58): a `goto_tab` index that
// `apprt.action.GotoTab = enum(c_int)` cannot hold is rejected by both action
// parsers, with the error each returns for an out-of-range integer. The
// largest index that fits is accepted. 4294967293..=4294967295 are the values
// the reference's ReleaseFast truncates onto the last/next/previous sentinels.
#[test]
fn goto_tab_index_above_c_int_max_is_rejected_by_both_parsers() {
    assert_eq!(GOTO_TAB_MAX, i32::MAX as usize);
    let too_large = [
        "2147483648",
        "3000000000",
        "4294967293",
        "4294967294",
        "4294967295",
        "18446744073709551615",
    ];

    // `Action.parse` (keybind values).
    assert_eq!(
        Ok(Action::GotoTab(GOTO_TAB_MAX)),
        Action::parse(b"goto_tab:2147483647")
    );
    for v in too_large {
        assert_eq!(
            Err(Error::InvalidFormat),
            Action::parse(format!("goto_tab:{v}").as_bytes()),
            "keybind goto_tab:{v}"
        );
    }

    // The tagged-union parser (`command-palette-entry` actions).
    assert_eq!(
        Ok(Action::GotoTab(GOTO_TAB_MAX)),
        Action::parse_auto_tagged_union(b"goto_tab:2147483647")
    );
    for v in too_large {
        assert_eq!(
            Err(rustty_args::args::Error::InvalidValue),
            Action::parse_auto_tagged_union(format!("goto_tab:{v}").as_bytes()),
            "tagged union goto_tab:{v}"
        );
    }
}

// test "parse: action invalid"
#[test]
fn parse_action_invalid() {
    // invalid action
    assert_eq!(
        Err(ParseSingleError::InvalidAction),
        parse_single(b"a=nopenopenope")
    );
}

// test "parse: action no parameters"
#[test]
fn parse_action_no_parameters() {
    // no parameters
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"a=ignore").unwrap()
    );
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=ignore:A")
    );
}

// test "parse: action with string"
#[test]
fn parse_action_with_string() {
    // parameter
    {
        let binding = parse_single(b"a=csi:A").unwrap();
        assert!(matches!(binding.action, Action::Csi(_)));
        if let Action::Csi(v) = &binding.action {
            assert_eq!(b"A", v.as_slice());
        }
    }
    // parameter
    {
        let binding = parse_single(b"a=esc:A").unwrap();
        assert!(matches!(binding.action, Action::Esc(_)));
        if let Action::Esc(v) = &binding.action {
            assert_eq!(b"A", v.as_slice());
        }
    }
    {
        let binding = parse_single(b"a=set_surface_title:surface").unwrap();
        assert!(matches!(binding.action, Action::SetSurfaceTitle(_)));
        if let Action::SetSurfaceTitle(v) = &binding.action {
            assert_eq!(b"surface", v.as_slice());
        }
    }
    {
        let binding = parse_single(b"a=set_tab_title:tab").unwrap();
        assert!(matches!(binding.action, Action::SetTabTitle(_)));
        if let Action::SetTabTitle(v) = &binding.action {
            assert_eq!(b"tab", v.as_slice());
        }
    }
}

// test "parse: action with enum"
#[test]
fn parse_action_with_enum() {
    // parameter
    {
        let binding = parse_single(b"a=new_split:right").unwrap();
        assert!(matches!(binding.action, Action::NewSplit(_)));
        if let Action::NewSplit(v) = binding.action {
            assert_eq!(SplitDirection::Right, v);
        }
    }
}

// test "parse: action with enum with default"
#[test]
fn parse_action_with_enum_with_default() {
    // parameter
    {
        let binding = parse_single(b"a=new_split").unwrap();
        assert!(matches!(binding.action, Action::NewSplit(_)));
        if let Action::NewSplit(v) = binding.action {
            assert_eq!(SplitDirection::Auto, v);
        }
    }
}

// test "parse: action with int"
#[test]
fn parse_action_with_int() {
    // parameter
    {
        let binding = parse_single(b"a=jump_to_prompt:-1").unwrap();
        assert!(matches!(binding.action, Action::JumpToPrompt(_)));
        if let Action::JumpToPrompt(v) = binding.action {
            assert_eq!(-1i16, v);
        }
    }
    {
        let binding = parse_single(b"a=jump_to_prompt:10").unwrap();
        assert!(matches!(binding.action, Action::JumpToPrompt(_)));
        if let Action::JumpToPrompt(v) = binding.action {
            assert_eq!(10i16, v);
        }
    }
}

// test "parse: action with float"
#[test]
fn parse_action_with_float() {
    // parameter
    {
        let binding = parse_single(b"a=scroll_page_fractional:-0.5").unwrap();
        assert!(matches!(binding.action, Action::ScrollPageFractional(_)));
        if let Action::ScrollPageFractional(v) = binding.action {
            assert_eq!(-0.5f32, v);
        }
    }
    {
        let binding = parse_single(b"a=scroll_page_fractional:+0.5").unwrap();
        assert!(matches!(binding.action, Action::ScrollPageFractional(_)));
        if let Action::ScrollPageFractional(v) = binding.action {
            assert_eq!(0.5f32, v);
        }
    }
}

/// S100-F1: the numeric action parameters go through Zig's
/// `std.fmt.parseInt(T, value, 10)` / `std.fmt.parseFloat(T, value)`
/// (`Binding.zig:1225-1231`), so they take `_` separators, `-0` for an
/// unsigned type and hex floats, and reject what Zig rejects. Every
/// integer type the actions use is covered (i16, isize, usize, u16) plus
/// the f32 one.
#[test]
fn parse_action_numbers_take_the_zig_grammar() {
    let action = |s: &[u8]| parse_single(s).map(|b| b.action);

    assert!(matches!(action(b"a=goto_tab:1_0"), Ok(Action::GotoTab(10))));
    assert!(matches!(action(b"a=goto_tab:-0"), Ok(Action::GotoTab(0))));
    assert!(matches!(
        action(b"a=scroll_to_row:+1_000"),
        Ok(Action::ScrollToRow(1000))
    ));
    assert!(matches!(
        action(b"a=scroll_page_lines:-1_0"),
        Ok(Action::ScrollPageLines(-10))
    ));
    assert!(matches!(
        action(b"a=move_tab:-2_0"),
        Ok(Action::MoveTab(-20))
    ));
    match action(b"a=resize_split:up,1_0") {
        Ok(Action::ResizeSplit(v)) => assert_eq!(10u16, v.1),
        other => panic!("resize_split:up,1_0 parsed as {other:?}"),
    }
    assert!(matches!(action(b"a=set_font_size:0x1.8p3"), Ok(Action::SetFontSize(v)) if v == 12.0));
    assert!(
        matches!(action(b"a=increase_font_size:1_0.5"), Ok(Action::IncreaseFontSize(v)) if v == 10.5)
    );

    for bad in [
        &b"a=goto_tab:_1"[..],
        b"a=goto_tab:1_",
        b"a=goto_tab:-1",
        b"a=set_font_size:1__0",
        b"a=set_font_size:0x",
    ] {
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(bad).map(|_| ()),
            "{:?}",
            String::from_utf8_lossy(bad)
        );
    }
}

// test "parse: action with a tuple"
#[test]
fn parse_action_with_a_tuple() {
    // parameter
    {
        let binding = parse_single(b"a=resize_split:up,10").unwrap();
        assert!(matches!(binding.action, Action::ResizeSplit(_)));
        if let Action::ResizeSplit(v) = binding.action {
            assert_eq!(SplitResizeDirection::Up, v.0);
            assert_eq!(10u16, v.1);
        }
    }

    // missing parameter
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=resize_split:up")
    );

    // too many
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=resize_split:up,10,12")
    );

    // invalid type
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=resize_split:up,four")
    );
}

// test "Action: clone"
#[test]
fn action_clone() {
    {
        let a: Action = Action::Ignore;
        let b = a.clone_action();
        assert!(b == Action::Ignore);
    }

    {
        let a: Action = Action::Text(b"foo".to_vec());
        let b = a.clone_action();
        assert!(matches!(b, Action::Text(_)));
    }
}

// test "parse: increase_font_size"
#[test]
fn parse_increase_font_size() {
    {
        let binding = parse_single(b"a=increase_font_size:1.5").unwrap();
        assert!(matches!(binding.action, Action::IncreaseFontSize(_)));
        if let Action::IncreaseFontSize(v) = binding.action {
            assert_eq!(1.5, v);
        }
    }
}

// test "parse: decrease_font_size"
#[test]
fn parse_decrease_font_size() {
    {
        let binding = parse_single(b"a=decrease_font_size:2.5").unwrap();
        assert!(matches!(binding.action, Action::DecreaseFontSize(_)));
        if let Action::DecreaseFontSize(v) = binding.action {
            assert_eq!(2.5, v);
        }
    }
}

// test "parse: reset_font_size"
#[test]
fn parse_reset_font_size() {
    {
        let binding = parse_single(b"a=reset_font_size").unwrap();
        assert!(matches!(binding.action, Action::ResetFontSize));
    }
}

// test "parse: set_font_size"
#[test]
fn parse_set_font_size() {
    {
        let binding = parse_single(b"a=set_font_size:13.5").unwrap();
        assert!(matches!(binding.action, Action::SetFontSize(_)));
        if let Action::SetFontSize(v) = binding.action {
            assert_eq!(13.5, v);
        }
    }
}

// test "parse: copy to clipboard default"
#[test]
fn parse_copy_to_clipboard_default() {
    {
        let binding = parse_single(b"a=copy_to_clipboard").unwrap();
        assert!(matches!(binding.action, Action::CopyToClipboard(_)));
        if let Action::CopyToClipboard(v) = binding.action {
            assert_eq!(CopyToClipboard::Mixed, v);
        }
    }
}

// test "parse: copy to clipboard explicit"
#[test]
fn parse_copy_to_clipboard_explicit() {
    {
        let binding = parse_single(b"a=copy_to_clipboard:html").unwrap();
        assert!(matches!(binding.action, Action::CopyToClipboard(_)));
        if let Action::CopyToClipboard(v) = binding.action {
            assert_eq!(CopyToClipboard::Html, v);
        }
    }
}

// test "parse: write screen file no format"
#[test]
fn parse_write_screen_file_no_format() {
    {
        let binding = parse_single(b"a=write_screen_file:copy").unwrap();
        assert!(matches!(binding.action, Action::WriteScreenFile(_)));
        if let Action::WriteScreenFile(v) = binding.action {
            assert_eq!(WriteScreen::COPY, v);
        }
    }
}

// test "parse: write screen file format"
#[test]
fn parse_write_screen_file_format() {
    {
        let binding = parse_single(b"a=write_screen_file:copy,html").unwrap();
        assert!(matches!(binding.action, Action::WriteScreenFile(_)));
        if let Action::WriteScreenFile(v) = binding.action {
            assert_eq!(
                WriteScreen {
                    action: WriteScreenAction::Copy,
                    emit: WriteScreenFormat::Html
                },
                v
            );
        }
    }
}

// test "parse: write screen file format as string"
#[test]
fn parse_write_screen_file_format_as_string() {
    {
        let mut buf = AllocatingWriter::init();
        let binding = parse_single(b"a=write_screen_file:copy,html").unwrap();
        binding.action.format(&mut buf).unwrap();
        assert_eq!(b"write_screen_file:copy,html", buf.written());
    }
}

// test "parse: write screen file invalid"
#[test]
fn parse_write_screen_file_invalid() {
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=write_screen_file:")
    );
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=write_screen_file:,")
    );
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=write_screen_file:copy,")
    );
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a=write_screen_file:copy,html,extra")
    );
}

// test "action: format"
#[test]
fn action_format() {
    let a: Action = Action::Text("👻Rustty'\"".as_bytes().to_vec());

    let mut buf = AllocatingWriter::init();
    a.format(&mut buf).unwrap();

    // const b = try Binding.Action.parse(buf.written());
    let b = Action::parse(buf.written()).unwrap();
    // try testing.expect(a.equal(b));
    assert!(a.equal(&b));
}

// test "action: format set title"
#[test]
fn action_format_set_title() {
    let a: Action = Action::SetTabTitle(b"foo bar".to_vec());

    let mut buf = AllocatingWriter::init();
    a.format(&mut buf).unwrap();
    assert_eq!(b"set_tab_title:foo bar", buf.written());
}
