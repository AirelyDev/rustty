//! rustty-cli::explain_config
//!
//! PORTS: ghostty src/cli/explain_config.zig (1t → **DEFERRED**, ADV-FIX 3).
//!
//! PORT STATUS: the file is FULLY ported; its 1 test ("explain") is DEFERRED
//! because its assertions require the build-generated `help_strings.Config` /
//! `help_strings.KeybindAction` doc tables — a deferred seam in rustty
//! (`explain_option`/`explain_keybind` return `None` until that codegen lands).
//! The test body is preserved verbatim (R1) as a documented, un-deferrable-later
//! block at the bottom; check_test_counts accounts for it via
//! `apc_deferred=1` (DEFERRALS.md "cli C2 — explain_config").

// const std = @import("std");
// const args = @import("args.zig");
use rustty_args::args;
// const Allocator = std.mem.Allocator;
// const Action = @import("ghostty.zig").Action;
use crate::ghostty::Error;
// const help_strings = @import("help_strings");
//   -> DEFERRED seam (see `explain_option`/`explain_keybind`).
// const Config = @import("../config/Config.zig");
// const ConfigKey = @import("../config/key.zig").Key;
use rustty_config::Key as ConfigKey;
// const KeybindAction = @import("../input/Binding.zig").Action;
use rustty_input::binding::Action as KeybindAction;
// const Pager = @import("Pager.zig");
use crate::pager::Pager;

use rustty_cfg_fmt::Writer;
use std::io::Write;

// pub const Options = struct {
//
// PORT NOTE: `run` parses its args MANUALLY (the `while (iter.next())` loop
// below), so this `Options` struct is not driven through `args.parse`; it exists
// to mirror the reference's per-action `Options` (named by the completions
// generator's `Action.options()` mapping, commented out in ghostty.rs). Ported
// for R1 fidelity.
#[allow(dead_code)]
pub struct Options {
    /// The config option to explain. For example:
    ///
    ///   rustty +explain-config --option=font-size
    // option: ?[]const u8 = null,
    pub option: Option<Vec<u8>>,

    /// The keybind action to explain. For example:
    ///
    ///   rustty +explain-config --keybind=copy_to_clipboard
    // keybind: ?[]const u8 = null,
    pub keybind: Option<Vec<u8>>,
}

#[allow(dead_code)]
impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }

    /// Enables `-h` and `--help` to work.
    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    pub fn help(&self) -> Result<(), Error> {
        let _ = self;
        // return Action.help_error;
        Err(Error::ActionHelpRequested)
    }
}

