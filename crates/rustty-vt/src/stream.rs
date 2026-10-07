//! rustty-vt stream
//!
//! PORTS: ghostty src/terminal/stream.zig
//!
//! PORT STATUS: done (vt-9) — the `Handler` trait, `Stream<H>`, `init`/`deinit`,
//! `next`/`next_slice`, the action router, and the full
//! `csi`/`osc`/`esc`/`dcs`/`apc` dispatch, with `TestHandler`s + all 63 named
//! reference tests (`stream.zig` also has one unnamed-form `test Action`, a
//! C-ABI union reification preserved as a comment at :4758 with its own PORT
//! NOTE — 64 `test` blocks in total).
//
// CORRECTION (task-18, an unclaimed instance of the S139-F1 stale-count class —
// see `NEW-FINDINGS.md` N33): this said "all 38 tests", as did the SIMD note
// below. Counted at pin `20abdb50a`: 64 `test` blocks in `stream.zig`, 63 of
// them named; all 63 have a preserved `// test "…"` comment and a live `#[test]`
// here (75 `#[test]` total, the extra 12 being port-specific). 38 was the count
// when vt-9 landed.
//!
//! SIMD STATUS (realigned 2026-07-10, VALIDATION_REPORT §3b — the former
//! "DEFERRED SEAM" wording here was stale): the SIMD `next_slice` fast path
//! (`nextSliceCapped` + `consumeUntilGround` over
//! `rustty_simd::vt::utf8DecodeUntilControlSeq`) is PORTED and LIVE — RESOLVED
//! Stage A.1, see `.port/phase-2/DEFERRALS.md` "Deferred to Dispatch/SIMD
//! phase". It stays gated on `options::SIMD` (the reference's own
//! `comptime debug or !build_options.simd` switch). In THIS crate's own default
//! feature set `SIMD` is `false`, so the scalar fallback — the reference's own
//! non-SIMD arm — runs by default under `cargo test -p rustty-vt`, and the SIMD
//! arm is exercised under `--features simd`. The SHIPPING tiers (rustty-core,
//! rustty-vt-c, rustty-apprt-c) turn the feature on, matching `-Dsimd`'s
//! non-wasm default; wasm targets resolve `SIMD` off regardless
//! (`options::SIMD`, `build/Config.zig:200-206`). (All 63 stream tests pass in both modes;
//! Miri covers the scalar arm, ASan the C-FFI SIMD arm per MIRI_EXCEPTIONS.md).

// PORT NOTE: the dispatch routines are a verbatim retargeting of the reference's
// `switch` statements (notably `switch (input.intermediates.len)` /
// `switch (input.params.len)` with `0 => ... else => ...`). Faithfully mirroring
// each Zig `switch` as a Rust `match` triggers `clippy::single_match` and
// `clippy::len_zero`; rewriting them as `if`/`else if` would distort the 1:1
// structural mapping with the reference. The lints are allowed crate-locally for
// this module to keep the dispatch shape identical to `stream.zig`.
#![allow(clippy::single_match)]
#![allow(clippy::len_zero)]

// const streampkg = @This();
// const std = @import("std");
// const build_options = @import("terminal_options");
// const assert = @import("../quirks.zig").inlineAssert;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const simd = @import("../simd/main.zig");
// const lib = @import("lib.zig");
// const Parser = @import("Parser.zig");
// const ansi = @import("ansi.zig");
// const charsets = @import("charsets.zig");
// const device_attributes = @import("device_attributes.zig");
// const device_status = @import("device_status.zig");
// const csi = @import("csi.zig");
// const kitty = @import("kitty.zig");
// const modes = @import("modes.zig");
// const osc = @import("osc.zig");
// const sgr = @import("sgr.zig");
// const continuationpkg = @import("stream_continuation.zig");
// const UTF8Decoder = @import("UTF8Decoder.zig");
// const MouseShape = @import("mouse.zig").Shape;
// PORT NOTE: retargeted to the ported crate-relative paths.
use crate::ansi;
use crate::charsets;
use crate::csi;
use crate::device_attributes;
use crate::device_status;
use crate::kitty;
use crate::modes;
use crate::mouse::Shape as MouseShape;
use crate::osc;
use crate::parser::{self, Parser};
use crate::sgr;
use crate::stream_continuation as continuationpkg;
use crate::utf8_decoder::UTF8Decoder;

mod csi_dispatch;
mod dispatch;

// const log = std.log.scoped(.stream);
const LOG_SCOPE: &str = "stream";

// /// Flip this to true when you want verbose debug output for
// /// debugging terminal stream issues. In addition to louder
// /// output this will also disable the SIMD optimizations in
// /// order to make it easier to see every byte. So if you're
// /// debugging an issue in the SIMD code then you'll need to
// /// do something else.
// const debug = false;
const DEBUG: bool = false;

/// `std.simd.suggestVectorLength(u8)` for `8c523ed03`'s APC payload scan.
/// NOTE this is the NATIVE width, not `simd.lanes` (= 2x native): `8c523ed03`
/// predates `f2a7652ab`'s `lanes` helper and calls `suggestVectorLength`
/// directly, so the ported scan uses the same width the reference does.
const APC_SCAN_LANES: Option<usize> = match rustty_simd::native_vector_bits() {
    Some(bits) => Some(bits / 8),
    None => None,
};

/// `APC_SCAN_LANES` as a plain length for the fixed-size `[u8; N]` group the
/// scan tests (the `ByteVector` of the reference). Unused (the scan is skipped)
/// when `APC_SCAN_LANES` is `None`; 1 keeps the array type well-formed then.
const APC_SCAN_N: usize = match APC_SCAN_LANES {
    Some(n) => n,
    None => 1,
};

/// One group of `consume_apc_string`'s vector scan: true iff some byte is not
/// an `apc_put` byte (CAN 0x18, SUB 0x1A, ESC 0x1B, or `>= 0x80`).
///
/// PERF(port, N511): the reference's
/// `(bytes == 0x18) | (bytes == 0x1A) | (bytes == 0x1B) | (bytes >= 0x80)` on a
/// `@Vector(16, u8)` and its `@reduce(.Or, invalid)` lower on aarch64 to
/// `and`/`cmeq`/`cmeq`/`cmlt`/`orr`/`orr`/`umaxv`: 14 instructions a group with
/// the loop. The first port looped over `input[end + j]` with short-circuit
/// `||` and a `bool` accumulator, which LLVM kept scalar (a branch and a
/// bounds check per byte; 1.8x the reference's time on `+apc-parser`). A
/// fixed-size array with the reduction spelled as an unsigned max does
/// vectorize, but on NEON LLVM's SLP vectorizer turns the max of 0/0xFF masks
/// back into an `.Or` reduction, which NEON has no instruction for, and lowers
/// it (with two `tbl` shuffles) to about 27 instructions a group. So aarch64
/// spells the reference's vector expression with the NEON intrinsics it
/// compiles to, the same thing `@Vector(16, u8)` is there; every other target
/// (x86-64 lowers the portable form to `pcmpeqb` + `pmovmskb`, which is the
/// reference's own x86 lowering) and Miri, which the intrinsic path is not
/// built for (the `crc32c.rs` precedent), use the portable form.
/// `apc_group_invalid_matches_the_portable_test` holds the two to the same
/// answer.
#[inline(always)]
fn apc_group_invalid(bytes: &[u8; APC_SCAN_N]) -> bool {
    #[cfg(all(target_arch = "aarch64", not(miri)))]
    {
        apc_group_invalid_neon(bytes)
    }
    #[cfg(not(all(target_arch = "aarch64", not(miri))))]
    {
        apc_group_invalid_portable(bytes)
    }
}

/// `apc_group_invalid` on NEON: the reference's `@Vector(16, u8)` lowering.
#[cfg(all(target_arch = "aarch64", not(miri)))]
#[inline(always)]
fn apc_group_invalid_neon(bytes: &[u8; APC_SCAN_N]) -> bool {
    use core::arch::aarch64::{vceqq_u8, vcgeq_u8, vdupq_n_u8, vld1q_u8, vmaxvq_u8, vorrq_u8};
    // `native_vector_bits` is 128 on aarch64, so a group is one Q register.
    const _: () = assert!(APC_SCAN_N == 16);
    // SAFETY: NEON is part of the aarch64 base ISA (every aarch64 target
    // Rust supports enables it), and `vld1q_u8` reads exactly the 16 bytes
    // of `bytes`, an initialised `[u8; 16]` borrowed for the whole call.
    unsafe {
        // const bytes: ByteVector = input[end..][0..vector_len].*;
        let v = vld1q_u8(bytes.as_ptr());
        // const invalid = (bytes == @as(ByteVector, @splat(0x18))) |
        //     (bytes == @as(ByteVector, @splat(0x1A))) |
        //     (bytes == @as(ByteVector, @splat(0x1B))) |
        //     (bytes >= @as(ByteVector, @splat(0x80)));
        let invalid = vorrq_u8(
            vorrq_u8(vceqq_u8(v, vdupq_n_u8(0x18)), vceqq_u8(v, vdupq_n_u8(0x1A))),
            vorrq_u8(vceqq_u8(v, vdupq_n_u8(0x1B)), vcgeq_u8(v, vdupq_n_u8(0x80))),
        );
        // @reduce(.Or, invalid): every lane is 0x00 or 0xFF, so the
        // across-lanes max is nonzero iff some lane is set.
        vmaxvq_u8(invalid) != 0
    }
}

/// `apc_group_invalid` in portable Rust: the per-lane test with the
/// non-short-circuit `|` over a fixed-size array (no bounds checks), the
/// `.Or` reduction as an unsigned max of 0/0xFF lane masks. Safe code, so
/// Miri runs it as is.
#[cfg_attr(all(target_arch = "aarch64", not(miri), not(test)), allow(dead_code))]
#[inline(always)]
fn apc_group_invalid_portable<const N: usize>(bytes: &[u8; N]) -> bool {
    let mut any: u8 = 0;
    for &b in bytes {
        let invalid = (b == 0x18) | (b == 0x1A) | (b == 0x1B) | (b >= 0x80);
        any = any.max(u8::from(invalid).wrapping_neg());
    }
    any != 0
}

/// `std.simd.suggestVectorLength(u8)` for `8c5bc3d29`'s OSC payload scan —
/// the same NATIVE width the APC scan uses (see `APC_SCAN_LANES`'s note; the
/// reference calls `suggestVectorLength` directly here too).
const OSC_SCAN_LANES: Option<usize> = match rustty_simd::native_vector_bits() {
    Some(bits) => Some(bits / 8),
    None => None,
};

/// `simd.lanes(u32)` for the printable-run scan in `next_slice_capped`
/// (f2a7652ab). See `rustty_simd::lanes`.
const PRINT_RUN_LANES: Option<usize> = rustty_simd::lanes(core::mem::size_of::<u32>());

// The possible actions that can be emitted by the Stream
// function for handling.
// pub const Action = union(Key) { ... };
//
// /// C ABI functions.
// const c_union = lib.TaggedUnion(
//     lib.target,
//     @This(),
//     // TODO: Before shipping an ABI-compatible libghostty, verify this.
//     // This was just arbitrarily chosen for now.
//     [16]u64,
// );
// pub const Tag = c_union.Tag;
// pub const Value = c_union.Value;
// pub const C = c_union.C;
// pub const CValue = c_union.CValue;
// pub const cval = c_union.cval;
//
// PORT NOTE: the reference `Action` is a tagged union whose tag enum `Key` and
// C-ABI union (`Tag`/`Value`/`C`/`CValue`/`cval`, kept verbatim above) drive
// the comptime `vt`
// dispatch. The Rust port models the dispatch surface directly as the `Handler`
// trait (one method per union field, see below); the union's payload field
// types are the trait method argument types. The C-ABI mirror lives in the
// `rustty-vt-c` crate. The payload helper structs are ported as the dedicated
// types below so the `Handler` method signatures match the reference field
// types verbatim.

/// `Action.CursorMovement`
// pub const CursorMovement = extern struct { value: u16 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorMovement {
    /// The value of the cursor movement. Depending on the tag of this
    /// union this may be an absolute value or it may be a relative
    /// value. For example, `cursor_up` is relative, but `cursor_row`
    /// is absolute.
    pub value: u16,
}

/// `Action.CursorPos`
// pub const CursorPos = extern struct { row: u16, col: u16 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorPos {
    pub row: u16,
    pub col: u16,
}

