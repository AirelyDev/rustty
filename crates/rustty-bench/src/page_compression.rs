//! Benchmarks raw LZ4 compression and decompression on page-sized byte
//! buffers.
//!
//! This benchmark is intentionally independent of terminal page ownership and
//! lifecycle. It treats its input as opaque bytes and calls only the standalone
//! LZ4 block codec. In particular, it does not compress pages owned by a live
//! terminal or measure the scheduling and state transitions used by automatic
//! scrollback compression.
//!
//! ## Input
//!
//! `--data` names a pre-generated raw byte corpus. The corpus is divided into
//! `--page-size` byte chunks, with a final short chunk retained when the file
//! size is not an exact multiple. The default page size is 400 KiB, matching a
//! standard terminal page in ReleaseFast builds on the current target.
//!
//! A raw dump of actual page backing memory is the most representative input:
//! it includes cells, rows, styles, graphemes, hyperlinks, allocator metadata,
//! and unused capacity exactly as the codec would see them. Keep such corpora
//! outside the repository and reuse the same file when comparing branches.
//! Arbitrary files are accepted too, but their compression ratios should not be
//! interpreted as terminal scrollback ratios.
//!
//! ## Modes
//!
//! * `noop` walks the input chunks without invoking the codec. This measures the
//!   benchmark loop's minimum overhead.
//! * `compress` compresses every input chunk into a reusable output buffer.
//! * `store` additionally copies each useful result into an exact-sized
//!   allocation. It retains the most recent `--retained-pages` blocks so that
//!   allocation, eviction, and allocator reuse resemble bounded scrollback.
//! * `decompress` prepares compressed blocks during setup, then decompresses
//!   every block into a reusable output buffer.
//! * `report` compresses each chunk once and prints raw and encoded sizes. It is
//!   for inspecting ratios, not timing comparisons.
//!
//! Dataset loading, output allocation, and preparation of blocks for
//! decompression happen in `setup` and are outside `Benchmark`'s timed region.
//! The `compress` and `decompress` steps perform no allocation. `store`
//! deliberately performs encoded-block allocations and frees inside the timed
//! step; only its reusable workspace and ring metadata are prepared in setup.
//! `hyperfine` still measures full process lifetime, so use `--loops` to
//! amortize setup and teardown when comparing small corpora.
//!
//! ## Examples
//!
//! PORT NOTE: the reference's shell snippets are 4-space-indented blocks, which
//! rustdoc would treat as Rust doctests; they are fenced as `text` here.
//!
//! Build benchmarks in release mode:
//!
//! ```text
//! cargo build -p rustty-bench --profile release-fast
//! ```
//!
//! Inspect the compression ratio of a page corpus:
//!
//! ```text
//! rustty-bench +page-compression --mode=report --data=/tmp/pages.raw
//! ```
//!
//! Measure the cost of exact encoded storage relative to the codec alone:
//!
//! ```text
//! hyperfine --warmup 3 \
//!   'rustty-bench +page-compression --mode=compress --loops=100 --data=/tmp/pages.raw' \
//!   'rustty-bench +page-compression --mode=store --loops=100 --data=/tmp/pages.raw'
//! ```
//!
//! PORTS: ghostty src/benchmark/PageCompression.zig (oracle origin/main).
// const PageCompression = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const compress = @import("../terminal/compress.zig");
// const CompressedPage = compress.Page;
// const lz4 = compress.lz4;
// const compat_file = @import("../lib/compat/file.zig");
// const global = @import("../global.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::BenchmarkAction;
use crate::options::{self, read_slice_short};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_vt::compress::{self, lz4};
use rustty_vt::pagelist::OutOfMemory;

// const log = std.log.scoped(.@"page-compression-bench");
const LOG_SCOPE: &str = "page-compression-bench";

