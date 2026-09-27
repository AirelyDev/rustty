//! rustty-cli::crash_report
//!
//! PORTS: ghostty src/cli/crash_report.zig (0t) — the `+crash-report` action:
//! list existing crash reports (viewing/sending are unimplemented in the
//! reference too). Backed by rustty-crash (`defaultDir`, `Report` iterator) and
//! rustty-config (`Duration` for the relative-age formatting). 0 tests.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const Config = @import("../config.zig").Config;
//   R3: `Config.Duration` -> the value-types `Duration` (nanosecond duration +
//   its `{f}` unit formatter).
use rustty_config::value_types::Duration;
// const crash = @import("../crash/main.zig");
use rustty_crash as crash;

use std::io::Write;

// std.time.ns_per_s — the seconds rounding factor for the relative age.
const NS_PER_S: u64 = 1_000_000_000;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {}

impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables "-h" and "--help" to work.
    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // The reference `Options` has no non-`_` fields, so `parseIntoField` always
    // falls through to `return error.InvalidField`.
    fn parse_into_field(
        &mut self,
        _key: &[u8],
        _value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        // return error.InvalidField;
        Err(args::Error::InvalidField)
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `crash-report` command is used to inspect and send crash reports.
///
/// When executed without any arguments, this will list existing crash reports.
///
/// This command currently only supports listing crash reports. Viewing
/// and sending crash reports is unimplemented and will be added in the future.
// pub fn run(alloc_gpa: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // Use an arena for the whole command to avoid manual memory management.
    // var arena = std.heap.ArenaAllocator.init(alloc_gpa);
    // defer arena.deinit();
    // const alloc = arena.allocator();
    //   PORT NOTE: the arena is dropped tree-wide; each collected value owns its
    //   bytes (Report.name is already owned by the iterator).

    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try args.argsIterator(alloc_gpa);
        // defer iter.deinit();
        // try args.parse(Options, alloc_gpa, &opts, &iter);
        let mut iter = args::args_iterator()?;
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // var buffer: [1024]u8 = undefined;
    // var stdout_file: std.fs.File = .stdout();
    // var stdout_writer = stdout_file.writer(&buffer);
    // const stdout = &stdout_writer.interface;
    //   PORT NOTE: the reference keeps `stdout_file` separate from the buffered
    //   `stdout` writer to `posix.isatty(stdout_file.handle)`; Rust queries
    //   `is_terminal()` on the stdout handle before wrapping it in the BufWriter.
    //   (Zig's stdout File, task W-4 N365.)
    let stdout_file = crate::io_file::stdout();
    let stdout_is_tty = stdout_file.is_terminal();
    let mut stdout = std::io::BufWriter::with_capacity(1024, stdout_file);

    // var environ_map = try global.environMap();
    // defer environ_map.deinit();
    // (PORT NOTE, P4-E33 `15c50c1db`: the CLI — a place that "would more
    // correctly interface with global state" — builds the environment snapshot
    // and passes it down; crash/dir.zig no longer reads global state itself.
    // `environ_map()` is infallible in Rust; Drop frees the map.)
    let environ_map = rustty_global::environ_map();
    // const result = runInner(global.io(), alloc, &environ_map, &stdout_file, stdout);
    let result = run_inner(&environ_map, stdout_is_tty, &mut stdout);
    // stdout.flush() catch {};
    let _ = stdout.flush();

    opts.deinit();

    // return result;
    result
}

