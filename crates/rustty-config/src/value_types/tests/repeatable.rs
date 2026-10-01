//! PORTS: ghostty src/config/Config.zig — the `RepeatableString`,
//! `SelectionWordChars`, `RepeatableFontVariation`,
//! `RepeatableClipboardCodepointMap` and `RepeatableCodepointMap` tests.

use super::*;

// -----------------------------------------------------------------------
// RepeatableString
// -----------------------------------------------------------------------

// test "parseCLI" (RepeatableString)
#[test]
fn repeatable_string_parse_cli() {
    let mut list = RepeatableString::default();
    // try list.parseCLI(alloc, "A");
    list.parse_cli(Some(b"A")).unwrap();
    // try list.parseCLI(alloc, "B");
    list.parse_cli(Some(b"B")).unwrap();
    // try testing.expectEqual(@as(usize, 2), list.list.items.len);
    assert_eq!(2usize, list.list.len());

    // try list.parseCLI(alloc, "");
    list.parse_cli(Some(b"")).unwrap();
    // try testing.expectEqual(@as(usize, 0), list.list.items.len);
    assert_eq!(0usize, list.list.len());
}

// test "parseCLI overwrite" (RepeatableString)
#[test]
fn repeatable_string_parse_cli_overwrite() {
    let mut list = RepeatableString::default();
    // try list.parseCLI(alloc, "A");
    list.parse_cli(Some(b"A")).unwrap();

    // Set our overwrite flag
    // list.overwrite_next = true;
    list.overwrite_next = true;

    // try list.parseCLI(alloc, "B");
    list.parse_cli(Some(b"B")).unwrap();
    // try testing.expectEqual(@as(usize, 1), list.list.items.len);
    assert_eq!(1usize, list.list.len());
    // try list.parseCLI(alloc, "C");
    list.parse_cli(Some(b"C")).unwrap();
    // try testing.expectEqual(@as(usize, 2), list.list.items.len);
    assert_eq!(2usize, list.list.len());
}

// test "formatConfig empty" (RepeatableString)
#[test]
fn repeatable_string_format_config_empty() {
    let mut buf = AllocatingWriter::init();
    let list = RepeatableString::default();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = \n", buf.written());
    assert_eq!(b"a = \n".as_slice(), buf.written());
}

// test "formatConfig single item" (RepeatableString)
#[test]
fn repeatable_string_format_config_single_item() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableString::default();
    // try list.parseCLI(alloc, "A");
    list.parse_cli(Some(b"A")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = A\n", buf.written());
    assert_eq!(b"a = A\n".as_slice(), buf.written());
}

// test "formatConfig multiple items" (RepeatableString)
#[test]
fn repeatable_string_format_config_multiple_items() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableString::default();
    list.parse_cli(Some(b"A")).unwrap();
    list.parse_cli(Some(b"B")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = A\na = B\n", buf.written());
    assert_eq!(b"a = A\na = B\n".as_slice(), buf.written());
}

// -----------------------------------------------------------------------
// SelectionWordChars
// -----------------------------------------------------------------------

// test "parseCLI" (SelectionWordChars)
#[test]
fn selection_word_chars_parse_cli() {
    let mut chars = SelectionWordChars::default();
    // try chars.parseCLI(alloc, " \t;,");
    chars.parse_cli(Some(b" \t;,")).unwrap();

    // Should have null + 4 characters
    // try testing.expectEqual(@as(usize, 5), chars.codepoints.len);
    assert_eq!(5usize, chars.codepoints.len());
    assert_eq!(0u32, chars.codepoints[0]);
    assert_eq!(b' ' as u32, chars.codepoints[1]);
    assert_eq!(b'\t' as u32, chars.codepoints[2]);
    assert_eq!(b';' as u32, chars.codepoints[3]);
    assert_eq!(b',' as u32, chars.codepoints[4]);
}

