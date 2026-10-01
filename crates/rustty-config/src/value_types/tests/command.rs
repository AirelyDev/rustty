//! PORTS: ghostty src/config/Config.zig — the `RepeatableCommand` tests.

use super::*;

// -----------------------------------------------------------------------
// RepeatableCommand
//
// PORT NOTE: the reference threads an `ArenaAllocator` through `parseCLI`;
// the Rust port owns its data (no allocator), so the arena setup/teardown
// has no analog. `inputpkg.Binding.Action` -> `rustty_input::binding::Action`;
// `inputpkg.command.defaults` -> `rustty_input::command::defaults()`.
// -----------------------------------------------------------------------
use rustty_input::binding::Action;

// No reference `test {}` — PROVING test for S75-F1. The expected values are
// an ORACLE TABLE: each one is `@bitCast(u64, std.fmt.parseFloat(f64, input))`
// as printed by a throwaway `zig test` against the SAME Zig std the oracle
// pins (0.16.0, `ghostty/build.zig.zon` `.minimum_zig_version`), so this
// asserts bit-for-bit agreement with the reference parser rather than
// agreement with a hand-derived expectation.
//
// The rows that FAILED before this change — every one accepted by Zig and
// rejected by Rust's `f64::from_str` — are the `_`-separator and hex-float
// forms: `1_000.5`, `1_0`, `1e1_0`, `1_0e1_0`, `1_2_3.4_5_6e1_2`, `0x1p8`,
// `0x1p1`, `0x1.8p0`, `0x1.8`, `0X1P1`, `0x1e`, `0x1_0p0`, `0x.8p1`,
// `0x1.fffffffffffffp1023`, `0x1p-1074`, `0x1p-1075`, `0x0p0`, `-0x0p0`,
// `0x123456789abcdef123456789abcdefp0`, `0x1_2.3_4p1_0`, `0x1.8p-1`.
// The rest are REGRESSION rows: forms Zig rejects (`1_`, `_1`, `1__0`,
// `1_.0`, `1._0`, `0x`, `0xp1`, `0x1p`, `1e`, `1abc`, leading spaces, a bare
// sign, empty) that a looser grammar would have started accepting, plus the
// inf/nan spellings and ordinary decimals that must not have moved.
#[test]
fn parse_float_f64_matches_std_fmt_parsefloat() {
    let cases: &[(&[u8], Option<u64>)] = &[
        (b"1_000.5".as_slice(), Some(0x408f440000000000u64)),
        (b"1_0".as_slice(), Some(0x4024000000000000u64)),
        (b"0x1p8".as_slice(), Some(0x4070000000000000u64)),
        (b"0x1p1".as_slice(), Some(0x4000000000000000u64)),
        (b"0x1.8p0".as_slice(), Some(0x3ff8000000000000u64)),
        (b"0x1.8".as_slice(), Some(0x3ff8000000000000u64)),
        (b"0X1P1".as_slice(), Some(0x4000000000000000u64)),
        (b"0x1e".as_slice(), Some(0x403e000000000000u64)),
        (b"0x1_0p0".as_slice(), Some(0x4030000000000000u64)),
        (b"1e1_0".as_slice(), Some(0x4202a05f20000000u64)),
        (b"1_0e1_0".as_slice(), Some(0x42374876e8000000u64)),
        (b"1_".as_slice(), None),
        (b"_1".as_slice(), None),
        (b"1__0".as_slice(), None),
        (b"1_.0".as_slice(), None),
        (b"1._0".as_slice(), None),
        (b"1.5".as_slice(), Some(0x3ff8000000000000u64)),
        (b"-1.5".as_slice(), Some(0xbff8000000000000u64)),
        (b"+1.5".as_slice(), Some(0x3ff8000000000000u64)),
        (b"inf".as_slice(), Some(0x7ff0000000000000u64)),
        (b"-inf".as_slice(), Some(0xfff0000000000000u64)),
        (b"Infinity".as_slice(), Some(0x7ff0000000000000u64)),
        (b"nan".as_slice(), Some(0x7ff8000000000000u64)),
        (b"NaN".as_slice(), Some(0x7ff8000000000000u64)),
        (b"-nan".as_slice(), Some(0x7ff8000000000000u64)),
        (b"".as_slice(), None),
        (b"-".as_slice(), None),
        (b"+".as_slice(), None),
        (b"1abc".as_slice(), None),
        (b"   1".as_slice(), None),
        (b"0x".as_slice(), None),
        (b"0xp1".as_slice(), None),
        (b"0x1p".as_slice(), None),
        (b"1e".as_slice(), None),
        (b".5".as_slice(), Some(0x3fe0000000000000u64)),
        (b"5.".as_slice(), Some(0x4014000000000000u64)),
        (b"0x.8p1".as_slice(), Some(0x3ff0000000000000u64)),
        (b"1e400".as_slice(), Some(0x7ff0000000000000u64)),
        (b"-1e400".as_slice(), Some(0xfff0000000000000u64)),
        (b"1e-400".as_slice(), Some(0x0u64)),
        (
            b"0x1.fffffffffffffp1023".as_slice(),
            Some(0x7fefffffffffffffu64),
        ),
        (b"0x1p-1074".as_slice(), Some(0x1u64)),
        (b"0x1p-1075".as_slice(), Some(0x0u64)),
        (b"0x0p0".as_slice(), Some(0x0u64)),
        (b"-0x0p0".as_slice(), Some(0x8000000000000000u64)),
        (
            b"12345678901234567890123456789012".as_slice(),
            Some(0x46637a61478c8b28u64),
        ),
        (
            b"1.7976931348623157e308".as_slice(),
            Some(0x7fefffffffffffffu64),
        ),
        (
            b"0x123456789abcdef123456789abcdefp0".as_slice(),
            Some(0x47323456789abcdeu64),
        ),
        (b"1_2_3.4_5_6e1_2".as_slice(), Some(0x42dc1215c1a00000u64)),
        (b"0x1_2.3_4p1_0".as_slice(), Some(0x40d2340000000000u64)),
        (b"2.5".as_slice(), Some(0x4004000000000000u64)),
        (b"100".as_slice(), Some(0x4059000000000000u64)),
        (b"0.0001".as_slice(), Some(0x3f1a36e2eb1c432du64)),
        (b"1e-5".as_slice(), Some(0x3ee4f8b588e368f1u64)),
        (b"0x1.8p-1".as_slice(), Some(0x3fe8000000000000u64)),
    ];
    for &(input, want) in cases {
        let got = parse_float_f64(input).ok().map(f64::to_bits);
        assert_eq!(
            got,
            want,
            "parse_float_f64({:?})",
            String::from_utf8_lossy(input)
        );
    }
}

