//! This benchmark tests the performance of the Screen.clone
//! function. This is useful because it is one of the primary lock
//! holders that impact IO performance when the renderer is active.
//! We do this very frequently.
// const ScreenClone = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const Terminal = terminalpkg.Terminal;
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short};
use crate::terminal_options;
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_vt::pagelist::{OutOfMemory, PageList};
use rustty_vt::point::{Coordinate, Point};
use rustty_vt::render::{Overscan, RenderState};
use rustty_vt::screen::Screen;
use rustty_vt::screen_set::Key;
use rustty_vt::terminal::{ScrollViewport, Terminal};

const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// terminal: Terminal,
pub struct ScreenClone {
    opts: Options,
    terminal: Terminal,
    /// UD376: the `clone` mode's copies, kept so teardown can free them. The
    /// reference says "We'll free when the benchmark exits" but never does:
    /// every step leaks 1000 screens until the process ends.
    clones: Vec<Screen>,
}

// pub const Options = struct {
pub struct Options {
    /// The type of codepoint width calculation to use.
    // mode: Mode = .clone,
    mode: Mode,

    /// Multiplier on the number of iterations each step runs. This is
    /// useful to make a benchmark run long enough for profiling.
    // loops: u32 = 1,
    loops: u32,

    /// The size of the terminal. This affects benchmarking when
    /// dealing with soft line wrapping and the memory impact
    /// of page sizes.
    // @"terminal-rows": u16 = 80,
    // @"terminal-cols": u16 = 120,
    terminal_rows: u16,
    terminal_cols: u16,

    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, then we will
    /// do nothing (benchmark is a noop). It'd be more unixy to
    /// use stdin by default but I find that a hanging CLI command
    /// with no interaction is a bit annoying.
    ///
    /// This will be used to initialize the terminal screen state before
    /// cloning. This data can switch to alt screen if it wants. The time
    /// to read this is not part of the benchmark.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            // mode: Mode = .clone,
            mode: Mode::Clone,
            // loops: u32 = 1,
            loops: 1,
            // @"terminal-rows": u16 = 80,
            terminal_rows: 80,
            // @"terminal-cols": u16 = 120,
            terminal_cols: 120,
            data: None,
        }
    }
}

// pub const Mode = enum {
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The baseline mode copies the screen by value.
    // noop,
    Noop,

    /// Full clone
    // clone,
    Clone,

    /// RenderState rather than a screen clone.
    // render,
    Render,

    /// Like render, but only the portion of the render state update
    /// that requires holding a terminal lock (begin_update). The
    /// deferred work (end_update) is excluded since it happens outside
    /// of any locks.
    // @"render-locked",
    RenderLocked,

    /// RenderState update with no changes to the terminal. This is
    /// the common case for a renderer that is redrawing frames (e.g.
    /// cursor blink, mouse movement) without terminal changes.
    // @"render-clean",
    RenderClean,

    /// RenderState update where a single row is dirty. This models the
    /// common case of a shell prompt or TUI updating a small portion
    /// of the screen between frames.
    // @"render-partial",
    RenderPartial,
    //
    // T12 recorded the three modes above as DEFERRED TO T19, because all three
    // call `RenderState::beginUpdate`, which did not exist yet. T19 landed the
    // begin/end split, so they are landed here — the deferral is discharged.
    /// RenderState update after scrolling the viewport by one row.
    /// This is the per-frame cost of user-driven scrolling through
    /// scrollback (e.g. a trackpad fling), which today changes the
    /// viewport pin and forces a full rebuild.
    // @"render-scroll",
    RenderScroll,

    /// RenderState update after scrolling the viewport by half a
    /// screen. This models paging (page up/down, wheel ticks with
    /// large multipliers) rather than smooth scrolling.
    // @"render-scroll-page",
    RenderScrollPage,

    /// RenderState update after one new line of output is written
    /// while the viewport follows the active area. This models a
    /// program streaming output. Each frame the viewport pin moves
    /// down one row.
    // @"render-output",
    RenderOutput,

    /// The render-scroll, render-output, and render-clean modes with
    /// overscan (`RenderState.overscan_request`) of 4 rows above and 1
    /// below. This models a renderer that captures extra rows so it can
    /// draw partially visible rows while smooth scrolling.
    // @"render-scroll-overscan",
    // @"render-output-overscan",
    // @"render-clean-overscan",
    RenderScrollOverscan,
    RenderOutputOverscan,
    RenderCleanOverscan,
}

