//! rustty-cli::list_keybinds
//!
//! PORTS: ghostty src/cli/list_keybinds.zig (0t) — the `+list-keybinds` action.
//!
//! PORT STATUS: C5 (CLI_BLUEPRINT.md §1/§7). Both paths land LIVE. The
//! `--plain`/non-tty path — the path the KAT and any redirected pipe hit —
//! streams `config.keybind.formatEntryDocs` over the ported `EntryFormatter`.
//! The interactive `prettyPrint` render path + its `ChordBinding`/`TriggerNode`/
//! `iterateBindings` helpers are now filled over the landed `rustty-vaxis` core
//! (WIRE1; DEFERRALS.md "cli C5" partial): the render is factored into a
//! `render_keybinds` seam (pure over a `Write` sink → smoke-tested + Miri-able)
//! behind the `pretty_print` device wrapper (opens a real tty → `#[ignore]`
//! -on-device). comment-fidelity (scoped-G3b) applies.

// const std = @import("std");
// const builtin = @import("builtin");
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const Arena = std.heap.ArenaAllocator;
// const Allocator = std.mem.Allocator;
// const configpkg = @import("../config.zig");
// const Config = configpkg.Config;
use rustty_config::Config;
//   PORT NOTE: `configpkg.entryFormatter("keybind", writer)` -> the ported
//   `rustty_cfg_fmt::entry_formatter`; `config.keybind.formatEntryDocs` is the
//   ported `Keybinds::format_entry_docs`.
use rustty_cfg_fmt::entry_formatter;
// const vaxis = @import("vaxis");
//   The interactive `prettyPrint` render leaf (WIRE1) drives the vaxis TUI core.
use rustty_vaxis::cell::{Color, Style};
use rustty_vaxis::gwidth::Method;
use rustty_vaxis::window::{PrintResult, Window};
use rustty_vaxis::{ctlseqs, Winsize};
// const input = @import("../input.zig");
// const tui = @import("tui.zig");
use crate::pretty_arena::{po, seg, styled, winsize_for, Arena};
use crate::tui;
// const Binding = input.Binding;
//   `Binding.Trigger`/`Binding.Action`/`Set`/`Value` back the `ChordBinding`/
//   `TriggerNode`/`iterateBindings` render helpers (WIRE1). `Action` here is the
//   INPUT-binding action (`BindingAction`), distinct from the cli `+command`
//   `Action` imported above.
use rustty_cfg_fmt::{AllocatingWriter, Writer as _};
use rustty_input::binding::{u21, Action as BindingAction, Trigger, TriggerKey, Value};

// The bare-stdout adapter presenting the buffered stdout as a
// `rustty_cfg_fmt::Writer` (the reference's `*std.Io.Writer`).
use crate::pager::IoWriter;
use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// If `true`, print out the default keybinds instead of the ones configured
    /// in the config file.
    // default: bool = false,
    pub default: bool,

    /// If `true`, print out documentation about the action associated with the
    /// keybinds.
    // docs: bool = false,
    pub docs: bool,

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

    // /// Enables `-h` and `--help` to work.
    // pub fn help(self: Options) !void {
    //     _ = self;
    //     return Action.help_error;
    // }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the three non-`_` bool flags.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // default: bool
            b"default" => {
                self.default = args::parse_field_bool(value)?;
                Ok(())
            }
            // docs: bool
            b"docs" => {
                self.docs = args::parse_field_bool(value)?;
                Ok(())
            }
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

/// The `list-keybinds` command is used to list all the available keybinds for
/// Rustty.
///
/// When executed without any arguments this will list the current keybinds
/// loaded by the config file. If no config file is found or there aren't any
/// changes to the keybinds it will print out the default ones configured for
/// Rustty
///
/// Flags:
///
///   * `--default`: will print out all the default keybinds
///
///   * `--docs`: currently does nothing, intended to print out documentation
///     about the action associated with the keybinds
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

    // var config = if (opts.default) try Config.default(alloc) else try Config.load(alloc);
    // defer config.deinit();
    //   PORT NOTE: `Config::deinit` resets the arena flag only; `config` drops at
    //   scope end.
    let config = if opts.default {
        Config::default_config()
    } else {
        Config::load()?
    };

    // var buffer: [1024]u8 = undefined;
    // const stdout: std.fs.File = .stdout();
    // var stdout_writer = stdout.writer(&buffer);
    // const writer = &stdout_writer.interface;
    let mut stdout_writer = IoWriter(std::io::BufWriter::new(std::io::stdout().lock()));

    // if (tui.can_pretty_print and !opts.plain and stdout.isTty()) {
    use std::io::IsTerminal;
    if tui::CAN_PRETTY_PRINT && !opts.plain && std::io::stdout().is_terminal() {
        // var arena = std.heap.ArenaAllocator.init(alloc);
        // defer arena.deinit();
        // return prettyPrint(arena.allocator(), config.keybind);
        //   WIRE1: the interactive render path, filled over rustty-vaxis (below).
        return pretty_print(&config.keybind);
    } else {
        // try config.keybind.formatEntryDocs(
        //     configpkg.entryFormatter("keybind", writer),
        //     opts.docs,
        // );
        let mut formatter = entry_formatter(b"keybind", &mut stdout_writer);
        config
            .keybind
            .format_entry_docs(&mut formatter, opts.docs)?;
    }

    // Don't forget to flush!
    // try writer.flush();
    stdout_writer.0.flush()?;

    opts.deinit();
    // return 0;
    Ok(0)
}

