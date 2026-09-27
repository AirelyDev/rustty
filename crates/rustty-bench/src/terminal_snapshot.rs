//! Benchmarks the terminal binary snapshot codecs (`terminal/snapshot`).
//!
//! Encoding and decoding a snapshot are the hot paths for terminal
//! persistence and handoff, so both directions are measured against the
//! same terminal state.
//!
//! ## Input
//!
//! `--data` names a pre-generated VT byte stream (for example from
//! `rustty-gen ascii`). The stream is fed to a terminal of the requested
//! dimensions during setup, outside the timed region. The resulting screen
//! and scrollback contents are what each step encodes or decodes.
//!
//! ## Modes
//!
//! * `noop` performs no codec work and establishes loop overhead.
//! * `encode` encodes one complete snapshot per loop into a reusable
//!   buffer. Buffer growth happens on the first iteration only.
//! * `decode` restores one complete snapshot per loop from bytes prepared
//!   during setup. The restored terminal is destroyed inside the step, so
//!   this measures the complete restore lifecycle.
//! * `decode-ready` restores only the renderable prefix per loop using the
//!   incremental decoder and stops at READY, skipping history. Compared
//!   against `decode`, this is the time-to-interactive win of applying
//!   history off the critical path.
//! * `report` encodes once and prints total and per-record-tag sizes. It
//!   is for inspecting the wire shape, not timing comparisons.
//!
//! ## Examples
//!
//! Build benchmarks in release mode:
//!
//! ```text
//! cargo build --release -p rustty-bench
//! ```
//!
//! Generate a deterministic corpus, then measure both directions:
//!
//! ```text
//! rustty-gen ascii | head -c 1000000 > /tmp/ascii.vt
//! hyperfine --warmup 3 \
//!   'rustty-bench +terminal-snapshot --mode=encode --loops=20 --data=/tmp/ascii.vt' \
//!   'rustty-bench +terminal-snapshot --mode=decode --loops=20 --data=/tmp/ascii.vt'
//! ```
//!
//! PORT NOTE (phase-4 wbench): this module ports
//! `benchmark/TerminalSnapshot.zig` (0b5e12453 + the e37865bed decode-ready
//! hunk).
//!
//! It was written against the snapshot codec drivers as DOCUMENTED in
//! `crates/rustty-vt/src/snapshot/codec.rs` before they landed, and lived
//! behind a `snapshot-codec` feature that CI never enabled, with the
//! `+terminal-snapshot` CLI action falling through to an `unimplemented!()`
//! leaf. `snapshot/{screen,terminal}.rs` and the drivers have since landed
//! (the wsnap-3 stage), but the module had drifted from the API that actually
//! shipped — six compile errors — so the gate was never lifted and the drift
//! was invisible. The gate is now gone and the call sites match the landed
//! signatures:
//!
//!   * `encode(Allocator, &mut dyn io::Writer, &Terminal, EncodeOptions)` —
//!     the reference's allocator parameter (restored by parity tail S7-8: the
//!     record scratch spills through it), and the sink is the type-erased
//!     writer, so the reusable buffer is an
//!     `io::AllocatingWriter` (the port's `std.Io.Writer.Allocating`) rather
//!     than a bare `Vec<u8>`.
//!   * `decode(&mut dyn io::Reader, Allocator, &DecodeOptions)` — reader
//!     first, options by reference.
//!   * `Decoder::ready(Allocator, &DecodeOptions)` — options by reference.
//!   * `FixedReader` lives in `snapshot::io`, not at `snapshot::`.
// const TerminalSnapshot = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const snapshot = terminalpkg.snapshot;
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
use rustty_vt::snapshot;
use rustty_vt::terminal::Terminal;

// const log = std.log.scoped(.@"terminal-snapshot-bench");
const LOG_SCOPE: &str = "terminal-snapshot-bench";

// alloc: Allocator,
// opts: Options,
// terminal: ?Terminal = null,
pub struct TerminalSnapshot {
    alloc: Allocator,
    opts: Options,
    terminal: Option<Terminal>,

