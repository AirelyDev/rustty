//! PORT NOTE: `synthetic/cli.zig`.

pub mod ascii;
pub mod kitty;
pub mod osc;
pub mod utf8;

use crate::io::{WriteFailed, Writer};
use std::io::Write;

/// The available actions for the CLI. This is the list of available
/// synthetic generators. View docs for each individual one in the
/// predictably named files under `cli/`.
///
/// PORT NOTE: `pub const Action = enum { ascii, osc, utf8 }`. The `Struct(action)`
/// comptime mapping (action → impl module) is expressed in Rust as the per-arm
/// dispatch in `main_action` below; each impl module exposes `Options`,
/// `create`, `destroy`, and `run`.
pub enum Action {
    Ascii,
    Osc,
    Utf8,
    // kitty (f6f79acce, WS10 T17)
    Kitty,
}

impl Action {
    /// `std.meta.stringToEnum(Action, name)` — the kebab CLI tokens are the enum
    /// tag names.
    pub fn from_name(name: &[u8]) -> Option<Action> {
        match name {
            b"ascii" => Some(Action::Ascii),
            b"osc" => Some(Action::Osc),
            b"utf8" => Some(Action::Utf8),
            b"kitty" => Some(Action::Kitty),
            _ => None,
        }
    }
}

// PORT NOTE: `pub fn Struct(comptime action: Action) type` — the action → impl
// module mapping (`.ascii => cli/Ascii`, `.osc => cli/Osc`, `.utf8 => cli/Utf8`):
//
// Returns the struct associated with the action. The struct
// should have a few decls:
//
//   - `const Options`: The CLI options for the action.
//   - `fn create`: Create a new instance of the action from options.
//   - `fn destroy`: Destroy the instance of the action.
//
// See TerminalStream for an example.

/// The error set of `main`/`mainAction`/`mainActionImpl` (Zig's inferred `!void`).
#[derive(Debug)]
pub enum Error {
    // return error.NoAction;
    NoAction,
    // std.meta.stringToEnum(...) orelse return DetectError.InvalidAction;
    InvalidAction,
    // if (pending != null) return DetectError.MultipleActions;
    MultipleActions,
    // try cli.args.parse(...)
    Parse(rustty_args::args::ParseError),
    // try Utf8.create(...) → error.InvalidValue
    InvalidValue,
    // create/run WriteFailed (unreachable in practice — the run loops swallow it).
    WriteFailed,
}

impl From<rustty_args::args::ParseError> for Error {
    fn from(e: rustty_args::args::ParseError) -> Error {
        Error::Parse(e)
    }
}

/// PORT NOTE: `pub const Args = union(enum) { cli, string: []const u8 }`.
pub enum Args<'a> {
    /// The arguments passed to the CLI via argc/argv.
    Cli,

    /// Simple string arguments, parsed via std.process.ArgIteratorGeneral.
    String(&'a [u8]),
}

/// PORT NOTE (binary-entry seam, per the placeholder-leaf-doctrine): the reference
/// `cli.action.detectArgs(Action, alloc)` is the SHARED generic `+action` detection
/// that lives in `rustty-cli` (a heavy crate — core/apprt/config — that
/// rustty-synthetic deliberately does NOT depend on to stay lean and acyclic).
/// Its detect logic (find the single `+name` arg, `stringToEnum`) is replicated
/// locally over `std::env::args_os` (mirroring `rustty-bench`'s `cli.rs`). The
/// synthetic `Action` declares no `detectSpecialCase`, so the special-case arm of
/// `detectIter` is elided.
///
/// An entrypoint for the synthetic generator CLI.
// pub fn main() !void {
//     const alloc = std.heap.c_allocator;
//     const action_ = try cli.action.detectArgs(Action, alloc);
//     const action = action_ orelse return error.NoAction;
//     try mainAction(alloc, action, .cli);
// }
pub fn main() -> Result<(), Error> {
    let action_ = detect_args()?;
    // const action = action_ orelse return error.NoAction;
    let action = action_.ok_or(Error::NoAction)?;
    // try mainAction(alloc, action, .cli);
    main_action(action, Args::Cli)
}

