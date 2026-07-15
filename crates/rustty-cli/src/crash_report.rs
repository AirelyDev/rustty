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

use std::io::{IsTerminal, Write};

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
    let stdout_file = std::io::stdout();
    let stdout_is_tty = stdout_file.is_terminal();
    let mut stdout = std::io::BufWriter::new(stdout_file.lock());

    // const result = runInner(alloc, &stdout_file, stdout);
    let result = run_inner(stdout_is_tty, &mut stdout);
    // stdout.flush() catch {};
    let _ = stdout.flush();

    opts.deinit();

    // return result;
    result
}

// fn runInner(alloc: Allocator, stdout_file: *std.fs.File, stdout: *std.Io.Writer) !u8 {
fn run_inner<W: Write>(stdout_is_tty: bool, stdout: &mut W) -> Result<u8, Error> {
    // const crash_dir = try crash.defaultDir(alloc);
    let crash_dir = crash::defaultDir()?;
    // var reports: std.ArrayList(crash.Report) = .empty;
    // errdefer reports.deinit(alloc);
    let mut reports: Vec<crash::Report> = Vec::new();

    // var it = try crash_dir.iterator();
    let mut it = crash_dir.iterator()?;
    // while (try it.next()) |report| try reports.append(alloc, .{
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
            let d = Duration {
                duration: diff as u64,
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
