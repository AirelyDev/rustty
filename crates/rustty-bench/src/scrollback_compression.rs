//! Benchmarks cold-history compression and restoration on pages owned by a
//! live terminal.
//!
//! Unlike `page-compression`, which measures the standalone codec against raw
//! byte chunks, this benchmark first parses a VT corpus into a real `Terminal`.
//! Its timed operations therefore include the PageList state transition,
//! exact-sized encoded allocation, retained-mapping reclamation, and transparent
//! restoration through `PageList::Node::page`.
//!
//! ## Input
//!
//! `--data` names a pre-generated VT byte stream. Parsing happens during setup
//! and is not part of the benchmark's timed region. Use the same saved corpus,
//! terminal dimensions, and scrollback limit when comparing revisions. In
//! particular, do not pipe a generator into this benchmark: generation and
//! pipe scheduling add noise which can overwhelm the operation being measured.
//!
//! The benchmark always operates on the primary screen's PageList. This keeps
//! the result focused on scrollback even if malformed or incomplete input
//! leaves the terminal in its alternate screen at end of file. `--max-scrollback`
//! is expressed in bytes and defaults to 10 MB.
//!
//! ## Modes
//!
//! * `noop` parses the corpus but performs no timed PageList operation. This is
//!   the common process and setup baseline for the other modes.
//! * `compress` times one complete `compress` invocation.
//! * `incremental` reaches the same final representation through
//!   `compress(Drain)`. It drains the candidate-bounded steps and final
//!   no-work verification pass in the timed region, making cursor and repeated
//!   traversal overhead directly comparable with `compress`.
//! * `restore` compresses cold history during setup, outside the timed region,
//!   then visits every fully historical node through `Node::page`. That public
//!   content-access boundary transparently restores compressed nodes.
//! * `report` performs one compression pass and prints compressed page count,
//!   encoded ratio, and estimated resident-byte savings. It is intended for
//!   inspecting a corpus rather than timing comparisons.
//!
//! A fully historical page is a node strictly before the node containing the
//! top of the active area. The boundary node is deliberately excluded because
//! it can contain both history and active rows. Pages intersecting the current
//! viewport are also excluded so visible contents remain resident. Normal
//! terminal execution compresses eligible pages incrementally after activity
//! becomes idle. The benchmark invokes PageList operations directly so its
//! timed regions exclude the production scheduler's idle delay and
//! renderer-thread coordination.
//!
//! ## Examples
//!
//! PORT NOTE: the reference's shell snippets are 4-space-indented blocks, which
//! rustdoc would treat as Rust doctests; they are fenced as `text` here.
//!
//! Build the benchmark in release mode:
//!
//! ```text
//! cargo build -p rustty-bench --profile release-fast
//! ```
//!
//! Inspect the memory reduction for a saved VT corpus:
//!
//! ```text
//! rustty-bench +scrollback-compression --mode=report \
//!   --data=/tmp/scrollback.vt --terminal-cols=120 --terminal-rows=80
//! ```
//!
//! Compare PageList compression and restoration cost. Setup still contributes
//! to full process time, so use a sufficiently large corpus and compare against
//! `noop` with identical arguments:
//!
//! ```text
//! hyperfine --warmup 3 \
//!   'rustty-bench +scrollback-compression --mode=noop --data=/tmp/scrollback.vt' \
//!   'rustty-bench +scrollback-compression --mode=compress --data=/tmp/scrollback.vt' \
//!   'rustty-bench +scrollback-compression --mode=incremental --data=/tmp/scrollback.vt' \
//!   'rustty-bench +scrollback-compression --mode=restore --data=/tmp/scrollback.vt'
//! ```
//!
//! PORTS: ghostty src/benchmark/ScrollbackCompression.zig (oracle origin/main).
// const ScrollbackCompression = @This();

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const terminalpkg = @import("../terminal/main.zig");
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const PageList = terminalpkg.PageList;
// const Terminal = terminalpkg.Terminal;
// const global = @import("../global.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use crate::options::{self, read_slice_short};
use crate::terminal_options;
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_datastruct::intrusive_linked_list::Node as _;
use rustty_vt::pagelist::compress::{CompressionMode, MemoryStats};
use rustty_vt::pagelist::{Node, OutOfMemory, PageList};
use rustty_vt::point;
use rustty_vt::screen_set::Key;
use rustty_vt::terminal::Terminal;

