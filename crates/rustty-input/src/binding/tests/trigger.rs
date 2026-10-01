//! PORTS: ghostty src/input/Binding.zig — the `Flags`, `SplitFocusDirection` and trigger-parse
//! (`parse: ...`, `Trigger: equal`) tests.

use super::*;

// test "cval"  (nested in `Flags`; the fn carries the reference name)
#[test]
fn cval() {
    assert_eq!(0b0001u8, Flags::default().cval());
    assert_eq!(
        0b0000u8,
        Flags {
            consumed: false,
            ..Flags::default()
        }
        .cval()
    );
    assert_eq!(
        0b0011u8,
        Flags {
            all: true,
            ..Flags::default()
        }
        .cval()
    );
    assert_eq!(
        0b0101u8,
        Flags {
            global: true,
            ..Flags::default()
        }
        .cval()
    );
    assert_eq!(
        0b1001u8,
        Flags {
            performable: true,
            ..Flags::default()
        }
        .cval()
    );
    assert_eq!(
        0b1111u8,
        Flags {
            consumed: true,
            all: true,
            global: true,
            performable: true,
        }
        .cval()
    );
}

// test "parse"  (SplitFocusDirection)
#[test]
fn split_focus_direction_parse() {
    assert_eq!(
        SplitFocusDirection::Previous,
        SplitFocusDirection::parse(b"previous").unwrap()
    );
    assert_eq!(
        SplitFocusDirection::Next,
        SplitFocusDirection::parse(b"next").unwrap()
    );

    assert_eq!(
        SplitFocusDirection::Up,
        SplitFocusDirection::parse(b"up").unwrap()
    );
    assert_eq!(
        SplitFocusDirection::Left,
        SplitFocusDirection::parse(b"left").unwrap()
    );
    assert_eq!(
        SplitFocusDirection::Down,
        SplitFocusDirection::parse(b"down").unwrap()
    );
    assert_eq!(
        SplitFocusDirection::Right,
        SplitFocusDirection::parse(b"right").unwrap()
    );

    assert_eq!(
        SplitFocusDirection::Up,
        SplitFocusDirection::parse(b"top").unwrap()
    );
    assert_eq!(
        SplitFocusDirection::Down,
        SplitFocusDirection::parse(b"bottom").unwrap()
    );

    assert_eq!(Err(Error::InvalidFormat), SplitFocusDirection::parse(b""));
    assert_eq!(
        Err(Error::InvalidFormat),
        SplitFocusDirection::parse(b"green")
    );
}

// test "parse: triggers"
#[test]
fn parse_triggers() {
    // single character
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

    // single modifier
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"shift+a=ignore").unwrap()
    );
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"ctrl+a=ignore").unwrap()
    );

    // multiple modifier
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ctrl: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"shift+ctrl+a=ignore").unwrap()
    );

    // key can come before modifier
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"a+shift=ignore").unwrap()
    );

    // physical keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: TriggerKey::Physical(KeyKey::KeyA),
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"shift+key_a=ignore").unwrap()
    );

    // unicode keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni('ö' as u32),
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single("shift+ö=ignore".as_bytes()).unwrap()
    );

    // unconsumed keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags {
                consumed: false,
                ..Default::default()
            },
        },
        parse_single(b"unconsumed:shift+a=ignore").unwrap()
    );

    // unconsumed physical keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: TriggerKey::Physical(KeyKey::KeyA),
            },
            action: Action::Ignore,
            flags: Flags {
                consumed: false,
                ..Default::default()
            },
        },
        parse_single(b"unconsumed:key_a+shift=ignore").unwrap()
    );

    // performable keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags {
                performable: true,
                ..Default::default()
            },
        },
        parse_single(b"performable:shift+a=ignore").unwrap()
    );

    // invalid key
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"foo=ignore")
    );

    // repeated control
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"shift+shift+a=ignore")
    );

    // multiple character
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"a+b=ignore")
    );
}

// test "parse: w3c key names"
#[test]
fn parse_w3c_key_names() {
    // Exact match
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: TriggerKey::Physical(KeyKey::KeyA),
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"KeyA=ignore").unwrap()
    );

    // Case-sensitive
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"Keya=ignore")
    );
}

// test "parse: catch_all"
#[test]
fn parse_catch_all() {
    // Basic catch_all
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: TriggerKey::CatchAll,
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"catch_all=ignore").unwrap()
    );

    // catch_all with modifiers
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
                key: TriggerKey::CatchAll,
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"ctrl+catch_all=ignore").unwrap()
    );
}

// test "parse: plus sign"
#[test]
fn parse_plus_sign() {
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: uni(b'+' as u32),
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"+=ignore").unwrap()
    );

    // Modifier
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: uni(b'+' as u32),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"ctrl++=ignore").unwrap()
    );

    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"++=ignore")
    );
}

