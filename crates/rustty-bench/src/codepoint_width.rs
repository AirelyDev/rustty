//! This benchmark tests the throughput of codepoint width calculation.
//! This is a common operation in terminal character printing and the
//! motivating factor to write this benchmark was discovering that our
//! codepoint width function was 30% of the runtime of every character
//! print.
// const CodepointWidth = @This();

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const UTF8Decoder = @import("../terminal/UTF8Decoder.zig");
// const simd = @import("../simd/main.zig");
// const table = @import("../unicode/main.zig").table;
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short, CacheLineBuf, DataFile};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use rustty_simd as simd;
use rustty_unicode::table;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::utf8_decoder::UTF8Decoder;

// const log = std.log.scoped(.@"terminal-stream-bench");
const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
pub struct CodepointWidth {
    opts: Options,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
}

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// The type of codepoint width calculation to use.
    // mode: Mode = .noop,
    mode: Mode,

    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, then we will
    /// do nothing (benchmark is a noop). It'd be more unixy to
    /// use stdin by default but I find that a hanging CLI command
    /// with no interaction is a bit annoying.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

// pub const Mode = enum {
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The baseline mode copies the data from the fd into a buffer. This
    /// is used to show the minimal overhead of reading the fd into memory
    /// and establishes a baseline for the other modes.
    // noop,
    #[default]
    Noop,

    /// libc wcwidth
    // wcwidth,
    Wcwidth,

    /// Our SIMD implementation.
    // simd,
    Simd,

    /// Test our lookup table implementation.
    // table,
    Table,
}

impl Mode {
    /// `std.meta.stringToEnum(Mode, name)` — the kebab CLI tokens.
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"wcwidth" => Some(Mode::Wcwidth),
            b"simd" => Some(Mode::Simd),
            b"table" => Some(Mode::Table),
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
            // mode: Mode
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
            // data: ?[]const u8
            b"data" => {
                self.data = Some(rustty_args::args::parse_field_string(value)?);
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

impl CodepointWidth {
    /// Create a new terminal stream handler for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*CodepointWidth {
    //     const ptr = try alloc.create(CodepointWidth);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{ .opts = opts };
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<CodepointWidth>, OutOfMemory> {
        let _ = alloc;
        Ok(Box::new(CodepointWidth { opts, data_f: None }))
    }

    // pub fn destroy(self: *CodepointWidth, alloc: Allocator) void {
    //     alloc.destroy(self);
    // }
    pub fn destroy(self: Box<CodepointWidth>, alloc: Allocator) {
        let _ = alloc;
        drop(self);
    }

    // pub fn benchmark(self: *CodepointWidth) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) {
                //     .noop => stepNoop, .wcwidth => stepWcwidth,
                //     .table => stepTable, .simd => stepSimd,
                // },
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Wcwidth => Self::step_wcwidth,
                    Mode::Table => Self::step_table,
                    Mode::Simd => Self::step_simd,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut CodepointWidth) };

