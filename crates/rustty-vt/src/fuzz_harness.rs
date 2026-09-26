//! Fuzz-harness entry points — a faithful port of the reference AFL++
//! harness package `test/fuzz-libghostty/src/fuzz_{osc,parser,stream}.zig`
//! (each exporting `zig_fuzz_init()` + `zig_fuzz_test(buf, len)`).
//!
//! The three functions here are the `zig_fuzz_test` bodies. They are consumed
//! by two callers that must share IDENTICAL semantics so the shared corpus
//! (`.port/phase-0/corpora/cvt/*` + the reference `corpus/*-cmin` sets)
//! exercises the same paths on both sides:
//!   * the cargo-fuzz targets in `crates/rustty-vt/fuzz/fuzz_targets/` (live
//!     libFuzzer fuzzing, G5 `vt_diff` smoke), and
//!   * the deterministic corpus-replay test
//!     `crates/rustty-vt/tests/fuzz_corpus_replay.rs` (plain `cargo test`).
//!
//! PORT NOTE (allocator bound): the reference harnesses allocate from
//! `mem.zig`'s `FuzzAllocator(N)` — a `FixedBufferAllocator` (8 MiB for osc,
//! 64 MiB for stream) whose bump pointer is reset per input, bounding fuzzer
//! memory and exercising the parsers' OOM-recovery paths. The osc harness
//! mirrors this with `rustty_lib::allocator::FixedBufferAllocator` per call
//! (reset-per-input == fresh-per-call). The stream harness cannot: the port's
//! `Terminal::init` takes the `allocator_api2` `Global` seam, not a pluggable
//! Zig-style allocator, so it draws from the global heap; the reference's
//! `Terminal.init(...) catch return` skip-on-OOM is preserved via the
//! `Result` returned by `Terminal::init`.

use crate::alloc::Global;
use crate::osc;
use crate::parser::Parser;
use crate::terminal::{Colors, Options, Terminal};
use crate::{color, kitty, modes};
use rustty_lib::allocator::{FixedBufferAllocator, ZigAllocatorRef};

/// The osc harness's fixed-buffer allocator size.
// var fuzz_alloc: mem.FuzzAllocator(8 * 1024 * 1024) = .{};
const OSC_FUZZ_ALLOC_BYTES: usize = 8 * 1024 * 1024;

/// Port of `fuzz_osc.zig`'s `zig_fuzz_test`: byte 0 selects the terminator
/// variant (`selector % 3`: 0 → BEL `0x07`, 1 → ST `0x9c`, else → missing);
/// the remaining bytes run through `osc::Parser::next` and then
/// `.end(terminator)` with an allocator present (so the allocating capture
/// paths for large payloads are exercised).
pub fn fuzz_osc(input: &[u8]) {
    // Need at least one byte for the terminator selector.
    // if (len == 0) return;
    if input.is_empty() {
        return;
    }

    // fuzz_alloc.reset();
    // const alloc = fuzz_alloc.allocator();
    let mut backing = vec![0u8; OSC_FUZZ_ALLOC_BYTES];
    let fba = FixedBufferAllocator::init(&mut backing);
    let alloc = ZigAllocatorRef::new(&fba);

    // Use the first byte to select the terminator variant.
    let selector = input[0];
    let payload = &input[1..];

    // var p = osc.Parser.init(alloc);
    // defer p.deinit();
    let mut p = osc::Parser::init(Some(alloc));
    // for (payload) |byte| p.next(byte);
    for &byte in payload {
        p.next(byte);
    }

    // Exercise all three terminator paths:
    //   0 -> BEL  (0x07)
    //   1 -> ST   (0x9c)
    //   2 -> missing terminator (null)
    let terminator: Option<u8> = match selector % 3 {
        0 => Some(0x07),
        1 => Some(0x9c),
        _ => None,
    };

    // _ = p.end(terminator);
    let _ = p.end(terminator);
    p.deinit();
}

/// Port of `fuzz_parser.zig`'s `zig_fuzz_test`: the raw VT state machine,
/// byte-at-a-time, no terminal.
pub fn fuzz_parser(input: &[u8]) {
    // var p: ghostty_vt.Parser = .init();
    // defer p.deinit();
    let mut p = Parser::init();
    // for (buf[0..@intCast(len)]) |byte| _ = p.next(byte);
    for &byte in input {
        let _ = p.next(byte);
    }
    p.deinit();
}

/// Port of `fuzz_stream.zig`'s `zig_fuzz_test`: the full terminal stream
/// handler over a real 80x24 `Terminal` (scrollback 100 bytes). Byte 0
/// selects the path (even → `next_slice` slice/SIMD path, odd → scalar
/// `next` byte-at-a-time UTF-8 path); the remaining bytes are the stream
/// input.
pub fn fuzz_stream(input: &[u8]) {
    // Do not test zero-length input paths.
    // if (len == 0) return;
    if input.is_empty() {
        return;
    }

    // Allocate a terminal; if we run out of fixed-buffer space just
    // skip this input (not a bug, just a very large allocation).
    // var t = Terminal.init(threaded.io(), alloc, .{
    //     .cols = 80, .rows = 24, .max_scrollback_bytes = 100,
    // }) catch return;
    // defer t.deinit(alloc);
    //
    // PORT NOTE: the remaining Options fields are the reference `Options`
    // struct defaults (the Zig init passes only cols/rows/max_scrollback_bytes;
    // same literal set as the `opts` test helper in terminal.rs). The
    // reference's `std.Io.Threaded` argv0 plumbing feeds `Terminal.init`'s
    // `Io` parameter, which the port's `Terminal::init` does not take.
    let mut t = match Terminal::init(
        Global,
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(100),
            max_scrollback_lines: None,
            colors: Colors {
                background: color::DynamicRGB::UNSET,
                foreground: color::DynamicRGB::UNSET,
                cursor: color::DynamicRGB::UNSET,
                palette: color::DynamicPalette::default_palette(),
            },
            default_modes: modes::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        },
    ) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Use the first byte to decide between the scalar and slice paths
    // so both code paths get exercised by the fuzzer.
    let mode = input[0];
    let data = &input[1..];

    {
        // var stream: TerminalStream = t.vtStream();
        // defer stream.deinit();
        let mut stream = t.vt_stream();

        if mode & 1 == 0 {
            // Slice path — exercises SIMD fast-path if enabled
            stream.next_slice(data);
        } else {
            // Scalar path — exercises byte-at-a-time UTF-8 decoding
            for &byte in data {
                stream.next(byte);
            }
        }

        stream.deinit();
    }

    t.deinit(Global);
}
