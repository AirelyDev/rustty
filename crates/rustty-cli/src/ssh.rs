//! rustty-cli::ssh
//!
//! PORTS: ghostty src/cli/ssh.zig — the `+ssh` wrapper action (wrap `ssh` to
//! configure Rustty terminal integration on remote hosts). Byte-faithful (R1);
//! ssh args and destinations are byte strings (R3). The `TERM=xterm-rustty`
//! wire token (the reference's `xterm-ghostty`), the `infocmp <name>`/`tic`
//! remote install script, and the `{APP_NAME}-ssh-` control-path prefix are
//! routed through `rustty-identity` (R2 / ADV-FIX 5 / Risk 3) so they track
//! the installed rustty-terminfo entry (`rustty-ssh-` by default,
//! `ghostty-ssh-` under the `identity-oracle` parity harness).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const cli_args = @import("args.zig");
use rustty_args::args as cli_args;
// const diagnostics = @import("diagnostics.zig");
use rustty_args::diagnostics::DiagnosticList;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const DiskCache = @import("ssh_cache.zig").DiskCache;
use crate::ssh_cache::DiskCache;
// const internal_os = @import("../os/main.zig");
use rustty_os as internal_os;
// const ghostty_terminfo = @import("../terminfo/main.zig").ghostty;
//   R2: the rustty-terminfo entry carrying the product identity.
use rustty_terminfo::rustty as rustty_terminfo_entry;

use std::io::Write;

// const log = std.log.scoped(.ssh);
const LOG_SCOPE: &str = "ssh";

// const usage =
//     \\Usage: ghostty +ssh [flags] [--] <ssh args...>
//     ...
//   R2: the `ghostty` binary name (command position, lowercase APP_NAME) and
//   `Ghostty terminfo` (product-name position, APP_NAME_TITLECASE per the
//   oracle at src/cli/ssh.zig:18) -> the Rustty product name/terminfo.
fn usage() -> String {
    let app = rustty_identity::APP_NAME;
    let app_name = rustty_identity::APP_NAME_TITLECASE;
    format!(
        "Usage: {app} +ssh [flags] [--] <ssh args...>\n\
         \n\
         Flags:\n\
         \x20 --forward-env[=bool]  Enable TERM / SendEnv forwarding. Default: true.\n\
         \x20 --terminfo[=bool]     Install {app_name} terminfo on first connect. Default: true.\n\
         \x20 --cache[=bool]        Use the terminfo install cache. Default: true.\n\
         \x20 --ssh=<path>          Path to the ssh binary. Default: first `ssh` on PATH.\n\
         \x20 --verbose             Print +ssh status lines to stderr.\n\
         \x20 --help                Show full help.\n\
         \n\
         ssh flags and the destination go after +ssh's own flags (or after `--`).\n"
    )
}

// pub const Options = struct {
pub struct Options {
    /// Set by the CLI parser for deinit.
    // _arena: ?ArenaAllocator = null,
    //   PORT NOTE: allocations are owned by the collected values; there is no
    //   shared arena. The struct still opts in via `arena_available` (the
    //   reference's `@hasField(T, "_arena")`).

    /// Maps to the `ssh-env` shell integration feature.
    // @"forward-env": bool = true,
    pub forward_env: bool,

    /// Maps to the `ssh-terminfo` shell integration feature.
    // terminfo: bool = true,
    pub terminfo: bool,

    /// When false, both cache read and write are bypassed.
    // cache: bool = true,
    pub cache: bool,

    /// The wrapped `ssh` binary.
    /// `/`-containing values are treated as paths; otherwise resolved via PATH.
    // ssh: []const u8 = "ssh",
    pub ssh: Vec<u8>,

    /// When true, print verbose output to stderr.
    // verbose: bool = false,
    pub verbose: bool,

    /// Arguments passed through to `ssh` verbatim. Populated by
    /// `parseManuallyHook` when we reach the first non-flag argument (or
    /// an explicit `--`).
    // _ssh_args: std.ArrayList([]const u8) = .empty,
    pub _ssh_args: Vec<Vec<u8>>,

    /// Enables arg parsing diagnostics so unknown flags become
    /// diagnostics rather than fatal errors.
    // _diagnostics: diagnostics.DiagnosticList = .{},
    pub _diagnostics: DiagnosticList,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            forward_env: true,
            terminfo: true,
            cache: true,
            ssh: b"ssh".to_vec(),
            verbose: false,
            _ssh_args: Vec::new(),
            _diagnostics: DiagnosticList::default(),
        }
    }
}

impl Options {
    // pub fn deinit(self: *Options) void { if (self._arena) |arena| arena.deinit(); self.* = undefined; }
    //   PORT NOTE: owned values drop naturally; there is no arena to free.
    pub fn deinit(&mut self) {
        let _ = self;
    }
}

