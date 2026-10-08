//! This benchmark tests the performance of the terminal stream
//! handler from input to terminal state update. This is useful to
//! test general throughput of VT parsing and handling.
//!
//! This uses the full readonly terminal stream handler
//! (terminal.TerminalStream) so every escape sequence updates real
//! terminal state (styles, cursor movement, erases, modes, etc.).
//! This closely mirrors the work done by the real IO thread.
//!
//! For more isolated measurements see the terminal-parser and
//! osc-parser benchmarks.
// const TerminalStream = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const Terminal = terminalpkg.Terminal;
// const Stream = terminalpkg.TerminalStream;
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short, DataFile};
use crate::terminal_options;
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::stream_terminal::{Handler, Stream};
use rustty_vt::terminal::Terminal;

// PORT NOTE (258de36d1, WS10 T10): the reference's `Stream.initAlloc(alloc, ...)`
// threads the terminal's gpa into the OSC parser. `Terminal::vt_stream` is the
// rustty equivalent, but it returns `Stream<'_>` borrowed from `&mut self`,
// which cannot be stored beside the `Terminal` in one struct. So `create`
// reproduces `vt_stream`'s body against the boxed terminal's stable address,
// passing the terminal's allocator (the Rust heap) as `vt_stream` passes
// `gpa()` (parity tail S7-7; it used to be a process-static
// `GlobalZigAllocator` reference).

const LOG_SCOPE: &str = "terminal-stream-bench";

// opts: Options,
// terminal: Terminal,
// stream: Stream,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
//
// PORT NOTE (§7.7): the reference is a self-referential heap struct — the stream
// holds a handler that borrows `&ptr.terminal`. In Rust `Stream<H>` owns its
// handler `H` by value, and `stream_terminal::Handler` already models its
// terminal as a raw `*mut Terminal` (its own PORT NOTE explains why), so the
// handler is given the terminal's heap address (see the next note). 258de36d1 removed the separate `handler` field entirely; so does this.
//
// PORT NOTE (task C-1, NEW-FINDINGS N391): the terminal lives in its OWN heap
// allocation, held as the raw pointer the handler also holds, and every access
// goes through that pointer. Inline in the `Box<TerminalStream>`, the handler's
// `*mut Terminal` was invalidated under Stacked Borrows by the first move of the
// box (`Ok(b)` in `create`) and by any `&mut self.terminal` reborrow, so the
// next `Stream::next` wrote through a dead tag (Miri, G6).
pub struct TerminalStream {
    opts: Options,
    /// Owned: `Box::into_raw` in `create`, freed in `destroy`.
    terminal: *mut Terminal,
    stream: Stream<'static>,
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

    /// Enable opt-in continuation tracking on the stream.
    // @"continuation-enabled": bool = false,
    continuation_enabled: bool,

    /// Maximum continuation suffix retained when tracking is enabled.
    // @"continuation-max-bytes": usize = 1024 * 1024,
    continuation_max_bytes: usize,

    /// The most bytes to keep from each OSC sequence whose number the
    /// parser does not implement. Zero, the default, discards them. Any
    /// other value also installs an `unknown_sequence` callback that does
    /// nothing, so the benchmark measures the whole path from input to
    /// callback.
    // @"osc-unknown-max-bytes": usize = 0,
    osc_unknown_max_bytes: usize,