/// `Action.InvokeCharset`
// pub const InvokeCharset = lib.Struct(lib.target, struct {
//     bank: charsets.ActiveSlot,
//     charset: charsets.Slots,
//     locking: bool,
// });
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvokeCharset {
    pub bank: charsets::ActiveSlot,
    pub charset: charsets::Slots,
    pub locking: bool,
}

/// `Action.ConfigureCharset`
// pub const ConfigureCharset = lib.Struct(lib.target, struct {
//     slot: charsets.Slots,
//     charset: charsets.Charset,
// });
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigureCharset {
    pub slot: charsets::Slots,
    pub charset: charsets::Charset,
}

/// `Action.Mode`
// pub const Mode = struct { mode: modes.Mode };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub mode: modes::Mode,
}

/// `Action.RawMode`
// pub const RawMode = extern struct { mode: u16, ansi: bool };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawMode {
    pub mode: u16,
    pub ansi: bool,
}

/// `Action.Margin`
// pub const Margin = extern struct { top_left: u16, bottom_right: u16 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Margin {
    pub top_left: u16,
    pub bottom_right: u16,
}

/// `Action.ResizeWindow`
///
/// A request to resize the window's text area (CSI 8 t). A value
/// of zero means the parameter was omitted or zero, and the current
/// size for that dimension should be kept. xterm treats an explicit
/// zero as the screen size, but we can't distinguish it from omitted.
// pub const ResizeWindow = extern struct { rows: u16, columns: u16 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeWindow {
    pub rows: u16,
    pub columns: u16,
}

/// `Action.DeviceStatus`
// pub const DeviceStatus = struct { request: device_status.Request };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceStatus {
    pub request: device_status::Request,
}

/// `Action.KittyKeyboardFlags`
// pub const KittyKeyboardFlags = struct { flags: kitty.KeyFlags };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KittyKeyboardFlags {
    pub flags: kitty::KeyFlags,
}

/// `Action.WindowTitle`
// pub const WindowTitle = struct { title: []const u8 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowTitle<'a> {
    pub title: &'a [u8],
}

/// `Action.ReportPwd`
// pub const ReportPwd = struct { url: []const u8 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportPwd<'a> {
    pub url: &'a [u8],
}

/// `Action.ShowDesktopNotification`
// pub const ShowDesktopNotification = struct { title: []const u8, body: []const u8 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowDesktopNotification<'a> {
    pub title: &'a [u8],
    pub body: &'a [u8],
}

/// `Action.StartHyperlink`
// pub const StartHyperlink = struct { uri: []const u8, id: ?[]const u8 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartHyperlink<'a> {
    pub uri: &'a [u8],
    pub id: Option<&'a [u8]>,
}

/// `Action.ClipboardContents`
// pub const ClipboardContents = struct {
//     kind: u8,
//     data: []const u8,
//     terminator: osc.Terminator,
// };
// (e03475c0c: the terminator rides so an OSC 52 read response can echo it.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardContents<'a> {
    pub kind: u8,
    pub data: &'a [u8],
    pub terminator: osc::Terminator,
}

/// `Action.ApcEnd`
// pub const ApcEnd = extern struct {
//     /// False when CAN, SUB, or another aborting transition ended the APC.
//     terminated: bool,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApcEnd {
    /// False when CAN, SUB, or another aborting transition ended the APC.
    pub terminated: bool,
}

/// `Action.ColorOperation`
// pub const ColorOperation = struct {
//     op: osc.color.Operation,
//     requests: osc.color.List,
//     terminator: osc.Terminator,
// };
// PORT NOTE (parity tail S7-7): `requests` borrows the OSC parser's list (the
// reference's by-value copy of the list struct shares its segments with the
// parser the same way).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorOperation<'a> {
    pub op: osc::color::Operation,
    pub requests: &'a osc::color::List,
    pub terminator: osc::Terminator,
}

/// `Action.SemanticPrompt = osc.Command.SemanticPrompt`
pub type SemanticPrompt<'p> = osc::SemanticPrompt<'p>;

/// Returns a type that can process a stream of tty control characters.
///
/// PORT NOTE: the reference `Stream(comptime H: type)` calls handler methods
/// through a single comptime `vt(comptime action, value)` entry point, guarded
/// per-action by `@hasDecl`/the union tag so the compiler can elide
/// unimplemented actions. Rust has no comptime method-existence reflection, so
/// the dispatch surface is modeled as the `Handler` trait below: every action
/// the reference can emit is a trait method with a **default no-op** body. A
/// concrete handler overrides only the actions it cares about, faithfully
/// mirroring the reference's optional dispatch — an unimplemented action is a
/// no-op by default exactly as `@hasDecl` would skip a missing method.
///
/// The handler must also provide `deinit` (the reference requires a `deinit`
/// function on the handler type).
pub trait Handler {
    /// `deinit` — called when the owning `Stream` is deinitialized.
    fn deinit(&mut self) {}

    /// Whether this handler declares a `vt_raw` hook.
    ///
    /// PORT NOTE (`1a88f3622`/`253e4f9c3`/`300f42c7a`, WS10 T13/T14): the
    /// reference gates its CSI fast paths on `comptime @hasDecl(T, "vtRaw")` —
    /// those paths dispatch WITHOUT going through `Parser.next`, so they would
    /// bypass a raw hook, and a handler that has one (the inspector) must keep
    /// the exact previous behaviour and observe every raw action.
    ///
    /// `@hasDecl` is a compile-time property of the type. rustty models `vtRaw`
    /// as a trait method WITH A DEFAULT, so "did this handler declare it?" is
    /// not recoverable from the method alone — every handler has one. This
    /// associated const is the missing half: it is the direct mapping of
    /// `@hasDecl(T, "vtRaw")`, it is `const` so the gate is monomorphized away
    /// exactly like the reference's `comptime`, and it defaults to `false`
    /// (= not declared) so the fast paths are ON by default.
    ///
    /// A HANDLER THAT OVERRIDES `vt_raw` MUST SET THIS TO `true`. Getting that
    /// wrong is silent — the hook simply stops seeing CSI actions — which is
    /// why `stream_fast_paths_bypassed_when_vt_raw_hooked` exists and is
    /// mutation-proven against exactly that mistake.
    const HAS_VT_RAW: bool = false;

    /// `vtRaw` — optional raw-action hook. A handler can expose this to get the
    /// raw parser action before it is further parsed. If this returns `true`
    /// then the stream skips its own processing of the action.
    // if (@hasDecl(T, "vtRaw")) { ... }
    fn vt_raw(&mut self, _action: &parser::Action<'_>) -> bool {
        false
    }

    // --- print / execute primitives ---

    /// `print` — draw a unicode codepoint to the screen.
    // .print => self.handler.vt(.print, .{ .cp = c });
    fn print(&mut self, _cp: u32) {}

    /// `print_slice` — a run of printable codepoints.
    ///
    /// A run of printable codepoints. This is emitted instead of
    /// individual print actions when the stream can decode multiple
    /// printable codepoints at once, so handlers can process them in
    /// batch with per-run rather than per-codepoint overhead (see
    /// Terminal.printSlice). A naive handler can simply loop and
    /// handle each codepoint like a print action.
    ///
    /// The slice is only valid for the duration of the handler call.
    //
    // .print_slice => self.handler.vt(.print_slice, .{ .cps = cps[i..end] });
    //
    // PORT NOTE (47e26df60, WS10 T16): the reference adds `print_slice:
    // PrintSlice` to the `Action` union, which is EXHAUSTIVE — every handler's
    // `vt` switch must gain an arm or fail to compile, and the commit adds one
    // to each of the three in-tree handlers. rustty models the union as this
    // trait, whose methods have defaults, so a new method with an empty default
    // would SILENTLY drop all printable text in every handler that only
    // implements `print` (25+ of them in-tree, almost all tests). The default
    // here is therefore the loop the reference's own doc comment prescribes for
    // a naive handler, which makes `print_slice` exactly equivalent to the
    // stream of `print` actions it replaces. Handlers that want the batch
    // (`stream_terminal`, `termio`) override it.
    fn print_slice(&mut self, cps: &[u32]) {
        for &cp in cps {
            self.print(cp);
        }
    }

    /// `print_repeat` (REP) — repeat the previous char N times.
    //
    // PORT NOTE (VALIDATION_REPORT §3d): the reference Action union carries
    // `print_repeat: usize` (stream.zig:36) — the u16 CSI param WIDENS into it.
    // An earlier draft narrowed this to `u16`; `usize` restores the reference
    // payload type (unreachable via the VT stream, whose params are u16, but
    // part of the handler API surface).
    fn print_repeat(&mut self, _count: usize) {}

    /// `bell` (BEL)
    fn bell(&mut self) {}

    /// `backspace` (BS)
    fn backspace(&mut self) {}

    /// `horizontal_tab` (HT / CHT)
    fn horizontal_tab(&mut self, _count: u16) {}

    /// `horizontal_tab_back` (CHT back)
    fn horizontal_tab_back(&mut self, _count: u16) {}

    /// `linefeed` (LF/VT/FF)
    fn linefeed(&mut self) {}

    /// `carriage_return` (CR)
    fn carriage_return(&mut self) {}

    /// `enquiry` (ENQ)
    fn enquiry(&mut self) {}

    /// `invoke_charset` (SO/SI, SS2/SS3, LS2/LS3, LS1R/LS2R/LS3R)
    fn invoke_charset(&mut self, _v: InvokeCharset) {}

    // --- cursor movement ---

    /// `cursor_up` (CUU)
    fn cursor_up(&mut self, _v: CursorMovement) {}

    /// `cursor_down` (CUD)
    fn cursor_down(&mut self, _v: CursorMovement) {}

    /// `cursor_left` (CUB)
    fn cursor_left(&mut self, _v: CursorMovement) {}

    /// `cursor_right` (CUF)
    fn cursor_right(&mut self, _v: CursorMovement) {}

    /// `cursor_col` (HPA)
    fn cursor_col(&mut self, _v: CursorMovement) {}

    /// `cursor_row` (VPA)
    fn cursor_row(&mut self, _v: CursorMovement) {}

    /// `cursor_col_relative` (HPR)
    fn cursor_col_relative(&mut self, _v: CursorMovement) {}

    /// `cursor_row_relative` (VPR)
    fn cursor_row_relative(&mut self, _v: CursorMovement) {}

    /// `cursor_pos` (CUP)
    fn cursor_pos(&mut self, _v: CursorPos) {}

    /// `cursor_style` (DECSCUSR)
    fn cursor_style(&mut self, _v: ansi::CursorStyle) {}

    // --- erase ---

    /// `erase_display_below`
    fn erase_display_below(&mut self, _protected: bool) {}

    /// `erase_display_above`
    fn erase_display_above(&mut self, _protected: bool) {}

    /// `erase_display_complete`
    fn erase_display_complete(&mut self, _protected: bool) {}

    /// `erase_display_scrollback`
    fn erase_display_scrollback(&mut self, _protected: bool) {}

    /// `erase_display_scroll_complete`
    fn erase_display_scroll_complete(&mut self, _protected: bool) {}

    /// `erase_line_right`
    fn erase_line_right(&mut self, _protected: bool) {}

    /// `erase_line_left`
    fn erase_line_left(&mut self, _protected: bool) {}

    /// `erase_line_complete`
    fn erase_line_complete(&mut self, _protected: bool) {}

    /// `erase_line_right_unless_pending_wrap`
    fn erase_line_right_unless_pending_wrap(&mut self, _protected: bool) {}

    // --- delete / insert / scroll ---

    /// `delete_chars` (DCH)
    fn delete_chars(&mut self, _count: usize) {}

    /// `erase_chars` (ECH)
    fn erase_chars(&mut self, _count: usize) {}

    /// `insert_lines` (IL)
    fn insert_lines(&mut self, _count: usize) {}

    /// `insert_blanks` (ICH)
    fn insert_blanks(&mut self, _count: usize) {}

    /// `delete_lines` (DL)
    fn delete_lines(&mut self, _count: usize) {}

    /// `scroll_up` (SU)
    fn scroll_up(&mut self, _count: usize) {}

    /// `scroll_down` (SD)
    fn scroll_down(&mut self, _count: usize) {}

    // --- tabs ---

    /// `tab_clear_current`
    fn tab_clear_current(&mut self) {}

    /// `tab_clear_all`
    fn tab_clear_all(&mut self) {}