    /// Reused across encode steps so buffer growth is a one-time setup cost.
    // encoded: std.Io.Writer.Allocating,
    //
    // PORT NOTE: `std.Io.Writer.Allocating` -> `snapshot::io::AllocatingWriter`,
    // the port's own analog of that type — `written()` is the slice and
    // `shrink_retaining_capacity(0)` is `shrinkRetainingCapacity(0)`, both
    // keeping capacity, so the reference's "growth is a one-time cost" property
    // holds. This was a bare `Vec<u8>` while the module was written against the
    // codec API as DOCUMENTED; the landed `snapshot::encode` takes
    // `&mut dyn io::Writer`, and AllocatingWriter is its allocating impl.
    encoded: snapshot::io::AllocatingWriter,
}

// pub const Options = struct {
//     /// Set by the shared CLI parser for string option ownership.
//     _arena: ?std.heap.ArenaAllocator = null,
//
// PORT NOTE (§6, terminal_stream precedent): the `_arena` + `deinit` pair
// exists for string option ownership; `data` is an owned `Vec<u8>` here, so
// Rust's automatic drop replaces both.
pub struct Options {
    /// Select the operation performed inside the timed benchmark step.
    // mode: Mode = .encode,
    mode: Mode,

    /// Number of codec operations per benchmark step. Increase this when
    /// the state is too small for stable `hyperfine` measurements.
    // loops: u32 = 1,
    loops: u32,

    /// The size of the terminal. This affects wrapping, page sizes, and
    /// the amount of state per encoded page.
    // @"terminal-rows": u16 = 24,
    // @"terminal-cols": u16 = 80,
    terminal_rows: u16,
    terminal_cols: u16,

    /// Pre-generated VT stream fed to the terminal during setup. `-` reads
    /// stdin, although a regular file is recommended so identical state can
    /// be reused across runs. When unset, the terminal is empty.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Encode,
            loops: 1,
            terminal_rows: 24,
            terminal_cols: 80,
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
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
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
    /// Establish the benchmark loop overhead.
    Noop,

    /// Encode one complete snapshot per loop.
    Encode,

    /// Restore one complete snapshot per loop, including its teardown.
    Decode,

    /// Restore one renderable READY prefix per loop, including teardown.
    // @"decode-ready",
    DecodeReady,

    /// Print encoded sizes by record tag. Not a timing benchmark.
    Report,
}

impl Mode {
    pub fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"encode" => Some(Mode::Encode),
            b"decode" => Some(Mode::Decode),
            b"decode-ready" => Some(Mode::DecodeReady),
            b"report" => Some(Mode::Report),
            _ => None,
        }
    }
}

impl TerminalSnapshot {
    // pub fn create(alloc: Allocator, opts: Options) !*TerminalSnapshot {
    //     const ptr = try alloc.create(TerminalSnapshot);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{
    //         .alloc = alloc,
    //         .opts = opts,
    //         .encoded = .init(alloc),
    //     };
    //     return ptr;
    // }
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<TerminalSnapshot>, OutOfMemory> {
        Ok(Box::new(TerminalSnapshot {
            alloc,
            opts,
            terminal: None,
            encoded: snapshot::io::AllocatingWriter::new(),
        }))
    }