/// The overscan request used by the overscan modes.
// const bench_overscan: terminalpkg.RenderState.Overscan = .{ .above = 4, .below = 1 };
const BENCH_OVERSCAN: Overscan = Overscan { above: 4, below: 1 };

/// The overscan request for the given mode.
// fn overscanRequest(mode: Mode) terminalpkg.RenderState.Overscan {
//     return switch (mode) {
//         .@"render-scroll-overscan",
//         .@"render-output-overscan",
//         .@"render-clean-overscan",
//         => bench_overscan,
//         else => .{},
//     };
// }
fn overscan_request(mode: Mode) -> Overscan {
    match mode {
        Mode::RenderScrollOverscan | Mode::RenderOutputOverscan | Mode::RenderCleanOverscan => {
            BENCH_OVERSCAN
        }
        _ => Overscan { above: 0, below: 0 },
    }
}

/// Number of scrollback lines written during setup so that the scroll
/// modes have room to move in both directions.
// const scroll_setup_lines = 4000;
const SCROLL_SETUP_LINES: usize = 4000;

/// Direction changes for the scroll modes, in iterations.
// const scroll_reverse_interval = 2000;
// const scroll_page_reverse_interval = 40;
const SCROLL_REVERSE_INTERVAL: usize = 2000;
const SCROLL_PAGE_REVERSE_INTERVAL: usize = 40;