// ===========================================================================
// The interactive pretty-printer (WIRE1) and its ordering helpers. Filled over
// the landed `rustty-vaxis` core (DEFERRALS.md "cli C5" partial). The render body
// (from `vx.resize` on) is factored into the `render_keybinds` seam so a `Vec<u8>`
// smoke test can exercise it without a tty; the `pretty_print` device wrapper owns
// the tty/vaxis lifecycle.
// ===========================================================================

// const TriggerNode = struct {
//     data: Binding.Trigger,
//     node: std.SinglyLinkedList.Node = .{},
//
//     pub fn get(node: *std.SinglyLinkedList.Node) *TriggerNode {
//         return @fieldParentPtr("node", node);
//     }
// };
//   PORT NOTE: the reference models a chord as an INTRUSIVE
//   `std.SinglyLinkedList` of `TriggerNode`s (`@fieldParentPtr` + arena
//   `alloc.create`). The observable semantics are an ordered `Trigger` sequence
//   supporting forward iteration, `len()`, and `prepend` (leader chords). Ported
//   as a plain owned `Vec<Trigger>` (`prepend` = `insert(0, ..)`, `.first`-walk =
//   `.iter()`, `.len()` = `.len()`, `node.next != null` = `idx + 1 < len`) — the
//   `unsafe`-free port of a local (non-`pub`) helper; the intrusive list +
//   `@fieldParentPtr` are an arena-allocation vehicle, not observable behavior
//   (placeholder-leaf doctrine: preserve semantics, not the allocation mechanism).

// const ChordBinding = struct {
//     table_name: ?[]const u8 = null,
//     triggers: std.SinglyLinkedList,
//     actions: []const Binding.Action,
struct ChordBinding<'a> {
    // table_name: ?[]const u8 = null,   (borrows a `tables` map key)
    table_name: Option<&'a [u8]>,
    // triggers: std.SinglyLinkedList,   (see the TriggerNode PORT NOTE → Vec<Trigger>)
    triggers: Vec<Trigger>,
    // actions: []const Binding.Action,  (owned; BindingAction: Clone)
    actions: Vec<BindingAction>,
}

// `.catch_all` sorts last (`std.math.maxInt(c_int)`); `.physical` uses the key's
// enum ordinal (`@intFromEnum`), `.unicode` the codepoint (`@intCast`).
fn trigger_sort_key(t: &Trigger) -> std::os::raw::c_int {
    match t.key {
        // .physical => |key| break :blk @intFromEnum(key),
        TriggerKey::Physical(key) => key as std::os::raw::c_int,
        // .unicode => |key| break :blk @intCast(key),
        TriggerKey::Unicode(key) => key as std::os::raw::c_int,
        // .catch_all => break :blk std.math.maxInt(c_int),
        TriggerKey::CatchAll => std::os::raw::c_int::MAX,
    }
}

// {u} — the UTF-8 encoding of the codepoint (allocPrint("{u}", .{c})).
fn utf8(c: u21) -> Vec<u8> {
    let mut b = [0u8; 4];
    // PORT NOTE: `u21` is a `u32` alias, so `@intCast`/`char::from_u32` needs no cast.
    char::from_u32(c)
        .unwrap()
        .encode_utf8(&mut b)
        .as_bytes()
        .to_vec()
}