impl cli_args::Target for Options {
    // The generic per-field dispatch (`parseIntoField`) for our five own flags.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), cli_args::ParseError> {
        match key {
            // @"forward-env": bool
            b"forward-env" => {
                self.forward_env = cli_args::parse_field_bool(value)?;
                Ok(())
            }
            // terminfo: bool
            b"terminfo" => {
                self.terminfo = cli_args::parse_field_bool(value)?;
                Ok(())
            }
            // cache: bool
            b"cache" => {
                self.cache = cli_args::parse_field_bool(value)?;
                Ok(())
            }
            // ssh: []const u8
            b"ssh" => {
                self.ssh = cli_args::parse_field_string(value)?;
                Ok(())
            }
            // verbose: bool
            b"verbose" => {
                self.verbose = cli_args::parse_field_bool(value)?;
                Ok(())
            }
            _ => Err(cli_args::Error::InvalidField),
        }
    }

    // @hasField(Options, "_arena") == true.
    fn arena_available(&self) -> bool {
        true
    }

    // canTrackDiags(Options) == @hasField(Options, "_diagnostics") == true.
    fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
        Some(&mut self._diagnostics)
    }

    /// Manual parse hook. For each argument:
    ///   - If it's a literal `--`, consume everything after it as ssh
    ///     args and stop parsing.
    ///   - If it doesn't start with `--`, this is the start of the ssh
    ///     argv. Consume this arg and everything after as ssh args and
    ///     stop parsing.
    ///   - Otherwise (a `--foo` arg), return true so the generic parser
    ///     handles it as one of our own flags.
    // pub fn parseManuallyHook(self, alloc, arg, iter) Allocator.Error!bool {
    //
    // PORT NOTE (iterator threading): the reference hook drains `iter` inline into
    // `_ssh_args`. The Rust `Target` hook can't hold `iter` (aliasing; see memory
    // `rust-mut-args-strongly-protected`), so the drain is routed through the
    // driver's `ConsumeRest` path: `record_replay_arg` appends each drained arg to
    // `_ssh_args`, and `apply_e_command` returns `false` (the reference's
    // `return false` — halt). The `--` and bare-arg branches return `ConsumeRest`;
    // the bare-arg branch first appends the current arg, exactly as the reference
    // appends it before draining the rest.
    fn parse_manually_hook(
        &mut self,
        arg: &[u8],
    ) -> Option<Result<cli_args::HookOutcome, cli_args::ParseError>> {
        // if (std.mem.eql(u8, arg, "--")) { while (iter.next()) |rest| self._ssh_args.append(dupe(rest)); return false; }
        if arg == b"--" {
            return Some(Ok(cli_args::HookOutcome::ConsumeRest));
        }

        // if (!std.mem.startsWith(u8, arg, "--")) {
        //     try self._ssh_args.append(alloc, try alloc.dupe(u8, arg));
        //     while (iter.next()) |rest| try self._ssh_args.append(alloc, try alloc.dupe(u8, rest));
        //     return false;
        // }
        if !arg.starts_with(b"--") {
            self._ssh_args.push(arg.to_vec());
            return Some(Ok(cli_args::HookOutcome::ConsumeRest));
        }

        // return true;
        Some(Ok(cli_args::HookOutcome::Continue))
    }

    // The `ConsumeRest` drain step: append each drained arg to `_ssh_args`
    // (the reference's `self._ssh_args.append(alloc, alloc.dupe(rest))`).
    fn record_replay_arg(&mut self, arg: &[u8]) {
        self._ssh_args.push(arg.to_vec());
    }

    // The reference hook's `return false` (halt) after draining.
    fn apply_e_command(
        &mut self,
        _command: Vec<Vec<u8>>,
        _location: rustty_args::diagnostics::Location,
    ) -> bool {
        false
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    /// Enables `-h` and `--help` to work.
    // pub fn help(_: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), cli_args::ParseError>> {
        let _ = Action::Help;
        Some(Err(cli_args::Error::ActionHelpRequested))
    }
}

/// Wrap `ssh` to automatically configure Rustty terminal integration on
/// remote hosts.
///
/// Any arguments that aren't recognized as `+ssh` flags are passed to
/// the real `ssh` binary unchanged. You can use `--` as an explicit
/// disambiguator if needed, though it's almost never required: `ssh`
/// has no long flags, and `+ssh` defines no short flags, so there's
/// nothing to collide.
///
/// This is typically called via Rustty's shell integration. When
/// `shell-integration-features` includes `ssh-env` or `ssh-terminfo`,
/// each shell defines an `ssh` function that runs:
///
/// ```text
///     rustty +ssh <flags> -- "$@"
/// ```
///
/// You can also run `rustty +ssh` directly, or alias it yourself (e.g.
/// `alias ssh='rustty +ssh --'`) if you prefer not to use the shell
/// integration.
///
/// `+ssh` performs up to two pieces of setup before launching `ssh`:
///
///   1. **Environment forwarding** (`--forward-env`). Sets `TERM` to
///      `xterm-256color` and requests `SendEnv` forwarding of
///      `COLORTERM`, `TERM_PROGRAM`, and `TERM_PROGRAM_VERSION` so the
///      remote shell can still detect that it's running inside Rustty.
///      The remote `sshd_config` must list these in `AcceptEnv` for
///      forwarding to succeed.
///
///   2. **Terminfo install** (`--terminfo`). On the first connection to a
///      given destination, installs Rustty's terminfo entry on the remote
///      host using `infocmp -x xterm-rustty | ssh tic -x -` over a
///      shared `ControlMaster` connection. Successful installs are cached
///      (see `rustty +ssh-cache`) so subsequent connections skip this
///      step. When terminfo is successfully installed or already cached,
///      `TERM` is set to `xterm-rustty` instead of `xterm-256color`.
///
/// If `--terminfo` install fails (e.g. `tic` not available on the
/// remote, filesystem permissions), a warning is logged and the
/// connection continues with `TERM=xterm-256color`.
///
/// Flags:
///
///   * `--forward-env=<bool>`: Enable `TERM` / `SendEnv` environment
///     forwarding. Default: `true`.
///
///   * `--terminfo=<bool>`: Enable automatic terminfo install on first
///     connection. Default: `true`.
///
///   * `--cache=<bool>`: Use the terminfo install cache. Default: `true`.
///     When `false`, both the cache read (skip-if-installed) and the
///     cache write (record-on-success) are bypassed, and every
///     connection performs the install. To one-shot reinstall a single
///     host while keeping the cache in use, prefer `rustty +ssh-cache
///     --remove=<host>` followed by a normal connection.
///
///   * `--ssh=<path>`: Path to the `ssh` binary to execute. Default: the
///     first `ssh` found on `PATH`.
///
///   * `--verbose`: Print +ssh status lines to stderr, and surface
///     remote stderr during the terminfo install.
///
/// Examples:
///
///   # Basic invocation using defaults:
///   rustty +ssh user@example.com
///
///   # Forward Rustty env vars but skip the terminfo install:
///   rustty +ssh --terminfo=false user@example.com
///
///   # `ssh` flags (short-form `-p`, etc.) pass through unchanged:
///   rustty +ssh -p 2222 -i ~/.ssh/id_ed25519 user@example.com
///
///   # Use `--` explicitly if your ssh args might collide with our flags:
///   rustty +ssh -- --some-rare-ssh-arg user@example.com
///
/// Pass `--verbose` to see what `+ssh` is doing. For cache inspection
/// and management, see `rustty +ssh-cache`.
///
/// Available since: 1.4.0
// pub fn run(alloc_gpa: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try cli_args.argsIterator(alloc_gpa);
        // defer iter.deinit();
        // try cli_args.parse(Options, alloc_gpa, &opts, &iter);
        let mut iter = cli_args::args_iterator()?;
        let result = cli_args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // var stderr_buffer: [1024]u8 = undefined; ... const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // Any diagnostic from the arg parser is an unknown flag or bad
    // value. Reject loudly — silently forwarding `--typo` to ssh would
    // produce confusing downstream errors.
    // if (!opts._diagnostics.empty()) {
    if !opts._diagnostics.empty() {
        // for (opts._diagnostics.items()) |diag| {
        for diag in opts._diagnostics.items() {
            // if (diag.key.len > 0) { stderr.print("Error: unknown flag `--{s}`.\n", .{diag.key}); }
            if !diag.key.is_empty() {
                let _ = writeln!(
                    stderr,
                    "Error: unknown flag `--{}`.",
                    String::from_utf8_lossy(&diag.key),
                );
            } else {
                // else { stderr.print("Error: {s}\n", .{diag.message}); }
                let _ = writeln!(stderr, "Error: {}", String::from_utf8_lossy(&diag.message));
            }
        }
        // stderr.print("\n{s}", .{usage}) catch {};
        let _ = write!(stderr, "\n{}", usage());
        // stderr.flush() catch {};
        let _ = stderr.flush();
        // return 2;
        return Ok(2);
    }

    // const result = runInner(alloc_gpa, &opts, stderr);
    let result = run_inner(&opts, &mut stderr);

    // stderr.flush() catch {};
    let _ = stderr.flush();

    opts.deinit();

    // return result;
    result.map_err(Error::from)
}