/// Prevent a malformed or accidentally enormous corpus from consuming
/// unbounded memory during benchmark setup.
// const max_data_size = 64 * 1024 * 1024;
const MAX_DATA_SIZE: usize = 64 * 1024 * 1024;

// alloc: Allocator,
// opts: Options,
// data: []u8 = &.{},
// encoded: std.ArrayList(Encoded) = .empty,
// stored: []?[]u8 = &.{},
// stored_next: usize = 0,
// compression_output: []u8 = &.{},
// decompression_output: []u8 = &.{},
// table: lz4.HashTable = undefined,
//
// PORT NOTE (WS7 D8): the reference's manual `alloc.free` bookkeeping in
// `clearPreparedData` is Rust's `Drop` on owned `Vec`s. The oracle keeps
// `clearPreparedData` shared between `teardown` and `destroy` so error paths and
// direct unit-test use stay leak-free; the Rust equivalent resets the owning
// fields to empty, which frees them, and drop covers `destroy`.
pub struct PageCompression {
    opts: Options,

    /// Complete contents of the input corpus. Individual pages are ranges into
    /// this allocation, so it remains alive until teardown.
    ///
    /// PORT NOTE: the reference stores `[]const u8` slices into `data` inside
    /// its `PageIterator`. Rust cannot hold a borrow of `self.data` inside
    /// `self`, so `PageIterator` yields `(offset, len)` ranges and callers index
    /// `self.data`. Identical traversal, no aliasing.
    data: Vec<u8>,

    /// Compressed blocks prepared during setup for `decompress` mode.
    encoded: Vec<Encoded>,

    /// Exact encoded allocations retained by `store` mode. `None` entries have
    /// not been reached yet; once full, `stored_next` identifies the oldest
    /// block.
    stored: Vec<Option<Vec<u8>>>,
    stored_next: usize,

    /// Reused by compression and report modes. Its length is the compression
    /// bound of the largest input chunk.
    compression_output: Vec<u8>,

    /// Reused by decompression mode. Its length is at least one input chunk.
    decompression_output: Vec<u8>,

    /// Fixed 16 KiB scratch table required by the compressor.
    table: Box<lz4::HashTable>,
}

// pub const Options = struct {
pub struct Options {
    /// Select the operation performed inside the timed benchmark step.
    // mode: Mode = .compress,
    mode: Mode,

    /// Repeat the complete corpus this many times per benchmark step. Increase
    /// this when the corpus is too small for stable `hyperfine` measurements.
    // loops: u32 = 1,
    loops: u32,

    /// Number of bytes treated as one independent LZ4 block. Real page dumps
    /// should use the exact backing-memory size of the pages being measured.
    // @"page-size": usize = 400 * 1024,
    page_size: usize,

    /// Number of exact encoded allocations retained by `store` mode before
    /// the oldest allocation is freed and replaced. The default approximates
    /// a 10 MB scrollback composed of standard 400 KiB terminal pages.
    // @"retained-pages": usize = 25,
    retained_pages: usize,

    /// Pre-generated input corpus. `-` reads stdin, although a regular file is
    /// recommended so identical bytes can be reused across benchmark runs.
    /// When unset, all modes are no-ops.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Compress,
            loops: 1,
            page_size: 400 * 1024,
            retained_pages: 25,
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
            // mode: Mode
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
            // loops: u32
            b"loops" => {
                self.loops = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            // @"page-size": usize
            b"page-size" => {
                self.page_size = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            // @"retained-pages": usize
            b"retained-pages" => {
                self.retained_pages = rustty_args::args::parse_field_int(value)?;
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

// pub const Mode = enum {
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Walk page boundaries and establish the benchmark loop overhead.
    // noop,
    Noop,

    /// Compress each raw page into a reusable compression-bound buffer.
    // compress,
    Compress,

    /// Compress and retain exact-sized encoded blocks in a bounded ring.
    // store,
    Store,

    /// Decompress blocks prepared before the timed region.
    // decompress,
    Decompress,

    /// Print per-page and aggregate encoded sizes. Not a timing benchmark.
    // report,
    Report,
}

impl Mode {
    /// `std.meta.stringToEnum(Mode, name)` — the kebab CLI tokens.
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"compress" => Some(Mode::Compress),
            b"store" => Some(Mode::Store),
            b"decompress" => Some(Mode::Decompress),
            b"report" => Some(Mode::Report),
            _ => None,
        }
    }
}

// const Encoded = struct {
struct Encoded {
    /// Exact compressed block bytes.
    // bytes: []u8,
    bytes: Vec<u8>,