    /// `tab_set` (HTS)
    fn tab_set(&mut self) {}

    /// `tab_reset`
    fn tab_reset(&mut self) {}

    // --- line / index ---

    /// `index` (IND)
    fn index(&mut self) {}

    /// `next_line` (NEL)
    fn next_line(&mut self) {}

    /// `reverse_index` (RI)
    fn reverse_index(&mut self) {}

    /// `full_reset` (RIS)
    fn full_reset(&mut self) {}

    // --- modes ---

    /// `set_mode` (SM)
    fn set_mode(&mut self, _v: Mode) {}

    /// `reset_mode` (RM)
    fn reset_mode(&mut self, _v: Mode) {}

    /// `save_mode`
    fn save_mode(&mut self, _v: Mode) {}

    /// `restore_mode`
    fn restore_mode(&mut self, _v: Mode) {}

    /// `request_mode` (DECRQM)
    fn request_mode(&mut self, _v: Mode) {}

    /// `request_mode_unknown` (DECRQM, unknown mode)
    fn request_mode_unknown(&mut self, _v: RawMode) {}

    // --- margins ---

    /// `top_and_bottom_margin` (DECSTBM)
    fn top_and_bottom_margin(&mut self, _v: Margin) {}

    /// `left_and_right_margin` (DECSLRM)
    fn left_and_right_margin(&mut self, _v: Margin) {}

    /// `left_and_right_margin_ambiguous`
    fn left_and_right_margin_ambiguous(&mut self) {}

    // --- cursor save/restore ---

    /// `save_cursor` (DECSC)
    fn save_cursor(&mut self) {}

    /// `restore_cursor` (DECRC)
    fn restore_cursor(&mut self) {}

    // --- misc CSI ---

    /// `modify_key_format` (XTMODKEYS)
    fn modify_key_format(&mut self, _v: ansi::ModifyKeyFormat) {}

    /// `mouse_shift_capture` (XTSHIFTESCAPE)
    fn mouse_shift_capture(&mut self, _capture: bool) {}

    /// `protected_mode_off`
    fn protected_mode_off(&mut self) {}

    /// `protected_mode_iso`
    fn protected_mode_iso(&mut self) {}

    /// `protected_mode_dec`
    fn protected_mode_dec(&mut self) {}

    /// `size_report` (XTWINOPS)
    fn size_report(&mut self, _v: csi::SizeReportStyle) {}

    /// `title_push` (XTWINOPS 22)
    fn title_push(&mut self, _v: u16) {}

    /// `title_pop` (XTWINOPS 23)
    fn title_pop(&mut self, _v: u16) {}

    /// `xtversion` (XTVERSION)
    fn xtversion(&mut self) {}

    /// `device_attributes` (DA1/DA2/DA3, DECID)
    fn device_attributes(&mut self, _req: device_attributes::Req) {}

    /// `device_status` (DSR)
    fn device_status(&mut self, _v: DeviceStatus) {}

    // --- kitty keyboard protocol ---

    /// `kitty_keyboard_query`
    fn kitty_keyboard_query(&mut self) {}

    /// `kitty_keyboard_push`
    fn kitty_keyboard_push(&mut self, _v: KittyKeyboardFlags) {}

    /// `kitty_keyboard_pop`
    fn kitty_keyboard_pop(&mut self, _v: u16) {}

    /// `kitty_keyboard_set`
    fn kitty_keyboard_set(&mut self, _v: KittyKeyboardFlags) {}

    /// `kitty_keyboard_set_or`
    fn kitty_keyboard_set_or(&mut self, _v: KittyKeyboardFlags) {}

    /// `kitty_keyboard_set_not`
    fn kitty_keyboard_set_not(&mut self, _v: KittyKeyboardFlags) {}

    // --- DCS ---

    /// `dcs_hook`
    fn dcs_hook(&mut self, _dcs: parser::DCS) {}

    /// `dcs_put`
    fn dcs_put(&mut self, _code: u8) {}

    /// `dcs_unhook`
    fn dcs_unhook(&mut self) {}

    // --- APC ---

    /// `apc_start`
    fn apc_start(&mut self) {}

    /// `apc_put`
    fn apc_put(&mut self, _code: u8) {}