// fn runInner(
//     io: std.Io,
//     alloc: Allocator,
//     environ_map: *const std.process.Environ.Map,
//     stdout_file: *std.Io.File,
//     stdout: *std.Io.Writer,
// ) !u8 {
// (PORT NOTE: the `io: std.Io` parameter is N-A — Rust std does I/O without a
// provider handle; see rustty-crash dir.rs.)
fn run_inner<W: Write>(
    environ_map: &rustty_global::EnvMap,
    stdout_is_tty: bool,
    stdout: &mut W,
) -> Result<u8, Error> {
    // const crash_dir = try crash.defaultDir(io, alloc, environ_map);
    let crash_dir = crash::defaultDir(environ_map)?;
    // var reports: std.ArrayList(crash.Report) = .empty;
    // errdefer reports.deinit(alloc);
    let mut reports: Vec<crash::Report> = Vec::new();

    // var it = try crash_dir.iterator(io);
    let mut it = crash_dir.iterator()?;
    // while (try it.next(io)) |report| try reports.append(alloc, .{
    //     .name = try alloc.dupe(u8, report.name),
    //     .mtime = report.mtime,
    // });
    //   PORT NOTE: `it.next()` already yields an owned `Report` (its `name` bytes
    //   are owned, not a borrow into the iterator), so the reference's
    //   `alloc.dupe(report.name)` copy is implicit — push the owned report.
    while let Some(report) = it.next()? {
        reports.push(crash::Report {
            name: report.name,
            mtime: report.mtime,
        });
    }

    // If we have no reports, then we're done. If we have a tty then we
    // print a message, otherwise we do nothing.
    // if (reports.items.len == 0) {
    if reports.is_empty() {
        // if (std.posix.isatty(stdout_file.handle)) {
        if stdout_is_tty {
            // try stdout.writeAll("No crash reports! 👻\n");
            //   PORT NOTE (R2 / CLI_BLUEPRINT.md Risk 3): the 👻 is a decorative
            //   Ghostty pun (ghost == Ghostty), not a product-name token or path;
            //   rustty-identity has no mapping for it, so the line is preserved
            //   verbatim pending a brand decision by the identity gate.
            stdout.write_all("No crash reports! 👻\n".as_bytes())?;
        }
        // return 0;
        return Ok(0);
    }

    // std.mem.sort(crash.Report, reports.items, {}, lt);
    //   PORT NOTE: `lt` orders by DESCENDING mtime (`lhs.mtime > rhs.mtime`). The
    //   reference `std.mem.sort` is unstable; `sort_by` is stable, which only
    //   affects the relative order of equal-mtime reports (cosmetic).
    reports.sort_by(lt_cmp);

    // for (reports.items) |report| {
    for report in &reports {
        // var buf: [128]u8 = undefined;
        // const now = std.time.nanoTimestamp();
        let now = nano_timestamp();
        // const diff = now - report.mtime;
        let diff: i128 = now - report.mtime;
        // const since = if (diff <= 0) "now" else s: {
        //     const d = Config.Duration{ .duration = @intCast(diff) };
        //     break :s try std.fmt.bufPrint(&buf, "{f} ago", .{d.round(std.time.ns_per_s)});
        // };
        let since: Vec<u8> = if diff <= 0 {
            b"now".to_vec()
        } else {
            // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD99.
            // `@intCast(diff)` into the `u64` nanoseconds of a `Duration` traps
            // in ReleaseSafe when a report's mtime is more than 2^64 ns (about
            // 584 years) before now, and wraps in ReleaseFast. The mtime comes
            // from the filesystem, and Linux tmpfs and btrfs store an mtime in
            // the year 1400 (measured), so `+crash-report` can trap on a file
            // in the crash directory. This port used to truncate with `as u64`
            // (a silently wrong age, inventory P3). A future mtime already takes
            // the reference's own "now" arm above; an age too large for a
            // `Duration` saturates to the largest one it holds, a lower bound on
            // the true age, instead of trapping or wrapping. The reference has
            // no error path here and no message to reuse.
            let d = Duration {
                duration: u64::try_from(diff).unwrap_or(u64::MAX),
            };
            let mut s = Vec::new();
            d.round(NS_PER_S).format(&mut s);
            s.extend_from_slice(b" ago");
            s
        };
        // try stdout.print("{s} ({s})\n", .{ report.name, since });
        stdout.write_all(&report.name)?;
        stdout.write_all(b" (")?;
        stdout.write_all(&since)?;
        stdout.write_all(b")\n")?;
    }

    // return 0;
    Ok(0)
}

// fn lt(_: void, lhs: crash.Report, rhs: crash.Report) bool {
//     return lhs.mtime > rhs.mtime;
// }
//   PORT NOTE: `std.mem.sort`'s `lt(a, b) == true` means "a before b"; the
//   `>` comparator yields descending mtime. Mapped to `Ordering` for `sort_by`.
fn lt_cmp(lhs: &crash::Report, rhs: &crash::Report) -> std::cmp::Ordering {
    // return lhs.mtime > rhs.mtime;
    rhs.mtime.cmp(&lhs.mtime)
}

