//! rustty-cli::new_tab
//!
//! PORTS: ghostty src/cli/new_tab.zig (034506f14 + 3650bef4e end state) — the
//! `+new-tab` action: use native platform IPC to open a new tab in a running
//! instance. The FULL arg-parsing surface lands (`parseManuallyHook`:
//! `--class`, `--surface-id` (base-0 parse), `--working-directory` realpath +
//! `~` expansion, `-e` command capture, cwd injection); only IPC *delivery* is
//! platform-dependent. Over the headless `none` backend
//! `apprt.App.performIpc` returns `false`, so this faithfully prints
//! "+new-tab is not supported on this platform." to stderr and exits 1 (the
//! same non-stub shape as `new_window.rs`, its template). Byte-faithful (R1);
//! the class / working-directory / command args stay raw bytes (R3). R2: the
//! "Ghostty" brand in the doc comments is retargeted to the Rustty product
//! name (incl. `GHOSTTY_SURFACE_ID` -> `{ENV_PREFIX}_SURFACE_ID`). 0 tests.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const Action = @import("../cli.zig").ghostty.Action;
use crate::rustty::{Action, Error};
// const apprt = @import("../apprt.zig");
//   PORT NOTE: the reference `apprt.App` is the comptime-selected
//   `apprt.runtime.App`; for the headless `rustty` CLI build that is `none.App`,
//   whose `perform_ipc` always returns `Ok(false)`. The ipc value types come from
//   `rustty-apprt-api` (rustty-apprt does not re-export them).
#[cfg(not(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk")))]
use allocator_api2::alloc::Global;
#[cfg(not(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk")))]
use rustty_apprt::none;
use rustty_apprt_api::ipc;
// const args = @import("args.zig");
use rustty_args::args;
use rustty_args::args::{ArgIterator, HookOutcome};
// const diagnostics = @import("diagnostics.zig");
use rustty_args::diagnostics::{DiagnosticList, Location, LocationProvider};
// const homedir = @import("../os/homedir.zig");
use rustty_os::expandHome;
// const internal_os = @import("../os/main.zig");
//   PORT NOTE: imported but unused by the reference file body (the
//   `--working-directory` machinery goes through `homedir`); nothing to bind.
// const global = @import("../global.zig");
//   -> `rustty_global` (environ map + args).

use std::io::Write;