    /// `apc_put_slice` — a run of APC string payload bytes.
    ///
    /// `pub const ApcPutSlice = struct { bytes: []const u8 }` in the reference
    /// `Action` union (whose `C`/`cval` extern form is mirrored in the
    /// `rustty-vt-c` crate, per this module's Action PORT NOTE).
    //
    // PORT NOTE (f6f79acce, WS10 T17): as with `print_slice`, the reference's
    // exhaustive union forces every handler to add an arm, while this trait's
    // methods have defaults. An empty default would silently DROP APC payload
    // for every handler that only implements `apc_put`, so the default is the
    // per-byte fan-out the bulk path replaces — making `apc_put_slice` exactly
    // equivalent to the `apc_put` stream it batches.
    fn apc_put_slice(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.apc_put(b);
        }
    }

    /// `apc_end`
    // .apc_end: ApcEnd — `terminated` is false when CAN, SUB, or another
    // aborting transition ended the APC (b53728241).
    fn apc_end(&mut self, _v: ApcEnd) {}

    // --- hyperlink ---

    /// `start_hyperlink` (OSC 8 start)
    fn start_hyperlink(&mut self, _v: StartHyperlink) {}

    /// `end_hyperlink` (OSC 8 end)
    fn end_hyperlink(&mut self) {}

    // --- status display ---

    /// `active_status_display` (DECSASD)
    fn active_status_display(&mut self, _v: ansi::StatusDisplay) {}

    /// `decaln` (DECALN)
    fn decaln(&mut self) {}

    // --- OSC-sourced ---

    /// `window_title` (OSC 0/2)
    fn window_title(&mut self, _v: WindowTitle) {}

    /// `report_pwd` (OSC 7)
    fn report_pwd(&mut self, _v: ReportPwd) {}

    /// `show_desktop_notification` (OSC 9 / OSC 777)
    fn show_desktop_notification(&mut self, _v: ShowDesktopNotification) {}

    /// `progress_report` (ConEmu OSC 9;4)
    fn progress_report(&mut self, _v: osc::ProgressReport) {}

    /// `clipboard_contents` (OSC 52)
    fn clipboard_contents(&mut self, _v: ClipboardContents) {}

    /// `mouse_shape` (OSC 22)
    fn mouse_shape(&mut self, _v: MouseShape) {}

    /// `configure_charset`
    fn configure_charset(&mut self, _v: ConfigureCharset) {}

    /// `set_attribute` (SGR)
    fn set_attribute(&mut self, _attr: sgr::Attribute) {}

    /// `kitty_color_report` (OSC 21)
    // PORT NOTE (parity tail S7-7): the OSC parser's `kitty_color`, borrowed.
    fn kitty_color_report(&mut self, _v: &kitty::color::OSC) {}

    /// `color_operation`
    fn color_operation(&mut self, _v: ColorOperation<'_>) {}

    /// `semantic_prompt` (OSC 133)
    fn semantic_prompt(&mut self, _v: SemanticPrompt<'_>) {}

    /// `kitty_clipboard` (OSC 5522, the Kitty clipboard protocol)
    // .kitty_clipboard_protocol => |v| { self.handler.vt(.kitty_clipboard, v); }
    // PORT NOTE (Action.KittyClipboard = osc.Command.KittyClipboardProtocol):
    // the action value is the parser's owned command payload, handed to the
    // handler by value like the reference's union payload move.
    fn kitty_clipboard(&mut self, _v: osc::KittyClipboardProtocol<'_>) {}

    /// `kitty_dnd` (OSC 72, the Kitty drag and drop protocol)
    // .kitty_dnd_protocol => |v| { self.handler.vt(.kitty_dnd, v); }
    // PORT NOTE (Action.KittyDnd = osc.Command.KittyDndProtocol): see
    // `kitty_clipboard` above.
    fn kitty_dnd(&mut self, _v: osc::KittyDndProtocol<'_>) {}

    /// `resize_window` (XTWINOPS 8, 714fe9b90)
    fn resize_window(&mut self, _v: ResizeWindow) {}
}

/// Returns a type that can process a stream of tty control characters.
/// This will call the `vt` function on type T with the following signature:
///
///   fn(comptime action: Action.Key, value: Action.Value(action)) void
///
/// The handler type T can choose to react to whatever actions it cares
/// about in its pursuit of implementing a terminal emulator or other
/// functionality.
///
/// Note that printable text is delivered via `print_slice` actions
/// (runs of codepoints) whenever the stream can decode multiple
/// codepoints at once, and via `print` actions otherwise. Handlers
/// that care about text must handle both.
///
/// The Handler type must also have a `deinit` function.
///
/// The "comptime" key is on purpose (vs. a standard Zig tagged union)
/// because it allows the compiler to optimize away unimplemented actions.
/// e.g. you don't need to pay a conditional branching cost on every single
/// action because the Zig compiler codegens separate code paths for every
/// single action at comptime.
// pub fn Stream(comptime H: type) type { return struct { ... }; }
// PORT NOTE: ported as the generic `Stream<H: Handler>` struct. The reference's
// comptime per-action codegen is mirrored by Rust monomorphization: `Stream<H>`
// is specialized per handler `H`, and the `Handler` trait's default no-op methods
// let the optimizer drop unimplemented actions — the same "no conditional cost on
// every action" property the comptime `vt` dispatch gives the reference.
pub struct Stream<H: Handler> {
    // handler: Handler,
    pub handler: H,
    // parser: Parser,
    pub parser: Parser,
    // utf8decoder: UTF8Decoder,
    pub utf8decoder: UTF8Decoder,
    // continuation: ?continuationpkg.Tracker,
    pub continuation: Option<continuationpkg::Tracker>,
}

/// An action that cannot borrow the parser (not an `osc_dispatch`), detached
/// from the parser's borrow (see `Action::into_static`).
#[inline(always)]
fn detach_action(action: parser::Action<'_>) -> parser::Action<'static> {
    match action.into_static() {
        Ok(action) => action,
        // `Parser::next` only produces `osc_dispatch` as the exit action.
        Err(_) => unreachable!("osc_dispatch is only ever the exit action"),
    }
}

// pub const Options = struct { ... };
pub struct Options<H> {
    /// The handler initial value, must be set.
    // handler: Handler,
    pub handler: H,

    /// Allocator to use. If this is not set then the stream
    /// will be fully allocation free. There are some operations
    /// that will be dropped in this case such as OSC 52 clipboard
    /// ops.
    // allocator: ?Allocator = null,
    // PORT NOTE (parity tail S7-7): the runtime allocator handle, as the OSC
    // parser's `alloc` now is; it was a borrowed `ZigAllocatorRef<'a>` that
    // only gated tracking, with the tracker's allocator passed separately to
    // `Stream::init_in` (S7-4). `init` uses it for both, as the reference does.
    pub allocator: Option<crate::alloc::Allocator>,

    /// Maximum size in bytes of the continuation suffix. If this is
    /// null or zero then continuation tracking is disabled. This is
    /// only applied when `allocator` is non-null; without an allocator
    /// continuation tracking is disabled. Feeding this continuation
    /// suffix into an equivalent stream at ground reconstructs the
    /// unfinished state without repeating committed terminal effects.
    /// Continuation tracking is only supported by TerminalStream.
    // continuation_max_bytes: ?usize = null,
    pub continuation_max_bytes: Option<usize>,
}

impl<H> Options<H> {
    /// `.{ .handler = h }` — the reference's Zig field defaults
    /// (`allocator = null`, `continuation_max_bytes = null`) have no Rust
    /// struct-literal equivalent, so this constructor fills them.
    pub fn with_handler(handler: H) -> Options<H> {
        Options {
            handler,
            allocator: None,
            continuation_max_bytes: None,
        }
    }
}

/// The error set of `writeContinuation`:
/// `(std.Io.Writer.Error || error{ContinuationDisabled, ContinuationUnavailable})`.
#[derive(Debug)]
pub enum WriteContinuationError {
    /// `std.Io.Writer.Error` (`error.WriteFailed`).
    WriteFailed(std::io::Error),
    ContinuationDisabled,
    ContinuationUnavailable,
}

impl<H: Handler> Stream<H> {
    /// Initialize a stream. Without an allocator, operations that require
    /// heap allocation are dropped.
    ///
    /// As a concrete example of something that requires heap allocation,
    /// consider OSC 52 (clipboard operations) which can be arbitrarily
    /// large.
    ///
    /// This takes ownership of the handler and will call deinit
    /// when the stream is deinitialized.
    // pub fn init(options: Options) Self {
    // CORRECTION (parity tail S7-7): S7-4 added `init_in(options,
    // tracker_alloc)` because `Options.allocator` could not carry the runtime
    // handle; it can now, so `init` is the reference's again and `init_in` is
    // gone.
    pub fn init(options: Options<H>) -> Stream<H> {
        // Initialize the parser
        // var parser: Parser = .init();
        // if (options.allocator) |alloc| parser.osc_parser.alloc = alloc;
        let mut parser = Parser::init();
        if let Some(alloc) = options.allocator {
            parser.osc_parser.alloc = Some(alloc);
        }

        // Initialize the continuation tracker if one is requested.
        // var tracker: ?continuationpkg.Tracker = null;
        // if (options.allocator) |alloc| {
        //     if (options.continuation_max_bytes) |max_bytes| {
        //         if (max_bytes > 0) tracker = .init(alloc, max_bytes);
        //     }
        // }
        let mut tracker: Option<continuationpkg::Tracker> = None;
        if let Some(alloc) = options.allocator {
            if let Some(max_bytes) = options.continuation_max_bytes {
                if max_bytes > 0 {
                    tracker = Some(continuationpkg::Tracker::init(alloc, max_bytes));
                }
            }
        }

        // return .{
        //     .handler = options.handler,
        //     .parser = parser,
        //     .utf8decoder = .{},
        //     .continuation = tracker,
        // };
        Stream {
            handler: options.handler,
            parser,
            utf8decoder: UTF8Decoder::new(),
            continuation: tracker,
        }
    }

    // pub fn deinit(self: *Self) void {
    pub fn deinit(&mut self) {
        // if (self.continuation) |*tracker| tracker.deinit();
        if let Some(tracker) = &mut self.continuation {
            tracker.deinit();
        }
        // self.parser.deinit();
        self.parser.deinit();
        // self.handler.deinit();
        self.handler.deinit();
    }

    /// Write the current continuation suffix directly to a caller-owned
    /// writer. The caller must pause and serialize access to this Stream.
    // pub fn writeContinuation(
    //     self: *const Self,
    //     writer: *std.Io.Writer,
    // ) (std.Io.Writer.Error || error{
    //     ContinuationDisabled,
    //     ContinuationUnavailable,
    // })!void {
    pub fn write_continuation(
        &self,
        writer: &mut dyn crate::stream_continuation::ByteWriter,
    ) -> Result<(), WriteContinuationError> {
        // SAFETY: `self` is a live shared borrow.
        unsafe { Self::write_continuation_raw(self, writer) }
    }

    /// [`Self::write_continuation`] by place.
    ///
    /// PORT NOTE (N452, P7-W2 T9): the C continuation APIs may run from an
    /// effect callback while a handler action holds a protected `&mut
    /// Handler`, so they must not form a whole-stream `&self`; only the
    /// `continuation` field is borrowed here.
    ///
    /// # Safety
    /// `this` must point to a live stream.
    pub unsafe fn write_continuation_raw(
        this: *const Self,
        writer: &mut dyn crate::stream_continuation::ByteWriter,
    ) -> Result<(), WriteContinuationError> {
        // const tracker = self.continuation orelse
        //     return error.ContinuationDisabled;
        // SAFETY: `this` is live (the caller's contract); a shared borrow of
        // the `continuation` field alone, for this call.
        let tracker = unsafe { (*this).continuation.as_ref() }
            .ok_or(WriteContinuationError::ContinuationDisabled)?;
        // if (tracker.broken)
        //     return error.ContinuationUnavailable;
        if tracker.broken {
            return Err(WriteContinuationError::ContinuationUnavailable);
        }
        // try tracker.write(writer);
        tracker
            .write(writer)
            .map_err(WriteContinuationError::WriteFailed)
    }

    /// True when no continuation suffix is needed to reproduce the
    /// stream's current parsing state.
    // pub inline fn ground(self: *const Self) bool {
    #[inline]
    pub fn ground(&self) -> bool {
        // SAFETY: `self` is a live shared borrow.
        unsafe { Self::ground_raw(self) }
    }

    /// [`Self::ground`] by place.
    ///
    /// PORT NOTE (N452, P7-W2 T9): a C effect callback may ask for the
    /// ground state (`get(.vt_ground)`, `setContinuationMaxBytes`) while a
    /// handler action runs. A whole-stream `&self` would retag the handler
    /// bytes a live protected `&mut Handler` covers, so the C API reads the
    /// two state fields by place through this.
    ///
    /// # Safety
    /// `this` must point to a live stream.
    #[inline]
    pub unsafe fn ground_raw(this: *const Self) -> bool {
        // Parser ground alone is not sufficient because the UTF-8
        // decoder may have some state.
        // SAFETY: `this` is live (the caller's contract); two field reads.
        unsafe { (*this).parser.state == parser::State::Ground && (*this).utf8decoder.state == 0 }
    }

    /// Update the continuation suffix after one complete feed call.
    /// Must only be called when tracking is enabled.
    // fn trackContinuation(self: *Self, input: []const u8) void {
    fn track_continuation(&mut self, input: &[u8]) {
        // const tracker = &self.continuation.?;
        // (deferred below: the ground() check borrows `self` immutably first)

        // If we're in a ground state, we have no continuation suffix
        // to track by definition.
        // if (self.ground()) { tracker.reset(); return; }
        if self.ground() {
            self.continuation.as_mut().unwrap().reset();
            return;
        }

        // Retain the part of this feed needed to replay the unfinished
        // state. When the parser is grounded here, the feed must have
        // ended inside a UTF-8 codepoint instead, because the ground
        // check above covers both state machines.
        // tracker.append(
        //     if (self.parser.state != .ground) .vt else .utf8,
        //     input,
        // );
        let pending = if self.parser.state != parser::State::Ground {
            continuationpkg::Pending::Vt
        } else {
            continuationpkg::Pending::Utf8
        };
        self.continuation.as_mut().unwrap().append(pending, input);
    }

    /// Process a string of characters.
    // pub inline fn nextSlice(self: *Self, input: []const u8) void {
    ///
    /// PORT NOTE (N452, P7-W2 T9): the reference's `self: *Self` is an
    /// unprotected pointer, and a handler effect may re-enter the owning
    /// terminal (the C API's `resize`, `set`, `get`, continuation calls) while
    /// this runs. A Rust `&mut self` would be a protected Unique over every
    /// stream byte for the whole call, so this body is raw-self; the `&mut
    /// self` method of the same name forwards here for Rust callers.
    ///
    /// # Safety
    /// `this` must point to a live stream, and no reference to it may be live
    /// in the caller across this call.
    pub unsafe fn next_slice_raw(this: *mut Self, input: &[u8]) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // self.nextSliceUntracked(input);
            Self::next_slice_untracked(this, input);

            // Continuation tracking is opt-in and this branch predicts
            // perfectly, so disabled streams pay nothing else here.
            // if (self.continuation != null) self.trackContinuation(input);
            if (*this).continuation.is_some() {
                (*this).track_continuation(input);
            }
        }
    }

    /// Process a string of characters (the safe entry; see
    /// [`Self::next_slice_raw`]).
    #[inline]
    pub fn next_slice(&mut self, input: &[u8]) {
        // SAFETY: `self` is a live exclusive borrow, passed down as the raw
        // root for the call.
        unsafe { Self::next_slice_raw(self, input) }
    }

    /// Process a string of characters, but only the shortest prefix
    /// needed to reach the ground state.
    ///
    /// The ground state is when the stream isn't in the middle of any
    /// type of sequence: UTF-8, ESC, CSI, OSC, etc. It is the stateless
    /// point of the stream.
    ///
    /// If the stream is already at ground then this consumes nothing
    /// and returns zero. A non-null return is the number of bytes consumed
    /// before reaching ground, including the byte that reaches it. A null
    /// return means the full slice was consumed without reaching ground.
    ///
    /// This is anywhere from 1% to 5% slower than nextSlice, depending
    /// on the types of inputs provided (e.g. OSC vs APC, whether it
    /// ever reaches ground, etc.).
    // pub inline fn nextSliceUntilGround(
    //     self: *Self,
    //     input: []const u8,
    // ) ?usize {
    ///
    /// PORT NOTE (N452, P7-W2 T9): the reference's `self: *Self` is an
    /// unprotected pointer, and a handler effect may re-enter the owning
    /// terminal (the C API's `resize`, `set`, `get`, continuation calls) while
    /// this runs. A Rust `&mut self` would be a protected Unique over every
    /// stream byte for the whole call, so this body is raw-self; the `&mut
    /// self` method of the same name forwards here for Rust callers.
    ///
    /// # Safety
    /// `this` must point to a live stream, and no reference to it may be live
    /// in the caller across this call.
    pub unsafe fn next_slice_until_ground_raw(this: *mut Self, input: &[u8]) -> Option<usize> {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // const consumed = self.nextSliceUntilGroundUntracked(input);
            let consumed = Self::next_slice_until_ground_untracked(this, input);
            // const consumed_len = consumed orelse input.len;
            let consumed_len = consumed.unwrap_or(input.len());
            // if (self.continuation != null and consumed_len > 0) {
            //     self.trackContinuation(input[0..consumed_len]);
            // }
            if (*this).continuation.is_some() && consumed_len > 0 {
                (*this).track_continuation(&input[0..consumed_len]);
            }

            // return consumed;
            consumed
        }
    }

    /// The safe entry of [`Self::next_slice_until_ground_raw`].
    #[inline]
    pub fn next_slice_until_ground(&mut self, input: &[u8]) -> Option<usize> {
        // SAFETY: as in `next_slice`.
        unsafe { Self::next_slice_until_ground_raw(self, input) }
    }

    // inline fn nextSliceUntilGroundUntracked(
    //     self: *Self,
    //     input: []const u8,
    // ) ?usize {
    unsafe fn next_slice_until_ground_untracked(this: *mut Self, input: &[u8]) -> Option<usize> {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // if (self.ground()) return 0;
            if Self::ground_raw(this) {
                return Some(0);
            }

            // Process UTF-8 if we're within that state.
            // var offset: usize = 0;
            // while (self.utf8decoder.state != 0) {
            //     if (offset >= input.len) return null;
            //     self.nextUtf8(input[offset]);
            //     offset += 1;
            // }
            let mut offset: usize = 0;
            // (`Self::next_utf8(this, ..)` advances the decoder through the raw
            // pointer, which clippy cannot see.)
            #[allow(clippy::while_immutable_condition)]
            while (*this).utf8decoder.state != 0 {
                if offset >= input.len() {
                    return None;
                }
                Self::next_utf8(this, input[offset]);
                offset += 1;
            }

            // Process non-UTF-8
            // if (self.parser.state != .ground) {
            //     offset += self.consumeUntilGround(input[offset..]);
            // }
            if (*this).parser.state != parser::State::Ground {
                offset += Self::consume_until_ground(this, &input[offset..]);
            }

            // return if (self.ground()) offset else null;
            if Self::ground_raw(this) {
                Some(offset)
            } else {
                None
            }
        }
    }

    // inline fn nextSliceUntracked(self: *Self, input: []const u8) void { ... }
    unsafe fn next_slice_untracked(this: *mut Self, input: &[u8]) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // Byte-at-a-time parsing in debug mode only. Even without
            // SIMD support (build_options.simd == false, e.g. wasm), the
            // batched path below is much faster than per-byte dispatch
            // because it decodes UTF-8 in bulk (via scalar fallbacks) and
            // hands printable runs to the handler as print_slice actions.
            // if (comptime debug) {
            //     for (input) |c| self.nextUntracked(c);
            //     return;
            // }
            if DEBUG {
                for &c in input {
                    Self::next_untracked(this, c);
                }
                return;
            }

            // This is the maximum number of codepoints we can decode
            // at one time for this function call. This is somewhat arbitrary
            // so if someone can demonstrate a better number then we can switch.
            // var cp_buf: [4096]u32 = undefined;
            let mut cp_buf: [u32; 4096] = [0; 4096];

            // Split the input into chunks that fit into cp_buf.
            // var i: usize = 0;
            let mut i: usize = 0;
            // while (true) {
            loop {
                // const len = @min(cp_buf.len, input.len - i);
                let len = core::cmp::min(cp_buf.len(), input.len() - i);
                // self.nextSliceCapped(input[i .. i + len], &cp_buf);
                Self::next_slice_capped(this, &input[i..i + len], &mut cp_buf);
                // i += len;
                i += len;
                // if (i >= input.len) break;
                if i >= input.len() {
                    break;
                }
            }
        }
    }

    // inline fn nextSliceCapped(
    //     self: *Self,
    //     input: []const u8,
    //     cp_buf: []u32,
    // ) void {
    unsafe fn next_slice_capped(this: *mut Self, input: &[u8], cp_buf: &mut [u32]) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // assert(input.len <= cp_buf.len);
            assert!(input.len() <= cp_buf.len());

            // var offset: usize = 0;
            let mut offset: usize = 0;

            // If the scalar UTF-8 decoder was in the middle of processing
            // a code sequence, we continue until it's not.
            // while (self.utf8decoder.state != 0) {
            // (`Self::next_utf8(this, ..)` advances the decoder through the raw
            // pointer, which clippy cannot see.)
            #[allow(clippy::while_immutable_condition)]
            while (*this).utf8decoder.state != 0 {
                // if (offset >= input.len) return;
                if offset >= input.len() {
                    return;
                }
                // self.nextUtf8(input[offset]);
                Self::next_utf8(this, input[offset]);
                // offset += 1;
                offset += 1;
            }
            // if (offset >= input.len) return;
            if offset >= input.len() {
                return;
            }

            // If we're not in the ground state then we process until
            // we are. This can happen if the last chunk of input put us
            // in the middle of a control sequence.
            // offset += self.consumeUntilGround(input[offset..]);
            offset += Self::consume_until_ground(this, &input[offset..]);
            // if (offset >= input.len) return;
            if offset >= input.len() {
                return;
            }
            // offset += self.consumeAllEscapes(input[offset..]);
            offset += Self::consume_all_escapes(this, &input[offset..]);

            // If we're in the ground state then we can use SIMD to process
            // input until we see an ESC (0x1B), since all other characters
            // up to that point are just UTF-8.
            // while (self.parser.state == .ground and offset < input.len) {
            while (*this).parser.state == parser::State::Ground && offset < input.len() {
                // const res = simd.vt.utf8DecodeUntilControlSeq(input[offset..], cp_buf);
                let res = rustty_simd::vt::utf8DecodeUntilControlSeq(&input[offset..], cp_buf);
                // const cps = cp_buf[0..res.decoded];
                let cps = &cp_buf[0..res.decoded];

                // Hand runs of printable codepoints to the handler as
                // print_slice actions so it can process them with
                // per-run rather than per-codepoint overhead.
                // var i: usize = 0;
                let mut i: usize = 0;
                // while (i < cps.len) {
                while i < cps.len() {
                    // const cp = cps[i];
                    let cp = cps[i];
                    // C0 and UTF-8-decoded C1 controls never enter
                    // printable runs. A codepoint is one of those
                    // exactly when it has no bit set outside 0x9F:
                    // bits 0-4 and bit 7 cover 0x00-0x1F and
                    // 0x80-0x9F and nothing else, so a single
                    // AND-test classifies both ranges.
                    // if ((cp & ~@as(u32, 0x9F)) == 0) {
                    if (cp & !0x9Fu32) == 0 {
                        // @branchHint(.unlikely);
                        // PORT NOTE: `@branchHint(.unlikely)` has no stable Rust
                        // equivalent (`core::intrinsics::likely` is nightly); the
                        // branch structure is otherwise identical.
                        // if (cp <= 0x1F) {
                        if cp <= 0x1F {
                            // C0 controls execute rather than print.
                            // self.execute(@intCast(cp));
                            Self::execute(&mut (*this).handler, cp as u8);
                        } else {
                            // C1 controls decoded from UTF-8 are ignored.
                            // logUnsupportedOnce(
                            //     "ignoring UTF-8-decoded C1 controls, first: 0x{x}",
                            //     .{cp},
                            //     0x80,
                            // );
                            if log_unsupported_once(&SEEN_UTF8_DECODED_C1_SLICE, 0x80) {
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "ignoring UTF-8-decoded C1 controls, first: 0x{cp:x}"
                                );
                            }
                        }
                        i += 1;
                        continue;
                    }

                    // Find the end of the printable run. This is an
                    // early-exit search loop that LLVM won't
                    // auto-vectorize, and printable runs dominate real
                    // input, so scan several codepoints at a time
                    // manually (same idiom as the printSliceFill run
                    // scan).
                    // var end = i + 1;
                    let mut end = i + 1;
                    'scan: {
                        if let Some(lanes) = PRINT_RUN_LANES {
                            // Non-printable (C0 or decoded C1) is
                            // equivalent to "no bit outside 0x9F is
                            // set": one AND-test per lane.
                            // const mask: V = @splat(~@as(u32, 0x9F));
                            // const zero: V = @splat(0);
                            //
                            // PERF(port): the reference's per-lane body is
                            // BRANCHLESS (vector compare -> @reduce -> @ctz); an
                            // earlier early-exit `break` per lane blocked
                            // vectorization and kept a bounds check per lane
                            // (measured 5.3x the reference's instruction count).
                            // The per-group test is the `@reduce(.Or, stop)` alone
                            // (an OR-accumulate LLVM lowers to a vector compare and
                            // a horizontal reduce); the lane bitmask is built only
                            // on the group that stops the run, as the reference
                            // does. (Task S-4: building the bitmask on EVERY group
                            // cost a scalar extract per lane — NEON has no
                            // movemask — and was the larger share of the
                            // terminal-stream instruction gap to the pinned oracle.)
                            while end + lanes <= cps.len() {
                                // const v: V = cps[end..][0..lanes].*;
                                let window = &cps[end..end + lanes];
                                // const stop = (v & mask) == zero;
                                // if (@reduce(.Or, stop)) {
                                //
                                // PERF(port): the `.Or` reduction is spelled as an
                                // unsigned min of `v & mask` (some lane stops iff
                                // the smallest masked value is zero), which LLVM
                                // lowers to one vector AND and an across-lanes min.
                                let mut min_masked: u32 = u32::MAX;
                                for &v in window {
                                    min_masked = min_masked.min(v & !0x9Fu32);
                                }
                                if min_masked == 0 {
                                    // const bits: std.meta.Int(.unsigned, lanes) = @bitCast(stop);
                                    // end += @ctz(bits);
                                    // break :scan;
                                    let mut stop: u32 = 0;
                                    for (j, &v) in window.iter().enumerate() {
                                        stop |= (((v & !0x9Fu32) == 0) as u32) << j;
                                    }
                                    end += stop.trailing_zeros() as usize;
                                    break 'scan;
                                }
                                end += lanes;
                            }
                        }
                        // while (end < cps.len and (cps[end] & ~@as(u32, 0x9F)) != 0) end += 1;
                        while end < cps.len() && (cps[end] & !0x9Fu32) != 0 {
                            end += 1;
                        }
                    }
                    // self.handler.vt(.print_slice, .{ .cps = cps[i..end] });
                    (*this).handler.print_slice(&cps[i..end]);
                    // i = end;
                    i = end;
                }
                // Consume the bytes we just processed.
                // offset += res.consumed;
                offset += res.consumed;

                // if (offset >= input.len) return;
                if offset >= input.len() {
                    return;
                }

                // If our offset is NOT an escape then we must have a
                // partial UTF-8 sequence. In that case, we pass it off
                // to the scalar parser.
                // if (input[offset] != 0x1B) {
                if input[offset] != 0x1B {
                    // const rem = input[offset..];
                    let rem = &input[offset..];
                    // for (rem) |c| self.nextUtf8(c);
                    for &c in rem {
                        Self::next_utf8(this, c);
                    }
                    // return;
                    return;
                }

                // Process control sequences until we run out.
                // offset += self.consumeAllEscapes(input[offset..]);
                offset += Self::consume_all_escapes(this, &input[offset..]);
            }
        }
    }

    /// Parses back-to-back escape sequences until none are left.
    /// Returns the number of bytes consumed from the provided input.
    ///
    /// Expects input to start with 0x1B, use consumeUntilGround first
    /// if the stream may be in the middle of an escape sequence.
    // inline fn consumeAllEscapes(self: *Self, input: []const u8) usize { ... }
    unsafe fn consume_all_escapes(this: *mut Self, input: &[u8]) -> usize {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // var offset: usize = 0;
            let mut offset: usize = 0;
            // while (input[offset] == 0x1B) {
            while input[offset] == 0x1B {
                // self.parser.state = .escape;
                (*this).parser.state = parser::State::Escape;
                // self.parser.clear();
                (*this).parser.clear();
                // offset += 1;
                offset += 1;
                // offset += self.consumeUntilGround(input[offset..]);
                offset += Self::consume_until_ground(this, &input[offset..]);
                // if (offset >= input.len) return input.len;
                if offset >= input.len() {
                    return input.len();
                }
            }
            // return offset;
            offset
        }
    }

    /// Parses escape sequences until the parser reaches the ground state.
    /// Returns the number of bytes consumed from the provided input.
    // inline fn consumeUntilGround(self: *Self, input: []const u8) usize { ... }
    unsafe fn consume_until_ground(this: *mut Self, input: &[u8]) -> usize {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // var offset: usize = 0;
            let mut offset: usize = 0;
            // while (self.parser.state != .ground) {
            while (*this).parser.state != parser::State::Ground {
                // if (offset >= input.len) return input.len;
                if offset >= input.len() {
                    return input.len();
                }

                // Fast path for CSI entry: "ESC [" is by far the most
                // common escape sequence prefix, so handle the '[' and
                // the byte that follows it here rather than paying a
                // nextNonUtf8 call for each.
                // if (self.parser.state == .escape and input[offset] == '[') {
                if (*this).parser.state == parser::State::Escape && input[offset] == b'[' {
                    (*this).parser.state = parser::State::CsiEntry;
                    offset += 1;
                    continue;
                }

                // if (comptime !@hasDecl(T, "vtRaw")) {
                if !H::HAS_VT_RAW {
                    // if (self.parser.state == .csi_entry) {
                    //     if (self.csiEntryByte(input[offset])) { offset += 1; continue; }
                    // }
                    if (*this).parser.state == parser::State::CsiEntry
                        && Self::csi_entry_byte(this, input[offset])
                    {
                        offset += 1;
                        continue;
                    }

                    // Bulk-consume CSI parameter bytes. This can't be
                    // used for handlers with a vtRaw hook because it
                    // dispatches the CSI directly (see nextNonUtf8).
                    // if (self.parser.state == .csi_param) {
                    if (*this).parser.state == parser::State::CsiParam {
                        // offset += self.consumeCsiParams(input[offset..]);
                        offset += Self::consume_csi_params(this, &input[offset..]);
                        // if (offset >= input.len) return input.len;
                        if offset >= input.len() {
                            return input.len();
                        }
                        // If we're still in csi_param then the next byte
                        // isn't a parameter byte; let nextNonUtf8 below
                        // handle it. Otherwise re-check our state.
                        // if (self.parser.state != .csi_param) continue;
                        if (*this).parser.state != parser::State::CsiParam {
                            continue;
                        }
                    }

                    // Bulk-consume APC string bytes into a single slice.
                    // APC payloads (e.g. Kitty graphics) can be megabytes
                    // of base64 data, so per-byte dispatch is far too slow.
                    // This can't be used for handlers with a vtRaw hook
                    // because it dispatches the slice directly.
                    // if (self.parser.state == .sos_pm_apc_string) {
                    if (*this).parser.state == parser::State::SosPmApcString {
                        // offset += self.consumeApcString(input[offset..]);
                        offset += Self::consume_apc_string(this, &input[offset..]);
                        // if (offset >= input.len) return input.len;
                        if offset >= input.len() {
                            return input.len();
                        }

                        // Fast-path normal string termination. This matches
                        // Parser.next's exit and entry actions while avoiding
                        // the generic action loop for every completed APC.
                        // switch (input[offset]) {
                        //     std.ascii.control_code.esc => { ... },
                        //     0x9C => { ... },
                        //     else => {},
                        // }
                        match input[offset] {
                            0x1B => {
                                (*this).parser.clear();
                                (*this).parser.state = parser::State::Escape;
                                // self.handler.vt(.apc_end, .{ .terminated = true });
                                (*this).handler.apc_end(ApcEnd { terminated: true });
                                offset += 1;
                                continue;
                            }
                            0x9C => {
                                (*this).parser.state = parser::State::Ground;
                                // self.handler.vt(.apc_end, .{ .terminated = true });
                                (*this).handler.apc_end(ApcEnd { terminated: true });
                                offset += 1;
                                continue;
                            }
                            _ => {}
                        }

                        // Aborting transitions need the scalar path so the
                        // handler can distinguish them from terminators.
                    }

                    // Bulk-consume OSC string bytes into the OSC parser.
                    // OSC payloads (e.g. OSC 52 clipboard operations and
                    // the kitty clipboard protocol) can be megabytes of
                    // base64 data, so per-byte dispatch is far too slow.
                    // This can't be used for handlers with a vtRaw hook
                    // because the bytes never produce osc_put actions.
                    // if (self.parser.state == .osc_string) {
                    if (*this).parser.state == parser::State::OscString {
                        // offset += self.consumeOscString(input[offset..]);
                        offset += (*this).consume_osc_string(&input[offset..]);
                        // if (offset >= input.len) return input.len;
                        if offset >= input.len() {
                            return input.len();
                        }

                        // Fast-path normal string termination. This matches
                        // Parser.next's exit and entry actions while avoiding
                        // the generic action loop for every completed OSC.
                        // switch (input[offset]) { ... }
                        match input[offset] {
                            // The sequence should be terminated by a full
                            // ST ("ESC \"); the "\" that should follow is
                            // dispatched as a normal escape sequence.
                            // std.ascii.control_code.esc => { ... }
                            0x1B => {
                                // if (self.parser.osc_parser.end(
                                //     std.ascii.control_code.esc,
                                // )) |cmd| self.oscDispatch(cmd.*);
                                // PORT NOTE (parity tail S7-7): the command borrows
                                // the OSC parser (as `cmd.*`'s slices do) and is
                                // handed to the handler, a disjoint field; it used
                                // to be a deep clone.
                                if let Some(cmd) = (*this).parser.osc_parser.end(Some(0x1B)) {
                                    Self::osc_dispatch(&mut (*this).handler, cmd);
                                }
                                (*this).parser.clear();
                                (*this).parser.state = parser::State::Escape;
                                offset += 1;
                                continue;
                            }
                            // BEL terminates the string directly.
                            // std.ascii.control_code.bel => { ... }
                            0x07 => {
                                if let Some(cmd) = (*this).parser.osc_parser.end(Some(0x07)) {
                                    Self::osc_dispatch(&mut (*this).handler, cmd);
                                }
                                (*this).parser.state = parser::State::Ground;
                                offset += 1;
                                continue;
                            }
                            // CAN/SUB abort and ignored C0 bytes go
                            // through the state machine below.
                            _ => {}
                        }
                    }
                }

                // self.nextNonUtf8(input[offset]);
                Self::next_non_utf8(this, input[offset]);
                // offset += 1;
                offset += 1;
            }
            // return offset;
            offset
        }
    }

    /// Bulk-consume APC string bytes and dispatch them as a single
    /// apc_put_slice action. Returns the number of bytes consumed.
    /// Stops at the first byte that is not an apc_put byte in the
    /// parse table, leaving it for the caller to process through
    /// the state machine. CAN, SUB, ESC, and most C1 bytes exit
    /// or abort the string state; 0xA0-0xFF are ignored by the
    /// table (not payload), so they can't be bulk-consumed either.
    ///
    /// Must not be used by handlers with a vtRaw hook because it
    /// dispatches the slice directly.
    // fn consumeApcString(self: *Self, input: []const u8) usize
    unsafe fn consume_apc_string(this: *mut Self, input: &[u8]) -> usize {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // comptime assert(!@hasDecl(T, "vtRaw"));
            //
            // Same treatment as `csi_entry_byte`/`consume_csi_params`: Rust cannot
            // assert an associated const inside a generic fn body (the fn is
            // monomorphized for EVERY handler, including the hooked ones, even
            // though only the unhooked ones reach the call), so this is a debug
            // assertion and `stream_fast_paths_bypassed_when_vt_raw_hooked` is the
            // real guard.
            debug_assert!(!H::HAS_VT_RAW);
            // assert(self.parser.state == .sos_pm_apc_string);
            assert!((*this).parser.state == parser::State::SosPmApcString);

            // var end: usize = 0;
            let mut end: usize = 0;

            // `8c523ed03`: scan ordinary APC bytes a vector at a time, keeping the
            // scalar loop below as the tail and as the whole scan on targets with
            // no recommended vector width.
            // if (comptime std.simd.suggestVectorLength(u8)) |vector_len| { ... }
            if APC_SCAN_LANES.is_some() {
                // const ByteVector = @Vector(vector_len, u8);
                // while (end + vector_len <= input.len) {
                while let Some(bytes) = input[end..].first_chunk::<APC_SCAN_N>() {
                    // const bytes: ByteVector = input[end..][0..vector_len].*;
                    // const invalid = (bytes == 0x18) | (bytes == 0x1A)
                    //               | (bytes == 0x1B) | (bytes >= 0x80);
                    // if (@reduce(.Or, invalid)) break;
                    if apc_group_invalid(bytes) {
                        break;
                    }
                    // end += vector_len;
                    end += APC_SCAN_N;
                }
            }

            // while (end < input.len) {
            while end < input.len() {
                match input[end] {
                    // Not apc_put bytes: CAN/SUB/ESC and most C1 exit
                    // or abort the state; 0xA0-0xFF are ignored by it.
                    // 0x18, 0x1A, 0x1B, 0x80...0xFF => break,
                    0x18 | 0x1A | 0x1B | 0x80..=0xFF => break,
                    // Everything else is an apc_put byte.
                    // else => end += 1,
                    _ => end += 1,
                }
            }

            // if (end > 0) self.handler.vt(.apc_put_slice, .{ .bytes = input[0..end] });
            if end > 0 {
                (*this).handler.apc_put_slice(&input[0..end]);
            }
            // return end;
            end
        }
    }

    /// Bulk-consume OSC string bytes into the OSC parser. Returns
    /// the number of bytes consumed. Stops at the first byte that
    /// is not an osc_put byte in the parse table, leaving it for
    /// the caller to process through the state machine. Every
    /// byte >= 0x20 is an osc_put byte; bytes below either terminate
    /// or abort the string (BEL, CAN, SUB, ESC) or are ignored.
    ///
    /// Must not be used by handlers with a vtRaw hook because the
    /// consumed bytes never produce osc_put actions.
    // fn consumeOscString(self: *Self, input: []const u8) usize {
    fn consume_osc_string(&mut self, input: &[u8]) -> usize {
        // comptime assert(!@hasDecl(T, "vtRaw"));
        //
        // Same treatment as `consume_apc_string`: Rust cannot assert an
        // associated const inside a generic fn body (the fn is monomorphized
        // for EVERY handler), so this is a debug assertion and
        // `stream_fast_paths_bypassed_when_vt_raw_hooked` is the real guard.
        debug_assert!(!H::HAS_VT_RAW);
        // assert(self.parser.state == .osc_string);
        assert!(self.parser.state == parser::State::OscString);

        // var end: usize = 0;
        let mut end: usize = 0;

        // `8c5bc3d29`: scan ordinary OSC bytes a vector at a time, keeping the
        // scalar loop below as the tail and as the whole scan on targets with
        // no recommended vector width (mirroring the APC path).
        // if (comptime std.simd.suggestVectorLength(u8)) |vector_len| {
        //     const ByteVector = @Vector(vector_len, u8);
        //     while (end + vector_len <= input.len) {
        //         const bytes: ByteVector = input[end..][0..vector_len].*;
        //         const stop = bytes < @as(ByteVector, @splat(0x20));
        //         if (@reduce(.Or, stop)) break;
        //         end += vector_len;
        //     }
        // }
        if let Some(vector_len) = OSC_SCAN_LANES {
            while end + vector_len <= input.len() {
                let mut stop = false;
                for j in 0..vector_len {
                    stop |= input[end + j] < 0x20;
                }
                if stop {
                    break;
                }
                end += vector_len;
            }
        }

        // while (end < input.len) {
        while end < input.len() {
            match input[end] {
                // Not osc_put bytes: BEL/CAN/SUB/ESC terminate or
                // abort the state; other C0 bytes are ignored by it.
                // 0x00...0x1F => break,
                0x00..=0x1F => break,
                // Everything else is an osc_put byte.
                // else => end += 1,
                _ => end += 1,
            }
        }

        // if (end > 0) self.parser.osc_parser.nextSlice(input[0..end]);
        if end > 0 {
            self.parser.osc_parser.next_slice(&input[0..end]);
        }
        // return end;
        end
    }

    /// Fast path for a byte in the csi_entry state, the state right
    /// after "ESC [". Virtually every CSI sequence spends exactly
    /// one byte in this state, on either a digit, a private marker,
    /// or a final byte. Returns true if the byte was fully handled;
    /// false means the caller must process it through the general
    /// state machine.
    ///
    /// Must not be used by handlers with a vtRaw hook because the
    /// final byte case dispatches the CSI directly.
    // inline fn csiEntryByte(self: *Self, c: u8) bool {
    //     comptime assert(!@hasDecl(T, "vtRaw"));
    #[inline(always)]
    unsafe fn csi_entry_byte(this: *mut Self, c: u8) -> bool {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // The reference's `comptime assert(!@hasDecl(T, "vtRaw"))` is a
            // compile-time precondition on the caller. Rust cannot assert an
            // associated const inside a generic fn body without a const-generic
            // trick that would obscure the port, so it is a debug assertion — and
            // `stream_fast_paths_bypassed_when_vt_raw_hooked` is the real guard.
            debug_assert!(!H::HAS_VT_RAW);
            // assert(self.parser.state == .csi_entry);
            debug_assert!((*this).parser.state == parser::State::CsiEntry);
            // switch (c) {
            match c {
                // First parameter digit.
                b'0'..=b'9' => {
                    (*this).parser.state = parser::State::CsiParam;
                    // param_acc is zero (cleared on escape entry)
                    // so accumulating is just the digit value.
                    (*this).parser.param_acc = u16::from(c - b'0');
                    (*this).parser.param_acc_idx = 1;
                }
                // An empty first parameter.
                b';' => {
                    (*this).parser.state = parser::State::CsiParam;
                    (*this).parser.params[0] = 0;
                    (*this).parser.params_idx = 1;
                }
                // Private marker (e.g. '?' in "ESC [ ? 2004 h").
                0x3C..=0x3F => {
                    (*this).parser.state = parser::State::CsiParam;
                    (*this).parser.collect(c);
                }
                // A final byte: a parameterless CSI.
                0x40..=0x7E => Self::csi_dispatch_final(this, c),
                // Defer to the state machine for anything else
                // (C0 controls, intermediates, colon).
                _ => return false,
            }
            // return true;
            true
        }
    }

    /// Bulk-consume CSI parameter bytes (digits and separators)
    /// and, if reached, the final byte (dispatching the CSI).
    /// Returns the number of bytes consumed. Stops at the first
    /// byte that isn't handled here, leaving the parser in the
    /// csi_param state so the caller can process that byte.
    //
    // PORT NOTE (`253e4f9c3`, WS10 T13) — the `PERF(port)` stop-and-decide this
    // task's Step 2 requires. The colliding rustty divergence is
    // `crates/rustty-vt/src/parser.rs:592` "PERF(port): state-equal fast path"
    // (the r5-parser-fastpath win). VERDICT: **coexist, both kept, neither
    // altered.**
    //
    // They operate at different levels and cannot subsume one another. The r5
    // divergence lives INSIDE `Parser::next` and short-circuits the
    // exit/transition/entry triple whenever `state == next_state`; it is what
    // makes the per-byte path cheap for EVERY parser state. `consume_csi_params`
    // does not touch `Parser::next` at all — it hoists one specific state
    // (`csi_param`) out of the byte loop entirely, so for those bytes
    // `Parser::next` is never called and the r5 fast path is simply not on the
    // path. For every other byte — every non-CSI state, and ALL CSI bytes when
    // the handler declares `vt_raw` — `Parser::next` still runs and the r5 fast
    // path still governs it. Nothing was deleted, nothing was weakened, and no
    // `PERF(port)` marker was removed.
    //
    // ERRATUM ON THE PLAN'S TASK ORDER. The plan sequences this as T13, BEFORE
    // T14's `1a88f3622`. That is backwards: `consumeCsiParams` calls
    // `self.csiDispatchFinal(c)`, which `1a88f3622` introduces, and upstream's
    // own order is `47e26df60` -> `1a88f3622` -> `253e4f9c3` -> `300f42c7a`
    // (`git -C ../ghostty log --oneline 4749c4e93..origin/main --
    // src/terminal/stream.zig`). Ported in the upstream order.
    // fn consumeCsiParams(self: *Self, input: []const u8) usize {
    unsafe fn consume_csi_params(this: *mut Self, input: &[u8]) -> usize {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // const p = &self.parser;
            // assert(p.state == .csi_param);
            debug_assert!((*this).parser.state == parser::State::CsiParam);

            // Accumulate parser state in locals for the hot loop.
            // var acc = p.param_acc;
            // var acc_idx = p.param_acc_idx;
            // var idx = p.params_idx;
            let mut acc = (*this).parser.param_acc;
            let mut acc_idx = (*this).parser.param_acc_idx;
            let mut idx = (*this).parser.params_idx;

            // var offset: usize = 0;
            let mut offset: usize = 0;
            // while (offset < input.len) {
            while offset < input.len() {
                // const c = input[offset];
                let c = input[offset];
                // switch (c) {
                match c {
                    // A parameter digit.
                    b'0'..=b'9' => {
                        // if (idx < Parser.MAX_PARAMS) {
                        if (idx as usize) < parser::MAX_PARAMS {
                            // acc *|= 10;
                            // acc +|= c - '0';
                            // acc_idx |= 1;
                            acc = acc.saturating_mul(10);
                            acc = acc.saturating_add(u16::from(c - b'0'));
                            acc_idx |= 1;
                        }
                        offset += 1;
                    }

                    // A parameter separator.
                    b':' | b';' => {
                        // if (idx < Parser.MAX_PARAMS) {
                        if (idx as usize) < parser::MAX_PARAMS {
                            // p.params[idx] = acc;
                            (*this).parser.params[idx as usize] = acc;
                            // if (c == ':') p.params_sep.set(idx);
                            if c == b':' {
                                (*this).parser.params_sep.set(idx as usize);
                            }
                            // idx += 1; acc = 0; acc_idx = 0;
                            idx += 1;
                            acc = 0;
                            acc_idx = 0;
                        }
                        offset += 1;
                    }

                    // A final byte: dispatch the CSI.
                    0x40..=0x7E => {
                        // p.param_acc = acc;
                        // p.param_acc_idx = acc_idx;
                        // p.params_idx = idx;
                        (*this).parser.param_acc = acc;
                        (*this).parser.param_acc_idx = acc_idx;
                        (*this).parser.params_idx = idx;
                        // self.csiDispatchFinal(c);
                        Self::csi_dispatch_final(this, c);
                        // return offset + 1;
                        return offset + 1;
                    }

                    // Anything else (C0 controls, intermediates, etc.)
                    // is handled by the caller.
                    _ => break,
                }
            }

            // p.param_acc = acc;
            // p.param_acc_idx = acc_idx;
            // p.params_idx = idx;
            // return offset;
            (*this).parser.param_acc = acc;
            (*this).parser.param_acc_idx = acc_idx;
            (*this).parser.params_idx = idx;
            offset
        }
    }

    /// Like nextSlice but takes one byte and is necessarily a scalar
    /// operation that can't use SIMD. Prefer nextSlice if you can and
    /// try to get multiple bytes at once.
    // pub inline fn next(self: *Self, c: u8) void {
    ///
    /// PORT NOTE (N452, P7-W2 T9): the reference's `self: *Self` is an
    /// unprotected pointer, and a handler effect may re-enter the owning
    /// terminal (the C API's `resize`, `set`, `get`, continuation calls) while
    /// this runs. A Rust `&mut self` would be a protected Unique over every
    /// stream byte for the whole call, so this body is raw-self; the `&mut
    /// self` method of the same name forwards here for Rust callers.
    ///
    /// # Safety
    /// `this` must point to a live stream, and no reference to it may be live
    /// in the caller across this call.
    pub unsafe fn next_raw(this: *mut Self, c: u8) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // self.nextUntracked(c);
            Self::next_untracked(this, c);
            // if (self.continuation != null) self.trackContinuation(&.{c});
            if (*this).continuation.is_some() {
                (*this).track_continuation(&[c]);
            }
        }
    }

    /// The safe entry of [`Self::next_raw`].
    #[inline]
    pub fn next(&mut self, c: u8) {
        // SAFETY: as in `next_slice`.
        unsafe { Self::next_raw(self, c) }
    }

    // inline fn nextUntracked(self: *Self, c: u8) void {
    unsafe fn next_untracked(this: *mut Self, c: u8) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // The scalar path can be responsible for decoding UTF-8.
            // if (self.parser.state == .ground) { self.nextUtf8(c); return; }
            if (*this).parser.state == parser::State::Ground {
                Self::next_utf8(this, c);
                return;
            }

            // self.nextNonUtf8(c);
            Self::next_non_utf8(this, c);
        }
    }

    /// Process the next byte and print as necessary.
    ///
    /// This assumes we're in the UTF-8 decoding state. If we may not
    /// be in the UTF-8 decoding state call nextSlice or next.
    // inline fn nextUtf8(self: *Self, c: u8) void { ... }
    unsafe fn next_utf8(this: *mut Self, c: u8) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // assert(self.parser.state == .ground);
            debug_assert!((*this).parser.state == parser::State::Ground);

            // const res = self.utf8decoder.next(c);
            let res = (*this).utf8decoder.next(c);
            // const consumed = res[1];
            let consumed = res.1;
            // if (res[0]) |codepoint| { self.handleCodepoint(codepoint); }
            if let Some(codepoint) = res.0 {
                Self::handle_codepoint(this, codepoint);
            }
            // if (!consumed) { ... }
            if !consumed {
                // We optimize for the scenario where the text being
                // printed in the terminal ISN'T full of ill-formed
                // UTF-8 sequences.
                // @branchHint(.unlikely);

                // const retry = self.utf8decoder.next(c);
                let retry = (*this).utf8decoder.next(c);
                // It should be impossible for the decoder
                // to not consume the byte twice in a row.
                // assert(retry[1] == true);
                debug_assert!(retry.1);
                // if (retry[0]) |codepoint| { self.handleCodepoint(codepoint); }
                if let Some(codepoint) = retry.0 {
                    Self::handle_codepoint(this, codepoint);
                }
            }
        }
    }

    /// To be called whenever the utf-8 decoder produces a codepoint.
    ///
    /// This function is abstracted this way to handle the case where
    /// the decoder emits a 0x1B after rejecting an ill-formed sequence.
    // inline fn handleCodepoint(self: *Self, c: u21) void { ... }
    unsafe fn handle_codepoint(this: *mut Self, c: u32) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // We need to increase the eval branch limit because a lot of
            // tests end up running almost completely at comptime due to
            // a chain of inline functions.
            // @setEvalBranchQuota(100_000);

            // C0 control or a C1 control decoded from UTF-8: exactly
            // the codepoints with no bit set outside 0x9F (bits 0-4
            // and bit 7 cover 0x00-0x1F and 0x80-0x9F and nothing
            // else), so the printable fast path stays a single
            // AND-test.
            // if ((c & ~@as(u21, 0x9F)) == 0) {
            if (c & !0x9Fu32) == 0 {
                // ESC
                // if (c == 0x1B) { self.parser.state = .escape; self.parser.clear(); return; }
                if c == 0x1B {
                    (*this).parser.state = parser::State::Escape;
                    (*this).parser.clear();
                    return;
                }

                // Ignore C1 that came via UTF-8 decoding, matching xterm.
                // if (c > 0x1F) {
                if c > 0x1F {
                    // logUnsupportedOnce(
                    //     "ignoring UTF-8-decoded C1 controls, first: 0x{x}",
                    //     .{c},
                    //     0x80,
                    // );
                    if log_unsupported_once(&SEEN_UTF8_DECODED_C1_CODEPOINT, 0x80) {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring UTF-8-decoded C1 controls, first: 0x{c:x}"
                        );
                    }
                    return;
                }

                // self.execute(@intCast(c));
                Self::execute(&mut (*this).handler, c as u8);
                return;
            }
            // self.print(@intCast(c));
            Self::print(&mut (*this).handler, c);
        }
    }

    /// Process the next character and call any callbacks if necessary.
    ///
    /// This assumes that we're not in the UTF-8 decoding state. If
    /// we may be in the UTF-8 decoding state call nextSlice or next.
    // fn nextNonUtf8(self: *Self, c: u8) void { ... }
    unsafe fn next_non_utf8(this: *mut Self, c: u8) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // assert(self.parser.state != .ground);
            debug_assert!((*this).parser.state != parser::State::Ground);

            // Fast path for CSI entry.
            // if (self.parser.state == .escape and c == '[') { self.parser.state = .csi_entry; return; }
            if (*this).parser.state == parser::State::Escape && c == b'[' {
                (*this).parser.state = parser::State::CsiEntry;
                return;
            }

            // The fast paths below dispatch actions directly rather than
            // going through Parser.next, so they'd bypass a handler's
            // vtRaw hook. Handlers with vtRaw (e.g. the inspector) use
            // the general path for anything that produces an action.
            // const has_vt_raw = comptime @hasDecl(T, "vtRaw");
            //
            // A `let` binding, not a nested `const` item — Rust nested items cannot
            // capture the outer generic `H`. `H::HAS_VT_RAW` is an associated const,
            // so this is still a compile-time constant per monomorphization and both
            // branches below are const-folded exactly like the reference's
            // `comptime`.
            let has_vt_raw: bool = H::HAS_VT_RAW;

            // Fast path for CSI params.
            // if (self.parser.state == .csi_param) csi_param: { ... }
            if (*this).parser.state == parser::State::CsiParam {
                // csi_param is the most common parser state
                // other than ground by a fairly wide margin.
                'csi_param: {
                    match c {
                        // A C0 escape (yes, this is valid):
                        0x00..=0x0F => Self::execute(&mut (*this).handler, c),
                        // We ignore C0 escapes > 0xF since execute
                        // doesn't have processing for them anyway:
                        0x10..=0x17 | 0x19 | 0x1C..=0x1F => {}
                        // We don't currently have any handling for
                        // 0x18 or 0x1A, but they should still move
                        // the parser state to ground.
                        0x18 | 0x1A => (*this).parser.state = parser::State::Ground,
                        // A parameter digit:
                        b'0'..=b'9' => {
                            if ((*this).parser.params_idx as usize) < parser::MAX_PARAMS {
                                (*this).parser.param_acc =
                                    (*this).parser.param_acc.saturating_mul(10);
                                (*this).parser.param_acc =
                                    (*this).parser.param_acc.saturating_add((c - b'0') as u16);
                                // The parser's CSI param action uses param_acc_idx
                                // to decide if there's a final param that needs to
                                // be consumed or not, but it doesn't matter really
                                // what it is as long as it's not 0.
                                (*this).parser.param_acc_idx |= 1;
                            }
                        }
                        // A parameter separator:
                        b':' | b';' => {
                            if ((*this).parser.params_idx as usize) < parser::MAX_PARAMS {
                                (*this).parser.params[(*this).parser.params_idx as usize] =
                                    (*this).parser.param_acc;
                                if c == b':' {
                                    (*this)
                                        .parser
                                        .params_sep
                                        .set((*this).parser.params_idx as usize);
                                }
                                (*this).parser.params_idx += 1;

                                (*this).parser.param_acc = 0;
                                (*this).parser.param_acc_idx = 0;
                            }
                        }
                        // A final byte: dispatch the CSI directly.
                        // 0x40...0x7E => if (comptime !has_vt_raw) {
                        //     self.csiDispatchFinal(c);
                        // } else break :csi_param,
                        0x40..=0x7E => {
                            if !has_vt_raw {
                                Self::csi_dispatch_final(this, c);
                            } else {
                                break 'csi_param;
                            }
                        }
                        // Explicitly ignored:
                        0x7F => {}
                        // Defer to the state machine to
                        // handle any other characters:
                        _ => break 'csi_param,
                    }
                    return;
                }
            }

            // Fast path for CSI entry, the state right after "ESC [".
            // if (comptime !has_vt_raw) {
            //     if (self.parser.state == .csi_entry) {
            //         if (self.csiEntryByte(c)) return;
            //     }
            // }
            if !has_vt_raw
                && (*this).parser.state == parser::State::CsiEntry
                && Self::csi_entry_byte(this, c)
            {
                return;
            }

            // We explicitly inline this call here for performance reasons.
            //
            // We do this rather than mark Parser.next as inline because doing
            // that causes weird behavior in some tests- I'm not sure if they
            // miscompile or it's just very counter-intuitive comptime stuff,
            // but regardless, this is the easy solution.
            // const actions = @call(.always_inline, Parser.next, .{ &self.parser, c });
            let [exit, transition, entry] = (*this).parser.next(c);

            // PORT NOTE (parity tail S7-7): the actions hold the parser borrowed
            // (an `osc_dispatch` command borrows the OSC parser's storage, as the
            // reference's does), and the loop below calls back into `self`. Only
            // the exit action can borrow (`Parser::next`), so the other two are
            // detached, and an `osc_dispatch` exit is run first, against the
            // handler alone, which is where the reference's loop runs it too.
            let transition = transition.map(detach_action);
            let entry = entry.map(detach_action);
            let exit = match exit {
                None => None,
                Some(action) => match action.into_static() {
                    Ok(action) => Some(action),
                    Err(cmd) => {
                        Self::run_osc_dispatch_action(&mut (*this).handler, cmd);
                        None
                    }
                },
            };

            // for (actions) |action_opt| { ... }
            for action_opt in [exit, transition, entry] {
                // const action = action_opt orelse continue;
                let action = match action_opt {
                    Some(a) => a,
                    None => continue,
                };
                if DEBUG {
                    log::info!(target: LOG_SCOPE, "action: {action:?}");
                }

                // A handler can expose this to get the raw action before
                // it is further parsed. If this returns `true` then we skip
                // processing ourselves.
                // if (@hasDecl(T, "vtRaw")) { ... }
                // PORT NOTE: `@hasDecl(T, "vtRaw")` is mirrored by the `vt_raw`
                // trait method (default returns `false` = do not skip). The
                // reference's `catch |err| ...` path (a handler may return an
                // error) is folded into the default: a faithful handler's
                // `vt_raw` returns the skip flag directly. The reference's error
                // arm rationale:
                //   // Always skip erroneous actions because we can't
                //   // be sure...
                //   break :err true;
                if (*this).handler.vt_raw(&action) {
                    continue;
                }

                // switch (action) { ... }
                match action {
                    parser::Action::Print(p) => Self::print(&mut (*this).handler, p),
                    parser::Action::Execute(code) => Self::execute(&mut (*this).handler, code),
                    // PERF(port): `*` unboxes the pointer-sized Action payloads
                    // (see the Action PERF note); the by-value handler signatures
                    // are unchanged.
                    parser::Action::CsiDispatch(csi_action) => {
                        Self::csi_dispatch(&mut (*this).handler, *csi_action)
                    }
                    parser::Action::EscDispatch(esc) => {
                        Self::esc_dispatch(&mut (*this).handler, *esc)
                    }
                    // (`into_static` hands every `osc_dispatch` to
                    // `run_osc_dispatch_action` above.)
                    parser::Action::OscDispatch(cmd) => {
                        Self::osc_dispatch(&mut (*this).handler, *cmd)
                    }
                    parser::Action::DcsHook(dcs) => (*this).handler.dcs_hook(*dcs),
                    parser::Action::DcsPut(code) => (*this).handler.dcs_put(code),
                    parser::Action::DcsUnhook => (*this).handler.dcs_unhook(),
                    parser::Action::ApcStart => (*this).handler.apc_start(),
                    parser::Action::ApcPut(code) => (*this).handler.apc_put(code),
                    // .apc_end => self.handler.vt(.apc_end, .{
                    //     .terminated = c == std.ascii.control_code.esc or c == 0x9C,
                    // }),
                    parser::Action::ApcEnd => (*this).handler.apc_end(ApcEnd {
                        terminated: c == 0x1B || c == 0x9C,
                    }),
                }
            }
        }
    }
}

