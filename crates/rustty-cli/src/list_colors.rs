//! rustty-cli::list_colors
//!
//! PORTS: ghostty src/cli/list_colors.zig (0t) — the `+list-colors` action.
//!
//! PORT STATUS: C5 (CLI_BLUEPRINT.md §1/§7). Both paths land LIVE. The
//! `--plain`/non-tty path — the path the KAT and any redirected pipe hit —
//! streams the x11_color enumeration. The interactive `prettyPrint` render path
//! is now filled over the landed `rustty-vaxis` core (WIRE1; DEFERRALS.md "cli C5"
//! partial): the render is factored into a `render_colors` seam (pure over a
//! `Write` sink → smoke-tested + Miri-able) behind the `pretty_print` device
//! wrapper (opens a real tty → `#[ignore]`-on-device). comment-fidelity
//! (scoped-G3b) applies.

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const args = @import("args.zig");
use rustty_args::args;
// const x11_color = @import("../terminal/main.zig").x11_color;
//   PORT NOTE: `terminal` -> `rustty-vt`; the X11 color map lives at
//   `rustty_vt::x11_color` (its `map.keys()` / `map.get()` mirror the reference
//   `StaticStringMap` surface).
use rustty_vt::x11_color;
// const vaxis = @import("vaxis");
//   The interactive `prettyPrint` render leaf (WIRE1) drives the vaxis TUI core.
use rustty_vaxis::cell::{Color, Style};
use rustty_vaxis::gwidth::Method;
use rustty_vaxis::window::PrintResult;
use rustty_vaxis::{ctlseqs, Winsize};
// const tui = @import("tui.zig");
use crate::pretty_arena::{po, seg, styled, winsize_for, Arena};
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
        //   WIRE1: the interactive render path, filled over rustty-vaxis (below).
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
// The interactive pretty-printer (WIRE1). Filled over the landed `rustty-vaxis`
// core (DEFERRALS.md "cli C5" partial). The device lifecycle lives in the outer
// `pretty_print`; the render body (from `vx.resize` on) is factored into the
// `render_colors` seam so a `Vec<u8>` smoke test can exercise it without a tty.
// ===========================================================================

// fn prettyPrint(alloc: Allocator, keys: [][]const u8) !u8 {
//   The device wrapper: sets up the tty/vaxis lifecycle, then delegates the
//   drawing to `render_colors` (the testable seam).
fn pretty_print(keys: &[&str]) -> Result<u8, Error> {
    // var arena = std.heap.ArenaAllocator.init(alloc);  (see list_colors.rs run())
    // defer arena.deinit();
    //   Declared BEFORE `vx` so it outlives the screen that borrows its duped
    //   bytes, mirroring the reference's outer arena / `defer arena.deinit()`.
    let arena = Arena::init();

    // Set up vaxis
    // var buf: [1024]u8 = undefined;
    // var tty = try vaxis.Tty.init(&buf);
    let mut tty = rustty_vaxis::Tty::init(vec![0u8; 1024])?;
    // defer tty.deinit();   (fired LIFO-last below)
    // var vx = try vaxis.init(alloc, .{});
    let mut vx = rustty_vaxis::Vaxis::init(rustty_vaxis::vaxis::Options::default());
    // defer vx.deinit(alloc, tty.writer());   (fired below)

    // We know we are ghostty, so let's enable mode 2027. Vaxis normally does this but you need an
    // event loop to auto-enable it.
    // vx.caps.unicode = .unicode;
    vx.caps.unicode = Method::Unicode;
    // try tty.writer().writeAll(vaxis.ctlseqs.unicode_set);
    tty.writer().write_all(ctlseqs::unicode_set)?;
    // defer tty.writer().writeAll(vaxis.ctlseqs.unicode_reset) catch {};   (fired LIFO-first below)

    let winsize = winsize_for(&tty)?;

    // The render body (from `vx.resize` on) — captured (not `?`-ed) so the three
    // reference `defer`s below fire on BOTH the Ok and the Err path.
    let res = render_colors(tty.writer(), &mut vx, winsize, keys, &arena);

    // PORT NOTE: the reference registers three `defer`s — `tty.deinit` (1st),
    // `vx.deinit` (2nd), `unicode_reset` (3rd) — that run LIFO at scope exit.
    // Emit them here in LIFO order (unicode_reset → vx.deinit → tty.deinit) so
    // cleanup runs on every exit; then propagate `res`.
    // defer tty.writer().writeAll(vaxis.ctlseqs.unicode_reset) catch {};
    let _ = tty.writer().write_all(ctlseqs::unicode_reset);
    // defer vx.deinit(alloc, tty.writer());
    vx.deinit(tty.writer());
    // defer tty.deinit();
    tty.deinit();
    res?;

    // return 0;
    Ok(0)
}