// test "parse: equals sign"
#[test]
fn parse_equals_sign() {
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: uni(b'=' as u32),
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"==ignore").unwrap()
    );

    // Modifier
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: uni(b'=' as u32),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"ctrl+==ignore").unwrap()
    );

    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"=ignore")
    );
}

// test "parse: text action equals sign"
#[test]
fn parse_text_action_equals_sign() {
    {
        let binding = parse_single(b"==text:=").unwrap();
        assert_eq!(
            Trigger {
                key: uni(b'=' as u32),
                ..Default::default()
            },
            binding.trigger
        );
        assert_eq!(b"=", matches_text(&binding.action));
    }

    {
        let binding = parse_single(b"==text:=hello").unwrap();
        assert_eq!(
            Trigger {
                key: uni(b'=' as u32),
                ..Default::default()
            },
            binding.trigger
        );
        assert_eq!(b"=hello", matches_text(&binding.action));
    }

    {
        let binding = parse_single(b"ctrl+==text:=hello").unwrap();
        assert_eq!(
            Trigger {
                key: uni(b'=' as u32),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            },
            binding.trigger
        );
        assert_eq!(b"=hello", matches_text(&binding.action));
    }

    {
        let binding = parse_single(b"=+ctrl=text:=hello").unwrap();
        assert_eq!(
            Trigger {
                key: uni(b'=' as u32),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            },
            binding.trigger
        );
        assert_eq!(b"=hello", matches_text(&binding.action));
    }
}

// PORT NOTE: helper for `binding.action.text` — extracts the `text` payload.
fn matches_text(a: &Action) -> &[u8] {
    match a {
        Action::Text(v) => v,
        _ => panic!("expected text action"),
    }
}

// For Rustty 1.2+ we changed our key names to match the W3C and removed
// `physical:`. This tests the backwards compatibility with the old format.
// Note that our backwards compatibility isn't 100% perfect since triggers
// like `a` now map to unicode instead of "translated" (which was also
// removed). But we did our best here with what was unambiguous.
// test "parse: backwards compatibility with <= 1.1.x"
#[test]
fn parse_backwards_compatibility_with_1_1_x() {
    // simple, for sanity
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: uni(b'0' as u32),
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"zero=ignore").unwrap()
    );
    assert_eq!(
        Binding {
            trigger: Trigger {
                key: TriggerKey::Physical(KeyKey::Digit0),
                ..Default::default()
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"physical:zero=ignore").unwrap()
    );

    // duplicates
    assert_eq!(
        Err(ParseSingleError::InvalidFormat),
        parse_single(b"zero+one=ignore")
    );

    // test our full map
    for (k, v) in BACKWARDS_COMPATIBLE_KEYS {
        // try std.fmt.bufPrint(&buf, "{s}=ignore", .{k})
        let mut input = k.to_vec();
        input.extend_from_slice(b"=ignore");
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: *v,
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(&input).unwrap()
        );
    }
}

// test "parse: global triggers"
#[test]
fn parse_global_triggers() {
    // global keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags {
                global: true,
                ..Default::default()
            },
        },
        parse_single(b"global:shift+a=ignore").unwrap()
    );

    // global physical keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: TriggerKey::Physical(KeyKey::KeyA),
            },
            action: Action::Ignore,
            flags: Flags {
                global: true,
                ..Default::default()
            },
        },
        parse_single(b"global:key_a+shift=ignore").unwrap()
    );

    // global unconsumed keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags {
                global: true,
                consumed: false,
                ..Default::default()
            },
        },
        parse_single(b"unconsumed:global:a+shift=ignore").unwrap()
    );

    // global sequences not allowed
    {
        let mut p = Parser::init(b"global:a>b=ignore").unwrap();
        assert_eq!(Err(Error::InvalidFormat), p.next());
    }
}

// test "parse: all triggers"
#[test]
fn parse_all_triggers() {
    // all keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags {
                all: true,
                ..Default::default()
            },
        },
        parse_single(b"all:shift+a=ignore").unwrap()
    );

    // all physical keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: TriggerKey::Physical(KeyKey::KeyA),
            },
            action: Action::Ignore,
            flags: Flags {
                all: true,
                ..Default::default()
            },
        },
        parse_single(b"all:key_a+shift=ignore").unwrap()
    );

    // all unconsumed keys
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            action: Action::Ignore,
            flags: Flags {
                all: true,
                consumed: false,
                ..Default::default()
            },
        },
        parse_single(b"unconsumed:all:a+shift=ignore").unwrap()
    );

    // all sequences not allowed
    {
        let mut p = Parser::init(b"all:a>b=ignore").unwrap();
        assert_eq!(Err(Error::InvalidFormat), p.next());
    }
}