impl ChordBinding<'_> {
    // Order keybinds based on various properties
    //    1. Default bindings before table bindings (tables grouped at end)
    //    2. Longest chord sequence
    //    3. Most active modifiers
    //    4. Alphabetically by active modifiers
    //    5. Trigger key order
    //    6. Within tables, sort by table name
    // These properties propagate through chorded keypresses
    //
    // Adapted from Binding.lessThan
    //   PORT NOTE: `lessThan(void, lhs, rhs) bool` → `cmp(&lhs, &rhs) -> Ordering`
    //   for `std.mem.sort` (Rust STABLE `sort_by`). Each reference `return <bool>`
    //   becomes `Less`/`Greater` (`true` → `Less`, "lhs sorts first"); the
    //   (unreachable) fall-through returns `Equal`.
    // pub fn lessThan(_: void, lhs: ChordBinding, rhs: ChordBinding) bool {
    fn cmp(lhs: &ChordBinding<'_>, rhs: &ChordBinding<'_>) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // const lhs_has_table = lhs.table_name != null;
        let lhs_has_table = lhs.table_name.is_some();
        // const rhs_has_table = rhs.table_name != null;
        let rhs_has_table = rhs.table_name.is_some();

        // if (lhs_has_table != rhs_has_table) {
        //     return !lhs_has_table;
        // }
        if lhs_has_table != rhs_has_table {
            // !lhs_has_table (defaults before tables): true → lhs first → Less.
            return if !lhs_has_table {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }

        // if (lhs_has_table) {
        if lhs_has_table {
            // const table_cmp = std.mem.order(u8, lhs.table_name.?, rhs.table_name.?);
            let table_cmp = lhs.table_name.unwrap().cmp(rhs.table_name.unwrap());
            // if (table_cmp != .eq) {
            //     return table_cmp == .lt;
            // }
            if table_cmp != Ordering::Equal {
                return table_cmp;
            }
        }

        // const lhs_len = lhs.triggers.len();
        let lhs_len = lhs.triggers.len();
        // const rhs_len = rhs.triggers.len();
        let rhs_len = rhs.triggers.len();

        // std.debug.assert(lhs_len != 0);
        debug_assert!(lhs_len != 0);
        // std.debug.assert(rhs_len != 0);
        debug_assert!(rhs_len != 0);

        // if (lhs_len != rhs_len) {
        //     return lhs_len > rhs_len;
        // }
        if lhs_len != rhs_len {
            // longest chord first: `lhs_len > rhs_len` → Less.
            return rhs_len.cmp(&lhs_len);
        }

        // const lhs_count: usize = blk: { ... count active mods across the chord ... };
        let lhs_count: usize = {
            // var count: usize = 0;
            let mut count: usize = 0;
            // var maybe_trigger = lhs.triggers.first;
            // while (maybe_trigger) |node| : (maybe_trigger = node.next) {
            //     const trigger: *TriggerNode = .get(node);
            for trigger in &lhs.triggers {
                // if (trigger.data.mods.super) count += 1;
                if trigger.mods.super_ {
                    count += 1;
                }
                // if (trigger.data.mods.ctrl) count += 1;
                if trigger.mods.ctrl {
                    count += 1;
                }
                // if (trigger.data.mods.shift) count += 1;
                if trigger.mods.shift {
                    count += 1;
                }
                // if (trigger.data.mods.alt) count += 1;
                if trigger.mods.alt {
                    count += 1;
                }
            }
            // break :blk count;
            count
        };
        // const rhs_count: usize = blk: { ... };
        let rhs_count: usize = {
            let mut count: usize = 0;
            for trigger in &rhs.triggers {
                if trigger.mods.super_ {
                    count += 1;
                }
                if trigger.mods.ctrl {
                    count += 1;
                }
                if trigger.mods.shift {
                    count += 1;
                }
                if trigger.mods.alt {
                    count += 1;
                }
            }
            count
        };

        // if (lhs_count != rhs_count)
        //     return lhs_count > rhs_count;
        if lhs_count != rhs_count {
            // most active modifiers first.
            return rhs_count.cmp(&lhs_count);
        }

        // { // pairwise mods.int() along the chord (alphabetically by active modifiers)
        {
            // var l_trigger = lhs.triggers.first;
            // var r_trigger = rhs.triggers.first;
            // while (l_trigger != null and r_trigger != null) {
            for (l, r) in lhs.triggers.iter().zip(rhs.triggers.iter()) {
                // const l_int = TriggerNode.get(l_trigger.?).data.mods.int();
                let l_int = l.mods.int();
                // const r_int = TriggerNode.get(r_trigger.?).data.mods.int();
                let r_int = r.mods.int();

                // if (l_int != r_int) {
                //     return l_int > r_int;
                // }
                if l_int != r_int {
                    return r_int.cmp(&l_int);
                }
                // l_trigger = l_trigger.?.next;
                // r_trigger = r_trigger.?.next;
            }
        }

        // var l_trigger = lhs.triggers.first;
        // var r_trigger = rhs.triggers.first;
        //
        // while (l_trigger != null and r_trigger != null) {
        //   PORT NOTE: the reference advances BOTH cursors at the TOP of the body,
        //   then tests "are we now at the end?" via the post-advance null check —
        //   the final in-range pair always returns (via `<`). Reproduce over the
        //   shared length (lhs_len == rhs_len here) with an index.
        let n = lhs_len;
        let mut idx = 0usize;
        while idx < n {
            // We want catch_all to sort last.
            // const lhs_key: c_int = blk: { switch (TriggerNode.get(l_trigger.?).data.key) {...} };
            let lhs_key = trigger_sort_key(&lhs.triggers[idx]);
            // const rhs_key: c_int = blk: { switch (TriggerNode.get(r_trigger.?).data.key) {...} };
            let rhs_key = trigger_sort_key(&rhs.triggers[idx]);

            // l_trigger = l_trigger.?.next;
            // r_trigger = r_trigger.?.next;
            idx += 1;

            // if (l_trigger == null or r_trigger == null) {
            //     return lhs_key < rhs_key;
            // }
            if idx >= n {
                return lhs_key.cmp(&rhs_key);
            }

            // if (lhs_key != rhs_key) {
            //     return lhs_key < rhs_key;
            // }
            if lhs_key != rhs_key {
                return lhs_key.cmp(&rhs_key);
            }
        }

        // The previous loop will always return something on its final iteration so we cannot
        // reach this point
        // unreachable;
        unreachable!()
    }
}

