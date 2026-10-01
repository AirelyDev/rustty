//! PORTS: ghostty src/termio/Exec.zig — the `execCommand` tests, plus the `appendEnv` tests that moved
//! with it.

use super::*;

// ---- MOVED FROM `rustty-os`'s `env.rs` (P3-W7 T2 / area-E `E13`) --------
// Upstream `e8525c0fd` moved `appendEnv`/`appendEnvAlways` into
// `src/termio/Exec.zig` when it deleted `src/os/env.zig`; their two tests
// move with them. Re-typed on bytes with the functions.

// test "appendEnv empty" {
#[test]
#[allow(non_snake_case)]
fn appendEnv_empty() {
    let result = appendEnv(b"", b"foo");
    assert_eq!(result, b"foo");
}

// test "appendEnv existing" {
#[test]
#[allow(non_snake_case)]
fn appendEnv_existing() {
    let result = appendEnv(b"a:b", b"foo");
    if cfg!(windows) {
        assert_eq!(result, b"a:b;foo");
    } else {
        assert_eq!(result, b"a:b:foo");
    }
}

/// NEGATIVE CONTROL (P3-W7 T2 Step 4): a **non-UTF-8** PATH component must
/// round-trip through `appendEnv`/`appendEnvAlways` byte-exactly. Reverting
/// either signature to `&str`/`String` makes this fail to COMPILE, which is
/// the strongest control available for a type-level fix; and the old
/// `String`-typed body could not even represent this input.
#[test]
fn append_env_preserves_non_utf8_path_component() {
    // A PATH entry with a lone 0xFF byte — legal on POSIX, invalid UTF-8.
    let current: &[u8] = &[b'/', b'u', b's', b'r', b'/', 0xff, b'x'];
    let added: &[u8] = &[b'/', 0xfe, b'o', b'p', b't'];

    let joined = appendEnv(current, added);
    assert_eq!(joined[..current.len()], *current);
    assert_eq!(
        joined[current.len()],
        crate::shell_integration::PATH_LIST_DELIMITER
    );
    assert_eq!(joined[current.len() + 1..], *added);
    assert!(
        std::str::from_utf8(&joined).is_err(),
        "input must be non-UTF-8"
    );

    // The always-form must keep the empty prefix (MANPATH semantics).
    let always = appendEnvAlways(b"", added);
    assert_eq!(always[0], crate::shell_integration::PATH_LIST_DELIMITER);
    assert_eq!(always[1..], *added);
}
// ---- end moved block ---------------------------------------------------

// test "execCommand darwin: shell command" {
#[test]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios")),
    ignore = "the body only exists on macOS/iOS (R14)"
)]
fn exec_command_darwin_shell_command() {
    // if (comptime !builtin.os.tag.isDarwin()) return error.SkipZigTest;
    // (PORT NOTE: cfg-gating the body is the SkipZigTest analogue — stage-3
    // Command / F3 DirectWrite precedent.)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // var arena = ArenaAllocator.init(testing.allocator);
        // defer arena.deinit();
        // const alloc = arena.allocator();

        // const result = try execCommand(alloc, .{ .shell = "foo bar baz" }, struct {
        //     fn get(_: Allocator) !PasswdEntry {
        //         return .{
        //             .name = "testuser",
        //         };
        //     }
        // });
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                Ok(PasswdEntry {
                    name: Some(c"testuser".to_owned()),
                    shell: None,
                    home: None,
                })
            }
        }
        let result =
            execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"foo bar baz".to_vec()))
                .unwrap();

        // try testing.expectEqual(8, result.len);
        assert_eq!(8, result.len());
        // try testing.expectEqualStrings(result[0], "/usr/bin/login");
        assert_eq!(result[0].to_bytes(), b"/usr/bin/login");
        // try testing.expectEqualStrings(result[1], "-flp");
        assert_eq!(result[1].to_bytes(), b"-flp");
        // try testing.expectEqualStrings(result[2], "testuser");
        assert_eq!(result[2].to_bytes(), b"testuser");
        // try testing.expectEqualStrings(result[3], "/bin/bash");
        assert_eq!(result[3].to_bytes(), b"/bin/bash");
        // try testing.expectEqualStrings(result[4], "--noprofile");
        assert_eq!(result[4].to_bytes(), b"--noprofile");
        // try testing.expectEqualStrings(result[5], "--norc");
        assert_eq!(result[5].to_bytes(), b"--norc");
        // try testing.expectEqualStrings(result[6], "-c");
        assert_eq!(result[6].to_bytes(), b"-c");
        // try testing.expectEqualStrings(result[7], "exec -l foo bar baz");
        assert_eq!(result[7].to_bytes(), b"exec -l foo bar baz");
    }
}