// const log = std.log.scoped(.@"scrollback-compression-bench");
const LOG_SCOPE: &str = "scrollback-compression-bench";

// opts: Options,
// terminal: Terminal,
pub struct ScrollbackCompression {
    opts: Options,
    terminal: Terminal,
}

// pub const Options = struct {
pub struct Options {
    /// Select the PageList operation performed inside the timed benchmark.
    // mode: Mode = .compress,
    mode: Mode,

    /// Dimensions used to construct the terminal which consumes the corpus.
    /// These affect wrapping and therefore the number and contents of pages.
    // @"terminal-rows": u16 = 80,
    // @"terminal-cols": u16 = 120,
    terminal_rows: u16,
    terminal_cols: u16,

    /// Maximum primary-screen scrollback allocation in bytes. PageList rounds
    /// this as required by its page allocation policy.
    // @"max-scrollback": usize = 10_000_000,
    max_scrollback: usize,

    /// Pre-generated VT corpus. `-` reads stdin, although a regular file is
    /// strongly recommended so comparisons can reuse identical input bytes.
    /// When unset, every mode operates on the initial empty terminal.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Compress,
            terminal_rows: 80,
            terminal_cols: 120,
            max_scrollback: 10_000_000,
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
            // @"max-scrollback": usize
            b"max-scrollback" => {
                self.max_scrollback = rustty_args::args::parse_field_int(value)?;
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
    /// Establish process and setup overhead without a PageList operation.
    // noop,
    Noop,

    /// Compress every eligible fully historical page once.
    // compress,
    Compress,

    /// Compress every eligible page through bounded resumable steps.
    // incremental,
    Incremental,

    /// Restore pages compressed outside the timed region.
    // restore,
    Restore,

    /// Compress once and print aggregate memory statistics.
    // report,
    Report,
}

impl Mode {
    /// `std.meta.stringToEnum(Mode, name)` — the kebab CLI tokens.
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"noop" => Some(Mode::Noop),
            b"compress" => Some(Mode::Compress),
            b"incremental" => Some(Mode::Incremental),
            b"restore" => Some(Mode::Restore),
            b"report" => Some(Mode::Report),
            _ => None,
        }
    }
}

impl ScrollbackCompression {
    // pub fn create(alloc: Allocator, opts: Options) !*ScrollbackCompression {
    //     const ptr = try alloc.create(ScrollbackCompression);
    //     errdefer alloc.destroy(ptr);
    //     ptr.* = .{ .opts = opts, .terminal = try .init(global.io(), alloc, .{
    //         .rows = opts.@"terminal-rows",
    //         .cols = opts.@"terminal-cols",
    //         .max_scrollback_bytes = opts.@"max-scrollback",
    //     }) };
    //     return ptr;
    // }
    //
    // PORT NOTE: the CLI option stays `--max-scrollback` (`usize`, bytes) — the
    // oracle did NOT rename the benchmark's own flag in `86f81fb5b`; only the
    // `terminal.Options` FIELD it feeds became `max_scrollback_bytes: ?usize`.
    // The `Some(..)` below is the Zig `usize -> ?usize` coercion. (The WS7 D8
    // divergence note that used to sit here is resolved by `P3-W3` T2.)
    pub fn create(
        alloc: Allocator,
        opts: Options,
    ) -> Result<Box<ScrollbackCompression>, OutOfMemory> {
        let mut topts = terminal_options(opts.terminal_rows, opts.terminal_cols);
        topts.max_scrollback_bytes = Some(opts.max_scrollback);
        let terminal = Terminal::init(alloc, topts)?;
        Ok(Box::new(ScrollbackCompression { opts, terminal }))
    }

