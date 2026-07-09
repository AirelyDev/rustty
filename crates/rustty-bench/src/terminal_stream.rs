//! This benchmark tests the performance of the terminal stream
//! handler from input to terminal state update. This is useful to
//! test general throughput of VT parsing and handling.
//!
//! Note that the handler used for this benchmark isn't the full
//! terminal handler, since that requires a significant amount of
//! state. This is a simplified version that only handles specific
//! terminal operations like printing characters. We should expand
//! this to include more operations to improve the accuracy of the
//! benchmark.
//!
//! It is a fairly broad benchmark that can be used to determine
//! if we need to optimize something more specific (e.g. the parser).
// const TerminalStream = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const Terminal = terminalpkg.Terminal;
// const Stream = terminalpkg.Stream(*Handler);
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::BenchmarkAction;
use crate::options::{self, read_slice_short, DataFile};
use crate::terminal_options;
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::stream::Stream;
use rustty_vt::terminal::Terminal;

const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// terminal: Terminal,
// handler: Handler,
// stream: Stream,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
//
// PORT NOTE (§7.7): the reference is a self-referential heap struct — `handler:
// .{ .t = &ptr.terminal }` (the handler borrows the terminal) and `stream:
// .init(&ptr.handler)` (the stream borrows the handler). Zig's `Stream(*Handler)`
// parameterizes the stream over a *pointer* to the handler. In Rust `Stream<H>`
// owns its handler `H` by value; the behavior-identical model owns the `Handler`
// inside the `Stream` (collapsing the `*Handler` indirection) and models
// `Handler.t` as `*mut Terminal` (a raw ptr, faithful to Zig's `*Terminal`). The
// self-reference is fixed up after boxing, where the terminal's address is
// stable.
pub struct TerminalStream {
    opts: Options,
    terminal: Terminal,
    stream: Stream<'static, Handler>,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
}

// pub const Options = struct {
pub struct Options {
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
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            terminal_rows: 80,
            terminal_cols: 120,
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

impl TerminalStream {
    /// Create a new terminal stream handler for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*TerminalStream {
    //     const ptr = try alloc.create(TerminalStream);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{
    //         .opts = opts,
    //         .terminal = try .init(alloc, .{ .rows = ..., .cols = ... }),
    //         .handler = .{ .t = &ptr.terminal },
    //         .stream = .init(&ptr.handler),
    //     };
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<TerminalStream>, OutOfMemory> {
        let terminal = Terminal::init(
            alloc,
            terminal_options(opts.terminal_rows, opts.terminal_cols),
        )?;

        // Box first (so the terminal's address is stable), then patch the
        // self-reference: the handler's `*mut Terminal` points into `b.terminal`.
        let mut b = Box::new(TerminalStream {
            opts,
            terminal,
            stream: Stream::init(Handler {
                t: core::ptr::null_mut(),
            }),
            data_f: None,
        });
        let tptr: *mut Terminal = &mut b.terminal;
        b.stream = Stream::init(Handler { t: tptr });
        Ok(b)
    }

    // pub fn destroy(self: *TerminalStream, alloc: Allocator) void {
    //     self.terminal.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<TerminalStream>, alloc: Allocator) {
        self.terminal.deinit(alloc);
        drop(self);
    }

    // pub fn benchmark(self: *TerminalStream) Benchmark {
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
        let self_ = unsafe { &mut *(ptr as *mut TerminalStream) };

        // Always reset our terminal state
        // self.terminal.fullReset();
        self_.terminal.full_reset();

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
        let self_ = unsafe { &mut *(ptr as *mut TerminalStream) };
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn step(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalStream) };

        // Get our buffered reader so we're not predominantly
        // waiting on file IO. It'd be better to move this fully into
        // memory. If we're IO bound though that should show up on
        // the benchmark results and... I know writing this that we
        // aren't currently IO bound.
        // const f = self.data_f orelse return;
        //
        // PORT NOTE: the read source (`self_.data_f`) and the sink
        // (`self_.stream`) are distinct fields; we split the borrow so the read
        // loop can feed the stream without aliasing.
        let (data_f, stream) = (&mut self_.data_f, &mut self_.stream);
        let f = match data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
        // var f_reader = f.reader(&read_buf);
        // const r = &f_reader.interface;
        let r = f;

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
            // self.stream.nextSlice(buf[0..n]);
            stream.next_slice(&buf[..n]);
        }
        Ok(())
    }
}

/// Implements the handler interface for the terminal.Stream.
/// We should expand this to include more operations to make
/// our benchmark more realistic.
// const Handler = struct {
//     t: *Terminal,
//     pub fn vt(self, comptime action, value) void {
//         switch (action) {
//             .print => self.t.print(value.cp) catch |err| { log.warn(...) },
//             else => {},
//         }
//     }
// };
//
// PORT NOTE (§7.7): the reference's `vt(comptime action, value)` handler that
// only acts on `.print` maps to the `Handler` trait, overriding *only* `print`;
// every other trait method keeps its default no-op body (exactly the reference
// `else => {}`).
struct Handler {
    t: *mut Terminal,
}

impl rustty_vt::stream::Handler for Handler {
    // .print => self.t.print(value.cp) catch |err| { log.warn(...) },
    fn print(&mut self, cp: u32) {
        if let Err(err) = unsafe { (*self.t).print(cp) } {
            eprintln!("[{LOG_SCOPE}] error processing benchmark print err={err:?}");
        }
    }
}

// test TerminalStream {
#[test]
fn terminal_stream() {
    let alloc = Allocator;
    // const impl: *TerminalStream = try .create(alloc, .{});
    let mut impl_ = TerminalStream::create(alloc, Options::default()).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for TerminalStream {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, OutOfMemory> {
        TerminalStream::create(alloc, opts)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        TerminalStream::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        TerminalStream::benchmark(self)
    }
}