// The S75-F1 surface the audit actually cites: `RepeatableFontVariation`'s
// axis value and `MouseScrollMultiplier`'s bare/`precision:`/`discrete:`
// fields all go through `parse_float_f64`. Before the fix each of these
// produced `error.InvalidValue` — a config diagnostic and a DISCARDED
// setting — where Ghostty loads the value.
#[test]
fn font_variation_and_scroll_multiplier_accept_zig_float_syntaxes() {
    let mut v = RepeatableFontVariation::default();
    // `wght=1_000` — digit separators.
    v.parse_cli(Some(b"wght=1_000")).unwrap();
    // `wdth=0x1p8` — a hex float, exponent present.
    v.parse_cli(Some(b"wdth=0x1p8")).unwrap();
    // `slnt=0x1.8` — a hex float with NO exponent, which C's `strtod`
    // rejects and Zig's parser does not.
    v.parse_cli(Some(b"slnt=0x1.8")).unwrap();
    assert_eq!(
        v.list.iter().map(|x| x.value).collect::<Vec<f64>>(),
        vec![1000.0, 256.0, 1.5],
    );

    let mut m = MouseScrollMultiplier::default();
    m.parse_cli(Some(b"precision:1_0,discrete:0x1p1")).unwrap();
    assert_eq!((m.precision, m.discrete), (10.0, 2.0));

    // Still rejected, exactly as the reference rejects them.
    assert!(RepeatableFontVariation::default()
        .parse_cli(Some(b"wght=1__0"))
        .is_err());
    assert!(RepeatableFontVariation::default()
        .parse_cli(Some(b"wght=1_"))
        .is_err());
}

