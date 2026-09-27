//! rustty-cli::list_actions
//!
//! PORTS: ghostty src/cli/list_actions.zig (0t) — the `+list-actions` action.

// const std = @import("std");
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const Allocator = std.mem.Allocator;
// const helpgen_actions = @import("../input/helpgen_actions.zig");
use rustty_input::helpgen_actions::{self, Format, RealHelpStrings};

use crate::pager::IoWriter;
use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// If `true`, print out documentation about the action associated with the
    /// keybinds.
    // docs: bool = false,
    pub docs: bool,
}

impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables `-h` and `--help` to work.
    // pub fn help(self: Options) !void {
    //     _ = self;
    //     return Action.help_error;
    // }
    //
    // PORT NOTE: the reference's `help` decl is realized via the
    // `args::Target::help`/`has_help_decl` hooks below (`args.parse` invokes them
    // for `--help`/`-h`). It returns the `ActionHelpRequested` sentinel that
    // `Action::run` catches to print help. `Action` is named here only for this
    // reference (`Action::help_error`).
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the single non-`_` field is `docs`.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // docs: bool
            b"docs" => {
                self.docs = args::parse_field_bool(value)?;
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

/// The `list-actions` command is used to list all the available keybind
/// actions for Rustty. These are distinct from the CLI Actions which can
/// be listed via `+help`
///
/// Flags:
///
///   * `--docs`: will print out the documentation for each action.
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

    // var stdout: std.fs.File = .stdout();
    // var buffer: [4096]u8 = undefined;
    // var stdout_writer = stdout.writer(&buffer);
    let mut stdout = IoWriter(std::io::BufWriter::with_capacity(
        4096,
        crate::io_file::stdout(),
    ));

    // try helpgen_actions.generate(
    //     &stdout_writer.interface,
    //     .plaintext,
    //     opts.docs,
    //     std.heap.page_allocator,
    // );
    //
    // PORT NOTE: `help_strings` (the build-generated per-action doc table) is
    // `RealHelpStrings`, delegating to the generated `keybind_help` — so `--docs`
    // prints each action's real doc text. The `page_allocator` arg is dropped
    // (the allocating writer owns its buffer).
    helpgen_actions::generate(&mut stdout, Format::Plaintext, opts.docs, &RealHelpStrings)?;
    // try stdout_writer.interface.flush();
    stdout.0.flush()?;

    // defer opts.deinit();  (no-op; runs here at end of scope)
    opts.deinit();

    // return 0;
    Ok(0)
}
