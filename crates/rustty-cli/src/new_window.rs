//! rustty-cli::new_window
//!
//! PORTS: ghostty src/cli/new_window.zig (0t) — the `+new-window` action: use
//! native platform IPC to open a new window in a running instance. The FULL
//! arg-parsing surface lands (`parseManuallyHook`: `--class`, `--working-directory`
//! realpath + `~` expansion, `-e` command capture, cwd injection); only IPC
//! *delivery* is deferred. Over the headless `none` backend
//! `apprt.App.performIpc` returns `false`, so this faithfully prints "+new-window
//! is not supported on this platform." to stderr and exits 1 (CLI_BLUEPRINT.md §1
//! / ADV-FIX 4 / Risk 4 — correct, NOT a stub). Byte-faithful (R1); the class /
//! working-directory / command args stay raw bytes (R3). R2: the "Ghostty" brand
//! in the doc comments is retargeted to the Rustty product name. 0 tests.

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
#[cfg(not(all(target_os = "linux", feature = "gtk")))]
use allocator_api2::alloc::Global;
#[cfg(not(all(target_os = "linux", feature = "gtk")))]
use rustty_apprt::none;
use rustty_apprt_api::ipc;
// const args = @import("args.zig");
use rustty_args::args;
use rustty_args::args::{ArgIterator, HookOutcome};
// const diagnostics = @import("diagnostics.zig");
use rustty_args::diagnostics::{DiagnosticList, Location, LocationProvider};
// const lib = @import("../lib/main.zig");
use rustty_lib::cut_prefix;
// const homedir = @import("../os/homedir.zig");
use rustty_os::expandHome;

use std::io::Write;

// std.ascii.whitespace = { ' ', '\t', '\n', 0x0b, 0x0c, '\r' } — note 0x0b
// (vertical tab), which Rust's `u8::is_ascii_whitespace` omits (R3 fidelity).
const ASCII_WHITESPACE: &[u8] = &[b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'];

/// The error set of the reference `parseManuallyHook`/`checkArg`:
/// `error{InvalidValue} || homedir.ExpandError || std.fs.Dir.RealPathAllocError
/// || Allocator.Error`. `checkArg`'s body only produces the `ExpandError`
/// (`expandHome`) and realpath (`RealPathAllocError` -> io::Error) failures;
/// `InvalidValue` is declared in the union but not produced, and `Allocator.Error`
/// is unreachable (Rust allocation panics).
#[derive(Debug)]
enum CheckArgError {
    // homedir.ExpandError
    ExpandHome(rustty_os::homedir::ExpandError),
    // std.fs.Dir.RealPathAllocError
    RealPath(std::io::Error),
}

impl std::fmt::Display for CheckArgError {
    // The reference `stderr.print("Error parsing args: {}\n", .{err})` prints the
    // underlying failure; carry each variant's error through.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckArgError::ExpandHome(e) => write!(f, "{e:?}"),
            CheckArgError::RealPath(e) => write!(f, "{e}"),
        }
    }
}

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// We store an I/O implementation to make maintenance easier.
    // _io: std.Io,
    //   PORT NOTE (`e8525c0fd`): upstream threads a `std.Io` handle through
    //   every filesystem call (`cwd.realPathFile(self._io, ...)`). Rust's
    //   `std::fs`/`std::path` are ambient, so there is no handle to store and
    //   the field is deliberately absent.

    /// This is set by the CLI parser for deinit.
    // _arena: ?ArenaAllocator = null,
    //   PORT NOTE: no shared arena; each collected value owns its bytes. The struct
    //   still opts in via `arena_available` (the reference's `@hasField(T, "_arena")`).

    /// If set, open up a new window in a custom instance of Rustty.
    // class: ?[:0]const u8 = null,
    pub class: Option<Vec<u8>>,

    /// Did the user specify a `--working-directory` argument on the command line?
    // _working_directory_seen: bool = false,
    pub _working_directory_seen: bool,

    /// All of the arguments after `+new-window`. They will be sent to Rustty
    /// for processing.
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
    /// `record_replay_arg` drain is not (aliasing; see the `Target` PORT NOTE).
    _raw: Vec<Vec<u8>>,
}