/// Local replica of `cli.action.detectArgs(Action, alloc)` (see `main`'s note).
fn detect_args() -> Result<Option<Action>, Error> {
    // var pending: ?E = null;
    let mut pending: Option<Action> = None;
    // while (iter.next()) |arg| {
    for arg in std::env::args_os() {
        // std.process.ArgIterator yields bytes; on Unix the OS bytes are exact.
        #[cfg(unix)]
        let bytes: Vec<u8> = {
            use std::os::unix::ffi::OsStrExt;
            arg.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let bytes: Vec<u8> = arg.to_string_lossy().into_owned().into_bytes();

        // Commands must start with "+"
        // if (arg.len == 0 or arg[0] != '+') continue;
        if bytes.is_empty() || bytes[0] != b'+' {
            continue;
        }
        // if (pending != null) return DetectError.MultipleActions;
        if pending.is_some() {
            return Err(Error::MultipleActions);
        }
        // pending = std.meta.stringToEnum(E, arg[1..]) orelse return DetectError.InvalidAction;
        pending = Some(Action::from_name(&bytes[1..]).ok_or(Error::InvalidAction)?);
    }
    // return fallback (there is none for the synthetic Action) => pending.
    Ok(pending)
}

/// PORT NOTE: `pub fn mainAction(alloc, action, args)` — dispatches on the action to
/// the concrete impl. The Zig `inline else` monomorphisation over `Action.Struct`
/// is named explicitly per arm here.
// pub fn mainAction(alloc: Allocator, action: Action, args: Args) !void {
//     switch (action) {
//         inline else => |comptime_action| {
//             const Impl = Action.Struct(comptime_action);
//             try mainActionImpl(Impl, alloc, args);
//         },
//     }
// }
pub fn main_action(action: Action, args: Args) -> Result<(), Error> {
    match action {
        Action::Ascii => main_action_impl_ascii(args),
        Action::Osc => main_action_impl_osc(args),
        Action::Utf8 => main_action_impl_utf8(args),
        Action::Kitty => main_action_impl_kitty(args),
    }
}

/// PORT NOTE: `std.fs.File.stdout().writer(&buffer)` with a 2048-byte buffer.
/// A buffered writer over stdout: accumulates up to 2048 bytes, auto-flushing to
/// the fd when full (this is the buffered `std.Io.Writer` behaviour — the buffer
/// filling is NOT `WriteFailed`; only a failing underlying `write` is). A broken
/// pipe / write error surfaces as `WriteFailed`, which every action's `run` loop
/// treats as end-of-generation (`return Ok`).
struct StdoutWriter {
    // var buffer: [2048]u8 = undefined;
    buf: [u8; 2048],
    len: usize,
}

impl StdoutWriter {
    fn new() -> StdoutWriter {
        StdoutWriter {
            buf: [0u8; 2048],
            len: 0,
        }
    }

    /// Flush the accumulated buffer to the fd. A write error => `WriteFailed`.
    fn flush(&mut self) -> Result<(), WriteFailed> {
        if self.len == 0 {
            return Ok(());
        }
        let mut out = std::io::stdout().lock();
        out.write_all(&self.buf[..self.len])
            .map_err(|_| WriteFailed)?;
        self.len = 0;
        Ok(())
    }
}

impl Writer for StdoutWriter {
    type Error = WriteFailed;

    fn writeByte(&mut self, byte: u8) -> Result<(), WriteFailed> {
        self.writeAll(&[byte])
    }

    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
        let mut rest = bytes;
        while !rest.is_empty() {
            // buffer full → auto-flush
            if self.len == self.buf.len() {
                self.flush()?;
            }
            let n = core::cmp::min(self.buf.len() - self.len, rest.len());
            self.buf[self.len..self.len + n].copy_from_slice(&rest[..n]);
            self.len += n;
            rest = &rest[n..];
        }
        Ok(())
    }
}