    // pub fn destroy(self: *ScrollbackCompression, alloc: Allocator) void {
    //     self.terminal.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<ScrollbackCompression>, alloc: Allocator) {
        self.terminal.deinit(alloc);
        drop(self);
    }

    // pub fn benchmark(self: *ScrollbackCompression) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) { ... },
                step_fn: match self.opts.mode {
                    Mode::Noop => Self::step_noop,
                    Mode::Compress => Self::step_compress,
                    Mode::Incremental => Self::step_incremental,
                    Mode::Restore => Self::step_restore,
                    Mode::Report => Self::step_report,
                },
                setup_fn: Some(Self::setup),
                // The reference declares no `teardownFn` for this benchmark.
                teardown_fn: None,
            },
        )
    }

    /// Reset the terminal and consume the complete VT corpus before timing starts.
    /// Restore mode also prepares its compressed representation here so its timed
    /// step contains decoding and mapping writes, but not encoding.
    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScrollbackCompression) };
        // self.terminal.fullReset();
        self_.terminal.full_reset();

        // self.loadCorpus() catch |err| {
        //     log.warn("failed to prepare scrollback compression benchmark err={}", .{err});
        //     return error.BenchmarkFailed;
        // };
        if let Err(err) = self_.load_corpus() {
            // log.warn("failed to prepare scrollback compression benchmark err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "failed to prepare scrollback compression benchmark err={err:?}"
            );
            return Err(Error::BenchmarkFailed);
        }

        // if (self.opts.mode == .restore) { _ = self.pages().compress(.full); }
        if self_.opts.mode == Mode::Restore {
            unsafe { (*self_.pages()).compress(CompressionMode::Full) };
        }
        Ok(())
    }

    /// Feed the corpus in the same 64 KiB chunks used by the real IO thread and
    /// terminal-stream benchmark. Parser and file IO costs remain in setup.
    // fn loadCorpus(self: *ScrollbackCompression) !void {
    fn load_corpus(&mut self) -> Result<(), std::io::Error> {
        // const data_file = try options.dataFile(self.opts.data) orelse return;
        // defer data_file.close(global.io());
        let mut data_file = match options::data_file(self.opts.data.as_deref())? {
            Some(f) => f,
            None => return Ok(()),
        };

        // var stream = self.terminal.vtStream();
        // defer stream.deinit();
        let mut stream = self.terminal.vt_stream();

        // var read_buf: [64 * 1024]u8 align(std.atomic.cache_line) = undefined;
        // var file_reader = data_file.reader(global.io(), &read_buf);
        // const reader = &file_reader.interface;
        //
        // var buf: [64 * 1024]u8 = undefined;
        // while (true) {
        //     const n = reader.readSliceShort(&buf) catch
        //         return file_reader.err orelse error.ReadFailed;
        //     if (n == 0) return;
        //     stream.nextSlice(buf[0..n]);
        // }
        //
        // PORT NOTE: the 64 KiB read buffer is boxed rather than a stack array —
        // two 64 KiB stack arrays would be 128 KiB of frame in a fn also holding
        // the stream. Identical chunking.
        let mut buf = vec![0u8; 64 * 1024];
        let result = loop {
            let n = match read_slice_short(&mut data_file, &mut buf) {
                Ok(n) => n,
                Err(err) => break Err(err),
            };
            if n == 0 {
                break Ok(());
            }
            stream.next_slice(&buf[..n]);
        };
        stream.deinit();
        result
    }

    /// Return the primary screen because it is the terminal screen which owns
    /// scrollback. A corpus ending in the alternate screen must not turn this into
    /// an alternate-screen allocation benchmark.
    // fn pages(self: *ScrollbackCompression) *PageList {
    //     return &self.terminal.screens.get(.primary).?.pages;
    // }
    //
    // PORT NOTE (WS7 D8): returns a RAW `*mut PageList`, matching the reference's
    // `*PageList` and this port's raw-pointer discipline — `Node::page` may
    // rewrite a node's storage union while the caller still holds this pointer,
    // so no `&mut PageList` is ever formed here.
    fn pages(&mut self) -> *mut PageList {
        let screen = self.terminal.screens.get(Key::Primary).unwrap();
        unsafe { core::ptr::addr_of_mut!((*screen).pages) }
    }

    // fn stepNoop(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_noop(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScrollbackCompression) };
        // std.mem.doNotOptimizeAway(&self.terminal);
        black_box(&self_.terminal);
        Ok(())
    }

    // fn stepCompress(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_compress(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScrollbackCompression) };
        // _ = self.pages().compress(.full);
        unsafe { (*self_.pages()).compress(CompressionMode::Full) };
        // std.mem.doNotOptimizeAway(&self.terminal);
        black_box(&self_.terminal);
        Ok(())
    }

    // fn stepIncremental(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_incremental(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScrollbackCompression) };
        // _ = self.pages().compress(.drain);
        unsafe { (*self_.pages()).compress(CompressionMode::Drain) };
        // std.mem.doNotOptimizeAway(&self.terminal);
        black_box(&self_.terminal);
        Ok(())
    }

    // fn stepRestore(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_restore(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScrollbackCompression) };
        // std.mem.doNotOptimizeAway(self.visitColdPages());
        black_box(self_.visit_cold_pages());
        Ok(())
    }

    /// Visit all nodes which were eligible for the setup compression pass.
    ///
    /// `page` is intentionally used instead of inspecting the union payload so the
    /// benchmark follows the same transparent restoration boundary as consumers.
    /// Compressible nodes restore here; resident candidates which failed the
    /// opportunistic pass are harmlessly visited through the same boundary.
    ///
    /// RESTORE-VS-INSPECT (WS7 D8, deliberate): this is the ONE place in the two
    /// new benchmarks that calls `Node::page`, and it calls it precisely BECAUSE
    /// restoration is the thing being measured. Every other node touch here
    /// (`memory_stats`, the active-boundary comparison) is inspect-only and never
    /// decompresses. `page_if_resident`/`metadata` would defeat the benchmark's
    /// entire purpose, so they are deliberately NOT used in this function.
    // fn visitColdPages(self: *ScrollbackCompression) usize {
    fn visit_cold_pages(&mut self) -> usize {
        // const page_list = self.pages();
        let page_list = self.pages();
        // const active_node = page_list.getTopLeft(.active).node;
        //
        // `get_top_left` is inspect-only: it reads pins and page metadata and
        // never restores.
        let active_node = unsafe { (*page_list).get_top_left(point::Tag::Active).node };
        // var visited: usize = 0;
        let mut visited: usize = 0;
        // var node_ = page_list.pages.first;
        let mut node_ = unsafe { (*page_list).pages.first };
        // while (node_) |node| : (node_ = node.next) {
        while !node_.is_null() {
            let node = node_;
            // The `next` link is read BEFORE `Node::page` restores this node, so
            // no reference into the node is live across the storage rewrite.
            node_ = unsafe { (*node).next() };
            // if (node == active_node) break;
            if core::ptr::eq(node, active_node) {
                break;
            }
            // std.mem.doNotOptimizeAway(node.page());
            black_box(unsafe { Node::page(node) });
            // visited += 1;
            visited += 1;
        }
        // return visited;
        visited
    }

    // fn stepReport(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_report(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut ScrollbackCompression) };
        // _ = self.pages().compress(.full);
        unsafe { (*self_.pages()).compress(CompressionMode::Full) };
        // const memory = self.pages().memoryStats();
        let memory: MemoryStats = unsafe { (*self_.pages()).memory_stats() };

        // std.debug.print("scrollback-compression compressed={d} raw={d} encoded={d}
        //     ratio={d:.2}% savings={d}\n", ...);
        println!(
            "scrollback-compression compressed={} raw={} encoded={} ratio={:.2}% savings={}",
            memory.compressed_pages,
            memory.decommitted_raw_bytes,
            memory.encoded_bytes,
            percentage(memory.encoded_bytes, memory.decommitted_raw_bytes),
            memory.estimated_savings(),
        );
        Ok(())
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

