//! PORTS: ghostty src/config/Config.zig — the `Limit`, `Color`, `TerminalColor`,
//! `BoldColor`, `ColorList` and `Palette` tests.

use super::*;

// -----------------------------------------------------------------------
// Limit
// -----------------------------------------------------------------------

// test "Limit default and parsing"
#[test]
fn limit_default_and_parsing() {
    // const TestLimit = Limit(usize, 42);
    type TestLimit = Limit<usize, 42>;

    // try testing.expectEqual(@as(usize, 42), TestLimit.default.value);
    assert_eq!(42usize, TestLimit::default_value().value);
    // try testing.expectEqual(@as(usize, 123), (try TestLimit.parseCLI("123")).value);
    assert_eq!(123usize, TestLimit::parse_cli(Some(b"123")).unwrap().value);
    // try testing.expectEqual(std.math.maxInt(usize), (try TestLimit.parseCLI("unlimited")).value);
    assert_eq!(
        usize::MAX,
        TestLimit::parse_cli(Some(b"unlimited")).unwrap().value
    );
    // try testing.expectError(error.InvalidValue, TestLimit.parseCLI("invalid"));
    assert_eq!(
        Err(Error::InvalidValue),
        TestLimit::parse_cli(Some(b"invalid"))
    );
}

// No reference test. LEDGER S78-F2: `parseCLI` is
// `std.fmt.parseInt(T, input, 0)`, which skips interior `_` separators,
// takes `0x`/`0o`/`0b` (either case), a leading `+`, and `-0` for an
// unsigned `T`. Expected values MEASURED on zig 0.16.0
// (`std.fmt.parseInt(usize, s, 0)` / `(u8, s, 0)`).
#[test]
fn limit_parse_cli_accepts_the_zig_parse_int_grammar() {
    type TestLimit = Limit<usize, 42>;
    let ok: &[(&[u8], usize)] = &[
        (b"50_000_000", 50_000_000),
        (b"0x1_0", 16),
        (b"0X10", 16),
        (b"0b1_0", 2),
        (b"0o7_7", 63),
        // Uppercase `0B` / `0O` (MEASURED on zig 0.16.0: 2 and 15).
        (b"0B10", 2),
        (b"0O17", 15),
        (b"0B1_0", 2),
        (b"0O7_7", 63),
        (b"1__0", 10),
        (b"+5", 5),
        (b"-0", 0),
        (b"18446744073709551615", usize::MAX),
    ];
    for &(input, want) in ok {
        assert_eq!(
            Ok(want),
            TestLimit::parse_cli(Some(input)).map(|l| l.value),
            "{:?}",
            String::from_utf8_lossy(input)
        );
    }
    let bad: &[&[u8]] = &[
        b"_1",
        b"1_",
        b"0x",
        b"0x_10",
        b"-1",
        b"18446744073709551616",
        b"1 ",
        b"",
    ];
    for &input in bad {
        assert_eq!(
            Err(Error::InvalidValue),
            TestLimit::parse_cli(Some(input)).map(|l| l.value),
            "{:?}",
            String::from_utf8_lossy(input)
        );
    }
    // A narrow `T` overflows exactly where Zig's does.
    assert_eq!(
        Ok(255),
        Limit::<u8, 1>::parse_cli(Some(b"0xf_f")).map(|l| l.value)
    );
    assert_eq!(
        Err(Error::InvalidValue),
        Limit::<u8, 1>::parse_cli(Some(b"256")).map(|l| l.value)
    );
}

// test "Limit optional"
#[test]
fn limit_optional() {
    // const TestLimit = Limit(u16, 42);
    type TestLimit = Limit<u16, 42>;

    // try testing.expectEqual(@as(?u16, 42), (TestLimit{}).optional());
    assert_eq!(Some(42u16), TestLimit::default_value().optional());
    // try testing.expectEqual(@as(?u16, null), (TestLimit{ .value = std.math.maxInt(u16) }).optional());
    assert_eq!(None, (TestLimit { value: u16::MAX }).optional());
}