// std.ascii.whitespace = { ' ', '\t', '\n', 0x0b, 0x0c, '\r' } — note 0x0b
// (vertical tab), which Rust's `u8::is_ascii_whitespace` omits (R3 fidelity).
const ASCII_WHITESPACE: &[u8] = &[b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'];

/// `std.fmt.ParseIntError = error{ Overflow, InvalidCharacter }` — the errors
/// of the reference's `std.fmt.parseUnsigned(u64, buf, 0)` calls.
use rustty_args::std_fmt::ParseIntError;

/// The error set of the reference `checkArg`: `error{InvalidValue} ||
/// error{Unexpected} || std.fmt.ParseIntError || homedir.ExpandError ||
/// std.Io.Dir.RealPathFileError || Allocator.Error`. The body only produces
/// the parse-int (`--surface-id=`), `ExpandError` (`expandHome`) and realpath
/// failures; `InvalidValue`/`Unexpected` are declared in the union but not
/// produced, and `Allocator.Error` is unreachable (Rust allocation panics).
#[derive(Debug)]
enum CheckArgError {
    // std.fmt.ParseIntError
    ParseInt(ParseIntError),
    // homedir.ExpandError
    ExpandHome(rustty_os::homedir::ExpandError),
    // std.Io.Dir.RealPathFileError — the reference error's name.
    RealPath(&'static str),
}

impl std::fmt::Display for CheckArgError {
    // The reference `stderr.print("Error parsing args: {}\n", .{err})` prints the
    // underlying failure; carry each variant's error through.
    //
    // PORT NOTE (parity S69-F5, task T-C): Zig's `{}` of an error value is
    // `error.<Name>`; the realpath arm carries the reference's error name for
    // the failed path (`zig_error::realpath_error_name`), not the OS message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckArgError::ParseInt(e) => write!(f, "error.{e:?}"),
            CheckArgError::ExpandHome(e) => write!(f, "error.{e:?}"),
            CheckArgError::RealPath(name) => write!(f, "error.{name}"),
        }
    }
}

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// We store an I/O implementation to make maintenance easier.
    // _io: std.Io,
    //   PORT NOTE (`e8525c0fd`): upstream threads a `std.Io` handle through
    //   every filesystem call (`cwd.realPathFile(global.io(), ...)`). Rust's
    //   `std::fs`/`std::path` are ambient, so there is no handle to store and
    //   the field is deliberately absent.

    /// This is set by the CLI parser for deinit.
    // _arena: ?ArenaAllocator = null,
    //   PORT NOTE: no shared arena; each collected value owns its bytes. The struct
    //   still opts in via `arena_available` (the reference's `@hasField(T, "_arena")`).

    /// If set, open up a new tab in a custom instance of Rustty.
    // class: ?[:0]const u8 = null,
    pub class: Option<Vec<u8>>,

    /// The surface to target.
    // @"surface-id": ?u64 = null,
    pub surface_id: Option<u64>,

    /// Did the user specify a `--working-directory` argument on the command line?
    // _working_directory_seen: bool = false,
    pub _working_directory_seen: bool,

    /// All of the arguments after `+new-tab`. They will be sent to Rustty
    /// for processing.
    //
    // PORT NOTE: the reference reads "sent to Ghosttty" (`new_tab.zig:28`), a
    // typo of the brand name, and `new_window.zig:24` has the same typo. It is
    // a brand token in doc prose, so it is retargeted to Rustty like every other
    // brand mention in this file (R2, module header), and matches
    // `new_window.rs`. The typo is recorded for upstream as
    // `.port/parity-closure/UPSTREAM-DEFECTS.md` UD43. (Task R13-R; this note
    // used to keep "Ghosttty" verbatim while `new_window.rs` did not.)
    // _arguments: std.ArrayList([:0]const u8) = .empty,
    pub _arguments: Vec<Vec<u8>>,

    /// Enable arg parsing diagnostics so that we don't get an error if
    /// there is a "normal" config setting on the cli.
    // _diagnostics: diagnostics.DiagnosticList = .{},
    pub _diagnostics: DiagnosticList,

    /// PORT NOTE: glue: the raw args the reference `parseManuallyHook` drains from
    /// `iter` (the triggering arg + the rest), collected here by the driver's
    /// ConsumeRest path (`parse_manually_hook` + `record_replay_arg`) and turned
    /// into `_arguments` by `gather_arguments` after `parse`. The transformation
    /// is split out of the hook because `checkArg` is FALLIBLE and the driver's
    /// `record_replay_arg` drain is not (aliasing; see `new_window.rs`).
    _raw: Vec<Vec<u8>>,
}

