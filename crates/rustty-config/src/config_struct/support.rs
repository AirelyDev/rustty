//! PORTS: ghostty src/config/Config.zig — `Replay` (`Replay.Step`,
//! `Replay.iterator`) and `ChangeIterator`, plus the support glue the passes use:
//! `LoadError`, the `log` sink (`log_err`/`log_warn`/`log_info`/`log_debug`), the
//! std/OS helpers (`expandHome`, the config template, `probableCliEnvironment`) and
//! the `finalize` default-lookup guards.

// `log_capture` (a `#[cfg(test)]` helper module, not the tests) sits beside the
// log sink it captures, ahead of the rest of this file, as it did in
// config_struct.rs before the split.
#![allow(clippy::items_after_test_module)]

use super::*;

// ===========================================================================
// Replay (the `_replay_steps` element type) + ChangeIterator.
// ===========================================================================

/// PORTS: `Replay.Step` (Config.zig:5115) — a replayed parse step.
// const Step = union(enum) { arg, expand, conditional_arg, diagnostic, "-e" };
#[derive(Debug, Clone)]
pub enum ReplayStep {
    /// An argument to parse as if it came from the CLI or file.
    Arg(Vec<u8>),
    /// A base path to expand relative paths against.
    Expand(Vec<u8>),
    /// A conditional argument. This arg is parsed only if all
    /// conditions match (an "AND"). An "OR" can be achieved by
    /// having multiple conditional arg entries.
    ConditionalArg {
        conditions: Vec<Conditional>,
        arg: Vec<u8>,
    },
    /// A diagnostic to be added to the new configuration when
    /// replayed. This should only be used for diagnostics that won't
    /// be reproduced during playback. For example, `config-file`
    /// errors are not reloaded so they should be added here.
    ///
    /// Diagnostics cannot be conditional. They are always present
    /// even if the conditionals don't match. This helps users find
    /// errors in their configuration.
    Diagnostic(rustty_args::diagnostics::Diagnostic),
    /// The start of a "-e" argument. This marks the end of
    /// traditional configuration and the beginning of the
    /// "-e" initial command magic. This is separate from "arg"
    /// because there are some behaviors unique to this (i.e.
    /// we want to keep this at the end for config-file).
    ///
    /// Note: when "-e" is used, ONLY this is present and
    /// not an additional "arg" with "-e" value.
    DashE,
}

impl ReplayStep {
    // fn clone(self: Step, alloc: Allocator) Allocator.Error!Step {
    //     return switch (self) {
    //         .@"-e" => self,
    //         .diagnostic => |v| .{ .diagnostic = try v.clone(alloc) },
    //         .arg => |v| .{ .arg = try alloc.dupeZ(u8, v) },
    //         .expand => |v| .{ .expand = try alloc.dupe(u8, v) },
    //         .conditional_arg => |v| conditional: {
    //             var conds = try alloc.alloc(Conditional, v.conditions.len);
    //             for (v.conditions, 0..) |cond, i| conds[i] = try cond.clone(alloc);
    //             break :conditional .{ .conditional_arg = .{
    //                 .conditions = conds,
    //                 .arg = try alloc.dupe(u8, v.arg),
    //             } };
    //         },
    //     };
    // }
    //
    // PARITY: named `clone_in` (the workspace convention for ported `clone`
    // decls) — Config relies on it to clone `_replay_steps`.
    pub fn clone_in(&self) -> ReplayStep {
        match self {
            // .@"-e" => self,
            ReplayStep::DashE => ReplayStep::DashE,
            // .diagnostic => |v| .{ .diagnostic = try v.clone(alloc) },
            ReplayStep::Diagnostic(v) => ReplayStep::Diagnostic(v.clone_in()),
            // .arg => |v| .{ .arg = try alloc.dupeZ(u8, v) },
            ReplayStep::Arg(v) => ReplayStep::Arg(v.clone()),
            // .expand => |v| .{ .expand = try alloc.dupe(u8, v) },
            ReplayStep::Expand(v) => ReplayStep::Expand(v.clone()),
            // .conditional_arg => |v| { var conds = ...; ... }
            ReplayStep::ConditionalArg { conditions, arg } => {
                // var conds = try alloc.alloc(Conditional, v.conditions.len);
                // for (v.conditions, 0..) |cond, i| conds[i] = try cond.clone(alloc);
                let conds: Vec<Conditional> = conditions.iter().map(|cond| cond.clone()).collect();
                ReplayStep::ConditionalArg {
                    conditions: conds,
                    // .arg = try alloc.dupe(u8, v.arg),
                    arg: arg.clone(),
                }
            }
        }
    }
}