// No reference `test {}` — PROVING tests for S75-F2 and for the two
// UNCLAIMED sibling sites the audit never names (`Config.zig:8887, 8893`,
// `RepeatableCommand.parseCLI`; PROCEDURE.md blind spot 4). All five are
// `Keybinds`/`RepeatableCommand` "special value" announcements that were
// carried as inert comments: `value_types.rs` contained no live emission at
// all before this change.
#[test]
fn keybinds_parse_cli_announces_the_reference_special_values() {
    // `keybind =` -> reset to the default keybinds.
    let mut set: Keybinds = Keybinds::default();
    crate::config_struct::log_capture::arm();
    set.parse_cli(Some(b"")).unwrap();
    assert_eq!(
        crate::config_struct::log_capture::take(),
        vec!["info(config): config has 'keybind =', using default keybinds".to_string()],
    );

    // `keybind = clear` -> drop every binding and every table.
    crate::config_struct::log_capture::arm();
    set.parse_cli(Some(b"clear")).unwrap();
    assert_eq!(
        crate::config_struct::log_capture::take(),
        vec!["info(config): config has 'keybind = clear', all keybinds cleared".to_string()],
    );

    // `keybind = <name>/` -> clear that table. `log.debug`, not `log.info`,
    // and the ONLY one of the three that formats a value. The reference's
    // `std_options.log_level` is `.debug` only in a Debug build and `.info`
    // otherwise (`main_ghostty.zig:208-211`), so a release build erases it:
    // the line is asserted in a debug build and its ABSENCE in a release one.
    set.parse_cli(Some(b"foo/shift+a=csi:hello")).unwrap();
    assert_eq!(
        set.tables.get(&b"foo".to_vec()).unwrap().bindings_count(),
        1
    );
    crate::config_struct::log_capture::arm();
    set.parse_cli(Some(b"foo/")).unwrap();
    // The clear arm ran on both profiles.
    assert_eq!(
        set.tables.get(&b"foo".to_vec()).unwrap().bindings_count(),
        0
    );
    let expected: Vec<String> = if cfg!(debug_assertions) {
        vec!["debug(config): config has 'keybind = foo/', table cleared".to_string()]
    } else {
        Vec::new()
    };
    assert_eq!(crate::config_struct::log_capture::take(), expected);
}

#[test]
fn repeatable_command_parse_cli_announces_the_reference_special_values() {
    let mut list = RepeatableCommand::default();
    crate::config_struct::log_capture::arm();
    list.parse_cli(Some(b"")).unwrap();
    assert_eq!(
        crate::config_struct::log_capture::take(),
        vec![
            "info(config): config has 'command-palette-entry =', using default entries".to_string()
        ],
    );

    crate::config_struct::log_capture::arm();
    list.parse_cli(Some(b"clear")).unwrap();
    assert_eq!(
        crate::config_struct::log_capture::take(),
        vec![concat!(
            "info(config): config has 'command-palette-entry = clear', ",
            "all command entries cleared"
        )
        .to_string()],
    );
}