impl Options {
    // fn checkArg(self: *Options, alloc: Allocator, arg: []const u8) CheckArgError!?[:0]const u8 {
    fn check_arg(&mut self, arg: &[u8]) -> Result<Option<Vec<u8>>, CheckArgError> {
        // if (std.mem.cutPrefix(u8, arg, "--surface-id=")) |rest| {
        if let Some(rest) = cut_prefix(arg, b"--surface-id=") {
            // self.@"surface-id" = try std.fmt.parseUnsigned(u64, std.mem.trim(u8, rest, &std.ascii.whitespace), 0);
            self.surface_id = Some(
                parse_unsigned_u64_base0(trim(rest, ASCII_WHITESPACE))
                    .map_err(CheckArgError::ParseInt)?,
            );
            // return null;
            return Ok(None);
        }

        // if (std.mem.cutPrefix(u8, arg, "--class=")) |rest| {
        if let Some(rest) = cut_prefix(arg, b"--class=") {
            // self.class = try alloc.dupeZ(u8, std.mem.trim(u8, rest, &std.ascii.whitespace));
            self.class = Some(trim(rest, ASCII_WHITESPACE).to_vec());
            // return null;
            return Ok(None);
        }

        // if (std.mem.cutPrefix(u8, arg, "--working-directory=")) |rest| {
        if let Some(rest) = cut_prefix(arg, b"--working-directory=") {
            // const stripped = std.mem.trim(u8, rest, &std.ascii.whitespace);
            let stripped = trim(rest, ASCII_WHITESPACE);
            // if (std.mem.eql(u8, stripped, "home")) return try alloc.dupeZ(u8, arg);
            if stripped == b"home" {
                return Ok(Some(arg.to_vec()));
            }
            // if (std.mem.eql(u8, stripped, "inherit")) return try alloc.dupeZ(u8, arg);
            if stripped == b"inherit" {
                return Ok(Some(arg.to_vec()));
            }
            // const cwd: std.Io.Dir = .cwd();
            // var expandhome_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const expanded = expanded: {
            //     var env = try global.environMap();
            //     defer env.deinit();
            //     break :expanded try homedir.expandHome(global.io(), &env, stripped, &expandhome_buf);
            // };
            let mut expandhome_buf = [0u8; libc::PATH_MAX as usize];
            let expanded = expandHome(&rustty_global::environ_map(), stripped, &mut expandhome_buf)
                .map_err(CheckArgError::ExpandHome)?;
            // var realpath_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const realpath = realpath_buf[0..try cwd.realPathFile(global.io(), expanded, &realpath_buf)];
            let realpath = realpath_bytes(expanded).map_err(|e| {
                CheckArgError::RealPath(crate::zig_error::realpath_error_name(expanded, &e))
            })?;
            // self._working_directory_seen = true;
            self._working_directory_seen = true;
            // return try std.fmt.allocPrintSentinel(alloc, "--working-directory={s}", .{realpath}, 0);
            let mut out = b"--working-directory=".to_vec();
            out.extend_from_slice(&realpath);
            return Ok(Some(out));
        }

        // return try alloc.dupeZ(u8, arg);
        Ok(Some(arg.to_vec()))
    }

    /// The reference `parseManuallyHook` body, deferred here because `checkArg` is
    /// fallible (see `_raw` / `new_window.rs`'s `Target` PORT NOTE). `_raw` holds
    /// the triggering arg (index 0) followed by the drained rest; this reproduces
    /// the hook's `e_seen` loop line-for-line, propagating `checkArg` errors as
    /// the reference `try` does (surfaced by `runArgs` as "Error parsing args").
    // pub fn parseManuallyHook(self: *Options, alloc: Allocator, arg: []const u8, iter: anytype) ParseManuallyHookError!bool {
    fn gather_arguments(&mut self) -> Result<(), CheckArgError> {
        let raw = std::mem::take(&mut self._raw);
        let mut it = raw.iter();
        // The triggering arg. (`_raw` is empty when `+new-tab` had no trailing
        // args, so the hook never fired — nothing to gather.)
        let Some(arg) = it.next() else {
            return Ok(());
        };
        // var e_seen: bool = std.mem.eql(u8, arg, "-e");
        let mut e_seen: bool = arg.as_slice() == b"-e";

        // Include the argument that triggered the manual parse hook.
        // if (try self.checkArg(alloc, arg)) |a| try self._arguments.append(alloc, a);
        if let Some(a) = self.check_arg(arg)? {
            self._arguments.push(a);
        }

        // Gather up the rest of the arguments to use as the command.
        // while (iter.next()) |param| {
        for param in it {
            let param: &[u8] = param;
            // if (e_seen) {
            //     try self._arguments.append(alloc, try alloc.dupeZ(u8, param));
            //     continue;
            // }
            if e_seen {
                self._arguments.push(param.to_vec());
                continue;
            }
            // if (std.mem.eql(u8, param, "-e")) {
            //     e_seen = true;
            //     try self._arguments.append(alloc, try alloc.dupeZ(u8, param));
            //     continue;
            // }
            if param == b"-e" {
                e_seen = true;
                self._arguments.push(param.to_vec());
                continue;
            }
            // if (try self.checkArg(alloc, param)) |a| try self._arguments.append(alloc, a);
            if let Some(a) = self.check_arg(param)? {
                self._arguments.push(a);
            }
        }

        // return false; — halt (handled by `apply_e_command` returning false).
        Ok(())
    }