/// A cache write scheduled after a successful ssh exit.
struct ToCache {
    cache: DiskCache,
    dest: Vec<u8>,
}

/// The resolved terminfo session: the `TERM` value plus an optional cache write.
struct Session {
    term: Vec<u8>,
    to_cache: Option<ToCache>,
}

// fn runInner(gpa: Allocator, opts: *const Options, stderr: *std.Io.Writer) !u8 {
fn run_inner<We: Write>(opts: &Options, stderr: &mut We) -> Result<u8, std::io::Error> {
    // var arena = ArenaAllocator.init(gpa); defer arena.deinit(); const alloc = arena.allocator();
    //   PORT NOTE: arena dropped tree-wide; each value owns its bytes.

    // if (opts._ssh_args.items.len == 0) {
    if opts._ssh_args.is_empty() {
        // try stderr.print("Error: no ssh arguments provided.\n\n{s}", .{usage});
        write!(stderr, "Error: no ssh arguments provided.\n\n{}", usage())?;
        // return 2;
        return Ok(2);
    }

    // const session: struct { term, to_cache } = session: {
    let session: Session = 'session: {
        // if (!opts.terminfo) break :session .{ .term = "xterm-256color" };
        if !opts.terminfo {
            break 'session Session {
                term: b"xterm-256color".to_vec(),
                to_cache: None,
            };
        }

        // const dest = resolveDestination(alloc, opts.ssh, opts._ssh_args.items) orelse {
        //     warnPrint(stderr, "could not resolve ssh destination; skipping terminfo install", .{});
        //     break :session .{ .term = "xterm-256color" };
        // };
        let dest = match resolve_destination(&opts.ssh, &opts._ssh_args) {
            Some(d) => d,
            None => {
                warn_print(
                    stderr,
                    "could not resolve ssh destination; skipping terminfo install",
                );
                break 'session Session {
                    term: b"xterm-256color".to_vec(),
                    to_cache: None,
                };
            }
        };

        // const cache: ?DiskCache = if (opts.cache) cache: {
        //     const path = DiskCache.defaultPath(alloc, "ghostty") catch |err| {
        //         warnPrint(stderr, "ghostty terminfo cache unavailable: {}", .{err});
        //         break :session .{ .term = "xterm-256color" };
        //     };
        //     break :cache .{ .path = path };
        // } else null;
        //   R2 / ADV-FIX 5: the program dir is the Rustty app name via identity.
        let cache: Option<DiskCache> = if opts.cache {
            match DiskCache::default_path(rustty_identity::APP_NAME) {
                Ok(path) => Some(DiskCache { path }),
                Err(err) => {
                    warn_print(
                        stderr,
                        &format!(
                            "{} terminfo cache unavailable: {err}",
                            rustty_identity::APP_NAME
                        ),
                    );
                    break 'session Session {
                        term: b"xterm-256color".to_vec(),
                        to_cache: None,
                    };
                }
            }
        } else {
            None
        };

        // if (cache) |c| {
        if let Some(c) = &cache {
            // if (c.contains(alloc, dest) catch false) {
            if c.contains(&dest).unwrap_or(false) {
                // verbosePrint(opts, stderr, "dest: {s} (cached, skipping install)", .{dest});
                verbose_print(
                    opts,
                    stderr,
                    &format!("dest: {} (cached, skipping install)", show(&dest)),
                );
                // break :session .{ .term = "xterm-ghostty" };
                break 'session Session {
                    term: rustty_identity::TERMINFO_PRIMARY_NAME.as_bytes().to_vec(),
                    to_cache: None,
                };
            } else {
                // verbosePrint(opts, stderr, "dest: {s} (not cached, will install)", .{dest});
                verbose_print(
                    opts,
                    stderr,
                    &format!("dest: {} (not cached, will install)", show(&dest)),
                );
            }
        } else {
            // verbosePrint(opts, stderr, "dest: {s} (cache disabled, will install)", .{dest});
            verbose_print(
                opts,
                stderr,
                &format!("dest: {} (cache disabled, will install)", show(&dest)),
            );
        }

        // stderr.print("Setting up xterm-ghostty terminfo on {s}...\n", .{dest}) catch {};
        // stderr.flush() catch {};
        let _ = writeln!(
            stderr,
            "Setting up {} terminfo on {}...",
            rustty_identity::TERMINFO_PRIMARY_NAME,
            show(&dest),
        );
        let _ = stderr.flush();

        // installRemoteTerminfo(alloc, opts, stderr) catch |err| {
        //     warnPrint(stderr, "failed to install terminfo: {}", .{err});
        //     break :session .{ .term = "xterm-256color" };
        // };
        if let Err(err) = install_remote_terminfo(opts, stderr) {
            warn_print(stderr, &format!("failed to install terminfo: {err}"));
            break 'session Session {
                term: b"xterm-256color".to_vec(),
                to_cache: None,
            };
        }

        // break :session .{ .term = "xterm-ghostty", .to_cache = if (cache) |c| .{ .cache = c, .dest = dest } else null };
        Session {
            term: rustty_identity::TERMINFO_PRIMARY_NAME.as_bytes().to_vec(),
            to_cache: cache.map(|c| ToCache {
                cache: c,
                dest: dest.clone(),
            }),
        }
    };

    // Build the full argv: [ssh, ...our opts, ...user args]
    // const env_opts: []const []const u8 = if (opts.@"forward-env") env_opts: {
    //     const set_term = try std.fmt.allocPrint(alloc, "SetEnv=TERM={s}", .{session.term});
    //     break :env_opts &.{ "-o", set_term, "-o", "SendEnv=COLORTERM", "-o", "SendEnv=TERM_PROGRAM", "-o", "SendEnv=TERM_PROGRAM_VERSION" };
    // } else &.{};
    let env_opts: Vec<Vec<u8>> = if opts.forward_env {
        let mut set_term = b"SetEnv=TERM=".to_vec();
        set_term.extend_from_slice(&session.term);
        vec![
            b"-o".to_vec(),
            set_term,
            b"-o".to_vec(),
            b"SendEnv=COLORTERM".to_vec(),
            b"-o".to_vec(),
            b"SendEnv=TERM_PROGRAM".to_vec(),
            b"-o".to_vec(),
            b"SendEnv=TERM_PROGRAM_VERSION".to_vec(),
        ]
    } else {
        Vec::new()
    };
    // const argv = try std.mem.concat(alloc, []const u8, &.{ &.{opts.ssh}, env_opts, opts._ssh_args.items });
    let mut argv: Vec<Vec<u8>> = Vec::new();
    argv.push(opts.ssh.clone());
    argv.extend(env_opts);
    argv.extend(opts._ssh_args.iter().cloned());
    // verbosePrint(opts, stderr, "exec: {f}", .{Joined{ .items = argv }});
    verbose_print(opts, stderr, &format!("exec: {}", show(&joined(&argv))));

    // const exit_code = childExec(alloc, argv) catch |err| {
    //     try stderr.print("Error: failed to run {s}: {}\n", .{ argv[0], err });
    //     return 1;
    // };
    let exit_code = match child_exec(&argv) {
        Ok(code) => code,
        Err(err) => {
            writeln!(stderr, "Error: failed to run {}: {err}", show(&argv[0]))?;
            return Ok(1);
        }
    };
    // verbosePrint(opts, stderr, "exit: {d}", .{exit_code});
    verbose_print(opts, stderr, &format!("exit: {exit_code}"));

    // Attempt to cache (if needed) on a successful ssh execution.
    // if (exit_code == 0) if (session.to_cache) |entry| {
    //     if (entry.cache.add(alloc, entry.dest, std.time.timestamp())) |_| {
    //         verbosePrint(opts, stderr, "cache: wrote {s}", .{entry.dest});
    //     } else |err| { log.debug("cache add failed for '{s}': {}", .{ entry.dest, err }); }
    // };
    if exit_code == 0 {
        if let Some(entry) = session.to_cache {
            match entry.cache.add(&entry.dest, timestamp()) {
                Ok(()) => {
                    verbose_print(opts, stderr, &format!("cache: wrote {}", show(&entry.dest)))
                }
                Err(err) => log_debug(&format!(
                    "cache add failed for '{}': {err}",
                    show(&entry.dest),
                )),
            }
        }
    }

    // return exit_code;
    Ok(exit_code)
}

