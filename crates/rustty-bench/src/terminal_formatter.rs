//! Benchmarks the terminal formatter (`terminal/formatter.zig`).
//!
//! The formatter is the hot path for clipboard copy (plain/VT/HTML),
//! `write_screen_file`, `selectionString`, terminal search window
//! encoding, and the librustty-vt formatter C API. This benchmark
//! measures formatting terminal contents that were built during setup
//! (outside the timed region) from a pre-generated VT stream.
//!
//! ## Input
//!
//! `--data` names a pre-generated VT byte stream (for example from
//! `rustty-gen styled`). The stream is fed to a terminal of the
//! requested dimensions with unlimited scrollback during setup. The
//! resulting screen contents (scrollback included) are what each step
//! formats.
//!
//! ## Modes
//!
//! * `noop` performs no formatting and establishes loop/setup overhead.
//!   Subtract this from `format` timings when using hyperfine.
//! * `format` formats the configured region once per loop into a
//!   reusable buffer. Buffer growth happens on the first iteration only.
//! * `report` formats once and prints content/output sizes. It is for
//!   computing throughput (cells/s, bytes/s), not timing comparisons.
//!
//! ## Examples
//!
//! Build benchmarks in release mode:
//!
//! ```text
//! cargo build --release -p rustty-bench
//! ```
//!
//! Generate a deterministic corpus, then measure:
//!
//! ```text
//! rustty-gen styled --seed=42 | head -c 640000 > /tmp/plain.vt
//! hyperfine --warmup 3 \
//!   'rustty-bench +terminal-formatter --mode=noop --data=/tmp/plain.vt' \
//!   'rustty-bench +terminal-formatter --emit=plain --loops=50 --data=/tmp/plain.vt' \
//!   'rustty-bench +terminal-formatter --emit=vt --loops=50 --data=/tmp/plain.vt'
//! ```
// const TerminalFormatter = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const formatterpkg = terminalpkg.formatter;
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const Terminal = terminalpkg.Terminal;
// const Selection = terminalpkg.Selection;
// const global = @import("../global.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_datastruct::intrusive_linked_list::Node as _;
use rustty_vt::formatter as formatterpkg;
use rustty_vt::pagelist::{Node as PageNode, OutOfMemory};
use rustty_vt::point;
use rustty_vt::screen::Screen;
use rustty_vt::selection::Selection;
use rustty_vt::terminal::Terminal;
// `std.hash.Wyhash` — the bit-exact Zig-stdlib shim (report-mode hashes).
use rustty_zigstd::wyhash::Wyhash;

// const log = std.log.scoped(.@"terminal-formatter-bench");
const LOG_SCOPE: &str = "terminal-formatter-bench";

// alloc: Allocator,
// opts: Options,
// terminal: ?Terminal = null,
pub struct TerminalFormatter {
    alloc: Allocator,
    opts: Options,
    terminal: Option<Terminal>,

    /// Reused across steps so buffer growth is a one-time setup cost.
    // output: std.Io.Writer.Allocating,
    //
    // PORT NOTE: `std.Io.Writer.Allocating` (a growable in-memory writer) is a
    // `Vec<u8>`; `written()` is the slice, `shrinkRetainingCapacity(0)` is
    // `clear()` (both keep capacity).
    output: Vec<u8>,

    /// Reused pin map storage for `--pin-map=true`.
    // pins: formatterpkg.PinMap.Map = .empty,
    pins: formatterpkg::PinMapMap,
}

// pub const Options = struct {
//     /// Set by the shared CLI parser for string option ownership.
//     _arena: ?std.heap.ArenaAllocator = null,
//
// PORT NOTE (§6, terminal_stream precedent): the `_arena` + `deinit` pair
// exists for string option ownership; `data` is an owned `Vec<u8>` here, so
// Rust's automatic drop replaces both.
pub struct Options {
    /// Select the operation performed inside the timed benchmark step.
    // mode: Mode = .format,
    mode: Mode,

    /// The output format to emit.
    // emit: Emit = .vt,
    emit: Emit,

    /// The region of the screen to format.
    // region: Region = .screen,
    region: Region,

    /// Unwrap soft-wrapped lines.
    // unwrap: bool = false,
    unwrap: bool,