// test "execCommand darwin: direct command" {
#[test]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios")),
    ignore = "the body only exists on macOS/iOS (R14)"
)]
fn exec_command_darwin_direct_command() {
    // if (comptime !builtin.os.tag.isDarwin()) return error.SkipZigTest;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // const result = try execCommand(alloc, .{ .direct = &.{
        //     "foo",
        //     "bar baz",
        // } }, struct {
        //     fn get(_: Allocator) !PasswdEntry {
        //         return .{
        //             .name = "testuser",
        //         };
        //     }
        // });
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                Ok(PasswdEntry {
                    name: Some(c"testuser".to_owned()),
                    shell: None,
                    home: None,
                })
            }
        }
        let result = execCommand::<TestPasswd>(&configpkg::command::Command::Direct(vec![
            b"foo".to_vec(),
            b"bar baz".to_vec(),
        ]))
        .unwrap();

        // try testing.expectEqual(5, result.len);
        assert_eq!(5, result.len());
        // try testing.expectEqualStrings(result[0], "/usr/bin/login");
        assert_eq!(result[0].to_bytes(), b"/usr/bin/login");
        // try testing.expectEqualStrings(result[1], "-flp");
        assert_eq!(result[1].to_bytes(), b"-flp");
        // try testing.expectEqualStrings(result[2], "testuser");
        assert_eq!(result[2].to_bytes(), b"testuser");
        // try testing.expectEqualStrings(result[3], "foo");
        assert_eq!(result[3].to_bytes(), b"foo");
        // try testing.expectEqualStrings(result[4], "bar baz");
        assert_eq!(result[4].to_bytes(), b"bar baz");
    }
}

// test "execCommand: shell command, empty passwd" {
#[test]
#[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
fn exec_command_shell_command_empty_passwd() {
    // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    #[cfg(not(windows))]
    {
        // const result = try execCommand(
        //     alloc,
        //     .{ .shell = "foo bar baz" },
        //     struct {
        //         fn get(_: Allocator) !PasswdEntry {
        //             // Empty passwd entry means we can't construct a macOS
        //             // login command and falls back to POSIX behavior.
        //             return .{};
        //         }
        //     },
        // );
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                // Empty passwd entry means we can't construct a macOS
                // login command and falls back to POSIX behavior.
                Ok(PasswdEntry::default())
            }
        }
        let result =
            execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"foo bar baz".to_vec()))
                .unwrap();

        // try testing.expectEqual(3, result.len);
        assert_eq!(3, result.len());
        // try testing.expectEqualStrings(result[0], "/bin/sh");
        assert_eq!(result[0].to_bytes(), b"/bin/sh");
        // try testing.expectEqualStrings(result[1], "-c");
        assert_eq!(result[1].to_bytes(), b"-c");
        // try testing.expectEqualStrings(result[2], "foo bar baz");
        assert_eq!(result[2].to_bytes(), b"foo bar baz");
    }
}

// test "execCommand: shell command, error passwd" {
#[test]
#[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
fn exec_command_shell_command_error_passwd() {
    // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    #[cfg(not(windows))]
    {
        // const result = try execCommand(
        //     alloc,
        //     .{ .shell = "foo bar baz" },
        //     struct {
        //         fn get(_: Allocator) !PasswdEntry {
        //             // Failed passwd entry means we can't construct a macOS
        //             // login command and falls back to POSIX behavior.
        //             return error.Fail;
        //         }
        //     },
        // );
        #[derive(Debug)]
        struct Fail;
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = Fail;
            fn get() -> Result<PasswdEntry, Self::Error> {
                // Failed passwd entry means we can't construct a macOS
                // login command and falls back to POSIX behavior.
                Err(Fail) // return error.Fail;
            }
        }
        let result =
            execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"foo bar baz".to_vec()))
                .unwrap();

        // try testing.expectEqual(3, result.len);
        assert_eq!(3, result.len());
        // try testing.expectEqualStrings(result[0], "/bin/sh");
        assert_eq!(result[0].to_bytes(), b"/bin/sh");
        // try testing.expectEqualStrings(result[1], "-c");
        assert_eq!(result[1].to_bytes(), b"-c");
        // try testing.expectEqualStrings(result[2], "foo bar baz");
        assert_eq!(result[2].to_bytes(), b"foo bar baz");
    }
}