// test ScrollbackCompression {
#[test]
fn scrollback_compression() {
    let alloc = Allocator;
    // const impl: *ScrollbackCompression = try .create(testing.allocator, .{});
    // defer impl.destroy(testing.allocator);
    let mut impl_ = ScrollbackCompression::create(alloc, Options::default()).unwrap();

    // const bench = impl.benchmark();
    let bench = impl_.benchmark();
    // _ = try bench.run(.once);
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();

    impl_.destroy(alloc);
}

// test "ScrollbackCompression restores cold terminal pages" {
#[test]
// MIRI EXCEPTION (rustty-bench scrollback_compression, SIMD-FFI class): the
// restore/drain corpora are fed through `Stream::next_slice`, which calls
// `rustty_simd_decode_utf8_until_control_seq` (highway C++). Miri hard-errors
// on the first foreign call and takes the whole lib binary with it. Identical
// to the `#[cfg_attr(miri, ignore)]` on `rustty-vt`'s
// `stream_print_slice_matches_per_byte` (WS10 T16), and the vt-side raw-pointer
// writes these corpora reach ARE Miri-covered there. Registered in
// .port/phase-2/MIRI_EXCEPTIONS.md ("rustty-bench — the 2026-07-31 CI triage").
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench): Stream::next_slice calls \
     the SIMD C++ decoder — see MIRI_EXCEPTIONS.md"
)]
fn scrollback_compression_restores_cold_terminal_pages() {
    let alloc = Allocator;
    // const impl: *ScrollbackCompression = try .create(testing.allocator, .{
    //     .mode = .restore, .@"terminal-rows" = 4,
    //     // Standard-width pages hold 215 rows. ...
    //     .@"terminal-cols" = 215, .@"max-scrollback" = 1_000_000 });
    let mut impl_ = ScrollbackCompression::create(
        alloc,
        Options {
            mode: Mode::Restore,
            terminal_rows: 4,
            // Standard-width pages hold 215 rows. Keeping that row capacity here
            // makes this test corpus small while still producing cold history.
            terminal_cols: 215,
            max_scrollback: 1_000_000,
            ..Default::default()
        },
    )
    .unwrap();

    // var stream = impl.terminal.vtStream();
    // defer stream.deinit();
    // for (0..256) |_| stream.nextSlice("aaaa\r\n");
    {
        let mut stream = impl_.terminal.vt_stream();
        for _ in 0..256 {
            stream.next_slice(b"aaaa\r\n");
        }
        stream.deinit();
    }

    // _ = impl.pages().compress(.full);
    unsafe { (*impl_.pages()).compress(CompressionMode::Full) };
    // const compressed = impl.pages().memoryStats();
    let compressed = unsafe { (*impl_.pages()).memory_stats() };
    // try testing.expect(compressed.compressed_pages > 0);
    assert!(compressed.compressed_pages > 0);
    // try testing.expect(impl.visitColdPages() >= compressed.compressed_pages);
    assert!(impl_.visit_cold_pages() >= compressed.compressed_pages);

    // Restored historical pages are resident and therefore eligible for a
    // later explicit pass. This also verifies that the benchmark traversal
    // went through Node.page rather than merely inspecting page metadata.
    // _ = impl.pages().compress(.full);
    unsafe { (*impl_.pages()).compress(CompressionMode::Full) };
    // const recompressed = impl.pages().memoryStats();
    let recompressed = unsafe { (*impl_.pages()).memory_stats() };
    // try testing.expectEqual(compressed.compressed_pages, recompressed.compressed_pages);
    assert_eq!(compressed.compressed_pages, recompressed.compressed_pages);

    impl_.destroy(alloc);
}

