//! Tests for `stream` (ghostty src/terminal/stream.zig tests), split by
//! topic under `tests/`. The shared CSI differential corpus, recording
//! handlers and drive helpers live here.

use super::*;
use crate::{ansi, csi, modes};

// PORT NOTE: the reference passes string/byte literals to `next`/`nextSlice`.
// Rust string literals are `&str`; `next` takes a `u8` and `next_slice` a
// `&[u8]`. We iterate the bytes of the literal exactly as Zig iterates the
// bytes of its string literal.

// ------------------------------------------------------------------
// WS10 T13/T14 — the CSI fast-path differential guards.
//
// `1a88f3622`, `253e4f9c3` and `300f42c7a` all ship WITHOUT new tests
// upstream; their claim is "byte-for-byte identical semantics with the
// per-byte fast path it hoists", and nothing checks it. These two tests
// check it, and both are mutation-proven.
//
// The corpus is shared by both and deliberately covers every shape the
// three commits touch: plain params, sub-params (colon), mixed
// separators, empty leading params, `>`/`?`/`<`/`=` private markers,
// intermediates (which must FALL BACK to the state machine), a colon
// directly after "ESC [" (the csi_entry edge case the reference calls
// out), parameterless finals, MAX_PARAMS overflow (24 params -> the
// dispatch is dropped entirely), param value saturation, C0 controls
// embedded mid-sequence, 0x7F, and CAN/SUB.
pub(super) const CSI_DIFFERENTIAL_CORPUS: &[&[u8]] = &[
    b"\x1b[m",
    b"\x1b[0m",
    b"\x1b[38;2;10;20;30m",
    b"\x1b[38:2:10:20:30m",
    b"\x1b[38;2:10;20:30m",
    b"\x1b[;5H",
    b"\x1b[;;m",
    b"\x1b[?2004h",
    b"\x1b[?1000;1002;1006l",
    b"\x1b[>4;2m",
    b"\x1b[<0c",
    b"\x1b[=0c",
    b"\x1b[!p",
    b"\x1b[ q",
    b"\x1b[2 q",
    b"\x1b[$}",
    b"\x1b[:3m",
    // Colon separators with a NON-'m' final. `csi_dispatch_final` (and the
    // parser's own `csi_dispatch`) must DROP the whole dispatch here —
    // "we only allow colon or mixed separators for the 'm' command". These
    // entries are what make a dropped `params_sep.set()` in
    // `consume_csi_params` observable; without them, every colon in the
    // corpus is on an 'm' whose SGR parse is separator-insensitive.
    b"\x1b[1:2H",
    b"\x1b[1;2:3H",
    b"\x1b[?1:2h",
    b"\x1b[1:2J",
    b"\x1b[H",
    b"\x1b[A\x1b[B\x1b[C\x1b[D",
    b"\x1b[1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20;21;22;23;24;25m",
    b"\x1b[99999999m",
    b"\x1b[1\x07;2m",
    b"\x1b[1\x0d;2H",
    b"\x1b[1\x7f2m",
    b"\x1b[1\x18;2m",
    b"\x1b[1\x1a;2m",
    b"\x1b[1;2\x1b[3;4m",
    b"hello\x1b[31mworld\x1b[0m\r\n",
    b"\x1b[3;J\x1b[2K\x1b[10X",
    b"\x1b[?25l\x1b[?25h",
];

// A handler that records the exact sequence of dispatch callbacks it
// receives. This is the observable the three commits must preserve: they
// change HOW an action is produced (fast path vs `Parser.next`), never
// WHAT the handler is told.
#[derive(Default)]
pub(super) struct RecordingHandler {
    log: Vec<String>,
}

// The recorder is instantiated twice with different `HAS_VT_RAW`, which is
// the whole point, so the callback bodies live in a macro to guarantee the
// two impls are textually identical.
macro_rules! recording_handler_body {
    () => {
        fn print(&mut self, cp: u32) {
            self.log.push(format!("print {cp}"));
        }
        fn cursor_up(&mut self, v: CursorMovement) {
            self.log.push(format!("cursor_up {}", v.value));
        }
        fn cursor_down(&mut self, v: CursorMovement) {
            self.log.push(format!("cursor_down {}", v.value));
        }
        fn cursor_left(&mut self, v: CursorMovement) {
            self.log.push(format!("cursor_left {}", v.value));
        }
        fn cursor_right(&mut self, v: CursorMovement) {
            self.log.push(format!("cursor_right {}", v.value));
        }
        fn cursor_pos(&mut self, v: CursorPos) {
            self.log.push(format!("cursor_pos {} {}", v.row, v.col));
        }
        fn cursor_style(&mut self, v: ansi::CursorStyle) {
            self.log.push(format!("cursor_style {v:?}"));
        }
        fn set_attribute(&mut self, attr: sgr::Attribute) {
            self.log.push(format!("sgr {attr:?}"));
        }
        fn set_mode(&mut self, v: Mode) {
            self.log.push(format!("set_mode {:?}", v.mode));
        }
        fn reset_mode(&mut self, v: Mode) {
            self.log.push(format!("reset_mode {:?}", v.mode));
        }
        fn erase_display_below(&mut self, p: bool) {
            self.log.push(format!("ed_below {p}"));
        }
        fn erase_display_scrollback(&mut self, p: bool) {
            self.log.push(format!("ed_scrollback {p}"));
        }
        fn erase_line_complete(&mut self, p: bool) {
            self.log.push(format!("el_complete {p}"));
        }
        fn erase_chars(&mut self, n: usize) {
            self.log.push(format!("erase_chars {n}"));
        }
        fn linefeed(&mut self) {
            self.log.push("linefeed".to_string());
        }
        fn carriage_return(&mut self) {
            self.log.push("carriage_return".to_string());
        }
        fn bell(&mut self) {
            self.log.push("bell".to_string());
        }
        fn modify_key_format(&mut self, v: ansi::ModifyKeyFormat) {
            self.log.push(format!("modify_key_format {v:?}"));
        }
        fn full_reset(&mut self) {
            self.log.push("full_reset".to_string());
        }
    };
}