// PORT NOTE (b5053153f, WS10 T7): the reference's `logUnsupportedOnce` is a
// generic `fn (comptime format, args, key: u16) void` whose dedup table is a
// `struct { var seen: [16]u32 }` inside the function body. Zig instantiates one
// such table per `(format, @TypeOf(args))` tuple — NOT per call site. Two sites
// that pass the same format string and the same argument type therefore SHARE a
// table, and the commit contains exactly two such pairs: the SM/RM
// `"unimplemented mode: {}"` sites, and the two `"unimplemented ESC action: {f}"`
// sites. Rust has no per-instantiation static, so the port makes the table an
// explicit `&'static UnsupportedOnceTable` argument and declares one static per
// distinct reference tuple — TEN statics for TWELVE call sites, with those
// pairs deliberately sharing theirs.
//
// The two `"ignoring UTF-8-decoded C1 controls, first: 0x{x}"` sites are NOT
// such a pair: `stream.zig:734` passes `.{cp}` where `cp` is a `u32`
// (`cp_buf: [4096]u32`, `stream.zig:671`) and `stream.zig:1185` passes `.{c}`
// where `c` is the `u21` parameter of `handleCodepoint`. Different
// `@TypeOf(args)` means two separate Zig instantiations, so each decode path
// has its own table and the message can be emitted once per path. The port
// gives each site its own static (`SEEN_UTF8_DECODED_C1_SLICE`,
// `SEEN_UTF8_DECODED_C1_CODEPOINT`; NEW-FINDINGS N3, which found them sharing
// one). Consequence of the per-tuple granularity that IS shared: under the
// reference, setting then resetting the same unimplemented mode logs ONCE,
// not twice.
//
// PORT NOTE (logging facade): the `log` facade IS wired now
// (DECISION-logging.md §6/§6r/§6r2, call shape 3 for `rustty-vt`), so the
// commit's tail — `log.warn(format, args)` — lands too, but at each CALL SITE
// rather than inside the helper: `log_unsupported_once` keeps its `-> bool`
// signature and every site emits `log::warn!(target: LOG_SCOPE, ...)` under the
// returned decision. See that function's doc comment for why the format string
// cannot be pushed back into the helper in Rust. Everything else — the table,
// the lock-free claim scan, the atomic orderings, the table-full suppression,
// the per-site key choice, and the call-site conversions — is live code and is
// exercised by `stream_logs_unsupported_once_per_value` and
// `stream_unsupported_once_emits_once_per_key`.
//
// Consequence for the commit's PERF claim: upstream measured +3.2% on a 2.6 GB
// real-session corpus, all of it eliminated `writev`. rustty issues no `writev`
// here to eliminate, so the ported change is perf-NEUTRAL on this side by
// construction — it is ported for fidelity and for the facade's benefit, not
// for a win. Recorded as such rather than claimed as one.