impl Mode {
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"clone" => Some(Mode::Clone),
            b"render" => Some(Mode::Render),
            b"render-locked" => Some(Mode::RenderLocked),
            b"render-clean" => Some(Mode::RenderClean),
            b"render-partial" => Some(Mode::RenderPartial),
            b"render-scroll" => Some(Mode::RenderScroll),
            b"render-scroll-page" => Some(Mode::RenderScrollPage),
            b"render-output" => Some(Mode::RenderOutput),
            b"render-scroll-overscan" => Some(Mode::RenderScrollOverscan),
            b"render-output-overscan" => Some(Mode::RenderOutputOverscan),
            b"render-clean-overscan" => Some(Mode::RenderCleanOverscan),
            _ => None,
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
            // loops: u32
            b"loops" => {
                self.loops = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            // @"terminal-rows": u16
            b"terminal-rows" => {
                self.terminal_rows = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            // @"terminal-cols": u16
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

impl ScreenClone {
    // pub fn create(alloc: Allocator, opts: Options) !*ScreenClone {
    //     const ptr = try alloc.create(ScreenClone);
    //     errdefer alloc.destroy(ptr);
    //
    //     var terminal_opts: Terminal.Options = .{
    //         .rows = opts.@"terminal-rows",
    //         .cols = opts.@"terminal-cols",
    //     };
    //
    //     // The scroll modes need real scrollback to move through. ...
    //     switch (opts.mode) { ... => terminal_opts.max_scrollback_bytes = 256 * 1024 * 1024, else => {} }
    //
    //     ptr.* = .{
    //         .opts = opts,
    //         .terminal = try .init(global.io(), alloc, terminal_opts),
    //     };
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<ScreenClone>, OutOfMemory> {
        let mut terminal_opts = terminal_options(opts.terminal_rows, opts.terminal_cols);

        // The scroll modes need real scrollback to move through. The
        // default limit is small enough that the setup lines would be
        // pruned immediately.
        match opts.mode {
            Mode::RenderScroll | Mode::RenderScrollPage | Mode::RenderScrollOverscan => {
                terminal_opts.max_scrollback_bytes = Some(256 * 1024 * 1024);
            }
            _ => {}
        }

        let terminal = Terminal::init(alloc.into(), terminal_opts)?;
        Ok(Box::new(ScreenClone {
            opts,
            terminal,
            clones: Vec::new(),
        }))
    }

    // pub fn destroy(self: *ScreenClone, alloc: Allocator) void {
    //     self.terminal.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<ScreenClone>, alloc: Allocator) {
        // UD376: any copies a run left behind (teardown normally frees them).
        for mut copy in self.clones.drain(..) {
            copy.deinit();
        }
        self.terminal.deinit(alloc.into());
        drop(self);
    }

    // pub fn benchmark(self: *ScreenClone) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) {
                //     .noop => stepNoop, .clone => stepClone, .render => stepRender,
                // },
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Clone => Self::step_clone,
                    Mode::Render => Self::step_render,
                    Mode::RenderLocked => Self::step_render_locked,
                    Mode::RenderClean | Mode::RenderCleanOverscan => Self::step_render_clean,
                    Mode::RenderPartial => Self::step_render_partial,
                    Mode::RenderScroll | Mode::RenderScrollOverscan => Self::step_render_scroll,
                    Mode::RenderScrollPage => Self::step_render_scroll_page,
                    Mode::RenderOutput | Mode::RenderOutputOverscan => Self::step_render_output,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // Always reset our terminal state
        // self.terminal.fullReset();
        self_.terminal.full_reset();

        // Force a style on every single row, which
        // var s = self.terminal.vtStream();
        // defer s.deinit();
        // s.nextSlice("\x1b[48;2;20;40;60m");
        //
        // // The scroll modes need scrollback above the screen so the
        // // viewport has somewhere to go.
        // switch (self.opts.mode) { ... => for (0..scroll_setup_lines) |_| s.nextSlice("hello\r\n"), else => {} }
        //
        // for (0..self.terminal.rows - 1) |_| s.nextSlice("hello\r\n");
        // s.nextSlice("hello");
        {
            let rows = self_.terminal.rows;
            let mode = self_.opts.mode;
            let mut s = self_.terminal.vt_stream();
            s.next_slice(b"\x1b[48;2;20;40;60m");

            // The scroll modes need scrollback above the screen so the
            // viewport has somewhere to go.
            match mode {
                Mode::RenderScroll | Mode::RenderScrollPage | Mode::RenderScrollOverscan => {
                    for _ in 0..SCROLL_SETUP_LINES {
                        s.next_slice(b"hello\r\n");
                    }
                }
                _ => {}
            }

            for _ in 0..rows - 1 {
                s.next_slice(b"hello\r\n");
            }
            s.next_slice(b"hello");
            s.deinit();
        }

        // Setup our terminal state
        // const data_f: std.fs.File = (options.dataFile(self.opts.data) catch |err| {
        //     log.warn("error opening data file err={}", .{err});
        //     return error.BenchmarkFailed;
        // }) orelse return;
        let mut data_f = match options::data_file(self_.opts.data.as_deref()) {
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
        let mut stream = self_.terminal.vt_stream();

        // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        // var f_reader = data_f.reader(&read_buf);
        // const r = &f_reader.interface;
        //
        // var buf: [4096]u8 = undefined;
        let mut buf = [0u8; 4096];
        // while (true) {
        let result = loop {
            // const n = r.readSliceShort(&buf) catch { ... return error.BenchmarkFailed; };
            let n = match read_slice_short(&mut data_f, &mut buf) {
                Ok(n) => n,
                Err(err) => {
                    // log.warn("error reading data file err={?}", .{f_reader.err});
                    log::warn!(target: LOG_SCOPE, "error reading data file err={err:?}");
                    break Err(Error::BenchmarkFailed);
                }
            };
            // if (n == 0) break; // EOF reached
            if n == 0 {
                break Ok(());
            }
            // stream.nextSlice(buf[0..n]);
            stream.next_slice(&buf[..n]);
        };
        stream.deinit();
        result
    }

    // fn teardown(ptr: *anyopaque) void {
    //     const self: *ScreenClone = @ptrCast(@alignCast(ptr));
    //     _ = self;
    // }
    //
    // UD376: the `clone` step's copies are freed here, outside the timed loop
    // (`Benchmark::run` takes its end time before the teardown guard runs). This
    // is the "We'll free when the benchmark exits" that `stepClone` promises and
    // the reference never does.
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };
        for mut copy in self_.clones.drain(..) {
            copy.deinit();
        }
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // We loop because its so fast that a single benchmark run doesn't
        // properly capture our speeds.
        // for (0..1000) |_| {
        for _ in 0..1000 {
            // const s: terminalpkg.Screen = self.terminal.screens.active.*;
            // std.mem.doNotOptimizeAway(s);
            //
            // PORT NOTE (§7.5): the reference does a shallow by-value struct copy
            // of `*active` that is NEVER deinit'd (it aliases the same backing
            // pages). In Rust `Screen` owns heap and is not `Copy`, so the
            // faithful equivalent is `ptr::read` (a bitwise shallow copy) plus
            // `mem::forget` so the aliasing copy is not dropped (no double-free).
            let s: Screen = unsafe { core::ptr::read(self_.terminal.screens.active) };
            black_box(&s);
            core::mem::forget(s);
        }
        Ok(())
    }

