// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const cli = @import("../cli.zig");
use crate::apc_parser::ApcParser;
use crate::benchmark::{self, Benchmark, RunMode};
use crate::codepoint_width::CodepointWidth;
use crate::grapheme_break::GraphemeBreak;
use crate::hyperlink_map::HyperlinkMap;
use crate::is_symbol::IsSymbol;
use crate::osc_parser::OscParser;
use crate::page_compression::PageCompression;
use crate::screen_clone::ScreenClone;
use crate::scrollback_compression::ScrollbackCompression;
use crate::terminal_formatter::TerminalFormatter;
use crate::terminal_parser::TerminalParser;
use crate::terminal_resize::TerminalResize;
use crate::terminal_snapshot::TerminalSnapshot;
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
    // @"terminal-formatter",
    TerminalFormatter,
    // @"terminal-parser",
    TerminalParser,
    // @"terminal-resize",
    TerminalResize,
    // @"terminal-snapshot",
    TerminalSnapshot,
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
    // @"hyperlink-map",
    HyperlinkMap,
    // @"apc-parser",
    ApcParser,
}

impl Action {
    /// `std.meta.stringToEnum(Action, name)` — the kebab CLI tokens are the enum
    /// tag names, preserved exactly (`@"codepoint-width"` etc.).
    pub fn from_name(name: &[u8]) -> Option<Action> {
        match name {
            b"codepoint-width" => Some(Action::CodepointWidth),
            b"grapheme-break" => Some(Action::GraphemeBreak),
            b"screen-clone" => Some(Action::ScreenClone),
            b"terminal-formatter" => Some(Action::TerminalFormatter),
            b"terminal-parser" => Some(Action::TerminalParser),
            b"terminal-resize" => Some(Action::TerminalResize),
            b"terminal-snapshot" => Some(Action::TerminalSnapshot),
            b"terminal-stream" => Some(Action::TerminalStream),
            b"is-symbol" => Some(Action::IsSymbol),
            b"osc-parser" => Some(Action::OscParser),
            b"page-compression" => Some(Action::PageCompression),
            b"scrollback-compression" => Some(Action::ScrollbackCompression),
            b"hyperlink-map" => Some(Action::HyperlinkMap),
            b"apc-parser" => Some(Action::ApcParser),
            _ => None,
        }
    }

    /// `@tagName(action)` — the inverse of [`Self::from_name`]: the kebab CLI
    /// token that IS the reference enum's tag name. Used where the reference
    /// formats `@tagName(action)` (`CApi.zig:27`, S61-F8).
    pub fn tag_name(self) -> &'static str {
        match self {
            Action::CodepointWidth => "codepoint-width",
            Action::GraphemeBreak => "grapheme-break",
            Action::ScreenClone => "screen-clone",
            Action::TerminalFormatter => "terminal-formatter",
            Action::TerminalParser => "terminal-parser",
            Action::TerminalResize => "terminal-resize",
            Action::TerminalSnapshot => "terminal-snapshot",
            Action::TerminalStream => "terminal-stream",
            Action::IsSymbol => "is-symbol",
            Action::OscParser => "osc-parser",
            Action::PageCompression => "page-compression",
            Action::ScrollbackCompression => "scrollback-compression",
            Action::HyperlinkMap => "hyperlink-map",
            Action::ApcParser => "apc-parser",
        }
    }

    /// Every action, in declaration order (test support for the
    /// [`Self::tag_name`] / [`Self::from_name`] round trip).
    #[cfg(test)]
    pub(crate) const ALL: [Action; 14] = [
        Action::CodepointWidth,
        Action::GraphemeBreak,
        Action::ScreenClone,
        Action::TerminalFormatter,
        Action::TerminalParser,
        Action::TerminalResize,
        Action::TerminalSnapshot,
        Action::TerminalStream,
        Action::IsSymbol,
        Action::OscParser,
        Action::PageCompression,
        Action::ScrollbackCompression,
        Action::HyperlinkMap,
        Action::ApcParser,
    ];

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
    fn create(alloc: Allocator, opts: Self::Options) -> Result<Box<Self>, CreateError>;
    // fn destroy(alloc) — free the instance.
    fn destroy(self: Box<Self>, alloc: Allocator);
    // fn benchmark() — returns a `Benchmark` instance for the action.
    fn benchmark(&mut self) -> Benchmark;
}