/// Log to `.ssh` and, if `--verbose`, also print to stderr.
// fn verbosePrint(opts, stderr, comptime fmt, args) void {
//
// PORT NOTE: the reference's `comptime fmt` + `args` are pre-formatted by the
// caller into `msg`; the `"+ssh: " ++ fmt ++ "\n"` framing is applied here.
fn verbose_print<We: Write>(opts: &Options, stderr: &mut We, msg: &str) {
    // log.debug(fmt, args);
    log_debug(msg);
    // if (!opts.verbose) return;
    if !opts.verbose {
        return;
    }
    // stderr.print("+ssh: " ++ fmt ++ "\n", args) catch return;
    // stderr.flush() catch return;
    let _ = writeln!(stderr, "+ssh: {msg}");
    let _ = stderr.flush();
}

/// Log a warning and also print a `Warning: <msg>` line to stderr.
// fn warnPrint(stderr, comptime fmt, args) void {
fn warn_print<We: Write>(stderr: &mut We, msg: &str) {
    // log.warn(fmt, args);
    log_warn(msg);
    // stderr.print("Warning: " ++ fmt ++ "\n", args) catch return;
    // stderr.flush() catch return;
    let _ = writeln!(stderr, "Warning: {msg}");
    let _ = stderr.flush();
}

