//! This benchmark tests the performance of Terminal.resize, with a
//! primary focus on column resizes that reflow soft-wrapped text.
//! Resize happens on the IO thread while holding the terminal lock,
//! so a slow resize directly translates into dropped input and a
//! frozen-feeling UI while the user drags the window edge (which
//! produces a rapid stream of resizes).
//!
//! The terminal is populated once during setup (synthetic fill and/or
//! a data file replayed through the VT stream) and then each step
//! ping-pongs the terminal between two sizes. A full cycle returns the
//! terminal to its original dimensions so the state reaches a steady
//! state after the first cycle and every iteration performs
//! equivalent work.
// const TerminalResize = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const Terminal = terminalpkg.Terminal;
// const global = @import("../global.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::terminal::{Resize, ResizeCellSize, Terminal};
// PORT NOTE (dc52c248e): `std.Random.DefaultPrng` (Xoshiro256++) is the
// workspace's std.Random port in rustty-synthetic (`rand.rs`) — bit-exact, so
// the deterministic fill draws the same sequence as the reference.
use rustty_synthetic::rand::DefaultPrng;

// const log = std.log.scoped(.@"terminal-resize-bench");
const LOG_SCOPE: &str = "terminal-resize-bench";

// opts: Options,
// alloc: Allocator,
// terminal: Terminal,
pub struct TerminalResize {
    opts: Options,
    alloc: Allocator,
    terminal: Terminal,
}

// pub const Options = struct {
pub struct Options {
    /// The resize pattern to benchmark. See Mode.
    // mode: Mode = .cols,
    mode: Mode,

    /// Multiplier on the number of resize cycles each step runs. This
    /// is useful to make a benchmark run long enough for profiling.
    // loops: u32 = 1,
    loops: u32,

    /// The initial size of the terminal. This is also the size that
    /// every resize cycle returns to.
    // @"terminal-rows": u16 = 80,
    // @"terminal-cols": u16 = 120,
    terminal_rows: u16,
    terminal_cols: u16,

    /// The dimensions to resize to for the cols/rows/both modes. If
    /// unset, they default to half of the respective terminal
    /// dimension, which forces every soft-wrapped line to rewrap.
    // @"resize-cols": ?u16 = null,
    // @"resize-rows": ?u16 = null,
    resize_cols: Option<u16>,
    resize_rows: Option<u16>,

    /// The number of synthetic lines written to the terminal during
    /// setup. The content is deterministic: a mix of short lines,
    /// soft-wrapped long lines, blank lines, styled cells, and wide
    /// characters, since all of these hit different reflow paths.
    /// Set to 0 to only use `data`.
    // @"fill-lines": u32 = 10_000,
    fill_lines: u32,

    /// The maximum scrollback size in bytes. Defaults to the Rustty
    /// application default. Reflow cost scales with the amount of
    /// scrollback, not just the visible screen.
    // @"scrollback-bytes": usize = 50_000_000,
    scrollback_bytes: usize,

    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, only the synthetic fill
    /// is used. The data is streamed into the terminal during setup
    /// (not part of the benchmark) to build the screen contents that
    /// get resized.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Cols,
            loops: 1,
            terminal_rows: 80,
            terminal_cols: 120,
            resize_cols: None,
            resize_rows: None,
            fill_lines: 10_000,
            scrollback_bytes: 50_000_000,
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
            // mode: Mode
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
            // loops: u32
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
            b"resize-cols" => {
                self.resize_cols = Some(rustty_args::args::parse_field_int(value)?);
                Ok(())
            }
            b"resize-rows" => {
                self.resize_rows = Some(rustty_args::args::parse_field_int(value)?);
                Ok(())
            }
            b"fill-lines" => {
                self.fill_lines = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            b"scrollback-bytes" => {
                self.scrollback_bytes = rustty_args::args::parse_field_int(value)?;
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
    /// Resize to the current dimensions. This measures the early-exit
    /// path (mode updates, pixel geometry) and acts as a baseline.
    Noop,

    /// Alternate the column count between `terminal-cols` and
    /// `resize-cols`. With wraparound enabled (the default), this
    /// reflows text in both directions: shrinking wraps long lines
    /// and growing unwraps them. This is the primary reflow benchmark.
    Cols,

    /// Like `cols`, but with wraparound mode disabled so the resize
    /// does not reflow. Useful as a baseline to isolate the cost of
    /// reflow itself from the rest of the resize.
    // @"cols-no-reflow",
    ColsNoReflow,

    /// Alternate the row count between `terminal-rows` and
    /// `resize-rows`. Column count is unchanged so no text reflows;
    /// this measures growing/trimming rows against scrollback.
    Rows,

    /// Alternate both dimensions at once, like a diagonal window drag.
    Both,
}

impl Mode {
    /// The kebab CLI tokens are the enum tag names, preserved exactly.
    pub fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"cols" => Some(Mode::Cols),
            b"cols-no-reflow" => Some(Mode::ColsNoReflow),
            b"rows" => Some(Mode::Rows),
            b"both" => Some(Mode::Both),
            _ => None,
        }
    }
}

