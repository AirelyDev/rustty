//! This benchmark tests the throughput of the OSC parser.
// const OscParser = @This();

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const Parser = @import("../terminal/osc.zig").Parser;
// const log = std.log.scoped(.@"osc-parser-bench");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, CacheLineBuf, DataFile};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_vt::osc::Parser;
use rustty_vt::pagelist::OutOfMemory;
use std::io::{BufReader, ErrorKind, Read};

const LOG_SCOPE: &str = "osc-parser-bench";

// PORT NOTE (§7.4): the reference threads `alloc` (a `std.mem.Allocator`) into
// `Parser.init`. CORRECTION (parity tail S7-7): the port's `Parser::init` takes
// the runtime allocator handle; the bench's `alloc` is the Rust heap, so it
// passes `rustty_vt::alloc::Global` (it used to be a process-static
// `GlobalZigAllocator` reference, giving a `Parser<'static>`).

// opts: Options,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
// parser: Parser,
pub struct OscParser {
    opts: Options,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
    parser: Parser,
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

    /// The most bytes to keep from each OSC sequence whose number the
    /// parser does not implement. Zero, the default, discards them like
    /// the parser does by default. See `osc.Parser.unknown_max_bytes`.
    // @"unknown-max-bytes": usize = 0,
    unknown_max_bytes: usize,
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
            b"unknown-max-bytes" => {
                self.unknown_max_bytes = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

impl OscParser {
    /// Create a new terminal stream handler for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*OscParser {
    //     const ptr = try alloc.create(OscParser);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{ .opts = opts, .data_f = null, .parser = .init(alloc) };
    //     ptr.parser.unknown_max_bytes = opts.@"unknown-max-bytes";
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<OscParser>, OutOfMemory> {
        let _ = alloc;
        let unknown_max_bytes = opts.unknown_max_bytes;
        let mut ptr = Box::new(OscParser {
            opts,
            data_f: None,
            parser: Parser::init(Some(rustty_vt::alloc::Global)),
        });
        ptr.parser.unknown_max_bytes = unknown_max_bytes;
        Ok(ptr)
    }

    // pub fn destroy(self: *OscParser, alloc: Allocator) void {
    //     self.parser.deinit();
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<OscParser>, alloc: Allocator) {
        let _ = alloc;
        self.parser.deinit();
        drop(self);
    }

    // pub fn benchmark(self: *OscParser) Benchmark {
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
        let self_ = unsafe { &mut *(ptr as *mut OscParser) };
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
        // self.parser.reset();
        self_.parser.reset();
        Ok(())
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut OscParser) };
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn step(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut OscParser) };

        // const f = self.data_f orelse return;
        let f = match self_.data_f.take() {
            Some(f) => f,
            None => return Ok(()),
        };
        // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        // var r = f.reader(&read_buf);
        //
        // PORT NOTE (§8): `f.reader(&read_buf)` is the Zig 0.15 buffered reader
        // (`fill`/`takeInt`/`readSliceAll`). We wrap the `DataFile` in a
        // `BufReader` and use `read_exact` for the length prefix and the record
        // body. The file is moved out (`take`) for the duration of the step and
        // restored below (the reference re-derives the reader each step).
        let mut r = BufReader::new(f);

        // var osc_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        let mut osc_buf = CacheLineBuf::new();
        // while (true) {
        let result = loop {
            // r.interface.fill(@bitSizeOf(usize) / 8) catch |err| switch (err) {
            //     error.EndOfStream => return,
            //     error.ReadFailed => return error.BenchmarkFailed,
            // };
            // const len = r.interface.takeInt(usize, .little) catch |err| switch (err) {
            //     error.EndOfStream => return,
            //     error.ReadFailed => return error.BenchmarkFailed,
            // };
            //
            // PORT NOTE (§8): `@bitSizeOf(usize)/8` == 8 on the 64-bit targets;
            // `takeInt(usize, .little)` == `u64::from_le_bytes`. A partial/empty
            // read at a record boundary (`UnexpectedEof`) is the reference's
            // `error.EndOfStream => return`; any other IO error is `ReadFailed
            // => BenchmarkFailed`.
            let mut len_bytes = [0u8; core::mem::size_of::<usize>()];
            match r.read_exact(&mut len_bytes) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::UnexpectedEof => break Ok(()),
                Err(_) => break Err(Error::BenchmarkFailed),
            }
            let len = usize::from_le_bytes(len_bytes);

            // if (len > osc_buf.len) return error.BenchmarkFailed;
            if len > osc_buf.0.len() {
                break Err(Error::BenchmarkFailed);
            }

            // r.interface.readSliceAll(osc_buf[0..len]) catch |err| switch (err) {
            //     error.EndOfStream => return,
            //     error.ReadFailed => return error.BenchmarkFailed,
            // };
            match r.read_exact(&mut osc_buf.0[..len]) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::UnexpectedEof => break Ok(()),
                Err(_) => break Err(Error::BenchmarkFailed),
            }

            // for (osc_buf[0..len]) |c| @call(.always_inline, Parser.next, .{ &self.parser, c });
            //
            // PORT NOTE (§7.4): the `@call(.always_inline, Parser.next, ...)`
            // inline hint has no faithful Rust knob (the nearest is `#[inline]`
            // on the vt-side `next`, which is defined in rustty-vt); a plain call
            // is used.
            for &c in &osc_buf.0[..len] {
                self_.parser.next(c);
            }
            // std.mem.doNotOptimizeAway(self.parser.end(std.ascii.control_code.bel));
            //   std.ascii.control_code.bel == 0x07.
            black_box(self_.parser.end(Some(0x07)));
            // self.parser.reset();
            self_.parser.reset();
        };

        // Restore the data file for the next step (the reference keeps the fd on
        // `self.data_f`; only the per-step reader is transient).
        self_.data_f = Some(r.into_inner());
        result
    }
}

// test OscParser {
#[test]
fn osc_parser() {
    let alloc = Allocator;
    // const impl: *OscParser = try .create(alloc, .{});
    let mut impl_ = OscParser::create(alloc, Options::default()).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for OscParser {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(OscParser::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        OscParser::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        OscParser::benchmark(self)
    }
}