    // pub fn deinit(self: *Options) void { if (self._arena) |arena| arena.deinit(); self.* = undefined; }
    //   PORT NOTE: owned values drop naturally; the reference `defer opts.deinit()`
    //   maps to Rust's drop-at-scope-end.
    pub fn deinit(&mut self) {
        let _ = self;
    }
}

impl args::Target for Options {
    // The reference `Options`' non-`_` fields (`class`, `surface-id`) are set via
    // `checkArg` inside the manual hook (which consumes every arg), so
    // `parseIntoField` is never reached; it falls through to `InvalidField`.
    fn parse_into_field(
        &mut self,
        _key: &[u8],
        _value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        Err(args::Error::InvalidField)
    }

    // @hasField(Options, "_arena") == true.
    fn arena_available(&self) -> bool {
        true
    }

    // canTrackDiags(Options) == @hasField(Options, "_diagnostics") == true.
    fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
        Some(&mut self._diagnostics)
    }

    /// Manual parse hook, collect all of the arguments after `+new-tab`.
    // pub fn parseManuallyHook(self: *Options, alloc, arg, iter) ParseManuallyHookError!bool {
    //
    // PORT NOTE (iterator threading): same split as `new_window.rs` — the hook +
    // `record_replay_arg` collect the raw args into `_raw`, and `runArgs` calls
    // `gather_arguments` right after `parse` to run the verbatim hook body with
    // `?` error propagation. `ConsumeRest` makes the driver drain the rest;
    // `apply_e_command` returns `false` (the hook's `return false`).
    fn parse_manually_hook(&mut self, arg: &[u8]) -> Option<Result<HookOutcome, args::ParseError>> {
        self._raw.push(arg.to_vec());
        Some(Ok(HookOutcome::ConsumeRest))
    }

    // The `ConsumeRest` drain step: collect each drained arg into `_raw` for
    // `gather_arguments` (the reference hook's `while (iter.next())` body).
    fn record_replay_arg(&mut self, arg: &[u8]) {
        self._raw.push(arg.to_vec());
    }

    // The reference hook's `return false` (halt) after draining.
    fn apply_e_command(&mut self, _command: Vec<Vec<u8>>, _location: Location) -> bool {
        false
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    /// Enables "-h" and "--help" to work.
    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `new-tab` will use native platform IPC to open up a new tab in a running
/// instance of Rustty.
///
/// If the `--class` flag is not set, the `new-tab` command will try and connect
/// to a running instance of Rustty based on what optimizations the Rustty
/// CLI was compiled with. Otherwise the `new-tab` command will try and contact
/// a running Rustty instance that was configured with the same `class` as was
/// given on the command line.
///
/// All of the arguments after the `+new-tab` argument (except for the `--class`
/// and `--surface-id` flag) will be sent to the remote Rustty instance and
/// will be parsed as command line flags. These flags will override certain
/// settings when creating the first surface in the new tab. Currently, only
/// `--working-directory`, `--command`, `--shell-integration=<mode>`, and
/// `--title` are supported. `-e` will also work as an alias for `--command`, except that if
/// `-e` is found on the command line all following arguments will become part
/// of the command and no more arguments will be parsed for configuration
/// settings.
///
/// If `--working-directory` is found on the command line and is a relative
/// path (i.e. doesn't start with `/`) it will be resolved to an absolute
/// path relative to the current working directory that the `rustty +new-tab`
/// command is run from. `~/` prefixes will also be expanded to the user's home
/// directory.
///
/// If `--working-directory` is _not_ found on the command line, the working
/// directory that `rustty +new-tab` is run from will be passed to Rustty.
///
/// GTK uses an application ID to identify instances of applications. If Rustty
/// is compiled with release optimizations, the default application ID will be
/// `io.airely.rustty`. If Rustty is compiled with debug optimizations, the
/// default application ID will be `io.airely.rustty-debug`.  The
/// `class` configuration entry can be used to set up a custom application
/// ID. The class name must follow the requirements defined [in the GTK
/// documentation](https://docs.gtk.org/gio/type_func.Application.id_is_valid.html)
/// or it will be ignored and Rustty will use the default as defined above.
///
/// On GTK, D-Bus activation must be properly configured. Rustty does not need
/// to be running for this to open a new tab, making it suitable for binding
/// to keys in your window manager (if other methods for configuring global
/// shortcuts are unavailable). D-Bus will handle launching a new instance
/// of Rustty if it is not already running. See the Rustty website for
/// information on properly configuring D-Bus activation.
///
/// Only supported on GTK.
///
/// Flags:
///
///   * `--class=<class>`: If set, open up a new window in a custom instance of
///     Rustty. The class must be a valid GTK application ID.
///
///   * `--surface-id=<surface id>`: If set, specifies a Rustty surface ID.
///     This is used to identify the window that the new tab will be added to.
///     If the surface ID is not specified or is `0` the currently focused
///     surface will be used.
///
///   * `--command`: The command to be executed in the first surface of the new tab.
///
///   * `--shell-integration=<mode>`: Whether to enable shell integration and
///     which shell to use. See the main configuration documentation for
///     supported values.
///
///   * `--working-directory=<directory>`: The working directory to pass to Rustty.
///
///   * `--title`: A title that will override the title of the first surface in
///     the new tab. The title override may be edited or removed later.
///
///   * `-e`: Any arguments after this will be interpreted as a command to
///     execute inside the first surface of the new tab instead of the
///     default command.
///
/// Available since: 1.4.0
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var iter = try args.argsIterator(alloc, global.args());
    // defer iter.deinit();
    let mut iter = args::args_iterator()?;

    // var buffer: [1024]u8 = undefined;
    // var stderr_writer = std.Io.File.stderr().writer(global.io(), &buffer);
    // const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // const result = runArgs(alloc, &iter, stderr);
    let result = run_args(&mut iter, &mut stderr);
    // stderr.flush() catch {};
    let _ = stderr.flush();
    // defer iter.deinit(); — runs at scope end, after the flush.
    iter.deinit();
    // return result;
    result
}

