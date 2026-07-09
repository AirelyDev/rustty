//! This benchmark tests the throughput of the terminal escape code parser.
// const TerminalParser = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::BenchmarkAction;
use crate::options::{self, read_slice_short, DataFile};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::parser::Parser;

const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
pub struct TerminalParser {
    opts: Options,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
}

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, then we will
    /// do nothing (benchmark is a noop). It'd be more unixy to
    /// use stdin by default but I find that a hanging CLI command
    /// with no interaction is a bit annoying.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            b"data" => {
                self.data = Some(rustty_args::args::parse_field_string(value)?);
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

impl TerminalParser {
    // pub fn create(alloc: Allocator, opts: Options) !*TerminalParser {
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<TerminalParser>, OutOfMemory> {
        let _ = alloc;
        Ok(Box::new(TerminalParser { opts, data_f: None }))
    }

    // pub fn destroy(self: *TerminalParser, alloc: Allocator) void {
    pub fn destroy(self: Box<TerminalParser>, alloc: Allocator) {
        let _ = alloc;
        drop(self);
    }

    // pub fn benchmark(self: *TerminalParser) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                step_fn: Self::step,
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalParser) };
        // assert(self.data_f == null);
        assert!(self_.data_f.is_none());
        self_.data_f = match options::data_file(self_.opts.data.as_deref()) {
            Ok(f) => f,
            Err(err) => {
                eprintln!("[{LOG_SCOPE}] error opening data file err={err}");
                return Err(Error::BenchmarkFailed);
            }
        };
        Ok(())
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut TerminalParser) };
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn step(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalParser) };

        // Get our buffered reader so we're not predominantly
        // waiting on file IO. It'd be better to move this fully into
        // memory. If we're IO bound though that should show up on
        // the benchmark results and... I know writing this that we
        // aren't currently IO bound.
        // const f = self.data_f orelse return;
        let f = match self_.data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        // var f_reader = f.reader(&read_buf);
        // var r = &f_reader.interface;
        let r = f;

        // var p: terminalpkg.Parser = .init();
        let mut p: Parser = Parser::init();

        // var buf: [4096]u8 = undefined;
        let mut buf = [0u8; 4096];
        // while (true) {
        loop {
            // const n = r.readSliceShort(&buf) catch { ... return error.BenchmarkFailed; };
            let n = match read_slice_short(r, &mut buf) {
                Ok(n) => n,
                Err(err) => {
                    eprintln!("[{LOG_SCOPE}] error reading data file err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            // if (n == 0) break; // EOF reached
            if n == 0 {
                break;
            }
            // parseAll(&p, buf[0..n]);
            parse_all(&mut p, &buf[..n]);
        }
        Ok(())
    }
}

/// Separated from `step` so that the tight per-byte loop gets its own
/// function alignment, insulating it from code-layout changes elsewhere
/// in the binary that would otherwise shift its cache-line placement.
// noinline fn parseAll(p: *terminalpkg.Parser, data: []const u8) void {
#[inline(never)]
fn parse_all(p: &mut Parser, data: &[u8]) {
    // for (data) |c| {
    for &c in data {
        // const actions = p.next(c);
        // _ = actions;
        let actions = p.next(c);
        let _ = actions;
    }
}

// test TerminalParser {
#[test]
fn terminal_parser() {
    let alloc = Allocator;
    // const impl: *TerminalParser = try .create(alloc, .{});
    let mut impl_ = TerminalParser::create(alloc, Options::default()).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for TerminalParser {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, OutOfMemory> {
        TerminalParser::create(alloc, opts)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        TerminalParser::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        TerminalParser::benchmark(self)
    }
}