// test "Limit formatting"
#[test]
fn limit_formatting() {
    type TestLimit = Limit<usize, 42>;

    {
        let mut buf = AllocatingWriter::init();
        // try (TestLimit{ .value = 123 }).formatEntry(formatterpkg.entryFormatter("limit", &buf.writer));
        let mut f = entry_formatter(b"limit", &mut buf);
        f.format_entry(&TestLimit { value: 123 }).unwrap();
        // try testing.expectEqualStrings("limit = 123\n", buf.written());
        assert_eq!(b"limit = 123\n".as_slice(), buf.written());
    }

    {
        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"limit", &mut buf);
        f.format_entry(&TestLimit { value: usize::MAX }).unwrap();
        // try testing.expectEqualStrings("limit = unlimited\n", buf.written());
        assert_eq!(b"limit = unlimited\n".as_slice(), buf.written());
    }
}

// -----------------------------------------------------------------------
// Color
// -----------------------------------------------------------------------

// REGRESSION (fuzz finding, `cargo fuzz run fuzz_osc` in rustty-vt,
// 2026-08-11): `parse_cli` hands its raw `[]const u8` straight to
// `RGB::parse`, which slices it by BYTE index exactly like the reference's
// `parse(value: []const u8)`. A config/CLI color value holding a multi-byte
// scalar (or raw invalid UTF-8) must be a plain `InvalidValue`, never a
// char-boundary panic.
#[test]
fn color_parse_cli_non_ascii_is_invalid_value_not_a_panic() {
    // 6 BYTES, so `RGB::parse` takes its 6-byte arm and cuts inside 'é'.
    assert!(Color::parse_cli(Some("aébcd".as_bytes())).is_err());
    assert!(Color::parse_cli(Some("#aébcd".as_bytes())).is_err());
    assert!(Color::parse_cli(Some("rgb:é/0/0".as_bytes())).is_err());
    // Raw invalid UTF-8 (never a valid spec in any form).
    assert!(Color::parse_cli(Some(b"U\x8evv")).is_err());
    assert!(Color::parse_cli(Some(b"\xff\xfe\xfd")).is_err());
}

// test "parseCLI hex" (was "fromHex" — oracle 2970e9a2a deleted fromHex;
// parseCLI now covers the same hex forms via RGB::parse delegation)
#[test]
fn color_parse_cli_hex() {
    // try testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.parseCLI("#000000"));
    assert_eq!(
        Color { r: 0, g: 0, b: 0 },
        Color::parse_cli(Some(b"#000000")).unwrap()
    );
    // try testing.expectEqual(Color{ .r = 10, .g = 11, .b = 12 }, try Color.parseCLI("#0A0B0C"));
    assert_eq!(
        Color {
            r: 10,
            g: 11,
            b: 12
        },
        Color::parse_cli(Some(b"#0A0B0C")).unwrap()
    );
    // try testing.expectEqual(Color{ .r = 10, .g = 11, .b = 12 }, try Color.parseCLI("0A0B0C"));
    assert_eq!(
        Color {
            r: 10,
            g: 11,
            b: 12
        },
        Color::parse_cli(Some(b"0A0B0C")).unwrap()
    );
    // try testing.expectEqual(Color{ .r = 255, .g = 255, .b = 255 }, try Color.parseCLI("FFFFFF"));
    assert_eq!(
        Color {
            r: 255,
            g: 255,
            b: 255
        },
        Color::parse_cli(Some(b"FFFFFF")).unwrap()
    );
    // try testing.expectEqual(Color{ .r = 255, .g = 255, .b = 255 }, try Color.parseCLI("FFF"));
    assert_eq!(
        Color {
            r: 255,
            g: 255,
            b: 255
        },
        Color::parse_cli(Some(b"FFF")).unwrap()
    );
    // try testing.expectEqual(Color{ .r = 51, .g = 68, .b = 85 }, try Color.parseCLI("#345"));
    assert_eq!(
        Color {
            r: 51,
            g: 68,
            b: 85
        },
        Color::parse_cli(Some(b"#345")).unwrap()
    );
}