    /// Exact output length expected by the raw block decoder.
    // raw_len: usize,
    raw_len: usize,
}

/// The error set of `setupData` (Zig's inferred `!void` over the codec errors,
/// `readToEndAlloc`, `OutOfMemory`, and the three validation errors).
///
/// The payloads are consumed by the `{err:?}` in `setup`'s warn (the reference's
/// `log.warn("... err={}", .{err})`); rustc's dead-code pass deliberately ignores
/// derived `Debug` as a use, hence the allow.
#[derive(Debug)]
#[allow(dead_code)]
enum SetupError {
    // return error.InvalidLoops;
    InvalidLoops,
    // return error.InvalidPageSize;
    InvalidPageSize,
    // return error.InvalidRetainedPages;
    InvalidRetainedPages,
    // try options.dataFile(...) / readToEndAlloc
    Io(std::io::Error),
    // try lz4.compressBound / lz4.compress
    Compress(lz4::CompressError),
    // _ = try lz4.decompress(...)
    Decompress(lz4::DecompressError),
    // if (!std.mem.eql(u8, page, output)) return error.RoundTripMismatch;
    RoundTripMismatch,
}

impl From<std::io::Error> for SetupError {
    fn from(e: std::io::Error) -> SetupError {
        SetupError::Io(e)
    }
}

impl From<lz4::CompressError> for SetupError {
    fn from(e: lz4::CompressError) -> SetupError {
        SetupError::Compress(e)
    }
}

impl From<lz4::DecompressError> for SetupError {
    fn from(e: lz4::DecompressError) -> SetupError {
        SetupError::Decompress(e)
    }
}

impl PageCompression {
    /// Allocate benchmark state. Input data is intentionally loaded later by
    /// `setup` so construction is cheap and follows the other benchmarks.
    // pub fn create(alloc: Allocator, opts: Options) !*PageCompression {
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<PageCompression>, OutOfMemory> {
        let _ = alloc;
        Ok(Box::new(PageCompression {
            opts,
            data: Vec::new(),
            encoded: Vec::new(),
            stored: Vec::new(),
            stored_next: 0,
            compression_output: Vec::new(),
            decompression_output: Vec::new(),
            // table: lz4.HashTable = undefined
            //
            // PORT NOTE: boxed because `HashTable` is 16 KiB; the reference's
            // inline `undefined` field lives in the heap-allocated impl struct
            // either way. `compress` resets the table before use, so the initial
            // contents are irrelevant (the reference's `undefined`).
            table: Box::new([0u32; lz4::HASH_TABLE_LEN]),
        }))
    }

    /// Release allocations retained across benchmark steps.
    // pub fn destroy(self: *PageCompression, alloc: Allocator) void {
    //     self.clearPreparedData();
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<PageCompression>, alloc: Allocator) {
        let _ = alloc;
        self.clear_prepared_data();
        drop(self);
    }

    /// Select one operation for the benchmark harness to time.
    // pub fn benchmark(self: *PageCompression) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) { ... },
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Compress => Self::step_compress,
                    Mode::Store => Self::step_store,
                    Mode::Decompress => Self::step_decompress,
                    Mode::Report => Self::step_report,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    /// Load and partition the input corpus. For decompression mode this also
    /// creates the encoded blocks, keeping compression outside the timed region.
    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        // assert(self.data.len == 0);
        assert!(self_.data.is_empty());
        // assert(self.encoded.items.len == 0);
        assert!(self_.encoded.is_empty());

