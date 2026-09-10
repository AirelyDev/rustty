//! This benchmark tests the throughput of grapheme break calculation.
//! This is a common operation in terminal character printing for terminals
//! that support grapheme clustering.
// const GraphemeBreak = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const UTF8Decoder = @import("../terminal/UTF8Decoder.zig");
// const unicode = @import("../unicode/main.zig");
// const uucode = @import("uucode");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short, CacheLineBuf, DataFile};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use rustty_unicode as unicode;
use rustty_unicode::grapheme::BreakState;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::utf8_decoder::UTF8Decoder;

// const log = std.log.scoped(.@"terminal-stream-bench");
const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
pub struct GraphemeBreak {
    opts: Options,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
}

// pub const Options = struct {
pub struct Options {
    /// The type of codepoint width calculation to use.
    // mode: Mode = .table,
    mode: Mode,

    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, then we will
    /// do nothing (benchmark is a noop). It'd be more unixy to
    /// use stdin by default but I find that a hanging CLI command
    /// with no interaction is a bit annoying.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            // mode: Mode = .table,
            mode: Mode::Table,
            data: None,
        }
    }
}

// pub const Mode = enum {
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The baseline mode copies the data from the fd into a buffer. This
    /// is used to show the minimal overhead of reading the fd into memory
    /// and establishes a baseline for the other modes.
    // noop,
    Noop,

    /// Ghostty's table-based approach.
    // table,
    Table,
}

impl Mode {
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
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
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
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

impl GraphemeBreak {
    /// Create a new terminal stream handler for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*GraphemeBreak {
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<GraphemeBreak>, OutOfMemory> {
        let _ = alloc;
        Ok(Box::new(GraphemeBreak { opts, data_f: None }))
    }

    // pub fn destroy(self: *GraphemeBreak, alloc: Allocator) void {
    pub fn destroy(self: Box<GraphemeBreak>, alloc: Allocator) {
        let _ = alloc;
        drop(self);
    }

    // pub fn benchmark(self: *GraphemeBreak) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) { .noop => stepNoop, .table => stepTable },
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Table => Self::step_table,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut GraphemeBreak) };
        // assert(self.data_f == null);
        assert!(self_.data_f.is_none());
        // self.data_f = options.dataFile(self.opts.data) catch |err| { ... };
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
        let self_ = unsafe { &mut *(ptr as *mut GraphemeBreak) };
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut GraphemeBreak) };

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
                // _ = d.next(c);
                let _ = d.next(c);
            }
        }
        Ok(())
    }

    // fn stepTable(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_table(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut GraphemeBreak) };

        // const f = self.data_f orelse return;
        let f = match self_.data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        let r = f;

        // var d: UTF8Decoder = .{};
        let mut d = UTF8Decoder::new();
        // var state: uucode.grapheme.BreakState = .default;
        let mut state: BreakState = BreakState::Default;
        // var cp1: u21 = 0;
        let mut cp1: u32 = 0;
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
                // if (cp_) |cp2| {
                if let Some(cp2) = cp_ {
                    // std.mem.doNotOptimizeAway(unicode.graphemeBreak(cp1, @intCast(cp2), &state));
                    crate::do_not_optimize_away(unicode::graphemeBreak(cp1, cp2, &mut state));
                    // cp1 = cp2;
                    cp1 = cp2;
                }
            }
        }
        Ok(())
    }
}

// test GraphemeBreak {
#[test]
fn grapheme_break() {
    let alloc = Allocator;
    // const impl: *GraphemeBreak = try .create(alloc, .{});
    let mut impl_ = GraphemeBreak::create(alloc, Options::default()).unwrap();
    // const bench = impl.benchmark();
    let bench = impl_.benchmark();
    // _ = try bench.run(.once);
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for GraphemeBreak {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(GraphemeBreak::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        GraphemeBreak::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        GraphemeBreak::benchmark(self)
    }
}