/// The error set of `BenchmarkImpl.create` (`!*T`). Zig infers a distinct error
/// set per impl and `mainActionImpl`'s own inferred set absorbs all of them;
/// Rust needs one named union, so this enum lists every variant any benchmark's
/// `create` can return. `OutOfMemory` covers the eight allocation-only impls;
/// `InvalidEntries`/`UnexpectedCapacity` are `HyperlinkMap.create`'s
/// (`7e14347c1`).
#[derive(Debug)]
pub enum CreateError {
    // error.OutOfMemory
    OutOfMemory,
    // return error.InvalidEntries;
    InvalidEntries,
    // return error.UnexpectedCapacity;
    UnexpectedCapacity,
}

impl From<OutOfMemory> for CreateError {
    fn from(_: OutOfMemory) -> CreateError {
        CreateError::OutOfMemory
    }
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
    // try BenchmarkImpl.create(...) — the non-allocation create failures.
    Create(CreateError),
    // _ = try b.run(.once);
    Benchmark(benchmark::Error),
    // try global.init(.{ .tool = minimal });
    GlobalInit(rustty_apprt::InitError),
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

impl From<CreateError> for Error {
    fn from(e: CreateError) -> Error {
        match e {
            CreateError::OutOfMemory => Error::OutOfMemory,
            other => Error::Create(other),
        }
    }
}

impl From<benchmark::Error> for Error {
    fn from(e: benchmark::Error) -> Error {
        Error::Benchmark(e)
    }
}

/// An entrypoint for the benchmark CLI.
// pub fn main(minimal: std.process.Init.Minimal) !void {
//     try global.init(.{ .tool = minimal });
//     const alloc = std.heap.c_allocator;
//     const action_ = try cli.action.detectArgs(Action, alloc, minimal.args);
//     const action = action_ orelse return error.NoAction;
//     try mainAction(alloc, action, .{ .cli = minimal.args });
// }
//
// (CORRECTION, parity S62-F2 review: the block above used to quote the older
// `pub fn main() !void`, with no `global.init` and `detectArgs(Action,
// alloc)`, which is not the pinned `benchmark/cli.zig:54-60`.)
//
// PORT NOTE: `std.heap.c_allocator` -> the process `Global` allocator;
// `minimal.args` -> `std::env::args_os` (read inside `detect_args` and
// `args_iterator`). The reference `cli.action.detectArgs` is the SHARED action
// auto-detect that lives in `rustty-cli`, which rustty-bench does not depend
// on; it does depend on rustty-apprt, for the `.tool` init (S43-F3).
// (CORRECTION: this note used to call rustty-cli "a heavy crate —
// core/apprt/config — that rustty-bench deliberately does NOT depend on to stay
// lean and acyclic", which stopped being true of apprt with S43-F3.)
// This standalone `main()` is exercised only by the out-of-scope `main_bench.zig`
// binary; its detect logic (find the single `+name` arg, `stringToEnum`) is
// replicated locally over `std::env::args_os` (DEFERRALS.md "benchmark: shared
// detectArgs"). The benchmark `Action` declares no `detectSpecialCase`, so the
// special-case arm of `detectIter` is elided.
//
// PORT NOTE (parity S43-F3, controller Decision B): the reference `main` now
// begins with `try global.init(.{ .tool = minimal });` (`cli.zig:55`): the full
// process init (RUSTTY_LOG, signals, rlimits, locale, crash, glslang,
// oniguruma, resources dir, i18n) with app-action detection skipped, because
// the tool's `+terminal-stream` etc. are not app actions. The port's global
// init is `rustty_apprt::init_with_action`, and `false` ("no app action") is
// that `.tool` arm. The C API reaches `main_action`, not this function, so it
// keeps relying on `rustty_init` having run, as the reference's `CApi.zig`
// does. The benchmark timers start inside `Benchmark::run`, after this.
pub fn main() -> Result<(), Error> {
    // try global.init(.{ .tool = minimal });
    rustty_apprt::init_with_action(false).map_err(Error::GlobalInit)?;
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
        Action::TerminalFormatter => main_action_impl::<TerminalFormatter>(alloc, args),
        Action::TerminalParser => main_action_impl::<TerminalParser>(alloc, args),
        Action::TerminalResize => main_action_impl::<TerminalResize>(alloc, args),
        Action::TerminalSnapshot => main_action_impl::<TerminalSnapshot>(alloc, args),
        Action::TerminalStream => main_action_impl::<TerminalStream>(alloc, args),
        Action::IsSymbol => main_action_impl::<IsSymbol>(alloc, args),
        Action::OscParser => main_action_impl::<OscParser>(alloc, args),
        Action::PageCompression => main_action_impl::<PageCompression>(alloc, args),
        Action::ScrollbackCompression => main_action_impl::<ScrollbackCompression>(alloc, args),
        Action::HyperlinkMap => main_action_impl::<HyperlinkMap>(alloc, args),
        Action::ApcParser => main_action_impl::<ApcParser>(alloc, args),
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
            // var iter = try std.process.Args.IteratorGeneral(.{}).init(alloc, str);
            // defer iter.deinit();
            //
            // PORT NOTE (parity S62-F1): the reference tokenizes with Zig std's
            // `IteratorGeneral(.{})`, which honours double quotes and backslash
            // escapes; its port is `rustty_args::arg_iterator_general`.
            // CORRECTION: this arm used to split on ASCII whitespace, with a note
            // saying "the quote/escape handling is not exercised".
            let mut iter = rustty_args::arg_iterator_general::ArgIteratorGeneral::init(str);
            // try cli.args.parse(Options, alloc, &opts, &iter);
            let result = rustty_args::args::parse(&mut opts, &mut iter);
            iter.deinit();
            result?;
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

    /// Every CLI token in `benchmark/cli.zig`'s `Action` round-trips through
    /// `from_name`, and nothing else does. Named for what it checks rather
    /// than extending the compression-only test above (WS10 T11).
    ///
    /// The list is the oracle's complete `Action` enum at the phase-4 pin
    /// `12967b68f` (the `terminal-formatter`/`terminal-resize`/
    /// `terminal-snapshot` trio landed with assessment B cluster C8); the
    /// G-BENCH oracle-parity arm cross-checks the same set from the oracle
    /// side.
    #[test]
    fn bench_every_action_round_trips_through_from_name() {
        let names: &[(&[u8], Action)] = &[
            (b"codepoint-width", Action::CodepointWidth),
            (b"grapheme-break", Action::GraphemeBreak),
            (b"screen-clone", Action::ScreenClone),
            (b"terminal-formatter", Action::TerminalFormatter),
            (b"terminal-parser", Action::TerminalParser),
            (b"terminal-resize", Action::TerminalResize),
            (b"terminal-snapshot", Action::TerminalSnapshot),
            (b"terminal-stream", Action::TerminalStream),
            (b"is-symbol", Action::IsSymbol),
            (b"osc-parser", Action::OscParser),
            (b"page-compression", Action::PageCompression),
            (b"scrollback-compression", Action::ScrollbackCompression),
            (b"hyperlink-map", Action::HyperlinkMap),
            (b"apc-parser", Action::ApcParser),
        ];
        assert_eq!(14, names.len());
        for (name, expected) in names {
            assert_eq!(Some(*expected), Action::from_name(name), "{name:?}");
        }
        assert_eq!(None, Action::from_name(b"nope"));
        // A near-miss must not resolve: `from_name` is an exact match.
        assert_eq!(None, Action::from_name(b"hyperlink_map"));
    }

    /// Parity S62-F1: the `.string` arm tokenizes with `IteratorGeneral(.{})`
    /// (`cli.zig:99-106`), so a double-quoted `--data` value containing a space
    /// is ONE argument with the quotes stripped. A whitespace split turns it
    /// into `--data="<dir>/a` and `b.vt"`, which fails to parse.
    #[test]
    fn string_args_honour_quotes_like_arg_iterator_general() {
        use super::{main_action, Args, Error};

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "rustty-s62-f1-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a b.vt");
        std::fs::write(&path, b"hello\r\n\x1b[1mworld").unwrap();

        let quoted = |p: &std::path::Path| {
            let mut s = b"--data=\"".to_vec();
            s.extend_from_slice(p.as_os_str().as_encoded_bytes());
            s.push(b'"');
            s
        };

        // The file exists: the quoted path opens and the benchmark runs.
        let args = quoted(&path);
        let ok = main_action(
            super::Allocator,
            Action::TerminalParser,
            Args::String(&args),
        );

        // The same quoted form naming a missing file fails in setup (the data
        // file cannot be opened), which shows the path reached `dataFile`.
        let missing = quoted(&dir.join("missing c.vt"));
        let err = main_action(
            super::Allocator,
            Action::TerminalParser,
            Args::String(&missing),
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);

        assert!(
            ok.is_ok(),
            "quoted --data with a space must parse and open: {ok:?}"
        );
        assert!(
            matches!(err, Err(Error::Benchmark(_))),
            "a missing quoted path must fail opening the data file, not parsing: {err:?}"
        );
    }
}