    // pub fn destroy(self: *TerminalSnapshot, alloc: Allocator) void {
    //     if (self.terminal) |*t| t.deinit(self.alloc);
    //     self.encoded.deinit();
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<TerminalSnapshot>, alloc: Allocator) {
        if let Some(t) = self.terminal.as_mut() {
            t.deinit(self.alloc.into());
        }
        let _ = alloc;
        drop(self);
    }

    // pub fn benchmark(self: *TerminalSnapshot) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Encode => Self::step_encode,
                    Mode::Decode => Self::step_decode,
                    Mode::DecodeReady => Self::step_decode_ready,
                    Mode::Report => Self::step_report,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    /// Build the terminal state every mode shares and, for decode mode, the
    /// encoded snapshot it restores. All of this is outside the timed region.
    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        // self.setupImpl() catch |err| {
        //     log.warn("failed to prepare snapshot benchmark err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        self_.setup_impl().map_err(|err| {
            // log.warn("failed to prepare snapshot benchmark err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to prepare snapshot benchmark err={err:?}");
            Error::BenchmarkFailed
        })
    }

    // fn setupImpl(self: *TerminalSnapshot) !void {
    fn setup_impl(&mut self) -> Result<(), SetupError> {
        if let Some(t) = self.terminal.as_mut() {
            t.deinit(self.alloc.into());
        }
        self.terminal = None;
        // self.terminal = try Terminal.init(global.io(), self.alloc, .{
        //     .cols = self.opts.@"terminal-cols",
        //     .rows = self.opts.@"terminal-rows",
        //     .max_scrollback_bytes = null,
        //     .max_scrollback_lines = null,
        // });
        self.terminal = Some(Terminal::init(
            self.alloc.into(),
            rustty_vt::terminal::Options {
                max_scrollback_bytes: None,
                max_scrollback_lines: None,
                ..crate::terminal_options(self.opts.terminal_rows, self.opts.terminal_cols)
            },
        )?);
        let terminal = self.terminal.as_mut().unwrap();

        // Feed the input corpus through the standard VT stream.
        // if (try options.dataFile(self.opts.data)) |data_f| {
        if let Some(mut data_f) = options::data_file(self.opts.data.as_deref())? {
            // var stream = terminal.vtStream();
            // defer stream.deinit();
            let mut stream = terminal.vt_stream();

            // var read_buf: [4096]u8 align(std.atomic.cache_line) = undefined;
            // var f_reader = data_f.reader(global.io(), &read_buf);
            // const r = &f_reader.interface;
            //
            // PORT NOTE (§8, terminal_stream precedent): `read_slice_short`
            // reads straight from the `DataFile`; `buf` below is the dispatch
            // buffer that matters.
            let r = &mut data_f;

            // var buf: [4096]u8 = undefined;
            let mut buf = [0u8; 4096];
            // while (true) {
            loop {
                // const n = try r.readSliceShort(&buf);
                let n = match read_slice_short(r, &mut buf) {
                    Ok(n) => n,
                    Err(err) => {
                        stream.deinit();
                        return Err(SetupError::Io(err));
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
        }

        // Decode restores the same bytes every step. Encode mode also uses
        // this buffer, in which case this is its one-time growth.
        // self.encoded.shrinkRetainingCapacity(0);
        self.encoded.shrink_retaining_capacity(0);
        // try snapshot.encode(self.alloc, &self.encoded.writer, terminal, .{
        //     .continuation = .ground,
        // });
        snapshot::encode(
            self.alloc.into(),
            &mut self.encoded,
            terminal,
            snapshot::EncodeOptions {
                continuation: snapshot::Continuation::Ground,
            },
        )?;
        Ok(())
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        if let Some(t) = self_.terminal.as_mut() {
            t.deinit(self_.alloc.into());
        }
        self_.terminal = None;
        // self.encoded.shrinkRetainingCapacity(0);
        self_.encoded.shrink_retaining_capacity(0);
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        for _ in 0..self_.opts.loops {
            // std.mem.doNotOptimizeAway(self.encoded.written());
            black_box(self_.encoded.written());
        }
        Ok(())
    }

    // fn stepEncode(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_encode(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        // const terminal = &self.terminal.?;
        //
        // PORT NOTE: the encode sink (`self_.encoded`) and the terminal are
        // distinct fields; the borrows are split so the encoder can read the
        // terminal while writing the buffer.
        let (terminal, encoded) = (self_.terminal.as_mut().unwrap(), &mut self_.encoded);
        let alloc = self_.alloc.into();
        for _ in 0..self_.opts.loops {
            // self.encoded.shrinkRetainingCapacity(0);
            encoded.shrink_retaining_capacity(0);
            // snapshot.encode(self.alloc, &self.encoded.writer, terminal, .{
            //     .continuation = .ground,
            // }) catch |err| {
            //     log.warn("snapshot encoding failed err={}", .{err});
            //     return error.BenchmarkFailed;
            // };
            if let Err(err) = snapshot::encode(
                alloc,
                &mut *encoded,
                terminal,
                snapshot::EncodeOptions {
                    continuation: snapshot::Continuation::Ground,
                },
            ) {
                // log.warn("snapshot encoding failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "snapshot encoding failed err={err:?}");
                return Err(Error::BenchmarkFailed);
            }
            // std.mem.doNotOptimizeAway(self.encoded.written());
            black_box(encoded.written());
        }
        Ok(())
    }

    // fn stepDecode(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_decode(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        // const bytes = self.encoded.written();
        let bytes: &[u8] = self_.encoded.written();
        for _ in 0..self_.opts.loops {
            // var reader: std.Io.Reader = .fixed(bytes);
            let mut reader = snapshot::io::FixedReader::new(bytes);
            // var decoded = snapshot.decode(self.alloc, global.io(), &reader, .{
            //     .max_continuation_bytes = 1024 * 1024,
            // }) catch |err| {
            //     log.warn("snapshot decoding failed err={}", .{err});
            //     return error.BenchmarkFailed;
            // };
            let mut decoded = match snapshot::decode(
                &mut reader,
                self_.alloc.into(),
                &snapshot::DecodeOptions {
                    max_continuation_bytes: 1024 * 1024,
                },
            ) {
                Ok(d) => d,
                Err(err) => {
                    // log.warn("snapshot decoding failed err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "snapshot decoding failed err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            // std.mem.doNotOptimizeAway(&decoded);
            black_box(&decoded);
            // decoded.deinit(self.alloc);
            decoded.deinit(self_.alloc.into());
        }
        Ok(())
    }

    /// The renderable-prefix half of decode: everything through READY, leaving
    /// all history unread. The gap between this and `stepDecode` is the latency
    /// class the incremental decoder removes from time-to-interactive.
    // fn stepDecodeReady(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_decode_ready(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        // const bytes = self.encoded.written();
        let bytes: &[u8] = self_.encoded.written();
        for _ in 0..self_.opts.loops {
            // var reader: std.Io.Reader = .fixed(bytes);
            let mut reader = snapshot::io::FixedReader::new(bytes);
            // var decoder: snapshot.Decoder = .init(&reader);
            let mut decoder = snapshot::Decoder::init(&mut reader);
            // var decoded = decoder.ready(self.alloc, global.io(), .{
            //     .max_continuation_bytes = 1024 * 1024,
            // }) catch |err| {
            //     log.warn("snapshot READY decoding failed err={}", .{err});
            //     return error.BenchmarkFailed;
            // };
            let mut decoded = match decoder.ready(
                self_.alloc.into(),
                &snapshot::DecodeOptions {
                    max_continuation_bytes: 1024 * 1024,
                },
            ) {
                Ok(d) => d,
                Err(err) => {
                    // log.warn("snapshot READY decoding failed err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "snapshot READY decoding failed err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            // std.mem.doNotOptimizeAway(&decoded);
            black_box(&decoded);
            // decoded.deinit(self.alloc);
            decoded.deinit(self_.alloc.into());
        }
        Ok(())
    }

    /// Print the encoded size grouped by record tag. This shares the encoder
    /// with encode mode but deliberately makes no timing claims.
    // fn stepReport(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_report(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut TerminalSnapshot) };
        // const bytes = self.encoded.written();
        let bytes: &[u8] = self_.encoded.written();

        // var payload_totals = std.enums.EnumArray(snapshot.record.Tag, u64).initFill(0);
        // var record_counts = std.enums.EnumArray(snapshot.record.Tag, u64).initFill(0);
        //
        // PORT NOTE: `std.enums.EnumArray(Tag, u64)` -> a `[u64; 7]` indexed by
        // the tag's declaration order (`TAGS`); `initFill(0)` is the zeroed
        // array and the iterator below walks the same declaration order.
        const TAGS: [snapshot::record::Tag; 7] = [
            snapshot::record::Tag::Terminal,
            snapshot::record::Tag::Screen,
            snapshot::record::Tag::Page,
            snapshot::record::Tag::History,
            snapshot::record::Tag::Ready,
            snapshot::record::Tag::Finish,
            snapshot::record::Tag::Continuation,
        ];
        fn tag_index(tag: snapshot::record::Tag) -> usize {
            // Tag values are 1..=7 in declaration order.
            (tag as u16 - 1) as usize
        }
        let mut payload_totals = [0u64; 7];
        let mut record_counts = [0u64; 7];
        // var record_total: u64 = 0;
        let mut record_total: u64 = 0;

        // var offset: usize = snapshot.envelope.encoded_len;
        let mut offset: usize = snapshot::envelope::ENCODED_LEN;
        // while (offset + snapshot.record.Header.len <= bytes.len) {
        while offset + snapshot::record::Header::LEN <= bytes.len() {
            // const tag_raw = std.mem.readInt(u16, bytes[offset..][0..2], .little);
            let tag_raw = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            // const payload_len = std.mem.readInt(u32, bytes[offset + 2 ..][0..4], .little);
            let payload_len = u32::from_le_bytes([
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
            ]);
            // const tag = std.enums.fromInt(snapshot.record.Tag, tag_raw) orelse {
            //     log.warn("unknown record tag {}", .{tag_raw});
            //     return error.BenchmarkFailed;
            // };
            let tag = match snapshot::record::Tag::from_int(tag_raw) {
                Some(tag) => tag,
                None => {
                    // log.warn("unknown record tag {}", .{tag_raw});
                    log::warn!(target: LOG_SCOPE, "unknown record tag {tag_raw}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            // payload_totals.getPtr(tag).* += payload_len;
            payload_totals[tag_index(tag)] += u64::from(payload_len);
            // record_counts.getPtr(tag).* += 1;
            record_counts[tag_index(tag)] += 1;
            record_total += 1;
            // offset += snapshot.record.Header.len + payload_len;
            offset += snapshot::record::Header::LEN + payload_len as usize;
        }

        // var it = payload_totals.iterator();
        // while (it.next()) |entry| {
        //     std.debug.print("terminal-snapshot tag={s} records={d} payload={d}\n", .{
        //         @tagName(entry.key), record_counts.get(entry.key), entry.value.*,
        //     });
        // }
        for tag in TAGS {
            debug_print!(
                "terminal-snapshot tag={} records={} payload={}\n",
                tag.name(),
                record_counts[tag_index(tag)],
                payload_totals[tag_index(tag)],
            );
        }
        // std.debug.print("terminal-snapshot total records={d} framing={d} encoded={d}\n", .{
        //     record_total,
        //     record_total * snapshot.record.Header.len + snapshot.envelope.encoded_len,
        //     bytes.len,
        // });
        debug_print!(
            "terminal-snapshot total records={} framing={} encoded={}\n",
            record_total,
            record_total * snapshot::record::Header::LEN as u64
                + snapshot::envelope::ENCODED_LEN as u64,
            bytes.len(),
        );
        Ok(())
    }
}

/// The inferred error set of `setupImpl` (`!void`): terminal allocation
/// failures, data-file I/O errors, and encode errors.
///
/// The payloads are consumed by the `{err:?}` in `setup`'s warn; rustc's
/// dead-code pass deliberately ignores derived `Debug` as a use, hence the
/// allow (the page_compression precedent).
#[derive(Debug)]
#[allow(dead_code)]
enum SetupError {
    OutOfMemory,
    Io(std::io::Error),
    Encode(snapshot::EncodeError),
}

impl From<OutOfMemory> for SetupError {
    fn from(_: OutOfMemory) -> SetupError {
        SetupError::OutOfMemory
    }
}

impl From<std::io::Error> for SetupError {
    fn from(e: std::io::Error) -> SetupError {
        SetupError::Io(e)
    }
}

impl From<snapshot::EncodeError> for SetupError {
    fn from(e: snapshot::EncodeError) -> SetupError {
        SetupError::Encode(e)
    }
}

// test TerminalSnapshot {
#[test]
fn terminal_snapshot() {
    let alloc = Allocator;
    // const impl: *TerminalSnapshot = try .create(testing.allocator, .{});
    let mut impl_ = TerminalSnapshot::create(alloc, Options::default()).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

// test "TerminalSnapshot decode round trip" {
#[test]
fn terminal_snapshot_decode_round_trip() {
    let alloc = Allocator;
    let mut impl_ = TerminalSnapshot::create(
        alloc,
        Options {
            mode: Mode::Decode,
            terminal_rows: 4,
            terminal_cols: 8,
            ..Options::default()
        },
    )
    .unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

// test "TerminalSnapshot decode READY prefix" {
#[test]
fn terminal_snapshot_decode_ready_prefix() {
    let alloc = Allocator;
    let mut impl_ = TerminalSnapshot::create(
        alloc,
        Options {
            mode: Mode::DecodeReady,
            terminal_rows: 4,
            terminal_cols: 8,
            ..Options::default()
        },
    )
    .unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for TerminalSnapshot {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(TerminalSnapshot::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        TerminalSnapshot::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        TerminalSnapshot::benchmark(self)
    }
}