        // self.setupData() catch |err| {
        //     log.warn("failed to prepare page compression benchmark err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        if let Err(err) = self_.setup_data() {
            eprintln!("[{LOG_SCOPE}] failed to prepare page compression benchmark err={err:?}");
            return Err(Error::BenchmarkFailed);
        }
        Ok(())
    }

    // fn setupData(self: *PageCompression) !void {
    fn setup_data(&mut self) -> Result<(), SetupError> {
        // if (self.opts.loops == 0) return error.InvalidLoops;
        if self.opts.loops == 0 {
            return Err(SetupError::InvalidLoops);
        }
        // if (self.opts.@"page-size" == 0) return error.InvalidPageSize;
        if self.opts.page_size == 0 {
            return Err(SetupError::InvalidPageSize);
        }
        // if (self.opts.mode == .store and self.opts.@"retained-pages" == 0)
        //     return error.InvalidRetainedPages;
        if self.opts.mode == Mode::Store && self.opts.retained_pages == 0 {
            return Err(SetupError::InvalidRetainedPages);
        }

        // const data_file = try options.dataFile(self.opts.data) orelse return;
        // defer data_file.close(global.io());
        let mut data_file = match options::data_file(self.opts.data.as_deref())? {
            Some(f) => f,
            None => return Ok(()),
        };

        // self.data = try compat_file.readToEndAlloc(data_file, self.alloc, max_data_size);
        // errdefer { self.alloc.free(self.data); self.data = &.{}; }
        //
        // PORT NOTE: `readToEndAlloc` errors with `StreamTooLong` past the limit;
        // we read at most `MAX_DATA_SIZE + 1` bytes and reject an over-long
        // corpus the same way. The `errdefer` is Rust's drop on the error return.
        self.data = read_to_end_limited(&mut data_file, MAX_DATA_SIZE)?;
        // if (self.data.len == 0) return;
        if self.data.is_empty() {
            return Ok(());
        }
        // if (self.opts.mode == .noop) return;
        if self.opts.mode == Mode::Noop {
            return Ok(());
        }

        // const largest_page = @min(self.opts.@"page-size", self.data.len);
        let largest_page = self.opts.page_size.min(self.data.len());
        // self.compression_output = try self.alloc.alloc(u8, try lz4.compressBound(largest_page));
        self.compression_output = vec![0u8; lz4::compress_bound(largest_page)?];

        // if (self.opts.mode == .store) try self.prepareStored();
        if self.opts.mode == Mode::Store {
            self.prepare_stored();
        }
        // if (self.opts.mode == .decompress) try self.prepareEncoded();
        if self.opts.mode == Mode::Decompress {
            self.prepare_encoded()?;
        }
        Ok(())
    }

    /// Allocate only the ring metadata for `store` mode. The encoded blocks which
    /// populate it are intentionally allocated by the timed benchmark step.
    // fn prepareStored(self: *PageCompression) !void {
    //     self.stored = try self.alloc.alloc(?[]u8, self.opts.@"retained-pages");
    //     @memset(self.stored, null);
    //     self.stored_next = 0;
    // }
    fn prepare_stored(&mut self) {
        self.stored = (0..self.opts.retained_pages).map(|_| None).collect();
        self.stored_next = 0;
    }

    /// Precompress every input page and verify one decode before benchmarking.
    /// This catches corpus or codec problems before the timer starts.
    // fn prepareEncoded(self: *PageCompression) !void {
    fn prepare_encoded(&mut self) -> Result<(), SetupError> {
        // self.decompression_output = try self.alloc.alloc(
        //     u8, @min(self.opts.@"page-size", self.data.len));
        // errdefer { ... }
        self.decompression_output = vec![0u8; self.opts.page_size.min(self.data.len())];

        // var it = self.pages();
        // while (it.next()) |page| { ... }
        let mut it = self.pages();
        while let Some((off, len)) = it.next() {
            // const encoded_len = try lz4.compress(page, self.compression_output, &self.table);
            let encoded_len = lz4::compress(
                &self.data[off..off + len],
                &mut self.compression_output,
                &mut self.table,
            )?;
            // const encoded = try self.alloc.dupe(u8, self.compression_output[0..encoded_len]);
            // self.encoded.append(self.alloc, .{ .bytes = encoded, .raw_len = page.len }) catch ...
            self.encoded.push(Encoded {
                bytes: self.compression_output[..encoded_len].to_vec(),
                raw_len: len,
            });
        }

        // var page_it = self.pages();
        // for (self.encoded.items) |block| {
        //     const page = page_it.next().?;
        //     const output = self.decompression_output[0..block.raw_len];
        //     _ = try lz4.decompress(block.bytes, output);
        //     if (!std.mem.eql(u8, page, output)) return error.RoundTripMismatch;
        // }
        let mut page_it = self.pages();
        for i in 0..self.encoded.len() {
            let (off, len) = page_it.next().unwrap();
            let raw_len = self.encoded[i].raw_len;
            // NOTE: the two disjoint field borrows are split so the decode can
            // write `decompression_output` while reading `encoded[i].bytes`.
            let (encoded, output) = (&self.encoded[i].bytes, &mut self.decompression_output);
            lz4::decompress(encoded, &mut output[..raw_len])?;
            if self.data[off..off + len] != self.decompression_output[..raw_len] {
                return Err(SetupError::RoundTripMismatch);
            }
        }
        Ok(())
    }

    /// Release everything created by setup. This is shared by teardown and
    /// destroy so errors and direct unit-test use remain leak-free.
    // fn clearPreparedData(self: *PageCompression) void {
    fn clear_prepared_data(&mut self) {
        // for (self.encoded.items) |block| self.alloc.free(block.bytes);
        // self.encoded.deinit(self.alloc); self.encoded = .empty;
        self.encoded = Vec::new();

        // for (self.stored) |block| if (block) |bytes| self.alloc.free(bytes);
        // if (self.stored.len > 0) self.alloc.free(self.stored);
        // self.stored = &.{}; self.stored_next = 0;
        self.stored = Vec::new();
        self.stored_next = 0;

        // if (self.compression_output.len > 0) self.alloc.free(self.compression_output);
        // self.compression_output = &.{};
        self.compression_output = Vec::new();

        // if (self.decompression_output.len > 0) self.alloc.free(self.decompression_output);
        // self.decompression_output = &.{};
        self.decompression_output = Vec::new();

        // if (self.data.len > 0) self.alloc.free(self.data);
        // self.data = &.{};
        self.data = Vec::new();
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        self_.clear_prepared_data();
    }

    /// Baseline mode: traverse exactly the same page boundaries as compression
    /// without invoking the codec.
    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        // for (0..self.opts.loops) |_| {
        for _ in 0..self_.opts.loops {
            // var it = self.pages();
            // while (it.next()) |page| std.mem.doNotOptimizeAway(page);
            let mut it = self_.pages();
            while let Some((off, len)) = it.next() {
                black_box(&self_.data[off..off + len]);
            }
        }
        Ok(())
    }

    /// Compress all pages into one reusable output buffer. Only the returned
    /// encoded length is consumed because retaining output pages would measure
    /// allocation and ownership rather than codec throughput.
    // fn stepCompress(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_compress(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        // for (0..self.opts.loops) |_| {
        for _ in 0..self_.opts.loops {
            let mut it = self_.pages();
            // while (it.next()) |page| {
            while let Some((off, len)) = it.next() {
                // const encoded_len = lz4.compress(page, self.compression_output, &self.table)
                //     catch |err| { log.warn(...); return error.BenchmarkFailed; };
                //
                // NOTE: `data` (read) and `compression_output`/`table` (written)
                // are disjoint fields; the split borrow makes that explicit.
                let PageCompression {
                    data,
                    compression_output,
                    table,
                    ..
                } = &mut *self_;
                let encoded_len =
                    match lz4::compress(&data[off..off + len], compression_output, table) {
                        Ok(n) => n,
                        Err(err) => {
                            eprintln!("[{LOG_SCOPE}] page compression failed err={err:?}");
                            return Err(Error::BenchmarkFailed);
                        }
                    };
                // std.mem.doNotOptimizeAway(encoded_len);
                black_box(encoded_len);
            }
        }
        Ok(())
    }

    /// Compress pages and retain their exact encoded allocations in a bounded
    /// ring. This models the part of `compress.Page.init` which differs from the
    /// allocation-free codec benchmark: copying the result, allocating its
    /// persistent storage, and freeing an old block when scrollback is full.
    // fn stepStore(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_store(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        // if (self.data.len == 0) return;
        if self_.data.is_empty() {
            return Ok(());
        }
        // assert(self.stored.len > 0);
        assert!(!self_.stored.is_empty());

        // for (0..self.opts.loops) |_| {
        for _ in 0..self_.opts.loops {
            let mut it = self_.pages();
            // while (it.next()) |page| {
            while let Some((off, len)) = it.next() {
                // const output_len = CompressedPage.requiredScratch(page.len) catch |err| {
                //     log.warn("failed to size stored page output err={}", .{err});
                //     return error.BenchmarkFailed;
                // };
                let output_len = match compress::required_scratch(len) {
                    Ok(n) => n,
                    Err(err) => {
                        eprintln!("[{LOG_SCOPE}] failed to size stored page output err={err:?}");
                        return Err(Error::BenchmarkFailed);
                    }
                };
                // if (output_len == 0) continue;
                if output_len == 0 {
                    continue;
                }

                // const encoded_len = lz4.compress(
                //     page, self.compression_output[0..output_len], &self.table) catch |err| ...
                let PageCompression {
                    data,
                    compression_output,
                    table,
                    ..
                } = &mut *self_;
                let encoded_len = match lz4::compress(
                    &data[off..off + len],
                    &mut compression_output[..output_len],
                    table,
                ) {
                    Ok(n) => n,
                    // This is the same profitability outcome as
                    // compress.Page.init: a block which exceeds the useful
                    // output limit remains resident and consumes no encoded
                    // allocation.
                    // error.OutputTooSmall => continue,
                    Err(lz4::CompressError::OutputTooSmall) => continue,
                    // error.InputTooLarge => { log.warn(...); return error.BenchmarkFailed; },
                    Err(err) => {
                        eprintln!("[{LOG_SCOPE}] stored page compression failed err={err:?}");
                        return Err(Error::BenchmarkFailed);
                    }
                };

                // const encoded = self.alloc.dupe(u8, self.compression_output[0..encoded_len])
                //     catch |err| { log.warn(...); return error.BenchmarkFailed; };
                let encoded = self_.compression_output[..encoded_len].to_vec();

                // const slot = &self.stored[self.stored_next];
                // if (slot.*) |previous| self.alloc.free(previous);
                // slot.* = encoded;
                // self.stored_next = (self.stored_next + 1) % self.stored.len;
                let next = self_.stored_next;
                self_.stored[next] = Some(encoded);
                self_.stored_next = (next + 1) % self_.stored.len();
                // std.mem.doNotOptimizeAway(encoded);
                black_box(self_.stored[next].as_deref());
            }
        }
        Ok(())
    }

    /// Decompress blocks prepared by setup. The output allocation is reused so
    /// this measures only decoding and the required memory writes.
    // fn stepDecompress(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_decompress(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        // for (0..self.opts.loops) |_| {
        for _ in 0..self_.opts.loops {
            // for (self.encoded.items) |block| {
            let PageCompression {
                encoded,
                decompression_output,
                ..
            } = &mut *self_;
            for block in encoded.iter() {
                // const output = self.decompression_output[0..block.raw_len];
                let output = &mut decompression_output[..block.raw_len];
                // _ = lz4.decompress(block.bytes, output) catch |err| {
                //     log.warn("page decompression failed err={}", .{err});
                //     return error.BenchmarkFailed;
                // };
                if let Err(err) = lz4::decompress(&block.bytes, output) {
                    eprintln!("[{LOG_SCOPE}] page decompression failed err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
                // std.mem.doNotOptimizeAway(output);
                black_box(&*output);
            }
        }
        Ok(())
    }

    /// Print size information for evaluating compression ratio. This shares the
    /// input and codec paths with compression mode but deliberately makes no
    /// timing claims.
    // fn stepReport(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_report(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut PageCompression) };
        // if (self.data.len == 0) return;
        if self_.data.is_empty() {
            return Ok(());
        }

        // var page_index: usize = 0;
        // var raw_total: usize = 0;
        // var encoded_total: usize = 0;
        let mut page_index: usize = 0;
        let mut raw_total: usize = 0;
        let mut encoded_total: usize = 0;
        // var it = self.pages();
        let mut it = self_.pages();
        // while (it.next()) |page| : (page_index += 1) {
        while let Some((off, len)) = it.next() {
            // const encoded_len = lz4.compress(page, self.compression_output, &self.table)
            //     catch |err| { log.warn(...); return error.BenchmarkFailed; };
            let PageCompression {
                data,
                compression_output,
                table,
                ..
            } = &mut *self_;
            let encoded_len = match lz4::compress(&data[off..off + len], compression_output, table)
            {
                Ok(n) => n,
                Err(err) => {
                    eprintln!("[{LOG_SCOPE}] page compression report failed err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            // raw_total += page.len;
            raw_total += len;
            // encoded_total += encoded_len;
            encoded_total += encoded_len;
            // std.debug.print("page-compression page={d} raw={d} encoded={d} ratio={d:.2}%\n", ...);
            println!(
                "page-compression page={page_index} raw={len} encoded={encoded_len} ratio={:.2}%",
                percentage(encoded_len, len)
            );
            page_index += 1;
        }

        // std.debug.print("page-compression total pages={d} raw={d} encoded={d}
        //     ratio={d:.2}% workspace={d} output_bound={d}\n", ...);
        println!(
            "page-compression total pages={page_index} raw={raw_total} encoded={encoded_total} \
             ratio={:.2}% workspace={} output_bound={}",
            percentage(encoded_total, raw_total),
            core::mem::size_of::<lz4::HashTable>(),
            self_.compression_output.len(),
        );
        Ok(())
    }

    /// Iterate fixed-size page chunks without allocating an index table.
    // fn pages(self: *const PageCompression) PageIterator {
    fn pages(&self) -> PageIterator {
        PageIterator {
            data_len: self.data.len(),
            page_size: self.opts.page_size,
            offset: 0,
        }
    }
}

// const PageIterator = struct {
//     data: []const u8,
//     page_size: usize,
//     offset: usize = 0,
//     fn next(self: *PageIterator) ?[]const u8 { ... }
// };
//
// PORT NOTE: yields `(offset, len)` instead of a `[]const u8` — see the `data`
// field note. Byte-identical boundaries: a final short chunk is retained.
struct PageIterator {
    data_len: usize,
    page_size: usize,
    offset: usize,
}

impl PageIterator {
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<(usize, usize)> {
        // if (self.offset >= self.data.len) return null;
        if self.offset >= self.data_len {
            return None;
        }
        // const len = @min(self.page_size, self.data.len - self.offset);
        let len = self.page_size.min(self.data_len - self.offset);
        // const end = self.offset + len;
        let end = self.offset + len;
        // defer self.offset = end;
        let start = self.offset;
        self.offset = end;
        // return self.data[self.offset..end];
        Some((start, len))
    }
}

// fn percentage(part: usize, whole: usize) f64 {
//     if (whole == 0) return 0;
//     return @as(f64, @floatFromInt(part)) * 100 / @as(f64, @floatFromInt(whole));
// }
fn percentage(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    (part as f64) * 100.0 / (whole as f64)
}

/// `compat_file.readToEndAlloc(file, alloc, max)` — read the whole file, failing
/// if it exceeds `max` bytes.
fn read_to_end_limited<R: std::io::Read>(r: &mut R, max: usize) -> Result<Vec<u8>, std::io::Error> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = read_slice_short(r, &mut chunk)?;
        if n == 0 {
            return Ok(out);
        }
        if out.len() + n > max {
            // error.StreamTooLong
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "corpus exceeds the benchmark's maximum data size",
            ));
        }
        out.extend_from_slice(&chunk[..n]);
    }
}