// ===========================================================================
// PORT NOTE: support — the std/OS glue + error/iterator types the machinery above
// uses. These add nothing the reference doesn't already do (R1): they are the
// minimal Rust counterparts of the inline std calls in the reference functions.
// ===========================================================================

/// `std.fs.max_path_bytes` — the platform max path length (4096 on Linux/macOS).
pub(super) const MAX_PATH_BYTES: usize = 4096;

/// `std.time.ns_per_s`.
pub(super) const NS_PER_S: u64 = 1_000_000_000;

/// The error set of `loadFile`/`loadFsFile`/`loadReader`/`loadRecursiveFiles`.
/// The reference threads `std.fs`/reader errors plus the parse error set; the
/// `loadOptionalFile` arm specifically distinguishes `FileNotFound`.
#[derive(Debug)]
pub enum LoadError {
    /// `error.FileNotFound` — used by `loadOptionalFile` to return `.not_found`.
    FileNotFound,
    /// Any other IO error during load.
    Io(std::io::Error),
    /// A parse error propagated from `loadIter`.
    Parse(ParseError),
    /// A default-path lookup error propagated from `file_load` (the reference's
    /// `try file_load.defaultXdgPath(alloc)` and siblings in `loadDefaultFiles`).
    FileLoad(crate::file_load::FileLoadError),
    /// `error.OutOfMemory` propagated from the theme lookup (`loadTheme`'s
    /// `try themepkg.open(..)`, reached from `finalize`'s `try
    /// self.loadTheme(theme.*)`). Its one reachable cause is
    /// `theme.Location.dir` remapping `xdg.config`'s `error.BufferTooSmall`
    /// (NEW-FINDINGS N140 (a), task S-15).
    OutOfMemory,
}

impl From<crate::theme::Error> for LoadError {
    fn from(e: crate::theme::Error) -> LoadError {
        match e {
            crate::theme::Error::OutOfMemory => LoadError::OutOfMemory,
        }
    }
}

impl From<crate::file_load::FileLoadError> for LoadError {
    fn from(e: crate::file_load::FileLoadError) -> LoadError {
        LoadError::FileLoad(e)
    }
}