    /// Track the source pin of every emitted byte. This exercises the
    /// (documented as expensive) pin_map path used by selectionString
    /// and search.
    // @"pin-map": bool = false,
    pin_map: bool,

    /// Number of format operations per benchmark step. Increase this
    /// when the content is too small for stable hyperfine measurements.
    // loops: u32 = 25,
    loops: u32,

    /// The size of the terminal. This affects wrapping and page sizes.
    // @"terminal-rows": u16 = 24,
    // @"terminal-cols": u16 = 80,
    terminal_rows: u16,
    terminal_cols: u16,

    /// Pre-generated VT stream fed to the terminal during setup. `-`
    /// reads stdin, although a regular file is recommended so identical
    /// state can be reused across runs. When unset, the terminal is
    /// empty.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Format,
            emit: Emit::Vt,
            region: Region::Screen,
            unwrap: false,
            pin_map: false,
            loops: 25,
            terminal_rows: 24,
            terminal_cols: 80,
            data: None,
        }
    }
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
            b"emit" => {
                self.emit = rustty_args::args::parse_field_enum(value, Emit::from_name)?;
                Ok(())
            }
            b"region" => {
                self.region = rustty_args::args::parse_field_enum(value, Region::from_name)?;
                Ok(())
            }
            b"unwrap" => {
                self.unwrap = rustty_args::args::parse_field_bool(value)?;
                Ok(())
            }
            b"pin-map" => {
                self.pin_map = rustty_args::args::parse_field_bool(value)?;
                Ok(())
            }
            b"loops" => {
                self.loops = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            b"terminal-rows" => {
                self.terminal_rows = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            b"terminal-cols" => {
                self.terminal_cols = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            b"data" => {
                self.data = Some(rustty_args::args::parse_field_string(value)?);
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

// pub const Mode = enum {
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Establish the benchmark loop and setup overhead.
    Noop,

    /// Format the configured region once per loop.
    Format,

    /// Print content and output sizes. Not a timing benchmark.
    Report,

    /// Semantic verification, not a timing benchmark: format the
    /// terminal (must be `--emit=vt`), feed the output into a fresh
    /// terminal of the same dimensions, format that, and verify the
    /// two outputs converge to identical bytes. This proves the VT
    /// output faithfully reconstructs the terminal content even when
    /// the exact byte encoding changes.
    Roundtrip,
}

impl Mode {
    pub fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"format" => Some(Mode::Format),
            b"report" => Some(Mode::Report),
            b"roundtrip" => Some(Mode::Roundtrip),
            _ => None,
        }
    }
}

// pub const Emit = enum {
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Emit {
    Plain,
    Vt,
    Html,
}

impl Emit {
    pub fn from_name(name: &[u8]) -> Option<Emit> {
        match name {
            b"plain" => Some(Emit::Plain),
            b"vt" => Some(Emit::Vt),
            b"html" => Some(Emit::Html),
            _ => None,
        }
    }

    /// `@tagName(emit)`.
    fn tag_name(self) -> &'static str {
        match self {
            Emit::Plain => "plain",
            Emit::Vt => "vt",
            Emit::Html => "html",
        }
    }

    // fn format(self: Emit) formatterpkg.Format {
    fn format(self) -> formatterpkg::Format {
        match self {
            Emit::Plain => formatterpkg::Format::Plain,
            Emit::Vt => formatterpkg::Format::Vt,
            Emit::Html => formatterpkg::Format::Html,
        }
    }
}

// pub const Region = enum {
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    /// Everything: scrollback and active screen.
    Screen,

    /// Only the active screen (bottom rows).
    Active,

    /// Only the scrollback.
    History,
}

impl Region {
    pub fn from_name(name: &[u8]) -> Option<Region> {
        match name {
            b"screen" => Some(Region::Screen),
            b"active" => Some(Region::Active),
            b"history" => Some(Region::History),
            _ => None,
        }
    }

    /// `@tagName(region)`.
    fn tag_name(self) -> &'static str {
        match self {
            Region::Screen => "screen",
            Region::Active => "active",
            Region::History => "history",
        }
    }
}

impl TerminalFormatter {
    // pub fn create(alloc: Allocator, opts: Options) !*TerminalFormatter {
    //     const ptr = try alloc.create(TerminalFormatter);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{
    //         .alloc = alloc,
    //         .opts = opts,
    //         .output = .init(alloc),
    //     };
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<TerminalFormatter>, OutOfMemory> {
        Ok(Box::new(TerminalFormatter {
            alloc,
            opts,
            terminal: None,
            output: Vec::new(),
            pins: formatterpkg::PinMapMap::EMPTY,
        }))
    }