        // Open our data file to prepare for reading. We can do more
        // validation here eventually.
        // assert(self.data_f == null);
        assert!(self_.data_f.is_none());
        // self.data_f = options.dataFile(self.opts.data) catch |err| {
        //     log.warn("error opening data file err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        self_.data_f = match options::data_file(self_.opts.data.as_deref()) {
            Ok(f) => f,
            Err(err) => {
                // log.warn("error opening data file err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error opening data file err={err}");
                return Err(Error::BenchmarkFailed);
            }
        };
        Ok(())
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut CodepointWidth) };
        // if (self.data_f) |f| { f.close(); self.data_f = null; }
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    //     _ = ptr;
    // }
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let _ = ptr;
        Ok(())
    }

    // fn stepWcwidth(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_wcwidth(ptr: *mut c_void) -> Result<(), Error> {
        // if (comptime builtin.os.tag == .windows) {
        //     log.warn("wcwidth is not available on Windows", .{});
        //     return;
        // }
        #[cfg(windows)]
        {
            let _ = ptr;
            // log.warn("wcwidth is not available on Windows", .{});
            log::warn!(target: LOG_SCOPE, "wcwidth is not available on Windows");
            // (the tail of the function on Windows, task W-2)
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let self_ = unsafe { &mut *(ptr as *mut CodepointWidth) };

            // const f = self.data_f orelse return;
            let f = match self_.data_f.as_mut() {
                Some(f) => f,
                None => return Ok(()),
            };
            // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
            // var f_reader = f.reader(&read_buf);
            // var r = &f_reader.interface;
            //
            // PORT NOTE (§8): the reference threads a cache-line-aligned read
            // buffer into `f.reader(&read_buf)`. `DataFile` reads directly (no
            // separate scratch reader); we keep the aligned buffer for the
            // scan buffer below to preserve the perf-fidelity alignment.
            let r = f;

            // var d: UTF8Decoder = .{};
            let mut d = UTF8Decoder::new();
            // var buf: [4096]u8 align(std.atomic.cache_line) = undefined;
            let mut buf = CacheLineBuf::new();
            // while (true) {
            loop {
                // const n = r.readSliceShort(&buf) catch { ... return error.BenchmarkFailed; };
                let n = match read_slice_short(r, &mut buf.0) {
                    Ok(n) => n,
                    Err(err) => {
                        // log.warn("error reading data file err={?}", .{f_reader.err});
                        log::warn!(target: LOG_SCOPE, "error reading data file err={err:?}");
                        return Err(Error::BenchmarkFailed);
                    }
                };
                // if (n == 0) break; // EOF reached
                if n == 0 {
                    break;
                }

                // for (buf[0..n]) |c| {
                for &c in &buf.0[..n] {
                    // const cp_, const consumed = d.next(c);
                    let (cp_, consumed) = d.next(c);
                    // assert(consumed);
                    // PERF(port): std.debug.assert tier — live in Debug, absent in release.
                    debug_assert!(consumed);
                    // if (cp_) |cp| {
                    if let Some(cp) = cp_ {
                        // std.mem.doNotOptimizeAway(wcwidth(cp));
                        crate::do_not_optimize_away(unsafe { wcwidth(cp) });
                    }
                }
            }
            Ok(())
        }
    }

    // fn stepTable(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_table(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut CodepointWidth) };

        // const f = self.data_f orelse return;
        let f = match self_.data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        let r = f;

        // var d: UTF8Decoder = .{};
        let mut d = UTF8Decoder::new();
        // var buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        let mut buf = CacheLineBuf::new();
        loop {
            let n = match read_slice_short(r, &mut buf.0) {
                Ok(n) => n,
                Err(err) => {
                    // log.warn("error reading data file err={?}", .{f_reader.err});
                    log::warn!(target: LOG_SCOPE, "error reading data file err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            if n == 0 {
                break;
            }

            for &c in &buf.0[..n] {
                let (cp_, consumed) = d.next(c);
                // PERF(port): std.debug.assert tier — live in Debug, absent in release.
                debug_assert!(consumed);
                if let Some(cp) = cp_ {
                    // This is the same trick we do in terminal.zig so we
                    // keep it here.
                    // std.mem.doNotOptimizeAway(if (cp <= 0xFF) 1 else table.get(@intCast(cp)).width);
                    crate::do_not_optimize_away(if cp <= 0xFF {
                        1u8
                    } else {
                        table().get(cp).width
                    });
                }
            }
        }
        Ok(())
    }

    // fn stepSimd(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_simd(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut CodepointWidth) };

        // const f = self.data_f orelse return;
        let f = match self_.data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        let r = f;

        // var d: UTF8Decoder = .{};
        let mut d = UTF8Decoder::new();
        // var buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        let mut buf = CacheLineBuf::new();
        loop {
            let n = match read_slice_short(r, &mut buf.0) {
                Ok(n) => n,
                Err(err) => {
                    // log.warn("error reading data file err={?}", .{f_reader.err});
                    log::warn!(target: LOG_SCOPE, "error reading data file err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            if n == 0 {
                break;
            }

            for &c in &buf.0[..n] {
                let (cp_, consumed) = d.next(c);
                // PERF(port): std.debug.assert tier — live in Debug, absent in release.
                debug_assert!(consumed);
                if let Some(cp) = cp_ {
                    // std.mem.doNotOptimizeAway(simd.codepointWidth(cp));
                    crate::do_not_optimize_away(simd::codepointWidth(cp));
                }
            }
        }
        Ok(())
    }
}

// extern "c" fn wcwidth(c: u32) c_int;
//
// PORT NOTE (§7.1): declared under `#[cfg(not(windows))]`; it links to libc's
// `wcwidth` (always linked on Unix), so no `libc` crate dep is needed. Zig
// passes `u32` here even though C `wchar_t`/`wint_t` is `c_int`; kept verbatim.
#[cfg(not(windows))]
extern "C" {
    fn wcwidth(c: u32) -> core::ffi::c_int;
}

// test CodepointWidth {
#[test]
fn codepoint_width() {
    // const testing = std.testing;
    // const alloc = testing.allocator;
    let alloc = Allocator;

    // const impl: *CodepointWidth = try .create(alloc, .{});
    // defer impl.destroy(alloc);
    let mut impl_ = CodepointWidth::create(alloc, Options::default()).unwrap();

    // const bench = impl.benchmark();
    let bench = impl_.benchmark();
    // _ = try bench.run(.once);
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();

    impl_.destroy(alloc);
}

// The `Action.Struct`-selected decls the CLI dispatch requires (§6).
impl BenchmarkAction for CodepointWidth {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(CodepointWidth::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        CodepointWidth::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        CodepointWidth::benchmark(self)
    }
}