// test "RepeatableCommand parseCLI"
// PORT NOTE: `set_font_size:3.14` is the reference test's literal input; the
// `3.14` expected value is the parsed result, not an approximation of PI — the
// `approx_constant` lint is allowed for this faithful expected value.
#[allow(clippy::approx_constant)]
#[test]
fn repeatable_command_parse_cli() {
    let mut list = RepeatableCommand::default();
    // try list.parseCLI(alloc, "title:Foo,action:ignore");
    list.parse_cli(Some(b"title:Foo,action:ignore")).unwrap();
    // try list.parseCLI(alloc, "title:Bar,description:bobr,action:text:ale bydle");
    list.parse_cli(Some(b"title:Bar,description:bobr,action:text:ale bydle"))
        .unwrap();
    // try list.parseCLI(alloc, "title:Quux,description:boo,action:increase_font_size:2.5");
    list.parse_cli(Some(
        b"title:Quux,description:boo,action:increase_font_size:2.5",
    ))
    .unwrap();
    // try list.parseCLI(alloc, "title:Baz,description:Raspberry Pie,action:set_font_size:3.14");
    list.parse_cli(Some(
        b"title:Baz,description:Raspberry Pie,action:set_font_size:3.14",
    ))
    .unwrap();

    // try testing.expectEqual(@as(usize, 4), list.value.items.len);
    assert_eq!(4usize, list.value.len());

    // try testing.expectEqual(inputpkg.Binding.Action.ignore, list.value.items[0].action);
    assert_eq!(Action::Ignore, list.value[0].action);
    // try testing.expectEqualStrings("Foo", list.value.items[0].title);
    assert_eq!(b"Foo".as_slice(), list.value[0].title.as_slice());

    // try testing.expect(list.value.items[1].action == .text);
    assert!(matches!(list.value[1].action, Action::Text(_)));
    // try testing.expectEqualStrings("ale bydle", list.value.items[1].action.text);
    assert_eq!(Action::Text(b"ale bydle".to_vec()), list.value[1].action);
    // try testing.expectEqualStrings("Bar", list.value.items[1].title);
    assert_eq!(b"Bar".as_slice(), list.value[1].title.as_slice());
    // try testing.expectEqualStrings("bobr", list.value.items[1].description);
    assert_eq!(b"bobr".as_slice(), list.value[1].description.as_slice());

    // try testing.expectEqual(inputpkg.Binding.Action{ .increase_font_size = 2.5 }, list.value.items[2].action);
    assert_eq!(Action::IncreaseFontSize(2.5), list.value[2].action);
    // try testing.expectEqualStrings("Quux", list.value.items[2].title);
    assert_eq!(b"Quux".as_slice(), list.value[2].title.as_slice());
    // try testing.expectEqualStrings("boo", list.value.items[2].description);
    assert_eq!(b"boo".as_slice(), list.value[2].description.as_slice());

    // try testing.expectEqual(inputpkg.Binding.Action{ .set_font_size = 3.14 }, list.value.items[3].action);
    assert_eq!(Action::SetFontSize(3.14), list.value[3].action);
    // try testing.expectEqualStrings("Baz", list.value.items[3].title);
    assert_eq!(b"Baz".as_slice(), list.value[3].title.as_slice());
    // try testing.expectEqualStrings("Raspberry Pie", list.value.items[3].description);
    assert_eq!(
        b"Raspberry Pie".as_slice(),
        list.value[3].description.as_slice()
    );

    // try list.parseCLI(alloc, "clear");
    list.parse_cli(Some(b"clear")).unwrap();
    // try testing.expectEqual(@as(usize, 0), list.value.items.len);
    assert_eq!(0usize, list.value.len());

    // try list.parseCLI(alloc, "");
    list.parse_cli(Some(b"")).unwrap();
    // try testing.expectEqual(inputpkg.command.defaults.len, list.value.items.len);
    assert_eq!(rustty_input::command::defaults().len(), list.value.len());
}

// test "RepeatableCommand clone rebuilds the C mirror" (5909690de)
#[test]
fn repeatable_command_clone_rebuilds_the_c_mirror() {
    let mut list = RepeatableCommand::default();
    // try list.parseCLI(alloc, "title:Foo,description:bar,action:new_tab");
    list.parse_cli(Some(b"title:Foo,description:bar,action:new_tab"))
        .unwrap();

    // const copy = try list.clone(alloc);
    let copy = list.clone_in();
    // try testing.expectEqual(list.value_c.items.len, copy.value_c.items.len);
    assert_eq!(list.value_c.len(), copy.value_c.len());
    // The clone's C strings must not alias the source's — the
    // source config can be freed while the clone lives on.
    // try testing.expect(list.value_c.items[0].title != copy.value_c.items[0].title);
    assert_ne!(list.value_c[0].title, copy.value_c[0].title);
    // try testing.expectEqualStrings(
    //     std.mem.span(list.value_c.items[0].title),
    //     std.mem.span(copy.value_c.items[0].title),
    // );
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(list.value_c[0].title).to_bytes() },
        unsafe { std::ffi::CStr::from_ptr(copy.value_c[0].title).to_bytes() },
    );
}

// test "RepeatableCommand formatConfig empty"
#[test]
fn repeatable_command_format_config_empty() {
    let mut buf = AllocatingWriter::init();
    let list = RepeatableCommand::default();
    // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    list.format_entry(b"a", &mut buf).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = \n", buf.written());
    assert_eq!(b"a = \n".as_slice(), buf.written());
}

