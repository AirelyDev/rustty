//! rustty-cli::validate_config
//!
//! PORTS: ghostty src/cli/validate_config.zig (0t) — the `+validate-config` action.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const Config = @import("../config.zig").Config;
use rustty_config::Config;

use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// The path of the config file to validate. If this isn't specified,
    /// then the default config file paths will be validated.
    // @"config-file": ?[:0]const u8 = null,
    pub config_file: Option<Vec<u8>>,
}

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
    // inline for (info.@"struct".fields) — the single non-`_` field is `config-file`.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // @"config-file": ?[:0]const u8
            b"config-file" => {
                // For optional fields, an empty (set-but-empty) value resets the
                // value to the default (null) — the reference `parseIntoField`
                // empty-value-default branch.
                if let Some(v) = value {
                    if v.is_empty() {
                        self.config_file = None;
                        return Ok(());
                    }
                }
                self.config_file = Some(args::parse_field_sentinel_string(value)?);
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

/// The `validate-config` command is used to validate a rustty config file.
///
/// When executed without any arguments, this will load the config from the default
/// location.
///
/// Flags:
///
///   * `--config-file`: can be passed to validate a specific target config file in
///     a non-default location
// pub fn run(alloc: std.mem.Allocator) !u8 {
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

    // var buffer: [1024]u8 = undefined;
    // var stdout_writer = std.fs.File.stdout().writer(&buffer);
    // const stdout = &stdout_writer.interface;
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    // const result = runInner(alloc, opts, stdout);
    let result = run_inner(&opts, &mut stdout);
    // try stdout_writer.end();
    stdout.flush()?;

    opts.deinit();

    // return result;
    result
}

// fn runInner(alloc, opts, stdout: *std.Io.Writer) !u8 {
fn run_inner<W: Write>(opts: &Options, stdout: &mut W) -> Result<u8, Error> {
    // var cfg = try Config.default(alloc);
    // defer cfg.deinit();
    //   PORT NOTE: `Config::deinit` only resets the arena flag (each value owns
    //   its bytes); `cfg` is dropped at scope end, so no explicit `deinit` is
    //   threaded through the early `return Ok(1)` path.
    let mut cfg = Config::default_config();

    // If a config path is passed, validate it, otherwise validate default configs
    // if (opts.@"config-file") |config_path| {
    if let Some(config_path) = &opts.config_file {
        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // const abs_path = try std.fs.cwd().realpath(config_path, &buf);
        //   `std.fs.cwd().realpath` -> `std::fs::canonicalize` (resolves relative
        //   to cwd + symlinks, returns an absolute path).
        let abs_path = std::fs::canonicalize(path_from_bytes(config_path))?;
        // try cfg.loadFile(alloc, abs_path);
        cfg.load_file(&path_bytes(&abs_path))?;
        // try cfg.loadRecursiveFiles(alloc);
        cfg.load_recursive_files()?;
    } else {
        // cfg = try Config.load(alloc);
        cfg = Config::load()?;
    }

    // try cfg.finalize();
    cfg.finalize()?;

    // if (cfg._diagnostics.items().len > 0) {
    if !cfg._diagnostics.items().is_empty() {
        // for (cfg._diagnostics.items()) |diag| {
        for diag in cfg._diagnostics.items() {
            // try stdout.print("{f}\n", .{diag});
            //   `{f}` -> the `Diagnostic::format` custom writer.
            let mut msg = Vec::new();
            diag.format(&mut msg);
            stdout.write_all(&msg)?;
            stdout.write_all(b"\n")?;
        }
        // return 1;
        return Ok(1);
    }

    // return 0;
    Ok(0)
}

/// Build a filesystem `Path` from raw config-value bytes (the CLI `config-file`
/// value is a byte string, R3).
fn path_from_bytes(b: &[u8]) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b))
    }
    #[cfg(not(unix))]
    {
        std::path::PathBuf::from(String::from_utf8_lossy(b).into_owned())
    }
}

/// The raw bytes of a resolved `Path`, for `Config::load_file` (which takes a
/// byte-slice path, R3).
fn path_bytes(p: &std::path::Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        p.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        p.to_string_lossy().into_owned().into_bytes()
    }
}
