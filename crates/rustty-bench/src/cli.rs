// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const cli = @import("../cli.zig");
use crate::benchmark::{self, Benchmark, RunMode};
use crate::codepoint_width::CodepointWidth;
use crate::grapheme_break::GraphemeBreak;
use crate::is_symbol::IsSymbol;
use crate::osc_parser::OscParser;
use crate::page_compression::PageCompression;
use crate::screen_clone::ScreenClone;
use crate::scrollback_compression::ScrollbackCompression;
use crate::terminal_parser::TerminalParser;
use crate::terminal_stream::TerminalStream;
use allocator_api2::alloc::Global as Allocator;
use rustty_vt::pagelist::OutOfMemory;

/// The available actions for the CLI. This is the list of available
/// benchmarks. View docs for each individual one in the predictably
/// named files.
// pub const Action = enum {
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    // @"codepoint-width",
    CodepointWidth,
    // @"grapheme-break",
    GraphemeBreak,
    // @"screen-clone",
    ScreenClone,
    // @"terminal-parser",
    TerminalParser,
    // @"terminal-stream",
    TerminalStream,
    // @"is-symbol",
    IsSymbol,
    // @"osc-parser",
    OscParser,
    // @"page-compression",
    PageCompression,
    // @"scrollback-compression",
    ScrollbackCompression,
}

impl Action {
    /// `std.meta.stringToEnum(Action, name)` — the kebab CLI tokens are the enum
    /// tag names, preserved exactly (`@"codepoint-width"` etc.).
    pub fn from_name(name: &[u8]) -> Option<Action> {
        match name {
            b"codepoint-width" => Some(Action::CodepointWidth),
            b"grapheme-break" => Some(Action::GraphemeBreak),
            b"screen-clone" => Some(Action::ScreenClone),
            b"terminal-parser" => Some(Action::TerminalParser),
            b"terminal-stream" => Some(Action::TerminalStream),
            b"is-symbol" => Some(Action::IsSymbol),
            b"osc-parser" => Some(Action::OscParser),
            b"page-compression" => Some(Action::PageCompression),
            b"scrollback-compression" => Some(Action::ScrollbackCompression),
            _ => None,
        }
    }

    // /// Returns the struct associated with the action. The struct
    // /// should have a few decls:
    // ///
    // ///   - `const Options`: The CLI options for the action.
    // ///   - `fn create`: Create a new instance of the action from options.
    // ///   - `fn benchmark`: Returns a `Benchmark` instance for the action.
    // ///
    // /// See TerminalStream for an example.
    // pub fn Struct(comptime action: Action) type {
    //     return switch (action) {
    //         .@"screen-clone" => @import("ScreenClone.zig"),
    //         ... one import per action ...
    //     };
    // }
    //
    // PORT NOTE (§6): Rust has no comptime type-switch. `Action.Struct(action)`
    // selecting the benchmark *type* is modeled by the `BenchmarkAction` trait
    // (the four decls the comptime code requires) plus the `match` in
    // `main_action` that monomorphizes `main_action_impl::<Impl>` per arm.
}

/// The decls that `Action.Struct(comptime_action)` requires of each benchmark
/// impl (`Options`, `create`, `destroy`, `benchmark`), captured as a trait so the
/// `main_action` `match` can monomorphize `main_action_impl::<Impl>` per arm.
pub trait BenchmarkAction: Sized {
    // const Options — the CLI options for the action.
    type Options: Default + rustty_args::args::Target;
    // fn create(alloc, opts) — create a new instance of the action from options.
    fn create(alloc: Allocator, opts: Self::Options) -> Result<Box<Self>, OutOfMemory>;
    // fn destroy(alloc) — free the instance.
    fn destroy(self: Box<Self>, alloc: Allocator);
    // fn benchmark() — returns a `Benchmark` instance for the action.
    fn benchmark(&mut self) -> Benchmark;
}

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
    // try BenchmarkImpl.create(...)
    OutOfMemory,
    // _ = try b.run(.once);
    Benchmark(benchmark::Error),
}

impl From<rustty_args::args::ParseError> for Error {
    fn from(e: rustty_args::args::ParseError) -> Error {
        Error::Parse(e)
    }
}

impl From<OutOfMemory> for Error {
    fn from(_: OutOfMemory) -> Error {
        Error::OutOfMemory
    }
}

impl From<benchmark::Error> for Error {
    fn from(e: benchmark::Error) -> Error {
        Error::Benchmark(e)
    }
}

/// An entrypoint for the benchmark CLI.
// pub fn main() !void {
//     const alloc = std.heap.c_allocator;
//     const action_ = try cli.action.detectArgs(Action, alloc);
//     const action = action_ orelse return error.NoAction;
//     try mainAction(alloc, action, .cli);
// }
//
// PORT NOTE: `std.heap.c_allocator` -> the process `Global` allocator. The
// reference `cli.action.detectArgs(Action, alloc)` is the SHARED action
// auto-detect that lives in `rustty-cli` (a heavy crate — core/apprt/config —
// that rustty-bench deliberately does NOT depend on to stay lean and acyclic).
// This standalone `main()` is exercised only by the out-of-scope `main_bench.zig`
// binary; its detect logic (find the single `+name` arg, `stringToEnum`) is
// replicated locally over `std::env::args_os` (DEFERRALS.md "benchmark: shared
// detectArgs"). The benchmark `Action` declares no `detectSpecialCase`, so the
// special-case arm of `detectIter` is elided.
pub fn main() -> Result<(), Error> {
    let alloc = Allocator;
    let action_ = detect_args()?;
    // const action = action_ orelse return error.NoAction;
    let action = action_.ok_or(Error::NoAction)?;
    // try mainAction(alloc, action, .cli);
    main_action(alloc, action, Args::Cli)
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
    // return fallback (there is none for the benchmark Action) => pending.
    Ok(pending)
}