    // pub fn destroy(self: *TerminalFormatter, alloc: Allocator) void {
    //     if (self.terminal) |*t| t.deinit(self.alloc);
    //     self.output.deinit();
    //     self.pins.deinit(self.alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<TerminalFormatter>, alloc: Allocator) {
        if let Some(t) = self.terminal.as_mut() {
            t.deinit(self.alloc.into());
        }
        self.pins.deinit(self.alloc.into());
        let _ = alloc;
        drop(self);
    }

    // pub fn benchmark(self: *TerminalFormatter) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Format => Self::step_format,
                    Mode::Report => Self::step_report,
                    Mode::Roundtrip => Self::step_roundtrip,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    /// Build the terminal state every mode shares. All of this is outside
    /// the timed region for `Benchmark`, but is included in whole-process
    /// timings, hence the `noop` mode.
    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalFormatter) };
        // self.setupImpl() catch |err| {
        //     log.warn("failed to prepare formatter benchmark err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        self_.setup_impl().map_err(|err| {
            // log.warn("failed to prepare formatter benchmark err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to prepare formatter benchmark err={err:?}");
            Error::BenchmarkFailed
        })
    }

    // fn setupImpl(self: *TerminalFormatter) !void {
    fn setup_impl(&mut self) -> Result<(), SetupError> {
        if let Some(t) = self.terminal.as_mut() {
            t.deinit(self.alloc.into());
        }
        self.terminal = None;
        // self.terminal = try Terminal.init(global.io(), self.alloc, .{
        //     .cols = self.opts.@"terminal-cols",
        //     .rows = self.opts.@"terminal-rows",
        //     .max_scrollback_bytes = null,
        //     .max_scrollback_lines = null,
        // });
        self.terminal = Some(Terminal::init(
            self.alloc.into(),
            rustty_vt::terminal::Options {
                max_scrollback_bytes: None,
                max_scrollback_lines: None,
                ..crate::terminal_options(self.opts.terminal_rows, self.opts.terminal_cols)
            },
        )?);
        let terminal = self.terminal.as_mut().unwrap();

        // Feed the input corpus through the standard VT stream.
        // if (try options.dataFile(self.opts.data)) |data_f| {
        if let Some(mut data_f) = options::data_file(self.opts.data.as_deref())? {
            // var stream = terminal.vtStream();
            // defer stream.deinit();
            let mut stream = terminal.vt_stream();

            // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
            // var f_reader = data_f.reader(global.io(), &read_buf);
            // const r = &f_reader.interface;
            //
            // PORT NOTE (§8, terminal_stream precedent): `read_slice_short`
            // reads straight from the `DataFile`; `buf` below is the dispatch
            // buffer that matters.
            let r = &mut data_f;

            // var buf: [4096]u8 = undefined;
            let mut buf = [0u8; 4096];
            // while (true) {
            loop {
                // const n = try r.readSliceShort(&buf);
                let n = match read_slice_short(r, &mut buf) {
                    Ok(n) => n,
                    Err(err) => {
                        stream.deinit();
                        return Err(SetupError::Io(err));
                    }
                };
                // if (n == 0) break; // EOF reached
                if n == 0 {
                    break;
                }
                // stream.nextSlice(buf[0..n]);
                stream.next_slice(&buf[..n]);
            }

            stream.deinit();
        }
        Ok(())
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut TerminalFormatter) };
        if let Some(t) = self_.terminal.as_mut() {
            t.deinit(self_.alloc.into());
        }
        self_.terminal = None;
        // self.output.shrinkRetainingCapacity(0);
        self_.output.clear();
        // self.pins.clearRetainingCapacity();
        self_.pins.clear_retaining_capacity();
    }

    /// `.{ .emit = self.opts.emit.format(), .unwrap = self.opts.unwrap }` —
    /// the reference struct literal relies on `Options` field defaults for the
    /// rest; the per-format constructors carry exactly those defaults, so only
    /// `unwrap` is overridden (Rust-only helper shared by `formatter` and
    /// `stepRoundtripImpl`, which spell the same literal twice upstream).
    fn formatter_options(&self) -> formatterpkg::Options {
        let mut opts = match self.opts.emit.format() {
            formatterpkg::Format::Plain => formatterpkg::Options::plain(),
            formatterpkg::Format::Vt => formatterpkg::Options::vt(),
            formatterpkg::Format::Html => formatterpkg::Options::html(),
        };
        opts.unwrap = self.opts.unwrap;
        opts
    }

    /// Build the screen formatter matching our options. This mirrors how
    /// Surface clipboard copy and write_screen_file construct formatters.
    // fn formatter(self: *TerminalFormatter) ?formatterpkg.ScreenFormatter {
    fn formatter<'a>(&self) -> Option<formatterpkg::ScreenFormatter<'a>> {
        // const screen = self.terminal.?.screens.active;
        let screen: *const Screen = self.terminal.as_ref().unwrap().screens.active;

        // var f: formatterpkg.ScreenFormatter = .init(screen, .{
        //     .emit = self.opts.emit.format(),
        //     .unwrap = self.opts.unwrap,
        // });
        let mut f = formatterpkg::ScreenFormatter::init(screen, self.formatter_options());

        // f.content = switch (self.opts.region) { ... };
        f.content = match self.opts.region {
            // .screen => .{ .selection = null },
            Region::Screen => formatterpkg::Content::Selection(None),

            // inline .active, .history => |region| content: { ... },
            region @ (Region::Active | Region::History) => {
                let tag: point::Tag = match region {
                    Region::Active => point::Tag::Active,
                    Region::History => point::Tag::History,
                    Region::Screen => unreachable!(),
                };
                // PORT NOTE: `screen` is the raw active-screen pointer; the
                // pagelist reads form a short-lived shared borrow (no `&mut`
                // Terminal is live).
                let pages = unsafe { &(*screen).pages };
                // const tl = screen.pages.getTopLeft(tag);
                let tl = pages.get_top_left(tag);
                // const br = screen.pages.getBottomRight(tag) orelse return null;
                let br = pages.get_bottom_right(tag)?;
                // break :content .{ .selection = Selection.init(tl, br, false) };
                formatterpkg::Content::Selection(Some(Selection::init(tl, br, false)))
            }
        };

        Some(f)
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalFormatter) };
        for _ in 0..self_.opts.loops {
            // std.mem.doNotOptimizeAway(self.output.written());
            black_box(self_.output.as_slice());
        }
        Ok(())
    }

    // fn stepFormat(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_format(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalFormatter) };
        for _ in 0..self_.opts.loops {
            // self.output.shrinkRetainingCapacity(0);
            self_.output.clear();

            // var f = self.formatter() orelse continue;
            let mut f = match self_.formatter() {
                Some(f) => f,
                None => continue,
            };
            // if (self.opts.@"pin-map") {
            if self_.opts.pin_map {
                self_.pins.clear_retaining_capacity();
                // f.pin_map = .{ .alloc = self.alloc, .map = &self.pins };
                f.pin_map = Some(formatterpkg::PinMap {
                    alloc: self_.alloc.into(),
                    map: &mut self_.pins,
                });
            }

            // f.format(&self.output.writer) catch |err| {
            //     log.warn("formatting failed err={}", .{err});
            //     return error.BenchmarkFailed;
            // };
            if let Err(err) = f.format(&mut self_.output) {
                // log.warn("formatting failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "formatting failed err={err}");
                return Err(Error::BenchmarkFailed);
            }
            // std.mem.doNotOptimizeAway(self.output.written());
            black_box(self_.output.as_slice());
        }
        Ok(())
    }

    /// Print the content dimensions and emitted output size. This shares
    /// the formatting code with format mode but deliberately makes no
    /// timing claims. Use it to compute cells/s and bytes/s from timings.
    // fn stepReport(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_report(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalFormatter) };

        self_.output.clear();
        // if (self.formatter()) |f_init| {
        if let Some(f_init) = self_.formatter() {
            // var f = f_init;
            let mut f = f_init;
            if self_.opts.pin_map {
                self_.pins.clear_retaining_capacity();
                f.pin_map = Some(formatterpkg::PinMap {
                    alloc: self_.alloc.into(),
                    map: &mut self_.pins,
                });
            }
            if let Err(err) = f.format(&mut self_.output) {
                // log.warn("formatting failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "formatting failed err={err}");
                return Err(Error::BenchmarkFailed);
            }
        }

        // Count the total pages and rows in the pagelist.
        // const screen = self.terminal.?.screens.active;
        let screen: &Screen = unsafe { &*self_.terminal.as_ref().unwrap().screens.active };
        let mut pages: usize = 0;
        let mut rows: usize = 0;
        {
            // var node = screen.pages.pages.first;
            // while (node) |n| : (node = n.next) {
            //     pages += 1;
            //     rows += n.page().size.rows;
            // }
            let mut node = screen.pages.pages.first;
            while !node.is_null() {
                pages += 1;
                rows += usize::from(unsafe { (*PageNode::page(node)).size.rows });
                node = unsafe { (*node).next() };
            }
        }

        // Hash the output (and the pin coordinates, which are stable across
        // runs unlike the node pointers) so different implementations can be
        // checked for identical output.
        // const out_hash = std.hash.Wyhash.hash(0, self.output.written());
        let out_hash = Wyhash::hash(0, &self_.output);
        // var pin_hasher = std.hash.Wyhash.init(0);
        let mut pin_hasher = Wyhash::init(0);
        // for (self.pins.points.items) |coord| {
        for coord in &self_.pins.points {
            // const x: u16 = coord.x;
            let x: u16 = coord.x;
            // const y: u16 = @intCast(coord.y);
            let y: u16 = u16::try_from(coord.y).unwrap();
            // pin_hasher.update(std.mem.asBytes(&x));
            pin_hasher.update(&x.to_ne_bytes());
            pin_hasher.update(&y.to_ne_bytes());
        }

        // std.debug.print("terminal-formatter emit={s} region={s} pages={d} rows={d} " ++
        //     "cols={d} cells={d} out_bytes={d} pin_bytes={d} out_hash={x} pin_hash={x}\n", ...);
        debug_print!(
            "terminal-formatter emit={} region={} pages={} rows={} \
             cols={} cells={} out_bytes={} pin_bytes={} \
             out_hash={:x} pin_hash={:x}\n",
            self_.opts.emit.tag_name(),
            self_.opts.region.tag_name(),
            pages,
            rows,
            self_.opts.terminal_cols,
            rows * usize::from(self_.opts.terminal_cols),
            self_.output.len(),
            self_.pins.count(),
            out_hash,
            pin_hasher.finalize(),
        );

        // Pin map storage details on a separate line so the main report
        // line remains comparable across implementations.
        if self_.opts.pin_map {
            // std.debug.print("terminal-formatter-pins points={d} nodes={d}\n", ...);
            debug_print!(
                "terminal-formatter-pins points={} nodes={}\n",
                self_.pins.points.len(),
                self_.pins.nodes.len(),
            );
        }
        Ok(())
    }

    // fn stepRoundtrip(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_roundtrip(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalFormatter) };
        // self.stepRoundtripImpl() catch |err| {
        //     log.warn("roundtrip failed err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        self_.step_roundtrip_impl().map_err(|err| {
            // log.warn("roundtrip failed err={}", .{err});
            log::warn!(target: LOG_SCOPE, "roundtrip failed err={err:?}");
            Error::BenchmarkFailed
        })
    }

    /// Format the terminal, replay the output into a fresh terminal of the
    /// same dimensions, format that, and require both outputs to be
    /// identical. This verifies that the emitted VT sequences faithfully
    /// reconstruct the terminal contents (text, styles, wrapping) without
    /// requiring any specific byte encoding of the first output.
    // fn stepRoundtripImpl(self: *TerminalFormatter) !void {
    fn step_roundtrip_impl(&mut self) -> Result<(), RoundtripError> {
        // Format the original terminal.
        self.output.clear();
        // if (self.formatter()) |f_init| {
        //     var f = f_init;
        //     try f.format(&self.output.writer);
        // }
        if let Some(f) = self.formatter() {
            f.format(&mut self.output)?;
        }
        // const first = self.output.written();
        //
        // PORT NOTE: `first` borrows `self.output`, which must stay untouched
        // while `t2` replays it (the reference relies on the same invariant).
        let first: &[u8] = &self.output;

        // Replay into a fresh terminal.
        // var t2 = try Terminal.init(global.io(), self.alloc, .{ ... });
        // defer t2.deinit(self.alloc);
        let mut t2 = Terminal::init(
            self.alloc.into(),
            rustty_vt::terminal::Options {
                max_scrollback_bytes: None,
                max_scrollback_lines: None,
                ..crate::terminal_options(self.opts.terminal_rows, self.opts.terminal_cols)
            },
        )?;
        {
            // var stream = t2.vtStream();
            // defer stream.deinit();
            let mut stream = t2.vt_stream();
            // stream.nextSlice(first);
            stream.next_slice(first);
            stream.deinit();
        }

        // Format the replayed terminal identically.
        // var out2: std.Io.Writer.Allocating = .init(self.alloc);
        // defer out2.deinit();
        let mut out2: Vec<u8> = Vec::new();
        // var f2: formatterpkg.ScreenFormatter = .init(t2.screens.active, .{
        //     .emit = self.opts.emit.format(),
        //     .unwrap = self.opts.unwrap,
        // });
        let f2 = formatterpkg::ScreenFormatter::init(t2.screens.active, self.formatter_options());
        // try f2.format(&out2.writer);
        let format_result = f2.format(&mut out2);
        t2.deinit(self.alloc.into());
        format_result?;
        // const second = out2.written();
        let second: &[u8] = &out2;

        // const equal = std.mem.eql(u8, first, second);
        let equal = first == second;
        // std.debug.print("terminal-formatter roundtrip emit={s} bytes={d} replay_bytes={d} equal={}\n", ...);
        debug_print!(
            "terminal-formatter roundtrip emit={} bytes={} replay_bytes={} equal={}\n",
            self.opts.emit.tag_name(),
            first.len(),
            second.len(),
            equal,
        );

        if !equal {
            // Find the first differing offset to ease debugging.
            // const n = @min(first.len, second.len);
            let n = core::cmp::min(first.len(), second.len());
            let mut i: usize = 0;
            while i < n && first[i] == second[i] {
                i += 1;
            }
            // std.debug.print("terminal-formatter roundtrip mismatch offset={d} first={f} second={f}\n", ...);
            //
            // `i -| 32` is saturating.
            debug_print!("{}", roundtrip_mismatch_line(i, first, second));
            // return error.RoundtripMismatch;
            return Err(RoundtripError::RoundtripMismatch);
        }
        Ok(())
    }
}

