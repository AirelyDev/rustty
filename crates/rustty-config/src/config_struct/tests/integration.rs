//! PORTS: ghostty src/config/Config.zig — the INTEGRATION tests
//! (finalize/clone/conditional/theme/compatibility).

// ===========================================================================
// INTEGRATION tests. PORTS: the top-level `test {}` blocks in the
// finalize/clone/conditional/theme/compatibility range of Config.zig
// (lines 10341-10934), assertion-for-assertion. The Duration tests
// ("parse duration"/"test format"/"test entryFormatter") and the
// "handle bom"/"clone default"/"clone preserves conditional state"/"changed"
// tests in this range are ported in their home modules (value_types.rs and the
// `tests`/`machinery_tests` modules above).
// ===========================================================================
use super::*;
use crate::value_types::{BoldColor, Color, MacOSDockDropBehavior, TerminalColor};
use rustty_os::tempdir::TempDir;

// const TestIterator = struct {
//     data: []const []const u8,
//     i: usize = 0,
//     pub fn next(self: *TestIterator) ?[]const u8 { ... }
// };
//
// PORTS the reference's test-local `TestIterator`. Like the reference's
// iterator it yields raw arg strings and has no `location` decl. It
// implements `SelfDrivenIterator<Config>` directly (ignoring the lent
// destination, exactly as a non-replay iterator does) so it can drive
// `Config::load_iter`.
struct TestIterator {
    data: Vec<Vec<u8>>,
    i: usize,
}

impl TestIterator {
    fn new(data: &[&[u8]]) -> TestIterator {
        TestIterator {
            data: data.iter().map(|s| s.to_vec()).collect(),
            i: 0,
        }
    }
}

impl rustty_args::args::SelfDrivenIterator<Config> for TestIterator {
    // pub fn next(self: *TestIterator) ?[]const u8 {
    //     if (self.i >= self.data.len) return null;
    //     const result = self.data[self.i];
    //     self.i += 1;
    //     return result;
    // }
    fn next_self(&mut self, _config: &mut Config) -> Option<Vec<u8>> {
        // if (self.i >= self.data.len) return null;
        if self.i >= self.data.len() {
            return None;
        }
        // const result = self.data[self.i];
        let result = self.data[self.i].clone();
        // self.i += 1;
        self.i += 1;
        // return result;
        Some(result)
    }

    // TestIterator has no `location` decl => `Location.fromIter` yields `.none`.
    fn location_self(&self) -> Option<rustty_args::diagnostics::Location> {
        None
    }
}

// test "parse hook: invalid command" {
#[test]
fn parse_hook_invalid_command() {
    // var cfg = try Config.default(testing.allocator);
    let mut cfg = Config::default_config();
    // const alloc = cfg._arena.?.allocator();

    // var it: TestIterator = .{ .data = &.{"foo"} };
    let mut it = TestIterator::new(&[b"foo"]);
    // try testing.expect(try cfg.parseManuallyHook(alloc, "--command", &it));
    //
    // PORT NOTE: `parseManuallyHook("--command", iter)` returns `Continue`
    // (the reference's `true`) — `--command` is not `-e`, so it records the
    // replay arg and returns "continue". We assert the `HookOutcome` rather
    // than a bare bool.
    assert_eq!(
        cfg.parse_manually_hook(b"--command").unwrap(),
        rustty_args::args::HookOutcome::Continue
    );
    let _ = &mut it;
    // try testing.expect(cfg.command == null);
    assert!(cfg.command.is_none());
}

// test "parse e: command only" {
#[test]
fn parse_e_command_only() {
    // var cfg = try Config.default(testing.allocator);
    let mut cfg = Config::default_config();

    // var it: TestIterator = .{ .data = &.{"foo"} };
    let mut it = TestIterator::new(&[b"foo"]);
    // try testing.expect(!try cfg.parseManuallyHook(alloc, "-e", &it));
    //
    // PORT NOTE: the reference invokes `parseManuallyHook("-e", &it)` and the
    // hook itself drains `it`. The Rust hook is split: it records the `-e`
    // marker and returns `ConsumeRest` (the reference's `!... => return false`,
    // i.e. "do not continue"); the DRIVER then drains `it` and calls
    // `apply_e_command`. This test reproduces the driver's verbatim drain loop
    // inline so it exercises exactly the path `parseManuallyHook("-e", &it)`
    // runs in the reference.
    assert_eq!(
        cfg.parse_manually_hook(b"-e").unwrap(),
        rustty_args::args::HookOutcome::ConsumeRest
    );
    // The driver's drain (see `parse_self`'s `ConsumeRest` arm).
    let mut command: Vec<Vec<u8>> = Vec::new();
    while let Some(param) = {
        use rustty_args::args::SelfDrivenIterator;
        it.next_self(&mut cfg)
    } {
        // `dst.record_replay_arg(&param)` — the driver's per-arg replay record.
        cfg._replay_steps.push(ReplayStep::Arg(param.clone()));
        command.push(param);
    }
    let consumed_everything =
        !cfg.apply_e_command(command, rustty_args::diagnostics::Location::None);
    // The reference's hook returns `false` ("do not continue") on this path.
    assert!(consumed_everything);

    // const cmd = cfg.@"initial-command".?;
    let cmd = cfg.initial_command.as_ref().unwrap();
    // try testing.expect(cmd == .direct);
    // try testing.expectEqual(cmd.direct.len, 1);
    // try testing.expectEqualStrings(cmd.direct[0], "foo");
    match cmd {
        Command::Direct(direct) => {
            assert_eq!(direct.len(), 1);
            assert_eq!(direct[0].as_slice(), b"foo");
        }
        _ => panic!("expected .direct"),
    }
}