/// The per-`(format, args)` dedup table behind [`log_unsupported_once`].
///
/// ```zig
/// const Static = struct {
///     var seen: [16]u32 = @splat(empty);
/// };
/// ```
struct UnsupportedOnceTable {
    seen: [core::sync::atomic::AtomicU32; 16],
}

impl UnsupportedOnceTable {
    /// `u32` slots so every `u16` key is representable alongside an empty
    /// sentinel and so 32-bit targets (e.g. wasm32) have native atomics.
    const EMPTY: u32 = u32::MAX;

    const fn new() -> Self {
        // `[AtomicU32; 16]` cannot use the `[expr; N]` repeat form because
        // `AtomicU32` is not `Copy`; the reference's `@splat(empty)` is spelled
        // out. `const`-callable so these can be `static`s.
        Self {
            seen: [
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
                core::sync::atomic::AtomicU32::new(Self::EMPTY),
            ],
        }
    }

    /// Test-only reader: the number of claimed slots. The table is the only
    /// observable this change has in a facade-free port, so the test asserts on
    /// it rather than on captured log output.
    #[cfg(test)]
    fn claimed(&self) -> usize {
        use core::sync::atomic::Ordering;
        self.seen
            .iter()
            .filter(|s| s.load(Ordering::Acquire) != Self::EMPTY)
            .count()
    }
}