// test "parseCLI escape sequences" (SelectionWordChars)
#[test]
fn selection_word_chars_parse_cli_escape_sequences() {
    // \t escape should be parsed as tab
    let mut chars = SelectionWordChars::default();
    // try chars.parseCLI(alloc, " \\t;,");
    chars.parse_cli(Some(b" \\t;,")).unwrap();

    assert_eq!(5usize, chars.codepoints.len());
    assert_eq!(0u32, chars.codepoints[0]);
    assert_eq!(b' ' as u32, chars.codepoints[1]);
    assert_eq!(b'\t' as u32, chars.codepoints[2]);
    assert_eq!(b';' as u32, chars.codepoints[3]);
    assert_eq!(b',' as u32, chars.codepoints[4]);
}

// test "parseCLI backslash escape" (SelectionWordChars)
#[test]
fn selection_word_chars_parse_cli_backslash_escape() {
    // \\ should be parsed as a single backslash
    let mut chars = SelectionWordChars::default();
    // try chars.parseCLI(alloc, "\\\\;");
    // PORT NOTE: the reference Zig literal `"\\\\;"` is the 3 bytes `\`,`\`,`;`;
    // the codepoint iterator decodes the `\\` escape to one backslash.
    chars.parse_cli(Some(b"\\\\;")).unwrap();

    assert_eq!(3usize, chars.codepoints.len());
    assert_eq!(0u32, chars.codepoints[0]);
    assert_eq!(b'\\' as u32, chars.codepoints[1]);
    assert_eq!(b';' as u32, chars.codepoints[2]);
}

// test "parseCLI unicode escape" (SelectionWordChars)
#[test]
fn selection_word_chars_parse_cli_unicode_escape() {
    // \u{2502} should be parsed as │
    let mut chars = SelectionWordChars::default();
    // try chars.parseCLI(alloc, "\\u{2502};");
    chars.parse_cli(Some(b"\\u{2502};")).unwrap();

    assert_eq!(3usize, chars.codepoints.len());
    assert_eq!(0u32, chars.codepoints[0]);
    assert_eq!('│' as u32, chars.codepoints[1]);
    assert_eq!(b';' as u32, chars.codepoints[2]);
}

// -----------------------------------------------------------------------
// RepeatableFontVariation
// -----------------------------------------------------------------------

// test "parseCLI" (RepeatableFontVariation)
#[test]
fn repeatable_font_variation_parse_cli() {
    let mut list = RepeatableFontVariation::default();
    // try list.parseCLI(alloc, "wght=200");
    list.parse_cli(Some(b"wght=200")).unwrap();
    // try list.parseCLI(alloc, "slnt=-15");
    list.parse_cli(Some(b"slnt=-15")).unwrap();

    // try testing.expectEqual(@as(usize, 2), list.list.items.len);
    assert_eq!(2usize, list.list.len());
    // try testing.expectEqual(fontpkg.face.Variation{ .id = Id.init("wght"), .value = 200 }, list.list.items[0]);
    assert_eq!(
        FaceVariation {
            id: face_variation_id_init(b"wght"),
            value: 200.0
        },
        list.list[0]
    );
    // try testing.expectEqual(fontpkg.face.Variation{ .id = Id.init("slnt"), .value = -15 }, list.list.items[1]);
    assert_eq!(
        FaceVariation {
            id: face_variation_id_init(b"slnt"),
            value: -15.0
        },
        list.list[1]
    );
}

// test "parseCLI with whitespace" (RepeatableFontVariation)
#[test]
fn repeatable_font_variation_parse_cli_with_whitespace() {
    let mut list = RepeatableFontVariation::default();
    // try list.parseCLI(alloc, "wght =200");
    list.parse_cli(Some(b"wght =200")).unwrap();
    // try list.parseCLI(alloc, "slnt= -15");
    list.parse_cli(Some(b"slnt= -15")).unwrap();

    assert_eq!(2usize, list.list.len());
    assert_eq!(
        FaceVariation {
            id: face_variation_id_init(b"wght"),
            value: 200.0
        },
        list.list[0]
    );
    assert_eq!(
        FaceVariation {
            id: face_variation_id_init(b"slnt"),
            value: -15.0
        },
        list.list[1]
    );
}