// test "execCommand: direct command, error passwd" {
#[test]
#[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
fn exec_command_direct_command_error_passwd() {
    // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    #[cfg(not(windows))]
    {
        // const result = try execCommand(alloc, .{
        //     .direct = &.{
        //         "foo",
        //         "bar baz",
        //     },
        // }, struct {
        //     fn get(_: Allocator) !PasswdEntry {
        //         // Failed passwd entry means we can't construct a macOS
        //         // login command and falls back to POSIX behavior.
        //         return error.Fail;
        //     }
        // });
        #[derive(Debug)]
        struct Fail;
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = Fail;
            fn get() -> Result<PasswdEntry, Self::Error> {
                // Failed passwd entry means we can't construct a macOS
                // login command and falls back to POSIX behavior.
                Err(Fail)
            }
        }
        let result = execCommand::<TestPasswd>(&configpkg::command::Command::Direct(vec![
            b"foo".to_vec(),
            b"bar baz".to_vec(),
        ]))
        .unwrap();

        // try testing.expectEqual(2, result.len);
        assert_eq!(2, result.len());
        // try testing.expectEqualStrings(result[0], "foo");
        assert_eq!(result[0].to_bytes(), b"foo");
        // try testing.expectEqualStrings(result[1], "bar baz");
        assert_eq!(result[1].to_bytes(), b"bar baz");
    }
}

// test "execCommand: direct command, config freed" {
#[test]
#[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
fn exec_command_direct_command_config_freed() {
    // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    #[cfg(not(windows))]
    {
        // var command_arena = ArenaAllocator.init(testing.allocator);
        // const command_alloc = command_arena.allocator();
        // const command = try (configpkg.Command{
        //     .direct = &.{
        //         "foo",
        //         "bar baz",
        //     },
        // }).clone(command_alloc);
        let command =
            configpkg::command::Command::Direct(vec![b"foo".to_vec(), b"bar baz".to_vec()]).clone();

        #[derive(Debug)]
        struct Fail;
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = Fail;
            fn get() -> Result<PasswdEntry, Self::Error> {
                // Failed passwd entry means we can't construct a macOS
                // login command and falls back to POSIX behavior.
                Err(Fail)
            }
        }
        // const result = try execCommand(alloc, command, struct { ... });
        let result = execCommand::<TestPasswd>(&command).unwrap();

        // command_arena.deinit();
        //
        // (PORT NOTE: the reference frees the source command BEFORE reading the
        // result, proving execCommand cloned it; Rust's drop is the same
        // proof.)
        drop(command);

        // try testing.expectEqual(2, result.len);
        assert_eq!(2, result.len());
        // try testing.expectEqualStrings(result[0], "foo");
        assert_eq!(result[0].to_bytes(), b"foo");
        // try testing.expectEqualStrings(result[1], "bar baz");
        assert_eq!(result[1].to_bytes(), b"bar baz");
    }
}

// test "execCommand windows: bare cmd.exe resolves via COMSPEC" {
#[test]
#[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
fn exec_command_windows_bare_cmd_exe_resolves_via_comspec() {
    // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
    #[cfg(windows)]
    {
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                Ok(PasswdEntry::default()) // return .{};
            }
        }
        // const result = try execCommand(alloc, .{ .shell = "cmd.exe" }, struct { ... });
        let result =
            execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"cmd.exe".to_vec()))
                .unwrap();

        // try testing.expectEqual(1, result.len);
        assert_eq!(1, result.len());

        // Expect COMSPEC if available, otherwise the documented fallback.
        // const expected = std.process.getEnvVarOwned(alloc, "COMSPEC") catch
        //     try alloc.dupe(u8, "C:\\Windows\\System32\\cmd.exe");
        let expected = std::env::var("COMSPEC")
            .unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".to_string());
        // try testing.expectEqualStrings(expected, result[0]);
        assert_eq!(expected.as_bytes(), result[0].to_bytes());
    }
}

// test "execCommand windows: bare non-cmd shell is passed through" {
#[test]
#[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
fn exec_command_windows_bare_non_cmd_shell_is_passed_through() {
    // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
    #[cfg(windows)]
    {
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                Ok(PasswdEntry::default())
            }
        }
        // const result = try execCommand(alloc, .{ .shell = "pwsh.exe" }, struct { ... });
        let result =
            execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"pwsh.exe".to_vec()))
                .unwrap();

        // try testing.expectEqual(1, result.len);
        assert_eq!(1, result.len());
        // try testing.expectEqualStrings("pwsh.exe", result[0]);
        assert_eq!(b"pwsh.exe", result[0].to_bytes());
    }
}

