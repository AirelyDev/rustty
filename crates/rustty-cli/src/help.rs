//! rustty-cli::help
//!
//! PORTS: ghostty src/cli/help.zig — the `+help` / `-h` / `--help` action.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::ghostty::{Action, Error};

// Note that this options struct doesn't implement the `help` decl like other
// actions. That is because the help command is special and wants to handle its
// own logic around help detection.
// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// This must be registered so that it isn't an error to pass `--help`
    // help: bool = false,  (the reference field default; `derive(Default)` == false)
    pub help: bool,
}

impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }
}

// PORT NOTE: `Options` deliberately does NOT implement the `help` decl (the
// reference comment above) — the `--help` flag just registers `help: bool` so it
// isn't an "unknown field" error. The `Target::parse_into_field` below performs
// the reference's per-field reflection for that single field.
impl args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            b"help" => {
                self.help = args::parse_field_bool(value)?;
                Ok(())
            }
            _ => Err(args::Error::InvalidField),
        }
    }
}

/// The `help` command shows general help about rustty. Recognized as either
/// `-h`, `--help`, or like other actions `+help`.
///
/// You can also specify `--help` or `-h` along with any action such as
/// `+list-themes` to see help for a specific action.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    use std::io::Write;

    // var opts: Options = .{};
    let mut opts = Options::default();
    // defer opts.deinit();

    {
        // var iter = try args.argsIterator(alloc);
        let mut iter = args::args_iterator()?;
        // defer iter.deinit();
        // try args.parse(Options, alloc, &opts, &iter);
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }
    opts.deinit();

    // var buffer: [2048]u8 = undefined; ... const stdout = &stdout_writer.interface;
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());

    // try stdout.writeAll(
    //     \\Usage: ghostty [+action] [options]
    //     ...
    //     \\Available actions:
    //     \\
    //     \\
    // );
    //   R2: user-facing `ghostty`/`Ghostty`/`Ghostty.app` -> the rustty product
    //   name (rustty-identity APP_NAME). These are user-facing product strings.
    //   The Zig multiline literal joins its `\\` lines with `\n` and has no
    //   trailing newline, so this block ends with `Available actions:\n\n`.
    let app = rustty_identity::APP_NAME;
    let head = format!(
        "Usage: {app} [+action] [options]\n\
         \n\
         Run the {app} terminal emulator or a specific helper action.\n\
         \n\
         If no `+action` is specified, run the {app} terminal emulator.\n\
         All configuration keys are available as command line options.\n\
         To specify a configuration key, use the `--<key>=<value>` syntax\n\
         where key and value are the same format you'd put into a configuration\n\
         file. For example, `--font-size=12` or `--font-family=\"Fira Code\"`.\n\
         \n\
         To see a list of all available configuration options, please see\n\
         the `src/config/Config.zig` file. A future update will allow seeing\n\
         the list of configuration options from the command line.\n\
         \n\
         A special command line argument `-e <command>` can be used to run\n\
         the specific command inside the terminal emulator. For example,\n\
         `{app} -e top` will run the `top` command inside the terminal.\n\
         \n\
         On macOS, launching the terminal emulator from the CLI is not\n\
         supported and only actions are supported. Use `open -na {app}.app`\n\
         instead, or `open -na {app}.app --args --foo=bar --baz=quz` to pass\n\
         arguments.\n\
         \n\
         Available actions:\n\
         \n"
    );
    stdout.write_all(head.as_bytes())?;

    // inline for (@typeInfo(Action).@"enum".fields) |field| {
    //     try stdout.print("  +{s}\n", .{field.name});
    // }
    for action in Action::ALL {
        writeln!(stdout, "  +{}", action.tag_name())?;
    }

    // try stdout.writeAll(
    //     \\
    //     \\Specify `+<action> --help` to see the help for a specific action,
    //     \\where `<action>` is one of actions listed above.
    //     \\
    // );
    stdout.write_all(
        b"\n\
          Specify `+<action> --help` to see the help for a specific action,\n\
          where `<action>` is one of actions listed above.\n",
    )?;
    // try stdout.flush();
    stdout.flush()?;

    // return 0;
    Ok(0)
}