impl LoadError {
    /// Map a `file_load.open` error (other than `NotAFile`, handled inline) into
    /// a `LoadError`, preserving the `FileNotFound` distinction.
    pub(super) fn from_open(err: crate::file_load::OpenFileError) -> LoadError {
        match err {
            // error.FileNotFound => loadOptionalFile returns .not_found
            crate::file_load::OpenFileError::FileNotFound => LoadError::FileNotFound,
            // `NotAFile` is handled inline in `loadFile`; the remaining variants
            // (`FileIsEmpty`, `FileOpenFailed`) are surfaced as IO errors.
            other => LoadError::Io(std::io::Error::other(format!("{other:?}"))),
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> LoadError {
        if e.kind() == std::io::ErrorKind::NotFound {
            LoadError::FileNotFound
        } else {
            LoadError::Io(e)
        }
    }
}

impl From<ParseError> for LoadError {
    fn from(e: ParseError) -> LoadError {
        LoadError::Parse(e)
    }
}

/// `std.log.info` / `std.log.warn` scoped `.config`. The reference routes these
/// through `main_ghostty.zig`'s `logFn`, whose stderr branch is gated on
/// `global.logging().stderr` (`if (!global.logging().stderr) break :stderr;`) —
/// set false by `global.zig` `init` when a CLI action is detected, so action
/// output stays clean. The port honors the same gate via the
/// `rustty_global::logging()` process-global; formatting is split from the
/// write so the gate is unit-testable without capturing stderr.
pub(super) fn format_log(kind: &str, msg: &str, logging: rustty_global::Logging) -> Option<String> {
    if !logging.stderr {
        return None;
    }
    // `level_txt ++ "(" ++ @tagName(scope) ++ "): "` — logFn's prefix for the
    // `.config` scope; `warning` is `std.log.Level.warn.asText()`.
    Some(format!("{kind}(config): {msg}"))
}

/// Test-only recorder for the three `log_*` helpers below.
///
/// The crate's diagnostics go to stderr through `eprintln!`, which a unit test
/// cannot read. `format_log` is already asserted directly (see
/// `config_log_gate_honors_logging_stderr`), but that proves only the
/// FORMATTING — not that a given reference call site still fires, at the right
/// level, with the right text. This records exactly `format_log`'s output shape
/// (`level_txt ++ "(config): " ++ msg`) at the helper's entry, BEFORE the
/// `Logging.stderr` gate, so a restored site is observable without a test
/// touching the `rustty_global` process-global that
/// `config_log_gate_reads_published_global` owns.
///
/// Thread-local and opt-in: a test calls `arm()`, exercises the site, then
/// `take()`. Untouched threads record nothing, so this cannot leak between the
/// parallel test threads libtest runs.
#[cfg(test)]
pub(crate) mod log_capture {
    use std::cell::RefCell;

    thread_local! {
        static RECORDS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    }

    /// Start recording on THIS thread, discarding anything already buffered.
    pub(crate) fn arm() {
        RECORDS.with(|r| *r.borrow_mut() = Some(Vec::new()));
    }

    /// Stop recording on this thread and return everything recorded since
    /// `arm()`, in emission order.
    pub(crate) fn take() -> Vec<String> {
        RECORDS.with(|r| r.borrow_mut().take().unwrap_or_default())
    }

    pub(crate) fn push(kind: &str, msg: &str) {
        RECORDS.with(|r| {
            if let Some(v) = r.borrow_mut().as_mut() {
                v.push(format!("{kind}(config): {msg}"));
            }
        });
    }
}

/// `std.log.err` scoped `.config` — the sibling of [`log_info`]/[`log_warn`]
/// for the reference's `log.err` sites (`config/CApi.zig`'s eleven `catch`
/// arms).
///
/// The level token is **`error`**, not `err`: Zig's `std.log.Level.err.asText()`
/// is `"error"` (`lib/std/log.zig`), and `logFn` prefixes with `level_txt`. This
/// rendering has been got wrong more than once on this plan, so
/// `config_log_err_renders_the_error_token` pins it separately.
pub(crate) fn log_err(msg: String) {
    #[cfg(test)]
    log_capture::push("error", &msg);
    if let Some(line) = format_log("error", &msg, rustty_global::logging()) {
        eprintln!("{line}");
    }
}

pub(crate) fn log_info(msg: String) {
    #[cfg(test)]
    log_capture::push("info", &msg);
    if let Some(line) = format_log("info", &msg, rustty_global::logging()) {
        eprintln!("{line}");
    }
}

pub(crate) fn log_warn(msg: String) {
    #[cfg(test)]
    log_capture::push("warning", &msg);
    if let Some(line) = format_log("warning", &msg, rustty_global::logging()) {
        eprintln!("{line}");
    }
}

/// `std.log.debug` scoped `.config` — the sibling of [`log_info`]/[`log_warn`]
/// for the reference's `log.debug` sites (`config/path.zig:192, 236`). `debug`
/// is `std.log.Level.debug.asText()`, the same `level_txt` `logFn` prefixes
/// with.
///
/// PORT NOTE: unlike info/warn this one carries a LEVEL gate as well as the
/// `stderr` gate. `main_ghostty.zig`'s `std_options.log_level` is
/// `switch (builtin.mode) { .Debug => .debug, else => .info }`, so Zig's
/// comptime `std.log` machinery erases every `log.debug` call in a release
/// build. `cfg!(debug_assertions)` is the direct analogue, and it is the same
/// default `rustty_core::logging::install_app_log_backend` ports
/// (`DECISION-logging.md` §6r CRITICAL 2 / IMPORTANT 4). Without it the port
/// would print debug traces where the reference prints nothing.
pub(crate) fn log_debug(msg: String) {
    if !cfg!(debug_assertions) {
        return;
    }
    #[cfg(test)]
    log_capture::push("debug", &msg);
    if let Some(line) = format_log("debug", &msg, rustty_global::logging()) {
        eprintln!("{line}");
    }
}

/// `std.fs.path.dirname` — the directory portion of `path`, or `None` when
/// `std.fs.path.dirname` returns null.
///
/// CORRECTION (task W-2): this was a split at the last
/// `std::path::MAIN_SEPARATOR`, which is not Zig's `dirname` (trailing and
/// repeated separators, and on Windows `/` and the `C:\` / UNC roots); it is
/// now the port of Zig's (`crate::path::dirname`).
pub(super) fn dirname(path: &[u8]) -> Option<&[u8]> {
    crate::path::dirname(path)
}

/// Build an `&OsStr` from raw path bytes without a lossy round-trip.
#[cfg(unix)]
pub(super) fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
pub(super) fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// `std.fs.Dir.realpath` result -> raw path bytes (no lossy round-trip).
#[cfg(unix)]
pub(super) fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
pub(super) fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
    p.to_string_lossy().into_owned().into_bytes()
}

/// `fn writeConfigTemplate(path: []const u8) !void` — create the template config
/// file at `path`. Reproduces the reference: makes the parent dir, creates the
/// file, and writes the embedded template with the `path` substituted.
// fn writeConfigTemplate(path: []const u8) !void {
pub(super) fn write_config_template(path: &[u8]) -> std::io::Result<()> {
    // log.info("creating template config file: path={s}", .{path});
    log_info(format!(
        "creating template config file: path={}",
        String::from_utf8_lossy(path)
    ));
    // if (std.fs.path.dirname(path)) |dir_path| {
    //     try std.fs.cwd().makePath(dir_path);
    // }
    if let Some(dir_path) = dirname(path) {
        std::fs::create_dir_all(os_str_from_bytes(dir_path))?;
    }
    // const file = try std.fs.createFileAbsolute(path, .{});
    // defer file.close();
    let mut file = std::fs::File::create(os_str_from_bytes(path))?;
    // var buf: [4096]u8 = undefined;
    // var file_writer = file.writer(&buf);
    // const writer = &file_writer.interface;
    // try writer.print(
    //     @embedFile("./config-template"),
    //     .{ .path = path },
    // );
    use std::io::Write;
    file.write_all(&render_config_template(path))?;
    Ok(())
}

/// PORTS: `@embedFile("./config-template")` rendered with `.{ .path = path }`.
/// The template is a format string with a single `{[path]s}` substitution.
fn render_config_template(path: &[u8]) -> Vec<u8> {
    // PORT NOTE: the reference embeds `src/config/config-template`; its sole
    // substitution is the config-file `path`. The template body is reproduced
    // here verbatim so the generated file is byte-faithful.
    let template = include_str!("../config-template");
    template
        .replace("{[path]s}", &String::from_utf8_lossy(path))
        .into_bytes()
}

/// This runs a heuristic to determine if we are likely running
/// Rustty in a CLI environment. We need this to change some behaviors.
/// We should keep the set of behaviors that depend on this as small
/// as possible because magic sucks, but each place is well documented.
// fn probableCliEnvironment() bool {
//
// (Windows: the `.windows => return false` arm below ends the function there,
// as the reference's comptime switch does; the rest is unreachable.)
#[cfg_attr(windows, allow(unreachable_code))]
pub(super) fn probable_cli_environment() -> bool {
    // switch (builtin.os.tag) {
    //     // Windows has its own problems, just ignore it for now since
    //     // its not a real supported target and GTK via WSL2 assuming
    //     // single instance is probably fine.
    //     .windows => return false,
    //     // On macOS, we don't want to detect `open` calls as CLI envs.
    //     // Our desktop detection on macOS is very accurate due to how
    //     // processes are launched on macOS, so if we detect we're launched
    //     // from the app bundle then we're not in a CLI environment.
    //     .macos => if (internal_os.launchedFromDesktop()) return false,
    //     else => {},
    // }
    #[cfg(target_os = "windows")]
    {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        if rustty_os::desktop::launchedFromDesktop() {
            return false;
        }
    }

    // If we have TERM_PROGRAM set to a non-empty value, we assume a graphical
    // terminal environment.
    //
    // TODO: This is not available on WASI without libc due to the memory
    // allocation requirement. This restricts this function to said platforms.
    // To be fair, the legacy getenv path had more restrictive issues (no WASI
    // period, or Windows for that matter).
    //
    // if (global.environ().containsUnemptyConstant("TERM_PROGRAM")) return true;
    //
    // PORT NOTE (`e8525c0fd`): upstream replaced `std.posix.getenv("TERM_PROGRAM")`
    // with the captured global environ, and this read MUST follow it there rather
    // than to `std::env`. Two reasons, both real: the `.c` arm of `global.init`
    // installs a caller-supplied block (`rustty_global::environ::set_environ`) and
    // libghostty embedders rely on the heuristic seeing THAT block rather than the
    // host process's; and `std::env::var` is a lossy `String` read, which the
    // byte-slice rule forbids. The WASI/libc caveat above is Zig-specific — Rust's
    // `OsString` path has no such restriction — but it is recorded because it is
    // why upstream's comment exists at all.
    if rustty_global::environ::environ().contains_unempty_constant(b"TERM_PROGRAM") {
        return true;
    }

    // CLI arguments makes things probable
    // if (global.args().vector.len > 1) return true;
    if std::env::args_os().len() > 1 {
        return true;
    }

    // Unlikely CLI environment
    // return false;
    false
}

/// Expand tilde paths to absolute paths to the user's home directory.
/// If expansion fails, an error is logged and the original path is returned.
// fn expandHome(path: []const u8, buf: []u8) []const u8 {
//
// PORT NOTE (`37174c73b`): file-scope helper in the reference (Config.zig, right
// before `loadTheme`); shared by `WorkingDirectory.finalize` and `Theme.finalize`
// (both in `value_types.rs`), hence `pub(crate)`.
pub(crate) fn expand_home<'a>(path: &'a [u8], buf: &'a mut [u8]) -> &'a [u8] {
    // if (!std.mem.startsWith(u8, path, "~/"))
    //     return path;
    if !path.starts_with(b"~/") {
        return path;
    }

    // WS14 T6 (wasm32 arm): a browser has no home directory to expand `~/`
    // against, so wasm returns the path unchanged, exactly as the error arms
    // below do. The reference never analyses this on wasm (Zig analyses
    // `config.zig.Wasm` lazily, so it never forces `internal_os.expandHome`);
    // Rust compiles whole crates, so the arm has to exist.
    #[cfg(target_family = "wasm")]
    {
        path
    }
    #[cfg(not(target_family = "wasm"))]
    {
        // var environ_map = global.environMap() catch |err| {
        //     log.warn("failed to get environment map for path \"{s}\": {}", .{ path, err });
        //     return path;
        // };
        // defer environ_map.deinit();
        //
        // PORT NOTE: `rustty_global::environ_map()` is infallible (a snapshot of
        // the captured environ), so the reference's `catch` warn-arm has no Rust
        // analogue.
        //
        // N16 (unclaimed-and-unrestorable): that makes `Config.zig:4563`'s
        // `log.warn` unrestorable rather than merely dropped — there is no
        // failure arm to fire it from. No LEDGER row cites it; it is the third
        // of the three sites behind `src/config/Config.zig`'s residual
        // G-LOGFILE allowance of 3.
        let environ_map = rustty_global::environ_map();

        // return internal_os.expandHome(
        //     global.io(),
        //     &environ_map,
        //     path,
        //     buf,
        // ) catch |err| {
        //     log.warn("failed to expand home directory in path \"{s}\": {}", .{ path, err });
        //     return path;
        // };
        match rustty_os::homedir::expandHome(&environ_map, path, buf) {
            Ok(expanded) => expanded,
            Err(err) => {
                log_warn(format!(
                    "failed to expand home directory in path \"{}\": {:?}",
                    String::from_utf8_lossy(path),
                    err
                ));
                path
            }
        }
    }
}

/// This is used to "replay" the configuration. See loadTheme for details.
///
/// PORTS: `const Replay = struct { ... }` — the `Step` element type lives above
/// as `ReplayStep`; this is the `Iterator` + `iterator` constructor.
// const Replay = struct { const Step = ...; const Iterator = struct {...}; fn iterator(...) {...} };
//
// PORT NOTE (aliasing): the reference `Replay.Iterator` holds `config: *Config`
// and mutates it inside `next` while `cli.args.parse` drives it with the SAME
// `*Config`. In Rust the iterator cannot capture `&mut Config` (it would alias
// the `&mut Config` `parse_self` holds), so `ReplayIterator` is a
// `SelfDrivenIterator<Config>`: its `next_self` receives `&mut Config` as a
// parameter, exactly recreating the reference's interleaving without aliasing.
pub struct ReplayIterator {
    // slice: []const Replay.Step,
    slice: Vec<ReplayStep>,
    // idx: usize = 0,
    idx: usize,
}

impl ReplayIterator {
    /// Construct a Replay iterator from a slice of replay elements.
    /// This can be used with args.parse and handles intermediate
    /// steps such as expanding relative paths.
    // fn iterator(slice: []const Replay.Step, dst: *Config) Iterator {
    //     return .{ .slice = slice, .config = dst };
    // }
    //
    // PORT NOTE: `dst` (the `*Config`) is no longer captured; it is lent to
    // `next_self` per the aliasing note above.
    pub fn new(slice: Vec<ReplayStep>) -> ReplayIterator {
        ReplayIterator { slice, idx: 0 }
    }
}

impl rustty_args::args::SelfDrivenIterator<Config> for ReplayIterator {
    // pub fn next(self: *Self) ?[]const u8 {
    fn next_self(&mut self, config: &mut Config) -> Option<Vec<u8>> {
        // while (true) {
        loop {
            // if (self.idx >= self.slice.len) return null;
            if self.idx >= self.slice.len() {
                return None;
            }
            // defer self.idx += 1;
            let cur = self.idx;
            self.idx += 1;
            // switch (self.slice[self.idx]) {
            match self.slice[cur].clone() {
                // .expand => |base| self.config.expandPaths(base) catch |err| {
                //     // This shouldn't happen because to reach this step
                //     // means that it succeeded before. Its possible since
                //     // expanding paths is a side effect process that the
                //     // world state changed and we can't expand anymore.
                //     // In that really unfortunate case, we log a warning.
                //     log.warn("error expanding paths err={}", .{err});
                // },
                ReplayStep::Expand(base) => {
                    if let Err(err) = config.expand_paths(&base) {
                        log_warn(format!("error expanding paths err={err:?}"));
                    }
                }

                // .diagnostic => |diag| diag: {
                ReplayStep::Diagnostic(diag) => {
                    // Best effort to clone and append the diagnostic.
                    // If it fails we log a warning and continue.
                    // const arena_alloc = self.config._arena.?.allocator();
                    // const cloned = diag.clone(arena_alloc) catch |err| {
                    //     log.warn("error cloning diagnostic err={}", .{err});
                    //     break :diag;
                    // };
                    //
                    // PORT NOTE (N16, unclaimed-and-unrestorable): the
                    // reference's two `catch |err|` arms here log and
                    // `break :diag`. Both operations are INFALLIBLE in the
                    // port — `Diagnostic::clone_in` returns `Self` (no
                    // allocator, no `Allocator.Error`) and
                    // `DiagnosticList::append` returns `()` — so neither
                    // `log.warn` has a failure arm to fire from. Restoring
                    // them would put a diagnostic on a success path, which
                    // this plan has already shipped once. No LEDGER row cites
                    // `Config.zig:5396`/`:5400`; they are two of the three
                    // sites behind `src/config/Config.zig`'s residual
                    // G-LOGFILE allowance of 3.
                    let cloned = diag.clone_in();
                    // self.config._diagnostics.append(arena_alloc, cloned) catch |err| {
                    //     log.warn("error appending diagnostic err={}", .{err});
                    //     break :diag;
                    // };
                    config._diagnostics.append(cloned);
                }

                // .conditional_arg => |v| conditional: {
                ReplayStep::ConditionalArg { conditions, arg } => {
                    // All conditions must match.
                    // for (v.conditions) |cond| {
                    //     if (!self.config._conditional_state.match(cond)) {
                    //         break :conditional;
                    //     }
                    // }
                    // return v.arg;
                    let mut all_match = true;
                    for cond in &conditions {
                        if !config._conditional_state.is_match(cond) {
                            all_match = false;
                            break;
                        }
                    }
                    if all_match {
                        return Some(arg);
                    }
                }

                // .arg => |arg| return arg,
                ReplayStep::Arg(arg) => return Some(arg),
                // .@"-e" => return "-e",
                ReplayStep::DashE => return Some(b"-e".to_vec()),
            }
        }
    }