// test "ScrollbackCompression drains incremental compression steps" {
#[test]
// MIRI EXCEPTION (rustty-bench scrollback_compression, SIMD-FFI class) — same
// cause and same registration as `scrollback_compression_restores_cold_terminal_pages`
// above (`Stream::next_slice` -> `rustty_simd_decode_utf8_until_control_seq`).
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench): Stream::next_slice calls \
     the SIMD C++ decoder — see MIRI_EXCEPTIONS.md"
)]
fn scrollback_compression_drains_incremental_compression_steps() {
    let alloc = Allocator;
    let mut impl_ = ScrollbackCompression::create(
        alloc,
        Options {
            mode: Mode::Incremental,
            terminal_rows: 4,
            terminal_cols: 215,
            max_scrollback: 1_000_000,
            ..Default::default()
        },
    )
    .unwrap();

    // var stream = impl.terminal.vtStream();
    // defer stream.deinit();
    // for (0..256) |_| stream.nextSlice("aaaa\r\n");
    {
        let mut stream = impl_.terminal.vt_stream();
        for _ in 0..256 {
            stream.next_slice(b"aaaa\r\n");
        }
        stream.deinit();
    }

    // _ = impl.pages().compress(.drain);
    unsafe { (*impl_.pages()).compress(CompressionMode::Drain) };
    // const incremental = impl.pages().memoryStats();
    let incremental = unsafe { (*impl_.pages()).memory_stats() };
    // try testing.expect(incremental.compressed_pages > 0);
    assert!(incremental.compressed_pages > 0);

    // Restore the same pages and compare against the monolithic operation.
    // Both paths should produce the same final storage representation.
    // _ = impl.visitColdPages();
    impl_.visit_cold_pages();
    // _ = impl.pages().compress(.full);
    unsafe { (*impl_.pages()).compress(CompressionMode::Full) };
    // const monolithic = impl.pages().memoryStats();
    let monolithic = unsafe { (*impl_.pages()).memory_stats() };
    // try testing.expectEqual(monolithic, incremental);
    assert_eq!(monolithic, incremental);

    impl_.destroy(alloc);
}

// The `Action.Struct`-selected decls the CLI dispatch requires (§6).
impl BenchmarkAction for ScrollbackCompression {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        Ok(ScrollbackCompression::create(alloc, opts)?)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        ScrollbackCompression::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        ScrollbackCompression::benchmark(self)
    }
}
