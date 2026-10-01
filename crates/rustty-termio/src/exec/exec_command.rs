//! PORTS: ghostty src/termio/Exec.zig — `execCommand` and its `passwdpkg` parameter.

use super::*;

/// PORT NOTE (Rust-only glue): the `comptime passwdpkg: type` parameter of
/// `execCommand` — "expected to have a single function `get(Allocator)` that
/// returns a passwd entry" — modeled as a trait with an inferred-error-set
/// associated type (the tests substitute their own impls exactly as the
/// reference passes anonymous structs). The `Allocator` param collapses.
pub trait PasswdPkg {
    // PORT NOTE (task-14e-1): `Debug` is required by the restored
    // `Exec.zig:1853` `log.warn("failed to read passwd, ... err={}", .{err})` —
    // the reference prints the caught error value, so the associated error type
    // has to be renderable. Every implementor's error already derives it.
    type Error: core::fmt::Debug;
    fn get() -> Result<rustty_os::passwd::Entry, Self::Error>;
}

/// The real `internal_os.passwd` package (the production argument).
pub struct OsPasswd;

impl PasswdPkg for OsPasswd {
    type Error = rustty_os::passwd::Error;
    fn get() -> Result<rustty_os::passwd::Entry, Self::Error> {
        #[cfg(not(windows))]
        return rustty_os::passwd::get();
        // `passwd.get`: `if (builtin.os.tag == .windows) @compileError("passwd is
        // not available on windows");`. Its only caller is `execCommand`'s
        // comptime-Darwin branch, so Zig never analyzes it on Windows; here the
        // body must compile, and it is unreachable by construction (R7).
        #[cfg(windows)]
        unreachable!("passwd is not available on windows");
    }
}

