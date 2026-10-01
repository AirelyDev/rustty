//! PORTS: ghostty src/termio/Exec.zig — the `Subprocess` struct (`init`, `start`, `childPreExec`,
//! `externalExit`, `stop`, `resize`, `getForegroundPid`, `getTtyName`) and
//! its `start` return shape.

use super::*;

// const Subprocess = struct {
//
// PORT NOTE: reference file-private; `pub` because it appears in the `Exec`
// field shape (Zig-private widening rule).
// [DOUBLE-FREE FIX] Ownership of the arena-collapsed C-string storage (`args`,
// `cwd`; `env` too) mirrors the reference `Subprocess` (Exec.zig:877-884): the
// storage is freed EXACTLY ONCE, inside `deinit` (the reference's
// `arena.deinit()`), and `deinit` then leaves every owned field EMPTY (the
// Rust-safe analogue of `self.* = undefined`). Freeing in `deinit` — rather
// than deferring to the value's Drop — is what keeps the free single even when a
// caller both `deinit()`s the value AND later drops it (an apprt surface
// finalize freeing its heap box by dropping it). This pairs with the in-place
// (`&mut self`) `Termio::deinit`, so deinit operates on the SAME object that is
// later dropped (never a bitwise `ptr::read` duplicate, which would defeat the
// empty-after-deinit invariant).
pub struct Subprocess {
    // const c = @import("posix_c");
    // (fe9cf6a26 replaced the errno.h/signal.h/unistd.h cImport with the
    // shared posix_c translate-c module; same headers, refresh only.)
    // (PORT NOTE: the `libc` binding — the SAME system headers; R3.)

    // arena: std.heap.ArenaAllocator,
    // (PORT NOTE: the arena collapses into the owned fields below — crash-tier
    // precedent.)
    // cwd: ?[:0]const u8,
    //
    // PORT NOTE (task R13-P, N50): raw bytes, not `CString`. The reference's
    // `dupeZ` copy keeps every byte of the configured or inherited directory,
    // an interior NUL included (OSC 7 decodes `%00` into one). `start` asks
    // `access` about those bytes and takes its `cannot access cwd, ignoring`
    // arm, the same arm Zig's `pathToPosix` `error.BadPathName` takes.
    pub cwd: Option<Vec<u8>>,
    // env: ?EnvMap,
    pub env: Option<EnvMap>,
    // args: []const [:0]const u8,
    pub args: Vec<CString>,
    pub grid_size: GridSize,
    pub screen_size: ScreenSize,
    // pty: ?Pty = null,
    pub pty: Option<Pty>,
    // process: ?Process = null,
    pub process: Option<Process>,

    pub rt_pre_exec_info: RtPreExecInfo,
    pub rt_post_fork_info: RtPostForkInfo,
}

/// Union that represents the running process type.
//
// const Process = union(enum) {
//
// PORT NOTE: the `fork_exec: Command` payload stores the reference's
// arena-pointer-backed Command by value; the Rust `Command<'static>` type
// carries the same shape (the deferred `start` is the only constructor — see
// its PORT NOTE).
pub enum Process {
    /// Standard POSIX fork/exec
    ForkExec(Command<'static>),

    /// Flatpak DBus command
    Flatpak(FlatpakHostCommand),
}

// const ArgsFormatter = struct {
//     args: []const [:0]const u8,
//
//     pub fn format(this: @This(), writer: *std.Io.Writer) std.Io.Writer.Error!void {
//         for (this.args, 0..) |a, i| {
//             if (i > 0) try writer.writeAll(", ");
//             try writer.print("`{s}`", .{a});
//         }
//     }
// };
//
// PORT NOTE: consumed only by `start`'s "starting command" debug log (the
// deferred body); ported as the Display impl for log parity.
// (dead_code: the only reference constructor is `start`'s debug log —
// deferred above.)
#[allow(dead_code)]
pub(crate) struct ArgsFormatter<'a> {
    pub args: &'a [CString],
}

impl core::fmt::Display for ArgsFormatter<'_> {
    fn fmt(&self, writer: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                writer.write_str(", ")?;
            }
            write!(writer, "`{}`", String::from_utf8_lossy(a.to_bytes()))?;
        }
        Ok(())
    }
}

// The reference's `os_pre_exec` callback (an anonymous `struct { fn callback(cmd:
// *Command) ?u8 } `). It runs /in the forked child/ after fork but before exec.
// Ported as a free fn matching `PreExecFn`; it recovers the owning `Subprocess`
// from `cmd.data` (the raw `*mut Subprocess` self-pointer stored by `start`) and
// dereferences it ONLY here in the child — where `fork` has copied the parent's
// address space, so the parent-side borrows are irrelevant (the raw-ptr
// container_of discipline, vtc-effects-trampoline-seam).
//
//     .os_pre_exec = switch (comptime builtin.os.tag) {
//         .windows => null,
//         else => f: {
//             const f = struct {
//                 fn callback(cmd: *Command) ?u8 {
//                     const sp = cmd.getData(Subprocess) orelse unreachable;
//                     sp.childPreExec() catch |err| log.err(
//                         "error initializing child: {}",
//                         .{err},
//                     );
//                     return null;
//                 }
//             };
//             break :f f.callback;
//         },
//     },
#[cfg(not(windows))]
fn subprocess_pre_exec(cmd: &mut Command) -> Option<u8> {
    // const sp = cmd.getData(Subprocess) orelse unreachable;
    let sp = cmd.getData::<Subprocess>().expect("getData(Subprocess)");
    // sp.childPreExec() catch |err| log.err("error initializing child: {}", .{err});
    if let Err(err) = unsafe { (*sp).childPreExec() } {
        // log.err("error initializing child: {}", .{err});
        //
        // PORT NOTE (async-signal safety) — see `log_err_child_raw` below.
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD101: the
        // reference's `log.err` here can deadlock the child on a lock another
        // thread held at the fork; this raw `write(2)` is the safe handling.
        // THIS RUNS IN THE FORKED CHILD, between `fork(2)` and `execve(2)`, so
        // the `log::` facade must never be used here. `rustty-apprt`'s
        // `gtk/pre_exec.rs` is the PRECEDENT this follows, not a case to
        // distinguish from: `pre_exec.zig:39` and `:58` call `log.err` through
        // the very same global `logFn` this site does, and that port renders
        // them as a raw `write(2)` for exactly this reason.
        log_err_child_raw(&err);
    }
    // return null;
    None
}

/// The FIFTH emission shape (`ADJUDICATION.md`, task-14d-3): `Exec.zig:1045`'s
/// `log.err("error initializing child: {}", .{err})`, hand-rendered and written
/// with a raw `write(2)` instead of going through the `log::` facade.
///
/// # Why the facade is forbidden here, and why fidelity does not require it
///
/// This is called from `subprocess_pre_exec`, the `os_pre_exec` hook, which
/// `Command::spawn` invokes IN THE FORKED CHILD after `fork(2)` returns 0 and
/// before `execve(2)`. Only the forking thread survives into the child, so any
/// lock held by another thread at the instant of the fork is held FOREVER in the
/// child. The parent here always has a renderer thread, an io thread and a
/// gather thread running. POSIX therefore permits only async-signal-safe
/// operations on this path.
///
/// `log::error!` violates that twice over. It formats through `core::fmt` into
/// the installed backend, and `rustty-core`'s `AppLogger` — the backend the real
/// app installs — both ALLOCATES (`logging.rs:61,76,97` build the rendered
/// string) and takes the process-wide STDERR LOCK (`logging.rs:99`). Either the
/// allocator's lock or stderr's can be held by a dead thread, and the child then
/// hangs with NO diagnostic at all — strictly worse than the failure this
/// diagnostic exists to report.
///
/// Fidelity here is owed to the reference's observable OUTPUT, not to its call
/// mechanism, and nothing is lost: the bytes below are exactly what `logFn`
/// would have written to the same fd. `rustty-apprt`'s `gtk/pre_exec.rs`
/// established this shape for the identical reference construct
/// (`pre_exec.zig:39,58` are also `log.err` through the same global `logFn`);
/// this is that precedent applied, not an exception to it.
///
/// The name is `log_*` on purpose: `G-LOGFILE`'s `discover_emitters` recognises
/// a `log_`-prefixed fn wrapping a `write!` as the fn-facade shape it already
/// knows (shape 3), so the call site is COUNTED rather than read as a dropped
/// diagnostic.
///
/// # The level token is `error`, not `err`
///
/// The prefix hand-renders `logFn`'s own output, so it must render it exactly.
/// Zig's `std.log.Level.asText` maps `.err` to the string **`"error"`**
/// (`lib/std/log.zig:45`), and this port agrees at
/// `rustty_core::logging::Level::as_text`. Two stages have now shipped `err(`
/// here; `log_err_child_raw_writes_exactly_what_logfn_would` captures the real
/// bytes off fd 2 rather than re-asserting the literal.
///
/// # No allocation below this line
///
/// The message is formatted into a STACK buffer through `core::fmt`, which does
/// not allocate, and the only argument is `crate::Error`'s derived `Debug`,
/// which does not allocate either. An over-long render is truncated rather than
/// grown. Then one `write(2)`, which is on POSIX's async-signal-safe list.
#[cfg(not(windows))]
pub(super) fn log_err_child_raw(err: &crate::Error) {
    /// A `core::fmt::Write` sink over a fixed stack buffer. Truncates; never
    /// allocates, never locks.
    struct StackBuf {
        buf: [u8; 256],
        len: usize,
    }

    impl core::fmt::Write for StackBuf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let room = self.buf.len() - self.len;
            let n = room.min(s.len());
            self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
            self.len += n;
            Ok(())
        }
    }

    use core::fmt::Write as _;
    let mut out = StackBuf {
        buf: [0u8; 256],
        len: 0,
    };
    // `logFn`'s rendering: `<level>(<scope>): <message>\n`. The scope is
    // `LOG_SCOPE` (`io_exec`), the same tag the facade sites in this file pass
    // as `target:`.
    // (`writeln!`, not `write!` + `\n`, only because clippy insists; `writeln!`
    // always emits a bare `\n` on every platform, so the bytes are identical.
    // `G-LOGFILE`'s `WRAPPER_WRITE` recognises both.)
    let _ = writeln!(out, "error({LOG_SCOPE}): error initializing child: {err:?}");

    // SAFETY: `out.buf[..out.len]` is a live, initialized byte range on this
    // frame; fd 2 is stderr. A short or failed write is ignored, as a logger's
    // would be. `write(2)` is async-signal-safe.
    unsafe {
        let _ = libc::write(2, out.buf.as_ptr().cast::<std::ffi::c_void>(), out.len);
    }
}

