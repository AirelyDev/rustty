//! PORTS: ghostty src/config/Config.zig — the `WorkingDirectory`, `FontStyle`,
//! `MouseScrollMultiplier`, `QuickTerminalSize`, `AlphaBlending`,
//! `BackgroundBlur`, `WindowDecoration`, `Theme`, `Duration` and
//! `WindowPadding` tests.

use super::*;

// -----------------------------------------------------------------------
// WorkingDirectory
// -----------------------------------------------------------------------

// test "WorkingDirectory parseCLI"
#[test]
fn working_directory_parse_cli() {
    // var wd: Self = .inherit;
    let mut wd = WorkingDirectory::Inherit;

    // try wd.parseCLI(alloc, "inherit");
    wd.parse_cli(Some(b"inherit")).unwrap();
    // try testing.expectEqual(.inherit, wd);
    assert_eq!(WorkingDirectory::Inherit, wd);

    // try wd.parseCLI(alloc, "home");
    wd.parse_cli(Some(b"home")).unwrap();
    // try testing.expectEqual(.home, wd);
    assert_eq!(WorkingDirectory::Home, wd);

    // try wd.parseCLI(alloc, "~/projects/ghostty");
    wd.parse_cli(Some(b"~/projects/ghostty")).unwrap();
    // try testing.expectEqualStrings("~/projects/ghostty", wd.path);
    assert_eq!(Some(b"~/projects/ghostty".as_slice()), wd.value());

    // try wd.parseCLI(alloc, "\"/tmp path\"");
    wd.parse_cli(Some(b"\"/tmp path\"")).unwrap();
    // try testing.expectEqualStrings("/tmp path", wd.path);
    assert_eq!(Some(b"/tmp path".as_slice()), wd.value());
}

// test "WorkingDirectory finalize"
#[test]
fn working_directory_finalize() {
    // var wd: Self = .{ .path = "~/projects/ghostty" };
    let mut wd = WorkingDirectory::Path(b"~/projects/ghostty".to_vec());
    // try wd.finalize(alloc);
    wd.finalize();

    // var buf: [std.fs.max_path_bytes]u8 = undefined;
    // const expected = internal_os.expandHome("~/projects/ghostty", &buf) catch "~/projects/ghostty";
    let mut buf = [0u8; MAX_PATH_BYTES];
    let expected: Vec<u8> = match rustty_os::homedir::expandHome(
        &rustty_global::environ_map(),
        b"~/projects/ghostty",
        &mut buf,
    ) {
        Ok(v) => v.to_vec(),
        Err(_) => b"~/projects/ghostty".to_vec(),
    };
    // try testing.expectEqualStrings(expected, wd.value().?);
    assert_eq!(expected.as_slice(), wd.value().unwrap());
}

// -----------------------------------------------------------------------
// FontStyle
// -----------------------------------------------------------------------

// test "parseCLI" (FontStyle)
#[test]
fn font_style_parse_cli() {
    // var p: Self = .{ .default = {} };
    let mut p = FontStyle::Default;
    // try p.parseCLI(alloc, "default");
    p.parse_cli(Some(b"default")).unwrap();
    // try testing.expectEqual(Self{ .default = {} }, p);
    assert_eq!(FontStyle::Default, p);

    // try p.parseCLI(alloc, "false");
    p.parse_cli(Some(b"false")).unwrap();
    // try testing.expectEqual(Self{ .false = {} }, p);
    assert_eq!(FontStyle::False, p);

    // try p.parseCLI(alloc, "bold");
    p.parse_cli(Some(b"bold")).unwrap();
    // try testing.expectEqualStrings("bold", p.name);
    assert_eq!(Some(b"bold".as_slice()), p.name_value());
}

// test "formatConfig default" (FontStyle)
#[test]
fn font_style_format_config_default() {
    let mut buf = AllocatingWriter::init();
    let mut p = FontStyle::Default;
    p.parse_cli(Some(b"default")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&p).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = default\n", buf.written());
    assert_eq!(b"a = default\n".as_slice(), buf.written());
}

// test "formatConfig false" (FontStyle)
#[test]
fn font_style_format_config_false() {
    let mut buf = AllocatingWriter::init();
    let mut p = FontStyle::Default;
    p.parse_cli(Some(b"false")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&p).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = false\n", buf.written());
    assert_eq!(b"a = false\n".as_slice(), buf.written());
}

// test "formatConfig named" (FontStyle)
#[test]
fn font_style_format_config_named() {
    let mut buf = AllocatingWriter::init();
    let mut p = FontStyle::Default;
    p.parse_cli(Some(b"bold")).unwrap();
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&p).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = bold\n", buf.written());
    assert_eq!(b"a = bold\n".as_slice(), buf.written());
}

// -----------------------------------------------------------------------
// MouseScrollMultiplier
// -----------------------------------------------------------------------