// fn prettyPrint(alloc: Allocator, keybinds: Config.Keybinds) !u8 {
//   The device wrapper: sets up the tty/vaxis lifecycle, then delegates the
//   drawing to `render_keybinds` (the testable seam).
fn pretty_print(keybinds: &rustty_config::value_types::Keybinds) -> Result<u8, Error> {
    // var arena = std.heap.ArenaAllocator.init(alloc);  (see run() above)
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
    // const writer = tty.writer();
    // defer vx.deinit(alloc, writer);   (fired below)

    // We know we are ghostty, so let's enable mode 2027. Vaxis normally does this but you need an
    // event loop to auto-enable it.
    // vx.caps.unicode = .unicode;
    vx.caps.unicode = Method::Unicode;
    // try writer.writeAll(vaxis.ctlseqs.unicode_set);
    tty.writer().write_all(ctlseqs::unicode_set)?;
    // defer writer.writeAll(vaxis.ctlseqs.unicode_reset) catch {};   (fired LIFO-first below)

    let winsize = winsize_for(&tty)?;

    // The render body (from `vx.resize` on) — captured (not `?`-ed) so the three
    // reference `defer`s below fire on BOTH the Ok and the Err path.
    let res = render_keybinds(tty.writer(), &mut vx, winsize, keybinds, &arena);

    // PORT NOTE: the reference registers three `defer`s — `tty.deinit` (1st),
    // `vx.deinit` (2nd), `unicode_reset` (3rd) — that run LIFO at scope exit.
    // Emit them here in LIFO order (unicode_reset → vx.deinit → tty.deinit).
    // defer writer.writeAll(vaxis.ctlseqs.unicode_reset) catch {};
    let _ = tty.writer().write_all(ctlseqs::unicode_reset);
    // defer vx.deinit(alloc, writer);
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
fn render_keybinds<'a, W: Write>(
    w: &mut W,
    vx: &mut rustty_vaxis::Vaxis<'a>,
    winsize: Winsize,
    keybinds: &'a rustty_config::value_types::Keybinds,
    arena: &'a Arena,
) -> std::io::Result<()> {
    // try vx.resize(alloc, writer, winsize);
    vx.resize(w, winsize)?;

    // const win = vx.window();
    //   Borrow note: `window()` returns a `Copy` `Window` holding a raw `*mut
    //   Screen` — it does NOT keep `&mut vx` alive, so the interleaved
    //   `vx.pretty_print(w)` at loop end is sound (the established §5 alias pattern).
    let win = vx.window();

    // Collect default bindings, recursively flattening chords
    // var iter = keybinds.set.bindings.iterator();
    // const default_bindings, var widest_chord = try iterateBindings(alloc, &iter, &win);
    let (default_bindings, mut widest_chord) = iterate_bindings(keybinds.set.iter_bindings(), win);

    // var bindings_list: std.ArrayList(ChordBinding) = .empty;
    // try bindings_list.appendSlice(alloc, default_bindings);
    let mut bindings_list: Vec<ChordBinding> = default_bindings;

    // Collect key table bindings
    // var widest_table_prefix: u16 = 0;
    let mut widest_table_prefix: u16 = 0;
    // var table_iter = keybinds.tables.iterator();
    // while (table_iter.next()) |table_entry| {
    for (table_name, set) in keybinds.tables.iter() {
        // const table_name = table_entry.key_ptr.*;
        // var binding_iter = table_entry.value_ptr.bindings.iterator();
        // const table_bindings, const table_width = try iterateBindings(alloc, &binding_iter, &win);
        let (mut table_bindings, table_width) = iterate_bindings(set.iter_bindings(), win);
        // for (table_bindings) |*b| {
        //     b.table_name = table_name;
        // }
        for b in &mut table_bindings {
            b.table_name = Some(table_name.as_slice());
        }

        // try bindings_list.appendSlice(alloc, table_bindings);
        bindings_list.extend(table_bindings);
        // widest_chord = @max(widest_chord, table_width);
        widest_chord = widest_chord.max(table_width);
        // widest_table_prefix = @max(widest_table_prefix, @as(u16, @intCast(win.gwidth(table_name) + win.gwidth("/"))));
        widest_table_prefix = widest_table_prefix.max(win.gwidth(table_name) + win.gwidth(b"/"));
    }

    // const bindings = bindings_list.items;
    // std.mem.sort(ChordBinding, bindings, {}, ChordBinding.lessThan);
    //   PORT NOTE: `std.mem.sort` is STABLE → Rust `sort_by`.
    bindings_list.sort_by(|a, b| ChordBinding::cmp(a, b));

    // Set up styles for each modifier
    // const super_style: vaxis.Style = .{ .fg = .{ .index = 1 } };
    let super_style = Style {
        fg: Color::Index(1),
        ..Default::default()
    };
    // const ctrl_style: vaxis.Style = .{ .fg = .{ .index = 2 } };
    let ctrl_style = Style {
        fg: Color::Index(2),
        ..Default::default()
    };
    // const alt_style: vaxis.Style = .{ .fg = .{ .index = 3 } };
    let alt_style = Style {
        fg: Color::Index(3),
        ..Default::default()
    };
    // const shift_style: vaxis.Style = .{ .fg = .{ .index = 4 } };
    let shift_style = Style {
        fg: Color::Index(4),
        ..Default::default()
    };
    // const table_style: vaxis.Style = .{ .fg = .{ .index = 8 } };
    let table_style = Style {
        fg: Color::Index(8),
        ..Default::default()
    };

    // Print the list
    // for (bindings) |bind| {
    for bind in &bindings_list {
        // win.clear();
        win.clear();

        // var result: vaxis.Window.PrintResult = .{ .col = 0, .row = 0, .overflow = false };
        let mut result = PrintResult {
            col: 0,
            row: 0,
            overflow: false,
        };

        // if (bind.table_name) |name| {
        if let Some(name) = bind.table_name {
            // result = win.printSegment(.{ .text = name, .style = table_style }, .{ .col_offset = result.col });
            result = win.print_segment(styled(name, table_style), po(result.col));
            // result = win.printSegment(.{ .text = "/", .style = table_style }, .{ .col_offset = result.col });
            result = win.print_segment(styled(b"/", table_style), po(result.col));
        }

        // var maybe_trigger = bind.triggers.first;
        // while (maybe_trigger) |node| : (maybe_trigger = node.next) {
        //     const trigger: *TriggerNode = .get(node);
        let n = bind.triggers.len();
        for (idx, trigger) in bind.triggers.iter().enumerate() {
            // if (trigger.data.mods.super) {
            if trigger.mods.super_ {
                // result = win.printSegment(.{ .text = "super", .style = super_style }, .{ .col_offset = result.col });
                result = win.print_segment(styled(b"super", super_style), po(result.col));
                // result = win.printSegment(.{ .text = " + " }, .{ .col_offset = result.col });
                result = win.print_segment(seg(b" + "), po(result.col));
            }
            // if (trigger.data.mods.ctrl) {
            if trigger.mods.ctrl {
                // result = win.printSegment(.{ .text = "ctrl ", .style = ctrl_style }, .{ .col_offset = result.col });
                result = win.print_segment(styled(b"ctrl ", ctrl_style), po(result.col));
                // result = win.printSegment(.{ .text = " + " }, .{ .col_offset = result.col });
                result = win.print_segment(seg(b" + "), po(result.col));
            }
            // if (trigger.data.mods.alt) {
            if trigger.mods.alt {
                // result = win.printSegment(.{ .text = "alt  ", .style = alt_style }, .{ .col_offset = result.col });
                result = win.print_segment(styled(b"alt  ", alt_style), po(result.col));
                // result = win.printSegment(.{ .text = " + " }, .{ .col_offset = result.col });
                result = win.print_segment(seg(b" + "), po(result.col));
            }
            // if (trigger.data.mods.shift) {
            if trigger.mods.shift {
                // result = win.printSegment(.{ .text = "shift", .style = shift_style }, .{ .col_offset = result.col });
                result = win.print_segment(styled(b"shift", shift_style), po(result.col));
                // result = win.printSegment(.{ .text = " + " }, .{ .col_offset = result.col });
                result = win.print_segment(seg(b" + "), po(result.col));
            }
            // const key = switch (trigger.data.key) {
            //     .physical => |k| try std.fmt.allocPrint(alloc, "{t}", .{k}),
            //     .unicode => |c| try std.fmt.allocPrint(alloc, "{u}", .{c}),
            //     .catch_all => "catch_all",
            // };
            let key: &[u8] = match trigger.key {
                TriggerKey::Physical(k) => arena.dupe(k.tag_name().as_bytes()),
                TriggerKey::Unicode(c) => arena.dupe(&utf8(c)),
                TriggerKey::CatchAll => b"catch_all",
            };
            // result = win.printSegment(.{ .text = key }, .{ .col_offset = result.col });
            result = win.print_segment(seg(key), po(result.col));

            // Print a separator between chorded keys
            // if (trigger.node.next != null) {
            if idx + 1 < n {
                // result = win.printSegment(.{ .text = "  >  ", .style = .{ .bold = true, .fg = .{ .index = 6 } } }, .{ .col_offset = result.col });
                result = win.print_segment(
                    styled(
                        b"  >  ",
                        Style {
                            bold: true,
                            fg: Color::Index(6),
                            ..Default::default()
                        },
                    ),
                    po(result.col),
                );
            }
        }

        // var action_col: u16 = widest_table_prefix + widest_chord + 3;
        let mut action_col: u16 = widest_table_prefix + widest_chord + 3;
        // for (bind.actions, 0..) |act, i| {
        for (i, act) in bind.actions.iter().enumerate() {
            // if (i > 0) {
            if i > 0 {
                // const chain_result = win.printSegment(.{ .text = ", ", .style = .{ .dim = true } }, .{ .col_offset = action_col });
                let chain_result = win.print_segment(
                    styled(
                        b", ",
                        Style {
                            dim: true,
                            ..Default::default()
                        },
                    ),
                    po(action_col),
                );
                // action_col = chain_result.col;
                action_col = chain_result.col;
            }

            // const action = try std.fmt.allocPrint(alloc, "{f}", .{act});
            //   PORT NOTE: `{f}` = `Action.format` into `std.Io.Writer.Allocating`
            //   → `AllocatingWriter` (Error = Infallible → `.unwrap()`); the bytes
            //   are duped into the arena so the `&'a [u8]` borrow lives across the
            //   `print`/`print_segment` (the reference `allocPrint` lifetime).
            let mut aw = AllocatingWriter::init();
            act.format(&mut aw).unwrap();
            let action: &[u8] = arena.dupe(aw.written());
            // If our action has an argument, we print the argument in a different color
            // if (std.mem.indexOfScalar(u8, action, ':')) |idx| {
            if let Some(idx) = action.iter().position(|&b| b == b':') {
                // const print_result = win.print(&.{
                //     .{ .text = action[0..idx] },
                //     .{ .text = action[idx .. idx + 1], .style = .{ .dim = true } },
                //     .{ .text = action[idx + 1 ..], .style = .{ .fg = .{ .index = 5 } } },
                // }, .{ .col_offset = action_col });
                let print_result = win.print(
                    &[
                        seg(&action[0..idx]),
                        styled(
                            &action[idx..idx + 1],
                            Style {
                                dim: true,
                                ..Default::default()
                            },
                        ),
                        styled(
                            &action[idx + 1..],
                            Style {
                                fg: Color::Index(5),
                                ..Default::default()
                            },
                        ),
                    ],
                    po(action_col),
                );
                // action_col = print_result.col;
                action_col = print_result.col;
            } else {
                // const print_result = win.printSegment(.{ .text = action }, .{ .col_offset = action_col });
                let print_result = win.print_segment(seg(action), po(action_col));
                // action_col = print_result.col;
                action_col = print_result.col;
            }
        }
        // try vx.prettyPrint(writer);
        vx.pretty_print(w)?;
    }
    // try writer.flush();
    w.flush()?;
    // return 0;
    Ok(())
}