/// Space-joined items, formattable as `{f}`.
// const Joined = struct {
struct Joined<'a> {
    // items: []const []const u8,
    items: &'a [Vec<u8>],
}

impl Joined<'_> {
    // pub fn format(self: Joined, writer: *std.Io.Writer) !void {
    fn format<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // for (self.items, 0..) |a, i| { if (i > 0) try writer.writeByte(' '); try writer.writeAll(a); }
        for (i, a) in self.items.iter().enumerate() {
            if i > 0 {
                writer.write_all(b" ")?;
            }
            writer.write_all(a)?;
        }
        Ok(())
    }
}

/// Render `Joined` into a byte string (the reference's `{f}` formatting).
fn joined(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = Joined { items }.format(&mut out);
    out
}

// fn checkExit(term: std.process.Child.Term, label: []const u8) error{ChildFailed}!void {
//
// PORT NOTE: `std.process.Child.Term` maps to `std::process::ExitStatus`; a
// clean `.Exited` is `status.code() == Some`, else the signal/abnormal case.
fn check_exit(status: &std::process::ExitStatus, label: &str) -> Result<(), ()> {
    match status.code() {
        // .Exited => |rc| if (rc != 0) { log.warn(...); return error.ChildFailed; },
        Some(rc) => {
            if rc != 0 {
                log_warn(&format!("{label} exited with non-zero status: {rc}"));
                return Err(());
            }
            Ok(())
        }
        // else => { log.warn("{s} terminated abnormally: {}", ...); return error.ChildFailed; },
        None => {
            log_warn(&format!("{label} terminated abnormally: {status:?}"));
            Err(())
        }
    }
}

/// Run `ssh -G <args>` and parse the output for `user` and `hostname`.
/// Returns the resolved `user@hostname`, or null if the destination
/// could not be resolved.
// fn resolveDestination(alloc, ssh, args) ?[]const u8 {
fn resolve_destination(ssh: &[u8], args: &[Vec<u8>]) -> Option<Vec<u8>> {
    // const argv = std.mem.concat(alloc, []const u8, &.{ &.{ ssh, "-G" }, args }) catch return null;
    let mut argv: Vec<Vec<u8>> = Vec::new();
    argv.push(ssh.to_vec());
    argv.push(b"-G".to_vec());
    argv.extend(args.iter().cloned());
    // const result = std.process.Child.run(.{ .allocator = alloc, .argv = argv }) catch |err| {
    //     log.warn("ssh -G spawn failed: {}", .{err});
    //     return null;
    // };
    let mut cmd = std::process::Command::new(as_os_str(&argv[0]));
    for a in &argv[1..] {
        cmd.arg(as_os_str(a));
    }
    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            log_warn(&format!("ssh -G spawn failed: {err}"));
            return None;
        }
    };
    // checkExit(result.term, "ssh -G") catch return null;
    if check_exit(&output.status, "ssh -G").is_err() {
        return None;
    }
    // return parseDestination(alloc, result.stdout);
    parse_destination(&output.stdout)
}

/// Parse `ssh -G` output for `user` and `hostname` and return the
/// formatted `user@hostname`. Returns null if either key is missing
/// or formatting fails.
// fn parseDestination(alloc, stdout) ?[]const u8 {
fn parse_destination(stdout: &[u8]) -> Option<Vec<u8>> {
    // var user: []const u8 = ""; var host: []const u8 = "";
    let mut user: &[u8] = b"";
    let mut host: &[u8] = b"";
    // var it = std.mem.tokenizeScalar(u8, stdout, '\n');
    let mut it = TokenizeScalar::new(stdout, b'\n');
    // while (it.next()) |line| {
    while let Some(line) = it.next() {
        // const space = std.mem.indexOfScalar(u8, line, ' ') orelse continue;
        let space = match line.iter().position(|&c| c == b' ') {
            Some(i) => i,
            None => continue,
        };
        // const key = line[0..space]; const value = line[space + 1 ..];
        let key = &line[0..space];
        let value = &line[space + 1..];
        // if (std.mem.eql(u8, key, "user")) { user = value; } else if (std.mem.eql(u8, key, "hostname")) { host = value; }
        if key == b"user" {
            user = value;
        } else if key == b"hostname" {
            host = value;
        }
        // if (user.len > 0 and host.len > 0) break;
        if !user.is_empty() && !host.is_empty() {
            break;
        }
    }

    // if (user.len == 0) { log.warn("ssh -G output missing user", .{}); return null; }
    if user.is_empty() {
        log_warn("ssh -G output missing user");
        return None;
    }
    // if (host.len == 0) { log.warn("ssh -G output missing hostname", .{}); return null; }
    if host.is_empty() {
        log_warn("ssh -G output missing hostname");
        return None;
    }

    // return std.fmt.allocPrint(alloc, "{s}@{s}", .{ user, host }) catch null;
    let mut out = Vec::with_capacity(user.len() + 1 + host.len());
    out.extend_from_slice(user);
    out.push(b'@');
    out.extend_from_slice(host);
    Some(out)
}