// fn runArgs(alloc_gpa: Allocator, argsIter: anytype, stderr: *std.Io.Writer) !u8 {
fn run_args<I, We>(args_iter: &mut I, stderr: &mut We) -> Result<u8, Error>
where
    I: ArgIterator + LocationProvider,
    We: Write,
{
    // var opts: Options = .{ ._io = global.io() };
    // defer opts.deinit();
    let mut opts = Options::default();

    // args.parse(Options, alloc_gpa, &opts, argsIter) catch |err| switch (err) {
    //     error.ActionHelpRequested => return err,
    //     else => { try stderr.print("Error parsing args: {}\n", .{err}); return 1; },
    // };
    match args::parse(&mut opts, args_iter) {
        Ok(()) => {}
        // error.ActionHelpRequested => return err,
        Err(args::Error::ActionHelpRequested) => return Err(Error::ActionHelpRequested),
        // else => { stderr.print("Error parsing args: {}\n", .{err}); return 1; },
        Err(err) => {
            writeln!(
                stderr,
                "Error parsing args: {}",
                crate::zig_error::args_error(&err)
            )?;
            return Ok(1);
        }
    }

    // PORT NOTE: run the deferred `parseManuallyHook` transformation (`checkArg` +
    // `-e`) over the collected `_raw`. In the reference this runs INSIDE
    // args.parse, so its error is caught by the `else` arm above; here it is
    // surfaced identically right after parse.
    if let Err(err) = opts.gather_arguments() {
        writeln!(stderr, "Error parsing args: {err}")?;
        return Ok(1);
    }

    // Print out any diagnostics, unless it's likely that the diagnostic was
    // generated trying to parse a "normal" configuration setting. Exit with an
    // error code if any diagnostics were printed.
    // if (!opts._diagnostics.empty()) {
    if !opts._diagnostics.empty() {
        // var exit: bool = false;
        let mut exit: bool = false;
        // outer: for (opts._diagnostics.items()) |diagnostic| {
        'outer: for diagnostic in opts._diagnostics.items() {
            // if (diagnostic.location != .cli) continue :outer;
            if !matches!(diagnostic.location, Location::Cli(_)) {
                continue 'outer;
            }
            // inner: inline for (@typeInfo(Options).@"struct".fields) |field| {
            //     if (field.name[0] == '_') continue :inner;
            //     if (std.mem.eql(u8, field.name, diagnostic.key)) {
            //         try stderr.print("config error: {f}\n", .{diagnostic});
            //         exit = true;
            //     }
            // }
            //   PORT NOTE: the non-`_` fields of Options are `class` and
            //   `surface-id`, so the `inline for` over the struct fields reduces
            //   to those two checks. (`{f}` -> `Diagnostic::format`.)
            if diagnostic.key == b"class" || diagnostic.key == b"surface-id" {
                let mut msg = Vec::new();
                diagnostic.format(&mut msg);
                write!(stderr, "config error: ")?;
                stderr.write_all(&msg)?;
                writeln!(stderr)?;
                exit = true;
            }
        }
        // if (exit) return 1;
        if exit {
            return Ok(1);
        }
    }

    // if (!opts._working_directory_seen) {
    if !opts._working_directory_seen {
        // const alloc = opts._arena.?.allocator();
        // const cwd: std.Io.Dir = .cwd();
        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // const wd = buf[0..try cwd.realPathFile(global.io(), ".", &buf)];
        let wd = realpath_bytes(b".")?;
        // This should be inserted at the beginning of the list, just in case `-e` was used.
        // try opts._arguments.insert(alloc, 0, try std.fmt.allocPrintSentinel(alloc, "--working-directory={s}", .{wd}, 0));
        let mut wd_arg = b"--working-directory=".to_vec();
        wd_arg.extend_from_slice(&wd);
        opts._arguments.insert(0, wd_arg);
    }

    // var arena = ArenaAllocator.init(alloc_gpa);
    // defer arena.deinit();
    // const alloc = arena.allocator();
    //   PORT NOTE: no arena; `Global` is passed as the (none-ignored) allocator.

    // const surface_id = opts.@"surface-id" orelse surface: {
    //     var env = global.environMap() catch break :surface 0;
    //     defer env.deinit();
    //     const e = env.get("GHOSTTY_SURFACE_ID") orelse break :surface 0;
    //     break :surface std.fmt.parseUnsigned(u64, e, 0) catch break :surface 0;
    // };
    let surface_id: u64 = match opts.surface_id {
        Some(v) => v,
        None => {
            let env = rustty_global::environ_map();
            // R2: `GHOSTTY_SURFACE_ID` -> the product env prefix (the same
            // `{ENV_PREFIX}_SURFACE_ID` the core surface exports).
            let key = format!("{}_SURFACE_ID", rustty_identity::ENV_PREFIX);
            match env.get(key.as_bytes()) {
                Some(e) => parse_unsigned_u64_base0(e).unwrap_or(0),
                None => 0,
            }
        }
    };

    // if (apprt.App.performIpc(
    //     alloc,
    //     if (opts.class) |class| .{ .class = class } else .detect,
    //     .new_tab,
    //     .{
    //         .surface_id = surface_id,
    //         .arguments = if (opts._arguments.items.len == 0) null else opts._arguments.items,
    //     },
    // ) catch |err| switch (err) { ... }) return 0;
    let target = match &opts.class {
        // if (opts.class) |class| .{ .class = class }
        Some(class) => ipc::Target::Class(class),
        // else .detect
        None => ipc::Target::Detect,
    };
    let arg_refs: Vec<&[u8]> = opts._arguments.iter().map(|a| a.as_slice()).collect();
    let new_tab = ipc::NewTab {
        // .surface_id = surface_id,
        surface_id,
        // .arguments = if (opts._arguments.items.len == 0) null else opts._arguments.items,
        arguments: if opts._arguments.is_empty() {
            None
        } else {
            Some(&arg_refs)
        },
    };
    let ipc_action = ipc::Action::NewTab(new_tab);
    // Route through the GTK apprt's D-Bus IPC on a GTK build (Linux/FreeBSD — the
    // two targets `Runtime.default()` maps to `.gtk`, `apprt/runtime.zig:14-24`);
    // the headless CLI build falls back to the `none` backend (always "not
    // performed"). `apprt/ipc.zig` itself has no os switch — the backend is
    // apprt-runtime-selected, so FreeBSD rides the gtk arm exactly like Linux.
    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
    let ipc_result = rustty_apprt::gtk::App::perform_ipc(target, ipc_action);
    #[cfg(not(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk")))]
    let ipc_result = none::App::perform_ipc(Global, target, ipc_action);
    match ipc_result {
        // if (apprt.App.performIpc(...)) return 0;
        Ok(delivered) => {
            if delivered {
                return Ok(0);
            }
        }
        // error.IPCFailed => {
        //     // The apprt should have printed a more specific error message
        //     // already.
        //     return 1;
        // },
        // else => { try stderr.print("Sending the IPC failed: {}", .{err}); return 1; },
        //   PORT NOTE: `ipc::Errors` has the single variant `IPCFailed`, so the
        //   reference's `else` arm collapses into this one; both reference branches
        //   `return 1` (the `else` would additionally print "Sending the IPC
        //   failed: {}", unreachable for the current single-variant error set).
        Err(ipc::Errors::IPCFailed) => return Ok(1),
    }

    // If we get here, the platform is not supported.
    // try stderr.print("+new-tab is not supported on this platform.\n", .{});
    writeln!(stderr, "+new-tab is not supported on this platform.")?;
    // return 1;
    Ok(1)
}