// test "formatConfig single" (RepeatableFontVariation)
#[test]
fn repeatable_font_variation_format_config_single() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableFontVariation::default();
    // try list.parseCLI(alloc, "wght = 200");
    list.parse_cli(Some(b"wght = 200")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = wght=200\n", buf.written());
    assert_eq!(b"a = wght=200\n".as_slice(), buf.written());
}

// -----------------------------------------------------------------------
// RepeatableClipboardCodepointMap
// -----------------------------------------------------------------------

// test "parseCLI codepoint replacement"
#[test]
fn repeatable_clipboard_codepoint_map_parse_cli_codepoint_replacement() {
    use rustty_vt::formatter::Replacement;
    let mut list = RepeatableClipboardCodepointMap::default();
    // try list.parseCLI(alloc, "U+2500=U+002D"); // box drawing → hyphen
    list.parse_cli(Some(b"U+2500=U+002D")).unwrap();

    // try testing.expectEqual(@as(usize, 1), list.map.list.len);
    assert_eq!(1usize, list.map.list.len());
    // const entry = list.map.list.get(0);
    let entry = &list.map.list[0];
    // try testing.expectEqual([2]u21{ 0x2500, 0x2500 }, entry.range);
    assert_eq!([0x2500u32, 0x2500u32], entry.range);
    // try testing.expect(entry.replacement == .codepoint);
    assert!(matches!(entry.replacement, Replacement::Codepoint(_)));
    // try testing.expectEqual(@as(u21, 0x002D), entry.replacement.codepoint);
    assert!(matches!(entry.replacement, Replacement::Codepoint(0x002D)));
}

// PORT NOTE: no reference test — this pins the fact that the replacement
// codepoint goes through `std.fmt.parseInt(u21, value[2..], 16)` and not a
// bare hex scan: a leading sign and interior `_` separators are accepted,
// while a leading/trailing `_` and an out-of-range magnitude are not.
#[test]
fn repeatable_clipboard_codepoint_map_parse_cli_codepoint_parse_int_forms() {
    use rustty_vt::formatter::Replacement;

    for (input, want) in [
        (&b"U+0041=U++002D"[..], 0x2Du32),
        (&b"U+0041=U+00_2D"[..], 0x2D),
        (&b"U+0041=U+1__0"[..], 0x10),
        (&b"U+0041=U+-0"[..], 0),
    ] {
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(input)).unwrap_or_else(|e| {
            panic!("input {input:?} unexpectedly failed: {e:?}");
        });
        assert_eq!(1usize, list.map.list.len(), "input {input:?}");
        assert!(
            matches!(list.map.list[0].replacement, Replacement::Codepoint(cp) if cp == want),
            "input {input:?}"
        );
    }

    for input in [
        &b"U+0041=U+_002D"[..],
        &b"U+0041=U+002D_"[..],
        &b"U+0041=U+200000"[..],
        &b"U+0041=U+-1"[..],
        &b"U+0041=U++"[..],
    ] {
        let mut list = RepeatableClipboardCodepointMap::default();
        assert_eq!(
            Err(Error::InvalidValue),
            list.parse_cli(Some(input)),
            "input {input:?}"
        );
    }
}

// test "parseCLI string replacement"
#[test]
fn repeatable_clipboard_codepoint_map_parse_cli_string_replacement() {
    use rustty_vt::formatter::Replacement;
    let mut list = RepeatableClipboardCodepointMap::default();
    // try list.parseCLI(alloc, "U+03A3=SUM"); // Greek sigma → "SUM"
    list.parse_cli(Some(b"U+03A3=SUM")).unwrap();

    assert_eq!(1usize, list.map.list.len());
    let entry = &list.map.list[0];
    // try testing.expectEqual([2]u21{ 0x03A3, 0x03A3 }, entry.range);
    assert_eq!([0x03A3u32, 0x03A3u32], entry.range);
    // try testing.expect(entry.replacement == .string);
    // try testing.expectEqualStrings("SUM", entry.replacement.string);
    match &entry.replacement {
        Replacement::String(s) => assert_eq!(b"SUM".as_slice(), s.as_slice()),
        _ => panic!("expected string replacement"),
    }
}