/// Logs an unsupported-input message at most once per distinct key
/// per process.
///
/// These messages are emitted in response to input that the terminal
/// application controls, so a misbehaving (or merely chatty) program
/// can trigger the same message millions of times, e.g. by toggling
/// an unimplemented mode on every frame. Each log call has a real
/// throughput cost (formatting plus a blocking write per message)
/// while adding no diagnostic value beyond the first occurrence.
///
/// The keys seen so far are tracked in a small fixed table (64 bytes)
/// instantiated per (format, argument type) tuple, i.e. roughly per
/// call site. Real streams only ever produce a handful of distinct
/// unsupported values per site, so if the table ever fills, messages
/// for further new values are suppressed as well: by that point the
/// log already shows this class of problem and unbounded distinct
/// values would flood it anyway.
///
/// Returns whether the caller should emit the message; each call site emits
/// `log::warn!(target: LOG_SCOPE, ...)` under that boolean. The reference emits
/// it internally (`log.warn(format, args)`) because Zig's `comptime format:
/// []const u8` + `args: anytype` costs nothing until the message is actually
/// written. Rust has no comptime-generic equivalent: a signature taking the
/// format string would have to accept `fmt::Arguments`, which forces the caller
/// to format EAGERLY on every call — destroying the whole point of the
/// once-table, which is to avoid formatting work on the repeat path. Returning
/// the decision keeps the reference's "format only when we will actually emit"
/// property exactly. See the PORT NOTE above.
fn log_unsupported_once(table: &UnsupportedOnceTable, key: u16) -> bool {
    use core::sync::atomic::Ordering;

    let key = u32::from(key);

    // The atomics make concurrent streams safe: slots are only ever
    // claimed, never changed, so the scan can stop at the first empty
    // slot. The worst case race is a benign duplicate message.
    //
    // for (&Static.seen) |*slot| { ... } else return;  // table full
    for slot in &table.seen {
        let cur = slot.load(Ordering::Acquire);
        if cur == key {
            return false; // already logged
        }
        if cur != UnsupportedOnceTable::EMPTY {
            continue; // other key, keep scanning
        }

        // Empty slot: claim it for this key and log below.
        match slot.compare_exchange(
            UnsupportedOnceTable::EMPTY,
            key,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            // `@cmpxchgStrong(...) orelse break` — null means we won.
            Ok(_) => return true,

            // Lost the race: suppress if it was to the same key, keep
            // scanning otherwise.
            Err(actual) => {
                if actual == key {
                    return false;
                }
            }
        }
    }

    // else return;  // table full: suppress new values too
    false
}