    /// Pre-generated data from rustty-gen. If this is "-" then
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
            continuation_enabled: false,
            continuation_max_bytes: 1024 * 1024,
            osc_unknown_max_bytes: 0,
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
            b"continuation-enabled" => {
                self.continuation_enabled = rustty_args::args::parse_field_bool(value)?;
                Ok(())
            }
            b"continuation-max-bytes" => {
                self.continuation_max_bytes = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            b"osc-unknown-max-bytes" => {
                self.osc_unknown_max_bytes = rustty_args::args::parse_field_int(value)?;
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

/// Stands in for an application that receives every unknown sequence
/// and does nothing with it.
// fn unknownSequence(
//     _: *Stream.Handler,
//     value: terminalpkg.UnknownSequence,
// ) void {
//     switch (value) {
//         inline else => |v| std.mem.doNotOptimizeAway(v.content.len),
//     }
// }
fn unknown_sequence(_: &mut Handler, value: rustty_vt::UnknownSequence) {
    match value {
        rustty_vt::UnknownSequence::Apc(v) => crate::do_not_optimize_away(v.content.len()),
        rustty_vt::UnknownSequence::Osc(v) => crate::do_not_optimize_away(v.content.len()),
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
    //         .stream = undefined,
    //     };
    //     ptr.stream = .initAlloc(alloc, .init(&ptr.terminal));
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<TerminalStream>, OutOfMemory> {
        let terminal = Terminal::init(
            alloc.into(),
            terminal_options(opts.terminal_rows, opts.terminal_cols),
        )?;

        // The terminal is boxed on its own (so its address is stable and no
        // move of the TerminalStream retags it), and the handler gets that
        // pointer. This is the port's spelling of the reference's `.stream = undefined;`
        // followed by:
        //     errdefer ptr.terminal.deinit(alloc);
        //     ptr.stream = .init(.{
        //         .allocator = alloc,
        //         .handler = .init(&ptr.terminal),
        //     var handler: Stream.Handler = .init(&ptr.terminal);
        //     if (opts.@"osc-unknown-max-bytes" > 0) {
        //         handler.unknown_sequence = &unknownSequence;
        //     }
        //     ptr.stream = .init(.{
        //         .allocator = alloc,
        //         .handler = handler,
        //         .continuation_max_bytes = if (opts.@"continuation-enabled")
        //             opts.@"continuation-max-bytes"
        //         else
        //             null,
        //         .osc_unknown_max_bytes = opts.@"osc-unknown-max-bytes",
        //     });
        // (The `errdefer` is subsumed: nothing after boxing can fail.)
        let continuation_max_bytes = if opts.continuation_enabled {
            Some(opts.continuation_max_bytes)
        } else {
            None
        };
        let osc_unknown_max_bytes = opts.osc_unknown_max_bytes;
        let tptr: *mut Terminal = Box::into_raw(Box::new(terminal));
        let mut handler = Handler::init(tptr);
        if osc_unknown_max_bytes > 0 {
            handler.unknown_sequence = Some(unknown_sequence);
        }
        Ok(Box::new(TerminalStream {
            opts,
            terminal: tptr,
            stream: Stream::init(rustty_vt::stream::Options {
                allocator: Some(rustty_vt::alloc::Global),
                handler,
                continuation_max_bytes,
                osc_unknown_max_bytes,
            }),
            data_f: None,
        }))
    }

    // pub fn destroy(self: *TerminalStream, alloc: Allocator) void {
    //     self.stream.deinit();
    //     self.terminal.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<TerminalStream>, alloc: Allocator) {
        // 258de36d1 adds this: the full handler owns an APC handler and an OSC
        // parser with allocated state, which the old print-only handler did not.
        self.stream.deinit();
        // SAFETY: `terminal` came from `Box::into_raw` in `create`; the stream
        // that also pointed at it is deinitialized above.
        unsafe {
            (*self.terminal).deinit(alloc.into());
            drop(Box::from_raw(self.terminal));
        }
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
        // SAFETY: the terminal is live for the TerminalStream's lifetime.
        unsafe { (*self_.terminal).full_reset() };

        // Open our data file to prepare for reading. We can do more validation
        // here eventually.
        // assert(self.data_f == null);
        assert!(self_.data_f.is_none());
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
        // Unbuffered: readSliceShort below reads directly into `buf`,
        // avoiding a per-chunk memcpy through an intermediate reader
        // buffer that would pollute the measurement.
        // var f_reader = f.reader(global.io(), &.{});
        // const r = &f_reader.interface;
        let mut r = f.unbuffered();

        // This buffer size matches the read buffer size used by the
        // real IO thread (see termio Exec.zig buffer_capacity) so that
        // the benchmark exercises the stream with realistic chunk sizes.
        // var buf: [64 * 1024]u8 = undefined;
        //
        // PORT NOTE (4c5b1d5d5): the reference's final shape is an UNBUFFERED
        // `std.Io.Reader` plus this 64 KiB dispatch buffer. `DataFile::unbuffered`
        // is the unbuffered half: `read_slice_short` reads straight from the
        // file into `buf`, so this is the only buffer. It determines how often
        // the stream crosses a chunk boundary, which is the measurement-relevant
        // half of the reference's change. (Before task S-4 this read went
        // through `DataFile`'s 256 KiB `BufReader`, which a 64 KiB request does
        // not bypass: one extra memcpy of the whole corpus, measured at about
        // 3% of the bench's samples.)
        let mut buf = [0u8; 64 * 1024];
        // while (true) {
        loop {
            // const n = r.readSliceShort(&buf) catch { ... return error.BenchmarkFailed; };
            let n = match read_slice_short(&mut r, &mut buf) {
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
            // self.stream.nextSlice(buf[0..n]);
            stream.next_slice(&buf[..n]);
        }
        Ok(())
    }
}

// 258de36d1 DELETED the local print-only `Handler` here. It is gone rather than
// kept-and-unused: the whole point of the commit is that the simplified handler
// understated escape-heavy streams, because Zig dispatches actions at comptime
// and so eliminated every unhandled arm — measuring dispatch code that does not
// exist in the real app. The full `stream_terminal::Handler` replaces it.

// rustty-only test (258de36d1 + 4c5b1d5d5, WS10 T10). NOT a mapped Zig `test{}`
// — the reference ships both commits with only the pre-existing smoke test
// below, which passes with EITHER handler.
//
// WHY IT EXISTS. The WS10 plan asserts that after the handler swap "the medians
// MUST move — otherwise the handler swap did not take effect, which is the
// vacuous-verification trap here". That inference is FALSE for this repo's
// corpus, and the corpus is the whole benchmark:
//
//   .port/phase-0/baseline/corpora/ascii.bin — 67 108 864 bytes, ESC count 0,
//   non-printable byte count 0.
//
// Every byte of `ascii.bin` is a printable ASCII codepoint, so BOTH handlers do
// exactly the same work on it: `print` flows into the same `Terminal::print`
// call either way, which is precisely what the upstream commit message says
// ("print throughput is identical under both handlers"). A neutral median is
// therefore the CORRECT outcome here and proves nothing either way — the trap
// the plan warns about is real, but the timing check is not the escape from it.
//
// So the swap is proven behaviourally instead: drive a stream carrying an
// escape sequence THROUGH THE BENCHMARK'S OWN step function and assert the
// terminal state it updates. The deleted print-only handler ignored every
// non-print action, so it could not have moved the cursor.
#[test]
// MIRI EXCEPTION (rustty-bench terminal_stream, file I/O class): the step
// function reads its corpus from a real file, so this test `File::create`s one
// — and Miri's isolation forbids `open` ("`open` not available when isolation
// is enabled"), aborting the whole rustty-bench lib binary. Same class as
// `rustty-vaxis/tests/ucd_provenance.rs`. Registered in
// .port/phase-2/MIRI_EXCEPTIONS.md ("rustty-bench — the 2026-07-31 CI triage").
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench): writes/reads a real corpus \
     file; Miri isolation forbids `open` — see MIRI_EXCEPTIONS.md"
)]
fn terminal_stream_uses_the_full_terminal_handler() {
    use std::io::Write as _;

    let alloc = Allocator;

    // "AB", then CUP to row 2 / col 3 (1-based), then "X".
    //   full handler:    X lands at (x=2, y=1); cursor advances to (x=3, y=1)
    //   old print-only:  the CSI is ignored -> "ABX" on row 0, cursor (x=3, y=0)
    // The ROW is the discriminator; the column coincides.
    let mut path = std::env::temp_dir();
    path.push(format!(
        "rustty-ws10-t10-{}-{:?}.bin",
        std::process::id(),
        std::thread::current().id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"AB\x1b[2;3HX").unwrap();
    }

    let opts = Options {
        data: Some(path.as_os_str().as_encoded_bytes().to_vec()),
        ..Options::default()
    };

    let mut impl_ = TerminalStream::create(alloc, opts).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();

    let cursor = unsafe { &(*(*impl_.terminal).screens.active).cursor };
    assert_eq!(
        (3u16, 1u16),
        (cursor.x, cursor.y),
        "the CSI 2;3H in the stream must have moved the cursor to row 1 — with \
         the deleted print-only handler it would be at (3, 0)"
    );

    impl_.destroy(alloc);
    let _ = std::fs::remove_file(&path);
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

    // const tracked: *TerminalStream = try .create(alloc, .{
    //     .@"continuation-enabled" = true,
    // });
    let mut tracked = TerminalStream::create(
        alloc,
        Options {
            continuation_enabled: true,
            ..Options::default()
        },
    )
    .unwrap();
    let tracked_bench = tracked.benchmark();
    let _ = tracked_bench.run(crate::benchmark::RunMode::Once).unwrap();
    tracked.destroy(alloc);
}

impl BenchmarkAction for TerminalStream {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(TerminalStream::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        TerminalStream::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        TerminalStream::benchmark(self)
    }
}