// color/palette parsing now delegates to the shared rustty-vt helpers
// (oracle 2970e9a2a Config.zig hunk); "rgb:R/G/B" is not accepted by the
// old config-local fromHex/x11-map-only parseCLI, but IS accepted by
// rustty_vt::color::RGB::parse — this asserts the delegation landed.
#[test]
fn color_config_accepts_bare_hex_and_prefixed_palette_index() {
    assert_eq!(
        Color {
            r: 10,
            g: 11,
            b: 12
        },
        Color::parse_cli(Some(b"0A0B0C")).unwrap()
    );
    assert_eq!(
        Color {
            r: 17,
            g: 34,
            b: 51
        },
        Color::parse_cli(Some(b"rgb:1/2/3")).unwrap()
    );

    let mut p = Palette::default();
    p.parse_cli(Some(b"0x10=#282c34")).unwrap();
    assert_eq!(p.value[16].r, 0x28);
    assert_eq!(p.value[16].g, 0x2c);
    assert_eq!(p.value[16].b, 0x34);
    assert!(p.mask.is_set(16));
}

// test "parseCLI from name"
#[test]
fn color_parse_cli_from_name() {
    // try std.testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.parseCLI("black"));
    assert_eq!(
        Color { r: 0, g: 0, b: 0 },
        Color::parse_cli(Some(b"black")).unwrap()
    );
}

// test "formatConfig"
#[test]
fn color_format_config() {
    // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
    let mut buf = AllocatingWriter::init();
    // var color: Color = .{ .r = 10, .g = 11, .b = 12 };
    let color = Color {
        r: 10,
        g: 11,
        b: 12,
    };
    // try color.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&color).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = #0a0b0c\n", buf.written());
    assert_eq!(b"a = #0a0b0c\n".as_slice(), buf.written());
}

// test "parseCLI with whitespace"
#[test]
fn color_parse_cli_with_whitespace() {
    // try testing.expectEqual(Color{ .r = 0xAA, .g = 0xBB, .b = 0xCC }, try Color.parseCLI(" #AABBCC   "));
    assert_eq!(
        Color {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC
        },
        Color::parse_cli(Some(b" #AABBCC   ")).unwrap()
    );
    // try testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.parseCLI("  black "));
    assert_eq!(
        Color { r: 0, g: 0, b: 0 },
        Color::parse_cli(Some(b"  black ")).unwrap()
    );
}

// -----------------------------------------------------------------------
// TerminalColor
// -----------------------------------------------------------------------

// test "parseCLI" (TerminalColor)
#[test]
fn terminal_color_parse_cli() {
    // try testing.expectEqual(TerminalColor{ .color = Color{ .r = 78, .g = 42, .b = 132 } }, try TerminalColor.parseCLI("#4e2a84"));
    assert_eq!(
        TerminalColor::Color(Color {
            r: 78,
            g: 42,
            b: 132
        }),
        TerminalColor::parse_cli(Some(b"#4e2a84")).unwrap()
    );
    // try testing.expectEqual(TerminalColor{ .color = Color{ .r = 0, .g = 0, .b = 0 } }, try TerminalColor.parseCLI("black"));
    assert_eq!(
        TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        TerminalColor::parse_cli(Some(b"black")).unwrap()
    );
    // try testing.expectEqual(TerminalColor.@"cell-foreground", try TerminalColor.parseCLI("cell-foreground"));
    assert_eq!(
        TerminalColor::CellForeground,
        TerminalColor::parse_cli(Some(b"cell-foreground")).unwrap()
    );
    // try testing.expectEqual(TerminalColor.@"cell-background", try TerminalColor.parseCLI("cell-background"));
    assert_eq!(
        TerminalColor::CellBackground,
        TerminalColor::parse_cli(Some(b"cell-background")).unwrap()
    );

    // try testing.expectError(error.InvalidValue, TerminalColor.parseCLI("a"));
    assert_eq!(
        Err(Error::InvalidValue),
        TerminalColor::parse_cli(Some(b"a"))
    );
}

// test "formatConfig" (TerminalColor)
#[test]
fn terminal_color_format_config() {
    // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
    let mut buf = AllocatingWriter::init();
    // var sc: TerminalColor = .@"cell-foreground";
    let sc = TerminalColor::CellForeground;
    // try sc.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&sc).unwrap();
    // try testing.expectEqualSlices(u8, "a = cell-foreground\n", buf.written());
    assert_eq!(b"a = cell-foreground\n".as_slice(), buf.written());
}

// -----------------------------------------------------------------------
// BoldColor
// -----------------------------------------------------------------------