// test "parse e: command and args" {
#[test]
fn parse_e_command_and_args() {
    // var cfg = try Config.default(testing.allocator);
    let mut cfg = Config::default_config();

    // var it: TestIterator = .{ .data = &.{ "echo", "foo", "bar baz" } };
    let mut it = TestIterator::new(&[b"echo", b"foo", b"bar baz"]);
    // try testing.expect(!try cfg.parseManuallyHook(alloc, "-e", &it));
    //
    // See `parse_e_command_only` for the hook/driver split. We reproduce the
    // driver's drain inline here too.
    assert_eq!(
        cfg.parse_manually_hook(b"-e").unwrap(),
        rustty_args::args::HookOutcome::ConsumeRest
    );
    let mut command: Vec<Vec<u8>> = Vec::new();
    while let Some(param) = {
        use rustty_args::args::SelfDrivenIterator;
        it.next_self(&mut cfg)
    } {
        // `dst.record_replay_arg(&param)` — the driver's per-arg replay record.
        cfg._replay_steps.push(ReplayStep::Arg(param.clone()));
        command.push(param);
    }
    let consumed_everything =
        !cfg.apply_e_command(command, rustty_args::diagnostics::Location::None);
    assert!(consumed_everything);

    // const cmd = cfg.@"initial-command".?;
    let cmd = cfg.initial_command.as_ref().unwrap();
    // try testing.expect(cmd == .direct);
    // try testing.expectEqual(cmd.direct.len, 3);
    // try testing.expectEqualStrings(cmd.direct[0], "echo");
    // try testing.expectEqualStrings(cmd.direct[1], "foo");
    // try testing.expectEqualStrings(cmd.direct[2], "bar baz");
    match cmd {
        Command::Direct(direct) => {
            assert_eq!(direct.len(), 3);
            assert_eq!(direct[0].as_slice(), b"echo");
            assert_eq!(direct[1].as_slice(), b"foo");
            assert_eq!(direct[2].as_slice(), b"bar baz");
        }
        _ => panic!("expected .direct"),
    }
}

