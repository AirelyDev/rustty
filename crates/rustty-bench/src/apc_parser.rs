//! This benchmark tests the throughput of APC sequence parsing
//! through the terminal stream: VT state machine dispatch, APC
//! protocol identification, and the protocol command parsers
//! (e.g. Kitty graphics). Completed commands are parsed and then
//! discarded; command execution (image decoding, storage) is not
//! included so this isolates the parsing path.
// const ApcParser = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short, DataFile};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_vt::apc;

// const log = std.log.scoped(.@"apc-parser-bench");
const LOG_SCOPE: &str = "apc-parser-bench";

// opts: Options,
// stream: Stream,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
pub struct ApcParser {
    opts: Options,
    stream: Stream,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
}

// pub const Options = struct {
pub struct Options {
    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, then we will
    /// do nothing (benchmark is a noop). It'd be more unixy to
    /// use stdin by default but I find that a hanging CLI command
    /// with no interaction is a bit annoying.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

// PORT NOTE: the reference declares the field default inline
// (`data: ?[]const u8 = null`); `Default` is the Rust model of `Options{}`.
#[allow(clippy::derivable_impls)]
impl Default for Options {
    fn default() -> Options {
        Options { data: None }
    }
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

// const Stream = terminalpkg.Stream(Handler);
type Stream = rustty_vt::stream::Stream<'static, Handler>;

/// A stream handler that only processes APC actions, parsing and
/// immediately discarding completed commands.
// const Handler = struct { alloc: Allocator, apc: terminalpkg.apc.Handler = .{} };
struct Handler {
    alloc: Allocator,
    apc: apc::Handler,
}

impl rustty_vt::stream::Handler for Handler {
    // pub fn deinit(self: *Handler) void { self.apc.deinit(); }
    fn deinit(&mut self) {
        self.apc.deinit();
    }

    // .apc_start => self.apc.start(),
    fn apc_start(&mut self) {
        self.apc.start();
    }
    // .apc_put => self.apc.feed(self.alloc, value),
    fn apc_put(&mut self, code: u8) {
        let alloc = self.alloc;
        self.apc.feed(alloc, code);
    }
    // .apc_put_slice => self.apc.feedSlice(self.alloc, value.bytes),
    fn apc_put_slice(&mut self, bytes: &[u8]) {
        let alloc = self.alloc;
        self.apc.feed_slice(alloc, bytes);
    }
    // .apc_end => if (self.apc.end()) |cmd| {
    //     var c = cmd;
    //     std.mem.doNotOptimizeAway(&c);
    //     c.deinit(self.alloc);
    // },
    fn apc_end(&mut self) {
        let alloc = self.alloc;
        if let Some(cmd) = self.apc.end(alloc) {
            let mut c = cmd;
            black_box(&c);
            c.deinit(alloc);
        }
    }
}

impl ApcParser {
    /// Create a new APC parser benchmark for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*ApcParser
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<ApcParser>, CreateError> {
        Ok(Box::new(ApcParser {
            opts,
            // .stream = .init(.{ .alloc = alloc }),
            stream: Stream::init(Handler {
                alloc,
                apc: apc::Handler::default(),
            }),
            data_f: None,
        }))
    }

    // pub fn destroy(self: *ApcParser, alloc: Allocator) void {
    //     self.stream.deinit();
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<ApcParser>, _alloc: Allocator) {
        self.stream.deinit();
        drop(self);
    }

    // pub fn benchmark(self: *ApcParser) Benchmark {
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
        let self_ = unsafe { &mut *(ptr as *mut ApcParser) };

        // Open our data file to prepare for reading. We can do more
        // validation here eventually.
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
        let self_ = unsafe { &mut *(ptr as *mut ApcParser) };
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn step(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ApcParser) };

        // const f = self.data_f orelse return;
        let (data_f, stream) = (&mut self_.data_f, &mut self_.stream);
        let f = match data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        // var read_buf: [64 * 1024]u8 align(std.atomic.cache_line) = undefined;
        // var f_reader = f.reader(&read_buf);
        // const r = &f_reader.interface;
        //
        // PORT NOTE: `read_slice_short` reads straight from the `DataFile`, so
        // the reference's intermediate reader buffer has no counterpart (same
        // as `terminal_stream.rs`).
        let r = f;

        // This buffer size matches the read buffer size used by the
        // real IO thread (see termio Exec.zig buffer_capacity) so that
        // the benchmark exercises the stream with realistic chunk sizes.
        // var buf: [64 * 1024]u8 = undefined;
        let mut buf = vec![0u8; 64 * 1024];
        // while (true) {
        loop {
            // const n = r.readSliceShort(&buf) catch { ... };
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
            // self.stream.nextSlice(buf[0..n]);
            stream.next_slice(&buf[..n]);
        }
        Ok(())
    }
}

impl BenchmarkAction for ApcParser {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        ApcParser::create(alloc, opts)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        ApcParser::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        ApcParser::benchmark(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::RunMode;

    // test ApcParser
    #[test]
    fn apc_parser() {
        let alloc = Allocator;

        let mut impl_ = ApcParser::create(alloc, Options::default()).unwrap();

        let bench = impl_.benchmark();
        bench.run(RunMode::Once).unwrap();
        impl_.destroy(alloc);
    }
}