// test "parse" (MouseScrollMultiplier)
#[test]
fn mouse_scroll_multiplier_parse() {
    // const epsilon = 0.00001;
    let epsilon = 0.00001;

    // var args: Self = .{ .precision = 0.1, .discrete = 3 };
    let mut args = MouseScrollMultiplier {
        precision: 0.1,
        discrete: 3.0,
    };
    // try args.parseCLI(alloc, "3");
    args.parse_cli(Some(b"3")).unwrap();
    // try testing.expectApproxEqAbs(3, args.precision, epsilon);
    assert!((3.0 - args.precision).abs() <= epsilon);
    assert!((3.0 - args.discrete).abs() <= epsilon);

    args = MouseScrollMultiplier {
        precision: 0.1,
        discrete: 3.0,
    };
    // try args.parseCLI(alloc, "precision:1");
    args.parse_cli(Some(b"precision:1")).unwrap();
    assert!((1.0 - args.precision).abs() <= epsilon);
    assert!((3.0 - args.discrete).abs() <= epsilon);

    args = MouseScrollMultiplier {
        precision: 0.1,
        discrete: 3.0,
    };
    // try args.parseCLI(alloc, "discrete:5");
    args.parse_cli(Some(b"discrete:5")).unwrap();
    assert!((0.1 - args.precision).abs() <= epsilon);
    assert!((5.0 - args.discrete).abs() <= epsilon);

    args = MouseScrollMultiplier {
        precision: 0.1,
        discrete: 3.0,
    };
    // try args.parseCLI(alloc, "precision:3,discrete:7");
    args.parse_cli(Some(b"precision:3,discrete:7")).unwrap();
    assert!((3.0 - args.precision).abs() <= epsilon);
    assert!((7.0 - args.discrete).abs() <= epsilon);

    args = MouseScrollMultiplier {
        precision: 0.1,
        discrete: 3.0,
    };
    // try args.parseCLI(alloc, "discrete:8,precision:6");
    args.parse_cli(Some(b"discrete:8,precision:6")).unwrap();
    assert!((6.0 - args.precision).abs() <= epsilon);
    assert!((8.0 - args.discrete).abs() <= epsilon);

    // args = .default;
    args = MouseScrollMultiplier::default();
    // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "foo:1"));
    assert_eq!(Err(Error::InvalidValue), args.parse_cli(Some(b"foo:1")));
    // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "precision:bar"));
    assert_eq!(
        Err(Error::InvalidValue),
        args.parse_cli(Some(b"precision:bar"))
    );
    // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "precision:1,discrete:3,foo:5"));
    assert_eq!(
        Err(Error::InvalidValue),
        args.parse_cli(Some(b"precision:1,discrete:3,foo:5"))
    );
    // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "precision:1,,discrete:3"));
    assert_eq!(
        Err(Error::InvalidValue),
        args.parse_cli(Some(b"precision:1,,discrete:3"))
    );
    // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, ",precision:1,discrete:3"));
    assert_eq!(
        Err(Error::InvalidValue),
        args.parse_cli(Some(b",precision:1,discrete:3"))
    );
}

// test "format entry MouseScrollMultiplier"
#[test]
fn format_entry_mouse_scroll_multiplier() {
    let mut buf = AllocatingWriter::init();
    // var args: Self = .{ .precision = 1.5, .discrete = 2.5 };
    let args = MouseScrollMultiplier {
        precision: 1.5,
        discrete: 2.5,
    };
    let mut f = entry_formatter(b"mouse-scroll-multiplier", &mut buf);
    f.format_entry(&args).unwrap();
    // try testing.expectEqualSlices(u8, "mouse-scroll-multiplier = precision:1.5,discrete:2.5\n", buf.written());
    assert_eq!(
        b"mouse-scroll-multiplier = precision:1.5,discrete:2.5\n".as_slice(),
        buf.written()
    );
}

// -----------------------------------------------------------------------
// QuickTerminalSize
// -----------------------------------------------------------------------