// test PageCompression {
#[test]
fn page_compression() {
    // const testing = std.testing;
    let alloc = Allocator;
    // const impl: *PageCompression = try .create(testing.allocator, .{});
    // defer impl.destroy(testing.allocator);
    let mut impl_ = PageCompression::create(alloc, Options::default()).unwrap();

    // const bench = impl.benchmark();
    let bench = impl_.benchmark();
    // _ = try bench.run(.once);
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();

    impl_.destroy(alloc);
}

// test "PageCompression store retains exact encoded allocations" {
#[test]
fn page_compression_store_retains_exact_encoded_allocations() {
    let alloc = Allocator;
    // const page_size = 1024;
    const PAGE_SIZE: usize = 1024;
    // const impl: *PageCompression = try .create(testing.allocator, .{
    //     .mode = .store, .@"page-size" = page_size, .@"retained-pages" = 2 });
    let mut impl_ = PageCompression::create(
        alloc,
        Options {
            mode: Mode::Store,
            page_size: PAGE_SIZE,
            retained_pages: 2,
            ..Default::default()
        },
    )
    .unwrap();

    // impl.data = try testing.allocator.alloc(u8, 3 * page_size);
    // @memset(impl.data, 0);
    impl_.data = vec![0u8; 3 * PAGE_SIZE];
    // impl.compression_output = try testing.allocator.alloc(
    //     u8, try lz4.compressBound(page_size));
    impl_.compression_output = vec![0u8; lz4::compress_bound(PAGE_SIZE).unwrap()];
    // try impl.prepareStored();
    impl_.prepare_stored();

    // try stepStore(impl);
    unsafe {
        PageCompression::step_store(&mut *impl_ as *mut PageCompression as *mut c_void).unwrap()
    };
    // try testing.expectEqual(@as(usize, 1), impl.stored_next);
    assert_eq!(1, impl_.stored_next);
    // for (impl.stored) |block| {
    for block in impl_.stored.iter() {
        // const encoded = block.?;
        let encoded = block.as_ref().unwrap();
        // try testing.expect(encoded.len < page_size);
        assert!(encoded.len() < PAGE_SIZE);

        // var decoded: [page_size]u8 = undefined;
        // _ = try lz4.decompress(encoded, &decoded);
        let mut decoded = [0u8; PAGE_SIZE];
        lz4::decompress(encoded, &mut decoded).unwrap();
        // try testing.expect(std.mem.allEqual(u8, &decoded, 0));
        assert!(decoded.iter().all(|&b| b == 0));
    }

    impl_.destroy(alloc);
}

// The `Action.Struct`-selected decls the CLI dispatch requires (§6).
impl BenchmarkAction for PageCompression {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, OutOfMemory> {
        PageCompression::create(alloc, opts)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        PageCompression::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        PageCompression::benchmark(self)
    }
}