/// Install Rustty's terminfo on the remote host over a short-lived SSH
/// ControlMaster connection. The master tears down with the client
/// (`ControlPersist=no`) so no socket lingers.
// fn installRemoteTerminfo(alloc, opts, stderr) !void {
fn install_remote_terminfo<We: Write>(
    opts: &Options,
    stderr: &mut We,
) -> Result<(), std::io::Error> {
    // var buf: std.Io.Writer.Allocating = .init(alloc); defer buf.deinit();
    // try ghostty_terminfo.encode(&buf.writer);
    // const terminfo = buf.written();
    let mut buf: Vec<u8> = Vec::new();
    rustty_terminfo_entry().encode(&mut buf)?;
    let terminfo = buf;

    // ControlPath is in TMPDIR with a short, random basename. ssh uses
    // ControlPath as the bind address for a Unix domain socket; macOS
    // limits sockaddr_un.sun_path to ~104 bytes, so keeping the path
    // short leaves margin.
    // const control_path = try internal_os.randomTmpPath(alloc, "ghostty-ssh-");
    //   R2: the temp-socket prefix carries the Rustty app name.
    let control_path = internal_os::randomTmpPath(&format!("{}-ssh-", rustty_identity::APP_NAME));
    // const control_path_opt = try std.fmt.allocPrint(alloc, "ControlPath={s}", .{control_path});
    let control_path_opt = format!("ControlPath={control_path}");

    // Under --verbose, let remote stderr through (the `tic` step is
    // the most common failure source) and inherit ssh's stderr so it
    // reaches the user's terminal. Other steps stay quiet either way.
    // const remote_script = if (opts.verbose)
    //     \\infocmp xterm-ghostty >/dev/null 2>&1 && exit 0
    //     \\command -v tic >/dev/null 2>&1 || exit 1
    //     \\mkdir -p ~/.terminfo 2>/dev/null && tic -x - && exit 0
    //     \\exit 1
    // else
    //     \\infocmp xterm-ghostty >/dev/null 2>&1 && exit 0
    //     \\command -v tic >/dev/null 2>&1 || exit 1
    //     \\mkdir -p ~/.terminfo 2>/dev/null && tic -x - 2>/dev/null && exit 0
    //     \\exit 1
    //   R2 / Risk 3: the `infocmp <name>` target is the installed rustty-terminfo
    //   entry name (a wire/interop token), routed through rustty-identity.
    let ti = rustty_identity::TERMINFO_PRIMARY_NAME;
    let remote_script = if opts.verbose {
        format!(
            "infocmp {ti} >/dev/null 2>&1 && exit 0\n\
             command -v tic >/dev/null 2>&1 || exit 1\n\
             mkdir -p ~/.terminfo 2>/dev/null && tic -x - && exit 0\n\
             exit 1"
        )
    } else {
        format!(
            "infocmp {ti} >/dev/null 2>&1 && exit 0\n\
             command -v tic >/dev/null 2>&1 || exit 1\n\
             mkdir -p ~/.terminfo 2>/dev/null && tic -x - 2>/dev/null && exit 0\n\
             exit 1"
        )
    };

    // Set up an SSH ControlMaster scoped to this single install:
    //   - ControlMaster=yes makes our client also act as the master,
    //     so `infocmp | ssh tic` runs over a single connection.
    //   - ControlPersist=no tears the master down when our client
    //     exits; no socket lingers on the remote side.
    // const argv = try std.mem.concat(alloc, []const u8, &.{ &.{opts.ssh},
    //     &.{ "-o", "ControlMaster=yes", "-o", "ControlPersist=no", "-o", control_path_opt },
    //     opts._ssh_args.items, &.{remote_script} });
    let mut argv: Vec<Vec<u8>> = Vec::new();
    argv.push(opts.ssh.clone());
    argv.extend([
        b"-o".to_vec(),
        b"ControlMaster=yes".to_vec(),
        b"-o".to_vec(),
        b"ControlPersist=no".to_vec(),
        b"-o".to_vec(),
        control_path_opt.into_bytes(),
    ]);
    argv.extend(opts._ssh_args.iter().cloned());
    argv.push(remote_script.into_bytes());
    // verbosePrint(opts, stderr, "exec: {f}", .{Joined{ .items = argv }});
    verbose_print(opts, stderr, &format!("exec: {}", show(&joined(&argv))));

    // var child: std.process.Child = .init(argv, alloc);
    // child.stdin_behavior = .Pipe; child.stdout_behavior = .Ignore;
    // child.stderr_behavior = if (opts.verbose) .Inherit else .Ignore;
    let mut cmd = std::process::Command::new(as_os_str(&argv[0]));
    for a in &argv[1..] {
        cmd.arg(as_os_str(a));
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(if opts.verbose {
        std::process::Stdio::inherit()
    } else {
        std::process::Stdio::null()
    });

    // child.spawn() catch |err| { log.warn("terminfo install spawn failed: {}", .{err}); return error.InstallFailed; };
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            log_warn(&format!("terminfo install spawn failed: {err}"));
            return Err(std::io::Error::other("terminfo install spawn failed"));
        }
    };

    // if (child.stdin) |stdin| { stdin.writeAll(terminfo) catch {}; stdin.close(); child.stdin = null; }
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&terminfo);
        drop(stdin);
    }

    // const term = child.wait() catch |err| { log.warn("terminfo install wait failed: {}", .{err}); return error.InstallFailed; };
    let status = match child.wait() {
        Ok(s) => s,
        Err(err) => {
            log_warn(&format!("terminfo install wait failed: {err}"));
            return Err(std::io::Error::other("terminfo install wait failed"));
        }
    };
    // checkExit(term, "terminfo install") catch return error.InstallFailed;
    check_exit(&status, "terminfo install")
        .map_err(|_| std::io::Error::other("terminfo install failed"))
}