// test "execCommand windows: shell with args is split on whitespace" {
#[test]
#[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
fn exec_command_windows_shell_with_args_is_split_on_whitespace() {
    // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
    #[cfg(windows)]
    {
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                Ok(PasswdEntry::default())
            }
        }
        // const result = try execCommand(alloc, .{ .shell = "wsl ~" }, struct { ... });
        let result =
            execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"wsl ~".to_vec()))
                .unwrap();

        // try testing.expectEqual(2, result.len);
        assert_eq!(2, result.len());
        // try testing.expectEqualStrings("wsl", result[0]);
        assert_eq!(b"wsl", result[0].to_bytes());
        // try testing.expectEqualStrings("~", result[1]);
        assert_eq!(b"~", result[1].to_bytes());
    }
}

// test "execCommand windows: direct command is passed through unchanged" {
#[test]
#[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
fn exec_command_windows_direct_command_is_passed_through_unchanged() {
    // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
    #[cfg(windows)]
    {
        struct TestPasswd;
        impl PasswdPkg for TestPasswd {
            type Error = std::convert::Infallible;
            fn get() -> Result<PasswdEntry, Self::Error> {
                Ok(PasswdEntry::default())
            }
        }
        // const result = try execCommand(alloc, .{ .direct = &.{
        //     "C:\\tools\\foo.exe",
        //     "arg with spaces",
        // } }, struct { ... });
        let result = execCommand::<TestPasswd>(&configpkg::command::Command::Direct(vec![
            b"C:\\tools\\foo.exe".to_vec(),
            b"arg with spaces".to_vec(),
        ]))
        .unwrap();

        // try testing.expectEqual(2, result.len);
        assert_eq!(2, result.len());
        // try testing.expectEqualStrings("C:\\tools\\foo.exe", result[0]);
        assert_eq!(b"C:\\tools\\foo.exe", result[0].to_bytes());
        // try testing.expectEqualStrings("arg with spaces", result[1]);
        assert_eq!(b"arg with spaces", result[1].to_bytes());
    }
}

/// B3. A NUL in `command` / `initial-command`. The reference copies the
/// value with `dupeZ` and `execve` reads argv up to the first NUL
/// (measured), so the port cuts it there instead of panicking.
#[test]
fn exec_command_cuts_a_nul_argument_where_execve_reads_it() {
    struct TestPasswd;
    impl PasswdPkg for TestPasswd {
        type Error = std::convert::Infallible;
        fn get() -> Result<PasswdEntry, Self::Error> {
            Ok(PasswdEntry {
                name: Some(c"testuser".to_owned()),
                shell: None,
                home: None,
            })
        }
    }

    // Through the config parser, which keeps the NUL in both forms.
    let mut cfg = configpkg::Config::default_config();
    let mut iter = rustty_args::args::LineIterator::init(
        b"command = direct:/bin/echo a\x00b\ninitial-command = echo hi\x00; echo AFTER\n",
    );
    cfg.load_iter(&mut iter).expect("load_iter");
    let direct = cfg.command.clone().expect("command");
    let shell = cfg.initial_command.clone().expect("initial-command");
    match &direct {
        configpkg::command::Command::Direct(v) => assert_eq!(
            vec![b"/bin/echo".to_vec(), b"a\x00b".to_vec()],
            *v,
            "the config parser keeps the NUL byte"
        ),
        configpkg::command::Command::Shell(_) => panic!("expected a direct command"),
    }

    let direct_args = execCommand::<TestPasswd>(&direct).expect("execCommand direct");
    let shell_args = execCommand::<TestPasswd>(&shell).expect("execCommand shell");
    let direct_last: Vec<&[u8]> = direct_args
        .iter()
        .rev()
        .take(2)
        .map(|c| c.to_bytes())
        .collect();
    assert_eq!(
        vec![b"a".as_slice(), b"/bin/echo".as_slice()],
        direct_last,
        "argv is cut at the NUL, as execve reads it"
    );
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    assert_eq!(
        b"exec -l echo hi".as_slice(),
        shell_args.last().expect("a shell arg").to_bytes()
    );
    #[cfg(not(any(target_os = "macos", target_os = "ios", windows)))]
    assert_eq!(
        b"echo hi".as_slice(),
        shell_args.last().expect("a shell arg").to_bytes()
    );
    // Windows: the shell command is tokenized on spaces and tabs
    // (`std.mem.tokenizeAny(u8, v, " \t")`), not wrapped in `/bin/sh -c`,
    // so the NUL cuts only its own token, `hi\x00;`.
    #[cfg(windows)]
    assert_eq!(
        vec![
            b"echo".as_slice(),
            b"hi".as_slice(),
            b"echo".as_slice(),
            b"AFTER".as_slice()
        ],
        shell_args.iter().map(|c| c.to_bytes()).collect::<Vec<_>>()
    );
}