// test "RepeatableCommand formatConfig single item"
#[test]
fn repeatable_command_format_config_single_item() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableCommand::default();
    // try list.parseCLI(alloc, "title:Bobr, action:text:Bober");
    list.parse_cli(Some(b"title:Bobr, action:text:Bober"))
        .unwrap();
    list.format_entry(b"a", &mut buf).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = title:\"Bobr\",action:\"text:Bober\"\n", buf.written());
    assert_eq!(
        b"a = title:\"Bobr\",action:\"text:Bober\"\n".as_slice(),
        buf.written()
    );
}

// PORT NOTE: no reference test. The reference renders each entry through
// `std.Io.Writer.fixed(&buf /* [4096]u8 */)`, so every `print` returns
// `error.OutOfMemory` once the entry passes 4096 bytes and the whole config
// dump fails. The port accumulated into an unbounded `Vec` and always
// succeeded (parity S76-F4). Entry shape for a plain title and a
// `text:z` action: `title:"` (7) + title + `"` (1) + `,action:"` (9) +
// `text:z` (6) + `"` (1) = title + 24, so 4072 title bytes fit and 4073 do
// not.
#[test]
fn repeatable_command_format_config_fixed_buffer_bound() {
    for (title_len, want_ok) in [(4072usize, true), (4073usize, false)] {
        let mut input: Vec<u8> = b"title:".to_vec();
        input.resize(input.len() + title_len, b'x');
        input.extend_from_slice(b", action:text:z");

        let mut list = RepeatableCommand::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let got = list.format_entry(b"a", &mut buf);
        if want_ok {
            got.unwrap();
            // "a = " + entry + "\n"
            assert_eq!(4 + (title_len + 24) + 1, buf.written().len());
        } else {
            assert_eq!(Err(FormatError::OutOfMemory), got);
        }
    }
}

// test "RepeatableCommand formatConfig multiple items"
#[test]
fn repeatable_command_format_config_multiple_items() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableCommand::default();
    // try list.parseCLI(alloc, "title:Bobr, action:text:kurwa");
    list.parse_cli(Some(b"title:Bobr, action:text:kurwa"))
        .unwrap();
    // try list.parseCLI(alloc, "title:Ja,   description: pierdole,  action:text:jakie bydle");
    list.parse_cli(Some(
        b"title:Ja,   description: pierdole,  action:text:jakie bydle",
    ))
    .unwrap();
    list.format_entry(b"a", &mut buf).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = title:\"Bobr\",action:\"text:kurwa\"\na = title:\"Ja\",description:\"pierdole\",action:\"text:jakie bydle\"\n", buf.written());
    assert_eq!(
        b"a = title:\"Bobr\",action:\"text:kurwa\"\na = title:\"Ja\",description:\"pierdole\",action:\"text:jakie bydle\"\n".as_slice(),
        buf.written()
    );
}

// test "RepeatableCommand parseCLI commas"
#[test]
fn repeatable_command_parse_cli_commas() {
    {
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:\"Bo,br\",action:\"text:kur,wa\"");
        list.parse_cli(Some(b"title:\"Bo,br\",action:\"text:kur,wa\""))
            .unwrap();
        // try testing.expectEqual(@as(usize, 1), list.value.items.len);
        assert_eq!(1usize, list.value.len());

        // const item = list.value.items[0];
        let item = &list.value[0];
        // try testing.expectEqualStrings("Bo,br", item.title);
        assert_eq!(b"Bo,br".as_slice(), item.title.as_slice());
        // try testing.expectEqualStrings("", item.description);
        assert_eq!(b"".as_slice(), item.description.as_slice());
        // try testing.expect(item.action == .text);
        assert!(matches!(item.action, Action::Text(_)));
        // try testing.expectEqualStrings("kur,wa", item.action.text);
        assert_eq!(Action::Text(b"kur,wa".to_vec()), item.action);
    }
    {
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:\"Bo,br\",description:\"abc,def\",action:text:kurwa");
        list.parse_cli(Some(
            b"title:\"Bo,br\",description:\"abc,def\",action:text:kurwa",
        ))
        .unwrap();
        // try testing.expectEqual(@as(usize, 1), list.value.items.len);
        assert_eq!(1usize, list.value.len());

        // const item = list.value.items[0];
        let item = &list.value[0];
        // try testing.expectEqualStrings("Bo,br", item.title);
        assert_eq!(b"Bo,br".as_slice(), item.title.as_slice());
        // try testing.expectEqualStrings("abc,def", item.description);
        assert_eq!(b"abc,def".as_slice(), item.description.as_slice());
        // try testing.expect(item.action == .text);
        assert!(matches!(item.action, Action::Text(_)));
        // try testing.expectEqualStrings("kurwa", item.action.text);
        assert_eq!(Action::Text(b"kurwa".to_vec()), item.action);
    }
}