/// Returns `128 + signum` for signal-killed children, matching shell convention.
// fn childExec(alloc, argv) !u8 {
fn child_exec(argv: &[Vec<u8>]) -> Result<u8, std::io::Error> {
    // var child: std.process.Child = .init(argv, alloc);
    // child.stdin_behavior = .Inherit; child.stdout_behavior = .Inherit; child.stderr_behavior = .Inherit;
    let mut cmd = std::process::Command::new(as_os_str(&argv[0]));
    for a in &argv[1..] {
        cmd.arg(as_os_str(a));
    }
    // (Command inherits stdio by default.)

    // try child.spawn();
    let mut child = cmd.spawn()?;
    // const term = try child.wait();
    let status = child.wait()?;
    // return switch (term) {
    //     .Exited => |rc| rc,
    //     .Signal => |sig| @as(u8, 128) + @as(u8, @intCast(@min(sig, 127))),
    //     .Stopped, .Unknown => 1,
    // };
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(rc) = status.code() {
            Ok(rc as u8)
        } else if let Some(sig) = status.signal() {
            Ok(128u8.wrapping_add(std::cmp::min(sig, 127) as u8))
        } else {
            Ok(1)
        }
    }
    #[cfg(not(unix))]
    {
        Ok(status.code().unwrap_or(1) as u8)
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for std facilities used above.
// ---------------------------------------------------------------------------

/// `std.time.timestamp()` — current Unix time in whole seconds.
fn timestamp() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// Interpret argv bytes as an `OsStr` for `std::process::Command` (R3: argv are
/// byte strings). `{s}`-formatted diagnostics use `show` (lossy) instead.
#[cfg(unix)]
fn as_os_str(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}
#[cfg(not(unix))]
fn as_os_str(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// Lossy view of a byte string for a `{s}` diagnostic (never on the wire).
fn show(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}

// The reference `log.debug` goes to the log system, filtered out at the default
// level; kept as a no-op call site to preserve the reference's structure without
// polluting stderr on non-verbose runs.
fn log_debug(_msg: &str) {}

/// `std.mem.tokenizeScalar(u8, buf, delim)`: iterate non-empty runs between
/// `delim` bytes.
struct TokenizeScalar<'a> {
    rest: &'a [u8],
    delim: u8,
}

impl<'a> TokenizeScalar<'a> {
    fn new(buf: &'a [u8], delim: u8) -> TokenizeScalar<'a> {
        TokenizeScalar { rest: buf, delim }
    }

    fn next(&mut self) -> Option<&'a [u8]> {
        while !self.rest.is_empty() && self.rest[0] == self.delim {
            self.rest = &self.rest[1..];
        }
        if self.rest.is_empty() {
            return None;
        }
        let mut i = 0;
        while i < self.rest.len() && self.rest[i] != self.delim {
            i += 1;
        }
        let tok = &self.rest[..i];
        self.rest = &self.rest[i..];
        Some(tok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_args::args::ArgIterator;
    use rustty_args::diagnostics::LocationProvider;

    // ---------------------------------------------------------------------------
    // `std.process.ArgIteratorGeneral(.{})` test helper — the same tokenizer the
    // reference tests use (splits on whitespace, honors double-quote groups /
    // backslash escapes). Implements the arg-parser's `ArgIterator` +
    // `LocationProvider` so `cli_args::parse` can drive it.
    // ---------------------------------------------------------------------------
    struct ArgIteratorGeneral {
        args: Vec<Vec<u8>>,
        idx: usize,
    }

    impl ArgIteratorGeneral {
        fn init(cmd: &[u8]) -> ArgIteratorGeneral {
            let mut args: Vec<Vec<u8>> = Vec::new();
            let mut cur: Vec<u8> = Vec::new();
            let mut in_token = false;
            let mut in_quote = false;
            let mut i = 0;
            while i < cmd.len() {
                let c = cmd[i];
                if in_quote {
                    match c {
                        b'"' => {
                            in_quote = false;
                            i += 1;
                        }
                        b'\\' => {
                            if i + 1 < cmd.len() {
                                cur.push(cmd[i + 1]);
                                i += 2;
                            } else {
                                i += 1;
                            }
                        }
                        _ => {
                            cur.push(c);
                            i += 1;
                        }
                    }
                } else {
                    match c {
                        b' ' | b'\t' | b'\n' | b'\r' => {
                            if in_token {
                                args.push(std::mem::take(&mut cur));
                                in_token = false;
                            }
                            i += 1;
                        }
                        b'"' => {
                            in_token = true;
                            in_quote = true;
                            i += 1;
                        }
                        b'\\' => {
                            in_token = true;
                            if i + 1 < cmd.len() {
                                cur.push(cmd[i + 1]);
                                i += 2;
                            } else {
                                i += 1;
                            }
                        }
                        _ => {
                            in_token = true;
                            cur.push(c);
                            i += 1;
                        }
                    }
                }
            }
            if in_token {
                args.push(cur);
            }
            ArgIteratorGeneral { args, idx: 0 }
        }

        fn deinit(&mut self) {}
    }

    impl ArgIterator for ArgIteratorGeneral {
        type Item = Vec<u8>;
        fn next(&mut self) -> Option<Vec<u8>> {
            if self.idx >= self.args.len() {
                return None;
            }
            let v = self.args[self.idx].clone();
            self.idx += 1;
            Some(v)
        }
    }

    impl LocationProvider for ArgIteratorGeneral {}

    // fn parseTestArgs(alloc, opts, line) !void {
    //     var iter = try std.process.ArgIteratorGeneral(.{}).init(alloc, line);
    //     defer iter.deinit();
    //     try cli_args.parse(Options, alloc, opts, &iter);
    // }
    fn parse_test_args(opts: &mut Options, line: &[u8]) -> Result<(), cli_args::ParseError> {
        let mut iter = ArgIteratorGeneral::init(line);
        let res = cli_args::parse(opts, &mut iter);
        iter.deinit();
        res
    }

    // test "parseManuallyHook: bare destination starts ssh args"
    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn parse_manually_hook_bare_destination_starts_ssh_args() {
        let mut opts = Options::default();
        parse_test_args(&mut opts, b"--terminfo=false user@example.com").unwrap();
        // try testing.expectEqual(false, opts.terminfo);
        assert_eq!(false, opts.terminfo);
        // try testing.expectEqual(true, opts.@"forward-env");
        assert_eq!(true, opts.forward_env);
        // try testing.expectEqual(@as(usize, 1), opts._ssh_args.items.len);
        assert_eq!(1_usize, opts._ssh_args.len());
        // try testing.expectEqualStrings("user@example.com", opts._ssh_args.items[0]);
        assert_eq!(b"user@example.com".as_slice(), opts._ssh_args[0].as_slice());
        opts.deinit();
    }

    // test "parseManuallyHook: short ssh flags pass through verbatim"
    #[test]
    fn parse_manually_hook_short_ssh_flags_pass_through_verbatim() {
        let mut opts = Options::default();
        parse_test_args(&mut opts, b"-p 2222 user@example.com").unwrap();
        // try testing.expectEqual(@as(usize, 3), opts._ssh_args.items.len);
        assert_eq!(3_usize, opts._ssh_args.len());
        // try testing.expectEqualStrings("-p", opts._ssh_args.items[0]);
        assert_eq!(b"-p".as_slice(), opts._ssh_args[0].as_slice());
        // try testing.expectEqualStrings("2222", opts._ssh_args.items[1]);
        assert_eq!(b"2222".as_slice(), opts._ssh_args[1].as_slice());
        // try testing.expectEqualStrings("user@example.com", opts._ssh_args.items[2]);
        assert_eq!(b"user@example.com".as_slice(), opts._ssh_args[2].as_slice());
        opts.deinit();
    }

    // test "parseManuallyHook: explicit -- separator"
    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn parse_manually_hook_explicit_separator() {
        let mut opts = Options::default();
        parse_test_args(
            &mut opts,
            b"--verbose -- --some-rare-ssh-arg user@example.com",
        )
        .unwrap();
        // try testing.expectEqual(true, opts.verbose);
        assert_eq!(true, opts.verbose);
        // try testing.expectEqual(@as(usize, 2), opts._ssh_args.items.len);
        assert_eq!(2_usize, opts._ssh_args.len());
        // try testing.expectEqualStrings("--some-rare-ssh-arg", opts._ssh_args.items[0]);
        assert_eq!(
            b"--some-rare-ssh-arg".as_slice(),
            opts._ssh_args[0].as_slice(),
        );
        // try testing.expectEqualStrings("user@example.com", opts._ssh_args.items[1]);
        assert_eq!(b"user@example.com".as_slice(), opts._ssh_args[1].as_slice());
        opts.deinit();
    }

    // test "parseDestination: typical ssh -G output"
    #[test]
    fn parse_destination_typical_ssh_g_output() {
        let stdout = "user alice\nhostname example.com\nport 22\nidentityfile ~/.ssh/id_ed25519\n";
        // const result = parseDestination(arena.allocator(), stdout);
        let result = parse_destination(stdout.as_bytes());
        // try testing.expectEqualStrings("alice@example.com", result.?);
        assert_eq!(b"alice@example.com".as_slice(), result.unwrap().as_slice());
    }

    // test "parseDestination: hostname before user"
    #[test]
    fn parse_destination_hostname_before_user() {
        let stdout = "hostname example.com\nport 22\nuser alice\n";
        let result = parse_destination(stdout.as_bytes());
        assert_eq!(b"alice@example.com".as_slice(), result.unwrap().as_slice());
    }

    // test "parseDestination: missing hostname returns null"
    #[test]
    fn parse_destination_missing_hostname_returns_null() {
        let stdout = "user alice\nport 22\n";
        // try testing.expectEqual(@as(?[]const u8, null), parseDestination(arena.allocator(), stdout));
        assert_eq!(None, parse_destination(stdout.as_bytes()));
    }

    // test "parseDestination: missing user returns null"
    #[test]
    fn parse_destination_missing_user_returns_null() {
        let stdout = "hostname example.com\nport 22\n";
        assert_eq!(None, parse_destination(stdout.as_bytes()));
    }

    // test "parseDestination: empty input returns null"
    #[test]
    fn parse_destination_empty_input_returns_null() {
        assert_eq!(None, parse_destination(b""));
    }

    // test "parseDestination: IPv6 hostname"
    #[test]
    fn parse_destination_ipv6_hostname() {
        let stdout = "user alice\nhostname ::1\n";
        let result = parse_destination(stdout.as_bytes());
        // try testing.expectEqualStrings("alice@::1", result.?);
        assert_eq!(b"alice@::1".as_slice(), result.unwrap().as_slice());
    }

    // test { ... Joined ... } (the anonymous test inside the Joined struct)
    #[test]
    fn joined_format() {
        // { var w = .fixed(&buf); w.print("{f}", .{Joined{ .items = &.{} }}); expectEqualStrings("", ...); }
        {
            let items: Vec<Vec<u8>> = vec![];
            let mut buf: Vec<u8> = Vec::new();
            Joined { items: &items }.format(&mut buf).unwrap();
            assert_eq!(b"".as_slice(), buf.as_slice());
        }
        // { ... Joined{ .items = &.{"only"} } -> "only"; }
        {
            let items = vec![b"only".to_vec()];
            let mut buf: Vec<u8> = Vec::new();
            Joined { items: &items }.format(&mut buf).unwrap();
            assert_eq!(b"only".as_slice(), buf.as_slice());
        }
        // { ... Joined{ .items = &.{ "a", "b", "c" } } -> "a b c"; }
        {
            let items = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
            let mut buf: Vec<u8> = Vec::new();
            Joined { items: &items }.format(&mut buf).unwrap();
            assert_eq!(b"a b c".as_slice(), buf.as_slice());
        }
    }
}