// test "clone can then change conditional state" {
#[test]
fn clone_can_then_change_conditional_state() {
    // This tests a particular bug sequence where:
    //   1. Load light
    //   2. Convert to dark
    //   3. Clone dark
    //   4. Convert to light
    //   5. Config is still dark (bug)

    // Setup our test theme
    // var td = try internal_os.TempDir.init();
    let mut td = TempDir::init().unwrap();
    // defer td.deinit();
    // {
    //     var file = try td.dir.createFile("theme_light", .{});
    //     ... writeAll(@embedFile("testdata/theme_light")); ...
    // }
    td.dir()
        .writeFile("theme_light", include_bytes!("../../testdata/theme_light"))
        .unwrap();
    // {
    //     var file = try td.dir.createFile("theme_dark", .{});
    //     ... writeAll(@embedFile("testdata/theme_dark")); ...
    // }
    td.dir()
        .writeFile("theme_dark", include_bytes!("../../testdata/theme_dark"))
        .unwrap();
    // const light = try td.dir.realpath("theme_light", &light_buf);
    let light = td.dir().realpath("theme_light").unwrap();
    let light = light.to_str().unwrap();
    // const dark = try td.dir.realpath("theme_dark", &dark_buf);
    let dark = td.dir().realpath("theme_dark").unwrap();
    let dark = dark.to_str().unwrap();

    // var cfg_light = try Config.default(alloc);
    let mut cfg_light = Config::default_config();
    // var it: TestIterator = .{ .data = &.{
    //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
    // } };
    let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
    let mut it = TestIterator::new(&[arg.as_slice()]);
    // try cfg_light.loadIter(alloc, &it);
    cfg_light.load_iter(&mut it).unwrap();
    // try cfg_light.finalize();
    cfg_light.finalize().unwrap();

    // var cfg_dark = (try cfg_light.changeConditionalState(.{ .theme = .dark })).?;
    let mut cfg_dark = cfg_light
        .change_conditional_state(conditional::State {
            theme: conditional::Theme::Dark,
            ..Default::default()
        })
        .unwrap()
        .unwrap();

    // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, cfg_dark.background);
    assert_eq!(
        Color {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE,
        },
        cfg_dark.background
    );

    // var cfg_clone = try cfg_dark.clone(alloc);
    let mut cfg_clone = cfg_dark.clone_config();
    // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, cfg_clone.background);
    assert_eq!(
        Color {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE,
        },
        cfg_clone.background
    );

    // var cfg_light2 = (try cfg_clone.changeConditionalState(.{ .theme = .light })).?;
    let mut cfg_light2 = cfg_clone
        .change_conditional_state(conditional::State {
            theme: conditional::Theme::Light,
            ..Default::default()
        })
        .unwrap()
        .unwrap();
    // try testing.expectEqual(Color{ .r = 0xFF, .g = 0xFF, .b = 0xFF }, cfg_light2.background);
    assert_eq!(
        Color {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        cfg_light2.background
    );

    // defers
    cfg_light2.deinit();
    cfg_clone.deinit();
    cfg_dark.deinit();
    cfg_light.deinit();
    td.deinit();
}

// test "clone preserves conditional set" {
#[test]
fn clone_preserves_conditional_set() {
    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // var it: TestIterator = .{ .data = &.{
    //     "--theme=light:foo,dark:bar",
    //     "--window-theme=auto",
    // } };
    let mut it = TestIterator::new(&[b"--theme=light:foo,dark:bar", b"--window-theme=auto"]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();
    // try cfg.finalize();
    cfg.finalize().unwrap();

    // var clone1 = try cfg.clone(alloc);
    let clone1 = cfg.clone_config();

    // try testing.expect(clone1._conditional_set.contains(.theme));
    assert!(clone1._conditional_set.contains(&conditional::Key::Theme));
}

// test "working-directory expands tilde" {
#[test]
fn working_directory_expands_tilde() {
    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // var it: TestIterator = .{ .data = &.{
    //     "--working-directory=~/projects/ghostty",
    // } };
    let mut it = TestIterator::new(&[b"--working-directory=~/projects/ghostty"]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();
    // try cfg.finalize();
    cfg.finalize().unwrap();

    // var buf: [std.fs.max_path_bytes]u8 = undefined;
    // const expected = internal_os.expandHome("~/projects/ghostty", &buf)
    //     catch "~/projects/ghostty";
    let mut buf = [0u8; MAX_PATH_BYTES];
    let expected: Vec<u8> = rustty_os::expandHome(
        &rustty_global::environ_map(),
        b"~/projects/ghostty",
        &mut buf,
    )
    .map(|s| s.to_vec())
    .unwrap_or_else(|_| b"~/projects/ghostty".to_vec());
    // try testing.expectEqualStrings(expected, cfg.@"working-directory".?.value().?);
    assert_eq!(
        expected.as_slice(),
        cfg.working_directory.as_ref().unwrap().value().unwrap()
    );
}

// test "changeConditionalState ignores irrelevant changes" {
#[test]
fn change_conditional_state_ignores_irrelevant_changes() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--theme=foo" } };
        let mut it = TestIterator::new(&[b"--theme=foo"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expect(try cfg.changeConditionalState(.{ .theme = .dark }) == null);
        assert!(cfg
            .change_conditional_state(conditional::State {
                theme: conditional::Theme::Dark,
                ..Default::default()
            })
            .unwrap()
            .is_none());
    }
}

// test "changeConditionalState applies relevant changes" {
#[test]
fn change_conditional_state_applies_relevant_changes() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--theme=light:foo,dark:bar" } };
        let mut it = TestIterator::new(&[b"--theme=light:foo,dark:bar"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // var cfg2 = (try cfg.changeConditionalState(.{ .theme = .dark })).?;
        let cfg2 = cfg
            .change_conditional_state(conditional::State {
                theme: conditional::Theme::Dark,
                ..Default::default()
            })
            .unwrap()
            .unwrap();

        // try testing.expect(cfg2._conditional_set.contains(.theme));
        assert!(cfg2._conditional_set.contains(&conditional::Key::Theme));
    }
}

// test "theme loading" {
#[test]
fn theme_loading() {
    // Setup our test theme
    // var td = try internal_os.TempDir.init();
    let mut td = TempDir::init().unwrap();
    // { var file = try td.dir.createFile("theme", .{}); ... @embedFile("testdata/theme_simple") ... }
    td.dir()
        .writeFile("theme", include_bytes!("../../testdata/theme_simple"))
        .unwrap();
    // const path = try td.dir.realpath("theme", &path_buf);
    let path = td.dir().realpath("theme").unwrap();
    let path = path.to_str().unwrap();

    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // var it: TestIterator = .{ .data = &.{
    //     try std.fmt.allocPrint(alloc_arena, "--theme={s}", .{path}),
    // } };
    let arg = format!("--theme={path}").into_bytes();
    let mut it = TestIterator::new(&[arg.as_slice()]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();
    // try cfg.finalize();
    cfg.finalize().unwrap();

    // try testing.expectEqual(Color{ .r = 0x12, .g = 0x3A, .b = 0xBC }, cfg.background);
    assert_eq!(
        Color {
            r: 0x12,
            g: 0x3A,
            b: 0xBC,
        },
        cfg.background
    );

    // Not a conditional theme
    // try testing.expect(!cfg._conditional_set.contains(.theme));
    assert!(!cfg._conditional_set.contains(&conditional::Key::Theme));

    // defer td.deinit();
    td.deinit();
}

// test "theme loading preserves conditional state" {
#[test]
fn theme_loading_preserves_conditional_state() {
    // Setup our test theme
    // var td = try internal_os.TempDir.init();
    let mut td = TempDir::init().unwrap();
    td.dir()
        .writeFile("theme", include_bytes!("../../testdata/theme_simple"))
        .unwrap();
    // const path = try td.dir.realpath("theme", &path_buf);
    let path = td.dir().realpath("theme").unwrap();
    let path = path.to_str().unwrap();

    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // cfg._conditional_state = .{ .theme = .dark };
    cfg._conditional_state = conditional::State {
        theme: conditional::Theme::Dark,
        ..Default::default()
    };
    // var it: TestIterator = .{ .data = &.{
    //     try std.fmt.allocPrint(alloc_arena, "--theme={s}", .{path}),
    // } };
    let arg = format!("--theme={path}").into_bytes();
    let mut it = TestIterator::new(&[arg.as_slice()]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();
    // try cfg.finalize();
    cfg.finalize().unwrap();

    // try testing.expect(cfg._conditional_state.theme == .dark);
    assert!(cfg._conditional_state.theme == conditional::Theme::Dark);

    // defer td.deinit();
    td.deinit();
}

// test "theme priority is lower than config" {
#[test]
fn theme_priority_is_lower_than_config() {
    // Setup our test theme
    // var td = try internal_os.TempDir.init();
    let mut td = TempDir::init().unwrap();
    td.dir()
        .writeFile("theme", include_bytes!("../../testdata/theme_simple"))
        .unwrap();
    // const path = try td.dir.realpath("theme", &path_buf);
    let path = td.dir().realpath("theme").unwrap();
    let path = path.to_str().unwrap();

    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // var it: TestIterator = .{ .data = &.{
    //     "--background=#ABCDEF",
    //     try std.fmt.allocPrint(alloc_arena, "--theme={s}", .{path}),
    // } };
    let theme_arg = format!("--theme={path}").into_bytes();
    let mut it = TestIterator::new(&[b"--background=#ABCDEF", theme_arg.as_slice()]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();
    // try cfg.finalize();
    cfg.finalize().unwrap();

    // try testing.expectEqual(Color{ .r = 0xAB, .g = 0xCD, .b = 0xEF }, cfg.background);
    assert_eq!(
        Color {
            r: 0xAB,
            g: 0xCD,
            b: 0xEF,
        },
        cfg.background
    );

    // defer td.deinit();
    td.deinit();
}

// test "theme loading correct light/dark" {
#[test]
fn theme_loading_correct_light_dark() {
    // Setup our test theme
    // var td = try internal_os.TempDir.init();
    let mut td = TempDir::init().unwrap();
    td.dir()
        .writeFile("theme_light", include_bytes!("../../testdata/theme_light"))
        .unwrap();
    td.dir()
        .writeFile("theme_dark", include_bytes!("../../testdata/theme_dark"))
        .unwrap();
    // const light = try td.dir.realpath("theme_light", &light_buf);
    let light = td.dir().realpath("theme_light").unwrap();
    let light = light.to_str().unwrap();
    // const dark = try td.dir.realpath("theme_dark", &dark_buf);
    let dark = td.dir().realpath("theme_dark").unwrap();
    let dark = dark.to_str().unwrap();

    // Light
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
        // } };
        let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
        let mut it = TestIterator::new(&[arg.as_slice()]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(Color{ .r = 0xFF, .g = 0xFF, .b = 0xFF }, cfg.background);
        assert_eq!(
            Color {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            },
            cfg.background
        );
    }

    // Dark
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // cfg._conditional_state = .{ .theme = .dark };
        cfg._conditional_state = conditional::State {
            theme: conditional::Theme::Dark,
            ..Default::default()
        };
        // var it: TestIterator = .{ .data = &.{
        //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
        // } };
        let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
        let mut it = TestIterator::new(&[arg.as_slice()]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, cfg.background);
        assert_eq!(
            Color {
                r: 0xEE,
                g: 0xEE,
                b: 0xEE,
            },
            cfg.background
        );
    }

    // Light to Dark
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
        // } };
        let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
        let mut it = TestIterator::new(&[arg.as_slice()]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // var new = (try cfg.changeConditionalState(.{ .theme = .dark })).?;
        let new = cfg
            .change_conditional_state(conditional::State {
                theme: conditional::Theme::Dark,
                ..Default::default()
            })
            .unwrap()
            .unwrap();
        // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, new.background);
        assert_eq!(
            Color {
                r: 0xEE,
                g: 0xEE,
                b: 0xEE,
            },
            new.background
        );
    }

    // defer td.deinit();
    td.deinit();
}

// test "theme specifying light/dark changes window-theme from auto" {
#[test]
fn theme_specifying_light_dark_changes_window_theme_from_auto() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     "--theme=light:foo,dark:bar",
        //     "--window-theme=auto",
        // } };
        let mut it = TestIterator::new(&[b"--theme=light:foo,dark:bar", b"--window-theme=auto"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expect(cfg.@"window-theme" == .system);
        assert!(cfg.window_theme == WindowTheme::System);
    }
}

// test "theme specifying light/dark sets theme usage in conditional state" {
#[test]
fn theme_specifying_light_dark_sets_theme_usage_in_conditional_state() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     "--theme=light:foo,dark:bar",
        //     "--window-theme=auto",
        // } };
        let mut it = TestIterator::new(&[b"--theme=light:foo,dark:bar", b"--window-theme=auto"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expect(cfg.@"window-theme" == .system);
        assert!(cfg.window_theme == WindowTheme::System);
        // try testing.expect(cfg._conditional_set.contains(.theme));
        assert!(cfg._conditional_set.contains(&conditional::Key::Theme));
    }
}

// test "scrollback limits" {
#[test]
fn scrollback_limits() {
    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // try testing.expectEqual(@as(usize, 50_000_000), cfg.@"scrollback-limit-bytes".value);
    assert_eq!(50_000_000usize, cfg.scrollback_limit_bytes.value);
    // try testing.expectEqual(std.math.maxInt(usize), cfg.@"scrollback-limit-lines".value);
    assert_eq!(usize::MAX, cfg.scrollback_limit_lines.value);

    // var it: TestIterator = .{ .data = &.{
    //     "--scrollback-limit-bytes=1234",
    //     "--scrollback-limit-lines=567",
    // } };
    let mut it = TestIterator::new(&[
        b"--scrollback-limit-bytes=1234",
        b"--scrollback-limit-lines=567",
    ]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();

    // try testing.expectEqual(@as(usize, 1234), cfg.@"scrollback-limit-bytes".value);
    assert_eq!(1234usize, cfg.scrollback_limit_bytes.value);
    // try testing.expectEqual(@as(usize, 567), cfg.@"scrollback-limit-lines".value);
    assert_eq!(567usize, cfg.scrollback_limit_lines.value);

    // var unlimited_it: TestIterator = .{ .data = &.{
    //     "--scrollback-limit-bytes=unlimited",
    //     "--scrollback-limit-lines=unlimited",
    // } };
    let mut unlimited_it = TestIterator::new(&[
        b"--scrollback-limit-bytes=unlimited",
        b"--scrollback-limit-lines=unlimited",
    ]);
    cfg.load_iter(&mut unlimited_it).unwrap();

    assert_eq!(usize::MAX, cfg.scrollback_limit_bytes.value);
    assert_eq!(usize::MAX, cfg.scrollback_limit_lines.value);

    // var reset_it: TestIterator = .{ .data = &.{
    //     "--scrollback-limit-bytes=",
    //     "--scrollback-limit-lines=",
    // } };
    let mut reset_it =
        TestIterator::new(&[b"--scrollback-limit-bytes=", b"--scrollback-limit-lines="]);
    cfg.load_iter(&mut reset_it).unwrap();

    assert_eq!(50_000_000usize, cfg.scrollback_limit_bytes.value);
    assert_eq!(usize::MAX, cfg.scrollback_limit_lines.value);

    // PORT NOTE (rustty-only, `P3-W3` T4 Step 5): the oracle's tests are
    // PARSE-ONLY, which a non-round-tripping formatter would pass. Assert
    // that `unlimited` survives `format_entry` as the literal sentinel and
    // that a finite value still formats as an integer.
    {
        let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
        cfg.format_entry_field(b"scrollback-limit-lines", &mut buf)
            .unwrap();
        assert_eq!(
            b"scrollback-limit-lines = unlimited\n".as_slice(),
            buf.written()
        );
    }
    {
        let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
        cfg.format_entry_field(b"scrollback-limit-bytes", &mut buf)
            .unwrap();
        assert_eq!(
            b"scrollback-limit-bytes = 50000000\n".as_slice(),
            buf.written()
        );
    }
    // And that the formatted `unlimited` parses back to the same value.
    let mut round = Config::default_config();
    round
        .parse_into_field(b"scrollback-limit-bytes", Some(b"unlimited"))
        .unwrap();
    assert_eq!(usize::MAX, round.scrollback_limit_bytes.value);
    assert_eq!(None, round.scrollback_limit_bytes.optional());
    assert_eq!(Some(50_000_000usize), cfg.scrollback_limit_bytes.optional());
}

// test "clipboard write limit" {
#[test]
fn clipboard_write_limit() {
    // var cfg = try Config.default(alloc);
    // defer cfg.deinit();
    let mut cfg = Config::default_config();
    // try testing.expectEqual(
    //     @as(usize, 64 * 1024 * 1024),
    //     cfg.@"clipboard-write-limit-bytes".value,
    // );
    assert_eq!(64 * 1024 * 1024, cfg.clipboard_write_limit_bytes.value);

    // var it: TestIterator = .{ .data = &.{
    //     "--clipboard-write-limit-bytes=1234",
    // } };
    let mut it = TestIterator::new(&[b"--clipboard-write-limit-bytes=1234"]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();

    // try testing.expectEqual(
    //     @as(usize, 1234),
    //     cfg.@"clipboard-write-limit-bytes".value,
    // );
    assert_eq!(1234usize, cfg.clipboard_write_limit_bytes.value);

    // var unlimited_it: TestIterator = .{ .data = &.{
    //     "--clipboard-write-limit-bytes=unlimited",
    // } };
    let mut unlimited_it = TestIterator::new(&[b"--clipboard-write-limit-bytes=unlimited"]);
    cfg.load_iter(&mut unlimited_it).unwrap();

    // try testing.expectEqual(
    //     std.math.maxInt(usize),
    //     cfg.@"clipboard-write-limit-bytes".value,
    // );
    assert_eq!(usize::MAX, cfg.clipboard_write_limit_bytes.value);
}

// rustty-only (P7-W6 T1, `714fe9b90` + `2a987d59d`; the reference adds the
// option without a `test {}`): `vt-window-resize-allowed` defaults to
// false, parses, formats, and sits right after `vt-kam-allowed` in the
// declaration (so `show-config`) order, where `2a987d59d` moved it.
#[test]
fn vt_window_resize_allowed_default_parse_and_order() {
    let mut cfg = Config::default_config();
    assert!(!cfg.vt_window_resize_allowed);
    {
        let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
        cfg.format_entry_field(b"vt-window-resize-allowed", &mut buf)
            .unwrap();
        assert_eq!(
            b"vt-window-resize-allowed = false\n".as_slice(),
            buf.written()
        );
    }

    let mut it = TestIterator::new(&[b"--vt-window-resize-allowed=true"]);
    cfg.load_iter(&mut it).unwrap();
    assert!(cfg._diagnostics.empty());
    assert!(cfg.vt_window_resize_allowed);
    assert!(cfg.changed(&Config::default_config(), Key::VtWindowResizeAllowed));

    let names: Vec<&[u8]> = CONFIG_FIELDS.iter().map(|(n, _)| *n).collect();
    let at = names
        .iter()
        .position(|n| *n == b"vt-window-resize-allowed".as_slice())
        .expect("vt-window-resize-allowed is a config field");
    assert_eq!(b"vt-kam-allowed".as_slice(), names[at - 1]);
    assert_eq!(b"custom-shader".as_slice(), names[at + 1]);
    assert_eq!(
        Some(Key::VtWindowResizeAllowed),
        Key::string_to_enum(b"vt-window-resize-allowed")
    );
}

// test "compatibility: scrollback-limit renamed to bytes" {
#[test]
fn compatibility_scrollback_limit_renamed_to_bytes() {
    // var cfg = try Config.default(alloc);
    let mut cfg = Config::default_config();
    // var it: TestIterator = .{ .data = &.{ "--scrollback-limit=1234" } };
    let mut it = TestIterator::new(&[b"--scrollback-limit=1234"]);
    // try cfg.loadIter(alloc, &it);
    cfg.load_iter(&mut it).unwrap();

    // try testing.expectEqual(@as(usize, 1234), cfg.@"scrollback-limit-bytes".value);
    assert_eq!(1234usize, cfg.scrollback_limit_bytes.value);

    // var unlimited_it: TestIterator = .{ .data = &.{ "--scrollback-limit=unlimited" } };
    let mut unlimited_it = TestIterator::new(&[b"--scrollback-limit=unlimited"]);
    cfg.load_iter(&mut unlimited_it).unwrap();

    // try testing.expectEqual(std.math.maxInt(usize), cfg.@"scrollback-limit-bytes".value);
    assert_eq!(usize::MAX, cfg.scrollback_limit_bytes.value);
}

// PORT-SIDE REGRESSION (`dfccdb2d4` + `e80ce2ed4`, #12600, phase-6): the
// reference added `compatCopyOnSelect` without a `test {}`. `true`/`false`
// are no longer variants of `CopyOnSelect`, so a bare enum parse of them
// fails and the compatibility handler is what maps them onto the new
// values — `true` -> `primary` on Linux/FreeBSD and `clipboard` elsewhere
// (the "correct mapping" the reference's table comment names), `false` ->
// `none`. NEGATIVE CONTROL: removing the `copy-on-select` row from
// `compatibility()` (or the handler's `true` arm) leaves the field at its
// default and records an `InvalidValue` diagnostic, failing the
// `cfg._diagnostics.empty()` assertions below.
#[test]
fn compatibility_copy_on_select_true_false() {
    // `true` maps to the platform's old "prefer the selection clipboard"
    // behaviour.
    {
        let mut cfg = Config::default_config();
        let mut it = TestIterator::new(&[b"--copy-on-select=true"]);
        cfg.load_iter(&mut it).unwrap();
        assert!(cfg._diagnostics.empty());
        let want = if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            CopyOnSelect::Primary
        } else {
            CopyOnSelect::Clipboard
        };
        assert_eq!(want, cfg.copy_on_select);
    }

    // `false` is an alias for `none`.
    {
        let mut cfg = Config::default_config();
        let mut it = TestIterator::new(&[b"--copy-on-select=false"]);
        cfg.load_iter(&mut it).unwrap();
        assert!(cfg._diagnostics.empty());
        assert_eq!(CopyOnSelect::None, cfg.copy_on_select);
    }

    // The new spellings parse as plain enum values (no handler involved).
    for (arg, want) in [
        (&b"--copy-on-select=none"[..], CopyOnSelect::None),
        (&b"--copy-on-select=primary"[..], CopyOnSelect::Primary),
        (&b"--copy-on-select=clipboard"[..], CopyOnSelect::Clipboard),
        (&b"--copy-on-select=both"[..], CopyOnSelect::Both),
    ] {
        let mut cfg = Config::default_config();
        let mut it = TestIterator::new(&[arg]);
        cfg.load_iter(&mut it).unwrap();
        assert!(cfg._diagnostics.empty());
        assert_eq!(want, cfg.copy_on_select);
    }

    // The handler only rescues `true`/`false`: anything else is still an
    // invalid value (recorded as a diagnostic, field left at its default).
    {
        let mut cfg = Config::default_config();
        let before = cfg.copy_on_select;
        let mut it = TestIterator::new(&[b"--copy-on-select=bogus"]);
        cfg.load_iter(&mut it).unwrap();
        assert!(!cfg._diagnostics.empty());
        assert_eq!(before, cfg.copy_on_select);
    }

    // The direct handler contract (`compatCopyOnSelect`): `true`/`false`
    // handled, anything else `false` (unhandled) and untouched.
    {
        let mut cfg = Config::default_config();
        assert!(cfg.compat_copy_on_select(b"copy-on-select", Some(b"false")));
        assert_eq!(CopyOnSelect::None, cfg.copy_on_select);
        assert!(!cfg.compat_copy_on_select(b"copy-on-select", Some(b"primary")));
        assert_eq!(CopyOnSelect::None, cfg.copy_on_select);
        assert!(!cfg.compat_copy_on_select(b"copy-on-select", None));
    }
}

// PORT-SIDE REGRESSION (`dfccdb2d4`, #12600, phase-6): `middle-click-action`
// gains `clipboard-paste` and keeps `primary-paste` (still the default).
#[test]
fn middle_click_action_clipboard_paste() {
    let cfg = Config::default_config();
    assert_eq!(MiddleClickAction::PrimaryPaste, cfg.middle_click_action);

    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[b"--middle-click-action=clipboard-paste"]);
    cfg.load_iter(&mut it).unwrap();
    assert!(cfg._diagnostics.empty());
    assert_eq!(MiddleClickAction::ClipboardPaste, cfg.middle_click_action);
}

// parity S67-F1: `cli/args.zig` `formatInvalidValue`/`formatValueRequired`
// append `formatValues`' `, valid values are: ...` for an enum-typed field.
// The cursor-style text is the one MEASURED from Ghostty's
// `+validate-config` (installed release build, not the pin).
#[test]
fn enum_field_diagnostics_list_the_valid_values() {
    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[
        b"--cursor-style=blah",
        b"--cursor-style",
        b"--font-size=bogus",
    ]);
    cfg.load_iter(&mut it).unwrap();
    let messages: Vec<&[u8]> = cfg
        ._diagnostics
        .items()
        .iter()
        .map(|d| d.message.as_slice())
        .collect();
    assert_eq!(
        vec![
            &b"invalid value \"blah\", valid values are: bar, block, underline, block_hollow"[..],
            &b"value required, valid values are: bar, block, underline, block_hollow"[..],
            // A non-enum field gets no suffix.
            &b"invalid value \"bogus\""[..],
        ],
        messages
    );
}

// parity S67-F5: a `CommaSplitter` error in `mouse-scroll-multiplier` is not
// `InvalidValue`, so `MouseScrollMultiplier.parseCLI` does not take its
// bare-float fallback and `parse` reports `unknown error error.<Name>`.
// MEASURED: Ghostty 1.3.1 `+validate-config` prints exactly these four
// messages for these four lines.
#[test]
fn auto_struct_comma_splitter_errors_are_not_invalid_value() {
    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[
        b"--mouse-scroll-multiplier=precision:\"1",
        b"--mouse-scroll-multiplier=precision:1\\q",
        b"--mouse-scroll-multiplier=precision:1\\",
        b"--mouse-scroll-multiplier=bogus",
    ]);
    cfg.load_iter(&mut it).unwrap();
    let messages: Vec<&[u8]> = cfg
        ._diagnostics
        .items()
        .iter()
        .map(|d| d.message.as_slice())
        .collect();
    // On Windows `CommaSplitter`'s `escape_outside_quotes` is false (the
    // backslash is the path separator), so the two escape lines are no
    // `CommaSplitter` error there: `MouseScrollMultiplier` takes its
    // `InvalidValue` arm (task W-2, measured on Windows).
    #[cfg(target_os = "windows")]
    assert_eq!(
        vec![
            &b"unknown error error.UnclosedQuote"[..],
            &b"invalid value \"precision:1\\q\""[..],
            &b"invalid value \"precision:1\\\""[..],
            &b"invalid value \"bogus\""[..],
        ],
        messages
    );
    #[cfg(not(target_os = "windows"))]
    assert_eq!(
        vec![
            &b"unknown error error.UnclosedQuote"[..],
            &b"unknown error error.IllegalEscape"[..],
            &b"unknown error error.UnfinishedEscape"[..],
            &b"invalid value \"bogus\""[..],
        ],
        messages
    );
}

// test "compatibility: gtk-single-instance desktop" {
#[test]
fn compatibility_gtk_single_instance_desktop() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--gtk-single-instance=desktop" } };
        let mut it = TestIterator::new(&[b"--gtk-single-instance=desktop"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();

        // We need to test this BEFORE finalize, because finalize will
        // convert our detect to a real value.
        // try testing.expectEqual(GtkSingleInstance.detect, cfg.@"gtk-single-instance");
        assert_eq!(GtkSingleInstance::Detect, cfg.gtk_single_instance);
    }
}

// test "compatibility: removed cursor-invert-fg-bg" {
#[test]
fn compatibility_removed_cursor_invert_fg_bg() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--cursor-invert-fg-bg" } };
        let mut it = TestIterator::new(&[b"--cursor-invert-fg-bg"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(TerminalColor.@"cell-foreground", cfg.@"cursor-color");
        assert_eq!(Some(TerminalColor::CellForeground), cfg.cursor_color);
        // try testing.expectEqual(TerminalColor.@"cell-background", cfg.@"cursor-text");
        assert_eq!(Some(TerminalColor::CellBackground), cfg.cursor_text);
    }
}

// test "compatibility: removed selection-invert-fg-bg" {
#[test]
fn compatibility_removed_selection_invert_fg_bg() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--selection-invert-fg-bg" } };
        let mut it = TestIterator::new(&[b"--selection-invert-fg-bg"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(TerminalColor.@"cell-background", cfg.@"selection-foreground");
        assert_eq!(
            Some(TerminalColor::CellBackground),
            cfg.selection_foreground
        );
        // try testing.expectEqual(TerminalColor.@"cell-foreground", cfg.@"selection-background");
        assert_eq!(
            Some(TerminalColor::CellForeground),
            cfg.selection_background
        );
    }
}

// test "compatibility: removed bold-is-bright" {
#[test]
fn compatibility_removed_bold_is_bright() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--bold-is-bright" } };
        let mut it = TestIterator::new(&[b"--bold-is-bright"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(BoldColor.bright, cfg.@"bold-color");
        assert_eq!(Some(BoldColor::Bright), cfg.bold_color);
    }
}

// test "compatibility: window new-window" {
#[test]
fn compatibility_window_new_window() {
    // {
    {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--macos-dock-drop-behavior=window" } };
        let mut it = TestIterator::new(&[b"--macos-dock-drop-behavior=window"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();
        // try testing.expectEqual(MacOSDockDropBehavior.@"new-window", cfg.@"macos-dock-drop-behavior");
        assert_eq!(
            MacOSDockDropBehavior::NewWindow,
            cfg.macos_dock_drop_behavior
        );
    }
}

// =======================================================================
// Parity tail T-E: rows whose fix changes an observable parse result are
// tested here through the real parser (`load_iter`), not only at the value
// type.
// =======================================================================

// No reference test. LEDGER S78-F2: `Limit.parseCLI` is
// `std.fmt.parseInt(T, input, 0)`, so `_` separators and a radix prefix are
// valid in the config (values MEASURED on zig 0.16.0).
#[test]
fn limit_keys_accept_zig_int_separators_through_the_parser() {
    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[
        b"--scrollback-limit-bytes=50_000_000",
        b"--clipboard-write-limit-bytes=0x4_000_000",
    ]);
    cfg.load_iter(&mut it).unwrap();
    assert!(
        cfg._diagnostics.empty(),
        "no diagnostic expected, got {:?}",
        cfg._diagnostics.items()
    );
    assert_eq!(50_000_000, cfg.scrollback_limit_bytes.value);
    assert_eq!(0x400_0000, cfg.clipboard_write_limit_bytes.value);

    // Still rejected, as the reference rejects it: a trailing separator.
    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[b"--scrollback-limit-bytes=1_"]);
    cfg.load_iter(&mut it).unwrap();
    assert!(!cfg._diagnostics.empty());
}

/// Format `cfg` the way `+show-config` does and return the text.
fn show_config(cfg: &Config) -> String {
    let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
    crate::formatter_file::FileFormatter::new(cfg)
        .format(&mut buf)
        .unwrap();
    String::from_utf8(buf.written().to_vec()).unwrap()
}

// No reference test. LEDGER S76-F6: `mouse-scroll-multiplier`'s bare
// fallback and its `precision:`/`discrete:` fields are all
// `std.fmt.parseFloat`, which takes hex floats and `_` separators. (Fixed
// by the `parse_float_f64` port under S75-F1, `7a5bbdb8`; this pins the
// row's own surface through the parser, including the bare form.)
#[test]
fn mouse_scroll_multiplier_accepts_zig_float_syntax_through_the_parser() {
    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[b"--mouse-scroll-multiplier=0x1p1"]);
    cfg.load_iter(&mut it).unwrap();
    assert!(cfg._diagnostics.empty(), "{:?}", cfg._diagnostics.items());
    assert_eq!(
        (2.0, 2.0),
        (
            cfg.mouse_scroll_multiplier.precision,
            cfg.mouse_scroll_multiplier.discrete
        )
    );

    let mut it = TestIterator::new(&[b"--mouse-scroll-multiplier=precision:1_0,discrete:0x1.8"]);
    cfg.load_iter(&mut it).unwrap();
    assert!(cfg._diagnostics.empty(), "{:?}", cfg._diagnostics.items());
    assert!(show_config(&cfg).contains("mouse-scroll-multiplier = precision:10,discrete:1.5\n"));
}

// No reference test. LEDGER S75-F3: `RepeatableFontVariation.formatEntry`
// prints the value with `{d}`, which renders NaN as `nan` (MEASURED, zig
// 0.16.0: `{d}` of `std.math.nan(f64)` is `nan`; `parseFloat("nan")` and
// `parseFloat("-nan")` both give a NaN with the sign bit CLEAR).
#[test]
fn font_variation_nan_formats_as_zig_nan() {
    let mut cfg = Config::default_config();
    let mut it = TestIterator::new(&[
        b"--font-variation=wght=nan",
        b"--font-variation=wdth=-nan",
        b"--font-variation=slnt=-inf",
    ]);
    cfg.load_iter(&mut it).unwrap();
    assert!(cfg._diagnostics.empty(), "{:?}", cfg._diagnostics.items());
    // The values really are NaN (so the formatting arm under test runs).
    assert!(cfg.font_variation.list[0].value.is_nan());
    assert!(cfg.font_variation.list[1].value.is_nan());
    let out = show_config(&cfg);
    assert!(out.contains("font-variation = wght=nan\n"), "{out}");
    assert!(out.contains("font-variation = wdth=nan\n"), "{out}");
    assert!(out.contains("font-variation = slnt=-inf\n"), "{out}");
}

// No reference test. LEDGER S73-F1: the generated help text (the
// `+show-config --docs` source) must carry the reference's doc text under
// the per-token rename (`Ghostty`->`Rustty`, `ghostty`->`rustty`,
// `.port/tools/PARITY_NAMING.md`). Reference lines: `Config.zig:3493`
// `Ghostty > Secure Keyboard Entry`, `:3564` `~/.config/ghostty/Ghostty.icns`,
// `:3707` `each new ghostty process`.
#[test]
fn help_text_keeps_the_reference_token_casing() {
    let help = |key: Key| {
        String::from_utf8(crate::help_strings::config_help(key).unwrap().to_vec()).unwrap()
    };
    let secure = help(Key::MacosAutoSecureInput);
    assert!(
        secure.contains("`Rustty > Secure Keyboard Entry` menu item."),
        "{secure}"
    );
    let icon = help(Key::MacosCustomIcon);
    assert!(
        icon.contains("Defaults to `~/.config/rustty/Rustty.icns`"),
        "{icon}"
    );
    let single = help(Key::GtkSingleInstance);
    assert!(
        single.contains("each new rustty process will launch"),
        "{single}"
    );
}

// No reference test. LEDGER S76-F2: `RepeatableLink.parseCLI` returns
// `error.NotImplemented` (`Config.zig:8719-8724`), which is outside the
// args base set, so `cli/args.zig:158-170` records it through the `else`
// arm: `"unknown error {}"` of the error, i.e. `unknown error
// error.NotImplemented`. (Before the fix the port returned `InvalidValue`
// and recorded the invalid-value message instead.)
#[test]
fn link_key_records_the_reference_not_implemented_diagnostic() {
    let mut cfg = Config::default_config();
    let links_before = cfg.link.links.len();
    let mut it = TestIterator::new(&[b"--link=foo"]);
    cfg.load_iter(&mut it).unwrap();
    let items = cfg._diagnostics.items();
    assert_eq!(1, items.len(), "{items:?}");
    assert_eq!(b"link".as_slice(), items[0].key.as_slice());
    assert_eq!(
        "unknown error error.NotImplemented",
        String::from_utf8_lossy(&items[0].message)
    );
    // The value is still not applied.
    assert_eq!(links_before, cfg.link.links.len());
}

// No reference test. LEDGER S74-F7: the reference skips `finalize`'s whole
// default command / working-directory lookup on wasm and in tests
// (`Config.zig:4737-4739`); the port gated it on `test` only, so on wasm
// an embedder-supplied environ (`rustty_global::environ::set_environ`)
// with `TERM_PROGRAM` and `SHELL` set `command` (ADJUDICATION.md S74-F7).
#[test]
fn finalize_default_lookup_is_skipped_on_wasm_and_in_tests() {
    assert!(finalize_looks_up_defaults(false, false));
    assert!(
        !finalize_looks_up_defaults(true, false),
        "wasm must skip it"
    );
    assert!(
        !finalize_looks_up_defaults(false, true),
        "tests must skip it"
    );
    assert!(!finalize_looks_up_defaults(true, true));
}

// No reference test. LEDGER S74-F6: the Windows arm's `try
// internal_os.home(...)` returns the error from `finalize`; only a `null`
// home falls back to `.inherit`.
#[test]
fn windows_home_lookup_error_propagates() {
    assert!(matches!(
        windows_home_working_directory(Ok(Some(b"C:\\Users\\me"))),
        Ok(WorkingDirectory::Path(p)) if p == b"C:\\Users\\me"
    ));
    assert!(matches!(
        windows_home_working_directory(Ok(None)),
        Ok(WorkingDirectory::Inherit)
    ));
    let err = windows_home_working_directory(Err(rustty_os::homedir::Error::BufferTooSmall));
    assert!(err.is_err(), "the home() error must propagate, got {err:?}");
}

/// Every diagnostic message in `cfg`, as text.
fn diagnostic_messages(cfg: &Config) -> Vec<String> {
    cfg._diagnostics
        .items()
        .iter()
        .map(|d| String::from_utf8_lossy(&d.message).into_owned())
        .collect()
}

// No reference test. LEDGER S74-F4: `loadRecursiveFiles` formats the open
// error with `{}` (`error.<Name>`) and a non-file with `@tagName(stat.kind)`
// (`Config.zig:4413-4448`). MEASURED on zig 0.16.0: `openFileAbsolute` of an
// unreadable file is `error.AccessDenied`, of `f/x` with `f` a regular file
// `error.NotDir`; `/dev/null` is a `character_device`.
#[cfg(unix)]
#[test]
fn config_file_diagnostics_use_zig_error_and_kind_names() {
    use std::os::unix::fs::PermissionsExt;

    let mut td = TempDir::init().unwrap();
    td.dir().writeFile("locked", b"font-size = 12\n").unwrap();
    td.dir().writeFile("plain", b"font-size = 12\n").unwrap();
    let locked = td.dir().realpath("locked").unwrap();
    let plain = td.dir().realpath("plain").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let locked = locked.to_str().unwrap().to_string();
    let not_dir = format!("{}/x", plain.to_str().unwrap());

    let mut cfg = Config::default_config();
    let args = [
        format!("--config-file={locked}"),
        format!("--config-file={not_dir}"),
        "--config-file=/dev/null".to_string(),
    ];
    let arg_refs: Vec<&[u8]> = args.iter().map(|a| a.as_bytes()).collect();
    let mut it = TestIterator::new(&arg_refs);
    cfg.load_iter(&mut it).unwrap();
    cfg.load_recursive_files().unwrap();
    let messages = diagnostic_messages(&cfg);

    // R14: the AccessDenied arm needs a process without read access to a
    // 0o000 file, which every CI lane and developer checkout has (only
    // root has it). There is no skip: under root this fails, naming why.
    assert!(
        std::fs::File::open(&locked).is_err(),
        "this test must run without root: a 0o000 file was readable, so the \
             AccessDenied arm cannot be reached"
    );
    assert!(
        messages.contains(&format!(
            "error opening config-file {locked}: error.AccessDenied"
        )),
        "{messages:?}"
    );
    assert!(
        messages.contains(&format!(
            "error opening config-file {not_dir}: error.NotDir"
        )),
        "{messages:?}"
    );
    assert!(
        messages.contains(
            &"config-file /dev/null: not reading because file type is character_device".to_string()
        ),
        "{messages:?}"
    );
    td.deinit();
}