    // fn stepClone(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_clone(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // UD376: room for this step's copies, reserved before the loop so the
        // loop itself never reallocates.
        self_.clones.reserve(1000);

        // We loop because its so fast that a single benchmark run doesn't
        // properly capture our speeds.
        // for (0..1000) |_| {
        for _ in 0..1000 {
            // const s: *terminalpkg.Screen = self.terminal.screens.active;
            let s: *mut Screen = self_.terminal.screens.active;
            // const copy = s.clone(s.alloc, .{ .viewport = .{} }, null) catch |err| {
            //     log.warn("error cloning screen err={}", .{err});
            //     return error.BenchmarkFailed;
            // };
            // std.mem.doNotOptimizeAway(copy);
            let alloc = unsafe { (*s).alloc };
            let copy =
                match unsafe { (*s).clone(alloc, Point::Viewport(Coordinate::default()), None) } {
                    Ok(c) => c,
                    Err(err) => {
                        // log.warn("error cloning screen err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                        return Err(Error::BenchmarkFailed);
                    }
                };
            black_box(&copy);

            // Note: we purposely do not free memory because we don't want
            // to benchmark that. We'll free when the benchmark exits.
            //
            // UD376: the reference never does free them (it drops the copy on
            // the floor); here the copy is kept and `teardown` frees it.
            self_.clones.push(copy);
        }
        Ok(())
    }

    // fn stepRender(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_render(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // We do this once out of the loop because a significant slowdown
        // on the first run is allocation. After that first run, even with
        // a full rebuild, it is much faster. Let's ignore that first run
        // slowdown.
        // const alloc = self.terminal.screens.active.alloc;
        let alloc = unsafe { (*self_.terminal.screens.active).alloc };
        // var state: terminalpkg.RenderState = .empty;
        let mut state: RenderState = RenderState::empty();
        // state.update(alloc, &self.terminal) catch |err| {
        //     log.warn("error cloning screen err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        if let Err(err) = state.update(alloc, &mut self_.terminal) {
            // log.warn("error cloning screen err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
            return Err(Error::BenchmarkFailed);
        }

        // We loop because its so fast that a single benchmark run doesn't
        // properly capture our speeds.
        // for (0..50_000 * @as(u64, self.opts.loops)) |_| {
        //
        // PORT NOTE (446f80f4e, WS10 T12): 1 000 -> 50 000 x `loops`. This is
        // the ONLY step function the reference re-scales; `stepNoop` and
        // `stepClone` stay at 1 000 (verified against
        // `git show 446f80f4e:src/benchmark/ScreenClone.zig` — only the three
        // render-family loops carry the multiplier). So `--loops` is a
        // render-mode knob, not a global one, and `screen-clone.{noop,clone}`
        // timings are UNCHANGED by this task.
        for _ in 0..(50_000u64 * u64::from(self_.opts.loops)) {
            // Forces a full rebuild because it thinks our screen changed
            // state.screen = .alternate;
            state.screen = Key::Alternate;
            // state.update(alloc, &self.terminal) catch |err| { ... };
            if let Err(err) = state.update(alloc, &mut self_.terminal) {
                // log.warn("error cloning screen err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            // std.mem.doNotOptimizeAway(state);
            black_box(&state);

            // Note: we purposely do not free memory because we don't want
            // to benchmark that. We'll free when the benchmark exits.
        }
        Ok(())
    }

    // fn stepRenderLocked(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_render_locked(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // We do this once out of the loop because a significant slowdown
        // on the first run is allocation. After that first run, even with
        // a full rebuild, it is much faster. Let's ignore that first run
        // slowdown.
        let alloc = unsafe { (*self_.terminal.screens.active).alloc };
        let mut state: RenderState = RenderState::empty();
        if let Err(err) = state.update(alloc, &mut self_.terminal) {
            // log.warn("error cloning screen err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
            return Err(Error::BenchmarkFailed);
        }

        // We loop because its so fast that a single benchmark run doesn't
        // properly capture our speeds.
        for _ in 0..(50_000u64 * u64::from(self_.opts.loops)) {
            // Forces a full rebuild because it thinks our screen changed
            state.screen = Key::Alternate;
            if let Err(err) = state.begin_update(alloc, &mut self_.terminal) {
                // log.warn("error cloning screen err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            black_box(&state);

            // Note: we purposely do not free memory because we don't want
            // to benchmark that. We'll free when the benchmark exits.
        }
        Ok(())
    }

    // fn stepRenderClean(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_render_clean(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // Initial update so that subsequent updates are clean (nothing
        // dirty, no rebuilds).
        let alloc = unsafe { (*self_.terminal.screens.active).alloc };
        let mut state: RenderState = RenderState::empty();
        // state.overscan_request = overscanRequest(self.opts.mode);
        state.overscan_request = overscan_request(self_.opts.mode);
        if let Err(err) = state.update(alloc, &mut self_.terminal) {
            // log.warn("error cloning screen err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
            return Err(Error::BenchmarkFailed);
        }

        // We loop because its so fast that a single benchmark run doesn't
        // properly capture our speeds.
        for _ in 0..(3_000_000u64 * u64::from(self_.opts.loops)) {
            if let Err(err) = state.update(alloc, &mut self_.terminal) {
                // log.warn("error cloning screen err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            black_box(&state);
        }
        Ok(())
    }

    // fn stepRenderPartial(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_render_partial(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        // Initial update so that subsequent updates are incremental.
        let alloc = unsafe { (*self_.terminal.screens.active).alloc };
        let mut state: RenderState = RenderState::empty();
        if let Err(err) = state.update(alloc, &mut self_.terminal) {
            // log.warn("error cloning screen err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
            return Err(Error::BenchmarkFailed);
        }

        // Grab a pin roughly in the middle of the active area that we
        // dirty on every iteration to simulate a small screen update.
        // const pages = &self.terminal.screens.active.pages;
        // const pin = pages.pin(.{ .active = .{ .x = 0, .y = rows / 2 } }).?;
        let pages: *mut PageList =
            unsafe { core::ptr::addr_of_mut!((*self_.terminal.screens.active).pages) };
        let pin = unsafe {
            (*pages)
                .pin(Point::Active(Coordinate {
                    x: 0,
                    y: u32::from(self_.terminal.rows / 2),
                }))
                .unwrap()
        };

        // We loop because its so fast that a single benchmark run doesn't
        // properly capture our speeds.
        for _ in 0..(2_000_000u64 * u64::from(self_.opts.loops)) {
            // Mark a single row dirty. `update` clears this so each
            // iteration rebuilds exactly one row.
            // pin.markDirty();
            pin.mark_dirty();
            if let Err(err) = state.update(alloc, &mut self_.terminal) {
                // log.warn("error cloning screen err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            black_box(&state);
        }
        Ok(())
    }
}