/// `std.mem.cutPrefix(u8, arg, prefix)` — the end-state reference uses the
/// (upstreamed) std helper (`1d6bc6829` deleted `lib/string.zig`'s
/// `cutPrefix`); identical semantics, inlined here to keep this file's helper
/// set self-contained like its template.
fn cut_prefix<'a>(slice: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    slice.strip_prefix(prefix)
}

/// `std.mem.trim(u8, slice, values)` — trim leading/trailing bytes contained in
/// `values` (here `std.ascii.whitespace`).
fn trim<'a>(slice: &'a [u8], values: &[u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = slice.len();
    while start < end && values.contains(&slice[start]) {
        start += 1;
    }
    while end > start && values.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[start..end]
}

/// `std.fs.Dir.realpath(path, buf)` — resolve `path` to an absolute canonical
/// path (resolving symlinks; the target must exist). R3: paths are byte strings.
fn realpath_bytes(path: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let canon = std::fs::canonicalize(std::path::Path::new(std::ffi::OsStr::from_bytes(path)))?;
        Ok(canon.into_os_string().into_vec())
    }
    #[cfg(not(unix))]
    {
        let s = std::str::from_utf8(path)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        let canon = std::fs::canonicalize(s)?;
        Ok(canon.into_os_string().into_encoded_bytes())
    }
}