/// The roundtrip mismatch diagnostic line (`TerminalFormatter.zig:415-423`):
/// `std.debug.print("terminal-formatter roundtrip mismatch offset={d}
/// first={f} second={f}\n", .{ i, std.zig.fmtString(first[i -| 32..@min(first.len,
/// i + 32)]), std.zig.fmtString(second[..]) })`.
//
// PORT NOTE (parity S62-F7): `std.zig.fmtString` is `std.zig.stringEscape`,
// ported as `rustty_args::args::zig_string_escape`. CORRECTION: this used
// `<[u8]>::escape_ascii`, which also escapes `'` and spells other bytes
// differently.
fn roundtrip_mismatch_line(i: usize, first: &[u8], second: &[u8]) -> String {
    let mut out = format!("terminal-formatter roundtrip mismatch offset={i} first=").into_bytes();
    rustty_args::args::zig_string_escape(
        &mut out,
        &first[i.saturating_sub(32)..core::cmp::min(first.len(), i + 32)],
    );
    out.extend_from_slice(b" second=");
    rustty_args::args::zig_string_escape(
        &mut out,
        &second[i.saturating_sub(32)..core::cmp::min(second.len(), i + 32)],
    );
    out.push(b'\n');
    // `stringEscape` emits only printable ASCII.
    String::from_utf8(out).expect("zig_string_escape output is ASCII")
}