// ScreenClone has NO test{} in the reference (§0/§9). The one test below is
// port-specific (P7-W3 T3): the six `ca4f719f7` mode names are bench-CLI
// visible, so every mode name must parse and run one iteration.

impl ScreenClone {
    // fn stepRenderScroll(ptr: *anyopaque) Benchmark.Error!void {
    //     const self: *ScreenClone = @ptrCast(@alignCast(ptr));
    //     try renderScrollLoop(self, 1, scroll_reverse_interval);
    // }
    unsafe fn step_render_scroll(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };
        self_.render_scroll_loop(1, SCROLL_REVERSE_INTERVAL)
    }

    // fn stepRenderScrollPage(ptr: *anyopaque) Benchmark.Error!void {
    //     const self: *ScreenClone = @ptrCast(@alignCast(ptr));
    //     try renderScrollLoop(
    //         self,
    //         @intCast(@max(1, self.terminal.rows / 2)),
    //         scroll_page_reverse_interval,
    //     );
    // }
    unsafe fn step_render_scroll_page(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };
        let step = core::cmp::max(1, self_.terminal.rows / 2) as isize;
        self_.render_scroll_loop(step, SCROLL_PAGE_REVERSE_INTERVAL)
    }

    /// Scroll the viewport by `step` rows per iteration, reversing
    /// direction every `interval` iterations, and update the render state
    /// after each scroll. The viewport starts at the bottom, so the first
    /// stretch scrolls up into scrollback.
    // fn renderScrollLoop(self: *ScreenClone, step: isize, interval: usize) Benchmark.Error!void {
    fn render_scroll_loop(&mut self, step: isize, interval: usize) -> Result<(), Error> {
        // const alloc = self.terminal.screens.active.alloc;
        let alloc = unsafe { (*self.terminal.screens.active).alloc };
        // var state: terminalpkg.RenderState = .empty;
        let mut state: RenderState = RenderState::empty();
        // state.overscan_request = overscanRequest(self.opts.mode);
        state.overscan_request = overscan_request(self.opts.mode);
        if let Err(err) = state.update(alloc, &mut self.terminal) {
            // log.warn("error cloning screen err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
            return Err(Error::BenchmarkFailed);
        }

        // var dir: isize = 1;
        let mut dir: isize = 1;
        // for (0..50_000 * @as(u64, self.opts.loops)) |i| {
        for i in 0..(50_000u64 * u64::from(self.opts.loops)) {
            // if (i % interval == 0) dir = -dir;
            if i % interval as u64 == 0 {
                dir = -dir;
            }
            // self.terminal.scrollViewport(.{ .delta = dir * step });
            self.terminal
                .scroll_viewport(ScrollViewport::Delta(dir * step));
            if let Err(err) = state.update(alloc, &mut self.terminal) {
                // log.warn("error cloning screen err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            // std.mem.doNotOptimizeAway(&state);
            black_box(&state);
        }
        Ok(())
    }

    // fn stepRenderOutput(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_render_output(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };

        let alloc = unsafe { (*self_.terminal.screens.active).alloc };
        let mut state: RenderState = RenderState::empty();
        // state.overscan_request = overscanRequest(self.opts.mode);
        state.overscan_request = overscan_request(self_.opts.mode);
        if let Err(err) = state.update(alloc, &mut self_.terminal) {
            // log.warn("error cloning screen err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
            return Err(Error::BenchmarkFailed);
        }

        // Make sure the viewport follows the active area so each line
        // written moves the viewport.
        // self.terminal.scrollViewport(.bottom);
        self_.terminal.scroll_viewport(ScrollViewport::Bottom);

        // var stream = self.terminal.vtStream();
        // defer stream.deinit();
        // for (0..50_000 * @as(u64, self.opts.loops)) |_| {
        //     stream.nextSlice("hello\r\n");
        //     state.update(alloc, &self.terminal) catch |err| { ... };
        //     std.mem.doNotOptimizeAway(&state);
        // }
        //
        // PORT NOTE: the reference holds one stream across the loop while it
        // also passes `&self.terminal` to `update`. In Rust the stream holds
        // `&mut Terminal`, so each iteration opens and closes a stream around
        // its write; `vt_stream`/`deinit` hold no state that outlives a
        // complete "hello\r\n" (the parser returns to ground after it).
        for _ in 0..(50_000u64 * u64::from(self_.opts.loops)) {
            {
                let mut stream = self_.terminal.vt_stream();
                stream.next_slice(b"hello\r\n");
                stream.deinit();
            }
            if let Err(err) = state.update(alloc, &mut self_.terminal) {
                // log.warn("error cloning screen err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error cloning screen err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            black_box(&state);
        }
        Ok(())
    }
}