// test "parse QuickTerminalSize"
#[test]
fn parse_quick_terminal_size() {
    // var v: QuickTerminalSize = undefined;
    let mut v = QuickTerminalSize::default();

    // try v.parseCLI("50%");
    v.parse_cli(Some(b"50%")).unwrap();
    // try testing.expectEqual(50, v.primary.?.percentage);
    assert_eq!(Some(QuickTerminalSizeSize::Percentage(50.0)), v.primary);
    // try testing.expectEqual(null, v.secondary);
    assert_eq!(None, v.secondary);

    // try v.parseCLI("200px");
    v.parse_cli(Some(b"200px")).unwrap();
    // try testing.expectEqual(200, v.primary.?.pixels);
    assert_eq!(Some(QuickTerminalSizeSize::Pixels(200)), v.primary);
    assert_eq!(None, v.secondary);

    // try v.parseCLI("50%,200px");
    v.parse_cli(Some(b"50%,200px")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Percentage(50.0)), v.primary);
    assert_eq!(Some(QuickTerminalSizeSize::Pixels(200)), v.secondary);

    // try testing.expectError(error.ValueRequired, v.parseCLI(null));
    assert_eq!(
        Err(QuickTerminalSizeParseError::ValueRequired),
        v.parse_cli(None)
    );
    // try testing.expectError(error.ValueRequired, v.parseCLI(""));
    assert_eq!(
        Err(QuickTerminalSizeParseError::ValueRequired),
        v.parse_cli(Some(b""))
    );
    // try testing.expectError(error.ValueRequired, v.parseCLI("69px,"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::ValueRequired),
        v.parse_cli(Some(b"69px,"))
    );
    // try testing.expectError(error.TooManyArguments, v.parseCLI("69px,42%,69px"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::TooManyArguments),
        v.parse_cli(Some(b"69px,42%,69px"))
    );

    // try testing.expectError(error.MissingUnit, v.parseCLI("420"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::MissingUnit),
        v.parse_cli(Some(b"420"))
    );
    // try testing.expectError(error.MissingUnit, v.parseCLI("bobr"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::MissingUnit),
        v.parse_cli(Some(b"bobr"))
    );
    // try testing.expectError(error.InvalidValue, v.parseCLI("bobr%"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::InvalidValue),
        v.parse_cli(Some(b"bobr%"))
    );
    // try testing.expectError(error.InvalidValue, v.parseCLI("-32%"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::InvalidValue),
        v.parse_cli(Some(b"-32%"))
    );
    // try testing.expectError(error.InvalidValue, v.parseCLI("-69px"));
    assert_eq!(
        Err(QuickTerminalSizeParseError::InvalidValue),
        v.parse_cli(Some(b"-69px"))
    );
}

// PORT NOTE (no reference test): `Size.parse` routes "px" through
// `std.fmt.parseInt(u32, ..., 10)` and "%" through `std.fmt.parseFloat(f32, ..)`.
// Both accept a leading `+` and `_` digit separators; `parseInt` additionally
// accepts DOUBLED separators while `parseFloat`'s `validUnderscores` requires a
// `_` to sit between two digits. Oracle-probed on Zig 0.16 (see the parser
// ports): parseInt "1__0"=>10 but parseFloat "1__0"=>InvalidCharacter.
#[test]
fn parse_quick_terminal_size_zig_number_spellings() {
    let mut v = QuickTerminalSize::default();

    // parseInt spellings on the pixel branch.
    v.parse_cli(Some(b"+69px")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Pixels(69)), v.primary);
    v.parse_cli(Some(b"1_0px")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Pixels(10)), v.primary);
    v.parse_cli(Some(b"1__0px")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Pixels(10)), v.primary);

    // parseFloat spellings on the percentage branch.
    v.parse_cli(Some(b"+10%")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Percentage(10.0)), v.primary);
    v.parse_cli(Some(b"1_0%")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Percentage(10.0)), v.primary);
    v.parse_cli(Some(b"1_0.5%")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Percentage(10.5)), v.primary);
    v.parse_cli(Some(b"1.5_5%")).unwrap();
    assert_eq!(Some(QuickTerminalSizeSize::Percentage(1.55)), v.primary);

    // Rejections: separator at either end, or without two digit neighbours.
    for bad in [
        b"_10px".as_slice(),
        b"10_px".as_slice(),
        b"_10%".as_slice(),
        b"10_%".as_slice(),
        // parseFloat rejects doubled separators (parseInt does not).
        b"1__0%".as_slice(),
        b"1_.5%".as_slice(),
        b"+%".as_slice(),
        b"+px".as_slice(),
    ] {
        assert_eq!(
            Err(QuickTerminalSizeParseError::InvalidValue),
            v.parse_cli(Some(bad)),
            "expected InvalidValue for {:?}",
            core::str::from_utf8(bad).unwrap()
        );
    }
}