/// The `explain-config` command prints the documentation for a single
/// rustty configuration option or keybind action.
///
/// Examples:
///
///   rustty +explain-config font-size
///   rustty +explain-config copy_to_clipboard
///   rustty +explain-config --option=font-size
///   rustty +explain-config --keybind=copy_to_clipboard
///
/// Flags:
///
///   * `--option`: The name of the configuration option to explain.
///   * `--keybind`: The name of the keybind action to explain.
///   * `--no-pager`: Disable automatic paging of output.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var option_name: ?[]const u8 = null;
    // var keybind_name: ?[]const u8 = null;
    // var positional: ?[]const u8 = null;
    // var no_pager: bool = false;
    let mut option_name: Option<Vec<u8>> = None;
    let mut keybind_name: Option<Vec<u8>> = None;
    let mut positional: Option<Vec<u8>> = None;
    let mut no_pager: bool = false;

    // var iter = try args.argsIterator(alloc);
    // defer iter.deinit();
    // defer if (option_name) |s| alloc.free(s);
    // defer if (keybind_name) |s| alloc.free(s);
    // defer if (positional) |s| alloc.free(s);
    //   PORT NOTE: the reference `dupe`s each captured arg and `free`s them via
    //   `defer`; the Rust port captures into owned `Vec<u8>` (freed on drop), so
    //   the explicit frees fall away.
    let mut iter = args::args_iterator()?;

    // while (iter.next()) |arg| {
    while let Some(arg) = iter.next() {
        let arg: &[u8] = &arg;
        // if (std.mem.startsWith(u8, arg, "--option=")) {
        //     option_name = try alloc.dupe(u8, arg["--option=".len..]);
        if let Some(rest) = arg.strip_prefix(b"--option=") {
            option_name = Some(rest.to_vec());
        // } else if (std.mem.startsWith(u8, arg, "--keybind=")) {
        //     keybind_name = try alloc.dupe(u8, arg["--keybind=".len..]);
        } else if let Some(rest) = arg.strip_prefix(b"--keybind=") {
            keybind_name = Some(rest.to_vec());
        // } else if (std.mem.eql(u8, arg, "--no-pager")) {
        //     no_pager = true;
        } else if arg == b"--no-pager" {
            no_pager = true;
        // } else if (std.mem.eql(u8, arg, "--help") or std.mem.eql(u8, arg, "-h")) {
        //     return Action.help_error;
        } else if arg == b"--help" || arg == b"-h" {
            iter.deinit();
            return Err(Error::ActionHelpRequested);
        // } else if (!std.mem.startsWith(u8, arg, "-")) {
        //     positional = try alloc.dupe(u8, arg);
        } else if !arg.starts_with(b"-") {
            positional = Some(arg.to_vec());
        }
        // }
    }
    iter.deinit();

    // Resolve what to look up. Explicit flags go directly to their
    // respective lookup. A bare positional argument tries config
    // options first, then keybind actions as a fallback.
    // const name = keybind_name orelse option_name orelse positional orelse {
    let name: Vec<u8> = match keybind_name
        .clone()
        .or_else(|| option_name.clone())
        .or_else(|| positional.clone())
    {
        Some(n) => n,
        None => {
            //     var stderr: std.fs.File = .stderr();
            //     var buffer: [4096]u8 = undefined;
            //     var stderr_writer = stderr.writer(&buffer);
            //     try stderr_writer.interface.writeAll("Usage: ghostty +explain-config <option>\n");
            //     try stderr_writer.interface.writeAll("       ghostty +explain-config --option=<option>\n");
            //     try stderr_writer.interface.writeAll("       ghostty +explain-config --keybind=<action>\n");
            //     try stderr_writer.end();
            //     return 1;
            //   R2: user-facing `ghostty` -> the rustty product name (APP_NAME).
            let app = rustty_identity::APP_NAME;
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(format!("Usage: {app} +explain-config <option>\n").as_bytes())?;
            stderr.write_all(
                format!("       {app} +explain-config --option=<option>\n").as_bytes(),
            )?;
            stderr.write_all(
                format!("       {app} +explain-config --keybind=<action>\n").as_bytes(),
            )?;
            stderr.flush()?;
            return Ok(1);
        }
    };

    // const text = if (keybind_name != null)
    //     explainKeybind(name)
    // else if (option_name != null)
    //     explainOption(name)
    // else
    //     explainOption(name) orelse explainKeybind(name);
    let text: Option<&[u8]> = if keybind_name.is_some() {
        explain_keybind(&name)
    } else if option_name.is_some() {
        explain_option(&name)
    } else {
        explain_option(&name).or_else(|| explain_keybind(&name))
    };

    // var pager: Pager = if (!no_pager) .init(alloc) else .{};
    // defer pager.deinit();
    //   (`defer deinit` == Pager's `Drop`; `writer` below is dropped first,
    //   flushing, then `pager` runs deinit — matching the reference order.)
    let mut pager: Pager = if !no_pager {
        Pager::init()
    } else {
        Pager::default()
    };
    // var buffer: [4096]u8 = undefined;
    // const writer = pager.writer(&buffer);
    let mut writer = pager.writer();

    // if (text) |t| {
    if let Some(t) = text {
        // try writer.writeAll(t);
        writer.write_all(t)?;
        // try writer.writeAll("\n");
        writer.write_all(b"\n")?;
    } else {
        // try writer.writeAll("Unknown: '");
        writer.write_all(b"Unknown: '")?;
        // try writer.writeAll(name);
        writer.write_all(&name)?;
        // try writer.writeAll("'.\n");
        writer.write_all(b"'.\n")?;
        // try writer.flush();
        writer.flush()?;
        // return 1;
        return Ok(1);
    }

    // try writer.flush();
    writer.flush()?;
    // return 0;
    Ok(0)
}

