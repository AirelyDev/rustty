//! rustty-cli::list_colors
//!
//! PORTS: ghostty src/cli/list_colors.zig (0t) — the `+list-colors` action.
//!
//! PORT STATUS: C5 (CLI_BLUEPRINT.md §1/§7). The `--plain`/non-tty path — the
//! ONLY path the KAT and any redirected pipe hit — lands LIVE here. The
//! interactive `prettyPrint` (vaxis) is a documented vaxis-deferred placeholder
//! leaf (DEFERRALS.md "cli C5"; GPU/GTK/xev precedent), its reference body +
//! comments preserved verbatim. 0 tests; comment-fidelity (scoped-G3b) applies.

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const Action = @import("ghostty.zig").Action;
use crate::ghostty::{Action, Error};
// const args = @import("args.zig");
use rustty_args::args;
// const x11_color = @import("../terminal/main.zig").x11_color;
//   PORT NOTE: `terminal` -> `rustty-vt`; the X11 color map lives at
//   `rustty_vt::x11_color` (its `map.keys()` / `map.get()` mirror the reference
//   `StaticStringMap` surface).
use rustty_vt::x11_color;
// const vaxis = @import("vaxis");
//   VAXIS-DEFERRED: the interactive `prettyPrint` render leaf (see below).
// const tui = @import("tui.zig");
use crate::tui;

use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// If `true`, print without formatting even if printing to a tty
    // plain: bool = false,
    pub plain: bool,
}

impl Options {
    // pub fn deinit(self: Options) void {
    //     _ = self;
    // }
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables "-h" and "--help" to work.
    // pub fn help(self: Options) !void {
    //     _ = self;
    //     return Action.help_error;
    // }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the single non-`_` field is `plain`.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // plain: bool
            b"plain" => {
                self.plain = args::parse_field_bool(value)?;
                Ok(())
            }
            // return error.InvalidField;
            _ => Err(args::Error::InvalidField),
        }
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help; // (documents the reference `Action.help_error` source)
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `list-colors` command is used to list all the named RGB colors in
/// rustty.
///
/// Flags:
///
///   * `--plain`: will disable formatting and make the output more
///     friendly for Unix tooling. This is default when not printing to a tty.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try args.argsIterator(alloc);
        let mut iter = args::args_iterator()?;
        // defer iter.deinit();
        // try args.parse(Options, alloc, &opts, &iter);
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // var keys: std.ArrayList([]const u8) = .empty;
    // defer keys.deinit(alloc);
    // for (x11_color.map.keys()) |key| try keys.append(alloc, key);
    let mut keys: Vec<&str> = Vec::new();
    for key in x11_color::map.keys() {
        keys.push(key.as_str());
    }

    // std.mem.sortUnstable([]const u8, keys.items, {}, struct {
    //     fn lessThan(_: void, lhs: []const u8, rhs: []const u8) bool {
    //         return std.ascii.orderIgnoreCase(lhs, rhs) == .lt;
    //     }
    // }.lessThan);
    keys.sort_unstable_by(|lhs, rhs| order_ignore_case(lhs.as_bytes(), rhs.as_bytes()));

    // Despite being under the posix namespace, this also works on Windows as of zig 0.13.0
    // var stdout: std.fs.File = .stdout();
    // if (tui.can_pretty_print and !opts.plain and std.posix.isatty(stdout.handle)) {
    use std::io::IsTerminal;
    if tui::CAN_PRETTY_PRINT && !opts.plain && std::io::stdout().is_terminal() {
        // var arena = std.heap.ArenaAllocator.init(alloc);
        // defer arena.deinit();
        // return prettyPrint(arena.allocator(), keys.items);
        //
        // VAXIS-DEFERRED (DEFERRALS.md "cli C5"): the interactive render path.
        return pretty_print(&keys);
    } else {
        // var buffer: [4096]u8 = undefined;
        // var stdout_writer = stdout.writer(&buffer);
        // const writer = &stdout_writer.interface;
        //   PORT NOTE: the reference File.Writer buffers into `buffer` but never
        //   flushes on this path (it falls straight to `return 0`); the Rust
        //   `BufWriter` owns its buffer and is flushed explicitly below (and again
        //   on drop), so the output is observably delivered.
        let mut stdout_writer = std::io::BufWriter::new(std::io::stdout().lock());
        let writer = &mut stdout_writer;
        // for (keys.items) |name| {
        for name in &keys {
            // const rgb = x11_color.map.get(name).?;
            let rgb = x11_color::map.get(name).unwrap();
            // try writer.print("{s} = #{x:0>2}{x:0>2}{x:0>2}\n", .{
            //     name,
            //     rgb.r,
            //     rgb.g,
            //     rgb.b,
            // });
            //   PORT NOTE: the `name` bytes are written verbatim then the `" =
            //   #RRGGBB\n"` tail is formatted (`writeln!` supplies the trailing
            //   newline the reference `\n` in the format string does).
            writer.write_all(name.as_bytes())?;
            writeln!(writer, " = #{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)?;
        }
        writer.flush()?;
    }

    opts.deinit();

    // return 0;
    Ok(0)
}