// std.time.nanoTimestamp() — wall-clock nanoseconds since the Unix epoch (i128,
// negative for pre-epoch times).
fn nano_timestamp() -> i128 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

    /// The label `run_inner` prints for an age, computed the way it computes it.
    fn label(duration: u64) -> String {
        let mut s = Vec::new();
        Duration { duration }.round(NS_PER_S).format(&mut s);
        String::from_utf8(s).unwrap()
    }

    /// UD99 (R13): a crash report whose mtime lies further in the past than a
    /// `Duration` can hold. The reference's `@intCast(diff)` traps there; the port
    /// used to truncate with `as u64` and print a wrong age (inventory P3). Linux
    /// tmpfs and btrfs store an mtime in the year 1400 (measured), so this drives
    /// the real `run_inner` over a real crash directory. macOS APFS clamps any
    /// earlier mtime to 1677, whose age still fits, so there the ancient report
    /// only checks the ordinary arm; the saturating arm is asserted on Linux.
    #[test]
    fn a_report_too_old_for_a_duration_saturates_instead_of_trapping() {
        let root = std::env::temp_dir().join(format!(
            "rustty-r13d-crash-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dir = root.join(format!("{}/crash", rustty_identity::APP_NAME));
        std::fs::create_dir_all(&dir).unwrap();

        // The year 1400 (the probe value), a report written a minute ago, and one
        // dated in the future (2200), which takes the reference's own "now" arm.
        //
        // Windows (task W-2): NTFS stores no time before 1601 (a FILETIME counts
        // from 1601-01-01, and `SetFileTime` rejects the year 1400 with
        // ERROR_INVALID_PARAMETER, measured), so the ancient report there is
        // dated 1601-01-02 (a zero FILETIME means "leave unchanged"); its age
        // fits a `Duration`, and like APFS it checks the ordinary arm.
        #[cfg(not(windows))]
        let ancient = UNIX_EPOCH - StdDuration::from_secs(17_987_443_200);
        #[cfg(windows)]
        let ancient = UNIX_EPOCH - StdDuration::from_secs(11_644_473_600 - 86_400);
        let recent = SystemTime::now() - StdDuration::from_secs(60);
        let future = UNIX_EPOCH + StdDuration::from_secs(7_258_118_400);
        for (name, mtime) in [("ancient", ancient), ("recent", recent), ("future", future)] {
            let f = std::fs::File::create(dir.join(name)).unwrap();
            f.set_modified(mtime).unwrap();
        }

        let mut env = rustty_global::EnvMap::new();
        env.insert(
            b"XDG_STATE_HOME".to_vec(),
            root.as_os_str().as_encoded_bytes().to_vec(),
        );
        let mut out: Vec<u8> = Vec::new();
        let rc = run_inner(&env, false, &mut out).expect("run_inner");
        let now = nano_timestamp();
        assert_eq!(rc, 0);
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "three reports listed: {text:?}");

        // Descending mtime: the future report first, then the recent one.
        assert_eq!(lines[0], "future (now)");
        assert!(
            lines[1].starts_with("recent (") && lines[1].ends_with(" ago)"),
            "{text:?}"
        );

        // The ancient report: its age as the filesystem stored it.
        let stored = std::fs::metadata(dir.join("ancient"))
            .unwrap()
            .modified()
            .unwrap();
        let stored_ns: i128 = match stored.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as i128,
            Err(e) => -(e.duration().as_nanos() as i128),
        };
        let age = now - stored_ns;
        if cfg!(target_os = "linux") {
            assert!(
                age > i128::from(u64::MAX),
                "precondition: the filesystem kept an mtime older than a Duration \
                 holds (age {age} ns); without it the saturating arm is not reached"
            );
        }
        let expected = match u64::try_from(age) {
            Ok(ns) => label(ns),
            Err(_) => label(u64::MAX),
        };
        // `now` was read after `run_inner`, so an ordinary age may differ by the
        // rounding second; the saturated one is exact.
        let got = lines[2]
            .strip_prefix("ancient (")
            .and_then(|s| s.strip_suffix(" ago)"))
            .unwrap_or_else(|| panic!("the ancient report's line: {text:?}"));
        if age > i128::from(u64::MAX) {
            assert_eq!(got, label(u64::MAX), "the age saturates, it does not wrap");
            assert!(got.starts_with("584y"), "{got:?}");
        } else {
            assert!(
                got.starts_with(&expected[..expected.len().min(8)]),
                "{got:?} vs {expected:?}"
            );
        }

        std::fs::remove_dir_all(&root).unwrap();
    }
}