// test "parseCLI range replacement"
#[test]
fn repeatable_clipboard_codepoint_map_parse_cli_range_replacement() {
    use rustty_vt::formatter::Replacement;
    let mut list = RepeatableClipboardCodepointMap::default();
    // try list.parseCLI(alloc, "U+2500-U+2503=|"); // box drawing range → pipe
    list.parse_cli(Some(b"U+2500-U+2503=|")).unwrap();

    assert_eq!(1usize, list.map.list.len());
    let entry = &list.map.list[0];
    // try testing.expectEqual([2]u21{ 0x2500, 0x2503 }, entry.range);
    assert_eq!([0x2500u32, 0x2503u32], entry.range);
    match &entry.replacement {
        Replacement::String(s) => assert_eq!(b"|".as_slice(), s.as_slice()),
        _ => panic!("expected string replacement"),
    }
}

// test "formatConfig codepoint"
#[test]
fn repeatable_clipboard_codepoint_map_format_config_codepoint() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableClipboardCodepointMap::default();
    list.parse_cli(Some(b"U+2500=U+002D")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = U+2500=U+002D\n", buf.written());
    assert_eq!(b"a = U+2500=U+002D\n".as_slice(), buf.written());
}

// test "formatConfig string"
#[test]
fn repeatable_clipboard_codepoint_map_format_config_string() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableClipboardCodepointMap::default();
    list.parse_cli(Some(b"U+03A3=SUM")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = U+03A3=SUM\n", buf.written());
    assert_eq!(b"a = U+03A3=SUM\n".as_slice(), buf.written());
}

// PORT NOTE: no reference test. The reference serializes each entry through
// `var buf: [1024]u8`, so `bufPrint` fails (`NoSpaceLeft` ->
// `error.OutOfMemory`) once the rendered entry passes 1024 bytes. The
// boundary is 1017 replacement bytes for the single-codepoint form
// ("U+XXXX=" is 7 bytes) and 1010 for the range form ("U+XXXX-U+XXXX=" is
// 14). A growable buffer accepted overlong entries the reference rejects.
#[test]
fn repeatable_clipboard_codepoint_map_format_config_fixed_buffer_bound() {
    // 1017 bytes: the largest single-codepoint entry that fits.
    {
        let mut input: Vec<u8> = b"U+2500=".to_vec();
        input.resize(input.len() + 1017, b'x');
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();

        let mut want: Vec<u8> = b"a = U+2500=".to_vec();
        want.resize(want.len() + 1017, b'x');
        want.push(b'\n');
        assert_eq!(want.as_slice(), buf.written());
    }

    // 1018 bytes: one past the reference's buffer.
    {
        let mut input: Vec<u8> = b"U+2500=".to_vec();
        input.resize(input.len() + 1018, b'x');
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&list));
    }

    // Range form: "U+XXXX-U+XXXX=" is 14 bytes, so 1010 fits and 1011 does not.
    {
        let mut input: Vec<u8> = b"U+2500-U+2501=".to_vec();
        input.resize(input.len() + 1010, b'x');
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        assert_eq!(1024 + 4 + 1, buf.written().len());
    }
    {
        let mut input: Vec<u8> = b"U+2500-U+2501=".to_vec();
        input.resize(input.len() + 1011, b'x');
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&list));
    }
}

// -----------------------------------------------------------------------
// RepeatableCodepointMap
// -----------------------------------------------------------------------

