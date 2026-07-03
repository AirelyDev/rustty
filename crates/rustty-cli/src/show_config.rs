//! rustty-cli::show_config
//!
//! PORTS: ghostty src/cli/show_config.zig (0t) — the `+show-config` action.

// const std = @import("std");
// const args = @import("args.zig");
use rustty_args::args;
// const Allocator = std.mem.Allocator;
// const Action = @import("ghostty.zig").Action;
use crate::ghostty::{Action, Error};
// const configpkg = @import("../config.zig");
// const Config = configpkg.Config;
use rustty_config::{Config, FileFormatter};
// const Pager = @import("Pager.zig");
use crate::pager::Pager;

// pub const Options = struct {
pub struct Options {
    /// If true, do not load the user configuration, only load the defaults.
    // default: bool = false,
    pub default: bool,

    /// Only show the options that have been changed from the default.
    /// This has no effect if `--default` is specified.
    // @"changes-only": bool = true,
    pub changes_only: bool,

    /// If true print the documentation above each option as a comment,
    /// if available.
    // docs: bool = false,
    pub docs: bool,

    /// Disable automatic paging of output.
    // @"no-pager": bool = false,
    pub no_pager: bool,
}

impl Default for Options {
    /// The reference struct-literal defaults (`.{}`): note `changes-only`
    /// defaults to `true` (not `false`), so this is a hand-written `Default`.
    fn default() -> Options {
        Options {
            default: false,
            changes_only: true,
            docs: false,
            no_pager: false,
        }
    }
}

impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables `-h` and `--help` to work.
    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the four non-`_` bool flags.
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
            // @"changes-only": bool
            b"changes-only" => {
                self.changes_only = args::parse_field_bool(value)?;
                Ok(())
            }
            // docs: bool
            b"docs" => {
                self.docs = args::parse_field_bool(value)?;
                Ok(())
            }
            // @"no-pager": bool
            b"no-pager" => {
                self.no_pager = args::parse_field_bool(value)?;
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
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `show-config` command shows the current configuration in a valid rustty
/// configuration file format.
///
/// When executed without any arguments this will output the current
/// configuration that is different from the default configuration. If you're
/// using the default configuration this will output nothing.
///
/// If you are a new user and want to see all available options with
/// documentation, run `rustty +show-config --default --docs`.
///
/// The output is not in any specific order, but the order should be consistent
/// between runs. The output is not guaranteed to be exactly match the input
/// configuration files, but it will result in the same behavior. Comments,
/// whitespace, and other formatting is not preserved from user configuration
/// files.
///
/// Flags:
///
///   * `--default`: Show the default configuration instead of loading
///     the user configuration.
///
///   * `--changes-only`: Only show the options that have been changed
///     from the default. This has no effect if `--default` is specified.
///
///   * `--docs`: Print the documentation above each option as a comment,
///     This is very noisy but is very useful to learn about available
///     options, especially paired with `--default`.
///
///   * `--no-pager`: Disable automatic paging of output.
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

    // const configfmt: configpkg.FileFormatter = .{
    //     .alloc = alloc,
    //     .config = &config,
    //     .changed = !opts.default and opts.@"changes-only",
    //     .docs = opts.docs,
    // };
    //   PORT NOTE: FileFormatter's `--docs` mode reads the build-generated
    //   `help_strings.Config` table (`config_help`, now wired in formatter_file.rs),
    //   so `+show-config --docs` prints each option's doc as `# ` comments.
    let configfmt = FileFormatter {
        config: &config,
        changed: !opts.default && opts.changes_only,
        docs: opts.docs,
    };

    // var pager: Pager = if (!opts.@"no-pager") .init(alloc) else .{};
    // defer pager.deinit();
    let mut pager: Pager = if !opts.no_pager {
        Pager::init()
    } else {
        Pager::default()
    };

    {
        // var buffer: [4096]u8 = undefined;
        // const writer = pager.writer(&buffer);
        //   (scoped so the writer's borrow of `pager` ends before `deinit`.)
        let mut writer = pager.writer();
        // try configfmt.format(writer);
        configfmt.format(&mut writer)?;
        // try writer.flush();
        writer.flush()?;
    }

    pager.deinit();
    opts.deinit();

    // return 0;
    Ok(0)
}