impl Subprocess {
    /// Initialize the subprocess. This will NOT start it, this only sets
    /// up the internal state necessary to start it later.
    //
    // pub fn init(gpa: Allocator, cfg: Config) !Subprocess {
    pub fn init(_gpa: Allocator, cfg: Config) -> Result<Subprocess, InitError> {
        // We have a lot of maybe-allocations that all share the same lifetime
        // so use an arena so we don't end up in an accounting nightmare.
        // var arena = std.heap.ArenaAllocator.init(gpa);
        // errdefer arena.deinit();
        // const alloc = arena.allocator();
        // (PORT NOTE: the arena collapses into the owned fields below.)

        // Get our env. If a default env isn't provided by the caller
        // then we get it ourselves.
        // var env = cfg.env;
        let mut env = cfg.env;

        // If we have a resources dir then set our env var
        // if (cfg.resources_dir) |dir| {
        //     log.info("found Ghostty resources dir: {s}", .{dir});
        //     try env.put("GHOSTTY_RESOURCES_DIR", dir);
        // }
        if let Some(dir) = &cfg.resources_dir {
            // log.info("found Ghostty resources dir: {s}", .{dir});
            //
            // (R2 identity: `Ghostty` in the reference's literal is the product name.)
            log::info!(
                target: LOG_SCOPE,
                "found {} resources dir: {}",
                rustty_identity::APP_NAME_TITLECASE,
                String::from_utf8_lossy(dir)
            );
            env.put(&env_name("RESOURCES_DIR"), dir);
        }

        // Set our TERM var. This is a bit complicated because we want to use
        // the ghostty TERM value but we want to only do that if we have
        // ghostty in the TERMINFO database.
        //
        // For now, we just look up a bundled dir but in the future we should
        // also load the terminfo database and look for it.
        // if (cfg.resources_dir) |base| {
        if let Some(base) = &cfg.resources_dir {
            // try env.put("TERM", cfg.term);
            env.put(b"TERM", &cfg.term);
            // try env.put("COLORTERM", "truecolor");
            env.put(b"COLORTERM", b"truecolor");

            // Assume that the resources directory is adjacent to the terminfo
            // database
            // var buf: [std.fs.max_path_bytes]u8 = undefined;
            // const dir = try std.fmt.bufPrint(&buf, "{s}/terminfo", .{
            //     std.fs.path.dirname(base) orelse unreachable,
            // });
            let mut dir = dirname(base).unwrap().to_vec();
            dir.extend_from_slice(b"/terminfo");
            // try env.put("TERMINFO", dir);
            env.put(b"TERMINFO", &dir);
        } else {
            // if (comptime builtin.target.os.tag.isDarwin()) {
            //     log.warn("ghostty terminfo not found, using xterm-256color", .{});
            //     log.warn("the terminfo SHOULD exist on macos, please ensure", .{});
            //     log.warn("you're using a valid app bundle.", .{});
            // }
            //
            // (R2 identity: `ghostty` in the first literal is the product name.)
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                log::warn!(
                    target: LOG_SCOPE,
                    "{} terminfo not found, using xterm-256color",
                    rustty_identity::APP_NAME
                );
                log::warn!(target: LOG_SCOPE, "the terminfo SHOULD exist on macos, please ensure");
                log::warn!(target: LOG_SCOPE, "you're using a valid app bundle.");
            }

            // try env.put("TERM", "xterm-256color");
            env.put(b"TERM", b"xterm-256color");
            // try env.put("COLORTERM", "truecolor");
            env.put(b"COLORTERM", b"truecolor");
        }

        // Add our binary to the path if we can find it.
        // ghostty_path: {
        'rustty_path: {
            // Skip this for flatpak since host cannot reach them
            // if ((comptime build_config.flatpak) and
            //     internal_os.isFlatpak())
            // {
            //     break :ghostty_path;
            // }
            if rustty_build_config::flatpak() && rustty_os::flatpak::isFlatpak() {
                break 'rustty_path;
            }