// test "calculate QuickTerminalSize"
#[test]
fn calculate_quick_terminal_size() {
    // const dims_landscape: Dimensions = .{ .width = 2560, .height = 1600 };
    let dims_landscape = QuickTerminalSizeDimensions {
        width: 2560,
        height: 1600,
    };
    // const dims_portrait: Dimensions = .{ .width = 1600, .height = 2560 };
    let dims_portrait = QuickTerminalSizeDimensions {
        width: 1600,
        height: 2560,
    };

    {
        // const size: QuickTerminalSize = .{};
        let size = QuickTerminalSize::default();
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 2560,
                height: 400
            },
            size.calculate(QuickTerminalPosition::Top, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 400,
                height: 1600
            },
            size.calculate(QuickTerminalPosition::Left, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 800,
                height: 400
            },
            size.calculate(QuickTerminalPosition::Center, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 400,
                height: 800
            },
            size.calculate(QuickTerminalPosition::Center, dims_portrait)
        );
    }
    {
        // const size: QuickTerminalSize = .{ .primary = .{ .percentage = 20 } };
        let size = QuickTerminalSize {
            primary: Some(QuickTerminalSizeSize::Percentage(20.0)),
            secondary: None,
        };
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 2560,
                height: 320
            },
            size.calculate(QuickTerminalPosition::Top, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 512,
                height: 1600
            },
            size.calculate(QuickTerminalPosition::Left, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 512,
                height: 400
            },
            size.calculate(QuickTerminalPosition::Center, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 400,
                height: 512
            },
            size.calculate(QuickTerminalPosition::Center, dims_portrait)
        );
    }
    {
        // const size: QuickTerminalSize = .{ .primary = .{ .pixels = 600 } };
        let size = QuickTerminalSize {
            primary: Some(QuickTerminalSizeSize::Pixels(600)),
            secondary: None,
        };
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 2560,
                height: 600
            },
            size.calculate(QuickTerminalPosition::Top, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 600,
                height: 1600
            },
            size.calculate(QuickTerminalPosition::Left, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 600,
                height: 400
            },
            size.calculate(QuickTerminalPosition::Center, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 400,
                height: 600
            },
            size.calculate(QuickTerminalPosition::Center, dims_portrait)
        );
    }
    {
        // const size: QuickTerminalSize = .{ .primary = .{ .percentage = 69 }, .secondary = .{ .pixels = 420 } };
        let size = QuickTerminalSize {
            primary: Some(QuickTerminalSizeSize::Percentage(69.0)),
            secondary: Some(QuickTerminalSizeSize::Pixels(420)),
        };
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 420,
                height: 1104
            },
            size.calculate(QuickTerminalPosition::Top, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 1766,
                height: 420
            },
            size.calculate(QuickTerminalPosition::Left, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 1766,
                height: 420
            },
            size.calculate(QuickTerminalPosition::Center, dims_landscape)
        );
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: 420,
                height: 1766
            },
            size.calculate(QuickTerminalPosition::Center, dims_portrait)
        );
    }
}

/// NEW-FINDINGS N123: a `quick-terminal-size` percentage is
/// `std.fmt.parseFloat(f32, ..)`, whose grammar includes hex floats and
/// `_` separators. MEASURED with Zig 0.16.0: `parseFloat(f32, "0x1p4")` is
/// 16, `"0x1.8p5"` is 48, `"1_0"` is 10.
#[test]
fn quick_terminal_size_percentage_uses_the_parse_float_grammar() {
    assert_eq!(
        Ok(QuickTerminalSizeSize::Percentage(16.0)),
        QuickTerminalSizeSize::parse(b"0x1p4%")
    );
    assert_eq!(
        Ok(QuickTerminalSizeSize::Percentage(48.0)),
        QuickTerminalSizeSize::parse(b"0x1.8p5%")
    );
    assert_eq!(
        Ok(QuickTerminalSizeSize::Percentage(10.0)),
        QuickTerminalSizeSize::parse(b"1_0%")
    );
}

/// UD96 (R13, task R13-D): `quick-terminal-size` percentages the parser
/// accepts but whose pixel size is outside `u32`. The reference's
/// `@intFromFloat(v / 100.0 * dim)` traps on `inf%` and `1e10%` (ReleaseFast:
/// 4294967295) and gives 0 for `nan%` (measured, Zig 0.16.0). The port
/// yields the reference's ReleaseFast values without trapping.
#[test]
fn quick_terminal_size_out_of_range_percentages_do_not_trap() {
    let dims = QuickTerminalSizeDimensions {
        width: 2560,
        height: 1600,
    };
    // Positive control: an ordinary percentage.
    let half = QuickTerminalSizeSize::parse(b"50%").unwrap();
    assert_eq!(800, half.to_pixels(dims.height));

    for (input, want) in [(&b"inf%"[..], u32::MAX), (b"1e10%", u32::MAX), (b"nan%", 0)] {
        let size = QuickTerminalSizeSize::parse(input).unwrap_or_else(|e| {
            panic!(
                "the parser accepts {:?}: {e:?}",
                String::from_utf8_lossy(input)
            )
        });
        assert!(
            matches!(size, QuickTerminalSizeSize::Percentage(_)),
            "precondition: {:?} is a percentage",
            String::from_utf8_lossy(input)
        );
        assert_eq!(want, size.to_pixels(dims.height));
        // And through `calculate`, as the apprt reaches it.
        let qts = QuickTerminalSize {
            primary: Some(size),
            secondary: None,
        };
        assert_eq!(
            QuickTerminalSizeDimensions {
                width: dims.width,
                height: want
            },
            qts.calculate(QuickTerminalPosition::Top, dims)
        );
    }
}

// -----------------------------------------------------------------------
// AlphaBlending
// -----------------------------------------------------------------------
// PORT NOTE: `AlphaBlending` has no `test {}` block in the reference; its
// `isLinear` is exercised indirectly. A direct smoke test of the three arms
// is added (Rust-only, not a reference assertion) to lock the mapping.
#[test]
fn alpha_blending_is_linear() {
    assert!(!AlphaBlending::Native.is_linear());
    assert!(AlphaBlending::Linear.is_linear());
    assert!(AlphaBlending::LinearCorrected.is_linear());
}

// -----------------------------------------------------------------------
// BackgroundBlur
// -----------------------------------------------------------------------