// The testable render seam: everything from `vx.resize` onward, verbatim in
// structure over a generic `Write` sink (a real tty writer in the device path,
// a `Vec<u8>` under test).
fn render_colors<'a, W: Write>(
    w: &mut W,
    vx: &mut rustty_vaxis::Vaxis<'a>,
    winsize: Winsize,
    keys: &[&'a str],
    arena: &'a Arena,
) -> std::io::Result<()> {
    // try vx.resize(alloc, tty.writer(), winsize);
    vx.resize(w, winsize)?;

    // const win = vx.window();
    //   Borrow note: `window()` returns a `Copy` `Window` holding a raw `*mut
    //   Screen` — it does NOT keep `&mut vx` alive, so the interleaved
    //   `vx.pretty_print(w)` at loop end is sound (the established §5 alias pattern).
    let win = vx.window();

    // var max_name_len: usize = 0;
    // for (keys) |name| {
    //     if (name.len > max_name_len) max_name_len = name.len;
    // }
    let mut max_name_len: usize = 0;
    for name in keys {
        if name.len() > max_name_len {
            max_name_len = name.len();
        }
    }

    // max name length plus " = #RRGGBB XX" plus "  " gutter between columns
    // const column_size = max_name_len + 15;
    let column_size = max_name_len + 15;
    // add two to take into account lack of gutter after last column
    // const columns: usize = @divFloor(win.width + 2, column_size);
    let columns: usize = (win.width as usize + 2) / column_size;

    // var i: usize = 0;
    // const step = @divFloor(keys.len, columns) + 1;
    let step = keys.len() / columns + 1;
    // while (i < step) : (i += 1) {
    for i in 0..step {
        // win.clear();
        win.clear();

        // var result: vaxis.Window.PrintResult = .{ .col = 0, .row = 0, .overflow = false };
        let mut result = PrintResult {
            col: 0,
            row: 0,
            overflow: false,
        };

        // for (0..columns) |j| {
        for j in 0..columns {
            // const k = i + (step * j);
            let k = i + (step * j);
            // if (k >= keys.len) continue;
            if k >= keys.len() {
                continue;
            }

            // const name = keys[k];
            let name = keys[k];
            // const rgb = x11_color.map.get(name).?;
            let rgb = x11_color::map.get(name).unwrap();

            // const style1: vaxis.Style = .{ .fg = .{ .rgb = .{ rgb.r, rgb.g, rgb.b } } };
            let style1 = Style {
                fg: Color::Rgb([rgb.r, rgb.g, rgb.b]),
                ..Default::default()
            };
            // const style2: vaxis.Style = .{ .fg = .{ .rgb = ... }, .bg = .{ .rgb = ... } };
            let style2 = Style {
                fg: Color::Rgb([rgb.r, rgb.g, rgb.b]),
                bg: Color::Rgb([rgb.r, rgb.g, rgb.b]),
                ..Default::default()
            };

            // name of the color
            // result = win.printSegment(.{ .text = name }, .{ .col_offset = result.col });
            result = win.print_segment(seg(name.as_bytes()), po(result.col));
            // push the color data to the end of the column
            // for (0..max_name_len - name.len) |_| {
            for _ in 0..(max_name_len - name.len()) {
                // result = win.printSegment(.{ .text = " " }, .{ .col_offset = result.col });
                result = win.print_segment(seg(b" "), po(result.col));
            }
            // result = win.printSegment(.{ .text = " = " }, .{ .col_offset = result.col });
            result = win.print_segment(seg(b" = "), po(result.col));
            // rgb triple
            // result = win.printSegment(.{
            //     .text = try std.fmt.allocPrint(alloc, "#{x:0>2}{x:0>2}{x:0>2}", .{ rgb.r, rgb.g, rgb.b }),
            //     .style = style1,
            // }, .{ .col_offset = result.col });
            //   PORT NOTE: `allocPrint(alloc, ...)` → `arena.dupe(...)`; the arena
            //   keeps the `#rrggbb` bytes alive for the screen's `'a` borrow (the
            //   reference arena's job).
            let hex = arena.dupe(format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b).as_bytes());
            result = win.print_segment(styled(hex, style1), po(result.col));
            // result = win.printSegment(.{ .text = " " }, .{ .col_offset = result.col });
            result = win.print_segment(seg(b" "), po(result.col));
            // colored block
            // result = win.printSegment(.{ .text = "  ", .style = style2 }, .{ .col_offset = result.col });
            result = win.print_segment(styled(b"  ", style2), po(result.col));
            // add the gutter if needed
            // if (j + 1 < columns) {
            if j + 1 < columns {
                // result = win.printSegment(.{ .text = "  " }, .{ .col_offset = result.col });
                result = win.print_segment(seg(b"  "), po(result.col));
            }
        }

        // clear the rest of the line
        // while (result.col != 0) {
        while result.col != 0 {
            // result = win.printSegment(.{ .text = " " }, .{ .col_offset = result.col });
            result = win.print_segment(seg(b" "), po(result.col));
        }

        // output the data
        // try vx.prettyPrint(tty.writer());
        vx.pretty_print(w)?;
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vaxis::vaxis::Options;

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // The render seam is pure over a `Vec<u8>` sink — no tty. Mirrors the vaxis
    // C4a `render_emits_*` smoke tests: drive rendering into a buffer, assert on
    // the emitted bytes. Runs under Miri.
    #[test]
    fn render_colors_emits_names_and_hex() {
        let mut vx = rustty_vaxis::Vaxis::init(Options::default());
        // enable mode 2027 like the device path does
        vx.caps.unicode = Method::Unicode;
        let winsize = Winsize {
            rows: 24,
            cols: 120,
            x_pixel: 0,
            y_pixel: 0,
        };
        let keys = ["black", "white"];
        let arena = Arena::init();
        let mut out: Vec<u8> = Vec::new();
        render_colors(&mut out, &mut vx, winsize, &keys, &arena).unwrap();

        // each color name is emitted (default-styled cells → contiguous bytes)
        assert!(contains(&out, b"black"));
        assert!(contains(&out, b"white"));
        // and its #rrggbb hex, styled with the color's rgb fg
        let black = x11_color::map.get("black").unwrap();
        let hex = format!("#{:02x}{:02x}{:02x}", black.r, black.g, black.b);
        assert!(contains(&out, hex.as_bytes()));
        // the `pretty_print` epilogue tail (sync_reset), as the C4a smoke asserts
        assert!(out.ends_with(ctlseqs::sync_reset));
    }

    // The device path opens a real `vaxis::Tty` (= PosixTty in a normal build of
    // rustty-cli's dep graph). `Tty::init` needs `/dev/tty`, absent in the sandbox
    // → `#[ignore]`-on-device.
    #[test]
    #[ignore = "opens a real /dev/tty; run only on a device"]
    fn list_colors_pretty_print_smoke() {
        let keys = ["black", "white"];
        assert_eq!(pretty_print(&keys).unwrap(), 0);
    }
}