/// Arguments that can be passed to the benchmark.
// pub const Args = union(enum) {
pub enum Args<'a> {
    /// The arguments passed to the CLI via argc/argv.
    // cli,
    Cli,

    /// Simple string arguments, parsed via std.process.ArgIteratorGeneral.
    // string: []const u8,
    String(&'a [u8]),
}

// pub fn mainAction(alloc: Allocator, action: Action, args: Args) !void {
//     switch (action) {
//         inline else => |comptime_action| {
//             const BenchmarkImpl = Action.Struct(comptime_action);
//             try mainActionImpl(BenchmarkImpl, alloc, args);
//         },
//     }
// }
pub fn main_action(alloc: Allocator, action: Action, args: Args) -> Result<(), Error> {
    match action {
        Action::CodepointWidth => main_action_impl::<CodepointWidth>(alloc, args),
        Action::GraphemeBreak => main_action_impl::<GraphemeBreak>(alloc, args),
        Action::ScreenClone => main_action_impl::<ScreenClone>(alloc, args),
        Action::TerminalParser => main_action_impl::<TerminalParser>(alloc, args),
        Action::TerminalStream => main_action_impl::<TerminalStream>(alloc, args),
        Action::IsSymbol => main_action_impl::<IsSymbol>(alloc, args),
        Action::OscParser => main_action_impl::<OscParser>(alloc, args),
        Action::PageCompression => main_action_impl::<PageCompression>(alloc, args),
        Action::ScrollbackCompression => main_action_impl::<ScrollbackCompression>(alloc, args),
    }
}

// fn mainActionImpl(comptime BenchmarkImpl: type, alloc: Allocator, args: Args) !void {
fn main_action_impl<B: BenchmarkAction>(alloc: Allocator, args: Args) -> Result<(), Error> {
    // First, parse our CLI options.
    // const Options = BenchmarkImpl.Options;
    // var opts: Options = .{};
    let mut opts: B::Options = B::Options::default();
    // defer if (@hasDecl(Options, "deinit")) opts.deinit();
    //   PORT NOTE (§6): the benchmark Options structs own no heap (their `data`
    //   is an owned `Vec<u8>` that drops with `opts`), so the `@hasDecl(Options,
    //   "deinit")` guard maps to Rust's automatic drop — no explicit deinit.
    // switch (args) {
    match args {
        // .cli => {
        Args::Cli => {
            // var iter = try cli.args.argsIterator(alloc);
            // defer iter.deinit();
            let mut iter = rustty_args::args::args_iterator().unwrap_or_else(|e| match e {});
            // try cli.args.parse(Options, alloc, &opts, &iter);
            let result = rustty_args::args::parse(&mut opts, &mut iter);
            iter.deinit();
            result?;
        }
        // .string => |str| {
        Args::String(str) => {
            // var iter = try std.process.ArgIteratorGeneral(.{}).init(alloc, str);
            // defer iter.deinit();
            //
            // PORT NOTE (§6): `std.process.ArgIteratorGeneral` splits a single
            // string into argv (honoring quotes/backslashes). rustty-args exposes
            // the pre-split `SliceIterator`; the benchmark C-ABI args are simple
            // whitespace-separated flags, so we tokenize on ASCII whitespace
            // (the quote/escape handling is not exercised by the benchmark).
            let tokens: Vec<&[u8]> = str
                .split(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                .filter(|t| !t.is_empty())
                .collect();
            let mut iter = rustty_args::args::slice_iterator(&tokens);
            // try cli.args.parse(Options, alloc, &opts, &iter);
            rustty_args::args::parse(&mut opts, &mut iter)?;
        }
    }

    // Create our implementation
    // const impl = try BenchmarkImpl.create(alloc, opts);
    // defer impl.destroy(alloc);
    let mut impl_ = B::create(alloc, opts)?;

    // Initialize our benchmark
    // const b = impl.benchmark();
    let b = impl_.benchmark();
    // _ = try b.run(.once);
    //
    // PORT NOTE: the reference's `defer impl.destroy(alloc)` runs on every exit
    // path (including the error return from `run`); we run `destroy` before
    // propagating the run result to match that.
    let result = b.run(RunMode::Once);
    impl_.destroy(alloc);
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Action;

    /// The two new actions are reachable by name, matching the oracle's CLI.
    ///
    /// PORT NOTE (WS7 D8): the plan's snippet writes
    /// `Action::from_name("page-compression")`; the real signature takes a byte
    /// slice (the Zig `[]const u8` CLI token), so the literals are `b"..."`.
    #[test]
    fn bench_registers_compression_actions() {
        assert!(Action::from_name(b"page-compression").is_some());
        assert!(Action::from_name(b"scrollback-compression").is_some());
    }
}