            // var exe_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const exe_bin_path = std.fs.selfExePath(&exe_buf) catch |err| {
            //     log.warn("failed to get ghostty exe path err={}", .{err});
            //     break :ghostty_path;
            // };
            let exe_bin_path = match std::env::current_exe() {
                Ok(p) => p,
                Err(err) => {
                    // log.warn("failed to get ghostty exe path err={}", .{err});
                    //
                    // (R2 identity: `ghostty` is the product name.)
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to get {} exe path err={err:?}",
                        rustty_identity::APP_NAME
                    );
                    break 'rustty_path;
                }
            };
            // const exe_dir = std.fs.path.dirname(exe_bin_path) orelse break :ghostty_path;
            // PORT NOTE (P3-W7 T2): the reference's `exe_dir` is `[]const u8`.
            // This used `Path::to_str()`, which SKIPPED the whole
            // ghostty-bin-on-PATH block for a non-UTF-8 executable path;
            // `as_encoded_bytes` is lossless on every platform.
            let exe_dir_path = match exe_bin_path.parent() {
                Some(d) => d,
                None => break 'rustty_path,
            };
            let exe_dir: &[u8] = exe_dir_path.as_os_str().as_encoded_bytes();
            // log.debug("appending ghostty bin to path dir={s}", .{exe_dir});
            //
            // (R2 identity: `ghostty` is the product name.)
            log::debug!(
                target: LOG_SCOPE,
                "appending {} bin to path dir={}",
                rustty_identity::APP_NAME,
                String::from_utf8_lossy(exe_dir)
            );

            // We always set this so that if the shell overwrites the path
            // scripts still have a way to find the Ghostty binary when
            // running in Ghostty.
            // try env.put("GHOSTTY_BIN_DIR", exe_dir);
            env.put(&env_name("BIN_DIR"), exe_dir);

            // Append if we have a path. We want to append so that ghostty is
            // the last priority in the path. If we don't have a path set
            // then we just set it to the directory of the binary.
            // if (env.get("PATH")) |path| {
            if let Some(path) = env.getv(b"PATH").map(<[u8]>::to_vec) {
                // Verify that our path doesn't already contain this entry
                // var it = std.mem.tokenizeScalar(u8, path, std.fs.path.delimiter);
                // while (it.next()) |entry| {
                //     if (std.mem.eql(u8, entry, exe_dir)) break :ghostty_path;
                // }
                let delimiter = crate::shell_integration::PATH_LIST_DELIMITER;
                for entry in path
                    .split(|&b: &u8| b == delimiter)
                    .filter(|s: &&[u8]| !s.is_empty())
                {
                    if entry == exe_dir {
                        break 'rustty_path;
                    }
                }

                // try env.put(
                //     "PATH",
                //     try internal_os.appendEnv(alloc, path, exe_dir),
                // );
                env.put(b"PATH", &appendEnv(&path, exe_dir));
            } else {
                // try env.put("PATH", exe_dir);
                env.put(b"PATH", exe_dir);
            }
        }

        // On macOS, export additional data directories from our
        // application bundle.
        // if (comptime builtin.target.os.tag.isDarwin()) darwin: {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        'darwin: {
            // const resources_dir = cfg.resources_dir orelse break :darwin;
            let resources_dir = match &cfg.resources_dir {
                Some(d) => d,
                None => break 'darwin,
            };

            // var buf: [std.fs.max_path_bytes]u8 = undefined;

            // const xdg_data_dir_key = "XDG_DATA_DIRS";
            let xdg_data_dir_key: &[u8] = b"XDG_DATA_DIRS";
            // if (std.fmt.bufPrint(&buf, "{s}/..", .{resources_dir})) |data_dir| {
            //     try env.put(
            //         xdg_data_dir_key,
            //         try internal_os.appendEnv(
            //             alloc,
            //             env.get(xdg_data_dir_key) orelse "/usr/local/share:/usr/share",
            //             data_dir,
            //         ),
            //     );
            // } else |err| {
            //     log.warn("error building {s}; err={}", .{ xdg_data_dir_key, err });
            // }
            // (PORT NOTE: the bufPrint NoSpaceLeft error branch collapses with the
            // growable format.)
            //
            // PORT NOTE (G-LOGFILE / task-14e-1): the reference's
            // `log.warn("error building {s}; err={}", ...)` hangs off that
            // `bufPrint` error, which the growable format cannot produce, so
            // there is no failure arm here. See the `MANPATH` sibling below and
            // the file-header note.
            let mut data_dir = resources_dir.clone();
            data_dir.extend_from_slice(b"/..");
            let current = env
                .getv(xdg_data_dir_key)
                .unwrap_or(b"/usr/local/share:/usr/share")
                .to_vec();
            env.put(xdg_data_dir_key, &appendEnv(&current, &data_dir));

            // const manpath_key = "MANPATH";
            let manpath_key: &[u8] = b"MANPATH";
            // if (std.fmt.bufPrint(&buf, "{s}/../man", .{resources_dir})) |man_dir| {
            //     // Always append with colon in front, as it mean that if
            //     // `MANPATH` is empty, then it should be treated as an extra
            //     // path instead of overriding all paths set by OS.
            //     try env.put(
            //         manpath_key,
            //         try internal_os.appendEnvAlways(
            //             alloc,
            //             env.get(manpath_key) orelse "",
            //             man_dir,
            //         ),
            //     );
            // } else |err| {
            //     log.warn("error building {s}; man pages may not be available; err={}", .{ manpath_key, err });
            // }
            // PORT NOTE (G-LOGFILE / task-14e-1): as with `XDG_DATA_DIRS` above,
            // the reference's
            // `log.warn("error building {s}; man pages may not be available; err={}", ...)`
            // hangs off a `bufPrint` failure the growable format cannot produce.
            let mut man_dir = resources_dir.clone();
            man_dir.extend_from_slice(b"/../man");
            let current = env.getv(manpath_key).unwrap_or(b"").to_vec();
            env.put(manpath_key, &appendEnvAlways(&current, &man_dir));
        }

        // Set environment variables used by some programs (such as neovim) to detect
        // which terminal emulator and version they're running under.
        // try env.put("TERM_PROGRAM", "ghostty");
        // (R2 identity: the product name.)
        env.put(b"TERM_PROGRAM", rustty_identity::TERM_PROGRAM.as_bytes());
        // try env.put("TERM_PROGRAM_VERSION", build_config.version_string);
        env.put(
            b"TERM_PROGRAM_VERSION",
            rustty_version::VERSION_STRING.as_bytes(),
        );

        // VTE_VERSION is set by gnome-terminal and other VTE-based terminals.
        // We don't want our child processes to think we're running under VTE.
        // This is not apprt-specific, so we do it here.
        // env.remove("VTE_VERSION");
        env.removev(b"VTE_VERSION");

        // Setup our shell integration, if we can.
        // const shell_command: configpkg.Command = shell: {
        let shell_command: configpkg::command::Command = 'shell: {
            // const default_shell_command: configpkg.Command =
            //     cfg.command orelse .{ .shell = switch (builtin.os.tag) {
            //         .windows => "cmd.exe",
            //         else => "sh",
            //     } };
            let default_shell_command: configpkg::command::Command = match &cfg.command {
                Some(c) => c.clone(),
                None => configpkg::command::Command::Shell(if cfg!(windows) {
                    b"cmd.exe".to_vec()
                } else {
                    b"sh".to_vec()
                }),
            };

            // Always set up shell features (GHOSTTY_SHELL_FEATURES). These are
            // used by both automatic and manual shell integrations.
            // try shell_integration.setupFeatures(
            //     &env,
            //     cfg.shell_integration_features,
            //     cfg.cursor_blink orelse true,
            // );
            shell_integration::setupFeatures(
                &mut env,
                cfg.shell_integration_features,
                cfg.cursor_blink.unwrap_or(true),
            );

            // const force: ?shell_integration.Shell = switch (cfg.shell_integration) {
            let force: Option<shell_integration::Shell> = match cfg.shell_integration {
                // .none => {
                configpkg::value_types::ShellIntegration::None => {
                    // This is a source of confusion for users despite being
                    // opt-in since it results in some Ghostty features not
                    // working. We always want to log it.
                    // log.info("shell integration disabled by configuration", .{});
                    log::info!(target: LOG_SCOPE, "shell integration disabled by configuration");
                    break 'shell default_shell_command;
                }

                // .detect => null,
                configpkg::value_types::ShellIntegration::Detect => None,
                // .bash => .bash,
                configpkg::value_types::ShellIntegration::Bash => {
                    Some(shell_integration::Shell::Bash)
                }
                // .elvish => .elvish,
                configpkg::value_types::ShellIntegration::Elvish => {
                    Some(shell_integration::Shell::Elvish)
                }
                // .fish => .fish,
                configpkg::value_types::ShellIntegration::Fish => {
                    Some(shell_integration::Shell::Fish)
                }
                // .nushell => .nushell,
                configpkg::value_types::ShellIntegration::Nushell => {
                    Some(shell_integration::Shell::Nushell)
                }
                // .zsh => .zsh,
                configpkg::value_types::ShellIntegration::Zsh => {
                    Some(shell_integration::Shell::Zsh)
                }
            };

            // const dir = cfg.resources_dir orelse {
            //     log.warn("no resources dir set, shell integration disabled", .{});
            //     break :shell default_shell_command;
            // };
            let dir = match &cfg.resources_dir {
                Some(d) => d,
                None => {
                    // log.warn("no resources dir set, shell integration disabled", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "no resources dir set, shell integration disabled"
                    );
                    break 'shell default_shell_command;
                }
            };

            // const integration = try shell_integration.setup(
            //     alloc,
            //     dir,
            //     default_shell_command,
            //     &env,
            //     force,
            // ) orelse {
            //     log.warn("shell could not be detected, no automatic shell integration will be injected", .{});
            //     break :shell default_shell_command;
            // };
            let integration = match shell_integration::setup(
                dir,
                &default_shell_command,
                &mut env,
                force,
            )
            .map_err(InitError::ShellIntegration)?
            {
                Some(i) => i,
                None => {
                    // log.warn("shell could not be detected, no automatic shell integration will be injected", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "shell could not be detected, no automatic shell integration will be injected"
                    );
                    break 'shell default_shell_command;
                }
            };

            // log.info(
            //     "shell integration automatically injected shell={}",
            //     .{integration.shell},
            // );
            log::info!(
                target: LOG_SCOPE,
                "shell integration automatically injected shell={:?}",
                integration.shell
            );

            // break :shell integration.command;
            integration.command
        };

        // Add the environment variables that override any others.
        {
            // var it = cfg.env_override.iterator();
            // while (it.next()) |entry| try env.put(
            //     entry.key_ptr.*,
            //     entry.value_ptr.*,
            // );
            for (key, value) in cfg.env_override.iterator() {
                env.insert(env_string(key), env_string(value));
            }
        }

        // Build our args list
        // const args: []const [:0]const u8 = execCommand(
        //     alloc,
        //     shell_command,
        //     internal_os.passwd,
        // ) catch |err| switch (err) {
        //     // If we fail to allocate space for the command we want to
        //     // execute, we'd still like to try to run something so
        //     // Ghostty can launch (and maybe the user can debug this further).
        //     // Realistically, if you're getting OOM, I think other stuff is
        //     // about to crash, but we can try.
        //     error.OutOfMemory => oom: {
        //         log.warn("failed to allocate space for command args, falling back to basic shell", .{});
        //
        //         // The comptime here is important to ensure the full slice
        //         // is put into the binary data and not the stack.
        //         break :oom comptime switch (builtin.os.tag) {
        //             .windows => &.{"cmd.exe"},
        //             else => &.{"/bin/sh"},
        //         };
        //     },
        //
        //     // This logs on its own, this is a bad error.
        //     error.SystemError => return err,
        // };
        //
        // (PORT NOTE: the OutOfMemory arm collapses with Rust's infallible-
        // allocation model — preserved verbatim above; SystemError propagates.)
        //
        // PORT NOTE (G-LOGFILE / task-14e-1): so the reference's
        // `log.warn("failed to allocate space for command args, falling back to
        // basic shell", .{})` has no arm to hang on. See the file-header note.
        let args: Vec<CString> = match execCommand::<OsPasswd>(&shell_command) {
            Ok(args) => args,
            Err(ExecCommandError::SystemError) => return Err(InitError::SystemError),
        };

        // We have to copy the cwd because there is no guarantee that
        // pointers in full_config remain valid.
        // const cwd: ?[:0]u8 = if (cfg.working_directory) |cwd|
        //     try alloc.dupeZ(u8, cwd)
        // else
        //     null;
        //
        // PORT NOTE (task R13-P, N50): the bytes are copied as they are. This was
        // `.map(cstring)`, which panicked on an interior NUL (an OSC 7 `%00`
        // inherited by the next new tab, or a NUL in `working-directory`), and
        // the macOS staticlib aborts on a panic. `start` decides what to do with
        // such a path, exactly where the reference does.
        let cwd: Option<Vec<u8>> = cfg.working_directory.clone();

        // Propagate the current working directory (CWD) to the shell, enabling
        // the shell to display the current directory name rather than the
        // resolved path for symbolic links. This is important and based
        // on the same behavior in Konsole and Kitty (see the linked issues):
        // https://bugs.kde.org/show_bug.cgi?id=242114
        // https://github.com/kovidgoyal/kitty/issues/1595
        // https://github.com/ghostty-org/ghostty/discussions/7769
        // if (cwd) |pwd| try env.put("PWD", pwd);
        if let Some(pwd) = &cwd {
            env.put(b"PWD", pwd);
        }

        // return .{
        Ok(Subprocess {
            // .arena = arena, — (collapsed; see the struct PORT NOTE)
            env: Some(env),
            cwd,
            args,

            rt_pre_exec_info: cfg.rt_pre_exec_info,
            rt_post_fork_info: cfg.rt_post_fork_info,

            // (Zig field defaults `pty: ?Pty = null` / `process: ?Process =
            // null` are explicit in Rust struct literals — Command precedent.)
            pty: None,
            process: None,

            // Should be initialized with initTerminal call.
            // .grid_size = .{},
            grid_size: GridSize::default(),
            // .screen_size = .{ .width = 1, .height = 1 },
            screen_size: ScreenSize {
                width: 1,
                height: 1,
            },
        })
    }

    /// Clean up the subprocess. This will stop the subprocess if it is started.
    //
    // pub fn deinit(self: *Subprocess) void {
    pub fn deinit(&mut self) {
        // self.stop();
        unsafe { Self::stop(self) };
        // if (self.pty) |*pty| pty.deinit();
        // (PORT NOTE: `take()` — the reference's `self.* = undefined` after this
        // clears `pty`; nulling it here makes deinit idempotent, so a double
        // deinit cannot double-CLOSE the pty master fd. `Pty::deinit` closes the
        // master.)
        if let Some(mut pty) = self.pty.take() {
            pty.deinit();
        }
        // if (self.env) |*env| env.deinit();
        self.env = None;
        // self.arena.deinit();
        //
        // (PORT NOTE: the reference's arena backs the collapsed owned fields —
        // the `args: Vec<CString>` and `cwd: Option<CString>` C-string storage
        // (and `env`, freed just above). `arena.deinit()` frees ALL of it HERE,
        // inside deinit. The port must free them IN PLACE too — NOT defer to the
        // value's Drop — so the C strings are freed exactly ONCE at deinit,
        // regardless of whether/how the value is later dropped. `mem::take`
        // moves each owned buffer out into a temporary that drops at the end of
        // the statement (the free); the fields are left EMPTY.)
        let _ = std::mem::take(&mut self.args);
        self.cwd = None;
        // self.* = undefined;
        //
        // (PORT NOTE: after the frees above every owned field is empty
        // (`args == &[]`, `cwd == None`, `env == None`, `pty` freed), so a
        // subsequent Drop of this Subprocess is a genuine no-op — dropping an
        // empty `Vec`/`None` touches no allocation. This is the Rust-safe
        // analogue of the reference's `self.* = undefined` poison: it makes
        // deinit idempotent AND makes the value safe to drop after deinit, so a
        // caller that both `deinit()`s and later drops (e.g. an apprt finalize
        // that frees the surface box by dropping it) cannot double-free the
        // C-string storage. See the [DOUBLE-FREE FIX] note on `Subprocess`.)
    }

    // PORTED (xev B1, 0 reference tests — R1-safe): the spawn body is now live.
    // The former deferral point was the final `self.process = .{ .fork_exec =
    // cmd }` — the reference stores a Command whose fields are arena POINTERS
    // into the subprocess's own args/env (freed-on-success env included), which
    // Zig tolerates as raw pointers but which as Rust references would be exactly
    // the strongly-protected-reference UB the port forbids (memory
    // rust-mut-args-strongly-protected). RESOLUTION (blueprint §2, the "pid-only
    // handle" the deferral named): the transient argv/env/cwd borrows live ONLY
    // inside `start()`'s frame (read exactly once, by `cmd.start()`); the stored
    // `Process::ForkExec(Command)` has TRUE 'static empty reference fields (`c""`,
    // `&[]`, `None`) with only `pid` live, so no self-referential borrow exists at
    // rest (`killCommand` reads only `pid`). [ADV-FIX F3] the pty.slave File is
    // ManuallyDrop (Command reads only `.as_raw_fd()`) + the single verbatim
    // `posix.close`. [ADV-FIX F4] the raw `*mut Subprocess` self-pointer (`data`)
    // is derived via `addr_of_mut!` BEFORE any `&self.*` borrow and dereferenced
    // ONLY in the forked child (`subprocess_pre_exec`). Reference preserved
    // verbatim below (R5: no silent drop):
    //
    // /// Start the subprocess. If the subprocess is already started this
    // /// will crash.
    // pub fn start(self: *Subprocess, alloc: Allocator) !struct {
    //     read: Pty.Fd,
    //     write: Pty.Fd,
    // } {
    //     assert(self.pty == null and self.process == null);
    //
    //     // This function is funny because on POSIX systems it can
    //     // fail in the forked process. This is flipped to true if
    //     // we're in an error state in the forked process (child
    //     // process).
    //     var in_child: bool = false;
    //
    //     // Create our pty
    //     var pty = try Pty.open(.{
    //         .ws_row = @intCast(self.grid_size.rows),
    //         .ws_col = @intCast(self.grid_size.columns),
    //         .ws_xpixel = @intCast(self.screen_size.width),
    //         .ws_ypixel = @intCast(self.screen_size.height),
    //     });
    //     self.pty = pty;
    //     errdefer if (!in_child) {
    //         if (comptime builtin.os.tag != .windows) {
    //             _ = posix.close(pty.slave);
    //         }
    //
    //         pty.deinit();
    //         self.pty = null;
    //     };
    //
    //     // Cleanup we only run in our parent when we successfully start
    //     // the process.
    //     defer if (!in_child and self.process != null) {
    //         if (comptime builtin.os.tag != .windows) {
    //             // Once our subcommand is started we can close the slave
    //             // side. This prevents the slave fd from being leaked to
    //             // future children.
    //             _ = posix.close(pty.slave);
    //         }
    //
    //         // Successful start we can clear out some memory.
    //         if (self.env) |*env| {
    //             env.deinit();
    //             self.env = null;
    //         }
    //     };
    //
    //     log.debug("starting command command={f}", .{ArgsFormatter{ .args = self.args }});
    //
    //     // If we can't access the cwd, then don't set any cwd and inherit.
    //     // This is important because our cwd can be set by the shell (OSC 7)
    //     // and we don't want to break new windows.
    //     const cwd: ?[:0]const u8 = if (self.cwd) |proposed| cwd: {
    //         if ((comptime build_config.flatpak) and internal_os.isFlatpak()) {
    //             // Flatpak sandboxing prevents access to certain reserved paths
    //             // regardless of configured permissions. Perform a test spawn
    //             // to get around this problem
    //             //
    //             // https://docs.flatpak.org/en/latest/sandbox-permissions.html#reserved-paths
    //             log.info("flatpak detected, will use host command to verify cwd access", .{});
    //             const dev_null = try std.fs.cwd().openFile("/dev/null", .{ .mode = .read_write });
    //             defer dev_null.close();
    //             var cmd: internal_os.FlatpakHostCommand = .{
    //                 .argv = &[_][]const u8{
    //                     "/bin/sh",
    //                     "-c",
    //                     ":",
    //                 },
    //                 .cwd = proposed,
    //                 .stdin = dev_null.handle,
    //                 .stdout = dev_null.handle,
    //                 .stderr = dev_null.handle,
    //             };
    //             _ = cmd.spawn(alloc) catch |err| {
    //                 log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
    //                 break :cwd null;
    //             };
    //             _ = try cmd.wait();
    //
    //             break :cwd proposed;
    //         }
    //
    //         if (std.fs.cwd().access(proposed, .{})) {
    //             break :cwd proposed;
    //         } else |err| {
    //             log.warn("cannot access cwd, ignoring: {}", .{err});
    //             break :cwd null;
    //         }
    //     } else null;
    //
    //     // In flatpak, we use the HostCommand to execute our shell.
    //     if (internal_os.isFlatpak()) flatpak: {
    //         if (comptime !build_config.flatpak) {
    //             log.warn("flatpak detected, but flatpak support not built-in", .{});
    //             break :flatpak;
    //         }
    //
    //         // Flatpak command must have a stable pointer.
    //         self.process = .{ .flatpak = .{
    //             .argv = self.args,
    //             .cwd = cwd,
    //             .env = if (self.env) |*env| env else null,
    //             .stdin = pty.slave,
    //             .stdout = pty.slave,
    //             .stderr = pty.slave,
    //         } };
    //         var cmd = &self.process.?.flatpak;
    //         const pid = try cmd.spawn(alloc);
    //         errdefer killCommandFlatpak(cmd);
    //
    //         log.info("started subcommand on host via flatpak API path={s} pid={}", .{
    //             self.args[0],
    //             pid,
    //         });
    //
    //         return .{
    //             .read = pty.master,
    //             .write = pty.master,
    //         };
    //     }
    //
    //     // Build our subcommand
    //     var cmd: Command = .{
    //         .path = self.args[0],
    //         .args = self.args,
    //         .env = if (self.env) |*env| env else null,
    //         .cwd = cwd,
    //         .stdin = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
    //         .stdout = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
    //         .stderr = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
    //         .pseudo_console = if (builtin.os.tag == .windows) pty.pseudo_console else {},
    //         .os_pre_exec = switch (comptime builtin.os.tag) {
    //             .windows => null,
    //             else => f: {
    //                 const f = struct {
    //                     fn callback(cmd: *Command) ?u8 {
    //                         const sp = cmd.getData(Subprocess) orelse unreachable;
    //                         sp.childPreExec() catch |err| log.err(
    //                             "error initializing child: {}",
    //                             .{err},
    //                         );
    //                         return null;
    //                     }
    //                 };
    //                 break :f f.callback;
    //             },
    //         },
    //         .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec")) apprt.runtime.pre_exec.preExec else null,
    //         .rt_pre_exec_info = self.rt_pre_exec_info,
    //         .rt_post_fork = if (comptime @hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.postFork else null,
    //         .rt_post_fork_info = self.rt_post_fork_info,
    //         .data = self,
    //     };
    //
    //     cmd.start(alloc) catch |err| {
    //         // We have to do this because start on Windows can't
    //         // ever return ExecFailedInChild
    //         const StartError = error{ExecFailedInChild} || @TypeOf(err);
    //         switch (@as(StartError, err)) {
    //             // If we fail in our child we need to flag it so our
    //             // errdefers don't run.
    //             error.ExecFailedInChild => {
    //                 in_child = true;
    //                 return err;
    //             },
    //
    //             else => return err,
    //         }
    //     };
    //     errdefer killCommand(&cmd) catch |err| {
    //         log.warn("error killing command during cleanup err={}", .{err});
    //     };
    //     log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });
    //
    //     self.process = .{ .fork_exec = cmd };
    //     return switch (builtin.os.tag) {
    //         .windows => .{
    //             .read = pty.out_pipe,
    //             .write = pty.in_pipe,
    //         },
    //
    //         else => .{
    //             .read = pty.master,
    //             .write = pty.master,
    //         },
    //     };
    // }
    // pub fn start(self: *Subprocess, alloc: Allocator) !struct {
    //     read: Pty.Fd,
    //     write: Pty.Fd,
    // } {
    pub fn start(&mut self, alloc: Allocator) -> Result<PtyFds, crate::Error> {
        // The windows spawn half (out_pipe/in_pipe + pseudo_console), PORTED
        // (WS20). It cannot be cross-checked from macOS (the `highway-sys` C++
        // build needs an MSVC sysroot); task W-1 built and ran it on a Windows
        // host (a ConPTY child through `threadEnter`, see
        // `windows_threadenter_reads_the_pty_through_the_windows_read_thread`).
        //
        // PORT NOTE (why this is a separate arm rather than `if (windows)` inside
        // one body, as the reference has it): the reference's single body differs
        // on windows in FOUR places — the `Command` literal's
        // stdin/stdout/stderr (`null`), its `pseudo_console` field, its
        // `os_pre_exec` (`null`), and the returned pair (`out_pipe`/`in_pipe` vs
        // `master`/`master`). In Rust those are TYPE differences (the fd type, an
        // absent-vs-present struct field), not value differences, so the split is
        // forced by the language, exactly as it already is for `Pty`/`Command`
        // themselves.
        #[cfg(windows)]
        {
            // assert(self.pty == null and self.process == null);
            assert!(self.pty.is_none() && self.process.is_none());

            // (`in_child` is the POSIX fork/exec flag; `Command.start` on windows
            // "can't ever return ExecFailedInChild" — the reference says so at the
            // `cmd.start` call site — so there is no child continuation here.)

            // Create our pty
            let pty = Pty::open(ptypkg::winsize {
                ws_row: self.grid_size.rows,
                ws_col: self.grid_size.columns,
                ws_xpixel: self.screen_size.width.try_into().unwrap(),
                ws_ypixel: self.screen_size.height.try_into().unwrap(),
            })
            .map_err(|_e| {
                note_thread_enter_failure!("Subprocess.start: Pty.open", _e);
                crate::Error
            })?;
            // (capture the handles the reference's `pty.*` reads want before the
            // move into `self.pty`; they are `Copy` HANDLEs.)
            let out_pipe = pty.out_pipe;
            let in_pipe = pty.in_pipe;
            let pseudo_console = pty.pseudo_console;
            // self.pty = pty;
            self.pty = Some(pty);

            let rt_pre = self.rt_pre_exec_info;
            let rt_post = self.rt_post_fork_info;
            // The reference resolves `@hasDecl(apprt.runtime, "pre_exec")` HERE,
            // at comptime (`Exec.zig:1034,1036`). Rust cannot name the apprt from
            // this crate (R4: it would cycle), so the pair is read from the
            // process-global the apprt installs — see the seam PORT NOTE above
            // `command::RtPreExecInfo`.
            let rt_hooks = crate::command::rt_hooks();
            let self_ptr: *mut Subprocess = std::ptr::addr_of_mut!(*self);

            // If we can't access the cwd, then don't set any cwd and inherit.
            // This is important because our cwd can be set by the shell (OSC 7)
            // and we don't want to break new windows.
            //
            // PORT NOTE: `std.fs.cwd().access(proposed, .{})` — on windows the
            // posix `access(2)` shim is not available, so the existence probe is
            // `std::path::Path::exists` over the same raw path bytes (R3: the same
            // question, asked through the platform's own API).
            let cwd: Option<&[u8]> = if let Some(proposed) = &self.cwd {
                let bytes = proposed.as_slice();
                let ok = std::str::from_utf8(bytes)
                    .map(|s| std::path::Path::new(s).exists())
                    .unwrap_or(false);
                if ok {
                    Some(bytes)
                } else {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    //
                    // PORT NOTE (log text): the reference's `err` is
                    // `std.fs.Dir.access`'s error value; this arm reaches the same
                    // decision through a `Path::exists()` predicate, which carries
                    // no error value, so the errno is read back for the payload.
                    log::warn!(
                        target: LOG_SCOPE,
                        "cannot access cwd, ignoring: {}",
                        std::io::Error::last_os_error()
                    );
                    None
                }
            } else {
                None
            };

            // (`if (internal_os.isFlatpak()) flatpak: { ... }` — flatpak is a
            // Linux-only container runtime and `build_config.flatpak` is const
            // false, so the branch is dead here exactly as it is on macOS.)

            let argv: Vec<&CStr> = self.args.iter().map(|c| c.as_c_str()).collect();
            let env_ref: Option<&EnvMap> = self.env.as_ref();

            // Build our subcommand
            let mut cmd: Command = Command {
                // .path = self.args[0],
                path: argv[0],
                // .args = self.args,
                args: argv.as_slice(),
                // .env = if (self.env) |*env| env else null,
                env: env_ref,
                // .cwd = cwd,
                cwd,
                // .stdin = if (builtin.os.tag == .windows) null else ...,
                stdin: None,
                // .stdout = if (builtin.os.tag == .windows) null else ...,
                stdout: None,
                // .stderr = if (builtin.os.tag == .windows) null else ...,
                stderr: None,
                // .pseudo_console = if (builtin.os.tag == .windows) pty.pseudo_console else {},
                pseudo_console: Some(pseudo_console),
                // .os_pre_exec = switch (comptime builtin.os.tag) { .windows => null, ... },
                os_pre_exec: None,
                // .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec")) ... else null,
                rt_pre_exec: rt_hooks.pre_exec,
                rt_pre_exec_info: rt_pre,
                rt_post_fork: rt_hooks.post_fork,
                rt_post_fork_info: rt_post,
                // .data = self,
                data: Some(self_ptr.cast::<std::ffi::c_void>()),
                pid: None,
            };

            // cmd.start(alloc) catch |err| { ... else => return err };
            let result: Result<PtyFds, crate::Error> = match cmd.start(alloc) {
                Err(_e) => {
                    note_thread_enter_failure!("Subprocess.start: Command.start", _e);
                    Err(crate::Error)
                }
                Ok(()) => {
                    // errdefer killCommand(&cmd) catch ...;
                    // (no reachable error return between here and the success
                    // return below — same as the posix arm.)
                    // log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });
                    log::info!(
                        target: LOG_SCOPE,
                        "started subcommand path={} pid={:?}",
                        String::from_utf8_lossy(self.args[0].to_bytes()),
                        cmd.pid
                    );
                    let pid = cmd.pid;

                    // self.process = .{ .fork_exec = cmd };
                    // (PORT NOTE §2: store a pid-only handle — the stored
                    // Command's reference fields are TRUE 'static empties, so no
                    // self-referential borrow can exist at rest.)
                    self.process = Some(Process::ForkExec(Command {
                        path: c"",
                        args: &[],
                        env: None,
                        cwd: None,
                        stdin: None,
                        stdout: None,
                        stderr: None,
                        pseudo_console: None,
                        os_pre_exec: None,
                        rt_pre_exec: None,
                        rt_pre_exec_info: rt_pre,
                        rt_post_fork: None,
                        rt_post_fork_info: rt_post,
                        data: None,
                        pid,
                    }));

                    // return switch (builtin.os.tag) {
                    //     .windows => .{ .read = pty.out_pipe, .write = pty.in_pipe },
                    // };
                    Ok(PtyFds {
                        read: out_pipe,
                        write: in_pipe,
                    })
                }
            };

            // The reference errdefer/defer, now that the spawn resolved. There is
            // no `posix.close(pty.slave)` half on windows (no slave fd exists —
            // the pty's child ends are the `*_pty` pipe handles the pseudo console
            // owns and `Pty.deinit` closes).
            match &result {
                Ok(_) => {
                    // Successful start we can clear out some memory.
                    // if (self.env) |*env| { env.deinit(); self.env = null; }
                    self.env = None;
                }
                Err(_) => {
                    // pty.deinit(); self.pty = null;
                    if let Some(p) = &mut self.pty {
                        p.deinit();
                    }
                    self.pty = None;
                }
            }

            result
        }

        #[cfg(not(windows))]
        {
            // assert(self.pty == null and self.process == null);
            assert!(self.pty.is_none() && self.process.is_none());

            // This function is funny because on POSIX systems it can
            // fail in the forked process. This is flipped to true if
            // we're in an error state in the forked process (child
            // process).
            // var in_child: bool = false;
            let mut in_child = false;

            // Create our pty
            // var pty = try Pty.open(.{ .ws_row = @intCast(self.grid_size.rows), ... });
            let pty = Pty::open(ptypkg::winsize {
                // .ws_row = @intCast(self.grid_size.rows),
                ws_row: self.grid_size.rows,
                // .ws_col = @intCast(self.grid_size.columns),
                ws_col: self.grid_size.columns,
                // .ws_xpixel = @intCast(self.screen_size.width),
                ws_xpixel: self.screen_size.width.try_into().unwrap(),
                // .ws_ypixel = @intCast(self.screen_size.height),
                ws_ypixel: self.screen_size.height.try_into().unwrap(),
            })
            // (S212-F1: `pty.zig`'s `error.OpenptyFailed` keeps its name for
            // `Thread.threadMain`'s failure screen; it was the opaque `Error`.)
            .map_err(|e| match e {
                ptypkg::PosixPtyError::OpenptyFailed => crate::Error::OpenptyFailed,
                _ => crate::Error::Other,
            })?;
            // self.pty = pty;
            // (PORT NOTE: the Rust `Pty` is not `Copy`; capture the slave/master fds
            // before the move — the reference's errdefer/defer read the local
            // copy's `pty.slave`/`pty.master`.)
            let slave = pty.slave;
            let master = pty.master;
            self.pty = Some(pty);
            // errdefer if (!in_child) { ... posix.close(pty.slave); pty.deinit();
            //     self.pty = null; ... };
            // defer if (!in_child and self.process != null) { ...
            //     posix.close(pty.slave); ... env.deinit(); self.env = null; ... };
            // (PORT NOTE: Rust has no defer/errdefer; both are modeled explicitly at
            // the exit points below, gated by `in_child` / `self.process`.)

            // (PORT NOTE: `rt_*_info` are trivially-copyable ZSTs; read once for both
            // the spawn Command and the stored pid-only handle.)
            let rt_pre = self.rt_pre_exec_info;
            let rt_post = self.rt_post_fork_info;
            // The reference resolves `@hasDecl(apprt.runtime, "pre_exec")` HERE,
            // at comptime (`Exec.zig:1034,1036`). Rust cannot name the apprt from
            // this crate (R4: it would cycle), so the pair is read from the
            // process-global the apprt installs — see the seam PORT NOTE above
            // `command::RtPreExecInfo`.
            let rt_hooks = crate::command::rt_hooks();

            // [ADV-FIX F4] derive the write-capable raw self-pointer BEFORE any
            // `&self.*` field borrow, so it and the shared `&self.env` are never
            // simultaneously live; it is dereferenced only in the forked child.
            let self_ptr: *mut Subprocess = std::ptr::addr_of_mut!(*self);

            // log.debug("starting command command={f}", .{ArgsFormatter{ .args = self.args }});
            log::debug!(
                target: LOG_SCOPE,
                "starting command command={}",
                ArgsFormatter { args: &self.args }
            );

            // If we can't access the cwd, then don't set any cwd and inherit.
            // This is important because our cwd can be set by the shell (OSC 7)
            // and we don't want to break new windows.
            // const cwd: ?[:0]const u8 = if (self.cwd) |proposed| cwd: {
            let cwd: Option<&[u8]> = if let Some(proposed) = &self.cwd {
                // if ((comptime build_config.flatpak) and internal_os.isFlatpak()) {
                //     // Flatpak sandboxing prevents access to certain reserved paths
                //     // regardless of configured permissions. Perform a test spawn
                //     // to get around this problem
                //     //
                //     // https://docs.flatpak.org/en/latest/sandbox-permissions.html#reserved-paths
                //     log.info("flatpak detected, will use host command to verify cwd access", .{});
                //     const dev_null = try std.fs.cwd().openFile("/dev/null", .{ .mode = .read_write });
                //     defer dev_null.close();
                //     var cmd: internal_os.FlatpakHostCommand = .{
                //         .argv = &[_][]const u8{ "/bin/sh", "-c", ":" },
                //         .cwd = proposed,
                //         .stdin = dev_null.handle, .stdout = dev_null.handle, .stderr = dev_null.handle,
                //     };
                //     _ = cmd.spawn(alloc) catch |err| {
                //         log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
                //         break :cwd null;
                //     };
                //     _ = try cmd.wait();
                //     break :cwd proposed;
                // }
                //
                // (PORT NOTE (WS20): filled in. `comptime build_config.flatpak` is the
                // `flatpak` cargo feature; with it off the block is compiled out.)
                // (PORT NOTE: Zig's `break :cwd` out of the middle of the block maps
                // to an `Option<Option<..>>` short-circuit: `Some(v)` == the labelled
                // break took the value `v`, `None` == fall through to the `access`
                // check below.)
                #[cfg(feature = "flatpak")]
                let flatpak_cwd: Option<Option<&[u8]>> = if rustty_os::flatpak::isFlatpak() {
                    // log.info("flatpak detected, will use host command to verify cwd access", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "flatpak detected, will use host command to verify cwd access"
                    );
                    // const dev_null = try ... openFile("/dev/null", .{ .mode = .read_write });
                    let dev_null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR) };
                    let mut cmd = rustty_os::flatpak::FlatpakHostCommand::default();
                    cmd.argv = vec![b"/bin/sh".to_vec(), b"-c".to_vec(), b":".to_vec()];
                    // (`.cwd = proposed` reaches D-Bus as the `[*:0]` pointer, so
                    // the host sees the bytes up to the first NUL — `cstring`.)
                    cmd.cwd = Some(cstring(proposed));
                    cmd.stdin = dev_null;
                    cmd.stdout = dev_null;
                    cmd.stderr = dev_null;
                    // _ = cmd.spawn(alloc) catch |err| { ...; break :cwd null; };
                    let spawned = match cmd.spawn() {
                        Ok(_) => true,
                        Err(err) => {
                            // log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "cannot spawn command at cwd, ignoring: {err:?}"
                            );
                            false
                        }
                    };
                    // _ = try cmd.wait();
                    if spawned {
                        let _ = cmd.wait();
                    }
                    // defer dev_null.close();
                    if dev_null >= 0 {
                        unsafe { libc::close(dev_null) };
                    }
                    Some(if spawned {
                        // break :cwd proposed;
                        Some(proposed.as_slice())
                    } else {
                        // log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
                        None
                    })
                } else {
                    None
                };
                #[cfg(not(feature = "flatpak"))]
                let flatpak_cwd: Option<Option<&[u8]>> = None;

                // if (std.fs.cwd().access(proposed, .{})) {
                //     break :cwd proposed;
                // } else |err| {
                //     log.warn("cannot access cwd, ignoring: {}", .{err});
                //     break :cwd null;
                // }
                //
                // PORT NOTE (task R13-P, N50): Zig's `access` converts the path with
                // `pathToPosix`, which returns `error.BadPathName` for a path holding
                // a NUL byte, before any syscall (`std/Io/Threaded.zig:4062,14267`
                // in Zig 0.16.0). That error takes this same `catch` arm: the
                // warning, then no cwd. The payload names the Zig error, since
                // there is no errno to read back.
                let proposed_z = CString::new(proposed.as_slice());
                if let Some(v) = flatpak_cwd {
                    v
                } else if proposed_z.is_err() {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "cannot access cwd, ignoring: {}",
                        "error.BadPathName"
                    );
                    None
                } else if proposed_z
                    .as_ref()
                    .is_ok_and(|p| unsafe { libc::access(p.as_ptr(), libc::F_OK) } == 0)
                {
                    Some(proposed.as_slice())
                } else {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "cannot access cwd, ignoring: {}",
                        std::io::Error::last_os_error()
                    );
                    None
                }
            } else {
                None
            };

            // In flatpak, we use the HostCommand to execute our shell.
            // if (internal_os.isFlatpak()) flatpak: {
            if rustty_os::flatpak::isFlatpak() {
                // if (comptime !build_config.flatpak) {
                //     log.warn("flatpak detected, but flatpak support not built-in", .{});
                //     break :flatpak;
                // }
                //
                // (PORT NOTE (WS20): `comptime !build_config.flatpak` is
                // `#[cfg(not(feature = "flatpak"))]`; with the option OFF this is
                // the warn + break, with it ON the HostCommand spawn below.)
                #[cfg(not(feature = "flatpak"))]
                log::warn!(target: LOG_SCOPE, "flatpak detected, but flatpak support not built-in");

                // Flatpak command must have a stable pointer.
                // self.process = .{ .flatpak = .{ .argv = self.args, .cwd = cwd,
                //     .env = if (self.env) |*env| env else null,
                //     .stdin = pty.slave, .stdout = pty.slave, .stderr = pty.slave } };
                // var cmd = &self.process.?.flatpak;
                // const pid = try cmd.spawn(alloc);
                // errdefer killCommandFlatpak(cmd);
                // log.info("started subcommand on host via flatpak API path={s} pid={}", .{ self.args[0], pid });
                // return .{ .read = pty.master, .write = pty.master };
                #[cfg(feature = "flatpak")]
                {
                    let mut fcmd = rustty_os::flatpak::FlatpakHostCommand::default();
                    // (Byte strings since parity S111-F9; this used to go through
                    // `String::from_utf8_lossy`, renaming a non-UTF-8 argument.)
                    fcmd.argv = self.args.iter().map(|a| a.to_bytes().to_vec()).collect();
                    // (was `CString::new(c).unwrap()`: unreachable while `cwd` was a
                    // `CString`, reachable once it holds the reference's raw bytes;
                    // the flatpak test spawn above can accept a NUL path.)
                    fcmd.cwd = cwd.map(cstring);
                    fcmd.env = self.env.clone();
                    fcmd.stdin = slave;
                    fcmd.stdout = slave;
                    fcmd.stderr = slave;
                    self.process = Some(Process::Flatpak(fcmd));
                    // `var cmd = &self.process.?.flatpak;` — the STABLE pointer the
                    // reference's comment demands (spawn's documented precondition).
                    let Some(Process::Flatpak(cmd)) = self.process.as_mut() else {
                        unreachable!()
                    };
                    match cmd.spawn() {
                        Ok(pid) => {
                            // log.info("started subcommand on host via flatpak API path={s} pid={}", .{ self.args[0], pid });
                            log::info!(
                                target: LOG_SCOPE,
                                "started subcommand on host via flatpak API path={} pid={pid}",
                                String::from_utf8_lossy(self.args[0].to_bytes())
                            );
                        }
                        Err(_) => {
                            // errdefer killCommandFlatpak(cmd);
                            let _ = killCommandFlatpak(cmd);
                            return Err(crate::Error);
                        }
                    }
                    return Ok(PtyFds {
                        read: master,
                        write: master,
                    });
                }
            }

            // [ADV-FIX F3] the pty.slave File is NON-owning: `Command` reads only
            // `.as_raw_fd()` and never closes it, and the slave fd is closed
            // EXACTLY ONCE by the ported `posix.close(pty.slave)` below. A
            // dropping `File` here would double-close (fd-reuse race).
            let pty_slave_file = ManuallyDrop::new(unsafe { File::from_raw_fd(slave) });

            // (PORT NOTE: the transient argv VIEW over self.args (Vec<CString>) — a
            // stack local, valid for this frame only; nothing post-start reads
            // it. This is the Rust materialization of the Zig `self.args` slice.)
            let argv: Vec<&CStr> = self.args.iter().map(|c| c.as_c_str()).collect();
            // .env = if (self.env) |*env| env else null,
            let env_ref: Option<&EnvMap> = self.env.as_ref();

            // Build our subcommand
            // var cmd: Command = .{
            let mut cmd: Command = Command {
                // .path = self.args[0],
                path: argv[0],
                // .args = self.args,
                args: argv.as_slice(),
                // .env = if (self.env) |*env| env else null,
                env: env_ref,
                // .cwd = cwd,
                cwd,
                // .stdin = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
                stdin: Some(&*pty_slave_file),
                // .stdout = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
                stdout: Some(&*pty_slave_file),
                // .stderr = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
                stderr: Some(&*pty_slave_file),
                // .pseudo_console = if (windows) pty.pseudo_console else {},
                // (posix: the field does not exist — cfg(windows) only.)
                // .os_pre_exec = switch (comptime builtin.os.tag) { .windows => null, else => ... },
                os_pre_exec: Some(subprocess_pre_exec),
                // .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec")) apprt.runtime.pre_exec.preExec else null,
                // (the `@hasDecl` answer, installed from above by the apprt —
                // `Some(rustty_apprt::gtk::pre_exec::rt_pre_exec)` under the GTK
                // apprt, `None` under every other one.)
                rt_pre_exec: rt_hooks.pre_exec,
                // .rt_pre_exec_info = self.rt_pre_exec_info,
                rt_pre_exec_info: rt_pre,
                // .rt_post_fork = if (comptime @hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.postFork else null,
                rt_post_fork: rt_hooks.post_fork,
                // .rt_post_fork_info = self.rt_post_fork_info,
                rt_post_fork_info: rt_post,
                // .data = self,
                data: Some(self_ptr.cast::<std::ffi::c_void>()),
                // (.pid is set by cmd.start.)
                pid: None,
            };

            // The scoped spawn: `cmd`'s transient self-borrows (argv/env/cwd/the
            // slave File) are read ONLY inside `cmd.start()`; `pid` is the sole
            // field alive past here.
            let result: Result<PtyFds, crate::Error> = 'spawn: {
                // cmd.start(alloc) catch |err| {
                //     const StartError = error{ExecFailedInChild} || @TypeOf(err);
                //     switch (@as(StartError, err)) {
                //         error.ExecFailedInChild => { in_child = true; return err; },
                //         else => return err,
                //     }
                // };
                match cmd.start(alloc) {
                    // If we fail in our child we need to flag it so our
                    // errdefers don't run.
                    Err(crate::command::Error::ExecFailedInChild) => {
                        in_child = true;
                        break 'spawn Err(crate::Error);
                    }
                    Err(_e) => break 'spawn Err(crate::Error),
                    Ok(()) => {}
                }
                // errdefer killCommand(&cmd) catch |err| {
                //     log.warn("error killing command during cleanup err={}", .{err});
                // };
                // (PORT NOTE: there is no reachable error return between here and the
                // success `return` below — the straight-line body never triggers
                // this errdefer — so no cleanup is registered for it.)
                //
                // PORT NOTE (G-LOGFILE / task-14e-1): so `Exec.zig:1078`'s
                // `log.warn("error killing command during cleanup err={}", ...)`,
                // which lives INSIDE that errdefer, has no arm to hang on. See the
                // file-header note.
                // log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });
                log::info!(
                    target: LOG_SCOPE,
                    "started subcommand path={} pid={:?}",
                    String::from_utf8_lossy(self.args[0].to_bytes()),
                    cmd.pid
                );

                // (the ONLY field alive past here)
                let pid = cmd.pid;

                // self.process = .{ .fork_exec = cmd };
                // (PORT NOTE §2: store a pid-only handle — the stored Command's
                // reference fields are TRUE 'static empties, so no
                // self-referential borrow can exist at rest.)
                self.process = Some(Process::ForkExec(Command {
                    path: c"",
                    args: &[],
                    env: None,
                    cwd: None,
                    stdin: None,
                    stdout: None,
                    stderr: None,
                    os_pre_exec: None,
                    rt_pre_exec: None,
                    rt_pre_exec_info: rt_pre,
                    rt_post_fork: None,
                    rt_post_fork_info: rt_post,
                    data: None,
                    pid,
                }));

                // return switch (builtin.os.tag) {
                //     else => .{ .read = pty.master, .write = pty.master },
                // };
                Ok(PtyFds {
                    read: master,
                    write: master,
                })
            };

            // If we are in the forked child and exec failed (`in_child`), the
            // reference propagates `error.ExecFailedInChild` up to
            // `Exec.threadEnter`, which does `posix.exit(1)`:
            //     if (err != error.ExecFailedInChild) return err;
            //     posix.exit(1);
            //
            // PORT NOTE ([ADV-FIX], xev Milestone B): the port's opaque
            // `crate::Error` (a unit struct, B1) cannot carry the
            // `ExecFailedInChild` discriminant across the `Subprocess::start`
            // boundary, so `threadEnter` cannot tell the child apart from a
            // genuine parent error. Because the forked child MUST NOT return into
            // the parent's continuation (it would run the event loop as a
            // duplicate process), the `posix.exit(1)` is performed HERE — the one
            // place `in_child` is known — BEFORE the parent-only errdefer/defer
            // cleanup (which `in_child` already guards). `posix.exit ->
            // crate::command::posix_exit` (the libc `exit(3)` shim — NOT
            // `std::process::exit`, whose `rt::cleanup()` locks/allocates and
            // is illegal in the forked child; T8). Behaviorally identical to
            // the reference: the child exits 1; the parent bubbles every other
            // error.
            if in_child {
                crate::command::posix_exit(1);
            }

            // The reference errdefer/defer, now that the scoped spawn resolved:
            match &result {
                // defer if (!in_child and self.process != null) { ... };
                Ok(_) => {
                    if !in_child {
                        // Once our subcommand is started we can close the slave
                        // side. This prevents the slave fd from being leaked to
                        // future children.
                        // _ = posix.close(pty.slave);
                        unsafe {
                            libc::close(slave);
                        }

                        // Successful start we can clear out some memory.
                        // if (self.env) |*env| { env.deinit(); self.env = null; }
                        self.env = None;
                    }
                }
                // errdefer if (!in_child) { ... }; (defer no-ops: process == null)
                Err(_) => {
                    if !in_child {
                        // _ = posix.close(pty.slave);
                        unsafe {
                            libc::close(slave);
                        }
                        // pty.deinit();
                        if let Some(p) = &mut self.pty {
                            p.deinit();
                        }
                        // self.pty = null;
                        self.pty = None;
                    }
                }
            }

            result
        }
    }

    /// This should be called after fork but before exec in the child process.
    /// To repeat: this function RUNS IN THE FORKED CHILD PROCESS before
    /// exec is called; it does NOT run in the main Ghostty process.
    //
    // fn childPreExec(self: *Subprocess) !void {
    //     // Setup our pty
    //     try self.pty.?.childPreExec();
    // }
    // (The only reference caller is `start`'s os_pre_exec closure — the
    // `subprocess_pre_exec` trampoline, live in `start`; on windows os_pre_exec
    // is null, so the fn is unused there.)
    #[cfg(not(windows))]
    fn childPreExec(&mut self) -> Result<(), crate::Error> {
        // Setup our pty
        self.pty
            .as_mut()
            .unwrap()
            .childPreExec()
            .map_err(|_| crate::Error)
    }

    /// Called to notify that we exited externally so we can unset our
    /// running state.
    //
    // pub fn externalExit(self: *Subprocess) void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self; the io writer thread's
    // `threadExit` calls it while the main thread may read the pty and args.
    // Only `process` is reached, by place.
    ///
    /// # Safety
    /// `self_` must point at a live `Subprocess`.
    pub unsafe fn externalExit(self_: *mut Subprocess) {
        unsafe { (*self_).process = None };
    }

    /// Stop the subprocess. This is safe to call anytime. This will wait
    /// for the subprocess to register that it has been signalled, but not
    /// for it to terminate, so it will not block.
    /// This does not close the pty.
    //
    // pub fn stop(self: *Subprocess) void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self, as `externalExit`; only `process`
    // is reached, by place.
    ///
    /// # Safety
    /// `self_` must point at a live `Subprocess`.
    pub unsafe fn stop(self_: *mut Subprocess) {
        // (Test builds only: count the calls, so the UD102 test can check that
        // the drain ran before this.)
        #[cfg(test)]
        tests::STOP_CALLS.with(|c| c.set(c.get() + 1));
        // switch (self.process orelse return) {
        if let Some(process) = unsafe { &mut (*self_).process } {
            match process {
                // .fork_exec => |*cmd| {
                Process::ForkExec(cmd) => {
                    // Note: this will also wait for the command to exit, so
                    // DO NOT call cmd.wait
                    // killCommand(cmd) catch |err|
                    //     log.err("error sending SIGHUP to command, may hang: {}", .{err});
                    if let Err(err) = killCommand(cmd) {
                        // log.err("error sending SIGHUP to command, may hang: {}", .{err});
                        log::error!(
                            target: LOG_SCOPE,
                            "error sending SIGHUP to command, may hang: {err:?}"
                        );
                    }
                }

                // .flatpak => |*cmd| if (comptime build_config.flatpak) {
                //     killCommandFlatpak(cmd) catch |err|
                //         log.err("error sending SIGHUP to command, may hang: {}", .{err});
                //     _ = cmd.wait() catch |err|
                //         log.err("error waiting for command to exit: {}", .{err});
                // },
                //
                // (PORT NOTE (WS20): filled in. The `if (comptime build_config.flatpak)`
                // guard is the `flatpak` cargo feature; with the feature off the arm
                // body is compiled out exactly as Zig's lazy compilation leaves it.)
                #[cfg(feature = "flatpak")]
                Process::Flatpak(cmd) => {
                    if let Err(err) = killCommandFlatpak(cmd) {
                        // log.err("error sending SIGHUP to command, may hang: {}", .{err});
                        log::error!(
                            target: LOG_SCOPE,
                            "error sending SIGHUP to command, may hang: {err:?}"
                        );
                    }
                    if let Err(err) = cmd.wait() {
                        // log.err("error waiting for command to exit: {}", .{err});
                        log::error!(target: LOG_SCOPE, "error waiting for command to exit: {err:?}");
                    }
                }
                #[cfg(not(feature = "flatpak"))]
                Process::Flatpak(_cmd) => {}
            }
        } else {
            return;
        }

        unsafe { (*self_).process = None };
    }

    /// Resize the pty subprocess. This is safe to call anytime.
    //
    // pub fn resize(
    //     self: *Subprocess,
    //     grid_size: renderer.GridSize,
    //     screen_size: renderer.ScreenSize,
    // ) !void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self. The io writer thread resizes
    // while the main thread reads `args` (close-confirm) and the pty
    // (`getProcessInfo`), so only the fields the reference touches are
    // reached, by place, and the pty through `&Pty` (`Pty::setSize` is `&self`;
    // see its note).
    ///
    /// # Safety
    /// `self_` must point at a live `Subprocess`.
    pub unsafe fn resize(
        self_: *mut Subprocess,
        grid_size: GridSize,
        screen_size: ScreenSize,
    ) -> Result<(), crate::Error> {
        unsafe { (*self_).grid_size = grid_size };
        unsafe { (*self_).screen_size = screen_size };

        // if (self.pty) |*pty| {
        if let Some(pty) = unsafe { &(*self_).pty } {
            // It is theoretically possible for the grid or screen size to
            // exceed u16, although the terminal in that case isn't very
            // usable. This should be protected upstream but we still clamp
            // in case there is a bad caller which has happened before.
            // try pty.setSize(.{
            //     .ws_row = std.math.cast(u16, grid_size.rows) orelse std.math.maxInt(u16),
            //     .ws_col = std.math.cast(u16, grid_size.columns) orelse std.math.maxInt(u16),
            //     .ws_xpixel = std.math.cast(u16, screen_size.width) orelse std.math.maxInt(u16),
            //     .ws_ypixel = std.math.cast(u16, screen_size.height) orelse std.math.maxInt(u16),
            // });
            //
            // (PORT NOTE: `GridSize`'s units are already u16 — those casts are
            // identities; the pixel casts clamp.)
            pty.setSize(ptypkg::winsize {
                ws_row: grid_size.rows,
                ws_col: grid_size.columns,
                ws_xpixel: u16::try_from(screen_size.width).unwrap_or(u16::MAX),
                ws_ypixel: u16::try_from(screen_size.height).unwrap_or(u16::MAX),
            })
            .map_err(|_| crate::Error)?;
        }

        Ok(())
    }

    /// Get information about the process(es) running within the subprocess.
    /// Returns `null` if there was an error getting the information or the
    /// information is not available on a particular platform.
    //
    // pub fn getProcessInfo(self: *Subprocess, comptime info: ProcessInfo) ?ProcessInfo.Type(info) {
    //     const pty = &(self.pty orelse return null);
    //     return pty.getProcessInfo(info);
    // }
    //
    // PORT NOTE: split into the two concrete accessors (rustty-os pty.rs
    // precedent; see `Exec::getForegroundPid`).
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (the main thread's query overlaps
    // the io writer thread's `resize` / `threadExit`); only `pty` is reached,
    // through `&Pty` (the pty's process-info accessors are `&self`).
    ///
    /// # Safety
    /// `self_` must point at a live `Subprocess`.
    pub unsafe fn getForegroundPid(self_: *mut Subprocess) -> Option<u64> {
        let pty = unsafe { (*self_).pty.as_ref() }?;
        pty.getForegroundPid()
    }

    /// See [`Subprocess::getForegroundPid`].
    ///
    /// # Safety
    /// As [`Subprocess::getForegroundPid`]; the caller picks `'a` no longer
    /// than the `Subprocess` (and its pty) lives.
    pub unsafe fn getTtyName<'a>(self_: *mut Subprocess) -> Option<&'a CStr> {
        let pty = unsafe { (*self_).pty.as_ref() }?;
        pty.getTtyName()
    }
}

/// PORT NOTE (Rust-only glue): the anonymous return struct of
/// `Subprocess.start` (`struct { read: Pty.Fd, write: Pty.Fd }`).
pub struct PtyFds {
    // PORT NOTE (WS20): the reference's return type is `struct { read: Pty.Fd,
    // write: Pty.Fd }`, and `Pty.Fd` is `posix.fd_t` — which IS a
    // `windows.HANDLE` on Windows. The port therefore carries the same per-target
    // split `rustty_os::pty` does (`PosixPtyFd` / `WindowsPtyFd`).
    #[cfg(not(windows))]
    pub read: ptypkg::PosixPtyFd,
    #[cfg(not(windows))]
    pub write: ptypkg::PosixPtyFd,
    #[cfg(windows)]
    pub read: ptypkg::WindowsPtyFd,
    #[cfg(windows)]
    pub write: ptypkg::WindowsPtyFd,
}