// test "parseCLI"
#[test]
fn repeatable_codepoint_map_parse_cli() {
    // var list: Self = .{};
    let mut list = RepeatableCodepointMap::default();
    // try list.parseCLI(alloc, "U+ABCD=Comic Sans");
    list.parse_cli(Some(b"U+ABCD=Comic Sans")).unwrap();
    // try list.parseCLI(alloc, "U+0001 - U+0005=Verdana");
    list.parse_cli(Some(b"U+0001 - U+0005=Verdana")).unwrap();
    // try list.parseCLI(alloc, "U+0006-U+0009, U+ABCD=Courier");
    list.parse_cli(Some(b"U+0006-U+0009, U+ABCD=Courier"))
        .unwrap();

    // try testing.expectEqual(@as(usize, 4), list.map.list.len);
    assert_eq!(4usize, list.map.entries().len());
    {
        // const entry = list.map.list.get(0);
        let entry = &list.map.entries()[0];
        // try testing.expectEqual([2]u21{ 0xABCD, 0xABCD }, entry.range);
        assert_eq!([0xABCDu32, 0xABCDu32], entry.range);
        // try testing.expectEqualStrings("Comic Sans", entry.descriptor.family.?);
        assert_eq!(
            b"Comic Sans".as_slice(),
            entry.descriptor.family.as_deref().unwrap()
        );
    }
    {
        // const entry = list.map.list.get(1);
        let entry = &list.map.entries()[1];
        // try testing.expectEqual([2]u21{ 1, 5 }, entry.range);
        assert_eq!([1u32, 5u32], entry.range);
        // try testing.expectEqualStrings("Verdana", entry.descriptor.family.?);
        assert_eq!(
            b"Verdana".as_slice(),
            entry.descriptor.family.as_deref().unwrap()
        );
    }
    {
        // const entry = list.map.list.get(2);
        let entry = &list.map.entries()[2];
        // try testing.expectEqual([2]u21{ 6, 9 }, entry.range);
        assert_eq!([6u32, 9u32], entry.range);
        // try testing.expectEqualStrings("Courier", entry.descriptor.family.?);
        assert_eq!(
            b"Courier".as_slice(),
            entry.descriptor.family.as_deref().unwrap()
        );
    }
    {
        // const entry = list.map.list.get(3);
        let entry = &list.map.entries()[3];
        // try testing.expectEqual([2]u21{ 0xABCD, 0xABCD }, entry.range);
        assert_eq!([0xABCDu32, 0xABCDu32], entry.range);
        // try testing.expectEqualStrings("Courier", entry.descriptor.family.?);
        assert_eq!(
            b"Courier".as_slice(),
            entry.descriptor.family.as_deref().unwrap()
        );
    }
}

// test "formatConfig single"
#[test]
fn repeatable_codepoint_map_format_config_single() {
    // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
    let mut buf = AllocatingWriter::init();
    // var list: Self = .{};
    let mut list = RepeatableCodepointMap::default();
    // try list.parseCLI(alloc, "U+ABCD=Comic Sans");
    list.parse_cli(Some(b"U+ABCD=Comic Sans")).unwrap();
    // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = U+ABCD=Comic Sans\n", buf.written());
    assert_eq!(b"a = U+ABCD=Comic Sans\n".as_slice(), buf.written());
}

// test "formatConfig range"
#[test]
fn repeatable_codepoint_map_format_config_range() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableCodepointMap::default();
    // try list.parseCLI(alloc, "U+0001 - U+0005=Verdana");
    list.parse_cli(Some(b"U+0001 - U+0005=Verdana")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = U+0001-U+0005=Verdana\n", buf.written());
    assert_eq!(b"a = U+0001-U+0005=Verdana\n".as_slice(), buf.written());
}

