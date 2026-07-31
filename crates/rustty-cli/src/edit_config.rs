//! rustty-cli::edit_config
//!
//! PORTS: ghostty src/cli/edit_config.zig (0t) — the `+edit-config` action:
//! open the user config file in `$VISUAL`/`$EDITOR` via a `/bin/sh -c` exec.
//! Backed by rustty-config (`Config.load`, `preferredDefaultFilePath`) and
//! rustty-os (`getenv`, `ShellEscapeWriter`, the POSIX exec). Byte-faithful (R1);
//! the editor / path bytes stay raw (R3). R2: the `ghostty +edit-config` binary
//! name, the "Ghostty configuration" brand, and the illustrative config paths are
//! retargeted to the Rustty product identity via rustty-identity. 0 tests.

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const args = @import("args.zig");
use rustty_args::args;
// const Allocator = std.mem.Allocator;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const configpkg = @import("../config.zig");
// const Config = configpkg.Config;
use rustty_config::file_load::preferred_default_file_path;
use rustty_config::Config;
// const internal_os = @import("../os/main.zig");
use rustty_os::string_encoding::{AllocatingWriter, Writer};
use rustty_os::{getenv, GetEnvResult, ShellEscapeWriter};

use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {}

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

/// The `edit-config` command opens the Rustty configuration file in the
/// editor specified by the `$VISUAL` or `$EDITOR` environment variables.
///
/// IMPORTANT: This command will not reload the configuration after
/// editing. You will need to manually reload the configuration using the
/// application menu, configured keybind, or by restarting Rustty. We
/// plan to auto-reload in the future, but Rustty isn't capable of
/// this yet.
///
/// The filepath opened is the default user-specific configuration
/// file, which is typically located at `$XDG_CONFIG_HOME/rustty/config.rustty`.
/// On macOS, this may also be located at
/// `~/Library/Application Support/io.airely.rustty/config.rustty`.
/// On macOS, whichever path exists and is non-empty will be prioritized,
/// prioritizing the Application Support directory if neither are
/// non-empty.
///
/// This command prefers the `$VISUAL` environment variable over `$EDITOR`,
/// if both are set. If neither are set, it will print an error
/// and exit.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // Implementation note (by @mitchellh): I do proper memory cleanup
    // throughout this command, even though we plan on doing `exec`.
    // I do this out of good hygiene in case we ever change this to
    // not using `exec` anymore and because this command isn't performance
    // critical where setting up the defer cleanup is a problem.

    // var buffer: [1024]u8 = undefined;
    // var stderr_writer = std.fs.File.stderr().writer(&buffer);
    // const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try args.argsIterator(alloc);
        // defer iter.deinit();
        // try args.parse(Options, alloc, &opts, &iter);
        let mut iter = args::args_iterator()?;
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // const result = runInner(alloc, stderr);
    let result = run_inner(&mut stderr);
    // Flushing *shouldn't* fail but...
    // stderr.flush() catch {};
    let _ = stderr.flush();

    opts.deinit();

    // return result;
    result
}