// test "parse BackgroundBlur"
#[test]
fn parse_background_blur() {
    // var v: BackgroundBlur = undefined;
    // PORT NOTE (Chunk 3a): `parseCLI` is now the config-local free fn
    // `background_blur_parse_cli` (the type is hoisted to rustty-render-types); the
    // test calls it identically. Test stays in config (Config.zig provenance).
    let mut v = BackgroundBlur::False;

    // try v.parseCLI(null);
    background_blur_parse_cli(&mut v, None).unwrap();
    // try testing.expectEqual(.true, v);
    assert_eq!(BackgroundBlur::True, v);

    // try v.parseCLI("true");
    background_blur_parse_cli(&mut v, Some(b"true")).unwrap();
    assert_eq!(BackgroundBlur::True, v);

    // try v.parseCLI("false");
    background_blur_parse_cli(&mut v, Some(b"false")).unwrap();
    assert_eq!(BackgroundBlur::False, v);

    // try v.parseCLI("42");
    background_blur_parse_cli(&mut v, Some(b"42")).unwrap();
    // try testing.expectEqual(42, v.radius);
    assert_eq!(BackgroundBlur::Radius(42), v);

    // try v.parseCLI("macos-glass-regular");
    background_blur_parse_cli(&mut v, Some(b"macos-glass-regular")).unwrap();
    assert_eq!(BackgroundBlur::MacosGlassRegular, v);

    // try v.parseCLI("macos-glass-clear");
    background_blur_parse_cli(&mut v, Some(b"macos-glass-clear")).unwrap();
    assert_eq!(BackgroundBlur::MacosGlassClear, v);

    // try testing.expectError(error.InvalidValue, v.parseCLI(""));
    assert_eq!(
        Err(Error::InvalidValue),
        background_blur_parse_cli(&mut v, Some(b""))
    );
    // try testing.expectError(error.InvalidValue, v.parseCLI("aaaa"));
    assert_eq!(
        Err(Error::InvalidValue),
        background_blur_parse_cli(&mut v, Some(b"aaaa"))
    );
    // try testing.expectError(error.InvalidValue, v.parseCLI("420"));
    assert_eq!(
        Err(Error::InvalidValue),
        background_blur_parse_cli(&mut v, Some(b"420"))
    );
}

// PORT NOTE: no reference test — this pins the `_` digit-separator rules of
// `std.fmt.parseInt(u8, input, 0)` (std/fmt.zig:439): a separator is only
// rejected as the FIRST or LAST char of the post-sign/post-prefix digit run,
// so `1__2` parses while `_12`, `12_` and `0x_10` are `error.InvalidCharacter`
// (-> `error.InvalidValue` here).
#[test]
fn parse_background_blur_underscore_separators() {
    let mut v = BackgroundBlur::False;

    for (input, want) in [
        (&b"12"[..], 12u8),
        (&b"0x10"[..], 16),
        (&b"1__2"[..], 12),
        (&b"+12"[..], 12),
        (&b"0b1_0"[..], 2),
    ] {
        background_blur_parse_cli(&mut v, Some(input)).unwrap();
        assert_eq!(BackgroundBlur::Radius(want), v, "input {input:?}");
    }

    for input in [&b"0x_10"[..], &b"_12"[..], &b"12_"[..], &b"0x10_"[..]] {
        assert_eq!(
            Err(Error::InvalidValue),
            background_blur_parse_cli(&mut v, Some(input)),
            "input {input:?}"
        );
    }
}

// -----------------------------------------------------------------------
// WindowDecoration
// -----------------------------------------------------------------------

// test "parse WindowDecoration"
#[test]
fn parse_window_decoration() {
    // const v = try WindowDecoration.parseCLI(null);
    // try testing.expectEqual(WindowDecoration.auto, v);
    assert_eq!(
        WindowDecoration::Auto,
        WindowDecoration::parse_cli(None).unwrap()
    );
    // try testing.expectEqual(WindowDecoration.auto, parseCLI("true"));
    assert_eq!(
        WindowDecoration::Auto,
        WindowDecoration::parse_cli(Some(b"true")).unwrap()
    );
    // try testing.expectEqual(WindowDecoration.none, parseCLI("false"));
    assert_eq!(
        WindowDecoration::None,
        WindowDecoration::parse_cli(Some(b"false")).unwrap()
    );
    // try testing.expectEqual(WindowDecoration.server, parseCLI("server"));
    assert_eq!(
        WindowDecoration::Server,
        WindowDecoration::parse_cli(Some(b"server")).unwrap()
    );
    // try testing.expectEqual(WindowDecoration.client, parseCLI("client"));
    assert_eq!(
        WindowDecoration::Client,
        WindowDecoration::parse_cli(Some(b"client")).unwrap()
    );
    // try testing.expectEqual(WindowDecoration.auto, parseCLI("auto"));
    assert_eq!(
        WindowDecoration::Auto,
        WindowDecoration::parse_cli(Some(b"auto")).unwrap()
    );
    // try testing.expectEqual(WindowDecoration.none, parseCLI("none"));
    assert_eq!(
        WindowDecoration::None,
        WindowDecoration::parse_cli(Some(b"none")).unwrap()
    );

    // try testing.expectError(error.InvalidValue, WindowDecoration.parseCLI(""));
    assert_eq!(
        Err(Error::InvalidValue),
        WindowDecoration::parse_cli(Some(b""))
    );
    // try testing.expectError(error.InvalidValue, WindowDecoration.parseCLI("aaaa"));
    assert_eq!(
        Err(Error::InvalidValue),
        WindowDecoration::parse_cli(Some(b"aaaa"))
    );
}

