//! rustty-cli::toggle_quick_terminal
//!
//! PORTS: ghostty src/cli/toggle_quick_terminal.zig (0t) — the
//! `+toggle-quick-terminal` action: use native platform IPC to toggle the quick
//! terminal in a running instance. Over the headless `none` backend,
//! `apprt.App.performIpc` returns `false`, so this faithfully prints
//! "+toggle-quick-terminal is not supported on this platform." to stderr and
//! exits 1 (CLI_BLUEPRINT.md §1 / ADV-FIX 4 / Risk 4 — correct, NOT a stub). Real
//! IPC delivery arrives with a live GTK/embedded backend (Phase-3). 0 tests.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const Action = @import("../cli.zig").ghostty.Action;
use crate::ghostty::{Action, Error};
// const apprt = @import("../apprt.zig");
//   PORT NOTE: the reference `apprt.App` is the comptime-selected
//   `apprt.runtime.App`; for the headless `rustty` CLI build that is
//   `none.App`, whose `perform_ipc` always returns `Ok(false)`. The ipc value
//   types come from `rustty-apprt-api` (rustty-apprt does not re-export them).
use allocator_api2::alloc::Global;
use rustty_apprt::none;
use rustty_apprt_api::ipc;

use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// If set, connect to a custom instance of Ghostty.
    // class: ?[:0]const u8 = null,
    //   R3: `[:0]const u8` -> owned raw bytes; R2: the "Ghostty" brand in the doc.
    pub class: Option<Vec<u8>>,
}

impl Options {
    // pub fn deinit(self: *Options) void { self.* = undefined; }
    pub fn deinit(&mut self) {
        let _ = self;
    }

    // /// Enables "-h" and "--help" to work.
    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl rustty_args::args::Target for Options {
    // class: ?[:0]const u8 — the single parseable flag.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            // class: ?[:0]const u8
            b"class" => {
                self.class = Some(rustty_args::args::parse_field_string(value)?);
                Ok(())
            }
            // return error.InvalidField;
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), rustty_args::args::ParseError>> {
        let _ = Action::Help;
        Some(Err(rustty_args::args::Error::ActionHelpRequested))
    }
}

/// The `+toggle-quick-terminal` command will use native platform IPC to toggle
/// the quick terminal in a running instance of rustty.
///
/// If the `--class` flag is not set, the command will try and connect to the
/// default running rustty instance. Otherwise it will contact a rustty
/// instance configured with the given `class`.
///
/// On GTK, D-Bus activation must be properly configured. rustty does not need
/// to be running, as D-Bus will handle launching a new instance if it is not
/// already running.
///
/// Only supported on GTK.
///
/// Flags:
///
///   * `--class=<class>`: If set, connect to a custom instance of rustty.
///     The class must be a valid GTK application ID.
///
/// Available since: 1.4.0
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var buf: [256]u8 = undefined;
    // var stderr_writer = std.fs.File.stderr().writer(&buf);
    // const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // if (apprt.App.performIpc(
    //     alloc,
    //     .detect,
    //     .toggle_quick_terminal,
    //     {},
    // ) catch |err| switch (err) {
    //     error.IPCFailed => { return 1; },
    //     else => { stderr.print("Sending the IPC failed: {}\n", .{err}); return 1; },
    // }) return 0;
    match none::App::perform_ipc(
        Global,
        ipc::Target::Detect,
        ipc::Action::ToggleQuickTerminal,
    ) {
        // if (apprt.App.performIpc(...)) return 0;
        Ok(delivered) => {
            if delivered {
                return Ok(0);
            }
        }
        // error.IPCFailed => { return 1; }
        // else => { stderr.print("Sending the IPC failed: {}\n", .{err}); return 1; }
        //   PORT NOTE: `ipc::Errors` has the single variant `IPCFailed`, so the
        //   reference's `else` arm collapses into this one; both reference branches
        //   `return 1` (the `else` would additionally print "Sending the IPC
        //   failed: {}", unreachable for the current single-variant error set).
        Err(ipc::Errors::IPCFailed) => return Ok(1),
    }

    // try stderr.print("+toggle-quick-terminal is not supported on this platform.\n", .{});
    writeln!(
        stderr,
        "+toggle-quick-terminal is not supported on this platform."
    )?;
    // return 1;
    Ok(1)
}