impl TerminalResize {
    // pub fn create(alloc: Allocator, opts: Options) !*TerminalResize {
    //     const ptr = try alloc.create(TerminalResize);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{
    //         .opts = opts,
    //         .alloc = alloc,
    //         .terminal = try .init(global.io(), alloc, .{
    //             .rows = opts.@"terminal-rows",
    //             .cols = opts.@"terminal-cols",
    //             .max_scrollback_bytes = opts.@"scrollback-bytes",
    //         }),
    //     };
    //     return ptr;
    // }
    //
    // PORT NOTE: the reference relies on `Terminal.Options` field defaults for
    // everything but rows/cols/max_scrollback_bytes; `crate::terminal_options`
    // reproduces those defaults, and only the scrollback bound differs here
    // (the reference passes the option instead of the 10_000 struct default).
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<TerminalResize>, OutOfMemory> {
        let terminal = Terminal::init(
            alloc,
            rustty_vt::terminal::Options {
                max_scrollback_bytes: Some(opts.scrollback_bytes),
                ..crate::terminal_options(opts.terminal_rows, opts.terminal_cols)
            },
        )?;
        Ok(Box::new(TerminalResize {
            opts,
            alloc,
            terminal,
        }))
    }

    // pub fn destroy(self: *TerminalResize, alloc: Allocator) void {
    //     self.terminal.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<TerminalResize>, alloc: Allocator) {
        self.terminal.deinit(alloc);
        drop(self);
    }

    // pub fn benchmark(self: *TerminalResize) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) {
                //     .noop => stepNoop,
                //     .cols, .@"cols-no-reflow" => stepCols,
                //     .rows => stepRows,
                //     .both => stepBoth,
                // },
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Cols | Mode::ColsNoReflow => Self::step_cols,
                    Mode::Rows => Self::step_rows,
                    Mode::Both => Self::step_both,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: None,
            },
        )
    }

    /// The column count used by the cols/both modes.
    // fn targetCols(self: *const TerminalResize) u16 {
    fn target_cols(&self) -> u16 {
        // return self.opts.@"resize-cols" orelse @max(1, self.opts.@"terminal-cols" / 2);
        self.opts
            .resize_cols
            .unwrap_or_else(|| core::cmp::max(1, self.opts.terminal_cols / 2))
    }

    /// The row count used by the rows/both modes.
    // fn targetRows(self: *const TerminalResize) u16 {
    fn target_rows(&self) -> u16 {
        // return self.opts.@"resize-rows" orelse @max(1, self.opts.@"terminal-rows" / 2);
        self.opts
            .resize_rows
            .unwrap_or_else(|| core::cmp::max(1, self.opts.terminal_rows / 2))
    }

    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalResize) };

        // Always reset our terminal state. Note this doesn't resize, but
        // create initializes (and steps return) the terminal to the
        // requested dimensions so we're always at terminal-rows/cols here.
        // self.terminal.fullReset();
        self_.terminal.full_reset();
        assert!(self_.terminal.cols == self_.opts.terminal_cols);
        assert!(self_.terminal.rows == self_.opts.terminal_rows);

        // Fill with synthetic content first, then replay the data file on
        // top if given. Both go through the VT stream so soft wraps,
        // styles, etc. are all set exactly as they would be in a real
        // session.
        self_.fill();
        self_.replay_data()?;

        // Reflow only happens when wraparound mode is set (it is by
        // default). We only disable it after filling so the fill itself
        // still soft-wraps identically in every mode.
        if self_.opts.mode == Mode::ColsNoReflow {
            // self.terminal.modes.set(.wraparound, false);
            self_
                .terminal
                .modes
                .set(rustty_vt::modes::Mode::Wraparound, false);
        }
        Ok(())
    }

    /// Write deterministic synthetic content to the terminal. The goal is
    /// content that is representative of a real session so that reflow
    /// touches its interesting paths: soft-wrapped lines (must rewrap),
    /// short lines (copied as-is), blank lines, styled cells (styles must
    /// be moved across pages), and wide characters (can't be split at the
    /// wrap column).
    // fn fill(self: *TerminalResize) void {
    fn fill(&mut self) {
        // if (self.opts.@"fill-lines" == 0) return;
        if self.opts.fill_lines == 0 {
            return;
        }

        // PORT NOTE: the reference reads `self.terminal.cols` while the stream
        // is live; `vt_stream` borrows `&mut self.terminal`, so the reads are
        // hoisted above it (same values — the stream cannot resize).
        let fill_lines = self.opts.fill_lines;
        // const cols: usize = self.terminal.cols;
        let cols: usize = usize::from(self.terminal.cols);

        // var s = self.terminal.vtStream();
        // defer s.deinit();
        let mut s = self.terminal.vt_stream();

        // var prng: std.Random.DefaultPrng = .init(0xB3);
        // const rand = prng.random();
        let prng = DefaultPrng::init(0xB3);
        let rand = prng.random();

        // var line_buf: [8192]u8 = undefined;
        let mut line_buf = [0u8; 8192];

        // for (0..self.opts.@"fill-lines") |i| {
        for i in 0..fill_lines as usize {
            // Periodically toggle a background style so reflow has to
            // carry styled cells into new pages.
            if i % 64 == 0 {
                s.next_slice(b"\x1b[48;2;20;40;60m");
            }
            if i % 64 == 32 {
                s.next_slice(b"\x1b[m");
            }

            // A small portion of lines are blank.
            if i % 16 == 15 {
                s.next_slice(b"\r\n");
                continue;
            }

            // Line lengths between ~25% and ~250% of the terminal width
            // so we get a mix of short lines and soft-wrapped lines.
            // const min = @max(1, cols / 4);
            let min = core::cmp::max(1, cols / 4);
            // const max = @min(line_buf.len, cols * 5 / 2);
            let max = core::cmp::min(line_buf.len(), cols * 5 / 2);
            // const len = min + rand.uintLessThan(usize, max - min);
            let len = min + rand.uint_less_than::<usize>(max - min);

            // var j: usize = 0;
            let mut j: usize = 0;
            while j < len {
                // Sprinkle wide characters into every 8th line.
                if i % 8 == 7 && j % 16 == 8 && j + 3 <= len {
                    // line_buf[j..][0..3].* = "漢".*;
                    line_buf[j..j + 3].copy_from_slice("漢".as_bytes());
                    j += 3;
                    continue;
                }

                // Words of ASCII separated by spaces.
                // line_buf[j] = if (j % 8 == 7) ' ' else rand.intRangeAtMost(u8, 'a', 'z');
                line_buf[j] = if j % 8 == 7 {
                    b' '
                } else {
                    rand.int_range_at_most::<u8>(b'a', b'z')
                };
                j += 1;
            }

            // s.nextSlice(line_buf[0..len]);
            s.next_slice(&line_buf[..len]);
            s.next_slice(b"\r\n");
        }

        s.deinit();
    }

    /// Stream the data file (if any) into the terminal.
    // fn replayData(self: *TerminalResize) Benchmark.Error!void {
    fn replay_data(&mut self) -> Result<(), Error> {
        // const data_f: std.Io.File = (options.dataFile(self.opts.data) catch |err| {
        //     log.warn("error opening data file err={}", .{err});
        //     return error.BenchmarkFailed;
        // }) orelse return;
        // defer data_f.close(global.io());
        let mut data_f = match options::data_file(self.opts.data.as_deref()) {
            Ok(Some(f)) => f,
            Ok(None) => return Ok(()),
            Err(err) => {
                // log.warn("error opening data file err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error opening data file err={err}");
                return Err(Error::BenchmarkFailed);
            }
        };

        // var stream = self.terminal.vtStream();
        // defer stream.deinit();
        let mut stream = self.terminal.vt_stream();

        // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        // var f_reader = data_f.reader(global.io(), &read_buf);
        // const r = &f_reader.interface;
        //
        // PORT NOTE (§8, terminal_stream precedent): `read_slice_short` reads
        // straight from the `DataFile`, so the intermediate reader buffer is
        // structural; `buf` below is the dispatch buffer that matters.
        let r = &mut data_f;

        // var buf: [4096]u8 = undefined;
        let mut buf = [0u8; 4096];
        // while (true) {
        loop {
            // const n = r.readSliceShort(&buf) catch {
            //     log.warn("error reading data file err={?}", .{f_reader.err});
            //     return error.BenchmarkFailed;
            // };
            let n = match read_slice_short(r, &mut buf) {
                Ok(n) => n,
                Err(err) => {
                    // log.warn("error reading data file err={?}", .{f_reader.err});
                    log::warn!(target: LOG_SCOPE, "error reading data file err={err:?}");
                    stream.deinit();
                    return Err(Error::BenchmarkFailed);
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
        Ok(())
    }

    // fn resizeTerminal(self: *TerminalResize, cols: u16, rows: u16) Benchmark.Error!void {
    fn resize_terminal(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        // self.terminal.resize(self.alloc, .{
        //     .cols = cols,
        //     .rows = rows,
        //     .cell_size_px = .{ .width = 10, .height = 20 },
        // }) catch |err| { ... };
        if let Err(err) = self.terminal.resize(
            self.alloc,
            Resize {
                cols,
                rows,
                // Realistic cell pixel geometry: real apprt resizes always
                // carry it, and it exercises the pixel dimension updates.
                cell_size_px: Some(ResizeCellSize {
                    width: 10,
                    height: 20,
                }),
            },
        ) {
            // log.warn("error resizing terminal err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error resizing terminal err={err:?}");
            return Err(Error::BenchmarkFailed);
        }
        // std.mem.doNotOptimizeAway(&self.terminal);
        black_box(&self.terminal);
        Ok(())
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalResize) };

        let cols = self_.opts.terminal_cols;
        let rows = self_.opts.terminal_rows;

        // We loop because it's so fast (a few ns) that a single resize
        // doesn't properly capture our speeds.
        // for (0..50_000_000 * @as(u64, self.opts.loops)) |_| {
        for _ in 0..50_000_000u64 * u64::from(self_.opts.loops) {
            self_.resize_terminal(cols, rows)?;
        }
        Ok(())
    }

    // fn stepCols(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_cols(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalResize) };

        let cols = self_.opts.terminal_cols;
        let rows = self_.opts.terminal_rows;
        let target = self_.target_cols();

        // Per-cycle cost differs by orders of magnitude between the two
        // modes (a reflow resize walks the entire scrollback; a non-reflow
        // resize doesn't), so pick a cycle count that makes each run long
        // enough to measure well above process startup and setup cost.
        // const cycles: u64 = switch (self.opts.mode) {
        //     .cols => 25,
        //     .@"cols-no-reflow" => 1_500,
        //     else => unreachable,
        // };
        let cycles: u64 = match self_.opts.mode {
            Mode::Cols => 25,
            Mode::ColsNoReflow => 1_500,
            _ => unreachable!(),
        };

        // Each cycle shrinks (rewrapping long lines) and grows back
        // (unwrapping them), ending at the original size.
        for _ in 0..cycles * u64::from(self_.opts.loops) {
            self_.resize_terminal(target, rows)?;
            self_.resize_terminal(cols, rows)?;
        }
        Ok(())
    }

    // fn stepRows(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_rows(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalResize) };

        let cols = self_.opts.terminal_cols;
        let rows = self_.opts.terminal_rows;
        let target = self_.target_rows();

        // Row-only resizes don't reflow so they're much cheaper (tens of
        // ns); loop a lot more so the measurement isn't dominated by
        // process overhead.
        // for (0..2_500_000 * @as(u64, self.opts.loops)) |_| {
        for _ in 0..2_500_000u64 * u64::from(self_.opts.loops) {
            self_.resize_terminal(cols, target)?;
            self_.resize_terminal(cols, rows)?;
        }
        Ok(())
    }

    // fn stepBoth(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_both(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalResize) };

        let cols = self_.opts.terminal_cols;
        let rows = self_.opts.terminal_rows;
        let target_cols = self_.target_cols();
        let target_rows = self_.target_rows();

        // for (0..25 * @as(u64, self.opts.loops)) |_| {
        for _ in 0..25u64 * u64::from(self_.opts.loops) {
            self_.resize_terminal(target_cols, target_rows)?;
            self_.resize_terminal(cols, rows)?;
        }
        Ok(())
    }
}

// test TerminalResize {
#[test]
fn terminal_resize() {
    let alloc = Allocator;

    // Small dimensions and fill so this is fast in debug builds while
    // still exercising real reflow in both directions.
    // const impl: *TerminalResize = try .create(alloc, .{
    //     .mode = .cols,
    //     .@"terminal-rows" = 10,
    //     .@"terminal-cols" = 20,
    //     .@"fill-lines" = 50,
    // });
    let mut impl_ = TerminalResize::create(
        alloc,
        Options {
            mode: Mode::Cols,
            terminal_rows: 10,
            terminal_cols: 20,
            fill_lines: 50,
            ..Options::default()
        },
    )
    .unwrap();

    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for TerminalResize {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(TerminalResize::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        TerminalResize::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        TerminalResize::benchmark(self)
    }
}