// -----------------------------------------------------------------------
// Theme
// -----------------------------------------------------------------------

// test "parse Theme"
#[test]
fn parse_theme() {
    // Single
    {
        // var v: Theme = undefined;
        let mut v = Theme::default();
        // try v.parseCLI(alloc, "foo");
        v.parse_cli(Some(b"foo")).unwrap();
        // try testing.expectEqualStrings("foo", v.light);
        assert_eq!(b"foo".as_slice(), v.light.as_slice());
        // try testing.expectEqualStrings("foo", v.dark);
        assert_eq!(b"foo".as_slice(), v.dark.as_slice());
    }

    // Single whitespace
    {
        let mut v = Theme::default();
        // try v.parseCLI(alloc, "  foo  ");
        v.parse_cli(Some(b"  foo  ")).unwrap();
        assert_eq!(b"foo".as_slice(), v.light.as_slice());
        assert_eq!(b"foo".as_slice(), v.dark.as_slice());
    }

    // Light/dark
    {
        let mut v = Theme::default();
        // try v.parseCLI(alloc, " light:foo,  dark : bar  ");
        v.parse_cli(Some(b" light:foo,  dark : bar  ")).unwrap();
        assert_eq!(b"foo".as_slice(), v.light.as_slice());
        assert_eq!(b"bar".as_slice(), v.dark.as_slice());
    }

    // Expand tilde to home
    {
        // var environ_map = try testing.environ.createMap(alloc);
        // defer environ_map.deinit();
        let environ_map = rustty_global::environ_map();

        // var home_buf: [std.fs.max_path_bytes]u8 = undefined;
        let mut home_buf = [0u8; MAX_PATH_BYTES];
        // const home = try internal_os.expandHome(
        //     testing.io,
        //     &environ_map,
        //     "~/",
        //     &home_buf,
        // );
        let home = rustty_os::homedir::expandHome(&environ_map, b"~/", &mut home_buf)
            .unwrap()
            .to_vec();

        // var v: Theme = undefined;
        let mut v = Theme::default();
        // try v.parseCLI(alloc, "light:~/foo, dark:~/bar");
        v.parse_cli(Some(b"light:~/foo, dark:~/bar")).unwrap();
        // try v.finalize(alloc);
        v.finalize();

        // var expected_buf: [std.fs.max_path_bytes]u8 = undefined;
        // try testing.expectEqualStrings(
        //     try std.fmt.bufPrint(&expected_buf, "{s}foo", .{home}),
        //     v.light,
        // );
        let mut expected = home.clone();
        expected.extend_from_slice(b"foo");
        assert_eq!(expected.as_slice(), v.light.as_slice());
        // try testing.expectEqualStrings(
        //     try std.fmt.bufPrint(&expected_buf, "{s}bar", .{home}),
        //     v.dark,
        // );
        let mut expected = home.clone();
        expected.extend_from_slice(b"bar");
        assert_eq!(expected.as_slice(), v.dark.as_slice());
    }

    let mut v = Theme::default();
    // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, null));
    assert_eq!(Err(Error::ValueRequired), v.parse_cli(None));
    // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, ""));
    assert_eq!(Err(Error::ValueRequired), v.parse_cli(Some(b"")));
    // try testing.expectError(error.InvalidValue, v.parseCLI(alloc, "light:foo"));
    assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"light:foo")));
    // try testing.expectError(error.InvalidValue, v.parseCLI(alloc, "dark:foo"));
    assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"dark:foo")));
}

// PORT NOTE: no reference test. `Theme.formatEntry` renders the light/dark
// pair through `var buf: [4096]u8`, so `bufPrint` fails (`NoSpaceLeft` ->
// `error.OutOfMemory`) once `light:<light>,dark:<dark>` passes 4096 bytes.
// `parseCLI` bounds neither component, so the case is reachable; the port
// used a growable `Vec` and always succeeded (parity S77-F1). The prefix
// `light:` (6) + `,dark:` (6) is 12 bytes, so two 2042-byte values fit
// (4096) and 2043/2042 does not (4097).
#[test]
fn theme_format_config_fixed_buffer_bound() {
    // Equal light/dark takes the early return, which uses NO buffer in the
    // reference: an arbitrarily long value formats fine.
    {
        let light = vec![b'x'; 9000];
        let v = Theme {
            dark: light.clone(),
            light,
        };
        let mut buf = AllocatingWriter::init();
        v.format_entry(b"a", &mut buf).unwrap();
        assert_eq!(4 + 9000 + 1, buf.written().len());
    }

    // Exactly 4096 bytes of rendered entry.
    {
        let v = Theme {
            light: vec![b'x'; 2042],
            dark: vec![b'y'; 2042],
        };
        let mut buf = AllocatingWriter::init();
        v.format_entry(b"a", &mut buf).unwrap();
        assert_eq!(4 + 4096 + 1, buf.written().len());
    }

    // One byte past it.
    {
        let v = Theme {
            light: vec![b'x'; 2043],
            dark: vec![b'y'; 2042],
        };
        let mut buf = AllocatingWriter::init();
        assert_eq!(
            Err(FormatError::OutOfMemory),
            v.format_entry(b"a", &mut buf),
        );
    }
}