// fn iterateBindings(
//     alloc: Allocator,
//     iter: anytype,
//     win: *const vaxis.Window,
// ) !struct { []ChordBinding, u16 } {
//   PORT NOTE: `win: *const vaxis.Window` is `Copy` → passed by value; it is used
//   only for `gwidth`. The returned owned slice is a `Vec<ChordBinding>` (Zig
//   `toOwnedSlice`). `'r` is the (free) `ChordBinding` lifetime (table_name is
//   `None` here — set by the caller); `'i` is the iterator's borrow.
fn iterate_bindings<'r, 'i, 'w>(
    iter: impl Iterator<Item = (&'i Trigger, &'i Value)>,
    win: Window<'w>,
) -> (Vec<ChordBinding<'r>>, u16) {
    // var widest_chord: u16 = 0;
    let mut widest_chord: u16 = 0;
    // var bindings: std.ArrayList(ChordBinding) = .empty;
    let mut bindings: Vec<ChordBinding<'r>> = Vec::new();
    // while (iter.next()) |bind| {
    for (t, value) in iter {
        // const width = blk: {
        let width = {
            // var buf: std.Io.Writer.Allocating = .init(alloc);
            let mut buf = AllocatingWriter::init();
            // const t = bind.key_ptr.*;   (already destructured above)

            // if (t.mods.super) try buf.writer.print("super + ", .{});
            if t.mods.super_ {
                buf.write_all(b"super + ").unwrap();
            }
            // if (t.mods.ctrl) try buf.writer.print("ctrl  + ", .{});
            if t.mods.ctrl {
                buf.write_all(b"ctrl  + ").unwrap();
            }
            // if (t.mods.alt) try buf.writer.print("alt   + ", .{});
            if t.mods.alt {
                buf.write_all(b"alt   + ").unwrap();
            }
            // if (t.mods.shift) try buf.writer.print("shift + ", .{});
            if t.mods.shift {
                buf.write_all(b"shift + ").unwrap();
            }

            // switch (t.key) {
            match t.key {
                // .physical => |k| try buf.writer.print("{t}", .{k}),
                TriggerKey::Physical(k) => buf.write_all(k.tag_name().as_bytes()).unwrap(),
                // .unicode => |c| try buf.writer.print("{u}", .{c}),
                TriggerKey::Unicode(c) => buf.write_all(&utf8(c)).unwrap(),
                // .catch_all => try buf.writer.print("catch_all", .{}),
                TriggerKey::CatchAll => buf.write_all(b"catch_all").unwrap(),
            }

            // break :blk win.gwidth(buf.written());
            win.gwidth(buf.written())
        };

        // switch (bind.value_ptr.*) {
        match value {
            // .leader => |leader| {
            Value::Leader(leader) => {
                // Recursively iterate on the set of bindings for this leader key
                // var n_iter = leader.bindings.iterator();
                // const sub_bindings, const max_width = try iterateBindings(alloc, &n_iter, win);
                let (mut sub_bindings, max_width) = iterate_bindings(leader.iter_bindings(), win);

                // Prepend the current keybind onto the list of sub-binds
                // for (sub_bindings) |*nb| {
                for nb in &mut sub_bindings {
                    // const prepend_node = try alloc.create(TriggerNode);
                    // prepend_node.* = .{ .data = bind.key_ptr.* };
                    // nb.triggers.prepend(&prepend_node.node);
                    //   PORT NOTE: intrusive `prepend` → `Vec::insert(0, *t)`.
                    nb.triggers.insert(0, *t);
                }

                // Add the longest sub-bind width to the current bind width along with a padding
                // of 5 for the '  >  ' spacer
                // widest_chord = @max(widest_chord, width + max_width + 5);
                widest_chord = widest_chord.max(width + max_width + 5);
                // try bindings.appendSlice(alloc, sub_bindings);
                bindings.extend(sub_bindings);
            }
            // .leaf => |leaf| {
            Value::Leaf(leaf) => {
                // const node = try alloc.create(TriggerNode);
                // node.* = .{ .data = bind.key_ptr.* };
                //
                // const actions = try alloc.alloc(Binding.Action, 1);
                // actions[0] = leaf.action;
                //
                // widest_chord = @max(widest_chord, width);
                widest_chord = widest_chord.max(width);
                // try bindings.append(alloc, .{ .triggers = .{ .first = &node.node }, .actions = actions });
                bindings.push(ChordBinding {
                    table_name: None,
                    triggers: vec![*t],
                    actions: vec![leaf.action.clone()],
                });
            }
            // .leaf_chained => |leaf| {
            Value::LeafChained(leaf) => {
                // const node = try alloc.create(TriggerNode);
                // node.* = .{ .data = bind.key_ptr.* };
                //
                // widest_chord = @max(widest_chord, width);
                widest_chord = widest_chord.max(width);
                // try bindings.append(alloc, .{ .triggers = .{ .first = &node.node }, .actions = leaf.actions.items });
                bindings.push(ChordBinding {
                    table_name: None,
                    triggers: vec![*t],
                    actions: leaf.actions.clone(),
                });
            }
        }
    }

    // return .{ try bindings.toOwnedSlice(alloc), widest_chord };
    (bindings, widest_chord)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vaxis::vaxis::Options;

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // Build a fresh Vaxis + real Window over a synthetic winsize; return them so a
    // test can call `iterate_bindings` / render with a live `gwidth`.
    fn resized_vaxis() -> rustty_vaxis::Vaxis<'static> {
        let mut vx = rustty_vaxis::Vaxis::init(Options::default());
        vx.caps.unicode = Method::Unicode;
        let mut sink: Vec<u8> = Vec::new();
        vx.resize(
            &mut sink,
            Winsize {
                rows: 24,
                cols: 200,
                x_pixel: 0,
                y_pixel: 0,
            },
        )
        .unwrap();
        vx
    }

    // The render seam is pure over a `Vec<u8>` sink — no tty. Mirrors the vaxis
    // C4a `render_emits_*` smoke tests. Runs under Miri.
    #[test]
    fn render_keybinds_emits_action_and_argument_split() {
        let keybinds = Config::default_config().keybind;
        let mut vx = rustty_vaxis::Vaxis::init(Options::default());
        vx.caps.unicode = Method::Unicode;
        let winsize = Winsize {
            rows: 24,
            cols: 200,
            x_pixel: 0,
            y_pixel: 0,
        };
        let arena = Arena::init();
        let mut out: Vec<u8> = Vec::new();
        render_keybinds(&mut out, &mut vx, winsize, &keybinds, &arena).unwrap();

        // the default keybinds contain a `new_split:right` action → both the tag
        // and (for a `foo:bar` action) the split emit the argument bytes.
        assert!(contains(&out, b"new_split"));
        assert!(contains(&out, b"right"));
        // the `pretty_print` epilogue tail (sync_reset), as the C4a smoke asserts.
        assert!(out.ends_with(ctlseqs::sync_reset));
    }

    // `ChordBinding::cmp`: defaults sort before tables; longer chords first.
    #[test]
    fn chord_binding_cmp_orders_defaults_then_tables_then_length() {
        use std::cmp::Ordering;
        let one = |mods_super: bool| Trigger {
            key: TriggerKey::Unicode(b'a' as u21),
            mods: rustty_vt::input::key_mods::Mods {
                super_: mods_super,
                ..Default::default()
            },
        };
        let default_single = ChordBinding {
            table_name: None,
            triggers: vec![one(false)],
            actions: vec![],
        };
        let table_single = ChordBinding {
            table_name: Some(b"foo".as_slice()),
            triggers: vec![one(false)],
            actions: vec![],
        };
        let default_chord = ChordBinding {
            table_name: None,
            triggers: vec![one(false), one(false)],
            actions: vec![],
        };

        // defaults before tables
        assert_eq!(
            ChordBinding::cmp(&default_single, &table_single),
            Ordering::Less
        );
        // longer chord first (both defaults)
        assert_eq!(
            ChordBinding::cmp(&default_chord, &default_single),
            Ordering::Less
        );
    }

    // `iterate_bindings` flattens a leader chord: the sub-binding's chord is
    // prepended with the leader trigger (len == 2).
    #[test]
    fn iterate_bindings_flattens_leader_chords() {
        let keybinds = Config::default_config().keybind;
        let mut vx = resized_vaxis();
        let win = vx.window();
        let (bindings, _widest) = iterate_bindings(keybinds.set.iter_bindings(), win);
        // every flattened binding has at least one trigger and non-empty actions
        assert!(!bindings.is_empty());
        for b in &bindings {
            assert!(!b.triggers.is_empty());
            assert!(!b.actions.is_empty());
        }
    }

    // The device path opens a real `vaxis::Tty` (= PosixTty in a normal build).
    // `Tty::init` needs `/dev/tty`, absent in the sandbox → `#[ignore]`-on-device.
    #[test]
    #[ignore = "opens a real /dev/tty; run only on a device"]
    fn list_keybinds_pretty_print_smoke() {
        let keybinds = Config::default_config().keybind;
        assert_eq!(pretty_print(&keybinds).unwrap(), 0);
    }
}