// test "parseCLI" (BoldColor)
#[test]
fn bold_color_parse_cli() {
    // try testing.expectEqual(BoldColor{ .color = Color{ .r = 78, .g = 42, .b = 132 } }, try BoldColor.parseCLI("#4e2a84"));
    assert_eq!(
        BoldColor::Color(Color {
            r: 78,
            g: 42,
            b: 132
        }),
        BoldColor::parse_cli(Some(b"#4e2a84")).unwrap()
    );
    // try testing.expectEqual(BoldColor{ .color = Color{ .r = 0, .g = 0, .b = 0 } }, try BoldColor.parseCLI("black"));
    assert_eq!(
        BoldColor::Color(Color { r: 0, g: 0, b: 0 }),
        BoldColor::parse_cli(Some(b"black")).unwrap()
    );
    // try testing.expectEqual(BoldColor.bright, try BoldColor.parseCLI("bright"));
    assert_eq!(
        BoldColor::Bright,
        BoldColor::parse_cli(Some(b"bright")).unwrap()
    );

    // try testing.expectError(error.InvalidValue, BoldColor.parseCLI("a"));
    assert_eq!(Err(Error::InvalidValue), BoldColor::parse_cli(Some(b"a")));
}

// test "formatConfig" (BoldColor)
#[test]
fn bold_color_format_config() {
    let mut buf = AllocatingWriter::init();
    // var sc: BoldColor = .bright;
    let sc = BoldColor::Bright;
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&sc).unwrap();
    // try testing.expectEqualSlices(u8, "a = bright\n", buf.written());
    assert_eq!(b"a = bright\n".as_slice(), buf.written());
}

// -----------------------------------------------------------------------
// ColorList
// -----------------------------------------------------------------------

// test "parseCLI" (ColorList)
#[test]
fn color_list_parse_cli() {
    // var p: Self = .{};
    let mut p = ColorList::default();
    // try p.parseCLI(alloc, "black,white");
    p.parse_cli(Some(b"black,white")).unwrap();
    // try testing.expectEqual(2, p.colors.items.len);
    assert_eq!(2, p.colors.len());

    // Test whitespace handling
    // try p.parseCLI(alloc, "black, white"); // space after comma
    p.parse_cli(Some(b"black, white")).unwrap();
    assert_eq!(2, p.colors.len());
    // try p.parseCLI(alloc, "black , white"); // spaces around comma
    p.parse_cli(Some(b"black , white")).unwrap();
    assert_eq!(2, p.colors.len());
    // try p.parseCLI(alloc, " black , white "); // extra spaces at ends
    p.parse_cli(Some(b" black , white ")).unwrap();
    assert_eq!(2, p.colors.len());

    // Error cases
    // try testing.expectError(error.ValueRequired, p.parseCLI(alloc, null));
    assert_eq!(Err(Error::ValueRequired), p.parse_cli(None));
    // try testing.expectError(error.InvalidValue, p.parseCLI(alloc, " "));
    assert_eq!(Err(Error::InvalidValue), p.parse_cli(Some(b" ")));
}

// test "format" (ColorList)
#[test]
fn color_list_format() {
    let mut buf = AllocatingWriter::init();
    let mut p = ColorList::default();
    // try p.parseCLI(alloc, "black,white");
    p.parse_cli(Some(b"black,white")).unwrap();
    // try p.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&p).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = #000000,#ffffff\n", buf.written());
    assert_eq!(b"a = #000000,#ffffff\n".as_slice(), buf.written());
}

// test "clone" (ColorList)
#[test]
fn color_list_clone() {
    let mut source = ColorList::default();
    // try source.parseCLI(alloc, "#ff0000,#00ff00,#0000ff");
    source.parse_cli(Some(b"#ff0000,#00ff00,#0000ff")).unwrap();

    // const cloned = try source.clone(alloc);
    let cloned = source.clone_in();

    // try testing.expect(source.equal(cloned));
    assert!(source.equal(&cloned));
    // try testing.expectEqual(source.colors_c.items.len, cloned.colors_c.items.len);
    assert_eq!(source.colors_c.len(), cloned.colors_c.len());
    // for (source.colors_c.items, cloned.colors_c.items) |src_c, clone_c| { ... }
    for (src_c, clone_c) in source.colors_c.iter().zip(cloned.colors_c.iter()) {
        assert_eq!(src_c.r, clone_c.r);
        assert_eq!(src_c.g, clone_c.g);
        assert_eq!(src_c.b, clone_c.b);
    }
}