// fn explainOption(name: []const u8) ?[]const u8 {
fn explain_option(name: &[u8]) -> Option<&'static [u8]> {
    // const key = std.meta.stringToEnum(ConfigKey, name) orelse return null;
    let _key = ConfigKey::string_to_enum(name)?;
    // return switch (key) {
    //     inline else => |tag| {
    //         const field_name = @tagName(tag);
    //         return if (@hasDecl(help_strings.Config, field_name))
    //             @field(help_strings.Config, field_name)
    //         else
    //             null;
    //     },
    // };
    //
    // PORT NOTE (DEFERRED SEAM, ADV-FIX 3): `help_strings.Config` is the
    // build-generated per-key doc table; it is not yet produced in rustty (same
    // deferral as formatter_file.rs / helpgen_actions `NoHelpStrings`). With no
    // generated table, `@hasDecl(help_strings.Config, field_name)` is false for
    // every valid key, so this returns `None`. The config-key VALIDATION above
    // (`string_to_enum`) is preserved; only the help-text source is deferred. When
    // the help_strings codegen lands, this returns the doc text and the "explain"
    // test is un-deferred.
    None
}

// fn explainKeybind(name: []const u8) ?[]const u8 {
fn explain_keybind(name: &[u8]) -> Option<&'static [u8]> {
    // const tag = std.meta.stringToEnum(std.meta.Tag(KeybindAction), name) orelse return null;
    //
    // PORT NOTE: `std.meta.Tag(KeybindAction)` is the keybind-action union's tag
    // enum; `stringToEnum` over it is modeled by membership in the union's field
    // names (`KeybindAction::FIELD_NAMES`, the single-source-of-truth order).
    if !KeybindAction::FIELD_NAMES
        .iter()
        .any(|&n| n.as_bytes() == name)
    {
        return None;
    }
    // return switch (tag) {
    //     inline else => |t| {
    //         const field_name = @tagName(t);
    //         return if (@hasDecl(help_strings.KeybindAction, field_name))
    //             @field(help_strings.KeybindAction, field_name)
    //         else
    //             null;
    //     },
    // };
    //
    // PORT NOTE (DEFERRED SEAM, ADV-FIX 3): `help_strings.KeybindAction` is the
    // build-generated per-action doc table — deferred (as above), so this returns
    // `None` for every valid action-tag until the codegen lands.
    None
}

// test "explain"  — DEFERRED (ADV-FIX 3; DEFERRALS.md "cli C2 — explain_config").
//
// The assertions require the build-generated `help_strings.Config` /
// `help_strings.KeybindAction` doc tables, which are a deferred seam
// (`explain_option`/`explain_keybind` return `None` for every name until that
// codegen lands), so `explainOption("font-size") != null` and
// `explainKeybind("copy_to_clipboard") != null` cannot pass yet. The body is
// preserved verbatim (R1) below and is restored as a live Rust test (add the
// `# [test]` attribute) — and the `apc_deferred=1` in check_test_counts removed —
// when the help_strings generator lands. Do NOT convert it to a passing/weakened
// test (R5). (The test attribute is written spaced below so check_test_counts'
// test-attribute grep does not count this deferred, non-live body.)
//
// # [test]
// fn explain() {
//     // Config options
//     // try std.testing.expect(explainOption("font-size") != null);
//     assert!(explain_option(b"font-size").is_some());
//     // try std.testing.expect(explainOption("copy_to_clipboard") == null);
//     assert!(explain_option(b"copy_to_clipboard").is_none());
//     // try std.testing.expect(explainOption("unknown-option") == null);
//     assert!(explain_option(b"unknown-option").is_none());
//
//     // Keybind actions
//     // try std.testing.expect(explainKeybind("copy_to_clipboard") != null);
//     assert!(explain_keybind(b"copy_to_clipboard").is_some());
//     // try std.testing.expect(explainKeybind("font-size") == null);
//     assert!(explain_keybind(b"font-size").is_none());
//     // try std.testing.expect(explainKeybind("unknown_keybind") == null);
//     assert!(explain_keybind(b"unknown_keybind").is_none());
// }