// The ten statics behind the twelve call sites, in the order they appear in
// `stream.zig`. `SEEN_UNIMPLEMENTED_MODE` and `SEEN_UNIMPLEMENTED_ESC_ACTION`
// are each shared by two call sites exactly as Zig shares them. The two
// UTF-8-decoded C1 sites are two Zig instantiations (`.{cp}` is `u32` at
// `stream.zig:734`, `.{c}` is `u21` at `:1185`), so each has its own table.
static SEEN_UTF8_DECODED_C1_SLICE: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_UTF8_DECODED_C1_CODEPOINT: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_INVALID_C0: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_INVALID_DEVICE_ATTRIBUTES: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_UNIMPLEMENTED_MODE: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_UNKNOWN_CSI_M_INTERMEDIATE: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_CSI_22_23_T_EXTRA_PARAMS: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_CSI_T_UNIMPLEMENTED_PARAM: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_OSC1_CHANGE_ICON: UnsupportedOnceTable = UnsupportedOnceTable::new();
static SEEN_UNIMPLEMENTED_ESC_ACTION: UnsupportedOnceTable = UnsupportedOnceTable::new();

// PORT NOTE: the reference test handlers are defined inline within each `test`
// block as a local struct `H` with a comptime `vt(comptime action, value)`
// dispatch method (Zig's optional dispatch). The Rust port mirrors each one as
// a test-local struct implementing the `Handler` trait, overriding exactly the
// methods that test observes — faithfully reproducing the reference's per-test
// handler. The reference's `Action.Key` tag enum (used by the CSI W tests to
// record *which* action fired) has no Rust equivalent because `Action` is
// modeled as the `Handler` trait; those tests record the observed action via a
// test-local enum that mirrors the relevant `Action.Key` variants.
#[cfg(test)]
mod tests;