// -----------------------------------------------------------------------
// Palette
// -----------------------------------------------------------------------

// test "parseCLI" (Palette)
#[test]
fn palette_parse_cli() {
    let mut p = Palette::default();
    // try p.parseCLI("0=#AABBCC");
    p.parse_cli(Some(b"0=#AABBCC")).unwrap();
    // try testing.expect(p.value[0].r == 0xAA);
    assert!(p.value[0].r == 0xAA);
    assert!(p.value[0].g == 0xBB);
    assert!(p.value[0].b == 0xCC);
    // try testing.expect(p.mask.isSet(0));
    assert!(p.mask.is_set(0));
    // try testing.expect(!p.mask.isSet(1));
    assert!(!p.mask.is_set(1));
}

// test "parseCLI base" (Palette)
#[test]
fn palette_parse_cli_base() {
    let mut p = Palette::default();

    // try p.parseCLI("0b1=#014589");
    p.parse_cli(Some(b"0b1=#014589")).unwrap();
    // try p.parseCLI("0o7=#234567");
    p.parse_cli(Some(b"0o7=#234567")).unwrap();
    // try p.parseCLI("0xF=#ABCDEF");
    p.parse_cli(Some(b"0xF=#ABCDEF")).unwrap();

    assert!(p.value[0b1].r == 0x01);
    assert!(p.value[0b1].g == 0x45);
    assert!(p.value[0b1].b == 0x89);

    assert!(p.value[0o7].r == 0x23);
    assert!(p.value[0o7].g == 0x45);
    assert!(p.value[0o7].b == 0x67);

    assert!(p.value[0xF].r == 0xAB);
    assert!(p.value[0xF].g == 0xCD);
    assert!(p.value[0xF].b == 0xEF);

    assert!(p.mask.is_set(0b1));
    assert!(p.mask.is_set(0o7));
    assert!(p.mask.is_set(0xF));
    assert!(!p.mask.is_set(0));
    assert!(!p.mask.is_set(2));
}

// test "parseCLI overflow" (Palette)
#[test]
fn palette_parse_cli_overflow() {
    let mut p = Palette::default();
    // try testing.expectError(error.Overflow, p.parseCLI("256=#AABBCC"));
    assert_eq!(
        Err(PaletteParseError::Overflow),
        p.parse_cli(Some(b"256=#AABBCC"))
    );
    // Mask should remain empty since parsing failed.
    // try testing.expectEqual(@as(usize, 0), p.mask.count());
    assert_eq!(0usize, p.mask.count());
}

// test "formatConfig" (Palette)
#[test]
fn palette_format_config() {
    let mut buf = AllocatingWriter::init();
    let list = Palette::default();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&list).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = 0=#1d1f21\n", buf.written()[0..14]);
    assert_eq!(b"a = 0=#1d1f21\n".as_slice(), &buf.written()[0..14]);
}

// test "parseCLI with whitespace" (Palette)
#[test]
fn palette_parse_cli_with_whitespace() {
    let mut p = Palette::default();
    // try p.parseCLI("0 =  #AABBCC");
    p.parse_cli(Some(b"0 =  #AABBCC")).unwrap();
    // try p.parseCLI(" 1= #DDEEFF    ");
    p.parse_cli(Some(b" 1= #DDEEFF    ")).unwrap();
    // try p.parseCLI("  2  =  #123456 ");
    p.parse_cli(Some(b"  2  =  #123456 ")).unwrap();

    assert!(p.value[0].r == 0xAA);
    assert!(p.value[0].g == 0xBB);
    assert!(p.value[0].b == 0xCC);

    assert!(p.value[1].r == 0xDD);
    assert!(p.value[1].g == 0xEE);
    assert!(p.value[1].b == 0xFF);

    assert!(p.value[2].r == 0x12);
    assert!(p.value[2].g == 0x34);
    assert!(p.value[2].b == 0x56);

    assert!(p.mask.is_set(0));
    assert!(p.mask.is_set(1));
    assert!(p.mask.is_set(2));
    assert!(!p.mask.is_set(3));
}