// PORT NOTE: `mainActionImpl(comptime Impl, alloc, args)`. The Zig implementation
// parses CLI options (via `cli.args.parse` over an argv/string iterator), seeds
// a DefaultPrng from `std.time.nanoTimestamp`, and runs the impl into stdout,
// flushing at the end. The three monomorphisations are named explicitly (mirroring
// Zig's `inline else`); each is structurally identical, differing only in the
// concrete `Options`/impl type and the `create` error mapping. The Zig body:
//
// fn mainActionImpl(comptime Impl: type, alloc: Allocator, args: Args) !void {
//     // First, parse our CLI options.
//     const Options = Impl.Options;
//     var opts: Options = .{};
//     defer if (@hasDecl(Options, "deinit")) opts.deinit();
//     switch (args) {
//         .cli => { var iter = try cli.args.argsIterator(alloc); defer iter.deinit();
//                   try cli.args.parse(Options, alloc, &opts, &iter); },
//         .string => |str| { var iter = try std.process.ArgIteratorGeneral(.{}).init(alloc, str);
//                            defer iter.deinit(); try cli.args.parse(Options, alloc, &opts, &iter); },
//     }
//     // TODO: Make this a command line option.
//     const seed: u64 = @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())));
//     var prng = std.Random.DefaultPrng.init(seed);
//     const rand = prng.random();
//     // Our output always goes to stdout.
//     var buffer: [2048]u8 = undefined;
//     var stdout_writer = std.fs.File.stdout().writer(&buffer);
//     const writer = &stdout_writer.interface;
//     // Create our implementation
//     const impl = try Impl.create(alloc, opts);
//     defer impl.destroy(alloc);
//     try impl.run(writer, rand);
//     // Always flush
//     writer.flush() catch |err| switch (err) { error.WriteFailed => return };
// }

fn main_action_impl_ascii(args: Args) -> Result<(), Error> {
    // First, parse our CLI options.
    // const Options = Impl.Options; var opts: Options = .{};
    let mut opts = ascii::Options::default();
    // defer if (@hasDecl(Options, "deinit")) opts.deinit();
    //   (Options own no heap → automatic drop; no explicit deinit.)
    parse_opts(&mut opts, args)?;

    // TODO: Make this a command line option.
    let rand_prng = time_seeded_prng();
    let rand = rand_prng.random();

    // Our output always goes to stdout.
    let mut writer = StdoutWriter::new();

    // Create our implementation
    // const impl = try Impl.create(alloc, opts); defer impl.destroy(alloc);
    let impl_ = ascii::Ascii::create(opts).map_err(|_| Error::WriteFailed)?;
    // try impl.run(writer, rand);
    //   (destroy runs on every path, matching Zig's `defer impl.destroy`.)
    let run_result = impl_.run(&mut writer, rand);
    impl_.destroy();
    run_result.map_err(|_| Error::WriteFailed)?;

    // Always flush — writer.flush() catch |err| switch (err) { error.WriteFailed => return };
    let _ = writer.flush();
    Ok(())
}

fn main_action_impl_osc(args: Args) -> Result<(), Error> {
    // First, parse our CLI options.
    let mut opts = osc::Options::default();
    parse_opts(&mut opts, args)?;

    // TODO: Make this a command line option.
    let rand_prng = time_seeded_prng();
    let rand = rand_prng.random();

    // Our output always goes to stdout.
    let mut writer = StdoutWriter::new();

    // Create our implementation
    let impl_ = osc::Osc::create(opts).map_err(|_| Error::WriteFailed)?;
    let run_result = impl_.run(&mut writer, rand);
    impl_.destroy();
    run_result.map_err(|_| Error::WriteFailed)?;

    // Always flush
    let _ = writer.flush();
    Ok(())
}

fn main_action_impl_kitty(args: Args) -> Result<(), Error> {
    // First, parse our CLI options.
    let mut opts = kitty::Options::default();
    parse_opts(&mut opts, args)?;

    // TODO: Make this a command line option.
    let rand_prng = time_seeded_prng();
    let rand = rand_prng.random();

    // Our output always goes to stdout.
    let mut writer = StdoutWriter::new();

    // Create our implementation
    let mut impl_ = kitty::Kitty::create(opts).map_err(|_| Error::WriteFailed)?;
    let run_result = impl_.run(&mut writer, rand);
    impl_.destroy();
    run_result.map_err(|_| Error::WriteFailed)?;

    // Always flush
    let _ = writer.flush();
    Ok(())
}