/// The inferred error set of `setupImpl` (`!void`): terminal allocation
/// failures and data-file I/O errors.
///
/// The payloads are consumed by the `{err:?}` in `setup`'s warn (the
/// reference's `log.warn("... err={}", .{err})`); rustc's dead-code pass
/// deliberately ignores derived `Debug` as a use, hence the allow (the
/// page_compression precedent).
#[derive(Debug)]
#[allow(dead_code)]
enum SetupError {
    OutOfMemory,
    Io(std::io::Error),
}

impl From<OutOfMemory> for SetupError {
    fn from(_: OutOfMemory) -> SetupError {
        SetupError::OutOfMemory
    }
}

impl From<std::io::Error> for SetupError {
    fn from(e: std::io::Error) -> SetupError {
        SetupError::Io(e)
    }
}

/// The inferred error set of `stepRoundtripImpl` (`!void`).
///
/// The payloads are consumed by the `{err:?}` in `stepRoundtrip`'s warn;
/// rustc's dead-code pass deliberately ignores derived `Debug` as a use,
/// hence the allow (the page_compression precedent).
#[derive(Debug)]
#[allow(dead_code)]
enum RoundtripError {
    OutOfMemory,
    // try f.format(...) — the formatter writes into a Vec (WriteFailed class).
    Format(std::io::Error),
    // return error.RoundtripMismatch;
    RoundtripMismatch,
}