// -----------------------------------------------------------------------
// Duration
// -----------------------------------------------------------------------

// test "parse duration"
#[test]
fn parse_duration() {
    // inline for (Duration.units) |unit| { const t = "0{s}"; const d = try Duration.parseCLI(t); expect 0 }
    for &(uname, _factor) in DURATION_UNITS {
        let mut t: Vec<u8> = Vec::new();
        t.push(b'0');
        t.extend_from_slice(uname);
        let d = Duration::parse_cli(Some(&t)).unwrap();
        assert_eq!(0u64, d.duration);
    }

    // inline for (Duration.units) |unit| { const t = "1{s}"; const d = try Duration.parseCLI(t); expect unit.factor }
    for &(uname, factor) in DURATION_UNITS {
        let mut t: Vec<u8> = Vec::new();
        t.push(b'1');
        t.extend_from_slice(uname);
        let d = Duration::parse_cli(Some(&t)).unwrap();
        assert_eq!(factor, d.duration);
    }

    // const d = try Duration.parseCLI("0"); expect 0
    assert_eq!(0u64, Duration::parse_cli(Some(b"0")).unwrap().duration);
    // const d = try Duration.parseCLI("100ns"); expect 100
    assert_eq!(
        100u64,
        Duration::parse_cli(Some(b"100ns")).unwrap().duration
    );
    // const d = try Duration.parseCLI("1µs"); expect 1000
    assert_eq!(
        1000u64,
        Duration::parse_cli(Some("1µs".as_bytes()))
            .unwrap()
            .duration
    );
    // const d = try Duration.parseCLI("1µs1ns"); expect 1001
    assert_eq!(
        1001u64,
        Duration::parse_cli(Some("1µs1ns".as_bytes()))
            .unwrap()
            .duration
    );
    // const d = try Duration.parseCLI("1µs 1ns"); expect 1001
    assert_eq!(
        1001u64,
        Duration::parse_cli(Some("1µs 1ns".as_bytes()))
            .unwrap()
            .duration
    );
    // const d = try Duration.parseCLI(" 1µs1ns"); expect 1001
    assert_eq!(
        1001u64,
        Duration::parse_cli(Some(" 1µs1ns".as_bytes()))
            .unwrap()
            .duration
    );
    // const d = try Duration.parseCLI("1µs1ns "); expect 1001
    assert_eq!(
        1001u64,
        Duration::parse_cli(Some("1µs1ns ".as_bytes()))
            .unwrap()
            .duration
    );
    // const d = try Duration.parseCLI("30s"); expect 30 * ns_per_s
    assert_eq!(
        30 * NS_PER_S,
        Duration::parse_cli(Some(b"30s")).unwrap().duration
    );
    // const d = try Duration.parseCLI("584y 49w 23h 34m 33s 709ms 551µs 615ns"); expect maxInt(u64)
    assert_eq!(
        u64::MAX,
        Duration::parse_cli(Some("584y 49w 23h 34m 33s 709ms 551µs 615ns".as_bytes()))
            .unwrap()
            .duration
    );

    // Overflow: const d = try Duration.parseCLI("600y"); expect maxInt(u64)
    assert_eq!(
        u64::MAX,
        Duration::parse_cli(Some(b"600y")).unwrap().duration
    );

    // Repeated units: const d = try Duration.parseCLI("100ns100ns"); expect 200
    assert_eq!(
        200u64,
        Duration::parse_cli(Some(b"100ns100ns")).unwrap().duration
    );

    // try expectError(error.ValueRequired, Duration.parseCLI(null));
    assert_eq!(Err(Error::ValueRequired), Duration::parse_cli(None));
    // try expectError(error.ValueRequired, Duration.parseCLI(""));
    assert_eq!(Err(Error::ValueRequired), Duration::parse_cli(Some(b"")));
    // try expectError(error.InvalidValue, Duration.parseCLI("1"));
    assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"1")));
    // try expectError(error.InvalidValue, Duration.parseCLI("s"));
    assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"s")));
    // try expectError(error.InvalidValue, Duration.parseCLI("1x"));
    assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"1x")));
    // try expectError(error.InvalidValue, Duration.parseCLI("1 "));
    assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"1 ")));
}