impl Handler for RecordingHandler {
    recording_handler_body!();
}

/// Same recorder, but declaring a `vt_raw` hook — which turns EVERY CSI
/// fast path off at monomorphization time, exactly as the reference's
/// `@hasDecl(T, "vtRaw")` does. This is the reference oracle for the two
/// tests below: the general `Parser.next` state machine.
#[derive(Default)]
pub(super) struct RawHookedRecorder {
    log: Vec<String>,
    raw: Vec<String>,
}

impl Handler for RawHookedRecorder {
    const HAS_VT_RAW: bool = true;

    fn vt_raw(&mut self, action: &parser::Action) -> bool {
        self.raw.push(format!("{action:?}"));
        // false = do not skip; the stream still processes the action, so
        // `log` stays comparable with `RecordingHandler`'s.
        false
    }

    recording_handler_body!();
}

/// Drive the input through the BULK path — `consume_until_ground`, which is
/// where `253e4f9c3`'s `consume_csi_params` and `300f42c7a`'s
/// `csi_entry_byte` live.
///
/// THIS IS NOT `next_slice`, AND THAT IS THE POINT. `next_slice` opens with
///
///     if DEBUG || !crate::options::SIMD { for &c in input { self.next(c); } return; }
///
/// and `options::SIMD` is `cfg!(feature = "simd")`, OFF by default. So in a
/// default `cargo test -p rustty-vt` build, `next_slice` degenerates to the
/// per-byte `next` loop and NEVER reaches `consume_until_ground` — a
/// differential test written against `next_slice` compares the per-byte
/// path with itself and is exactly vacuous. That was caught here by
/// mutation: a deliberate off-by-one in `consume_csi_params`'s final-byte
/// return left the ENTIRE 2163-test suite green.
///
/// This driver instead reproduces the ESC-scan loop `next_slice_capped`
/// runs when SIMD is on, calling `consume_until_ground` directly, so the
/// bulk path is exercised in BOTH feature configurations.
pub(super) fn drive_bulk<H: Handler + Default>(input: &[u8]) -> H {
    let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
    let mut i = 0usize;
    while i < input.len() {
        if s.parser.state != parser::State::Ground {
            // `consume_until_ground` always advances (it consumes at least
            // one byte per iteration, or returns `input.len()`).
            i += unsafe { Stream::consume_until_ground(&mut s, &input[i..]) };
            continue;
        }
        let c = input[i];
        if c == 0x1B {
            s.parser.state = parser::State::Escape;
            s.parser.clear();
            i += 1;
            continue;
        }
        unsafe { Stream::next_utf8(&mut s, c) };
        i += 1;
    }
    s.handler
}

/// `drive_bulk` for a stream that already exists, so a test can feed
/// several chunks through the bulk path with the parser state carried
/// across calls (the reference does this with repeated `nextSlice`).
pub(super) fn drive_bulk_into<H: Handler>(s: &mut Stream<H>, input: &[u8]) {
    let mut i = 0usize;
    while i < input.len() {
        if s.parser.state != parser::State::Ground {
            i += unsafe { Stream::consume_until_ground(s, &input[i..]) };
            continue;
        }
        let c = input[i];
        if c == 0x1B {
            s.parser.state = parser::State::Escape;
            s.parser.clear();
            i += 1;
            continue;
        }
        unsafe { Stream::next_utf8(s, c) };
        i += 1;
    }
}

/// The public `next_slice` entry point. Identical to `drive_bulk` when the
/// `simd` feature is on; identical to `drive_per_byte` when it is off.
/// Both configurations are asserted, so neither can rot.
pub(super) fn drive_slice<H: Handler + Default>(input: &[u8]) -> H {
    let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
    s.next_slice(input);
    s.handler
}

pub(super) fn drive_per_byte<H: Handler + Default>(input: &[u8]) -> H {
    let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
    for b in input {
        s.next(*b);
    }
    s.handler
}

// rustty-only test (WS10 T13 Step 4): the bulk `consume_csi_params` /
// `csi_entry_byte` slice path must produce the identical handler action
// sequence as the per-byte `next` path, for every shape in the corpus.
//
// This is the falsifiable form of `253e4f9c3`'s "guaranteeing byte-for-byte
// identical semantics with the per-byte fast path it hoists".

/// `testing.allocator` for `Options.allocator`: the Rust heap handle, as
/// `Terminal::vt_stream` passes.
pub(super) fn testing_allocator() -> crate::alloc::Allocator {
    crate::alloc::Global
}

mod csi_dispatch;
mod parser_utf8_simd;
mod slice_continuation;