impl BenchmarkAction for ScreenClone {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(ScreenClone::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        ScreenClone::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        ScreenClone::benchmark(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::RunMode;

    // PORT-SPECIFIC (P7-W3 T3): every `Mode` name parses to its variant, and a
    // benchmark in that mode runs one iteration (`RunMode::Once`: setup, one
    // step, teardown) on a small terminal without failing. `loops` is the
    // number of passes of each mode's timed loop.
    fn every_mode_name_parses_and_runs_one_iteration(loops: &[u8]) {
        use rustty_args::args::Target;

        let modes: &[(&[u8], Mode)] = &[
            (b"noop", Mode::Noop),
            (b"clone", Mode::Clone),
            (b"render", Mode::Render),
            (b"render-locked", Mode::RenderLocked),
            (b"render-clean", Mode::RenderClean),
            (b"render-partial", Mode::RenderPartial),
            (b"render-scroll", Mode::RenderScroll),
            (b"render-scroll-page", Mode::RenderScrollPage),
            (b"render-output", Mode::RenderOutput),
            (b"render-scroll-overscan", Mode::RenderScrollOverscan),
            (b"render-output-overscan", Mode::RenderOutputOverscan),
            (b"render-clean-overscan", Mode::RenderCleanOverscan),
        ];
        for &(name, mode) in modes {
            let mut opts = Options::default();
            opts.parse_into_field(b"mode", Some(name)).unwrap();
            assert!(opts.mode == mode, "{}", String::from_utf8_lossy(name));
            opts.parse_into_field(b"loops", Some(loops)).unwrap();
            opts.parse_into_field(b"terminal-rows", Some(b"4")).unwrap();
            opts.parse_into_field(b"terminal-cols", Some(b"8")).unwrap();

            let mut bench = ScreenClone::create(Allocator, opts).unwrap();
            let result = bench.benchmark().run(RunMode::Once);
            assert!(result.is_ok(), "{}", String::from_utf8_lossy(name));
            bench.destroy(Allocator);
        }
        assert!(Mode::from_name(b"render-overscan").is_none());
    }

    // UD376: the `clone` step keeps its 1000 copies (the reference drops them
    // on the floor until the process exits), and the teardown frees them, so
    // a finished run holds none. G-LSAN found the leak (P7-W14).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-bench, P7-W14): 1000 screen clones of a live terminal; runs natively and under G-ASAN/G-LSAN"
    )]
    fn screen_clone_clone_mode_frees_its_copies_at_teardown() {
        use rustty_args::args::Target;

        let mut opts = Options::default();
        opts.parse_into_field(b"mode", Some(b"clone")).unwrap();
        opts.parse_into_field(b"terminal-rows", Some(b"4")).unwrap();
        opts.parse_into_field(b"terminal-cols", Some(b"8")).unwrap();
        let mut bench = ScreenClone::create(Allocator, opts).unwrap();
        let ptr = &mut *bench as *mut ScreenClone as *mut c_void;
        unsafe { ScreenClone::step_clone(ptr) }.unwrap();
        assert_eq!(1000, bench.clones.len(), "the step keeps every copy");
        unsafe { ScreenClone::teardown(ptr) };
        assert!(bench.clones.is_empty(), "teardown frees every copy");
        // A whole run, too: setup, one step, teardown.
        assert!(bench.benchmark().run(RunMode::Once).is_ok());
        assert!(bench.clones.is_empty(), "a finished run holds no copy");
        bench.destroy(Allocator);
    }