fn main_action_impl_utf8(args: Args) -> Result<(), Error> {
    // First, parse our CLI options.
    let mut opts = utf8::Options::default();
    parse_opts(&mut opts, args)?;

    // TODO: Make this a command line option.
    let rand_prng = time_seeded_prng();
    let rand = rand_prng.random();

    // Our output always goes to stdout.
    let mut writer = StdoutWriter::new();

    // Create our implementation
    //   Utf8.create validates `invalid-rate`/weights → error.InvalidValue.
    let impl_ = utf8::Utf8::create(opts).map_err(|_| Error::InvalidValue)?;
    // PORT NOTE (§3): when `opts.seed` is `Some`, `Utf8::run` internally re-seeds a
    // fresh `DefaultPrng` and ignores this time-seeded `rand` (faithful to Zig).
    let run_result = impl_.run(&mut writer, rand);
    impl_.destroy();
    run_result.map_err(|_| Error::WriteFailed)?;

    // Always flush
    let _ = writer.flush();
    Ok(())
}

/// PORT NOTE: the shared `switch (args)` option-parse (identical for all three
/// impls; monomorphised in Zig by `mainActionImpl`'s `comptime Impl`).
fn parse_opts<T: rustty_args::args::Target>(opts: &mut T, args: Args) -> Result<(), Error> {
    // switch (args) {
    match args {
        // .cli => {
        Args::Cli => {
            // var iter = try cli.args.argsIterator(alloc); defer iter.deinit();
            let mut iter = rustty_args::args::args_iterator().unwrap_or_else(|e| match e {});
            // try cli.args.parse(Options, alloc, &opts, &iter);
            let result = rustty_args::args::parse(opts, &mut iter);
            iter.deinit();
            result?;
        }
        // .string => |str| {
        Args::String(str) => {
            // var iter = try std.process.ArgIteratorGeneral(.{}).init(alloc, str);
            // defer iter.deinit();
            //
            // PORT NOTE: `std.process.ArgIteratorGeneral` splits a single string
            // into argv (honoring quotes/backslashes). rustty-args exposes the
            // pre-split `SliceIterator`; the synthetic string args are simple
            // whitespace-separated flags, so we tokenize on ASCII whitespace
            // (the quote/escape handling is not exercised here — mirrors bench §6).
            let tokens: Vec<&[u8]> = str
                .split(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                .filter(|t| !t.is_empty())
                .collect();
            let mut iter = rustty_args::args::slice_iterator(&tokens);
            // try cli.args.parse(Options, alloc, &opts, &iter);
            rustty_args::args::parse(opts, &mut iter)?;
        }
    }
    Ok(())
}

/// PORT NOTE (§3): the time seed.
// const seed: u64 = @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())));
// var prng = std.Random.DefaultPrng.init(seed);
//   `std.time.nanoTimestamp()` is `i128` ns since the Unix epoch; `@bitCast` to
//   `u128` then `@truncate` keeps the low 64 bits. `Duration::as_nanos()` returns
//   the `u128` magnitude for any post-epoch instant (the only reachable case), and
//   `as u64` performs the identical low-64-bit `@truncate`. Pre-epoch is
//   astronomically unreachable; we map the `duration_since` error to `0`.
fn time_seeded_prng() -> crate::rand::DefaultPrng {
    let nanos: u128 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seed: u64 = nanos as u64;
    crate::rand::DefaultPrng::init(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_all_actions() {
        // Make sure we ref all our actions
        // (PORT NOTE: `inline for (@typeInfo(Action).@"enum".fields) |field| { … _ = Impl; }`)
        let _ = core::mem::size_of::<ascii::Ascii>();
        let _ = core::mem::size_of::<osc::Osc>();
        let _ = core::mem::size_of::<utf8::Utf8>();
        let _ = core::mem::size_of::<kitty::Kitty>();
        let _ = Action::Ascii;
        let _ = Action::Osc;
        let _ = Action::Utf8;
        let _ = Action::Kitty;
    }
}