/// Builds the argv array for the process we should exec for the
/// configured command. This isn't as straightforward as it seems since
/// we deal with shell-wrapping, macOS login shells, etc.
///
/// The passwdpkg comptime argument is expected to have a single function
/// `get(Allocator)` that returns a passwd entry. This is used by macOS
/// to determine the username and home directory for the login shell.
/// It is unused on other platforms.
///
/// Memory ownership:
///
/// The allocator should be an arena, since the returned value may or
/// may not be allocated and args may or may not be allocated (or copied).
/// Pointers in the return value may point to pointers in the command
/// struct.
//
// fn execCommand(
//     alloc: Allocator,
//     command: configpkg.Command,
//     comptime passwdpkg: type,
// ) (Allocator.Error || error{SystemError})![]const [:0]const u8 {
//
// PORT NOTE: the returned `[]const [:0]const u8` is the owned `Vec<CString>`
// (each element carries the `[:0]` sentinel; the arena collapses).
//
// (Windows: the shell arm's `.windows` block returns, as the reference's
// comptime branch does, so the `/bin/sh -c` tail is unreachable there.)
#[cfg_attr(windows, allow(unreachable_code))]
pub(super) fn execCommand<P: PasswdPkg>(
    command: &configpkg::command::Command,
) -> Result<Vec<CString>, ExecCommandError> {
    // PORT NOTE: the reference's `comptime passwdpkg: type` is read only by the
    // `darwin:` block below, which is a real `cfg` here; the parameter stays in
    // the signature on every target (the reference's does, and the callers —
    // including the ported tests — pass it), so name it off Darwin rather than
    // letting it read as extraneous.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let _ = core::marker::PhantomData::<P>;

    // If we're on macOS, we have to use `login(1)` to get all of
    // the proper environment variables set, a login shell, and proper
    // hushlogin behavior.
    // if (comptime builtin.target.os.tag.isDarwin()) darwin: {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    'darwin: {
        // const passwd = passwdpkg.get(alloc) catch |err| {
        //     log.warn("failed to read passwd, not using a login shell err={}", .{err});
        //     break :darwin;
        // };
        let passwd = match P::get() {
            Ok(p) => p,
            Err(err) => {
                // log.warn("failed to read passwd, not using a login shell err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to read passwd, not using a login shell err={err:?}"
                );
                break 'darwin;
            }
        };

        // const username = passwd.name orelse {
        //     log.warn("failed to get username, not using a login shell", .{});
        //     break :darwin;
        // };
        let username = match passwd.name {
            Some(n) => n,
            None => {
                // log.warn("failed to get username, not using a login shell", .{});
                log::warn!(target: LOG_SCOPE, "failed to get username, not using a login shell");
                break 'darwin;
            }
        };

        // const hush = if (passwd.home) |home| hush: { ... } else false;
        let hush = if let Some(home) = &passwd.home {
            'hush: {
                // var dir = std.fs.openDirAbsolute(home, .{}) catch |err| {
                //     log.warn(
                //         "failed to open home dir, not checking for hushlogin err={}",
                //         .{err},
                //     );
                //     break :hush false;
                // };
                // defer dir.close();
                let home_path = std::path::Path::new(os_str_from_bytes(home.to_bytes()));
                match std::fs::read_dir(home_path) {
                    Ok(dir) => drop(dir), // defer dir.close();
                    Err(err) => {
                        // log.warn(
                        //     "failed to open home dir, not checking for hushlogin err={}",
                        //     .{err},
                        // );
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to open home dir, not checking for hushlogin err={err:?}"
                        );
                        break 'hush false;
                    }
                }

                // break :hush if (dir.access(".hushlogin", .{})) true else |_| false;
                std::fs::metadata(home_path.join(".hushlogin")).is_ok()
            }
        } else {
            false
        };

        // If we made it this far we're going to start building
        // the actual command.
        // var args: std.ArrayList([:0]const u8) = try .initCapacity(
        //     alloc,
        //
        //     // This capacity is chosen based on what we'd need to
        //     // execute a shell command (very common). We can/will
        //     // grow if necessary for a longer command (uncommon).
        //     9,
        // );
        // defer args.deinit(alloc);
        let mut args: Vec<CString> = Vec::with_capacity(9);

        // The reason for executing login this way is unclear. This
        // comment will attempt to explain but prepare for a truly
        // unhinged reality.
        //
        // The first major issue is that on macOS, a lot of users
        // put shell configurations in ~/.bash_profile instead of
        // ~/.bashrc (or equivalent for another shell). This file is only
        // loaded for a login shell so macOS users expect all their terminals
        // to be login shells. No other platform behaves this way and its
        // totally braindead but somehow the entire dev community on
        // macOS has cargo culted their way to this reality so we have to
        // do it...
        //
        // To get a login shell, you COULD just prepend argv0 with a `-`
        // but that doesn't fully work because `getlogin()` C API will
        // return the wrong value, SHELL won't be set, and various
        // other login behaviors that macOS users expect.
        //
        // The proper way is to use `login(1)`. But login(1) forces
        // the working directory to change to the home directory,
        // which we may not want. If we specify "-l" then we can avoid
        // this behavior but now the shell isn't a login shell.
        //
        // There is another issue: `login(1)` on macOS 14.3 and earlier
        // checked for ".hushlogin" in the working directory. This means
        // that if we specify "-l" then we won't get hushlogin honored
        // if its in the home directory (which is standard). To get
        // around this, we check for hushlogin ourselves and if present
        // specify the "-q" flag to login(1).
        //
        // So to get all the behaviors we want, we specify "-l" but
        // execute "bash" (which is built-in to macOS). We then use
        // the bash builtin "exec" to replace the process with a login
        // shell ("-l" on exec) with the command we really want.
        //
        // We use "bash" instead of other shells that ship with macOS
        // because as of macOS Sonoma, we found with a microbenchmark
        // that bash can `exec` into the desired command ~2x faster
        // than zsh.
        //
        // To figure out a lot of this logic I read the login.c
        // source code in the OSS distribution Apple provides for
        // macOS.
        //
        // Awesome.
        // try args.append(alloc, "/usr/bin/login");
        args.push(c"/usr/bin/login".to_owned());
        // if (hush) try args.append(alloc, "-q");
        if hush {
            args.push(c"-q".to_owned());
        }
        // try args.append(alloc, "-flp");
        args.push(c"-flp".to_owned());
        // try args.append(alloc, username);
        args.push(username);

        // switch (command) {
        match command {
            // Direct args can be passed directly to login, since
            // login uses execvp we don't need to worry about PATH
            // searching.
            // .direct => |v| try args.appendSlice(alloc, v),
            configpkg::command::Command::Direct(v) => {
                for arg in v {
                    args.push(cstring(arg));
                }
            }

            // .shell => |v| {
            configpkg::command::Command::Shell(v) => {
                // Use "exec" to replace the bash process with
                // our intended command so we don't have a parent
                // process hanging around.
                // const cmd = try std.fmt.allocPrintSentinel(
                //     alloc,
                //     "exec -l {s}",
                //     .{v},
                //     0,
                // );
                let mut cmd_bytes = b"exec -l ".to_vec();
                cmd_bytes.extend_from_slice(v);
                let cmd = cstring(&cmd_bytes);

                // We execute bash with "--noprofile --norc" so that it doesn't
                // load startup files so that (1) our shell integration doesn't
                // break and (2) user configuration doesn't mess this process
                // up.
                // try args.append(alloc, "/bin/bash");
                args.push(c"/bin/bash".to_owned());
                // try args.append(alloc, "--noprofile");
                args.push(c"--noprofile".to_owned());
                // try args.append(alloc, "--norc");
                args.push(c"--norc".to_owned());
                // try args.append(alloc, "-c");
                args.push(c"-c".to_owned());
                // try args.append(alloc, cmd);
                args.push(cmd);
            }
        }

        // return try args.toOwnedSlice(alloc);
        return Ok(args);
    }

    // return switch (command) {
    match command {
        // We need to clone the command since there's no guarantee the config remains valid.
        // .direct => |_| (try command.clone(alloc)).direct,
        configpkg::command::Command::Direct(_) => match command.clone() {
            configpkg::command::Command::Direct(v) => {
                Ok(v.iter().map(|arg| cstring(arg)).collect())
            }
            configpkg::command::Command::Shell(_) => unreachable!(),
        },

        // .shell => |v| shell: {
        configpkg::command::Command::Shell(v) => {
            // var args: std.ArrayList([:0]const u8) = try .initCapacity(alloc, 4);
            // defer args.deinit(alloc);
            let mut args: Vec<CString> = Vec::with_capacity(4);

            // if (comptime builtin.os.tag == .windows) {
            #[cfg(windows)]
            {
                // On Windows we run the shell value directly rather than
                // wrapping in `cmd.exe /C <shell>`. An intermediate cmd
                // process is wasteful for the common case (`wsl ~`,
                // `pwsh -NoLogo`, etc.) and has visible side effects
                // (extra process in the tree, per-process cmd AutoRun
                // state not reaching the user's actual shell).
                //
                // Values with arguments are split on whitespace. This
                // does not honor Windows CLI quoting rules; users who
                // need quoted arguments should use the direct command
                // form, which takes an argv array as-is.
                //
                // Note we don't free any of the memory below since it is
                // allocated in the arena.
                // if (std.mem.indexOfAny(u8, v, " \t") == null) {
                if !v.iter().any(|&b| b == b' ' || b == b'\t') {
                    // No arguments. If the shell is literally "cmd.exe"
                    // (the default), resolve via %COMSPEC% which is the
                    // documented path to the current command processor.
                    // Other values are passed as-is and resolved by
                    // `internal_os.path.expand` in Command.startWindows.
                    // const argv0 = if (std.ascii.eqlIgnoreCase(v, "cmd.exe"))
                    //     std.process.getEnvVarOwned(alloc, "COMSPEC") catch
                    //         try alloc.dupe(u8, v)
                    // else
                    //     try alloc.dupe(u8, v);
                    let argv0: Vec<u8> = if v.eq_ignore_ascii_case(b"cmd.exe") {
                        match std::env::var("COMSPEC") {
                            Ok(comspec) => comspec.into_bytes(),
                            Err(_) => v.clone(),
                        }
                    } else {
                        v.clone()
                    };
                    // try args.append(alloc, try alloc.dupeZ(u8, argv0));
                    args.push(cstring(&argv0));
                } else {
                    // var it = std.mem.tokenizeAny(u8, v, " \t");
                    // while (it.next()) |tok| {
                    //     try args.append(alloc, try alloc.dupeZ(u8, tok));
                    // }
                    for tok in v
                        .split(|&b| b == b' ' || b == b'\t')
                        .filter(|s| !s.is_empty())
                    {
                        args.push(cstring(tok));
                    }
                }
                // break :shell try args.toOwnedSlice(alloc);
                return Ok(args);
            }
            #[cfg(not(windows))]
            {
                // We run our shell wrapped in `/bin/sh` so that we don't have
                // to parse the command line ourselves if it has arguments.
                // Additionally, some environments (NixOS, I found) use /bin/sh
                // to setup some environment variables that are important to
                // have set.
                // try args.append(alloc, "/bin/sh");
                args.push(c"/bin/sh".to_owned());
                // if (internal_os.isFlatpak()) try args.append(alloc, "-l");
                if rustty_os::flatpak::isFlatpak() {
                    args.push(c"-l".to_owned());
                }
                // try args.append(alloc, "-c");
                args.push(c"-c".to_owned());
            }

            // try args.append(alloc, v);
            args.push(cstring(v));
            // break :shell try args.toOwnedSlice(alloc);
            Ok(args)
        }
    }
}
