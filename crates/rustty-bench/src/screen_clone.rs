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
use rustty_vt::render::RenderState;
use rustty_vt::screen::Screen;
use rustty_vt::screen_set::Key;
use rustty_vt::terminal::Terminal;

const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// terminal: Terminal,
pub struct ScreenClone {
    opts: Options,
    terminal: Terminal,
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
}

impl Mode {
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"clone" => Some(Mode::Clone),
            b"render" => Some(Mode::Render),
            b"render-locked" => Some(Mode::RenderLocked),
            b"render-clean" => Some(Mode::RenderClean),
            b"render-partial" => Some(Mode::RenderPartial),
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
    //     ptr.* = .{
    //         .opts = opts,
    //         .terminal = try .init(alloc, .{
    //             .rows = opts.@"terminal-rows",
    //             .cols = opts.@"terminal-cols",
    //         }),
    //     };
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<ScreenClone>, OutOfMemory> {
        let terminal = Terminal::init(
            alloc.into(),
            terminal_options(opts.terminal_rows, opts.terminal_cols),
        )?;
        Ok(Box::new(ScreenClone { opts, terminal }))
    }

    // pub fn destroy(self: *ScreenClone, alloc: Allocator) void {
    //     self.terminal.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<ScreenClone>, alloc: Allocator) {
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
                    Mode::RenderClean => Self::step_render_clean,
                    Mode::RenderPartial => Self::step_render_partial,
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
        // for (0..self.terminal.rows - 1) |_| s.nextSlice("hello\r\n");
        // s.nextSlice("hello");
        {
            let rows = self_.terminal.rows;
            let mut s = self_.terminal.vt_stream();
            s.next_slice(b"\x1b[48;2;20;40;60m");
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
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut ScreenClone) };
        let _ = self_;
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
            core::mem::forget(copy);
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

// ScreenClone has NO test{} in the reference (§0/§9) — faithfully no `#[test]`.

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