// PORT NOTE: no reference test. The reference serializes each entry through
// `var buf: [1024]u8`, so `bufPrint` fails (`NoSpaceLeft` ->
// `error.OutOfMemory`) once the rendered entry passes 1024 bytes. The
// boundary is 1017 family bytes for the single-codepoint form ("U+XXXX=" is
// 7 bytes) and 1010 for the range form ("U+XXXX-U+XXXX=" is 14). A growable
// buffer accepted overlong entries the reference rejects (parity S76-F3).
#[test]
fn repeatable_codepoint_map_format_config_fixed_buffer_bound() {
    // 1017 bytes: the largest single-codepoint entry that fits.
    {
        let mut input: Vec<u8> = b"U+2500=".to_vec();
        input.resize(input.len() + 1017, b'x');
        let mut list = RepeatableCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();

        let mut want: Vec<u8> = b"a = U+2500=".to_vec();
        want.resize(want.len() + 1017, b'x');
        want.push(b'\n');
        assert_eq!(want.as_slice(), buf.written());
    }

    // 1018 bytes: one past the reference's buffer.
    {
        let mut input: Vec<u8> = b"U+2500=".to_vec();
        input.resize(input.len() + 1018, b'x');
        let mut list = RepeatableCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&list));
    }

    // Range form: "U+XXXX-U+XXXX=" is 14 bytes, so 1010 fits and 1011 does not.
    {
        let mut input: Vec<u8> = b"U+2500-U+2501=".to_vec();
        input.resize(input.len() + 1010, b'x');
        let mut list = RepeatableCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        assert_eq!(1024 + 4 + 1, buf.written().len());
    }
    {
        let mut input: Vec<u8> = b"U+2500-U+2501=".to_vec();
        input.resize(input.len() + 1011, b'x');
        let mut list = RepeatableCodepointMap::default();
        list.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"a", &mut buf);
        assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&list));
    }
}

// test "formatConfig multiple"
#[test]
fn repeatable_codepoint_map_format_config_multiple() {
    let mut buf = AllocatingWriter::init();
    let mut list = RepeatableCodepointMap::default();
    // try list.parseCLI(alloc, "U+0006-U+0009, U+ABCD=Courier");
    list.parse_cli(Some(b"U+0006-U+0009, U+ABCD=Courier"))
        .unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8,
    //     \\a = U+0006-U+0009=Courier
    //     \\a = U+ABCD=Courier
    //     \\
    // , buf.written());
    assert_eq!(
        b"a = U+0006-U+0009=Courier\na = U+ABCD=Courier\n".as_slice(),
        buf.written()
    );
}

// PORT NOTE (no reference test): locks the DELIBERATE, documented divergence
// recorded on `RepeatableCodepointMap::equal` (see its PORT NOTE and
// .port/phase-2/DEFERRALS.md) — the reference's `std.meta.eql` compares the
// entry's `descriptor.family` SLICE by pointer+len identity, so a Ghostty
// clone (which `dupeZ`s the family) compares UNEQUAL to its source; the port
// compares `family` by CONTENT, which is the equality `Config.changed`/
// `deepEqual` actually wants. This test fails the moment that content
// compare silently drifts back toward identity semantics.
#[test]
fn repeatable_codepoint_map_clone_equal() {
    let mut list = RepeatableCodepointMap::default();
    list.parse_cli(Some(b"U+0041,U+0043-U+0045=Cascadia Code"))
        .unwrap();

    // A clone re-allocates every `family`; content equality still holds
    // (the reference's pointer identity would report `false` here).
    let cloned = list.clone_in();
    assert!(list.equal(&cloned));
    assert!(cloned.equal(&list));

    // Same for an independently re-parsed (reloaded) map.
    let mut reparsed = RepeatableCodepointMap::default();
    reparsed
        .parse_cli(Some(b"U+0041,U+0043-U+0045=Cascadia Code"))
        .unwrap();
    assert!(list.equal(&reparsed));

    // Different family bytes -> unequal.
    let mut other_family = RepeatableCodepointMap::default();
    other_family
        .parse_cli(Some(b"U+0041,U+0043-U+0045=Comic Sans"))
        .unwrap();
    assert!(!list.equal(&other_family));

    // Different range -> unequal.
    let mut other_range = RepeatableCodepointMap::default();
    other_range
        .parse_cli(Some(b"U+0041,U+0043-U+0046=Cascadia Code"))
        .unwrap();
    assert!(!list.equal(&other_range));

    // Different entry count -> unequal.
    let mut other_len = RepeatableCodepointMap::default();
    other_len.parse_cli(Some(b"U+0041=Cascadia Code")).unwrap();
    assert!(!list.equal(&other_len));
    assert!(!other_len.equal(&list));
}