impl From<OutOfMemory> for RoundtripError {
    fn from(_: OutOfMemory) -> RoundtripError {
        RoundtripError::OutOfMemory
    }
}

impl From<std::io::Error> for RoundtripError {
    fn from(e: std::io::Error) -> RoundtripError {
        RoundtripError::Format(e)
    }
}

// test TerminalFormatter {
#[test]
fn terminal_formatter() {
    let alloc = Allocator;
    // const impl: *TerminalFormatter = try .create(testing.allocator, .{});
    let mut impl_ = TerminalFormatter::create(alloc, Options::default()).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

// test "TerminalFormatter roundtrip" {
#[test]
fn terminal_formatter_roundtrip() {
    let alloc = Allocator;
    let mut impl_ = TerminalFormatter::create(
        alloc,
        Options {
            mode: Mode::Roundtrip,
            terminal_rows: 4,
            terminal_cols: 8,
            ..Options::default()
        },
    )
    .unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

// test "TerminalFormatter formats all emit formats and regions" {
#[test]
fn terminal_formatter_formats_all_emit_formats_and_regions() {
    let alloc = Allocator;

    // inline for (.{ Emit.plain, Emit.vt, Emit.html }) |emit| {
    //     inline for (.{ Region.screen, Region.active, Region.history }) |region| {
    for emit in [Emit::Plain, Emit::Vt, Emit::Html] {
        for region in [Region::Screen, Region::Active, Region::History] {
            let mut impl_ = TerminalFormatter::create(
                alloc,
                Options {
                    emit,
                    region,
                    loops: 1,
                    terminal_rows: 4,
                    terminal_cols: 8,
                    ..Options::default()
                },
            )
            .unwrap();
            let bench = impl_.benchmark();
            let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
            impl_.destroy(alloc);
        }
    }
}

// test "TerminalFormatter pin map" {
#[test]
fn terminal_formatter_pin_map() {
    let alloc = Allocator;
    let mut impl_ = TerminalFormatter::create(
        alloc,
        Options {
            pin_map: true,
            loops: 1,
            terminal_rows: 4,
            terminal_cols: 8,
            ..Options::default()
        },
    )
    .unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for TerminalFormatter {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(TerminalFormatter::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        TerminalFormatter::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        TerminalFormatter::benchmark(self)
    }
}

/// Parity S62-F7: the mismatch windows render as `std.zig.fmtString` does
/// (`'` bare, `"` and `\` backslash-escaped, other non-printables `\xNN` in
/// lower-case hex), and the window is the saturating `i -| 32 .. i + 32`.
#[test]
fn terminal_formatter_roundtrip_mismatch_line_uses_zig_fmt_string() {
    let first: &[u8] = b"a'b\"c\x1bd\\\t";
    let second: &[u8] = b"a'b\"c\x1bE\\\t";
    assert_eq!(
        "terminal-formatter roundtrip mismatch offset=6 \
         first=a'b\\\"c\\x1bd\\\\\\t second=a'b\\\"c\\x1bE\\\\\\t\n",
        roundtrip_mismatch_line(6, first, second),
    );

    // The window starts 32 bytes before the first difference.
    let long_first = [b'x'; 40];
    let mut long_second = [b'x'; 40];
    long_second[35] = b'y';
    let line = roundtrip_mismatch_line(35, &long_first, &long_second);
    assert_eq!(
        format!(
            "terminal-formatter roundtrip mismatch offset=35 first={} second={}y{}\n",
            "x".repeat(37),
            "x".repeat(32),
            "x".repeat(4),
        ),
        line,
    );
}