// ===========================================================================
// VAXIS-DEFERRED placeholder leaf (DEFERRALS.md "cli C5") — the interactive
// pretty-printer. `vaxis` is an unported Zig-source dependency (same class as
// libxev/z2d); the reference body + comments are preserved verbatim below and the
// entry point is an `unimplemented!()` dependency marker. 0 tests; un-deferred
// when the `vaxis` port lands. The plain path above is fully functional today.
// ===========================================================================

// fn prettyPrint(alloc: Allocator, keys: [][]const u8) !u8 {
#[allow(dead_code)]
fn pretty_print(keys: &[&str]) -> Result<u8, Error> {
    let _ = keys;
    unimplemented!("cli/list_colors.zig prettyPrint — vaxis-deferred leaf (DEFERRALS.md cli C5)")
    // Set up vaxis
    // var buf: [1024]u8 = undefined;
    // var tty = try vaxis.Tty.init(&buf);
    // defer tty.deinit();
    // var vx = try vaxis.init(alloc, .{});
    // defer vx.deinit(alloc, tty.writer());
    //
    // // We know we are ghostty, so let's enable mode 2027. Vaxis normally does this but you need an
    // // event loop to auto-enable it.
    // vx.caps.unicode = .unicode;
    // try tty.writer().writeAll(vaxis.ctlseqs.unicode_set);
    // defer tty.writer().writeAll(vaxis.ctlseqs.unicode_reset) catch {};
    //
    // const winsize: vaxis.Winsize = switch (builtin.os.tag) {
    //     // We use some default, it doesn't really matter for what
    //     // we're doing because we don't do any wrapping.
    //     .windows => .{
    //         .rows = 24,
    //         .cols = 120,
    //         .x_pixel = 1024,
    //         .y_pixel = 768,
    //     },
    //
    //     else => try vaxis.Tty.getWinsize(tty.fd),
    // };
    // try vx.resize(alloc, tty.writer(), winsize);
    //
    // const win = vx.window();
    //
    // var max_name_len: usize = 0;
    // for (keys) |name| {
    //     if (name.len > max_name_len) max_name_len = name.len;
    // }
    //
    // // max name length plus " = #RRGGBB XX" plus "  " gutter between columns
    // const column_size = max_name_len + 15;
    // // add two to take into account lack of gutter after last column
    // const columns: usize = @divFloor(win.width + 2, column_size);
    //
    // var i: usize = 0;
    // const step = @divFloor(keys.len, columns) + 1;
    // while (i < step) : (i += 1) {
    //     win.clear();
    //
    //     var result: vaxis.Window.PrintResult = .{ .col = 0, .row = 0, .overflow = false };
    //
    //     for (0..columns) |j| {
    //         const k = i + (step * j);
    //         if (k >= keys.len) continue;
    //
    //         const name = keys[k];
    //         const rgb = x11_color.map.get(name).?;
    //
    //         const style1: vaxis.Style = .{
    //             .fg = .{
    //                 .rgb = .{ rgb.r, rgb.g, rgb.b },
    //             },
    //         };
    //         const style2: vaxis.Style = .{
    //             .fg = .{
    //                 .rgb = .{ rgb.r, rgb.g, rgb.b },
    //             },
    //             .bg = .{
    //                 .rgb = .{ rgb.r, rgb.g, rgb.b },
    //             },
    //         };
    //
    //         // name of the color
    //         result = win.printSegment(
    //             .{ .text = name },
    //             .{ .col_offset = result.col },
    //         );
    //         // push the color data to the end of the column
    //         for (0..max_name_len - name.len) |_| {
    //             result = win.printSegment(
    //                 .{ .text = " " },
    //                 .{ .col_offset = result.col },
    //             );
    //         }
    //         result = win.printSegment(
    //             .{ .text = " = " },
    //             .{ .col_offset = result.col },
    //         );
    //         // rgb triple
    //         result = win.printSegment(.{
    //             .text = try std.fmt.allocPrint(
    //                 alloc,
    //                 "#{x:0>2}{x:0>2}{x:0>2}",
    //                 .{
    //                     rgb.r, rgb.g, rgb.b,
    //                 },
    //             ),
    //             .style = style1,
    //         }, .{ .col_offset = result.col });
    //         result = win.printSegment(
    //             .{ .text = " " },
    //             .{ .col_offset = result.col },
    //         );
    //         // colored block
    //         result = win.printSegment(
    //             .{
    //                 .text = "  ",
    //                 .style = style2,
    //             },
    //             .{ .col_offset = result.col },
    //         );
    //         // add the gutter if needed
    //         if (j + 1 < columns) {
    //             result = win.printSegment(
    //                 .{
    //                     .text = "  ",
    //                 },
    //                 .{ .col_offset = result.col },
    //             );
    //         }
    //     }
    //
    //     // clear the rest of the line
    //     while (result.col != 0) {
    //         result = win.printSegment(
    //             .{
    //                 .text = " ",
    //             },
    //             .{ .col_offset = result.col },
    //         );
    //     }
    //
    //     // output the data
    //     try vx.prettyPrint(tty.writer());
    // }
    //
    // return 0;
}

// PORT NOTE: `std.ascii.orderIgnoreCase` — compares two slices treating ASCII
// letters case-insensitively; on a common prefix, the shorter slice orders first.
fn order_ignore_case(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i < n {
        let la = a[i].to_ascii_lowercase();
        let lb = b[i].to_ascii_lowercase();
        match la.cmp(&lb) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        i += 1;
    }
    a.len().cmp(&b.len())
}