// test "RepeatableCommand cval"
#[test]
fn repeatable_command_cval() {
    let mut list = RepeatableCommand::default();
    // try list.parseCLI(alloc, "title:Foo,action:ignore");
    list.parse_cli(Some(b"title:Foo,action:ignore")).unwrap();
    // try list.parseCLI(alloc, "title:Bar,description:bobr,action:text:ale bydle");
    list.parse_cli(Some(b"title:Bar,description:bobr,action:text:ale bydle"))
        .unwrap();

    // try testing.expectEqual(@as(usize, 2), list.value.items.len);
    assert_eq!(2usize, list.value.len());
    // try testing.expectEqual(@as(usize, 2), list.value_c.items.len);
    assert_eq!(2usize, list.value_c.len());

    // const cv = list.cval();
    let cv = list.cval();
    // try testing.expectEqual(@as(usize, 2), cv.len);
    assert_eq!(2usize, cv.len);

    // PORT NOTE: `cv.commands` is `*const Command.C`; view the `len` entries as
    // a slice. `std.mem.sliceTo(_, 0)` strips the NUL terminator the `C` fields
    // carry.
    //
    // PORT NOTE (ABI FIX, S1-F1/S103-F1): `C`'s fields are now raw
    // `*const c_char` (not owned `Vec<u8>`), so each field is read via
    // `CStr::from_ptr` rather than `slice_to_nul` over an owned byte
    // slice. The pointers stay valid here because `list` (and its
    // `storage`) outlives this scope.
    let commands: &[rustty_input::command::C] =
        unsafe { std::slice::from_raw_parts(cv.commands, cv.len) };

    // First entry
    // try testing.expectEqualStrings("Foo", std.mem.sliceTo(cv.commands[0].title, 0));
    assert_eq!(b"Foo".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[0].title).to_bytes()
    });
    // try testing.expectEqualStrings("ignore", std.mem.sliceTo(cv.commands[0].action_key, 0));
    assert_eq!(b"ignore".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[0].action_key).to_bytes()
    });
    // try testing.expectEqualStrings("ignore", std.mem.sliceTo(cv.commands[0].action, 0));
    assert_eq!(b"ignore".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[0].action).to_bytes()
    });

    // Second entry
    // try testing.expectEqualStrings("Bar", std.mem.sliceTo(cv.commands[1].title, 0));
    assert_eq!(b"Bar".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[1].title).to_bytes()
    });
    // try testing.expectEqualStrings("bobr", std.mem.sliceTo(cv.commands[1].description, 0));
    assert_eq!(b"bobr".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[1].description).to_bytes()
    });
    // try testing.expectEqualStrings("text", std.mem.sliceTo(cv.commands[1].action_key, 0));
    assert_eq!(b"text".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[1].action_key).to_bytes()
    });
    // try testing.expectEqualStrings("text:ale bydle", std.mem.sliceTo(cv.commands[1].action, 0));
    assert_eq!(b"text:ale bydle".as_slice(), unsafe {
        std::ffi::CStr::from_ptr(commands[1].action).to_bytes()
    });
}

// test "RepeatableCommand cval cleared"
#[test]
fn repeatable_command_cval_cleared() {
    let mut list = RepeatableCommand::default();
    // try list.parseCLI(alloc, "title:Foo,action:ignore");
    list.parse_cli(Some(b"title:Foo,action:ignore")).unwrap();
    // try testing.expectEqual(@as(usize, 1), list.cval().len);
    assert_eq!(1usize, list.cval().len);

    // try list.parseCLI(alloc, "clear");
    list.parse_cli(Some(b"clear")).unwrap();
    // try testing.expectEqual(@as(usize, 0), list.cval().len);
    assert_eq!(0usize, list.cval().len);
}