// fn runInner(alloc: Allocator, stderr: *std.Io.Writer) !u8 {
fn run_inner<We: Write>(stderr: &mut We) -> Result<u8, Error> {
    // We load the configuration once because that will write our
    // default configuration files to disk. We don't use the config.
    // var config = try Config.load(alloc);
    // defer config.deinit();
    //   PORT NOTE: loaded only for its disk side-effect; `_config` drops at scope
    //   end (the reference's `defer config.deinit()`), and Rustty's
    //   `Config::deinit` resets the arena flag only.
    let _config = Config::load()?;

    // Find the preferred path.
    // const path = try configpkg.preferredDefaultFilePath(alloc);
    // defer alloc.free(path);
    let path = preferred_default_file_path()?;

    // We don't currently support Windows because we use the exec syscall.
    // if (comptime builtin.os.tag == .windows) {
    #[cfg(windows)]
    {
        // try stderr.print(
        //     \\The `ghostty +edit-config` command is not supported on Windows.
        //     \\Please edit the configuration file manually at the following path:
        //     \\
        //     \\{s}
        //     \\
        // , .{path});
        //   R2: the `ghostty` binary name -> the Rustty product name.
        write!(
            stderr,
            "The `{app} +edit-config` command is not supported on Windows.\n\
             Please edit the configuration file manually at the following path:\n\
             \n\
             {path}\n",
            app = rustty_identity::APP_NAME,
        )?;
        // return 1;
        return Ok(1);
    }

    // Get our editor
    // const get_env_: ?internal_os.GetEnvResult = env: {
    let get_env_: Option<GetEnvResult> = 'env: {
        // VISUAL vs. EDITOR: https://unix.stackexchange.com/questions/4859/visual-vs-editor-what-s-the-difference
        // if (try internal_os.getenv(alloc, "VISUAL")) |v| {
        //     if (v.value.len > 0) break :env v;
        //     v.deinit(alloc);
        // }
        if let Some(v) = getenv(b"VISUAL")? {
            if !v.value.is_empty() {
                break 'env Some(v);
            }
            v.deinit();
        }

        // if (try internal_os.getenv(alloc, "EDITOR")) |v| {
        //     if (v.value.len > 0) break :env v;
        //     v.deinit(alloc);
        // }
        if let Some(v) = getenv(b"EDITOR")? {
            if !v.value.is_empty() {
                break 'env Some(v);
            }
            v.deinit();
        }

        // break :env null;
        None
    };
    // defer if (get_env_) |v| v.deinit(alloc); — `get_env_` drops at scope end.
    // const editor: []const u8 = if (get_env_) |v| v.value else "";
    let editor: &[u8] = match &get_env_ {
        Some(v) => v.value.as_slice(),
        None => b"",
    };

    // If we don't have `$EDITOR` set then we can't do anything
    // but we can still print a helpful message.
    // if (editor.len == 0) {
    if editor.is_empty() {
        // try stderr.print(
        //     \\The $EDITOR or $VISUAL environment variable is not set or is empty.
        //     \\This environment variable is required to edit the Ghostty configuration
        //     \\via this CLI command.
        //     ...
        // , .{});
        //   R2: "Ghostty configuration" -> the Rustty product name
        //   (APP_NAME_TITLECASE; the oracle prints the capitalized product name).
        write!(
            stderr,
            "The $EDITOR or $VISUAL environment variable is not set or is empty.\n\
             This environment variable is required to edit the {app} configuration\n\
             via this CLI command.\n\
             \n\
             Please set the environment variable to your preferred terminal\n\
             text editor and try again.\n\
             \n\
             If you prefer to edit the configuration file another way,\n\
             you can find the configuration file at the following path:\n\
             \n\
             \n",
            app = rustty_identity::APP_NAME_TITLECASE,
        )?;

        // Output the path using the OSC8 sequence so that it is linked.
        // try stderr.print(
        //     "\x1b]8;;file://{s}\x1b\\{s}\x1b]8;;\x1b\\\n",
        //     .{ path, path },
        // );
        writeln!(stderr, "\x1b]8;;file://{path}\x1b\\{path}\x1b]8;;\x1b\\")?;

        // return 1;
        return Ok(1);
    }

    // const command = command: {
    //     var buffer: std.io.Writer.Allocating = .init(alloc);
    //     defer buffer.deinit();
    //     const writer = &buffer.writer;
    //     try writer.writeAll(editor);
    //     try writer.writeByte(' ');
    //     {
    //         var sh: internal_os.ShellEscapeWriter = .init(writer);
    //         try sh.writer.writeAll(path);
    //         try sh.writer.flush();
    //     }
    //     try writer.flush();
    //     break :command try buffer.toOwnedSliceSentinel(0);
    // };
    // defer alloc.free(command);
    let command: Vec<u8> = {
        let mut buffer = AllocatingWriter::init();
        buffer.writeAll(editor)?;
        buffer.writeByte(b' ')?;
        {
            let mut sh = ShellEscapeWriter::init(&mut buffer);
            sh.writeAll(path.as_bytes())?;
            // sh.writer.flush() — the allocating writer has no buffered state.
        }
        // writer.flush() — no-op for the allocating writer.
        // break :command try buffer.toOwnedSliceSentinel(0);
        //   PORT NOTE: `toOwnedSliceSentinel(0)` yields a NUL-terminated `[:0]const
        //   u8` for `execvpeZ`; Rust's `Command::arg` re-appends the C NUL, so the
        //   logical (NUL-free) bytes are kept here.
        buffer.written().to_vec()
    };

    // We require libc because we want to use std.c.environ for envp
    // and not have to build that ourselves. We can remove this
    // limitation later but Ghostty already heavily requires libc
    // so this is not a big deal.
    // comptime assert(builtin.link_libc);
    //   PORT NOTE: Rust always links libc; the compile-time assertion holds
    //   trivially and needs no runtime check.

    // const err = std.posix.execvpeZ(
    //     "/bin/sh",
    //     &.{ "/bin/sh", "-c", command },
    //     std.c.environ,
    // );
    //   PORT NOTE: `execvpeZ` replaces the process image using the inherited
    //   environment (`std.c.environ`); Rust's `CommandExt::exec` (`execvp` with the
    //   inherited env) is the faithful map — it returns only on failure.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(as_os_str(&command))
            .exec();

        // If we reached this point then exec failed.
        // try stderr.print(
        //     \\Failed to execute the editor. Error code={}.
        //     ...
        //     \\Editor: {s}
        //     \\Path: {s}
        //     \\
        // , .{ err, editor, path });
        writeln!(
            stderr,
            "Failed to execute the editor. Error code={err}.\n\
             \n\
             This is usually due to the executable path not existing, invalid\n\
             permissions, or the shell environment not being set up\n\
             correctly.\n\
             \n\
             Editor: {editor}\n\
             Path: {path}",
            editor = show(editor),
        )?;
        // return 1;
        Ok(1)
    }
    #[cfg(not(unix))]
    {
        // Non-POSIX, non-Windows targets have no `exec` syscall; the reference
        // `comptime assert(builtin.link_libc)` + `execvpeZ` are POSIX-only.
        let _ = &command;
        unreachable!("edit-config requires a POSIX exec syscall")
    }
}

/// Interpret command bytes as an `OsStr` for `std::process::Command` (R3: argv
/// are byte strings). `{s}`-formatted diagnostics use `show` (lossy) instead.
#[cfg(unix)]
fn as_os_str(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

/// Lossy view of a byte string for a `{s}` diagnostic (never on the wire).
#[cfg(unix)]
fn show(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}