impl Options {
    // fn checkArg(self: *Options, alloc: Allocator, arg: []const u8) (error{InvalidValue}
    //     || homedir.ExpandError || std.fs.Dir.RealPathAllocError || Allocator.Error)!?[:0]const u8 {
    fn check_arg(&mut self, arg: &[u8]) -> Result<Option<Vec<u8>>, CheckArgError> {
        // if (lib.cutPrefix(u8, arg, "--class=")) |rest| {
        if let Some(rest) = cut_prefix(arg, b"--class=") {
            // self.class = try alloc.dupeZ(u8, std.mem.trim(u8, rest, &std.ascii.whitespace));
            self.class = Some(trim(rest, ASCII_WHITESPACE).to_vec());
            // return null;
            return Ok(None);
        }

        // if (lib.cutPrefix(u8, arg, "--working-directory=")) |rest| {
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
            // const cwd: std.fs.Dir = std.fs.cwd();
            // var expandhome_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const expanded = try homedir.expandHome(stripped, &expandhome_buf);
            let mut expandhome_buf = [0u8; libc::PATH_MAX as usize];
            let expanded = expandHome(&rustty_global::environ_map(), stripped, &mut expandhome_buf)
                .map_err(CheckArgError::ExpandHome)?;
            // var realpath_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const realpath = try cwd.realpath(expanded, &realpath_buf);
            let realpath = realpath_bytes(expanded).map_err(CheckArgError::RealPath)?;
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
    /// fallible (see `_raw` / the `Target` PORT NOTE). `_raw` holds the triggering
    /// arg (index 0) followed by the drained rest; this reproduces the hook's
    /// `e_seen` loop line-for-line, propagating `checkArg` errors as the reference
    /// `try` does (surfaced by `runArgs` as "Error parsing args").
    // pub fn parseManuallyHook(self: *Options, alloc: Allocator, arg: []const u8, iter: anytype) !bool {
    fn gather_arguments(&mut self) -> Result<(), CheckArgError> {
        let raw = std::mem::take(&mut self._raw);
        let mut it = raw.iter();
        // The triggering arg. (`_raw` is empty when `+new-window` had no trailing
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
    // The reference `Options`' only non-`_` field is `class`, and it is set via
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

    /// Manual parse hook, collect all of the arguments after `+new-window`.
    // pub fn parseManuallyHook(self: *Options, alloc, arg, iter) !bool {
    //
    // PORT NOTE (iterator threading): the reference hook drains `iter` inline into
    // `_arguments` (applying `checkArg` + the `-e` capture). The Rust `Target` hook
    // can't hold `iter` (aliasing; see memory `rust-mut-args-strongly-protected`),
    // and — unlike ssh's pass-through drain — new_window's per-arg transformation
    // is FALLIBLE, which the driver's infallible `record_replay_arg` cannot carry.
    // The faithful split: the hook + `record_replay_arg` collect the raw args (the
    // triggering arg here, the rest in `record_replay_arg`) into `_raw`, and
    // `runArgs` calls `gather_arguments` right after `parse` to run the verbatim
    // hook body with `?` error propagation. `ConsumeRest` makes the driver drain
    // the rest; `apply_e_command` returns `false` (the hook's `return false`).
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

/// The `new-window` will use native platform IPC to open up a new window in a
/// running instance of Rustty.
///
/// If the `--class` flag is not set, the `new-window` command will try and
/// connect to a running instance of Rustty based on what optimizations the
/// Rustty CLI was compiled with. Otherwise the `new-window` command will try
/// and contact a running Rustty instance that was configured with the same
/// `class` as was given on the command line.
///
/// All of the arguments after the `+new-window` argument (except for the
/// `--class` flag) will be sent to the remote Rustty instance and will be
/// parsed as command line flags. These flags will override certain settings
/// when creating the first surface in the new window. Currently, only
/// `--working-directory`, `--command`, and `--title` are supported. `-e` will
/// also work as an alias for `--command`, except that if `-e` is found on the
/// command line all following arguments will become part of the command and no
/// more arguments will be parsed for configuration settings.
///
/// If `--working-directory` is found on the command line and is a relative
/// path (i.e. doesn't start with `/`) it will be resolved to an absolute path
/// relative to the current working directory that the `rustty +new-window`
/// command is run from. `~/` prefixes will also be expanded to the user's home
/// directory.
///
/// If `--working-directory` is _not_ found on the command line, the working
/// directory that `rustty +new-window` is run from will be passed to Rustty.
///
/// GTK uses an application ID to identify instances of applications. If Rustty
/// is compiled with release optimizations, the default application ID will be
/// `io.airely.rustty`. If Rustty is compiled with debug optimizations,
/// the default application ID will be `io.airely.rustty-debug`.  The
/// `class` configuration entry can be used to set up a custom application
/// ID. The class name must follow the requirements defined [in the GTK
/// documentation](https://docs.gtk.org/gio/type_func.Application.id_is_valid.html)
/// or it will be ignored and Rustty will use the default as defined above.
///
/// On GTK, D-Bus activation must be properly configured. Rustty does not need
/// to be running for this to open a new window, making it suitable for binding
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
///   * `--command`: The command to be executed in the first surface of the new window.
///
///   * `--working-directory=<directory>`: The working directory to pass to Rustty.
///
///   * `--title`: A title that will override the title of the first surface in
///     the new window. The title override may be edited or removed later.
///
///   * `-e`: Any arguments after this will be interpreted as a command to
///     execute inside the first surface of the new window instead of the
///     default command.
///
/// Available since: 1.2.0
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var iter = try args.argsIterator(alloc);
    // defer iter.deinit();
    let mut iter = args::args_iterator()?;

    // var buffer: [1024]u8 = undefined;
    // var stderr_writer = std.fs.File.stderr().writer(&buffer);
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
    // var opts: Options = .{};
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
            writeln!(stderr, "Error parsing args: {err:?}")?;
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
            //   PORT NOTE: the only non-`_` field of Options is `class`, so the
            //   `inline for` over the struct fields reduces to a single `class` check.
            //   (`{f}` -> `Diagnostic::format`.)
            if diagnostic.key == b"class" {
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
        // const cwd: std.fs.Dir = std.fs.cwd();
        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // const wd = try cwd.realpath(".", &buf);
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

    // if (apprt.App.performIpc(
    //     alloc,
    //     if (opts.class) |class| .{ .class = class } else .detect,
    //     .new_window,
    //     .{
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
    let new_window = ipc::NewWindow {
        // .arguments = if (opts._arguments.items.len == 0) null else opts._arguments.items,
        arguments: if opts._arguments.is_empty() {
            None
        } else {
            Some(&arg_refs)
        },
    };
    let ipc_action = ipc::Action::NewWindow(new_window);
    // Route through the GTK apprt's D-Bus IPC on Linux/GTK; the headless CLI build
    // falls back to the `none` backend (always "not performed").
    #[cfg(all(target_os = "linux", feature = "gtk"))]
    let ipc_result = rustty_apprt::gtk::App::perform_ipc(target, ipc_action);
    #[cfg(not(all(target_os = "linux", feature = "gtk")))]
    let ipc_result = none::App::perform_ipc(Global, target, ipc_action);
    match ipc_result {
        // if (apprt.App.performIpc(...)) return 0;
        Ok(delivered) => {
            if delivered {
                return Ok(0);
            }
        }
        // error.IPCFailed => {
        //     // The apprt should have printed a more specific error message already.
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
    // try stderr.print("+new-window is not supported on this platform.\n", .{});
    writeln!(stderr, "+new-window is not supported on this platform.")?;
    // return 1;
    Ok(1)
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