    // pub fn location(self: *const Self, _: Allocator) error{}!?diags.Location {
    //     return .{ .cli = self.index };
    // }
    //
    // PORT NOTE: the reference's `Replay.Iterator` has no `location` decl; it
    // relies on `cli.Location.fromIter` returning `.none` for iterators without
    // one. We return `None`, which `parse_self` maps to `Location::None`.
    fn location_self(&self) -> Option<rustty_args::diagnostics::Location> {
        None
    }
}

/// PORTS: `pub const ChangeIterator` (Config.zig:5056) — iterates `Key`s whose
/// value differs between `old` and `new`.
///
/// This yields a key for every changed field between old and new.
// pub const ChangeIterator = struct { old, new, i = 0, pub fn next(...) ?Key };
pub struct ChangeIterator<'a> {
    pub old: &'a Config,
    pub new: &'a Config,
    pub i: usize,
}

impl<'a> ChangeIterator<'a> {
    // pub fn next(self: *ChangeIterator) ?Key
    //
    // PORT NOTE: the reference exposes a `next` method (not a Zig std iterator);
    // we keep the same inherent-method name for call-site parity rather than
    // implementing `std::iter::Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Key> {
        // const fields = comptime std.meta.fields(Key);
        // while (self.i < fields.len) {
        //     switch (self.i) {
        //         inline 0...(fields.len - 1) => |i| {
        //             const field = fields[i];
        //             const key = @field(Key, field.name);
        //             self.i += 1;
        //             if (self.old.changed(self.new, key)) return key;
        //         },
        //         else => unreachable,
        //     }
        // }
        // return null;
        //
        // PORT NOTE: Zig's `std.meta.fields(Key)` is the comptime `Key`-variant
        // sequence. Rust has no enum reflection, so we iterate the `CONFIG_FIELDS`
        // table's keys, which are the `Key` variants in the reference's struct
        // declaration order (the same order `std.meta.fields(Key)` yields, since
        // `Key` is derived from the Config fields in order). `self.i` indexes that
        // sequence exactly as the reference's `self.i` indexes `fields`.
        while self.i < CONFIG_FIELDS.len() {
            let (_name, key) = CONFIG_FIELDS[self.i];
            self.i += 1;
            if self.old.changed(self.new, key) {
                return Some(key);
            }
        }

        None
    }
}

/// `(comptime !builtin.target.cpu.arch.isWasm()) and (comptime
/// !builtin.is_test)` — the guard around `finalize`'s default command /
/// working-directory lookup (`Config.zig:4737-4739`). Called with the
/// target's `cfg!` values; a function so the decision itself is testable on a
/// host that is not wasm.
pub(super) const fn finalize_looks_up_defaults(is_wasm: bool, is_test: bool) -> bool {
    !is_wasm && !is_test
}

/// The Windows arm's `wd == .home` step in `finalize` (`Config.zig:4776-4785`):
/// `if (try internal_os.home(...)) |home| { wd = .{ .path = home }; } else {
/// wd = .inherit; }`. The `try` returns `home`'s error from `finalize`.
#[cfg(any(target_os = "windows", test))]
pub(super) fn windows_home_working_directory(
    home: Result<Option<&[u8]>, rustty_os::homedir::Error>,
) -> Result<WorkingDirectory, LoadError> {
    match home {
        Ok(Some(home)) => Ok(WorkingDirectory::Path(home.to_vec())),
        Ok(None) => Ok(WorkingDirectory::Inherit),
        Err(e) => Err(LoadError::Io(std::io::Error::other(format!("{e:?}")))),
    }
}