/// S65-F1: `foldedCodepoint`'s Unicode slow path is
/// `uucode.get(.case_folding_full, cp)` (`Binding.zig:2003-2011`), so two
/// triggers differing only in the case of a non-ASCII letter hash and
/// compare equal, and a multi-code-point folding fills more than one slot.
#[test]
fn trigger_folds_non_ascii_case() {
    // U+00C9 É folds to U+00E9 é.
    assert_eq!(folded_codepoint(0x00C9), [0x00E9, 0, 0]);
    // U+00DF ß folds to "ss", and so does U+1E9E ẞ.
    assert_eq!(folded_codepoint(0x00DF), [0x73, 0x73, 0]);
    assert_eq!(folded_codepoint(0x1E9E), [0x73, 0x73, 0]);
    // No folding: the code point itself.
    assert_eq!(folded_codepoint(0x00E9), [0x00E9, 0, 0]);

    let upper = Trigger {
        key: TriggerKey::Unicode(0x00C9),
        mods: Mods::default(),
    };
    let lower = Trigger {
        key: TriggerKey::Unicode(0x00E9),
        mods: Mods::default(),
    };
    // `foldedEqual` is the binding set's equality (`bindingSetEqual`);
    // plain `equal` stays exact.
    assert!(upper.folded_equal(lower));
    assert!(!upper.equal(lower));
    assert_eq!(upper.hash(), lower.hash());
    // A different letter still differs.
    let other = Trigger {
        key: TriggerKey::Unicode(0x00E8),
        mods: Mods::default(),
    };
    assert!(!upper.folded_equal(other));
}

// test "Trigger: equal"
#[test]
fn trigger_equal() {
    // Equal physical keys
    {
        let t1 = Trigger {
            key: TriggerKey::Physical(KeyKey::ArrowUp),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
        };
        let t2 = Trigger {
            key: TriggerKey::Physical(KeyKey::ArrowUp),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
        };
        assert!(t1.equal(t2));
    }

    // Different physical keys
    {
        let t1 = Trigger {
            key: TriggerKey::Physical(KeyKey::ArrowUp),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
        };
        let t2 = Trigger {
            key: TriggerKey::Physical(KeyKey::ArrowDown),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
        };
        assert!(!t1.equal(t2));
    }

    // Different mods
    {
        let t1 = Trigger {
            key: TriggerKey::Physical(KeyKey::ArrowUp),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
        };
        let t2 = Trigger {
            key: TriggerKey::Physical(KeyKey::ArrowUp),
            mods: Mods {
                shift: true,
                ..Default::default()
            },
        };
        assert!(!t1.equal(t2));
    }

    // Equal unicode keys
    {
        let t1 = Trigger {
            key: uni(b'a' as u32),
            mods: Mods::default(),
        };
        let t2 = Trigger {
            key: uni(b'a' as u32),
            mods: Mods::default(),
        };
        assert!(t1.equal(t2));
    }

    // Different unicode keys
    {
        let t1 = Trigger {
            key: uni(b'a' as u32),
            mods: Mods::default(),
        };
        let t2 = Trigger {
            key: uni(b'b' as u32),
            mods: Mods::default(),
        };
        assert!(!t1.equal(t2));
    }

    // Different key types
    {
        let t1 = Trigger {
            key: uni(b'a' as u32),
            mods: Mods::default(),
        };
        let t2 = Trigger {
            key: TriggerKey::Physical(KeyKey::KeyA),
            mods: Mods::default(),
        };
        assert!(!t1.equal(t2));
    }

    // catch_all
    {
        let t1 = Trigger {
            key: TriggerKey::CatchAll,
            mods: Mods::default(),
        };
        let t2 = Trigger {
            key: TriggerKey::CatchAll,
            mods: Mods::default(),
        };
        assert!(t1.equal(t2));
    }

    // catch_all with different mods
    {
        let t1 = Trigger {
            key: TriggerKey::CatchAll,
            mods: Mods::default(),
        };
        let t2 = Trigger {
            key: TriggerKey::CatchAll,
            mods: Mods {
                alt: true,
                ..Default::default()
            },
        };
        assert!(!t1.equal(t2));
    }
}

// test "parse: modifier aliases"
#[test]
fn parse_modifier_aliases() {
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    super_: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32)
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"cmd+a=ignore").unwrap()
    );
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    super_: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32)
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"command+a=ignore").unwrap()
    );

    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    alt: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32)
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"opt+a=ignore").unwrap()
    );
    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    alt: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32)
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"option+a=ignore").unwrap()
    );

    assert_eq!(
        Binding {
            trigger: Trigger {
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32)
            },
            action: Action::Ignore,
            flags: Flags::default(),
        },
        parse_single(b"control+a=ignore").unwrap()
    );
}