/// PORTS: `std.fmt.parseUnsigned(u64, buf, 0)`, through the ported std parser
/// (`rustty_args::std_fmt::parse_unsigned`).
//
// PORT NOTE (NEW-FINDINGS N122): this was a hand-written copy that stripped a
// leading `+`, on the belief that `parseUnsigned` is `parseIntWithSign(.pos)`
// AFTER a sign strip. It is not: `parseUnsigned` passes `buf` straight to
// `parseIntWithSign(.., .pos)`, so a `+` reaches `charToDigit` and is
// `error.InvalidCharacter` (std's own `test parseUnsigned`: `parseUnsigned(u8,
// "+0", 10)` is `error.InvalidCharacter`; MEASURED with Zig 0.16.0:
// `parseUnsigned(u64, "+5", 0)` is `error.InvalidCharacter`). The port
// accepted `--surface-id=+5` and a `+5` surface-id env value where the
// reference rejects them.
fn parse_unsigned_u64_base0(buf: &[u8]) -> Result<u64, ParseIntError> {
    rustty_args::std_fmt::parse_unsigned::<u64>(buf, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct VecArgs(std::vec::IntoIter<Vec<u8>>);
    impl ArgIterator for VecArgs {
        type Item = Vec<u8>;
        fn next(&mut self) -> Option<Vec<u8>> {
            self.0.next()
        }
    }
    impl LocationProvider for VecArgs {}

    fn run_with(arg: &[u8]) -> (u8, Vec<u8>) {
        let mut it = VecArgs(vec![arg.to_vec()].into_iter());
        let mut stderr: Vec<u8> = Vec::new();
        let rc = run_args(&mut it, &mut stderr).unwrap();
        (rc, stderr)
    }

    // parity S69-F5: `stderr.print("Error parsing args: {}\n", .{err})` prints a
    // Zig error value as `error.<Name>`. The realpath name is the one Zig 0.16.0
    // `realPathFile` returns for a missing path (MEASURED: error.FileNotFound).
    #[test]
    fn error_parsing_args_prints_the_zig_error_name() {
        assert_eq!(
            (1, b"Error parsing args: error.FileNotFound\n".to_vec()),
            run_with(b"--working-directory=/nonexistent-rustty-t-c/x")
        );
        assert_eq!(
            (1, b"Error parsing args: error.InvalidCharacter\n".to_vec()),
            run_with(b"--surface-id=abc")
        );
    }

    // NEW-FINDINGS N122: `std.fmt.parseUnsigned` takes no sign, so the
    // reference rejects `--surface-id=+5` with `error.InvalidCharacter`
    // (MEASURED with Zig 0.16.0) and prints it. An unsigned id still parses,
    // with base-0 prefix detection and `_` separators.
    #[test]
    fn surface_id_rejects_a_leading_plus_like_parse_unsigned() {
        assert_eq!(
            (1, b"Error parsing args: error.InvalidCharacter\n".to_vec()),
            run_with(b"--surface-id=+5")
        );

        let mut opts = Options::default();
        assert!(matches!(
            opts.check_arg(b"--surface-id=+5"),
            Err(CheckArgError::ParseInt(ParseIntError::InvalidCharacter))
        ));
        assert_eq!(opts.surface_id, None);
        assert!(matches!(opts.check_arg(b"--surface-id= 5 "), Ok(None)));
        assert_eq!(opts.surface_id, Some(5));
        assert!(matches!(opts.check_arg(b"--surface-id=0x1_0"), Ok(None)));
        assert_eq!(opts.surface_id, Some(16));

        assert_eq!(
            parse_unsigned_u64_base0(b"+5"),
            Err(ParseIntError::InvalidCharacter)
        );
        assert_eq!(
            parse_unsigned_u64_base0(b"-5"),
            Err(ParseIntError::InvalidCharacter)
        );
        assert_eq!(parse_unsigned_u64_base0(b"5"), Ok(5));
    }
}