// test "test format" (Duration)
#[test]
fn duration_test_format() {
    // inline for (Duration.units) |unit| {
    //     const d: Duration = .{ .duration = unit.factor };
    //     const actual = "{f}".{d};
    //     const expected = if (!eql(unit.name, "us")) "1{s}".{unit.name} else "1µs";
    //     expectEqualSlices(expected, actual);
    // }
    for &(uname, factor) in DURATION_UNITS {
        let d = Duration { duration: factor };
        let mut actual: Vec<u8> = Vec::new();
        d.format(&mut actual);
        let expected: Vec<u8> = if uname != b"us" {
            let mut e: Vec<u8> = vec![b'1'];
            e.extend_from_slice(uname);
            e
        } else {
            "1µs".as_bytes().to_vec()
        };
        assert_eq!(expected, actual);
    }
}

// test "test entryFormatter" (Duration)
#[test]
fn duration_test_entry_formatter() {
    let mut buf = AllocatingWriter::init();
    // var p: Duration = .{ .duration = std.math.maxInt(u64) };
    let p = Duration { duration: u64::MAX };
    let mut f = entry_formatter(b"a", &mut buf);
    f.format_entry(&p).unwrap();
    // try std.testing.expectEqualStrings("a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n", buf.written());
    assert_eq!(
        "a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n".as_bytes(),
        buf.written()
    );
}

// -----------------------------------------------------------------------
// WindowPadding
// -----------------------------------------------------------------------

// test "parse WindowPadding"
#[test]
fn parse_window_padding() {
    {
        // const v = try WindowPadding.parseCLI("100");
        let v = WindowPadding::parse_cli(Some(b"100")).unwrap();
        // try testing.expectEqual(WindowPadding{ .top_left = 100, .bottom_right = 100 }, v);
        assert_eq!(
            WindowPadding {
                top_left: 100,
                bottom_right: 100
            },
            v
        );
    }

    {
        // const v = try WindowPadding.parseCLI("100,200");
        let v = WindowPadding::parse_cli(Some(b"100,200")).unwrap();
        assert_eq!(
            WindowPadding {
                top_left: 100,
                bottom_right: 200
            },
            v
        );
    }

    // Trim whitespace
    {
        // const v = try WindowPadding.parseCLI(" 100 , 200 ");
        let v = WindowPadding::parse_cli(Some(b" 100 , 200 ")).unwrap();
        assert_eq!(
            WindowPadding {
                top_left: 100,
                bottom_right: 200
            },
            v
        );
    }

    // try testing.expectError(error.ValueRequired, WindowPadding.parseCLI(null));
    assert_eq!(Err(Error::ValueRequired), WindowPadding::parse_cli(None));
    // try testing.expectError(error.InvalidValue, WindowPadding.parseCLI(""));
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b""))
    );
    // try testing.expectError(error.InvalidValue, WindowPadding.parseCLI("a"));
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"a"))
    );
}

// PORT NOTE (no reference test): `parseCLI` routes both branches through
// `std.fmt.parseInt(u32, ..., 10)`, which accepts a leading `+`/`-` sign and
// `_` digit separators. Locks the accept/reject boundary of the ported
// `parse_int_u32_base10` against the Zig-0.16 oracle (probed:
// "+1"=>1, "1_0"=>10, "1__0"=>10, "-0"=>0, "_10"/"10_"=>InvalidCharacter,
// "-1"=>Overflow — the latter two both collapse to `error.InvalidValue`).
#[test]
fn parse_window_padding_zig_int_spellings() {
    let pad = |tl: u32, br: u32| WindowPadding {
        top_left: tl,
        bottom_right: br,
    };

    // Leading '+' is accepted by parseInt.
    assert_eq!(Ok(pad(1, 1)), WindowPadding::parse_cli(Some(b"+1")));
    assert_eq!(Ok(pad(1, 2)), WindowPadding::parse_cli(Some(b"+1,+2")));

    // '_' digit separators, including doubled ones (parseInt skips them).
    assert_eq!(Ok(pad(10, 10)), WindowPadding::parse_cli(Some(b"1_0")));
    assert_eq!(Ok(pad(10, 2)), WindowPadding::parse_cli(Some(b"1_0,2")));
    assert_eq!(Ok(pad(10, 10)), WindowPadding::parse_cli(Some(b"1__0")));

    // Unsigned negative zero parses to 0; any other negative overflows.
    assert_eq!(Ok(pad(0, 0)), WindowPadding::parse_cli(Some(b"-0")));
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"-1"))
    );

    // A separator at either end of the digit run is invalid.
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"_10"))
    );
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"10_"))
    );
    // A bare sign is invalid.
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"+"))
    );
    // Base 10 means no `0x` prefix detection.
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"0x10"))
    );
    // Overflow of u32 is still rejected.
    assert_eq!(
        Err(Error::InvalidValue),
        WindowPadding::parse_cli(Some(b"4294967296"))
    );
    assert_eq!(
        Ok(pad(4294967295, 4294967295)),
        WindowPadding::parse_cli(Some(b"4294967295"))
    );
}