    // `loops = 1`: one real pass of every mode's timed loop (50,000 to
    // 3,000,000 updates). Too slow unoptimized, so a debug build reports it
    // `ignored` (R14 compile-time scope); the --release arm of G-BENCH runs it.
    // (P7-W12 T0 split the debug-only `loops = 0` arm out into the test below,
    // replacing a runtime "skips the timed inner loop" note.)
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-bench, P7-W3 T3): tens of thousands of render-state updates per mode; runs natively in the G-BENCH lane (--release)"
    )]
    #[cfg_attr(
        all(not(miri), debug_assertions),
        ignore = "R14 compile-time scope: one pass of each mode's timed loop is too slow unoptimized; the --release arm of G-BENCH runs it, and screen_clone_every_mode_name_parses_and_runs_setup_with_zero_loops covers debug"
    )]
    fn screen_clone_every_mode_name_parses_and_runs_one_iteration() {
        every_mode_name_parses_and_runs_one_iteration(b"1");
    }

    // `loops = 0`: setup and the initial render-state update with the mode's
    // overscan request, no timed loop. Runs in every build profile.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-bench, P7-W3 T3): a render-state update per mode on a live terminal; runs natively in the G-BENCH lane (debug and --release)"
    )]
    fn screen_clone_every_mode_name_parses_and_runs_setup_with_zero_loops() {
        every_mode_name_parses_and_runs_one_iteration(b"0");
    }
}
