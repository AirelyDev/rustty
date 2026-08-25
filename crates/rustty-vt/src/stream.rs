//! rustty-vt stream
//!
//! PORTS: ghostty src/terminal/stream.zig
//!
//! PORT STATUS: done (vt-9) — the `Handler` trait, `Stream<H>`, `init`/`deinit`,
//! `next`/`next_slice`, the action router, and the full
//! `csi`/`osc`/`esc`/`dcs`/`apc` dispatch, with `TestHandler`s + all 38 tests.
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
//! (`options::SIMD`, `build/Config.zig:200-206`). (All 38 stream tests pass in both modes;
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

// const log = std.log.scoped(.stream);

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
// pub const ClipboardContents = struct { kind: u8, data: []const u8 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardContents<'a> {
    pub kind: u8,
    pub data: &'a [u8],
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorOperation {
    pub op: osc::color::Operation,
    pub requests: osc::color::List,
    pub terminator: osc::Terminator,
}

/// `Action.SemanticPrompt = osc.Command.SemanticPrompt`
pub type SemanticPrompt = osc::SemanticPrompt;

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
    fn vt_raw(&mut self, _action: &parser::Action) -> bool {
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
    fn kitty_color_report(&mut self, _v: kitty::color::OSC) {}

    /// `color_operation`
    fn color_operation(&mut self, _v: ColorOperation) {}

    /// `semantic_prompt` (OSC 133)
    fn semantic_prompt(&mut self, _v: SemanticPrompt) {}
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
pub struct Stream<'a, H: Handler> {
    // handler: Handler,
    pub handler: H,
    // parser: Parser,
    pub parser: Parser<'a>,
    // utf8decoder: UTF8Decoder,
    pub utf8decoder: UTF8Decoder,
    // continuation: ?continuationpkg.Tracker,
    pub continuation: Option<continuationpkg::Tracker>,
}

// pub const Options = struct { ... };
pub struct Options<'a, H> {
    /// The handler initial value, must be set.
    // handler: Handler,
    pub handler: H,

    /// Allocator to use. If this is not set then the stream
    /// will be fully allocation free. There are some operations
    /// that will be dropped in this case such as OSC 52 clipboard
    /// ops.
    // allocator: ?Allocator = null,
    pub allocator: Option<rustty_lib::allocator::ZigAllocatorRef<'a>>,

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

impl<'a, H> Options<'a, H> {
    /// `.{ .handler = h }` — the reference's Zig field defaults
    /// (`allocator = null`, `continuation_max_bytes = null`) have no Rust
    /// struct-literal equivalent, so this constructor fills them.
    pub fn with_handler(handler: H) -> Options<'a, H> {
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

impl<'a, H: Handler> Stream<'a, H> {
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
    pub fn init(options: Options<'a, H>) -> Stream<'a, H> {
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
        // PORT NOTE: the Rust `Tracker` allocates on the global allocator (see
        // its `alloc` PORT NOTE), so the gate consumes `options.allocator`'s
        // presence only.
        let mut tracker: Option<continuationpkg::Tracker> = None;
        if options.allocator.is_some() {
            if let Some(max_bytes) = options.continuation_max_bytes {
                if max_bytes > 0 {
                    tracker = Some(continuationpkg::Tracker::init(max_bytes));
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
        writer: &mut dyn std::io::Write,
    ) -> Result<(), WriteContinuationError> {
        // const tracker = self.continuation orelse
        //     return error.ContinuationDisabled;
        let tracker = self
            .continuation
            .as_ref()
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
        // Parser ground alone is not sufficient because the UTF-8
        // decoder may have some state.
        self.parser.state == parser::State::Ground && self.utf8decoder.state == 0
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
    pub fn next_slice(&mut self, input: &[u8]) {
        // self.nextSliceUntracked(input);
        self.next_slice_untracked(input);

        // Continuation tracking is opt-in and this branch predicts
        // perfectly, so disabled streams pay nothing else here.
        // if (self.continuation != null) self.trackContinuation(input);
        if self.continuation.is_some() {
            self.track_continuation(input);
        }
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
    pub fn next_slice_until_ground(&mut self, input: &[u8]) -> Option<usize> {
        // const consumed = self.nextSliceUntilGroundUntracked(input);
        let consumed = self.next_slice_until_ground_untracked(input);
        // const consumed_len = consumed orelse input.len;
        let consumed_len = consumed.unwrap_or(input.len());
        // if (self.continuation != null and consumed_len > 0) {
        //     self.trackContinuation(input[0..consumed_len]);
        // }
        if self.continuation.is_some() && consumed_len > 0 {
            self.track_continuation(&input[0..consumed_len]);
        }

        // return consumed;
        consumed
    }

    // inline fn nextSliceUntilGroundUntracked(
    //     self: *Self,
    //     input: []const u8,
    // ) ?usize {
    fn next_slice_until_ground_untracked(&mut self, input: &[u8]) -> Option<usize> {
        // if (self.ground()) return 0;
        if self.ground() {
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
        while self.utf8decoder.state != 0 {
            if offset >= input.len() {
                return None;
            }
            self.next_utf8(input[offset]);
            offset += 1;
        }

        // Process non-UTF-8
        // if (self.parser.state != .ground) {
        //     offset += self.consumeUntilGround(input[offset..]);
        // }
        if self.parser.state != parser::State::Ground {
            offset += self.consume_until_ground(&input[offset..]);
        }

        // return if (self.ground()) offset else null;
        if self.ground() {
            Some(offset)
        } else {
            None
        }
    }

    // inline fn nextSliceUntracked(self: *Self, input: []const u8) void { ... }
    fn next_slice_untracked(&mut self, input: &[u8]) {
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
                self.next_untracked(c);
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
            self.next_slice_capped(&input[i..i + len], &mut cp_buf);
            // i += len;
            i += len;
            // if (i >= input.len) break;
            if i >= input.len() {
                break;
            }
        }
    }

    // inline fn nextSliceCapped(
    //     self: *Self,
    //     input: []const u8,
    //     cp_buf: []u32,
    // ) void {
    fn next_slice_capped(&mut self, input: &[u8], cp_buf: &mut [u32]) {
        // assert(input.len <= cp_buf.len);
        assert!(input.len() <= cp_buf.len());

        // var offset: usize = 0;
        let mut offset: usize = 0;

        // If the scalar UTF-8 decoder was in the middle of processing
        // a code sequence, we continue until it's not.
        // while (self.utf8decoder.state != 0) {
        while self.utf8decoder.state != 0 {
            // if (offset >= input.len) return;
            if offset >= input.len() {
                return;
            }
            // self.nextUtf8(input[offset]);
            self.next_utf8(input[offset]);
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
        offset += self.consume_until_ground(&input[offset..]);
        // if (offset >= input.len) return;
        if offset >= input.len() {
            return;
        }
        // offset += self.consumeAllEscapes(input[offset..]);
        offset += self.consume_all_escapes(&input[offset..]);

        // If we're in the ground state then we can use SIMD to process
        // input until we see an ESC (0x1B), since all other characters
        // up to that point are just UTF-8.
        // while (self.parser.state == .ground and offset < input.len) {
        while self.parser.state == parser::State::Ground && offset < input.len() {
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
                // if (cp <= 0xF) {
                if cp <= 0xF {
                    // @branchHint(.unlikely);
                    // PORT NOTE: `@branchHint(.unlikely)` has no stable Rust
                    // equivalent (`core::intrinsics::likely` is nightly); the
                    // branch structure is otherwise identical.
                    // self.execute(@intCast(cp));
                    self.execute(cp as u8);
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
                        while end + lanes <= cps.len() {
                            // const gt = v > threshold;
                            // if (!@reduce(.And, gt)) { end += @ctz(~bits); break :scan; }
                            let mut first_bad: Option<usize> = None;
                            for j in 0..lanes {
                                if cps[end + j] <= 0xF {
                                    first_bad = Some(j);
                                    break;
                                }
                            }
                            if let Some(j) = first_bad {
                                end += j;
                                break 'scan;
                            }
                            end += lanes;
                        }
                    }
                    // while (end < cps.len and cps[end] > 0xF) end += 1;
                    while end < cps.len() && cps[end] > 0xF {
                        end += 1;
                    }
                }
                // self.handler.vt(.print_slice, .{ .cps = cps[i..end] });
                self.handler.print_slice(&cps[i..end]);
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
                    self.next_utf8(c);
                }
                // return;
                return;
            }

            // Process control sequences until we run out.
            // offset += self.consumeAllEscapes(input[offset..]);
            offset += self.consume_all_escapes(&input[offset..]);
        }
    }

    /// Parses back-to-back escape sequences until none are left.
    /// Returns the number of bytes consumed from the provided input.
    ///
    /// Expects input to start with 0x1B, use consumeUntilGround first
    /// if the stream may be in the middle of an escape sequence.
    // inline fn consumeAllEscapes(self: *Self, input: []const u8) usize { ... }
    fn consume_all_escapes(&mut self, input: &[u8]) -> usize {
        // var offset: usize = 0;
        let mut offset: usize = 0;
        // while (input[offset] == 0x1B) {
        while input[offset] == 0x1B {
            // self.parser.state = .escape;
            self.parser.state = parser::State::Escape;
            // self.parser.clear();
            self.parser.clear();
            // offset += 1;
            offset += 1;
            // offset += self.consumeUntilGround(input[offset..]);
            offset += self.consume_until_ground(&input[offset..]);
            // if (offset >= input.len) return input.len;
            if offset >= input.len() {
                return input.len();
            }
        }
        // return offset;
        offset
    }

    /// Parses escape sequences until the parser reaches the ground state.
    /// Returns the number of bytes consumed from the provided input.
    // inline fn consumeUntilGround(self: *Self, input: []const u8) usize { ... }
    fn consume_until_ground(&mut self, input: &[u8]) -> usize {
        // var offset: usize = 0;
        let mut offset: usize = 0;
        // while (self.parser.state != .ground) {
        while self.parser.state != parser::State::Ground {
            // if (offset >= input.len) return input.len;
            if offset >= input.len() {
                return input.len();
            }

            // Fast path for CSI entry: "ESC [" is by far the most
            // common escape sequence prefix, so handle the '[' and
            // the byte that follows it here rather than paying a
            // nextNonUtf8 call for each.
            // if (self.parser.state == .escape and input[offset] == '[') {
            if self.parser.state == parser::State::Escape && input[offset] == b'[' {
                self.parser.state = parser::State::CsiEntry;
                offset += 1;
                continue;
            }

            // if (comptime !@hasDecl(T, "vtRaw")) {
            if !H::HAS_VT_RAW {
                // if (self.parser.state == .csi_entry) {
                //     if (self.csiEntryByte(input[offset])) { offset += 1; continue; }
                // }
                if self.parser.state == parser::State::CsiEntry
                    && self.csi_entry_byte(input[offset])
                {
                    offset += 1;
                    continue;
                }

                // Bulk-consume CSI parameter bytes. This can't be
                // used for handlers with a vtRaw hook because it
                // dispatches the CSI directly (see nextNonUtf8).
                // if (self.parser.state == .csi_param) {
                if self.parser.state == parser::State::CsiParam {
                    // offset += self.consumeCsiParams(input[offset..]);
                    offset += self.consume_csi_params(&input[offset..]);
                    // if (offset >= input.len) return input.len;
                    if offset >= input.len() {
                        return input.len();
                    }
                    // If we're still in csi_param then the next byte
                    // isn't a parameter byte; let nextNonUtf8 below
                    // handle it. Otherwise re-check our state.
                    // if (self.parser.state != .csi_param) continue;
                    if self.parser.state != parser::State::CsiParam {
                        continue;
                    }
                }

                // Bulk-consume APC string bytes into a single slice.
                // APC payloads (e.g. Kitty graphics) can be megabytes
                // of base64 data, so per-byte dispatch is far too slow.
                // This can't be used for handlers with a vtRaw hook
                // because it dispatches the slice directly.
                // if (self.parser.state == .sos_pm_apc_string) {
                if self.parser.state == parser::State::SosPmApcString {
                    // offset += self.consumeApcString(input[offset..]);
                    offset += self.consume_apc_string(&input[offset..]);
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
                            self.parser.clear();
                            self.parser.state = parser::State::Escape;
                            // self.handler.vt(.apc_end, .{ .terminated = true });
                            self.handler.apc_end(ApcEnd { terminated: true });
                            offset += 1;
                            continue;
                        }
                        0x9C => {
                            self.parser.state = parser::State::Ground;
                            // self.handler.vt(.apc_end, .{ .terminated = true });
                            self.handler.apc_end(ApcEnd { terminated: true });
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
                if self.parser.state == parser::State::OscString {
                    // offset += self.consumeOscString(input[offset..]);
                    offset += self.consume_osc_string(&input[offset..]);
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
                            // PORT NOTE: `cmd.*` copies the command out of the
                            // parser — mirrored as the same clone the scalar
                            // path's OscDispatch exit action performs.
                            let cmd = self
                                .parser
                                .osc_parser
                                .end(Some(0x1B))
                                .map(|cmd| cmd.clone());
                            if let Some(cmd) = cmd {
                                self.osc_dispatch(cmd);
                            }
                            self.parser.clear();
                            self.parser.state = parser::State::Escape;
                            offset += 1;
                            continue;
                        }
                        // BEL terminates the string directly.
                        // std.ascii.control_code.bel => { ... }
                        0x07 => {
                            let cmd = self
                                .parser
                                .osc_parser
                                .end(Some(0x07))
                                .map(|cmd| cmd.clone());
                            if let Some(cmd) = cmd {
                                self.osc_dispatch(cmd);
                            }
                            self.parser.state = parser::State::Ground;
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
            self.next_non_utf8(input[offset]);
            // offset += 1;
            offset += 1;
        }
        // return offset;
        offset
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
    fn consume_apc_string(&mut self, input: &[u8]) -> usize {
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
        assert!(self.parser.state == parser::State::SosPmApcString);

        // var end: usize = 0;
        let mut end: usize = 0;

        // `8c523ed03`: scan ordinary APC bytes a vector at a time, keeping the
        // scalar loop below as the tail and as the whole scan on targets with
        // no recommended vector width.
        // if (comptime std.simd.suggestVectorLength(u8)) |vector_len| { ... }
        if let Some(vector_len) = APC_SCAN_LANES {
            while end + vector_len <= input.len() {
                // const invalid = (bytes == 0x18) | (bytes == 0x1A)
                //               | (bytes == 0x1B) | (bytes >= 0x80);
                // if (@reduce(.Or, invalid)) break;
                let mut invalid = false;
                for j in 0..vector_len {
                    let b = input[end + j];
                    invalid |= b == 0x18 || b == 0x1A || b == 0x1B || b >= 0x80;
                }
                if invalid {
                    break;
                }
                end += vector_len;
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
            self.handler.apc_put_slice(&input[0..end]);
        }
        // return end;
        end
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
    fn csi_entry_byte(&mut self, c: u8) -> bool {
        // The reference's `comptime assert(!@hasDecl(T, "vtRaw"))` is a
        // compile-time precondition on the caller. Rust cannot assert an
        // associated const inside a generic fn body without a const-generic
        // trick that would obscure the port, so it is a debug assertion — and
        // `stream_fast_paths_bypassed_when_vt_raw_hooked` is the real guard.
        debug_assert!(!H::HAS_VT_RAW);
        // assert(self.parser.state == .csi_entry);
        debug_assert!(self.parser.state == parser::State::CsiEntry);
        // switch (c) {
        match c {
            // First parameter digit.
            b'0'..=b'9' => {
                self.parser.state = parser::State::CsiParam;
                // param_acc is zero (cleared on escape entry)
                // so accumulating is just the digit value.
                self.parser.param_acc = u16::from(c - b'0');
                self.parser.param_acc_idx = 1;
            }
            // An empty first parameter.
            b';' => {
                self.parser.state = parser::State::CsiParam;
                self.parser.params[0] = 0;
                self.parser.params_idx = 1;
            }
            // Private marker (e.g. '?' in "ESC [ ? 2004 h").
            0x3C..=0x3F => {
                self.parser.state = parser::State::CsiParam;
                self.parser.collect(c);
            }
            // A final byte: a parameterless CSI.
            0x40..=0x7E => self.csi_dispatch_final(c),
            // Defer to the state machine for anything else
            // (C0 controls, intermediates, colon).
            _ => return false,
        }
        // return true;
        true
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
    fn consume_csi_params(&mut self, input: &[u8]) -> usize {
        // const p = &self.parser;
        // assert(p.state == .csi_param);
        debug_assert!(self.parser.state == parser::State::CsiParam);

        // Accumulate parser state in locals for the hot loop.
        // var acc = p.param_acc;
        // var acc_idx = p.param_acc_idx;
        // var idx = p.params_idx;
        let mut acc = self.parser.param_acc;
        let mut acc_idx = self.parser.param_acc_idx;
        let mut idx = self.parser.params_idx;

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
                        self.parser.params[idx as usize] = acc;
                        // if (c == ':') p.params_sep.set(idx);
                        if c == b':' {
                            self.parser.params_sep.set(idx as usize);
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
                    self.parser.param_acc = acc;
                    self.parser.param_acc_idx = acc_idx;
                    self.parser.params_idx = idx;
                    // self.csiDispatchFinal(c);
                    self.csi_dispatch_final(c);
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
        self.parser.param_acc = acc;
        self.parser.param_acc_idx = acc_idx;
        self.parser.params_idx = idx;
        offset
    }

    /// Like nextSlice but takes one byte and is necessarily a scalar
    /// operation that can't use SIMD. Prefer nextSlice if you can and
    /// try to get multiple bytes at once.
    // pub inline fn next(self: *Self, c: u8) void {
    pub fn next(&mut self, c: u8) {
        // self.nextUntracked(c);
        self.next_untracked(c);
        // if (self.continuation != null) self.trackContinuation(&.{c});
        if self.continuation.is_some() {
            self.track_continuation(&[c]);
        }
    }

    // inline fn nextUntracked(self: *Self, c: u8) void {
    fn next_untracked(&mut self, c: u8) {
        // The scalar path can be responsible for decoding UTF-8.
        // if (self.parser.state == .ground) { self.nextUtf8(c); return; }
        if self.parser.state == parser::State::Ground {
            self.next_utf8(c);
            return;
        }

        // self.nextNonUtf8(c);
        self.next_non_utf8(c);
    }

    /// Process the next byte and print as necessary.
    ///
    /// This assumes we're in the UTF-8 decoding state. If we may not
    /// be in the UTF-8 decoding state call nextSlice or next.
    // inline fn nextUtf8(self: *Self, c: u8) void { ... }
    fn next_utf8(&mut self, c: u8) {
        // assert(self.parser.state == .ground);
        debug_assert!(self.parser.state == parser::State::Ground);

        // const res = self.utf8decoder.next(c);
        let res = self.utf8decoder.next(c);
        // const consumed = res[1];
        let consumed = res.1;
        // if (res[0]) |codepoint| { self.handleCodepoint(codepoint); }
        if let Some(codepoint) = res.0 {
            self.handle_codepoint(codepoint);
        }
        // if (!consumed) { ... }
        if !consumed {
            // We optimize for the scenario where the text being
            // printed in the terminal ISN'T full of ill-formed
            // UTF-8 sequences.
            // @branchHint(.unlikely);

            // const retry = self.utf8decoder.next(c);
            let retry = self.utf8decoder.next(c);
            // It should be impossible for the decoder
            // to not consume the byte twice in a row.
            // assert(retry[1] == true);
            debug_assert!(retry.1);
            // if (retry[0]) |codepoint| { self.handleCodepoint(codepoint); }
            if let Some(codepoint) = retry.0 {
                self.handle_codepoint(codepoint);
            }
        }
    }

    /// To be called whenever the utf-8 decoder produces a codepoint.
    ///
    /// This function is abstracted this way to handle the case where
    /// the decoder emits a 0x1B after rejecting an ill-formed sequence.
    // inline fn handleCodepoint(self: *Self, c: u21) void { ... }
    fn handle_codepoint(&mut self, c: u32) {
        // We need to increase the eval branch limit because a lot of
        // tests end up running almost completely at comptime due to
        // a chain of inline functions.
        // @setEvalBranchQuota(100_000);

        // C0 control
        // if (c <= 0xF) { self.execute(@intCast(c)); return; }
        if c <= 0xF {
            self.execute(c as u8);
            return;
        }
        // ESC
        // if (c == 0x1B) { self.parser.state = .escape; self.parser.clear(); return; }
        if c == 0x1B {
            self.parser.state = parser::State::Escape;
            self.parser.clear();
            return;
        }
        // self.print(@intCast(c));
        self.print(c);
    }

    /// Process the next character and call any callbacks if necessary.
    ///
    /// This assumes that we're not in the UTF-8 decoding state. If
    /// we may be in the UTF-8 decoding state call nextSlice or next.
    // fn nextNonUtf8(self: *Self, c: u8) void { ... }
    fn next_non_utf8(&mut self, c: u8) {
        // assert(self.parser.state != .ground);
        debug_assert!(self.parser.state != parser::State::Ground);

        // Fast path for CSI entry.
        // if (self.parser.state == .escape and c == '[') { self.parser.state = .csi_entry; return; }
        if self.parser.state == parser::State::Escape && c == b'[' {
            self.parser.state = parser::State::CsiEntry;
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
        if self.parser.state == parser::State::CsiParam {
            // csi_param is the most common parser state
            // other than ground by a fairly wide margin.
            'csi_param: {
                match c {
                    // A C0 escape (yes, this is valid):
                    0x00..=0x0F => self.execute(c),
                    // We ignore C0 escapes > 0xF since execute
                    // doesn't have processing for them anyway:
                    0x10..=0x17 | 0x19 | 0x1C..=0x1F => {}
                    // We don't currently have any handling for
                    // 0x18 or 0x1A, but they should still move
                    // the parser state to ground.
                    0x18 | 0x1A => self.parser.state = parser::State::Ground,
                    // A parameter digit:
                    b'0'..=b'9' => {
                        if (self.parser.params_idx as usize) < parser::MAX_PARAMS {
                            self.parser.param_acc = self.parser.param_acc.saturating_mul(10);
                            self.parser.param_acc =
                                self.parser.param_acc.saturating_add((c - b'0') as u16);
                            // The parser's CSI param action uses param_acc_idx
                            // to decide if there's a final param that needs to
                            // be consumed or not, but it doesn't matter really
                            // what it is as long as it's not 0.
                            self.parser.param_acc_idx |= 1;
                        }
                    }
                    // A parameter separator:
                    b':' | b';' => {
                        if (self.parser.params_idx as usize) < parser::MAX_PARAMS {
                            self.parser.params[self.parser.params_idx as usize] =
                                self.parser.param_acc;
                            if c == b':' {
                                self.parser.params_sep.set(self.parser.params_idx as usize);
                            }
                            self.parser.params_idx += 1;

                            self.parser.param_acc = 0;
                            self.parser.param_acc_idx = 0;
                        }
                    }
                    // A final byte: dispatch the CSI directly.
                    // 0x40...0x7E => if (comptime !has_vt_raw) {
                    //     self.csiDispatchFinal(c);
                    // } else break :csi_param,
                    0x40..=0x7E => {
                        if !has_vt_raw {
                            self.csi_dispatch_final(c);
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
        if !has_vt_raw && self.parser.state == parser::State::CsiEntry && self.csi_entry_byte(c) {
            return;
        }

        // We explicitly inline this call here for performance reasons.
        //
        // We do this rather than mark Parser.next as inline because doing
        // that causes weird behavior in some tests- I'm not sure if they
        // miscompile or it's just very counter-intuitive comptime stuff,
        // but regardless, this is the easy solution.
        // const actions = @call(.always_inline, Parser.next, .{ &self.parser, c });
        let actions = self.parser.next(c);

        // for (actions) |action_opt| { ... }
        for action_opt in actions {
            // const action = action_opt orelse continue;
            let action = match action_opt {
                Some(a) => a,
                None => continue,
            };
            // if (comptime debug) log.info("action: {f}", .{action});

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
            if self.handler.vt_raw(&action) {
                continue;
            }

            // switch (action) { ... }
            match action {
                parser::Action::Print(p) => self.print(p),
                parser::Action::Execute(code) => self.execute(code),
                // PERF(port): `*` unboxes the pointer-sized Action payloads
                // (see the Action PERF note); the by-value handler signatures
                // are unchanged.
                parser::Action::CsiDispatch(csi_action) => self.csi_dispatch(*csi_action),
                parser::Action::EscDispatch(esc) => self.esc_dispatch(*esc),
                parser::Action::OscDispatch(cmd) => self.osc_dispatch(*cmd),
                parser::Action::DcsHook(dcs) => self.handler.dcs_hook(*dcs),
                parser::Action::DcsPut(code) => self.handler.dcs_put(code),
                parser::Action::DcsUnhook => self.handler.dcs_unhook(),
                parser::Action::ApcStart => self.handler.apc_start(),
                parser::Action::ApcPut(code) => self.handler.apc_put(code),
                // .apc_end => self.handler.vt(.apc_end, .{
                //     .terminated = c == std.ascii.control_code.esc or c == 0x9C,
                // }),
                parser::Action::ApcEnd => self.handler.apc_end(ApcEnd {
                    terminated: c == 0x1B || c == 0x9C,
                }),
            }
        }
    }

    /// Finalize and dispatch a CSI directly from parser state for
    /// the fast paths in nextNonUtf8, without going through
    /// Parser.next. This must match the behavior of the parser's
    /// csi_dispatch action.
    // fn csiDispatchFinal(self: *Self, c: u8) void {
    fn csi_dispatch_final(&mut self, c: u8) {
        // const p = &self.parser;
        // p.state = .ground;
        let p = &mut self.parser;
        p.state = parser::State::Ground;

        // Ignore sequences with too many parameters, matching the
        // parser's behavior of dropping the dispatch entirely.
        // if (p.params_idx >= Parser.MAX_PARAMS) { @branchHint(.unlikely); return; }
        if p.params_idx as usize >= parser::MAX_PARAMS {
            return;
        }

        // Finalize the last parameter if we have one.
        // if (p.param_acc_idx > 0) { p.params[p.params_idx] = p.param_acc; p.params_idx += 1; }
        if p.param_acc_idx > 0 {
            p.params[p.params_idx as usize] = p.param_acc;
            p.params_idx += 1;
        }

        // const action: Parser.Action.CSI = .{
        //     .intermediates = p.intermediates[0..p.intermediates_idx],
        //     .params = p.params[0..p.params_idx],
        //     .params_sep = p.params_sep,
        //     .final = c,
        // };
        //
        // PORT NOTE: the reference's `action` slices into the parser's buffers;
        // rustty's `parser::CSI` owns `InlineVec` prefixes (the `Action` PORT
        // NOTE), so this is the same `InlineVec::from_buf` construction the
        // parser's own `csi_dispatch` arm performs (`parser.rs:807`) — and
        // deliberately so: any divergence between the two would be a
        // fast-path-only behaviour difference, which is precisely what
        // `stream_csi_bulk_params_match_per_byte` exists to catch.
        let action = parser::CSI {
            intermediates: parser::InlineVec::from_buf(p.intermediates, p.intermediates_idx),
            params: parser::InlineVec::from_buf(p.params, p.params_idx),
            params_sep: p.params_sep,
            final_: c,
        };

        // We only allow colon or mixed separators for the 'm' command.
        // if (c != 'm' and p.params_sep.count() > 0) {
        //     @branchHint(.cold);
        //     log.warn(
        //         "CSI colon or mixed separators only allowed for 'm' command, got: {f}",
        //         .{action},
        //     );
        //     return;
        // }
        if c != b'm' && p.params_sep.count() > 0 {
            // log.warn("CSI colon or mixed separators only allowed for 'm' command, got: {f}", ...);
            return;
        }

        // if (comptime debug) log.info("action: {f}", .{Parser.Action{ .csi_dispatch = action }});
        // self.csiDispatch(action);
        self.csi_dispatch(action);
    }

    // inline fn print(self: *Self, c: u21) void {
    fn print(&mut self, c: u32) {
        // self.handler.vt(.print, .{ .cp = c });
        self.handler.print(c);
    }

    // inline fn execute(self: *Self, c: u8) void { ... }
    fn execute(&mut self, c: u8) {
        // If the character is > 0x7F, it's a C1 (8-bit) control,
        // which is strictly equivalent to `ESC` plus `c - 0x40`.
        // if (c > 0x7F) { ... self.escDispatch(.{ .intermediates = &.{}, .final = c - 0x40 }); return; }
        if c > 0x7F {
            self.esc_dispatch(parser::ESC {
                intermediates: parser::InlineVec::new(),
                final_: c - 0x40,
            });
            return;
        }

        // const c0: ansi.C0 = @enumFromInt(c);
        // if (comptime debug) log.info("execute: {f}", .{c0});
        let c0 = ansi::C0::from_int(c);
        // switch (c0) { ... }
        match c0 {
            // We ignore SOH/STX: https://github.com/microsoft/terminal/issues/10786
            ansi::C0::NUL | ansi::C0::SOH | ansi::C0::STX => {}

            ansi::C0::ENQ => self.handler.enquiry(),
            ansi::C0::BEL => self.handler.bell(),
            ansi::C0::BS => self.handler.backspace(),
            ansi::C0::HT => self.handler.horizontal_tab(1),
            ansi::C0::LF | ansi::C0::VT | ansi::C0::FF => self.handler.linefeed(),
            ansi::C0::CR => self.handler.carriage_return(),
            ansi::C0::SO => self.handler.invoke_charset(InvokeCharset {
                bank: charsets::ActiveSlot::GL,
                charset: charsets::Slots::G1,
                locking: false,
            }),
            ansi::C0::SI => self.handler.invoke_charset(InvokeCharset {
                bank: charsets::ActiveSlot::GL,
                charset: charsets::Slots::G0,
                locking: false,
            }),

            // else => logUnsupportedOnce("invalid C0 character, ignoring: 0x{x}", .{c}, c),
            _ => {
                log_unsupported_once(&SEEN_INVALID_C0, u16::from(c));
            }
        }
    }

    // inline fn csiDispatch(self: *Self, input: Parser.Action.CSI) void { ... }
    fn csi_dispatch(&mut self, input: parser::CSI) {
        // The branch hints here are based on real world data
        // which indicates that the most common CSI finals are:
        //
        // 1. m
        // 2. H
        // 3. K
        // 4. A
        // 5. C
        // 6. X
        // 7. l
        // 8. h
        // 9. r
        //
        // Together, these 9 finals make up about 96% of all
        // CSI sequences encountered in real world scenarios.
        //
        // Additionally, within the prongs, unlikely branch
        // hints have been added to branches that deal with
        // invalid sequences/commands, this is in order to
        // optimize for the happy path where we're getting
        // valid data from the program we're running.
        //
        // ref: https://github.com/qwerasd205/asciinema-stats

        // switch (input.final) {
        match input.final_ {
            // CUU - Cursor Up
            // 'A', 'k' => { ... }
            b'A' | b'k' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    // 0 => self.handler.vt(.cursor_up, .{ .value = ... }),
                    0 => self.handler.cursor_up(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor up command: {f}", .{input});
                                return;
                            }
                        },
                    }),

                    // else => log.warn("ignoring unimplemented CSI A with intermediates: {s}", ...),
                    _ => {}
                }
            }

            // CUD - Cursor Down
            // 'B' => switch (input.intermediates.len) { ... }
            b'B' => match input.intermediates.len() {
                0 => self.handler.cursor_down(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid cursor down command: {f}", .{input});
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI B with intermediates: {s}", ...),
                _ => {}
            },

            // CUF - Cursor Right
            // 'C' => { ... }
            b'C' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => self.handler.cursor_right(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor right command: {f}", .{input});
                                return;
                            }
                        },
                    }),

                    // else => log.warn("ignoring unimplemented CSI C with intermediates: {s}", ...),
                    _ => {}
                }
            }

            // CUB - Cursor Left
            // 'D', 'j' => switch (input.intermediates.len) { ... }
            b'D' | b'j' => match input.intermediates.len() {
                0 => self.handler.cursor_left(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid cursor left command: {f}", .{input});
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI D with intermediates: {s}", ...),
                _ => {}
            },

            // CNL - Cursor Next Line
            // 'E' => switch (input.intermediates.len) { ... }
            b'E' => match input.intermediates.len() {
                0 => {
                    self.handler.cursor_down(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor up command: {f}", .{input});
                                return;
                            }
                        },
                    });
                    self.handler.carriage_return();
                }

                // else => log.warn("ignoring unimplemented CSI E with intermediates: {s}", ...),
                _ => {}
            },

            // CPL - Cursor Previous Line
            // 'F' => switch (input.intermediates.len) { ... }
            b'F' => match input.intermediates.len() {
                0 => {
                    self.handler.cursor_up(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor down command: {f}", .{input});
                                return;
                            }
                        },
                    });
                    self.handler.carriage_return();
                }

                // else => log.warn("ignoring unimplemented CSI F with intermediates: {s}", ...),
                _ => {}
            },

            // HPA - Cursor Horizontal Position Absolute
            // TODO: test
            // 'G', '`' => switch (input.intermediates.len) { ... }
            b'G' | b'`' => match input.intermediates.len() {
                0 => self.handler.cursor_col(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid HPA command: {f}", .{input});
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI G with intermediates: {s}", ...),
                _ => {}
            },

            // CUP - Set Cursor Position.
            // TODO: test
            // 'H', 'f' => { ... }
            b'H' | b'f' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => {
                        // const pos: streampkg.Action.CursorPos = switch (input.params.len) {
                        let pos: CursorPos = match input.params.len() {
                            0 => CursorPos { row: 1, col: 1 },
                            1 => CursorPos {
                                row: input.params[0],
                                col: 1,
                            },
                            2 => CursorPos {
                                row: input.params[0],
                                col: input.params[1],
                            },
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid CUP command: {f}", .{input});
                                return;
                            }
                        };
                        self.handler.cursor_pos(pos);
                    }

                    // else => log.warn("ignoring unimplemented CSI H with intermediates: {s}", ...),
                    _ => {}
                }
            }

            // CHT - Cursor Horizontal Tabulation
            // 'I' => switch (input.intermediates.len) { ... }
            b'I' => match input.intermediates.len() {
                0 => self.handler.horizontal_tab(match input.params.len() {
                    0 => 1,
                    1 => input.params[0],
                    _ => {
                        // log.warn("invalid horizontal tab command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI I with intermediates: {s}", ...),
                _ => {}
            },

            // Erase Display
            // 'J' => { ... }
            b'J' => {
                // const protected_: ?bool = switch (input.intermediates.len) {
                let protected_: Option<bool> = match input.intermediates.len() {
                    0 => Some(false),
                    1 => {
                        if input.intermediates[0] == b'?' {
                            Some(true)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // const protected = protected_ orelse { ... };
                let protected = match protected_ {
                    Some(p) => p,
                    None => {
                        // log.warn("invalid erase display command: {f}", .{input});
                        return;
                    }
                };

                // const mode_: ?csi.EraseDisplay = switch (input.params.len) {
                let mode_: Option<csi::EraseDisplay> = match input.params.len() {
                    0 => Some(csi::EraseDisplay::Below),
                    // 1 => std.meta.intToEnum(csi.EraseDisplay, input.params[0]) catch null,
                    1 => match input.params[0] {
                        0 => Some(csi::EraseDisplay::Below),
                        1 => Some(csi::EraseDisplay::Above),
                        2 => Some(csi::EraseDisplay::Complete),
                        3 => Some(csi::EraseDisplay::Scrollback),
                        22 => Some(csi::EraseDisplay::ScrollComplete),
                        _ => None,
                    },
                    _ => None,
                };

                // const mode = mode_ orelse { ... };
                let mode = match mode_ {
                    Some(m) => m,
                    None => {
                        // log.warn("invalid erase display command: {f}", .{input});
                        return;
                    }
                };

                // switch (mode) {
                match mode {
                    csi::EraseDisplay::Below => self.handler.erase_display_below(protected),
                    csi::EraseDisplay::Above => self.handler.erase_display_above(protected),
                    csi::EraseDisplay::Complete => self.handler.erase_display_complete(protected),
                    csi::EraseDisplay::Scrollback => {
                        self.handler.erase_display_scrollback(protected)
                    }
                    csi::EraseDisplay::ScrollComplete => {
                        self.handler.erase_display_scroll_complete(protected)
                    }
                }
            }

            // Erase Line
            // 'K' => { ... }
            b'K' => {
                // @branchHint(.likely);
                // const protected_: ?bool = switch (input.intermediates.len) {
                let protected_: Option<bool> = match input.intermediates.len() {
                    0 => Some(false),
                    1 => {
                        if input.intermediates[0] == b'?' {
                            Some(true)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // const protected = protected_ orelse { ... };
                let protected = match protected_ {
                    Some(p) => p,
                    None => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid erase line command: {f}", .{input});
                        return;
                    }
                };

                // const mode_: ?csi.EraseLine = switch (input.params.len) {
                let mode_: Option<csi::EraseLine> = match input.params.len() {
                    0 => Some(csi::EraseLine::Right),
                    // 1 => if (input.params[0] < 3) @enumFromInt(input.params[0]) else null,
                    1 => {
                        if input.params[0] < 3 {
                            Some(match input.params[0] {
                                0 => csi::EraseLine::Right,
                                1 => csi::EraseLine::Left,
                                2 => csi::EraseLine::Complete,
                                _ => csi::EraseLine::Other(input.params[0] as u8),
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // const mode = mode_ orelse { ... };
                let mode = match mode_ {
                    Some(m) => m,
                    None => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid erase line command: {f}", .{input});
                        return;
                    }
                };

                // switch (mode) {
                match mode {
                    csi::EraseLine::Right => self.handler.erase_line_right(protected),
                    csi::EraseLine::Left => self.handler.erase_line_left(protected),
                    csi::EraseLine::Complete => self.handler.erase_line_complete(protected),
                    csi::EraseLine::RightUnlessPendingWrap => {
                        self.handler.erase_line_right_unless_pending_wrap(protected)
                    }
                    // _ => { ... }
                    csi::EraseLine::Other(_) => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid erase line mode: {}", .{mode});
                    }
                }
            }

            // IL - Insert Lines
            // TODO: test
            // 'L' => switch (input.intermediates.len) { ... }
            b'L' => match input.intermediates.len() {
                0 => self.handler.insert_lines(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid IL command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI L with intermediates: {s}", ...),
                _ => {}
            },

            // DL - Delete Lines
            // TODO: test
            // 'M' => switch (input.intermediates.len) { ... }
            b'M' => match input.intermediates.len() {
                0 => self.handler.delete_lines(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid DL command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI M with intermediates: {s}", ...),
                _ => {}
            },

            // Delete Character (DCH)
            // 'P' => switch (input.intermediates.len) { ... }
            b'P' => match input.intermediates.len() {
                0 => self.handler.delete_chars(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid delete characters command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI P with intermediates: {s}", ...),
                _ => {}
            },

            // Scroll Up (SD)
            // 'S' => switch (input.intermediates.len) { ... }
            b'S' => match input.intermediates.len() {
                0 => self.handler.scroll_up(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid scroll up command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI S with intermediates: {s}", ...),
                _ => {}
            },

            // Scroll Down (SD)
            // 'T' => switch (input.intermediates.len) { ... }
            b'T' => match input.intermediates.len() {
                0 => self.handler.scroll_down(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid scroll down command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI T with intermediates: {s}", ...),
                _ => {}
            },

            // Cursor Tabulation Control
            // 'W' => switch (input.intermediates.len) { ... }
            b'W' => match input.intermediates.len() {
                0 => {
                    // if (input.params.len == 0 or
                    //     (input.params.len == 1 and input.params[0] == 0))
                    if input.params.len() == 0 || (input.params.len() == 1 && input.params[0] == 0)
                    {
                        self.handler.tab_set();
                        return;
                    }

                    // switch (input.params.len) {
                    match input.params.len() {
                        // 0 => unreachable,
                        0 => unreachable!(),

                        // 1 => switch (input.params[0]) {
                        1 => match input.params[0] {
                            // 0 => unreachable,
                            0 => unreachable!(),

                            2 => self.handler.tab_clear_current(),

                            5 => self.handler.tab_clear_all(),

                            _ => {}
                        },

                        _ => {}
                    }

                    // log.warn("invalid cursor tabulation control: {f}", .{input});
                    #[allow(clippy::needless_return)]
                    return;
                }

                // 1 => if (...) { self.handler.vt(.tab_reset, {}); } else log.warn(...),
                1 => {
                    if input.intermediates[0] == b'?'
                        && input.params.len() == 1
                        && input.params[0] == 5
                    {
                        self.handler.tab_reset();
                    }
                    // else log.warn("invalid cursor tabulation control: {f}", .{input});
                }

                // else => log.warn("ignoring unimplemented CSI W with intermediates: {s}", ...),
                _ => {}
            },

            // Erase Characters (ECH)
            // 'X' => { ... }
            b'X' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => self.handler.erase_chars(match input.params.len() {
                        0 => 1,
                        1 => input.params[0] as usize,
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid erase characters command: {f}", .{input});
                            return;
                        }
                    }),

                    // else => log.warn("ignoring unimplemented CSI X with intermediates: {s}", ...),
                    _ => {}
                }
            }

            // CHT - Cursor Horizontal Tabulation Back
            // 'Z' => switch (input.intermediates.len) { ... }
            b'Z' => match input.intermediates.len() {
                0 => self.handler.horizontal_tab_back(match input.params.len() {
                    0 => 1,
                    1 => input.params[0],
                    _ => {
                        // log.warn("invalid horizontal tab back command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI Z with intermediates: {s}", ...),
                _ => {}
            },

            // HPR - Cursor Horizontal Position Relative
            // 'a' => switch (input.intermediates.len) { ... }
            b'a' => match input.intermediates.len() {
                0 => self.handler.cursor_col_relative(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // log.warn("invalid HPR command: {f}", .{input});
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI a with intermediates: {s}", ...),
                _ => {}
            },

            // Repeat Previous Char (REP)
            // 'b' => switch (input.intermediates.len) { ... }
            b'b' => match input.intermediates.len() {
                0 => self.handler.print_repeat(match input.params.len() {
                    0 => 1,
                    // (u16 param widens into the reference's `usize` payload.)
                    1 => usize::from(input.params[0]),
                    _ => {
                        // log.warn("invalid print repeat command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI b with intermediates: {s}", ...),
                _ => {}
            },

            // c - Device Attributes (DA1)
            // 'c' => { ... }
            b'c' => {
                // const req: ?device_attributes.Req = switch (input.intermediates.len) {
                let req: Option<device_attributes::Req> = match input.intermediates.len() {
                    0 => Some(device_attributes::Req::Primary),
                    1 => match input.intermediates[0] {
                        b'>' => Some(device_attributes::Req::Secondary),
                        b'=' => Some(device_attributes::Req::Tertiary),
                        _ => None,
                    },
                    _ => None,
                };

                // if (req) |r| { ... } else { ... }
                if let Some(r) = req {
                    self.handler.device_attributes(r);
                } else {
                    // logUnsupportedOnce(
                    //     "invalid device attributes command: {f}",
                    //     .{input},
                    //     if (input.params.len > 0) input.params[0] else 0,
                    // );
                    log_unsupported_once(
                        &SEEN_INVALID_DEVICE_ATTRIBUTES,
                        if input.params.len() > 0 {
                            input.params[0]
                        } else {
                            0
                        },
                    );
                    #[allow(clippy::needless_return)]
                    return;
                }
            }

            // VPA - Cursor Vertical Position Absolute
            // 'd' => switch (input.intermediates.len) { ... }
            b'd' => match input.intermediates.len() {
                0 => self.handler.cursor_row(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // log.warn("invalid VPA command: {f}", .{input});
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI d with intermediates: {s}", ...),
                _ => {}
            },

            // VPR - Cursor Vertical Position Relative
            // 'e' => switch (input.intermediates.len) { ... }
            b'e' => match input.intermediates.len() {
                0 => self.handler.cursor_row_relative(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // log.warn("invalid VPR command: {f}", .{input});
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI e with intermediates: {s}", ...),
                _ => {}
            },

            // TBC - Tab Clear
            // TODO: test
            // 'g' => switch (input.intermediates.len) { ... }
            b'g' => match input.intermediates.len() {
                0 => {
                    // const mode: csi.TabClear = switch (input.params.len) {
                    let mode: csi::TabClear = match input.params.len() {
                        // 1 => std.meta.intToEnum(csi.TabClear, input.params[0]) catch { ... },
                        1 => match u8::try_from(input.params[0]) {
                            Ok(v) => match v {
                                0 => csi::TabClear::Current,
                                3 => csi::TabClear::All,
                                _ => csi::TabClear::Other(v),
                            },
                            Err(_) => {
                                // log.warn("invalid tab clear mode: {}", .{input.params[0]});
                                return;
                            }
                        },
                        _ => {
                            // log.warn("invalid tab clear command: {f}", .{input});
                            return;
                        }
                    };
                    // switch (mode) {
                    match mode {
                        csi::TabClear::Current => self.handler.tab_clear_current(),
                        csi::TabClear::All => self.handler.tab_clear_all(),
                        // _ => log.warn("unknown tab clear mode: {}", .{mode}),
                        csi::TabClear::Other(_) => {}
                    }
                }

                // else => log.warn("ignoring unimplemented CSI g with intermediates: {s}", ...),
                _ => {}
            },

            // SM - Set Mode
            // 'h' => mode: { ... }
            b'h' => 'mode: {
                // @branchHint(.likely);
                // const ansi_mode = ansi: { ... };
                let ansi_mode = 'ansi: {
                    if input.intermediates.len() == 0 {
                        break 'ansi true;
                    }
                    if input.intermediates.len() == 1 && input.intermediates[0] == b'?' {
                        break 'ansi false;
                    }

                    // log.warn("invalid set mode command: {f}", .{input});
                    break 'mode;
                };

                // for (input.params) |mode_int| {
                for &mode_int in input.params.iter() {
                    // if (modes.modeFromInt(mode_int, ansi_mode)) |mode| {
                    if let Some(mode) = modes::mode_from_int(mode_int, ansi_mode) {
                        self.handler.set_mode(Mode { mode });
                    } else {
                        // logUnsupportedOnce("unimplemented mode: {}", .{mode_int}, mode_int);
                        log_unsupported_once(&SEEN_UNIMPLEMENTED_MODE, mode_int);
                    }
                }
            }

            // RM - Reset Mode
            // 'l' => mode: { ... }
            b'l' => 'mode: {
                // @branchHint(.likely);
                // const ansi_mode = ansi: { ... };
                let ansi_mode = 'ansi: {
                    if input.intermediates.len() == 0 {
                        break 'ansi true;
                    }
                    if input.intermediates.len() == 1 && input.intermediates[0] == b'?' {
                        break 'ansi false;
                    }

                    // log.warn("invalid set mode command: {f}", .{input});
                    break 'mode;
                };

                // for (input.params) |mode_int| {
                for &mode_int in input.params.iter() {
                    // if (modes.modeFromInt(mode_int, ansi_mode)) |mode| {
                    if let Some(mode) = modes::mode_from_int(mode_int, ansi_mode) {
                        self.handler.reset_mode(Mode { mode });
                    } else {
                        // logUnsupportedOnce("unimplemented mode: {}", .{mode_int}, mode_int);
                        log_unsupported_once(&SEEN_UNIMPLEMENTED_MODE, mode_int);
                    }
                }
            }

            // SGR - Select Graphic Rendition
            // 'm' => { ... }
            b'm' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => {
                        // This is the most common case.
                        // @branchHint(.likely);
                        // log.info("parse SGR params={any}", .{input.params});
                        // var p: sgr.Parser = .{ .params = ..., .params_sep = ... };
                        // PORT NOTE: the reference passes the single `SepList` type
                        // straight through. In the port, `Parser.Action.CSI.SepList`
                        // (`parser::SepList`) and the `sgr` module's structurally
                        // identical local seam (`sgr::SepList`) are still distinct
                        // Rust types (see the PORT NOTE on `parser::SepList`); bridge
                        // them by copying the set separator bits through the public
                        // `is_set`/`set` API so the SGR parser observes the identical
                        // separator set.
                        let mut params_sep = sgr::SepList::init_empty();
                        for i in 0..parser::MAX_PARAMS {
                            if input.params_sep.is_set(i) {
                                params_sep.set(i);
                            }
                        }
                        let mut p: sgr::Parser = sgr::Parser {
                            params: &input.params,
                            params_sep,
                            idx: 0,
                        };
                        // while (p.next()) |attr| {
                        while let Some(attr) = p.next() {
                            // log.info("SGR attribute: {}", .{attr});
                            self.handler.set_attribute(attr);
                        }
                    }

                    // 1 => switch (input.intermediates[0]) {
                    1 => match input.intermediates[0] {
                        // '>' => blk: { ... }
                        b'>' => 'blk: {
                            // if (input.params.len == 0) { ... }
                            if input.params.len() == 0 {
                                // Reset
                                self.handler
                                    .modify_key_format(ansi::ModifyKeyFormat::Legacy);
                                break 'blk;
                            }

                            // var format: ansi.ModifyKeyFormat = switch (input.params[0]) {
                            let mut format: ansi::ModifyKeyFormat = match input.params[0] {
                                0 => ansi::ModifyKeyFormat::Legacy,
                                1 => ansi::ModifyKeyFormat::CursorKeys,
                                2 => ansi::ModifyKeyFormat::FunctionKeys,
                                4 => ansi::ModifyKeyFormat::OtherKeysNone,
                                _ => {
                                    // @branchHint(.unlikely);
                                    // log.warn("invalid setModifyKeyFormat: {f}", .{input});
                                    break 'blk;
                                }
                            };

                            // if (input.params.len > 2) { ... }
                            if input.params.len() > 2 {
                                // @branchHint(.unlikely);
                                // log.warn("invalid setModifyKeyFormat: {f}", .{input});
                                break 'blk;
                            }

                            // if (input.params.len == 2) {
                            if input.params.len() == 2 {
                                // switch (format) {
                                match format {
                                    // We don't support any of the subparams yet for these.
                                    ansi::ModifyKeyFormat::Legacy => {}
                                    ansi::ModifyKeyFormat::CursorKeys => {}
                                    ansi::ModifyKeyFormat::FunctionKeys => {}

                                    // We only support the numeric form.
                                    // .other_keys_none => switch (input.params[1]) {
                                    ansi::ModifyKeyFormat::OtherKeysNone => match input.params[1] {
                                        2 => format = ansi::ModifyKeyFormat::OtherKeysNumeric,
                                        _ => {}
                                    },
                                    ansi::ModifyKeyFormat::OtherKeysNumericExcept => {}
                                    ansi::ModifyKeyFormat::OtherKeysNumeric => {}
                                }
                            }

                            self.handler.modify_key_format(format);
                        }

                        // else => logUnsupportedOnce(
                        //     "unknown CSI m with intermediate: {}",
                        //     .{input.intermediates[0]},
                        //     input.intermediates[0],
                        // ),
                        _ => {
                            log_unsupported_once(
                                &SEEN_UNKNOWN_CSI_M_INTERMEDIATE,
                                u16::from(input.intermediates[0]),
                            );
                        }
                    },

                    _ => {
                        // Nothing, but I wanted a place to put this comment:
                        // there are others forms of CSI m that have intermediates.
                        // `vim --clean` uses `CSI ? 4 m` and I don't know what
                        // that means.
                        // log.warn("ignoring unimplemented CSI m with intermediates: {s}", ...);
                    }
                }
            }

            // TODO: test
            // 'n' => { ... }
            b'n' => {
                // Handle deviceStatusReport first
                // if (input.intermediates.len == 0 or input.intermediates[0] == '?')
                if input.intermediates.len() == 0 || input.intermediates[0] == b'?' {
                    // if (input.params.len != 1) { ... }
                    if input.params.len() != 1 {
                        // log.warn("invalid device status report command: {f}", .{input});
                        return;
                    }

                    // const question = question: { ... };
                    let question = 'question: {
                        if input.intermediates.len() == 0 {
                            break 'question false;
                        }
                        if input.intermediates.len() == 1 && input.intermediates[0] == b'?' {
                            break 'question true;
                        }

                        // log.warn("invalid set mode command: {f}", .{input});
                        return;
                    };

                    // const req = device_status.reqFromInt(input.params[0], question) orelse { ... };
                    let req = match device_status::req_from_int(input.params[0], question) {
                        Some(req) => req,
                        None => {
                            // log.warn("invalid device status report command: {f}", .{input});
                            return;
                        }
                    };

                    self.handler.device_status(DeviceStatus { request: req });
                    return;
                }

                // Handle other forms of CSI n
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    // 0 => unreachable, // handled above
                    0 => unreachable!(), // handled above

                    // 1 => switch (input.intermediates[0]) {
                    1 => match input.intermediates[0] {
                        b'>' => {
                            // This isn't strictly correct. CSI > n has parameters that
                            // control what exactly is being disabled. However, we
                            // only support reverting back to modify other keys in
                            // numeric except format.
                            self.handler
                                .modify_key_format(ansi::ModifyKeyFormat::OtherKeysNumericExcept);
                        }

                        // else => log.warn("unknown CSI n with intermediate: {}", ...),
                        _ => {}
                    },

                    // else => log.warn("ignoring unimplemented CSI n with intermediates: {s}", ...),
                    _ => {}
                }
            }

            // DECRQM - Request Mode
            // 'p' => switch (input.intermediates.len) { ... }
            b'p' => match input.intermediates.len() {
                // 2 => decrqm: { ... }
                2 => 'decrqm: {
                    // const ansi_mode = ansi: { ... };
                    let ansi_mode = 'ansi: {
                        // switch (input.intermediates.len) {
                        match input.intermediates.len() {
                            1 => {
                                if input.intermediates[0] == b'$' {
                                    break 'ansi true;
                                }
                            }
                            2 => {
                                if input.intermediates[0] == b'?' && input.intermediates[1] == b'$'
                                {
                                    break 'ansi false;
                                }
                            }
                            _ => {}
                        }

                        // log.warn("ignoring unimplemented CSI p with intermediates: {s}", ...);
                        break 'decrqm;
                    };

                    // if (input.params.len != 1) { ... }
                    if input.params.len() != 1 {
                        // log.warn("invalid DECRQM command: {f}", .{input});
                        break 'decrqm;
                    }

                    // const mode_raw = input.params[0];
                    let mode_raw = input.params[0];
                    // const mode = modes.modeFromInt(mode_raw, ansi_mode);
                    let mode = modes::mode_from_int(mode_raw, ansi_mode);
                    // if (mode) |m| { ... } else { ... }
                    if let Some(m) = mode {
                        self.handler.request_mode(Mode { mode: m });
                    } else {
                        self.handler.request_mode_unknown(RawMode {
                            mode: mode_raw,
                            ansi: ansi_mode,
                        });
                    }
                }

                // else => log.warn("ignoring unimplemented CSI p with intermediates: {s}", ...),
                _ => {}
            },

            // 'q' => switch (input.intermediates.len) { ... }
            b'q' => match input.intermediates.len() {
                // 1 => switch (input.intermediates[0]) {
                1 => match input.intermediates[0] {
                    // DECSCUSR - Select Cursor Style
                    // TODO: test
                    // ' ' => { ... }
                    b' ' => {
                        // const style: ansi.CursorStyle = switch (input.params.len) {
                        let style: ansi::CursorStyle = match input.params.len() {
                            0 => ansi::CursorStyle::Default,
                            // 1 => switch (input.params[0]) {
                            1 => match input.params[0] {
                                0 => ansi::CursorStyle::Default,
                                1 => ansi::CursorStyle::BlinkingBlock,
                                2 => ansi::CursorStyle::SteadyBlock,
                                3 => ansi::CursorStyle::BlinkingUnderline,
                                4 => ansi::CursorStyle::SteadyUnderline,
                                5 => ansi::CursorStyle::BlinkingBar,
                                6 => ansi::CursorStyle::SteadyBar,
                                _ => {
                                    // log.warn("invalid cursor style value: {}", .{input.params[0]});
                                    return;
                                }
                            },
                            _ => {
                                // log.warn("invalid set cursor style command: {f}", .{input});
                                return;
                            }
                        };
                        self.handler.cursor_style(style);
                    }

                    // DECSCA
                    // '"' => { ... }
                    b'"' => {
                        // const mode_: ?ansi.ProtectedMode = switch (input.params.len) {
                        let mode_: Option<ansi::ProtectedMode> = match input.params.len() {
                            0 => Some(ansi::ProtectedMode::Off),
                            // 1 => switch (input.params[0]) {
                            1 => match input.params[0] {
                                0 | 2 => Some(ansi::ProtectedMode::Off),
                                1 => Some(ansi::ProtectedMode::Dec),
                                _ => None,
                            },
                            _ => None,
                        };

                        // const mode = mode_ orelse { ... };
                        let mode = match mode_ {
                            Some(m) => m,
                            None => {
                                // log.warn("invalid set protected mode command: {f}", .{input});
                                return;
                            }
                        };

                        // switch (mode) {
                        match mode {
                            ansi::ProtectedMode::Off => self.handler.protected_mode_off(),
                            ansi::ProtectedMode::Iso => self.handler.protected_mode_iso(),
                            ansi::ProtectedMode::Dec => self.handler.protected_mode_dec(),
                        }
                    }

                    // XTVERSION
                    b'>' => self.handler.xtversion(),
                    _ => {
                        // log.warn("ignoring unimplemented CSI q with intermediates: {s}", ...);
                    }
                },

                // else => log.warn("ignoring unimplemented CSI p with intermediates: {s}", ...),
                _ => {}
            },

            // 'r' => { ... }
            b'r' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    // DECSTBM - Set Top and Bottom Margins
                    // 0 => switch (input.params.len) {
                    0 => match input.params.len() {
                        0 => self.handler.top_and_bottom_margin(Margin {
                            top_left: 0,
                            bottom_right: 0,
                        }),
                        1 => self.handler.top_and_bottom_margin(Margin {
                            top_left: input.params[0],
                            bottom_right: 0,
                        }),
                        2 => self.handler.top_and_bottom_margin(Margin {
                            top_left: input.params[0],
                            bottom_right: input.params[1],
                        }),
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid DECSTBM command: {f}", .{input});
                        }
                    },

                    // 1 => switch (input.intermediates[0]) {
                    1 => match input.intermediates[0] {
                        // Restore Mode
                        b'?' => {
                            // for (input.params) |mode_int| {
                            for &mode_int in input.params.iter() {
                                // if (modes.modeFromInt(mode_int, false)) |mode| {
                                if let Some(mode) = modes::mode_from_int(mode_int, false) {
                                    self.handler.restore_mode(Mode { mode });
                                } else {
                                    // log.warn("unimplemented restore mode: {}", .{mode_int});
                                }
                            }
                        }

                        // else => log.warn("unknown CSI s with intermediate: {f}", .{input}),
                        _ => {}
                    },

                    // else => log.warn("ignoring unimplemented CSI s with intermediates: {f}", ...),
                    _ => {}
                }
            }

            // 's' => switch (input.intermediates.len) { ... }
            b's' => match input.intermediates.len() {
                // DECSLRM
                // 0 => switch (input.params.len) {
                0 => match input.params.len() {
                    // CSI S is ambiguous with zero params so we defer
                    // to our handler to do the proper logic. If mode 69
                    // is set, then we should invoke DECSLRM, otherwise
                    // we should invoke SC.
                    0 => self.handler.left_and_right_margin_ambiguous(),
                    1 => self.handler.left_and_right_margin(Margin {
                        top_left: input.params[0],
                        bottom_right: 0,
                    }),
                    2 => self.handler.left_and_right_margin(Margin {
                        top_left: input.params[0],
                        bottom_right: input.params[1],
                    }),
                    // else => log.warn("invalid DECSLRM command: {f}", .{input}),
                    _ => {}
                },

                // 1 => switch (input.intermediates[0]) {
                1 => match input.intermediates[0] {
                    b'?' => {
                        // for (input.params) |mode_int| {
                        for &mode_int in input.params.iter() {
                            // if (modes.modeFromInt(mode_int, false)) |mode| {
                            if let Some(mode) = modes::mode_from_int(mode_int, false) {
                                self.handler.save_mode(Mode { mode });
                            } else {
                                // log.warn("unimplemented save mode: {}", .{mode_int});
                            }
                        }
                    }

                    // XTSHIFTESCAPE
                    // '>' => capture: { ... }
                    b'>' => 'capture: {
                        // const capture = switch (input.params.len) {
                        let capture = match input.params.len() {
                            0 => false,
                            // 1 => switch (input.params[0]) {
                            1 => match input.params[0] {
                                0 => false,
                                1 => true,
                                _ => {
                                    // log.warn("invalid XTSHIFTESCAPE command: {f}", .{input});
                                    break 'capture;
                                }
                            },
                            _ => {
                                // log.warn("invalid XTSHIFTESCAPE command: {f}", .{input});
                                break 'capture;
                            }
                        };

                        self.handler.mouse_shift_capture(capture);
                    }

                    // else => log.warn("unknown CSI s with intermediate: {f}", .{input}),
                    _ => {}
                },

                // else => log.warn("ignoring unimplemented CSI s with intermediates: {f}", ...),
                _ => {}
            },

            // XTWINOPS
            // 't' => switch (input.intermediates.len) { ... }
            b't' => match input.intermediates.len() {
                0 => {
                    // if (input.params.len > 0) {
                    if input.params.len() > 0 {
                        // switch (input.params[0]) {
                        match input.params[0] {
                            // 14 => if (input.params.len == 1) { ... } else log.warn(...),
                            14 => {
                                if input.params.len() == 1 {
                                    // report the text area size in pixels
                                    self.handler.size_report(csi::SizeReportStyle::Csi14T);
                                }
                                // else log.warn("ignoring CSI 14 t with extra parameters: {f}", ...),
                            }
                            // 16 => if (input.params.len == 1) { ... } else log.warn(...),
                            16 => {
                                if input.params.len() == 1 {
                                    // report cell size in pixels
                                    self.handler.size_report(csi::SizeReportStyle::Csi16T);
                                }
                                // else log.warn("ignoring CSI 16 t with extra parameters: {f}", ...),
                            }
                            // 18 => if (input.params.len == 1) { ... } else log.warn(...),
                            18 => {
                                if input.params.len() == 1 {
                                    // report screen size in characters
                                    self.handler.size_report(csi::SizeReportStyle::Csi18T);
                                }
                                // else log.warn("ignoring CSI 18 t with extra parameters: {f}", ...),
                            }
                            // 21 => if (input.params.len == 1) { ... } else log.warn(...),
                            21 => {
                                if input.params.len() == 1 {
                                    // report window title
                                    self.handler.size_report(csi::SizeReportStyle::Csi21T);
                                }
                                // else log.warn("ignoring CSI 21 t with extra parameters: {f}", ...),
                            }
                            // inline 22, 23 => |number| if (...) { ... } else log.warn(...),
                            number @ (22 | 23) => {
                                if (input.params.len() == 2 || input.params.len() == 3) &&
                                    // we only support window title
                                    (input.params[1] == 0 || input.params[1] == 2)
                                {
                                    // push/pop title
                                    // const index: u16 = if (input.params.len == 3) ... else 0;
                                    let index: u16 = if input.params.len() == 3 {
                                        input.params[2]
                                    } else {
                                        0
                                    };
                                    // switch (number) {
                                    match number {
                                        22 => self.handler.title_push(index),
                                        23 => self.handler.title_pop(index),
                                        // else => @compileError("unreachable"),
                                        _ => unreachable!(),
                                    }
                                }
                                // else logUnsupportedOnce(
                                //     "ignoring CSI 22/23 t with extra parameters: {f}",
                                //     .{input},
                                //     input.params[0],
                                // ),
                                else {
                                    log_unsupported_once(
                                        &SEEN_CSI_22_23_T_EXTRA_PARAMS,
                                        input.params[0],
                                    );
                                }
                            }
                            // else => logUnsupportedOnce(
                            //     "ignoring CSI t with unimplemented parameter: {f}",
                            //     .{input},
                            //     input.params[0],
                            // ),
                            _ => {
                                log_unsupported_once(
                                    &SEEN_CSI_T_UNIMPLEMENTED_PARAM,
                                    input.params[0],
                                );
                            }
                        }
                    }
                    // else log.err("ignoring CSI t with no parameters: {f}", .{input});
                }
                // else => log.warn("ignoring unimplemented CSI t with intermediates: {f}", ...),
                _ => {}
            },

            // 'u' => switch (input.intermediates.len) { ... }
            b'u' => match input.intermediates.len() {
                0 => self.handler.restore_cursor(),

                // Kitty keyboard protocol
                // 1 => switch (input.intermediates[0]) {
                1 => match input.intermediates[0] {
                    b'?' => self.handler.kitty_keyboard_query(),

                    // '>' => push: { ... }
                    b'>' => 'push: {
                        // const flags: u5 = if (input.params.len == 1) std.math.cast(u5, ...) else 0;
                        let flags: u8 = if input.params.len() == 1 {
                            match u8::try_from(input.params[0]).ok().filter(|v| *v < 32) {
                                Some(v) => v,
                                None => {
                                    // log.warn("invalid pushKittyKeyboard command: {f}", .{input});
                                    break 'push;
                                }
                            }
                        } else {
                            0
                        };

                        self.handler.kitty_keyboard_push(KittyKeyboardFlags {
                            flags: kitty::KeyFlags::from_int(flags),
                        });
                    }

                    // '<' => { ... }
                    b'<' => {
                        // const number: u16 = if (input.params.len == 1) input.params[0] else 1;
                        let number: u16 = if input.params.len() == 1 {
                            input.params[0]
                        } else {
                            1
                        };

                        self.handler.kitty_keyboard_pop(number);
                    }

                    // '=' => set: { ... }
                    b'=' => 'set: {
                        // const flags: u5 = if (input.params.len >= 1) std.math.cast(u5, ...) else 0;
                        let flags: u8 = if input.params.len() >= 1 {
                            match u8::try_from(input.params[0]).ok().filter(|v| *v < 32) {
                                Some(v) => v,
                                None => {
                                    // log.warn("invalid setKittyKeyboard command: {f}", .{input});
                                    break 'set;
                                }
                            }
                        } else {
                            0
                        };

                        // const number: u16 = if (input.params.len >= 2) input.params[1] else 1;
                        let number: u16 = if input.params.len() >= 2 {
                            input.params[1]
                        } else {
                            1
                        };

                        // const action_tag: streampkg.Action.Tag = switch (number) {
                        // PORT NOTE: the reference selects an `Action.Tag` then re-switches
                        // on it to dispatch; the Rust port folds the two switches into one
                        // since there is no separate tag value to carry. The `kitty_flags`
                        // payload and the per-`number` dispatch are preserved verbatim.
                        // const kitty_flags: streampkg.Action.KittyKeyboardFlags =
                        //     .{ .flags = @as(kitty.KeyFlags, @bitCast(flags)) };
                        let kitty_flags: KittyKeyboardFlags = KittyKeyboardFlags {
                            flags: kitty::KeyFlags::from_int(flags),
                        };
                        match number {
                            // 1 => .kitty_keyboard_set,
                            1 => self.handler.kitty_keyboard_set(kitty_flags),
                            // 2 => .kitty_keyboard_set_or,
                            2 => self.handler.kitty_keyboard_set_or(kitty_flags),
                            // 3 => .kitty_keyboard_set_not,
                            3 => self.handler.kitty_keyboard_set_not(kitty_flags),
                            _ => {
                                // log.warn("invalid setKittyKeyboard command: {f}", .{input});
                                break 'set;
                            }
                        }
                    }

                    // else => log.warn("unknown CSI s with intermediate: {f}", .{input}),
                    _ => {}
                },

                // else => log.warn("ignoring unimplemented CSI u: {f}", .{input}),
                _ => {}
            },

            // ICH - Insert Blanks
            // '@' => switch (input.intermediates.len) { ... }
            b'@' => match input.intermediates.len() {
                0 => self.handler.insert_blanks(match input.params.len() {
                    0 => 1,
                    1 => std::cmp::max(1, input.params[0]) as usize,
                    _ => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid ICH command: {f}", .{input});
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI @: {f}", .{input}),
                _ => {}
            },

            // DECSASD - Select Active Status Display
            // '}' => decsasd: { ... }
            b'}' => 'decsasd: {
                // Verify we're getting a DECSASD command
                // if (input.intermediates.len != 1 or input.intermediates[0] != '$') { ... }
                if input.intermediates.len() != 1 || input.intermediates[0] != b'$' {
                    // log.warn("unimplemented CSI callback: {f}", .{input});
                    break 'decsasd;
                }
                // if (input.params.len != 1) { ... }
                if input.params.len() != 1 {
                    // log.warn("unimplemented CSI callback: {f}", .{input});
                    break 'decsasd;
                }

                // const display: ansi.StatusDisplay = switch (input.params[0]) {
                let display: ansi::StatusDisplay = match input.params[0] {
                    0 => ansi::StatusDisplay::Main,
                    1 => ansi::StatusDisplay::StatusLine,
                    _ => {
                        // log.warn("unimplemented CSI callback: {f}", .{input});
                        break 'decsasd;
                    }
                };

                self.handler.active_status_display(display);
            }

            // else => log.warn("unimplemented CSI action: {f}", .{input}),
            _ => {}
        }
    }

    // inline fn oscDispatch(self: *Self, cmd: osc.Command) void { ... }
    fn osc_dispatch(&mut self, cmd: osc::Command) {
        // The branch hints here are based on real world data
        // which indicates that the most common OSC commands are:
        //
        // 1. hyperlink_end
        // 2. change_window_title
        // 3. change_window_icon
        // 4. hyperlink_start
        // 5. report_pwd
        // 6. color_operation
        // 7. semantic_prompt
        //
        // Together, these 7 commands make up about 96% of all
        // OSC commands encountered in real world scenarios.
        //
        // Additionally, within the prongs, unlikely branch
        // hints have been added to branches that deal with
        // invalid sequences/commands, this is in order to
        // optimize for the happy path where we're getting
        // valid data from the program we're running.
        //
        // ref: https://github.com/qwerasd205/asciinema-stats

        // switch (cmd) {
        match cmd {
            // .semantic_prompt => |sp| { ... }
            osc::Command::SemanticPrompt(sp) => {
                // @branchHint(.likely);
                self.handler.semantic_prompt(sp);
            }

            // .change_window_title => |title| { ... }
            osc::Command::ChangeWindowTitle(title) => {
                // @branchHint(.likely);
                // if (!std.unicode.utf8ValidateSlice(title)) { ... }
                if std::str::from_utf8(&title).is_err() {
                    // @branchHint(.unlikely);
                    // log.warn("change title request: invalid utf-8, ignoring request", .{});
                    return;
                }

                self.handler.window_title(WindowTitle { title: &title });
            }

            // .change_window_icon => |icon| { ... }
            osc::Command::ChangeWindowIcon(_icon) => {
                // @branchHint(.likely);
                // logUnsupportedOnce(
                //     "OSC 1 (change icon) received and ignored icon={s}",
                //     .{icon},
                //     0,
                // );
                //
                // b5053153f also moved this site from `log.info` to the
                // `logUnsupportedOnce` (warn) family. The key is a literal 0, so
                // this message is emitted at most ONCE per process regardless of
                // the icon value.
                log_unsupported_once(&SEEN_OSC1_CHANGE_ICON, 0);
            }

            // .clipboard_contents => |clip| { ... }
            osc::Command::ClipboardContents { kind, data } => {
                self.handler
                    .clipboard_contents(ClipboardContents { kind, data: &data });
            }

            // .report_pwd => |v| { ... }
            osc::Command::ReportPwd { value } => {
                // @branchHint(.likely);
                self.handler.report_pwd(ReportPwd { url: &value });
            }

            // .mouse_shape => |v| { ... }
            osc::Command::MouseShape { value } => {
                // const shape = MouseShape.fromString(v.value) orelse { ... };
                let shape = match MouseShape::from_string(&value) {
                    Some(shape) => shape,
                    None => {
                        // @branchHint(.unlikely);
                        // log.warn("unknown cursor shape: {s}", .{v.value});
                        return;
                    }
                };

                self.handler.mouse_shape(shape);
            }

            // .color_operation => |v| { ... }
            osc::Command::ColorOperation {
                op,
                requests,
                terminator,
            } => {
                // @branchHint(.likely);
                self.handler.color_operation(ColorOperation {
                    op,
                    requests,
                    terminator,
                });
            }

            // .kitty_color_protocol => |v| { ... }
            osc::Command::KittyColorProtocol(v) => {
                self.handler.kitty_color_report(v);
            }

            // .show_desktop_notification => |v| { ... }
            osc::Command::ShowDesktopNotification { title, body } => {
                self.handler
                    .show_desktop_notification(ShowDesktopNotification {
                        title: &title,
                        body: &body,
                    });
            }

            // .hyperlink_start => |v| { ... }
            osc::Command::HyperlinkStart { id, uri } => {
                // @branchHint(.likely);
                self.handler.start_hyperlink(StartHyperlink {
                    uri: &uri,
                    id: id.as_deref(),
                });
            }

            // .hyperlink_end => { ... }
            osc::Command::HyperlinkEnd => {
                // @branchHint(.likely);
                self.handler.end_hyperlink();
            }

            // .conemu_progress_report => |v| { ... }
            osc::Command::ConemuProgressReport(v) => {
                self.handler.progress_report(v);
            }

            // .conemu_sleep, .conemu_show_message_box, .conemu_change_tab_title,
            // .conemu_wait_input, .conemu_guimacro, .conemu_comment,
            // .conemu_xterm_emulation, .conemu_output_environment_variable,
            // .conemu_run_process, .kitty_text_sizing, .kitty_clipboard_protocol,
            // .kitty_desktop_notification, .context_signal => { ... }
            osc::Command::ConemuSleep { .. }
            | osc::Command::ConemuShowMessageBox(_)
            | osc::Command::ConemuChangeTabTitle(_)
            | osc::Command::ConemuWaitInput
            | osc::Command::ConemuGuimacro(_)
            | osc::Command::ConemuComment(_)
            | osc::Command::ConemuXtermEmulation { .. }
            | osc::Command::ConemuOutputEnvironmentVariable(_)
            | osc::Command::ConemuRunProcess(_)
            | osc::Command::KittyTextSizing(_)
            | osc::Command::KittyClipboardProtocol(_)
            | osc::Command::KittyDndProtocol(_)
            | osc::Command::KittyDesktopNotification(_)
            | osc::Command::ContextSignal(_) => {
                // log.debug("unimplemented OSC callback: {}", .{cmd});
            }

            // .invalid => { ... }
            osc::Command::Invalid => {
                // @branchHint(.cold);
                // This is an invalid internal state, not an invalid OSC
                // string being parsed. We shouldn't see this.
                // log.warn("invalid OSC, should never happen", .{});
            }
        }
    }

    // inline fn configureCharset(self: *Self, intermediates: []const u8, set: charsets.Charset) void { ... }
    fn configure_charset(&mut self, intermediates: &[u8], set: charsets::Charset) {
        // if (intermediates.len != 1) { ... }
        if intermediates.len() != 1 {
            // log.warn("invalid charset intermediate: {any}", .{intermediates});
            return;
        }

        // const slot: charsets.Slots = switch (intermediates[0]) {
        let slot: charsets::Slots = match intermediates[0] {
            // TODO: support slots '-', '.', '/'
            b'(' => charsets::Slots::G0,
            b')' => charsets::Slots::G1,
            b'*' => charsets::Slots::G2,
            b'+' => charsets::Slots::G3,
            _ => {
                // @branchHint(.unlikely);
                // log.warn("invalid charset intermediate: {any}", .{intermediates});
                return;
            }
        };

        self.handler
            .configure_charset(ConfigureCharset { slot, charset: set });
    }

    // inline fn escDispatch(self: *Self, action: Parser.Action.ESC) void { ... }
    fn esc_dispatch(&mut self, action: parser::ESC) {
        // The branch hints here are based on real world data
        // which indicates that the most common ESC finals are:
        //
        // 1. B
        // 2. \
        // 3. 0
        // 4. M
        // 5. 8
        // 6. 7
        // 7. >
        // 8. =
        //
        // Together, these 8 finals make up nearly 99% of all
        // ESC sequences encountered in real world scenarios.
        //
        // Additionally, within the prongs, unlikely branch
        // hints have been added to branches that deal with
        // invalid sequences/commands, this is in order to
        // optimize for the happy path where we're getting
        // valid data from the program we're running.
        //
        // ref: https://github.com/qwerasd205/asciinema-stats

        // switch (action.final) {
        match action.final_ {
            // Charsets
            // 'B' => { ... }
            b'B' => {
                // @branchHint(.likely);
                self.configure_charset(&action.intermediates, charsets::Charset::Ascii);
            }
            // 'A' => self.configureCharset(action.intermediates, .british),
            b'A' => self.configure_charset(&action.intermediates, charsets::Charset::British),
            // '0' => { ... }
            b'0' => {
                // @branchHint(.likely);
                self.configure_charset(&action.intermediates, charsets::Charset::DecSpecial);
            }

            // DECSC - Save Cursor
            // '7' => { ... }
            b'7' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => self.handler.save_cursor(),
                    _ => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid command: {f}", .{action});
                        #[allow(clippy::needless_return)]
                        return;
                    }
                }
            }

            // '8' => blk: { ... }
            b'8' => 'blk: {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    // DECRC - Restore Cursor
                    0 => {
                        self.handler.restore_cursor();
                        break 'blk;
                    }

                    // 1 => switch (action.intermediates[0]) {
                    1 => match action.intermediates[0] {
                        // DECALN - Fill Screen with E
                        b'#' => {
                            self.handler.decaln();
                            break 'blk;
                        }

                        _ => {}
                    },

                    _ => {} // fall through
                }

                // logUnsupportedOnce(
                //     "unimplemented ESC action: {f}",
                //     .{action},
                //     action.final,
                // );
                log_unsupported_once(&SEEN_UNIMPLEMENTED_ESC_ACTION, u16::from(action.final_));
            }

            // IND - Index
            // 'D' => switch (action.intermediates.len) { ... }
            b'D' => match action.intermediates.len() {
                0 => self.handler.index(),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid index command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // NEL - Next Line
            // 'E' => switch (action.intermediates.len) { ... }
            b'E' => match action.intermediates.len() {
                0 => self.handler.next_line(),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid next line command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // HTS - Horizontal Tab Set
            // 'H' => switch (action.intermediates.len) { ... }
            b'H' => match action.intermediates.len() {
                0 => self.handler.tab_set(),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid tab set command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // RI - Reverse Index
            // 'M' => { ... }
            b'M' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => self.handler.reverse_index(),
                    _ => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid reverse index command: {f}", .{action});
                        #[allow(clippy::needless_return)]
                        return;
                    }
                }
            }

            // SS2 - Single Shift 2
            // 'N' => switch (action.intermediates.len) { ... }
            b'N' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G2,
                    locking: true,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 2 command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // SS3 - Single Shift 3
            // 'O' => switch (action.intermediates.len) { ... }
            b'O' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G3,
                    locking: true,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 3 command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // SPA - Start of Guarded Area
            // 'V' => switch (action.intermediates.len) { ... }
            b'V' => match action.intermediates.len() {
                0 => self.handler.protected_mode_iso(),
                // else => log.warn("unimplemented ESC callback: {f}", .{action}),
                _ => {}
            },

            // EPA - End of Guarded Area
            // 'W' => switch (action.intermediates.len) { ... }
            b'W' => match action.intermediates.len() {
                0 => self.handler.protected_mode_off(),
                // else => log.warn("unimplemented ESC callback: {f}", .{action}),
                _ => {}
            },

            // DECID
            // 'Z' => if (action.intermediates.len == 0) { ... } else log.warn(...),
            b'Z' => {
                if action.intermediates.len() == 0 {
                    self.handler
                        .device_attributes(device_attributes::Req::Primary);
                }
                // else log.warn("unimplemented ESC callback: {f}", .{action});
            }

            // RIS - Full Reset
            // 'c' => switch (action.intermediates.len) { ... }
            b'c' => match action.intermediates.len() {
                0 => self.handler.full_reset(),
                _ => {
                    // log.warn("invalid full reset command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS2 - Locking Shift 2
            // 'n' => switch (action.intermediates.len) { ... }
            b'n' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G2,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 2 command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS3 - Locking Shift 3
            // 'o' => switch (action.intermediates.len) { ... }
            b'o' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G3,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 3 command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS1R - Locking Shift 1 Right
            // '~' => switch (action.intermediates.len) { ... }
            b'~' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GR,
                    charset: charsets::Slots::G1,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid locking shift 1 right command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS2R - Locking Shift 2 Right
            // '}' => switch (action.intermediates.len) { ... }
            b'}' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GR,
                    charset: charsets::Slots::G2,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid locking shift 2 right command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS3R - Locking Shift 3 Right
            // '|' => switch (action.intermediates.len) { ... }
            b'|' => match action.intermediates.len() {
                0 => self.handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GR,
                    charset: charsets::Slots::G3,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid locking shift 3 right command: {f}", .{action});
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // Set application keypad mode
            // '=' => { ... }
            b'=' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => self.handler.set_mode(Mode {
                        mode: modes::Mode::KeypadKeys,
                    }),
                    // else => log.warn("unimplemented setMode: {f}", .{action}),
                    _ => {}
                }
            }

            // Reset application keypad mode
            // '>' => { ... }
            b'>' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => self.handler.reset_mode(Mode {
                        mode: modes::Mode::KeypadKeys,
                    }),
                    // else => log.warn("unimplemented setMode: {f}", .{action}),
                    _ => {}
                }
            }

            // Sets ST (string terminator). We don't have to do anything
            // because our parser always accepts ST.
            // '\\' => { ... }
            b'\\' => {
                // @branchHint(.likely);
            }

            // else => logUnsupportedOnce(
            //     "unimplemented ESC action: {f}",
            //     .{action},
            //     action.final,
            // ),
            _ => {
                log_unsupported_once(&SEEN_UNIMPLEMENTED_ESC_ACTION, u16::from(action.final_));
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
// distinct reference tuple — eight statics for ten call sites, with the two
// pairs deliberately sharing theirs. That is the reference's own granularity,
// and it is observable: under the reference, setting then resetting the same
// unimplemented mode logs ONCE, not twice.
//
// PORT NOTE (logging facade): the port carries NO logging facade — every
// `log.*` call in the tree is preserved as a comment (see e.g.
// `.port/phase-2/W1_DESIGN.md`'s "no logging facade is wired in the port yet"),
// and `rustty-vt` does not depend on the `log` crate at all. So the ONE line of
// this commit that cannot land is its tail, `log.warn(format, args)`. Everything
// else — the table, the lock-free claim scan, the atomic orderings, the
// table-full suppression, the per-site key choice, and the call-site
// conversions — is live code and is exercised by
// `stream_logs_unsupported_once_per_value`. The claim is performed at each site
// exactly as the reference performs it, so when a facade is eventually wired the
// only edit is to emit the message under the returned boolean.
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
/// Returns whether the caller should emit the message. The reference emits it
/// directly (`log.warn(format, args)`); the port has no facade to emit it to,
/// so the decision is returned instead — see the PORT NOTE above.
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

// The eight reference `(format, @TypeOf(args))` instantiations, in the order
// they appear in `stream.zig`. `SEEN_UNIMPLEMENTED_MODE` and
// `SEEN_UNIMPLEMENTED_ESC_ACTION` are each shared by two call sites, exactly as
// Zig shares them.
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
mod tests {
    use super::*;
    use crate::{ansi, csi, modes};

    // PORT NOTE: the reference passes string/byte literals to `next`/`nextSlice`.
    // Rust string literals are `&str`; `next` takes a `u8` and `next_slice` a
    // `&[u8]`. We iterate the bytes of the literal exactly as Zig iterates the
    // bytes of its string literal.

    // test Action {
    //     // Forces the C type to be reified when the target is C, ensuring
    //     // all our types are C ABI compatible.
    //     _ = Action.C;
    // }
    // PORT NOTE: the reference `test Action` reifies the C-ABI union type so the
    // compiler verifies C ABI compatibility. The Rust port models `Action` as
    // the `Handler` trait; the C-ABI mirror lives in the `rustty-vt-c` crate, so
    // there is no `Action.C` type to reify here. The test is intentionally
    // retargeted to a no-op assertion that the dispatch surface compiles.
    #[test]
    fn action() {
        // No C-ABI union in this crate; see PORT NOTE above.
    }

    // rustty-only test (b5053153f, WS10 T7). The reference ships this commit with
    // NO test: its observable is the log stream, and Zig's `log.warn` is not
    // capturable in a `test` block. In the port the observable is better —
    // `UnsupportedOnceTable` is real state — so the dedup semantics are asserted
    // directly. Two arms:
    //
    //   (1) the MECHANISM, on a table local to this test, so it is deterministic
    //       and cannot be perturbed by other tests running in parallel;
    //   (2) the WIRING, i.e. that the dispatch sites really call the helper,
    //       through the ONE site whose key is a compile-time constant.
    #[test]
    fn stream_logs_unsupported_once_per_value() {
        // ---- (1) mechanism -------------------------------------------------
        let table = UnsupportedOnceTable::new();

        // First sighting of a key logs; every repeat is suppressed.
        assert!(log_unsupported_once(&table, 34));
        assert!(!log_unsupported_once(&table, 34));
        assert!(!log_unsupported_once(&table, 34));
        assert_eq!(1, table.claimed());

        // A DISTINCT value logs again — "once per distinct value", not "once".
        assert!(log_unsupported_once(&table, 35));
        assert!(!log_unsupported_once(&table, 35));
        assert!(!log_unsupported_once(&table, 34));
        assert_eq!(2, table.claimed());

        // Key 0 is a real key, not the empty sentinel (the sentinel is
        // `u32::MAX`, which no `u16` key can reach — that is why the slots are
        // `u32`). If 0 were the sentinel this pair would both log.
        assert!(log_unsupported_once(&table, 0));
        assert!(!log_unsupported_once(&table, 0));

        // Fill the remaining slots, then assert the reference's table-full rule:
        // "if a table fills, new values are suppressed too".
        for k in 100u16..(100 + 13) {
            assert!(log_unsupported_once(&table, k), "slot {k} should be free");
        }
        assert_eq!(16, table.claimed());
        assert!(
            !log_unsupported_once(&table, 9999),
            "table full: new values must be suppressed too"
        );
        // ...but an ALREADY-CLAIMED key is still found by the scan even when the
        // table is full, so a full table does not resurrect duplicate messages.
        assert!(!log_unsupported_once(&table, 34));

        // ---- (2) wiring ----------------------------------------------------
        // OSC 1 is the only converted site whose key is a literal (`0`), so its
        // global table can never hold more than one claimed slot no matter what
        // any other test does — which makes this arm order- and
        // parallelism-independent, unlike an arm keyed on a mode number.
        struct H;
        impl Handler for H {}

        let mut s: Stream<H> = Stream::init(Options::with_handler(H));
        s.next_slice(b"\x1b]1;icon\x07");
        assert_eq!(
            1,
            SEEN_OSC1_CHANGE_ICON.claimed(),
            "the OSC 1 dispatch arm must claim its dedup slot"
        );

        // Driving the same unsupported input a second time must NOT claim
        // anything further — this is the "once" half of the commit.
        s.next_slice(b"\x1b]1;other-icon\x07");
        assert_eq!(1, SEEN_OSC1_CHANGE_ICON.claimed());
        assert!(
            !log_unsupported_once(&SEEN_OSC1_CHANGE_ICON, 0),
            "key 0 was already claimed by the dispatch arm"
        );
    }

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
    const CSI_DIFFERENTIAL_CORPUS: &[&[u8]] = &[
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
    struct RecordingHandler {
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
    struct RawHookedRecorder {
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
    fn drive_bulk<H: Handler + Default>(input: &[u8]) -> H {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        let mut i = 0usize;
        while i < input.len() {
            if s.parser.state != parser::State::Ground {
                // `consume_until_ground` always advances (it consumes at least
                // one byte per iteration, or returns `input.len()`).
                i += s.consume_until_ground(&input[i..]);
                continue;
            }
            let c = input[i];
            if c == 0x1B {
                s.parser.state = parser::State::Escape;
                s.parser.clear();
                i += 1;
                continue;
            }
            s.next_utf8(c);
            i += 1;
        }
        s.handler
    }

    /// `drive_bulk` for a stream that already exists, so a test can feed
    /// several chunks through the bulk path with the parser state carried
    /// across calls (the reference does this with repeated `nextSlice`).
    fn drive_bulk_into<H: Handler>(s: &mut Stream<H>, input: &[u8]) {
        let mut i = 0usize;
        while i < input.len() {
            if s.parser.state != parser::State::Ground {
                i += s.consume_until_ground(&input[i..]);
                continue;
            }
            let c = input[i];
            if c == 0x1B {
                s.parser.state = parser::State::Escape;
                s.parser.clear();
                i += 1;
                continue;
            }
            s.next_utf8(c);
            i += 1;
        }
    }

    /// The public `next_slice` entry point. Identical to `drive_bulk` when the
    /// `simd` feature is on; identical to `drive_per_byte` when it is off.
    /// Both configurations are asserted, so neither can rot.
    fn drive_slice<H: Handler + Default>(input: &[u8]) -> H {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        s.next_slice(input);
        s.handler
    }

    fn drive_per_byte<H: Handler + Default>(input: &[u8]) -> H {
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
    // ---------------------------------------------------------------------
    // WS10 T16 (47e26df60 + f2a7652ab): print_slice run batching in the
    // ground-state SIMD loop.
    // ---------------------------------------------------------------------

    /// Drive the input through the GROUND-STATE loop in `next_slice_capped`,
    /// which is where `47e26df60`'s printable-run batching and `f2a7652ab`'s
    /// vectorized run scan live.
    ///
    /// THIS IS NOT `next_slice`, AND THAT IS THE POINT — same vacuity trap
    /// `drive_bulk` documents above: `next_slice` opens with
    /// `if DEBUG || !crate::options::SIMD { per-byte; return; }`, and `simd` is
    /// OFF by default, so a test written against `next_slice` would compare the
    /// per-byte path with itself in a default `cargo test` build and never
    /// reach a single `print_slice`. `next_slice_capped` has no such gate, so
    /// calling it directly exercises the batching in BOTH feature
    /// configurations.
    fn drive_ground<H: Handler + Default>(input: &[u8]) -> H {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        let mut cp_buf: [u32; 4096] = [0; 4096];
        assert!(input.len() <= cp_buf.len());
        s.next_slice_capped(input, &mut cp_buf);
        s.handler
    }

    /// Records the `print_slice` runs verbatim (rather than fanning them out
    /// to `print` the way the trait default does), so a test can assert on the
    /// RUN STRUCTURE and not just the resulting codepoints.
    #[derive(Default)]
    struct PrintSliceRecorder {
        runs: Vec<Vec<u32>>,
        singles: Vec<u32>,
        bells: usize,
    }

    impl Handler for PrintSliceRecorder {
        fn print(&mut self, cp: u32) {
            self.singles.push(cp);
        }
        fn print_slice(&mut self, cps: &[u32]) {
            self.runs.push(cps.to_vec());
        }
        fn bell(&mut self) {
            // 0x07 is the control the tests below use as a run separator; the
            // stream's `execute` routes it here.
            self.bells += 1;
        }
    }

    /// The printable-run batching must be observationally identical to the
    /// per-byte `print` stream it replaces: `RecordingHandler` does NOT
    /// override `print_slice`, so its trait default fans each run back out to
    /// `print` and the two logs must be equal byte for byte.
    ///
    /// This is the falsifiable form of `47e26df60`'s "semantically identical
    /// to calling `print` for each codepoint in order".
    #[test]
    // MIRI: `next_slice_capped` calls `rustty_simd_decode_utf8_until_control_seq`,
    // a foreign (C++/highway) function Miri cannot execute ("unsupported
    // operation: can't call foreign function"). Structural, not a soundness
    // finding — no Rust unsafe code of this port goes unverified, because the
    // raw-pointer cell writes these tests reach live in `print_slice_fill` and
    // ARE Miri-covered by `terminal_print_slice_*`, which drive
    // `Terminal::print_slice` directly with no SIMD decode. Registered in
    // .port/phase-2/MIRI_EXCEPTIONS.md (WS10 T16).
    #[cfg_attr(miri, ignore)]
    fn stream_print_slice_matches_per_byte() {
        const CORPUS: &[&[u8]] = &[
            b"hello world",
            b"",
            b"a",
            b"\x07",
            b"hello\nworld\r\n",
            b"one\x07two\x08three",
            // Multi-byte UTF-8 mixed with ASCII and controls.
            "caf\u{e9} \u{4e00}\u{4e01} \u{1f600}!".as_bytes(),
            // Every C0 in range, interleaved with printables.
            b"a\x00b\x01c\x02d\x03e\x04f\x05g\x06h\x07i\x08j\x09k\x0al\x0bm\x0cn\x0do\x0ep\x0fq",
            // A run longer than any plausible vector width, then a control.
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x07b",
            // Control first.
            b"\x0aabc",
            // Escape sequences bracketing printable runs.
            b"abc\x1b[31mdef\x1b[0mghi",
        ];

        for input in CORPUS {
            let per_byte = drive_per_byte::<RecordingHandler>(input);
            let ground = drive_ground::<RecordingHandler>(input);
            assert_eq!(
                per_byte.log,
                ground.log,
                "print_slice batching diverged from per-byte path on {:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    /// The run scan must actually BATCH: a contiguous printable run arrives as
    /// ONE `print_slice` carrying every codepoint, and a C0 splits it into two
    /// runs with the control executed in between. Without this, the previous
    /// test still passes with a `print_slice` that emits one codepoint at a
    /// time (which is what an off-by-one in the vectorized `end` scan would
    /// produce), so this is the negative control for `f2a7652ab`'s lane loop.
    #[test]
    // MIRI: `next_slice_capped` calls `rustty_simd_decode_utf8_until_control_seq`,
    // a foreign (C++/highway) function Miri cannot execute ("unsupported
    // operation: can't call foreign function"). Structural, not a soundness
    // finding — no Rust unsafe code of this port goes unverified, because the
    // raw-pointer cell writes these tests reach live in `print_slice_fill` and
    // ARE Miri-covered by `terminal_print_slice_*`, which drive
    // `Terminal::print_slice` directly with no SIMD decode. Registered in
    // .port/phase-2/MIRI_EXCEPTIONS.md (WS10 T16).
    #[cfg_attr(miri, ignore)]
    fn stream_print_slice_batches_printable_runs() {
        // 70 'a's is longer than 2x any native vector width for u32, so the
        // vector loop runs to completion and hands off to the scalar tail.
        let mut input = vec![b'a'; 70];
        input.push(0x07); // BEL, a C0 -> execute
        input.extend_from_slice(b"bcd");

        let h = drive_ground::<PrintSliceRecorder>(&input);
        assert_eq!(2, h.runs.len(), "expected exactly two printable runs");
        assert_eq!(70, h.runs[0].len());
        assert!(h.runs[0].iter().all(|&c| c == 'a' as u32));
        assert_eq!(
            vec!['b' as u32, 'c' as u32, 'd' as u32],
            h.runs[1],
            "the run after the control must be batched too"
        );
        assert_eq!(1, h.bells);
        assert!(
            h.singles.is_empty(),
            "the ground loop must emit print_slice, never bare print"
        );

        // Every lane boundary: a control at offset N for N across a full
        // vector's worth of positions must split exactly there.
        for n in 0..40usize {
            let mut input: Vec<u8> = vec![b'x'; n];
            input.push(0x07);
            input.extend_from_slice(b"yz");
            let h = drive_ground::<PrintSliceRecorder>(&input);
            let expect_runs = if n == 0 { 1 } else { 2 };
            assert_eq!(expect_runs, h.runs.len(), "n={n}");
            if n > 0 {
                assert_eq!(n, h.runs[0].len(), "n={n}");
            }
            assert_eq!(
                vec!['y' as u32, 'z' as u32],
                *h.runs.last().unwrap(),
                "n={n}"
            );
        }
    }

    // ---------------------------------------------------------------------
    // WS10 T17 (f6f79acce + 8c523ed03): bulk APC slice dispatch.
    // ---------------------------------------------------------------------

    /// A test handler that accumulates APC bytes regardless of whether they
    /// arrive per-byte (apc_put) or in bulk (apc_put_slice).
    // const ApcTestHandler = struct { ... };
    struct ApcTestHandler {
        buf: [u8; 256],
        len: usize,
        slices: usize,
        puts: usize,
        started: usize,
        ended: usize,
    }

    impl Default for ApcTestHandler {
        fn default() -> ApcTestHandler {
            ApcTestHandler {
                buf: [0; 256],
                len: 0,
                slices: 0,
                puts: 0,
                started: 0,
                ended: 0,
            }
        }
    }

    impl Handler for ApcTestHandler {
        fn apc_start(&mut self) {
            self.started += 1;
        }
        fn apc_end(&mut self, _v: ApcEnd) {
            self.ended += 1;
        }
        fn apc_put(&mut self, code: u8) {
            self.buf[self.len] = code;
            self.len += 1;
            self.puts += 1;
        }
        fn apc_put_slice(&mut self, bytes: &[u8]) {
            self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
            self.len += bytes.len();
            self.slices += 1;
        }
    }

    /// Drive input through the bulk path (`consume_until_ground`, where
    /// `consume_apc_string` lives), NOT through `next_slice` — see the vacuity
    /// note on `drive_bulk`. `next_slice` is per-byte without `--features
    /// simd`, so a test written against it would never reach a single
    /// `apc_put_slice` in the default build.
    fn drive_apc_bulk(input: &[u8]) -> ApcTestHandler {
        drive_bulk::<ApcTestHandler>(input)
    }

    // test "stream: apc bulk slice"
    #[test]
    fn stream_apc_bulk_slice() {
        let h = drive_apc_bulk(b"\x1b_Gf=24,s=10,v=20;aGVsbG8=\x1b\\");

        assert_eq!(1, h.started);
        assert_eq!(1, h.ended);
        assert_eq!(b"Gf=24,s=10,v=20;aGVsbG8=".as_slice(), &h.buf[0..h.len]);

        // With SIMD enabled the body must arrive as a single slice.
        //
        // PORT NOTE: the reference guards this with
        // `if (comptime build_options.simd and !debug)`, because its `nextSlice`
        // driver is the one that reaches the bulk path. This port drives
        // `consume_until_ground` directly (see `drive_apc_bulk`), which is
        // always the bulk path, so the assertion is UNCONDITIONAL here — and
        // that is strictly stronger: it holds in both feature configurations.
        assert_eq!(1, h.slices);
        assert_eq!(0, h.puts);
    }

    // test "stream: apc bulk slice C1 ST"
    #[test]
    fn stream_apc_bulk_slice_c1_st() {
        // var s: Stream(ApcTestHandler) = .init(.{ .handler = .{} });
        // s.nextSlice("\x1b_Gpayload\x9c");
        //
        // PORT NOTE: driven through `drive_apc_bulk` (the module's bulk-path
        // driver, see its vacuity note) so the C1 ST fast path in
        // `consume_until_ground` is exercised in both feature configurations.
        let h = drive_apc_bulk(b"\x1b_Gpayload\x9c");

        assert_eq!(1, h.started);
        assert_eq!(1, h.ended);
        assert_eq!(b"Gpayload".as_slice(), &h.buf[0..h.len]);
    }

    // test "stream: apc bulk slice split across inputs"
    #[test]
    fn stream_apc_bulk_slice_split_across_inputs() {
        let mut s: Stream<ApcTestHandler> =
            Stream::init(Options::with_handler(ApcTestHandler::default()));
        for chunk in [
            b"\x1b_Gf=24,s=10".as_slice(),
            b",v=20;aGVs".as_slice(),
            b"bG8=\x1b\\".as_slice(),
        ] {
            drive_bulk_into(&mut s, chunk);
        }
        let h = s.handler;

        assert_eq!(1, h.started);
        assert_eq!(1, h.ended);
        assert_eq!(b"Gf=24,s=10,v=20;aGVsbG8=".as_slice(), &h.buf[0..h.len]);
    }

    // test "stream: apc bulk slice keeps C0 bytes as data"
    #[test]
    fn stream_apc_bulk_slice_keeps_c0_bytes_as_data() {
        // BEL does not terminate an APC string; it is payload data.
        let h = drive_apc_bulk(b"\x1b_Gx\x07y\x1b\\");

        assert_eq!(1, h.ended);
        assert_eq!(b"Gx\x07y".as_slice(), &h.buf[0..h.len]);
    }

    // test "stream: apc aborted by CAN"  (text widened by 8c523ed03)
    #[test]
    fn stream_apc_aborted_by_can() {
        // CAN (0x18) aborts the APC string via the anywhere => ground
        // transition. Exiting the sos_pm_apc_string state emits apc_end,
        // and the trailing bytes are printed, not treated as APC data.
        let h = drive_apc_bulk(b"\x1b_Gabcdefghijklmnopqrstuvwxyz0123456789\x18def");

        assert_eq!(1, h.started);
        assert_eq!(1, h.ended);
        assert_eq!(
            b"Gabcdefghijklmnopqrstuvwxyz0123456789".as_slice(),
            &h.buf[0..h.len]
        );
    }

    // test "stream: apc scalar path matches"
    #[test]
    fn stream_apc_scalar_path_matches() {
        let h = drive_per_byte::<ApcTestHandler>(b"\x1b_Gf=24;aGVsbG8=\x1b\\");

        assert_eq!(1, h.started);
        assert_eq!(1, h.ended);
        assert_eq!(b"Gf=24;aGVsbG8=".as_slice(), &h.buf[0..h.len]);
    }

    // test "stream: apc vector boundaries match scalar path"
    //
    // This is the negative control for `8c523ed03`'s vectorization: it must
    // FAIL if the lane boundary handling is off by one.
    #[test]
    fn stream_apc_vector_boundaries_match_scalar_path() {
        let positions = [15usize, 16, 17, 31, 32, 33, 63, 64, 65];
        let controls = [0x18u8, 0x1A, 0x1B, 0x80, 0xFF];

        for position in positions {
            for control in controls {
                let mut input: Vec<u8> = Vec::with_capacity(96);
                input.extend_from_slice(b"\x1b_G");
                input.extend(core::iter::repeat_n(b'a', position));
                input.push(control);
                input.push(b'\\');
                let bytes = &input[..];

                let bulk = drive_apc_bulk(bytes);
                let scalar = drive_per_byte::<ApcTestHandler>(bytes);

                assert_eq!(
                    scalar.started, bulk.started,
                    "pos={position} ctl={control:#x}"
                );
                assert_eq!(scalar.ended, bulk.ended, "pos={position} ctl={control:#x}");
                assert_eq!(
                    &scalar.buf[0..scalar.len],
                    &bulk.buf[0..bulk.len],
                    "pos={position} ctl={control:#x}"
                );
            }
        }
    }

    /// RUSTTY-SIDE (WS10 T17): the oracle's boundary test above NEVER reaches
    /// `consume_apc_string`'s vector `invalid` branch.
    ///
    /// Its inputs end two bytes after the control, so with a 16-lane scan the
    /// last FULL window always stops just short of the control and the scalar
    /// tail finds it — the `if invalid { break }` arm is never taken. Proven by
    /// mutation: making that arm `end += 1; break` left the oracle test GREEN.
    ///
    /// This variant pads the input after the control so the control lands
    /// mid-window and the vector loop must detect it. Note 0x80..0xFF are
    /// "ignored" by the parse table (still APC state), so the trailing bytes
    /// after those controls are real payload that both paths must agree on —
    /// which is exactly the differential this asserts.
    #[test]
    fn stream_apc_vector_invalid_branch_matches_scalar_path() {
        let controls = [0x18u8, 0x1A, 0x1B, 0x80, 0xFF];

        // Enough trailing bytes that the control is inside a full vector
        // window for any plausible lane count (16 for NEON/SSE2 u8, 32 for
        // AVX2).
        for position in 0..40usize {
            for control in controls {
                let mut input: Vec<u8> = Vec::with_capacity(160);
                input.extend_from_slice(b"\x1b_G");
                input.extend(core::iter::repeat_n(b'a', position));
                input.push(control);
                input.extend(core::iter::repeat_n(b'z', 80));
                input.extend_from_slice(b"\x1b\\");
                let bytes = &input[..];

                let bulk = drive_apc_bulk(bytes);
                let scalar = drive_per_byte::<ApcTestHandler>(bytes);

                assert_eq!(
                    scalar.started, bulk.started,
                    "pos={position} ctl={control:#x}"
                );
                assert_eq!(scalar.ended, bulk.ended, "pos={position} ctl={control:#x}");
                assert_eq!(
                    &scalar.buf[0..scalar.len],
                    &bulk.buf[0..bulk.len],
                    "pos={position} ctl={control:#x}"
                );
            }
        }
    }

    #[test]
    fn stream_csi_bulk_params_match_per_byte() {
        for input in CSI_DIFFERENTIAL_CORPUS {
            let per_byte = drive_per_byte::<RecordingHandler>(input);

            let bulk = drive_bulk::<RecordingHandler>(input);
            assert_eq!(
                per_byte.log,
                bulk.log,
                "bulk path diverged from per-byte path on {:?}",
                String::from_utf8_lossy(input)
            );

            // The public entry point too, so whichever of the two shapes the
            // active feature set selects is also covered.
            let sliced = drive_slice::<RecordingHandler>(input);
            assert_eq!(
                per_byte.log,
                sliced.log,
                "next_slice diverged from per-byte path on {:?}",
                String::from_utf8_lossy(input)
            );

            // Split every input at every boundary and feed it as two chunks.
            // `consume_csi_params` carries accumulator state across calls
            // (`param_acc`/`param_acc_idx`/`params_idx` are written back on
            // exit), so a mid-parameter split is exactly where a save/restore
            // bug would hide.
            for split in 1..input.len() {
                let mut s: Stream<RecordingHandler> =
                    Stream::init(Options::with_handler(RecordingHandler::default()));
                let mut feed = |chunk: &[u8]| {
                    let mut i = 0usize;
                    while i < chunk.len() {
                        if s.parser.state != parser::State::Ground {
                            i += s.consume_until_ground(&chunk[i..]);
                            continue;
                        }
                        let c = chunk[i];
                        if c == 0x1B {
                            s.parser.state = parser::State::Escape;
                            s.parser.clear();
                            i += 1;
                            continue;
                        }
                        s.next_utf8(c);
                        i += 1;
                    }
                };
                feed(&input[..split]);
                feed(&input[split..]);
                assert_eq!(
                    per_byte.log,
                    s.handler.log,
                    "split at {split} diverged on {:?}",
                    String::from_utf8_lossy(input)
                );
            }
        }
    }

    // rustty-only test (WS10 T14 Step 3): the `vtRaw` interlock.
    //
    // `D-terminal-subsystems.md:506` flags this as load-bearing for the
    // inspector: the fast paths dispatch without going through `Parser.next`,
    // so a handler with a raw hook must stay on the general path or its VT
    // event log silently loses every CSI. Two properties, both required:
    //
    //   (a) a raw-hooked handler still observes a `CsiDispatch` action for
    //       every CSI in the corpus — the property that breaks if `HAS_VT_RAW`
    //       is not set on a handler that overrides `vt_raw`;
    //   (b) the fast paths did not change WHAT is dispatched — the fast-path
    //       handler's callback log equals the general-path handler's, which
    //       makes the general path the oracle for the whole fast-path family
    //       (T13's test compares two fast-path spellings; this one compares
    //       fast path against no fast path at all).
    #[test]
    fn stream_fast_paths_bypassed_when_vt_raw_hooked() {
        let mut saw_csi = 0usize;
        for input in CSI_DIFFERENTIAL_CORPUS {
            let hooked = drive_bulk::<RawHookedRecorder>(input);
            let fast = drive_bulk::<RecordingHandler>(input);

            // (a)
            let csis = hooked
                .raw
                .iter()
                .filter(|a| a.starts_with("CsiDispatch"))
                .count();
            saw_csi += csis;

            // (b)
            assert_eq!(
                hooked.log,
                fast.log,
                "fast path diverged from the general state machine on {:?}",
                String::from_utf8_lossy(input)
            );
        }

        // Guard against the corpus silently ceasing to contain CSIs, which
        // would make arm (a) vacuous. A FLOOR, not an equality: the count is
        // not one-per-entry, because some entries deliberately produce no
        // dispatch (the MAX_PARAMS-overflow entry) and some produce several
        // (`\x1b[A\x1b[B\x1b[C\x1b[D`). Measured 33 at landing over 34 entries;
        // the floor is set below that so ADDING corpus entries never reddens
        // this, only losing CSIs does.
        assert!(
            saw_csi >= 30,
            "the raw hook must still observe the corpus's CSI dispatches; \
             saw {saw_csi} across {} entries (floor 30)",
            CSI_DIFFERENTIAL_CORPUS.len()
        );
    }

    // test "stream: print" { ... }
    #[test]
    fn stream_print() {
        struct H {
            c: Option<u32>,
        }
        impl Handler for H {
            fn print(&mut self, cp: u32) {
                self.c = Some(cp);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { c: Some(0) }));
        s.next(b'x');
        assert_eq!(120u32, s.handler.c.unwrap());
    }

    // rustty-only test (VALIDATION_REPORT §3d): the REP dispatch delivers the
    // reference's `print_repeat: usize` payload (stream.zig:36) — the u16 CSI
    // param widens into it; the handler signature must be usize, not u16.
    #[test]
    fn stream_print_repeat_payload_is_usize() {
        struct H {
            count: Option<usize>,
        }
        impl Handler for H {
            fn print_repeat(&mut self, count: usize) {
                self.count = Some(count);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { count: None }));
        // CSI 500 b => REP with the max-clamped u16 param widened to usize 500.
        for b in b"\x1b[500b" {
            s.next(*b);
        }
        assert_eq!(Some(500usize), s.handler.count);
    }

    // test "simd: print invalid utf-8" { ... }
    #[test]
    fn simd_print_invalid_utf8() {
        struct H {
            c: Option<u32>,
        }
        impl Handler for H {
            fn print(&mut self, cp: u32) {
                self.c = Some(cp);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { c: Some(0) }));
        s.next_slice(&[0xFF]);
        assert_eq!(0xFFFDu32, s.handler.c.unwrap());
    }

    // test "simd: complete incomplete utf-8" { ... }
    #[test]
    fn simd_complete_incomplete_utf8() {
        struct H {
            c: Option<u32>,
        }
        impl Handler for H {
            fn print(&mut self, cp: u32) {
                self.c = Some(cp);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { c: None }));
        s.next_slice(&[0xE0]); // 3 byte
        assert!(s.handler.c.is_none());
        s.next_slice(&[0xA0]); // still incomplete
        assert!(s.handler.c.is_none());
        s.next_slice(&[0x80]);
        assert_eq!(0x800u32, s.handler.c.unwrap());
    }

    // test "stream: cursor right (CUF)" { ... }
    #[test]
    fn stream_cursor_right_cuf() {
        struct H {
            amount: u16,
        }
        impl Handler for H {
            fn cursor_right(&mut self, v: CursorMovement) {
                self.amount = v.value;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { amount: 0 }));
        s.next_slice(b"\x1B[C");
        assert_eq!(1u16, s.handler.amount);

        s.next_slice(b"\x1B[5C");
        assert_eq!(5u16, s.handler.amount);

        s.handler.amount = 0;
        s.next_slice(b"\x1B[5;4C");
        assert_eq!(0u16, s.handler.amount);

        s.handler.amount = 0;
        s.next_slice(b"\x1b[?3C");
        assert_eq!(0u16, s.handler.amount);
    }

    // test "stream: dec set mode (SM) and reset mode (RM)" { ... }
    #[test]
    fn stream_dec_set_mode_sm_and_reset_mode_rm() {
        // PORT NOTE: the reference sentinel `@as(modes.Mode, @enumFromInt(1))`
        // is the mode whose tag is 1 (DECCKM / `cursor_keys`); used here as a
        // "not origin" marker. We use `modes::Mode::CursorKeys` (= 1) directly.
        struct H {
            mode: modes::Mode,
        }
        impl Handler for H {
            fn set_mode(&mut self, v: Mode) {
                self.mode = v.mode;
            }
            fn reset_mode(&mut self, _v: Mode) {
                self.mode = modes::Mode::CursorKeys;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H {
            mode: modes::Mode::CursorKeys,
        }));
        s.next_slice(b"\x1B[?6h");
        assert_eq!(modes::Mode::Origin, s.handler.mode);

        s.next_slice(b"\x1B[?6l");
        assert_eq!(modes::Mode::CursorKeys, s.handler.mode);

        s.handler.mode = modes::Mode::CursorKeys;
        s.next_slice(b"\x1B[6 h");
        assert_eq!(modes::Mode::CursorKeys, s.handler.mode);
    }

    // test "stream: ansi set mode (SM) and reset mode (RM)" { ... }
    #[test]
    fn stream_ansi_set_mode_sm_and_reset_mode_rm() {
        struct H {
            mode: Option<modes::Mode>,
        }
        impl Handler for H {
            fn set_mode(&mut self, v: Mode) {
                self.mode = Some(v.mode);
            }
            fn reset_mode(&mut self, _v: Mode) {
                self.mode = None;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { mode: None }));
        s.next_slice(b"\x1B[4h");
        assert_eq!(modes::Mode::Insert, s.handler.mode.unwrap());

        s.next_slice(b"\x1B[4l");
        assert!(s.handler.mode.is_none());

        s.handler.mode = None;
        s.next_slice(b"\x1B[>5h");
        assert!(s.handler.mode.is_none());
    }

    // test "stream: ansi set mode (SM) and reset mode (RM) with unknown value" { ... }
    #[test]
    fn stream_ansi_set_mode_sm_and_reset_mode_rm_with_unknown_value() {
        // PORT NOTE: the reference handler defines a `setMode` method that is
        // never invoked (the modern dispatch uses `vt`), and a no-op `vt`. The
        // mode `6` (DECCKM-adjacent value) used here is `origin`, which IS a
        // known mode, but the no-op handler ignores it, so `mode` stays null.
        struct H {
            mode: Option<modes::Mode>,
        }
        impl Handler for H {}

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { mode: None }));
        s.next_slice(b"\x1B[6h");
        assert!(s.handler.mode.is_none());

        s.next_slice(b"\x1B[6l");
        assert!(s.handler.mode.is_none());
    }

    // test "stream: restore mode" { ... }
    #[test]
    fn stream_restore_mode() {
        struct H {
            called: bool,
        }
        impl Handler for H {
            fn top_and_bottom_margin(&mut self, _v: Margin) {
                self.called = true;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
        for c in b"\x1B[?42r" {
            s.next(*c);
        }
        assert!(!s.handler.called);
    }

    // test "stream: pop kitty keyboard with no params defaults to 1" { ... }
    #[test]
    fn stream_pop_kitty_keyboard_with_no_params_defaults_to_1() {
        struct H {
            n: u16,
        }
        impl Handler for H {
            fn kitty_keyboard_pop(&mut self, v: u16) {
                self.n = v;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { n: 0 }));
        for c in b"\x1B[<u" {
            s.next(*c);
        }
        assert_eq!(1u16, s.handler.n);
    }

    // test "stream: DECSCA" { ... }
    #[test]
    fn stream_decsca() {
        struct H {
            v: Option<ansi::ProtectedMode>,
        }
        impl Handler for H {
            fn protected_mode_off(&mut self) {
                self.v = Some(ansi::ProtectedMode::Off);
            }
            fn protected_mode_iso(&mut self) {
                self.v = Some(ansi::ProtectedMode::Iso);
            }
            fn protected_mode_dec(&mut self) {
                self.v = Some(ansi::ProtectedMode::Dec);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { v: None }));
        {
            for c in b"\x1B[\"q" {
                s.next(*c);
            }
            assert_eq!(ansi::ProtectedMode::Off, s.handler.v.unwrap());
        }
        {
            for c in b"\x1B[0\"q" {
                s.next(*c);
            }
            assert_eq!(ansi::ProtectedMode::Off, s.handler.v.unwrap());
        }
        {
            for c in b"\x1B[2\"q" {
                s.next(*c);
            }
            assert_eq!(ansi::ProtectedMode::Off, s.handler.v.unwrap());
        }
        {
            for c in b"\x1B[1\"q" {
                s.next(*c);
            }
            assert_eq!(ansi::ProtectedMode::Dec, s.handler.v.unwrap());
        }
    }

    // test "stream: DECED, DECSED" { ... }
    #[test]
    fn stream_deced_decsed() {
        struct H {
            mode: Option<csi::EraseDisplay>,
            protected: Option<bool>,
        }
        impl Handler for H {
            fn erase_display_below(&mut self, protected: bool) {
                self.mode = Some(csi::EraseDisplay::Below);
                self.protected = Some(protected);
            }
            fn erase_display_above(&mut self, protected: bool) {
                self.mode = Some(csi::EraseDisplay::Above);
                self.protected = Some(protected);
            }
            fn erase_display_complete(&mut self, protected: bool) {
                self.mode = Some(csi::EraseDisplay::Complete);
                self.protected = Some(protected);
            }
            fn erase_display_scrollback(&mut self, protected: bool) {
                self.mode = Some(csi::EraseDisplay::Scrollback);
                self.protected = Some(protected);
            }
            fn erase_display_scroll_complete(&mut self, protected: bool) {
                self.mode = Some(csi::EraseDisplay::ScrollComplete);
                self.protected = Some(protected);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H {
            mode: None,
            protected: None,
        }));
        {
            for c in b"\x1B[?J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?0J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?1J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Above, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?2J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Complete, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?3J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Scrollback, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }

        {
            for c in b"\x1B[J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[0J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[1J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Above, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[2J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Complete, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[3J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Scrollback, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            // Invalid and ignored by the handler
            for c in b"\x1B[>0J" {
                s.next(*c);
            }
            assert_eq!(csi::EraseDisplay::Scrollback, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
    }

    // test "stream: DECEL, DECSEL" { ... }
    #[test]
    fn stream_decel_decsel() {
        struct H {
            mode: Option<csi::EraseLine>,
            protected: Option<bool>,
        }
        impl Handler for H {
            fn erase_line_right(&mut self, protected: bool) {
                self.mode = Some(csi::EraseLine::Right);
                self.protected = Some(protected);
            }
            fn erase_line_left(&mut self, protected: bool) {
                self.mode = Some(csi::EraseLine::Left);
                self.protected = Some(protected);
            }
            fn erase_line_complete(&mut self, protected: bool) {
                self.mode = Some(csi::EraseLine::Complete);
                self.protected = Some(protected);
            }
            fn erase_line_right_unless_pending_wrap(&mut self, protected: bool) {
                self.mode = Some(csi::EraseLine::RightUnlessPendingWrap);
                self.protected = Some(protected);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H {
            mode: None,
            protected: None,
        }));
        {
            for c in b"\x1B[?K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?0K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?1K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Left, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[?2K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Complete, s.handler.mode.unwrap());
            assert!(s.handler.protected.unwrap());
        }

        {
            for c in b"\x1B[K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[0K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[1K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Left, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            for c in b"\x1B[2K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Complete, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
        {
            // Invalid and ignored by the handler
            for c in b"\x1B[<1K" {
                s.next(*c);
            }
            assert_eq!(csi::EraseLine::Complete, s.handler.mode.unwrap());
            assert!(!s.handler.protected.unwrap());
        }
    }

    // test "stream: DECSCUSR" { ... }
    #[test]
    fn stream_decscusr() {
        struct H {
            style: Option<ansi::CursorStyle>,
        }
        impl Handler for H {
            fn cursor_style(&mut self, v: ansi::CursorStyle) {
                self.style = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));
        s.next_slice(b"\x1B[ q");
        assert!(s.handler.style.unwrap() == ansi::CursorStyle::Default);

        s.next_slice(b"\x1B[1 q");
        assert!(s.handler.style.unwrap() == ansi::CursorStyle::BlinkingBlock);

        // Invalid and ignored by the handler
        s.next_slice(b"\x1B[?0 q");
        assert!(s.handler.style.unwrap() == ansi::CursorStyle::BlinkingBlock);
    }

    // test "stream: DECSCUSR without space" { ... }
    #[test]
    fn stream_decscusr_without_space() {
        struct H {
            style: Option<ansi::CursorStyle>,
        }
        impl Handler for H {
            fn cursor_style(&mut self, v: ansi::CursorStyle) {
                self.style = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));
        s.next_slice(b"\x1B[q");
        assert!(s.handler.style.is_none());

        s.next_slice(b"\x1B[1q");
        assert!(s.handler.style.is_none());
    }

    // test "stream: XTSHIFTESCAPE" { ... }
    #[test]
    fn stream_xtshiftescape() {
        struct H {
            escape: Option<bool>,
        }
        impl Handler for H {
            fn mouse_shift_capture(&mut self, capture: bool) {
                self.escape = Some(capture);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { escape: None }));
        s.next_slice(b"\x1B[>2s");
        assert!(s.handler.escape.is_none());

        s.next_slice(b"\x1B[>s");
        assert!(!s.handler.escape.unwrap());

        s.next_slice(b"\x1B[>0s");
        assert!(!s.handler.escape.unwrap());

        s.next_slice(b"\x1B[>1s");
        assert!(s.handler.escape.unwrap());

        // Invalid and ignored by the handler
        s.next_slice(b"\x1B[1 s");
        assert!(s.handler.escape.unwrap());
    }

    // test "stream: change window title with invalid utf-8" { ... }
    #[test]
    fn stream_change_window_title_with_invalid_utf8() {
        struct H {
            seen: bool,
        }
        impl Handler for H {
            fn window_title(&mut self, _v: WindowTitle) {
                self.seen = true;
            }
        }

        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H { seen: false }));
            s.next_slice(b"\x1b]2;abc\x1b\\");
            assert!(s.handler.seen);
        }

        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H { seen: false }));
            s.next_slice(b"\x1b]2;abc\xc0\x1b\\");
            assert!(!s.handler.seen);
        }
    }

    // test "stream: osc 52 large payload in chunks" { ... }
    #[test]
    fn stream_osc_52_large_payload_in_chunks() {
        struct H {
            data: Vec<u8>,
            kind: u8,
            // PORT NOTE (W3 seam): the reference handler also records
            // `terminator: ?osc.Terminator = null` from
            // `value.terminator` and asserts `osc.Terminator.st` below —
            // the `terminator` field on the `clipboard_contents` action
            // lands with `e03475c0c` (B-1/W3). Carry the field + assert
            // when W3 ports that commit.
            count: usize,
        }
        impl Handler for H {
            // .clipboard_contents => { ... }
            fn clipboard_contents(&mut self, v: ClipboardContents) {
                self.count += 1;
                self.kind = v.kind;
                // self.terminator = value.terminator;  (W3 seam, see above)
                // self.data.appendSlice(testing.allocator, value.data) catch @panic("OOM");
                self.data.extend_from_slice(v.data);
            }
        }

        // A payload that far exceeds the fixed buffer so the allocating
        // capture is used, fed in chunk sizes that don't align with the
        // sequence so the bulk path sees every kind of boundary.
        let prefix = b"\x1b]52;c;";
        let payload_len: usize = 150_000;
        let mut input: Vec<u8> = Vec::new();
        input.extend_from_slice(prefix);
        // for (0..payload_len) |i| try input.append(alloc, 'A' + @as(u8, @intCast(i % 26)));
        for i in 0..payload_len {
            input.push(b'A' + u8::try_from(i % 26).unwrap());
        }
        input.extend_from_slice(b"\x1b\\");

        // var s: Stream(H) = .init(.{ .handler = .{}, .allocator = alloc });
        let mut s: Stream<H> = Stream::init(Options {
            handler: H {
                data: Vec::new(),
                kind: 0,
                count: 0,
            },
            allocator: Some(testing_allocator()),
            continuation_max_bytes: None,
        });

        let mut i: usize = 0;
        while i < input.len() {
            // const end = @min(i + 4093, input.items.len);
            let end = (i + 4093).min(input.len());
            s.next_slice(&input[i..end]);
            i = end;
        }

        assert_eq!(1usize, s.handler.count);
        assert_eq!(b'c', s.handler.kind);
        // try testing.expectEqual(osc.Terminator.st, s.handler.terminator.?);
        // (W3 seam — see the handler PORT NOTE above.)
        assert_eq!(
            &input[prefix.len()..prefix.len() + payload_len],
            s.handler.data.as_slice()
        );

        // defer s.parser.deinit();
        s.parser.deinit();
    }

    // test "stream: osc bulk path matches per-byte path" { ... }
    #[test]
    fn stream_osc_bulk_path_matches_per_byte_path() {
        // Records every dispatch relevant to OSC processing in a
        // normalized text form so streams fed different ways can be
        // compared byte-for-byte.
        //
        // PORT NOTE: the reference handler carries `alloc: Allocator` for
        // `std.fmt.allocPrint`; the ported journal is an owned `Vec<u8>`
        // built by direct byte appends (`{s}` prints RAW bytes — kept
        // byte-exact rather than routed through a lossy UTF-8 format).
        #[derive(Default)]
        struct H {
            journal: Vec<u8>,
        }
        impl H {
            // fn record(self: *Self, comptime fmt: []const u8, args: anytype) void { ... }
            fn record(&mut self, s: &[u8]) {
                self.journal.extend_from_slice(s);
            }
        }
        impl Handler for H {
            // .clipboard_contents => self.record(
            //     "clip kind={c} term={s} data={s}\n",
            //     .{ value.kind, @tagName(value.terminator), value.data },
            // ),
            // PORT NOTE (W3 seam): ` term={s}` (`@tagName(value.terminator)`)
            // is omitted until `e03475c0c` (B-1/W3) lands the `terminator`
            // field on the clipboard_contents action; both the per-byte and
            // bulk streams journal identically without it, preserving the
            // equality property this test checks.
            fn clipboard_contents(&mut self, v: ClipboardContents) {
                let mut msg: Vec<u8> = Vec::new();
                msg.extend_from_slice(b"clip kind=");
                msg.push(v.kind);
                msg.extend_from_slice(b" data=");
                msg.extend_from_slice(v.data);
                msg.push(b'\n');
                self.record(&msg);
            }
            // .kitty_clipboard => self.record(
            //     "kitty meta={s} payload={?s}\n",
            //     .{ value.metadata, value.payload },
            // ),
            // PORT NOTE (W3 seam): the `kitty_clipboard` stream action does
            // not exist yet (B-1/W3, the `6959fd46c` dispatch chain); rustty
            // still routes `Command::KittyClipboardProtocol` to the ignored
            // `osc_dispatch` arm, so BOTH the per-byte and bulk paths journal
            // nothing for the 5522 case below — the equality property is
            // preserved. Add the journal arm when W3 lands the dispatch.
            //
            // .window_title => self.record("title {s}\n", .{value.title}),
            fn window_title(&mut self, v: WindowTitle) {
                let mut msg: Vec<u8> = Vec::new();
                msg.extend_from_slice(b"title ");
                msg.extend_from_slice(v.title);
                msg.push(b'\n');
                self.record(&msg);
            }
            // Normalize prints to per-codepoint so the per-byte
            // and slice paths journal identically.
            // .print => self.record("print {u}\n", .{value.cp}),
            fn print(&mut self, cp: u32) {
                let mut msg: Vec<u8> = Vec::new();
                msg.extend_from_slice(b"print ");
                let mut buf = [0u8; 4];
                msg.extend_from_slice(char::from_u32(cp).unwrap().encode_utf8(&mut buf).as_bytes());
                msg.push(b'\n');
                self.record(&msg);
            }
            // .print_slice => for (value.cps) |cp| self.record(
            //     "print {u}\n",
            //     .{@as(u21, @intCast(cp))},
            // ),
            fn print_slice(&mut self, cps: &[u32]) {
                for &cp in cps {
                    self.print(cp);
                }
            }
        }

        // const cases = [_][]const u8{ ... };
        let mut cases: Vec<Vec<u8>> = vec![
            b"\x1b]52;c;aGVsbG8=\x1b\\".to_vec(),
            b"\x1b]52;c;aGVsbG8=\x07".to_vec(),
            b"\x1b]52;;aGVsbG8=\x07".to_vec(),
            // Ignored C0 byte embedded in the payload.
            b"\x1b]52;c;aGVs\x01bG8=\x07".to_vec(),
            // CAN and SUB aborts.
            b"\x1b]52;c;aGVsbG8=\x18".to_vec(),
            b"\x1b]52;c;aGVsbG8=\x1a".to_vec(),
            // C1 byte embedded in the payload is data.
            b"\x1b]0;ab\x9ccd\x07".to_vec(),
            // Terminated with trailing printable text.
            b"\x1b]0;a title\x07x".to_vec(),
            // Back-to-back sequences.
            b"\x1b]2;another title\x1b\\\x1b]0;t2\x07".to_vec(),
            b"\x1b]5522;type=write;aGVsbG8=\x1b\\".to_vec(),
            // Invalid OSC number.
            b"\x1b]999;junk\x07".to_vec(),
        ];
        // Exceeds the fixed buffer: allocating capture.
        // "\x1b]52;c;" ++ "y" ** 3000 ++ "\x1b\\",
        {
            let mut c = b"\x1b]52;c;".to_vec();
            c.extend(std::iter::repeat(b'y').take(3000));
            c.extend_from_slice(b"\x1b\\");
            cases.push(c);
        }
        // Exceeds the fixed buffer: overflow, no dispatch.
        // "\x1b]0;" ++ "x" ** 3000 ++ "\x07",
        {
            let mut c = b"\x1b]0;".to_vec();
            c.extend(std::iter::repeat(b'x').take(3000));
            c.extend_from_slice(b"\x07");
            cases.push(c);
        }

        for case in &cases {
            // Reference: byte-at-a-time.
            let mut r: Stream<H> = Stream::init(Options {
                handler: H::default(),
                allocator: Some(testing_allocator()),
                continuation_max_bytes: None,
            });
            for &c in case.iter() {
                r.next(c);
            }

            // The whole slice at once.
            {
                let mut s: Stream<H> = Stream::init(Options {
                    handler: H::default(),
                    allocator: Some(testing_allocator()),
                    continuation_max_bytes: None,
                });
                s.next_slice(case);
                // try testing.expectEqualSlices(u8, ref.handler.journal.items, s.handler.journal.items);
                assert_eq!(r.handler.journal, s.handler.journal);
                // defer s.parser.deinit();
                s.parser.deinit();
            }

            // Split into two slices at every possible boundary.
            for split in 0..=case.len() {
                let mut s: Stream<H> = Stream::init(Options {
                    handler: H::default(),
                    allocator: Some(testing_allocator()),
                    continuation_max_bytes: None,
                });
                s.next_slice(&case[..split]);
                s.next_slice(&case[split..]);
                assert_eq!(r.handler.journal, s.handler.journal);
                // defer s.parser.deinit();
                s.parser.deinit();
            }

            // defer ref.parser.deinit();
            r.parser.deinit();
        }
    }

    // test "stream: insert characters" { ... }
    #[test]
    fn stream_insert_characters() {
        struct H {
            called: bool,
        }
        impl Handler for H {
            fn insert_blanks(&mut self, _count: usize) {
                self.called = true;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
        for c in b"\x1B[42@" {
            s.next(*c);
        }
        assert!(s.handler.called);

        s.handler.called = false;
        for c in b"\x1B[?42@" {
            s.next(*c);
        }
        assert!(!s.handler.called);
    }

    // test "stream: insert characters explicit zero clamps to 1" { ... }
    #[test]
    fn stream_insert_characters_explicit_zero_clamps_to_1() {
        struct H {
            value: Option<usize>,
        }
        impl Handler for H {
            fn insert_blanks(&mut self, count: usize) {
                self.value = Some(count);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { value: None }));
        for c in b"\x1B[0@" {
            s.next(*c);
        }
        assert_eq!(1usize, s.handler.value.unwrap());
    }

    // test "stream: SCOSC" { ... }
    #[test]
    fn stream_scosc() {
        struct H {
            called: bool,
        }
        impl Handler for H {
            fn left_and_right_margin(&mut self, _v: Margin) {
                panic!("bad");
            }
            fn left_and_right_margin_ambiguous(&mut self) {
                self.called = true;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
        for c in b"\x1B[s" {
            s.next(*c);
        }
        assert!(s.handler.called);
    }

    // test "stream: SCORC" { ... }
    #[test]
    fn stream_scorc() {
        struct H {
            called: bool,
        }
        impl Handler for H {
            fn restore_cursor(&mut self) {
                self.called = true;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
        for c in b"\x1B[u" {
            s.next(*c);
        }
        assert!(s.handler.called);
    }

    // test "stream: too many csi params" { ... }
    #[test]
    fn stream_too_many_csi_params() {
        struct H {}
        impl Handler for H {
            fn cursor_right(&mut self, _v: CursorMovement) {
                unreachable!();
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H {}));
        s.next_slice(b"\x1B[1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1C");
    }

    // test "stream: csi param too long" { ... }
    #[test]
    fn stream_csi_param_too_long() {
        struct H {}
        impl Handler for H {}

        let mut s: Stream<H> = Stream::init(Options::with_handler(H {}));
        s.next_slice(b"\x1B[1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111C");
    }

    // test "stream: send report with CSI t" { ... }
    #[test]
    fn stream_send_report_with_csi_t() {
        struct H {
            style: Option<csi::SizeReportStyle>,
        }
        impl Handler for H {
            fn size_report(&mut self, v: csi::SizeReportStyle) {
                self.style = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));

        s.next_slice(b"\x1b[14t");
        assert_eq!(Some(csi::SizeReportStyle::Csi14T), s.handler.style);

        s.next_slice(b"\x1b[16t");
        assert_eq!(Some(csi::SizeReportStyle::Csi16T), s.handler.style);

        s.next_slice(b"\x1b[18t");
        assert_eq!(Some(csi::SizeReportStyle::Csi18T), s.handler.style);

        s.next_slice(b"\x1b[21t");
        assert_eq!(Some(csi::SizeReportStyle::Csi21T), s.handler.style);
    }

    // test "stream: invalid CSI t" { ... }
    #[test]
    fn stream_invalid_csi_t() {
        // PORT NOTE: the reference handler defines an (unused) `sendSizeReport`
        // method and a no-op `vt`; CSI 19 t is unimplemented so nothing fires.
        struct H {
            style: Option<csi::SizeReportStyle>,
        }
        impl Handler for H {}

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));

        s.next_slice(b"\x1b[19t");
        assert_eq!(None, s.handler.style);
    }

    // test "stream: CSI t push title" { ... }
    #[test]
    fn stream_csi_t_push_title() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_push(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[22;0t");
        assert_eq!(0u16, s.handler.index.unwrap());
    }

    // test "stream: CSI t push title with explicit window" { ... }
    #[test]
    fn stream_csi_t_push_title_with_explicit_window() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_push(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[22;2t");
        assert_eq!(0u16, s.handler.index.unwrap());
    }

    // test "stream: CSI t push title with explicit icon" { ... }
    #[test]
    fn stream_csi_t_push_title_with_explicit_icon() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_push(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[22;1t");
        assert_eq!(None, s.handler.index);
    }

    // test "stream: CSI t push title with index" { ... }
    #[test]
    fn stream_csi_t_push_title_with_index() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_push(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[22;0;5t");
        assert_eq!(5u16, s.handler.index.unwrap());
    }

    // test "stream: CSI t pop title" { ... }
    #[test]
    fn stream_csi_t_pop_title() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_pop(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[23;0t");
        assert_eq!(0u16, s.handler.index.unwrap());
    }

    // test "stream: CSI t pop title with explicit window" { ... }
    #[test]
    fn stream_csi_t_pop_title_with_explicit_window() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_pop(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[23;2t");
        assert_eq!(0u16, s.handler.index.unwrap());
    }

    // test "stream: CSI t pop title with explicit icon" { ... }
    #[test]
    fn stream_csi_t_pop_title_with_explicit_icon() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_pop(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[23;1t");
        assert_eq!(None, s.handler.index);
    }

    // test "stream: CSI t pop title with index" { ... }
    #[test]
    fn stream_csi_t_pop_title_with_index() {
        struct H {
            index: Option<u16>,
        }
        impl Handler for H {
            fn title_pop(&mut self, v: u16) {
                self.index = Some(v);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

        s.next_slice(b"\x1b[23;0;5t");
        assert_eq!(5u16, s.handler.index.unwrap());
    }

    // PORT NOTE: the next three tests record `?Action.Key` — *which* action
    // fired. The Rust port has no `Action.Key` tag enum (Action == Handler
    // trait), so each test declares a local `Observed` enum mirroring exactly
    // the `Action.Key` variants those tests can witness, and the handler sets
    // the field from whichever overridden method runs.

    // test "stream CSI W clear tab stops" { ... }
    #[test]
    fn stream_csi_w_clear_tab_stops() {
        #[derive(PartialEq, Eq, Debug)]
        #[allow(clippy::enum_variant_names)]
        enum Observed {
            TabClearCurrent,
            TabClearAll,
            TabSet,
            TabReset,
        }
        struct H {
            action: Option<Observed>,
        }
        impl Handler for H {
            fn tab_clear_current(&mut self) {
                self.action = Some(Observed::TabClearCurrent);
            }
            fn tab_clear_all(&mut self) {
                self.action = Some(Observed::TabClearAll);
            }
            fn tab_set(&mut self) {
                self.action = Some(Observed::TabSet);
            }
            fn tab_reset(&mut self) {
                self.action = Some(Observed::TabReset);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { action: None }));

        s.next_slice(b"\x1b[2W");
        assert_eq!(
            Observed::TabClearCurrent,
            *s.handler.action.as_ref().unwrap()
        );

        s.next_slice(b"\x1b[5W");
        assert_eq!(Observed::TabClearAll, *s.handler.action.as_ref().unwrap());
    }

    // test "stream CSI W tab set" { ... }
    #[test]
    fn stream_csi_w_tab_set() {
        #[derive(PartialEq, Eq, Debug)]
        #[allow(clippy::enum_variant_names)]
        enum Observed {
            TabClearCurrent,
            TabClearAll,
            TabSet,
            TabReset,
        }
        struct H {
            action: Option<Observed>,
        }
        impl Handler for H {
            fn tab_clear_current(&mut self) {
                self.action = Some(Observed::TabClearCurrent);
            }
            fn tab_clear_all(&mut self) {
                self.action = Some(Observed::TabClearAll);
            }
            fn tab_set(&mut self) {
                self.action = Some(Observed::TabSet);
            }
            fn tab_reset(&mut self) {
                self.action = Some(Observed::TabReset);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { action: None }));

        s.next_slice(b"\x1b[W");
        assert_eq!(Observed::TabSet, *s.handler.action.as_ref().unwrap());

        s.handler.action = None;
        s.next_slice(b"\x1b[0W");
        assert_eq!(Observed::TabSet, *s.handler.action.as_ref().unwrap());

        s.handler.action = None;
        s.next_slice(b"\x1b[>W");
        assert!(s.handler.action.is_none());

        s.handler.action = None;
        s.next_slice(b"\x1b[99W");
        assert!(s.handler.action.is_none());
    }

    // test "stream CSI ? W reset tab stops" { ... }
    #[test]
    fn stream_csi_question_w_reset_tab_stops() {
        #[derive(PartialEq, Eq, Debug)]
        #[allow(clippy::enum_variant_names)]
        enum Observed {
            TabClearCurrent,
            TabClearAll,
            TabSet,
            TabReset,
        }
        struct H {
            action: Option<Observed>,
        }
        impl Handler for H {
            fn tab_clear_current(&mut self) {
                self.action = Some(Observed::TabClearCurrent);
            }
            fn tab_clear_all(&mut self) {
                self.action = Some(Observed::TabClearAll);
            }
            fn tab_set(&mut self) {
                self.action = Some(Observed::TabSet);
            }
            fn tab_reset(&mut self) {
                self.action = Some(Observed::TabReset);
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { action: None }));

        s.next_slice(b"\x1b[?2W");
        assert!(s.handler.action.is_none());

        s.next_slice(b"\x1b[?5W");
        assert_eq!(Observed::TabReset, *s.handler.action.as_ref().unwrap());

        // Invalid and ignored by the handler
        s.handler.action = None;
        s.next_slice(b"\x1b[?1;2;3W");
        assert!(s.handler.action.is_none());
    }

    // test "stream: SGR with 17+ parameters for underline color" { ... }
    #[test]
    fn stream_sgr_with_17_plus_parameters_for_underline_color() {
        // PORT NOTE: `sgr::Attribute<'a>` borrows; the reference stores `attrs`
        // but only asserts `called`. We record `called` only (the stored attr
        // is observationally unused in the assertion).
        struct H {
            called: bool,
        }
        impl Handler for H {
            fn set_attribute(&mut self, _attr: sgr::Attribute) {
                self.called = true;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));

        // Kakoune-style SGR with underline color as 17th parameter
        // This tests the fix where param 17 was being dropped
        s.next_slice(b"\x1b[4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136;0m");
        assert!(s.handler.called);
    }

    // test "stream: tab clear with overflowing param" { ... }
    #[test]
    fn stream_tab_clear_with_overflowing_param() {
        // Regression test for a fuzz crash: CSI with a parameter value that
        // saturates to 65535 (u16 max) causes @enumFromInt to panic when
        // converting to TabClear (enum(u8)).
        struct H {
            called: bool,
        }
        impl Handler for H {
            fn tab_clear_current(&mut self) {
                self.called = true;
            }
            fn tab_clear_all(&mut self) {
                self.called = true;
            }
        }

        let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
        // This is the exact input from the fuzz crash (minus the mode byte):
        // CSI with a huge numeric param that saturates to 65535, followed by 'g'.
        s.next_slice(b"\x1b[388888888888888888888888888888888888g\x1b[0m");
    }

    // ---------------------------------------------------------------------
    // Phase-4 A-3 (68beeeb3f + a69a591af): continuation tracking + ground
    // detection.
    // ---------------------------------------------------------------------

    use crate::stream_continuation::FixedIoWriter;
    #[cfg(debug_assertions)]
    use crate::stream_continuation::{tracker_tw_end, tracker_tw_error_always, TrackerFp};

    /// `testing.allocator` for the `Options.allocator` gate: the tests only
    /// need the allocator's *presence* (the Rust `Tracker` allocates on the
    /// global allocator; see its PORT NOTE), backed by the process-global zig
    /// allocator exactly as `Terminal::vt_stream` does.
    static CONTINUATION_TEST_ALLOC: rustty_lib::allocator::GlobalZigAllocator =
        rustty_lib::allocator::GlobalZigAllocator;

    fn testing_allocator() -> rustty_lib::allocator::ZigAllocatorRef<'static> {
        rustty_lib::allocator::ZigAllocatorRef::new(&CONTINUATION_TEST_ALLOC)
    }

    // const ContinuationTestHandler = struct { ... };
    //
    // PORT NOTE: the reference handler's `vt` switch ends in
    // `else => self.committed += 1`. Rust cannot express a catch-all over the
    // `Handler` trait's per-action methods, so the `else` arm is expanded into
    // explicit overrides of every other action the continuation test corpora
    // can produce (`bell`, `set_attribute`, `window_title`); the assertions
    // (absolute counts and source/restored equalities) validate the coverage.
    struct ContinuationTestHandler {
        committed: usize,
        apc_active: bool,
        apc_buf: [u8; 256],
        apc_len: usize,
        dcs_active: bool,
    }

    impl Default for ContinuationTestHandler {
        fn default() -> ContinuationTestHandler {
            ContinuationTestHandler {
                committed: 0,
                apc_active: false,
                apc_buf: [0; 256],
                apc_len: 0,
                dcs_active: false,
            }
        }
    }

    impl Handler for ContinuationTestHandler {
        // pub fn deinit(_: *@This()) void {}
        fn deinit(&mut self) {}

        // .apc_start => self.apc_active = true,
        fn apc_start(&mut self) {
            self.apc_active = true;
        }
        // .apc_put => { self.apc_buf[self.apc_len] = value; self.apc_len += 1; },
        fn apc_put(&mut self, code: u8) {
            self.apc_buf[self.apc_len] = code;
            self.apc_len += 1;
        }
        // .apc_put_slice => { @memcpy(...); self.apc_len += value.bytes.len; },
        fn apc_put_slice(&mut self, bytes: &[u8]) {
            self.apc_buf[self.apc_len..self.apc_len + bytes.len()].copy_from_slice(bytes);
            self.apc_len += bytes.len();
        }
        // .dcs_hook => self.dcs_active = true,
        fn dcs_hook(&mut self, _dcs: parser::DCS) {
            self.dcs_active = true;
        }
        // .dcs_put => {},
        fn dcs_put(&mut self, _code: u8) {}
        // .apc_end => { ...; self.committed += 1; },
        fn apc_end(&mut self, _v: ApcEnd) {
            self.apc_active = false;
            self.apc_len = 0;
            self.committed += 1;
        }
        // .dcs_unhook => { self.dcs_active = false; self.committed += 1; },
        fn dcs_unhook(&mut self) {
            self.dcs_active = false;
            self.committed += 1;
        }
        // .print => self.committed += 1,
        fn print(&mut self, _cp: u32) {
            self.committed += 1;
        }
        // .print_slice => self.committed += value.cps.len,
        fn print_slice(&mut self, cps: &[u32]) {
            self.committed += cps.len();
        }
        // .print_repeat => self.committed += value,
        fn print_repeat(&mut self, count: usize) {
            self.committed += count;
        }
        // else => self.committed += 1, (expanded; see the PORT NOTE above)
        fn bell(&mut self) {
            self.committed += 1;
        }
        fn set_attribute(&mut self, _attr: sgr::Attribute) {
            self.committed += 1;
        }
        fn window_title(&mut self, _v: WindowTitle) {
            self.committed += 1;
        }
    }

    // const ContinuationNullHandler = struct { ... };
    #[derive(Default)]
    struct ContinuationNullHandler;
    impl Handler for ContinuationNullHandler {}

    // test "stream: nextSliceUntilGround stops at the earliest boundary"
    #[test]
    fn stream_next_slice_until_ground_stops_at_the_earliest_boundary() {
        let mut stream: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer stream.deinit();

        stream.next_slice(b"\x1b[31");
        assert!(!stream.ground());
        stream.handler.committed = 0;

        let input = b"mABC\x1b[";
        let consumed = stream.next_slice_until_ground(input).unwrap();
        assert_eq!(1, consumed);
        assert!(stream.ground());
        assert_eq!(1, stream.handler.committed);

        // The suffix was not inspected by the handler and can be processed after
        // the caller performs work at the boundary.
        stream.next_slice(&input[consumed..]);
        assert!(!stream.ground());
        assert_eq!(parser::State::CsiEntry, stream.parser.state);
        assert_eq!(4, stream.handler.committed);

        stream.deinit();
    }

    // test "stream: nextSliceUntilGround consumes all input without a boundary"
    #[test]
    fn stream_next_slice_until_ground_consumes_all_input_without_a_boundary() {
        let mut stream: Stream<ContinuationNullHandler> =
            Stream::init(Options::with_handler(ContinuationNullHandler));
        // defer stream.deinit();

        assert_eq!(Some(0), stream.next_slice_until_ground(b"unprocessed"));
        assert!(stream.ground());

        stream.next_slice(b"\x1b[");
        assert_eq!(None, stream.next_slice_until_ground(b"123"));
        assert!(!stream.ground());

        // A boundary on the final byte is distinguishable from exhausting the
        // input while still pending.
        assert_eq!(Some(1), stream.next_slice_until_ground(b"m"));
        assert!(stream.ground());

        stream.deinit();
    }

    // test "stream: nextSliceUntilGround handles UTF-8 boundaries"
    #[test]
    fn stream_next_slice_until_ground_handles_utf8_boundaries() {
        // A completed codepoint is committed synchronously, and the printable
        // suffix remains untouched.
        let mut valid: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer valid.deinit();
        valid.next_slice(&[0xF0]);
        let valid_input = [0x9Fu8, 0x98, 0x84, b'X'];
        assert_eq!(Some(3), valid.next_slice_until_ground(&valid_input));
        assert!(valid.ground());
        assert_eq!(1, valid.handler.committed);

        // A malformed continuation emits the replacement codepoint and retries
        // the same byte. Ground is observed after that complete byte operation.
        let mut malformed: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer malformed.deinit();
        malformed.next_slice(&[0xE0, 0xA0]);
        assert_eq!(Some(1), malformed.next_slice_until_ground(b"A!"));
        assert!(malformed.ground());
        assert_eq!(2, malformed.handler.committed);

        // If the retried byte is ESC, the stream has begun VT state at the end of
        // that byte and must continue to the following ground boundary.
        let mut retry_escape: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer retry_escape.deinit();
        retry_escape.next_slice(&[0xE0, 0xA0]);
        assert_eq!(Some(3), retry_escape.next_slice_until_ground(b"\x1b[mX"));
        assert!(retry_escape.ground());

        retry_escape.deinit();
        malformed.deinit();
        valid.deinit();
    }

    // test "stream: nextSliceUntilGround handles aborts and bulk strings"
    #[test]
    fn stream_next_slice_until_ground_handles_aborts_and_bulk_strings() {
        let mut aborted: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer aborted.deinit();
        aborted.next_slice(b"\x1b[123");
        assert_eq!(Some(1), aborted.next_slice_until_ground(&[0x18, b'X']));
        assert!(aborted.ground());

        let mut apc: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer apc.deinit();
        apc.next_slice(b"\x1b_Gseed");
        let mut input = [0u8; 131];
        input[0..128].fill(b'a');
        input[128..130].copy_from_slice(b"\x1b\\");
        input[130] = b'X';
        assert_eq!(Some(130), apc.next_slice_until_ground(&input));
        assert!(apc.ground());

        apc.deinit();
        aborted.deinit();
    }

    // test "stream: nextSliceUntilGround tracks only the consumed prefix"
    #[test]
    fn stream_next_slice_until_ground_tracks_only_the_consumed_prefix() {
        let mut stream: Stream<ContinuationNullHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationNullHandler,
            continuation_max_bytes: Some(64),
        });
        // defer stream.deinit();

        stream.next_slice(b"\x1b[31");
        assert_eq!(Some(1), stream.next_slice_until_ground(b"mX\x1b["));

        let mut buf = [0u8; 64];
        let mut writer = FixedIoWriter::new(&mut buf);
        stream.write_continuation(&mut writer).unwrap();
        assert_eq!(0, writer.buffered().len());

        stream.next_slice(b"\x1b[");
        assert_eq!(None, stream.next_slice_until_ground(b"123"));
        let mut pending_buf = [0u8; 64];
        let mut pending_writer = FixedIoWriter::new(&mut pending_buf);
        stream.write_continuation(&mut pending_writer).unwrap();
        assert_eq!(b"\x1b[123".as_slice(), pending_writer.buffered());

        stream.deinit();
    }

    // test "stream: continuation lifecycle"
    #[test]
    #[cfg_attr(
        not(debug_assertions),
        ignore = "the FailingAllocator arm needs the TRACKER_TW tripwire, which \
                  (like Zig's builtin.is_test gate) is debug-only"
    )]
    fn stream_continuation_lifecycle() {
        let mut disabled: Stream<ContinuationTestHandler> =
            Stream::init(Options::with_handler(ContinuationTestHandler::default()));
        // defer disabled.deinit();
        let mut disabled_buf = [0u8; 1];
        let mut disabled_writer = FixedIoWriter::new(&mut disabled_buf);
        assert!(matches!(
            disabled.write_continuation(&mut disabled_writer),
            Err(WriteContinuationError::ContinuationDisabled)
        ));

        let mut zero_capacity: Stream<ContinuationTestHandler> = Stream::init(Options {
            handler: ContinuationTestHandler::default(),
            allocator: None,
            continuation_max_bytes: Some(0),
        });
        // defer zero_capacity.deinit();
        let mut zero_capacity_buf = [0u8; 1];
        let mut zero_capacity_writer = FixedIoWriter::new(&mut zero_capacity_buf);
        assert!(matches!(
            zero_capacity.write_continuation(&mut zero_capacity_writer),
            Err(WriteContinuationError::ContinuationDisabled)
        ));
        let mut no_allocator: Stream<ContinuationTestHandler> = Stream::init(Options {
            handler: ContinuationTestHandler::default(),
            allocator: None,
            continuation_max_bytes: Some(64),
        });
        // defer no_allocator.deinit();
        let mut no_allocator_buf = [0u8; 1];
        let mut no_allocator_writer = FixedIoWriter::new(&mut no_allocator_buf);
        assert!(matches!(
            no_allocator.write_continuation(&mut no_allocator_writer),
            Err(WriteContinuationError::ContinuationDisabled)
        ));

        let mut tracked: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(64),
        });
        // defer tracked.deinit();

        tracked.next_slice(b"complete input");
        let mut empty_buf = [0u8; 1];
        let mut empty_writer = FixedIoWriter::new(&mut empty_buf);
        tracked.write_continuation(&mut empty_writer).unwrap();
        assert_eq!(0, empty_writer.end);

        tracked.next_slice(b"\x1b[");
        let mut short_buf = [0u8; 1];
        let mut short_writer = FixedIoWriter::new(&mut short_buf);
        assert!(matches!(
            tracked.write_continuation(&mut short_writer),
            Err(WriteContinuationError::WriteFailed(_))
        ));

        // var failing = testing.FailingAllocator.init(testing.allocator, .{
        //     .fail_index = 0,
        // });
        // PORT NOTE: FailingAllocator → the TRACKER_TW tripwire seam.
        #[cfg(debug_assertions)]
        {
            tracker_tw_error_always(TrackerFp::Alloc);
            let mut failing_stream: Stream<ContinuationNullHandler> = Stream::init(Options {
                allocator: Some(testing_allocator()),
                handler: ContinuationNullHandler,
                continuation_max_bytes: Some(64),
            });
            // defer failing_stream.deinit();
            failing_stream.next_slice(b"\x1b[");
            let mut unavailable_buf = [0u8; 1];
            let mut unavailable_writer = FixedIoWriter::new(&mut unavailable_buf);
            assert!(matches!(
                failing_stream.write_continuation(&mut unavailable_writer),
                Err(WriteContinuationError::ContinuationUnavailable)
            ));
            tracker_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            failing_stream.deinit();
        }

        tracked.deinit();
        no_allocator.deinit();
        zero_capacity.deinit();
        disabled.deinit();
    }

    // test "stream: continuation suffixes are replay safe"
    #[test]
    fn stream_continuation_suffixes_are_replay_safe() {
        struct Case {
            input: &'static [u8],
            expected: &'static [u8],
        }
        let cases = [
            Case {
                input: b"text\x1b",
                expected: b"\x1b",
            },
            Case {
                input: b"text\x1b[12;",
                expected: b"\x1b[12;",
            },
            Case {
                input: b"text\x1b[1\x07;2",
                expected: b"\x1b[1;2",
            },
            Case {
                input: b"text\x1b]2;hello",
                expected: b"\x1b]2;hello",
            },
            Case {
                input: b"text\x1b_Gabc",
                expected: b"\x1b_Gabc",
            },
            Case {
                input: b"text\x1bP+qabc",
                expected: b"\x1bP+qabc",
            },
            Case {
                input: b"text\xE0\xA0\xF0",
                expected: b"\xF0",
            },
            Case {
                input: b"text\x1b[12\x1b",
                expected: b"\x1b",
            },
            Case {
                input: b"text\x1b[12\x9D2;title",
                expected: b"\x1b[12\x9D2;title",
            },
        ];

        for case in &cases {
            let mut stream: Stream<ContinuationTestHandler> = Stream::init(Options {
                allocator: Some(testing_allocator()),
                handler: ContinuationTestHandler::default(),
                continuation_max_bytes: Some(1024),
            });
            // defer stream.deinit();
            stream.next_slice(case.input);

            let mut buf = [0u8; 1024];
            let mut writer = FixedIoWriter::new(&mut buf);
            stream.write_continuation(&mut writer).unwrap();
            assert_eq!(case.expected, writer.buffered());

            stream.deinit();
        }
    }

    // test "stream: continuation reconstructs every unfinished VT state"
    #[test]
    fn stream_continuation_reconstructs_every_unfinished_vt_state() {
        struct Case {
            input: &'static [u8],
            state: parser::State,
        }
        let cases = [
            Case {
                input: b"\x1b",
                state: parser::State::Escape,
            },
            Case {
                input: b"\x1b(",
                state: parser::State::EscapeIntermediate,
            },
            Case {
                input: b"\x1b[",
                state: parser::State::CsiEntry,
            },
            Case {
                input: b"\x1b[1",
                state: parser::State::CsiParam,
            },
            Case {
                input: b"\x1b[1$",
                state: parser::State::CsiIntermediate,
            },
            Case {
                input: b"\x1b[:",
                state: parser::State::CsiIgnore,
            },
            Case {
                input: b"\x1bP",
                state: parser::State::DcsEntry,
            },
            Case {
                input: b"\x1bP1",
                state: parser::State::DcsParam,
            },
            Case {
                input: b"\x1bP1$",
                state: parser::State::DcsIntermediate,
            },
            Case {
                input: b"\x1bP1q",
                state: parser::State::DcsPassthrough,
            },
            Case {
                input: b"\x1bP:",
                state: parser::State::DcsIgnore,
            },
            Case {
                input: b"\x1b]2;title",
                state: parser::State::OscString,
            },
            Case {
                input: b"\x1b_Gpayload",
                state: parser::State::SosPmApcString,
            },
        ];

        for case in &cases {
            let mut source: Stream<ContinuationTestHandler> = Stream::init(Options {
                allocator: Some(testing_allocator()),
                handler: ContinuationTestHandler::default(),
                continuation_max_bytes: Some(1024),
            });
            // defer source.deinit();
            source.next_slice(case.input);
            assert_eq!(case.state, source.parser.state);

            let mut buf = [0u8; 1024];
            let mut writer = FixedIoWriter::new(&mut buf);
            source.write_continuation(&mut writer).unwrap();

            let mut restored: Stream<ContinuationTestHandler> = Stream::init(Options {
                allocator: Some(testing_allocator()),
                handler: ContinuationTestHandler::default(),
                continuation_max_bytes: Some(1024),
            });
            // defer restored.deinit();
            restored.next_slice(writer.buffered());
            assert_eq!(0, restored.handler.committed);
            assert_eq!(source.parser.state, restored.parser.state);
            assert_eq!(source.utf8decoder.state, restored.utf8decoder.state);

            restored.deinit();
            source.deinit();
        }

        // Parser ground is still unfinished while the UTF-8 decoder is waiting
        // for the remaining bytes of a codepoint.
        let mut utf8: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(4),
        });
        // defer utf8.deinit();
        utf8.next(0xF0);
        assert_eq!(parser::State::Ground, utf8.parser.state);
        assert!(utf8.utf8decoder.state != 0);
        let mut utf8_buf = [0u8; 4];
        let mut utf8_writer = FixedIoWriter::new(&mut utf8_buf);
        utf8.write_continuation(&mut utf8_writer).unwrap();
        assert_eq!(&[0xF0u8], utf8_writer.buffered());

        utf8.deinit();
    }

    // test "stream: continuation is chunking-independent and idempotent"
    #[test]
    fn stream_continuation_is_chunking_independent_and_idempotent() {
        let input = b"committed\x1b[1\x07;2";

        let mut bulk: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer bulk.deinit();
        bulk.next_slice(input);

        let mut scalar: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer scalar.deinit();
        for &c in input {
            scalar.next(c);
        }

        let mut bulk_buf = [0u8; 1024];
        let mut bulk_writer = FixedIoWriter::new(&mut bulk_buf);
        bulk.write_continuation(&mut bulk_writer).unwrap();
        let mut scalar_buf = [0u8; 1024];
        let mut scalar_writer = FixedIoWriter::new(&mut scalar_buf);
        scalar.write_continuation(&mut scalar_writer).unwrap();
        assert_eq!(bulk_writer.buffered(), scalar_writer.buffered());

        let mut restored: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer restored.deinit();
        restored.next_slice(bulk_writer.buffered());
        assert_eq!(0, restored.handler.committed);

        let mut restored_buf = [0u8; 1024];
        let mut restored_writer = FixedIoWriter::new(&mut restored_buf);
        restored.write_continuation(&mut restored_writer).unwrap();
        assert_eq!(bulk_writer.buffered(), restored_writer.buffered());

        bulk.handler.committed = 0;
        restored.handler.committed = 0;
        bulk.next_slice(b"mZ");
        restored.next(b'm');
        restored.next(b'Z');
        assert_eq!(bulk.handler.committed, restored.handler.committed);

        restored.deinit();
        scalar.deinit();
        bulk.deinit();
    }

    // test "stream: continuation rebuilds APC handler input"
    #[test]
    fn stream_continuation_rebuilds_apc_handler_input() {
        let mut source: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer source.deinit();
        source.next_slice(b"committed\x1b_Gabc");

        let mut continuation = [0u8; 1024];
        let mut writer = FixedIoWriter::new(&mut continuation);
        source.write_continuation(&mut writer).unwrap();

        let mut restored: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer restored.deinit();
        restored.next_slice(writer.buffered());
        assert_eq!(0, restored.handler.committed);
        assert!(restored.handler.apc_active);
        assert_eq!(
            &source.handler.apc_buf[0..source.handler.apc_len],
            &restored.handler.apc_buf[0..restored.handler.apc_len],
        );

        source.handler.committed = 0;
        restored.handler.committed = 0;
        source.next_slice(b"\x1b\\");
        restored.next_slice(b"\x1b\\");
        assert_eq!(source.handler.committed, restored.handler.committed);
        assert!(!source.handler.apc_active);
        assert!(!restored.handler.apc_active);

        restored.deinit();
        source.deinit();
    }

    // test "stream: continuation cap and recovery"
    #[test]
    fn stream_continuation_cap_and_recovery() {
        // The raw feed exceeds the cap, but only the unfinished three-byte
        // CSI suffix is retained.
        let mut seeded: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(4),
        });
        // defer seeded.deinit();
        seeded.next_slice(b"committed text\x1b[1");
        let mut seeded_buf = [0u8; 4];
        let mut seeded_writer = FixedIoWriter::new(&mut seeded_buf);
        seeded.write_continuation(&mut seeded_writer).unwrap();
        assert_eq!(b"\x1b[1".as_slice(), seeded_writer.buffered());

        let mut exceeded: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(4),
        });
        // defer exceeded.deinit();
        exceeded.next_slice(b"\x1b[123");
        let mut unavailable_buf = [0u8; 1];
        let mut unavailable_writer = FixedIoWriter::new(&mut unavailable_buf);
        assert!(matches!(
            exceeded.write_continuation(&mut unavailable_writer),
            Err(WriteContinuationError::ContinuationUnavailable)
        ));

        // Completing the CSI reaches ground and recovers without rebuilding the
        // Stream. A later unfinished sequence is tracked normally.
        exceeded.next_slice(b"mtext\x1b[");
        let mut recovered_buf = [0u8; 4];
        let mut recovered_writer = FixedIoWriter::new(&mut recovered_buf);
        exceeded.write_continuation(&mut recovered_writer).unwrap();
        assert_eq!(b"\x1b[".as_slice(), recovered_writer.buffered());

        // A fresh ESC seed also recovers broken tracking even when the stream
        // never reaches ground: the ESC abandons the previous unfinished
        // state and everything after it is retained.
        exceeded.next_slice(b"\x1b[123");
        let mut reexceeded_buf = [0u8; 1];
        let mut reexceeded_writer = FixedIoWriter::new(&mut reexceeded_buf);
        assert!(matches!(
            exceeded.write_continuation(&mut reexceeded_writer),
            Err(WriteContinuationError::ContinuationUnavailable)
        ));
        exceeded.next_slice(b"\x1b]0;");
        let mut seed_buf = [0u8; 4];
        let mut seed_writer = FixedIoWriter::new(&mut seed_buf);
        exceeded.write_continuation(&mut seed_writer).unwrap();
        assert_eq!(b"\x1b]0;".as_slice(), seed_writer.buffered());

        exceeded.deinit();
        seeded.deinit();
    }

    // test "stream: continuation spans multiple bulk feeds"
    #[test]
    fn stream_continuation_spans_multiple_bulk_feeds() {
        // An unfinished APC grows across feeds that contain no new seed.
        let mut apc: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer apc.deinit();
        apc.next_slice(b"text\x1b_Gab");
        apc.next_slice(b"cd");
        apc.next_slice(b"ef");
        let mut apc_buf = [0u8; 16];
        let mut apc_writer = FixedIoWriter::new(&mut apc_buf);
        apc.write_continuation(&mut apc_writer).unwrap();
        assert_eq!(b"\x1b_Gabcdef".as_slice(), apc_writer.buffered());

        // An incomplete UTF-8 sequence grows across feeds of its
        // continuation bytes.
        let mut utf8: Stream<ContinuationTestHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationTestHandler::default(),
            continuation_max_bytes: Some(1024),
        });
        // defer utf8.deinit();
        utf8.next_slice(b"text\xF0");
        utf8.next_slice(b"\x9F");
        let mut utf8_buf = [0u8; 4];
        let mut utf8_writer = FixedIoWriter::new(&mut utf8_buf);
        utf8.write_continuation(&mut utf8_writer).unwrap();
        assert_eq!(b"\xF0\x9F".as_slice(), utf8_writer.buffered());

        // A later feed with its own seed drops everything retained earlier.
        utf8.next_slice(b"\x98\x84 done \x1b[38;5");
        let mut seed_buf = [0u8; 8];
        let mut seed_writer = FixedIoWriter::new(&mut seed_buf);
        utf8.write_continuation(&mut seed_writer).unwrap();
        assert_eq!(b"\x1b[38;5".as_slice(), seed_writer.buffered());

        utf8.deinit();
        apc.deinit();
    }

    // test "stream: continuation exact cap and large unfinished string"
    #[test]
    fn stream_continuation_exact_cap_and_large_unfinished_string() {
        let mut exact: Stream<ContinuationNullHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationNullHandler,
            continuation_max_bytes: Some(5),
        });
        // defer exact.deinit();
        exact.next_slice(b"\x1b[123");
        let mut exact_buf = [0u8; 5];
        let mut exact_writer = FixedIoWriter::new(&mut exact_buf);
        exact.write_continuation(&mut exact_writer).unwrap();
        assert_eq!(b"\x1b[123".as_slice(), exact_writer.buffered());

        let payload_len = 12 * 1024;
        let mut input = vec![0u8; payload_len];
        input[0..3].copy_from_slice(b"\x1b_G");
        input[3..].fill(b'A');

        let mut large: Stream<ContinuationNullHandler> = Stream::init(Options {
            allocator: Some(testing_allocator()),
            handler: ContinuationNullHandler,
            continuation_max_bytes: Some(payload_len),
        });
        // defer large.deinit();
        large.next_slice(&input);
        let mut large_buf = vec![0u8; payload_len];
        let mut large_writer = FixedIoWriter::new(&mut large_buf);
        large.write_continuation(&mut large_writer).unwrap();
        assert_eq!(input.as_slice(), large_writer.buffered());

        large.deinit();
        exact.deinit();
    }

    // test "stream: continuation allocation failure recovers"
    #[test]
    #[cfg_attr(
        not(debug_assertions),
        ignore = "needs the TRACKER_TW tripwire (the FailingAllocator stand-in), \
                  which is debug-only"
    )]
    fn stream_continuation_allocation_failure_recovers() {
        #[cfg(debug_assertions)]
        {
            let mut stream: Stream<ContinuationNullHandler> = Stream::init(Options {
                allocator: Some(testing_allocator()),
                handler: ContinuationNullHandler,
                continuation_max_bytes: Some(16 * 1024),
            });
            // defer stream.deinit();

            let mut input = vec![0u8; 12 * 1024];
            input[0..2].copy_from_slice(b"\x1b[");
            input[2..].fill(b'1');

            // failing.fail_index = failing.alloc_index;
            tracker_tw_error_always(TrackerFp::Alloc);
            stream.next_slice(&input);
            let mut unavailable_buf = [0u8; 1];
            let mut unavailable_writer = FixedIoWriter::new(&mut unavailable_buf);
            assert!(matches!(
                stream.write_continuation(&mut unavailable_writer),
                Err(WriteContinuationError::ContinuationUnavailable)
            ));

            // failing.fail_index = std.math.maxInt(usize);
            tracker_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            stream.next(b'm');
            let mut recovered_buf = [0u8; 1];
            let mut recovered_writer = FixedIoWriter::new(&mut recovered_buf);
            stream.write_continuation(&mut recovered_writer).unwrap();
            assert_eq!(0, recovered_writer.end);

            stream.deinit();
        }
    }

    // test "stream: continuation every-byte cuts preserve future behavior"
    #[test]
    fn stream_continuation_every_byte_cuts_preserve_future_behavior() {
        let corpora: [&[u8]; 10] = [
            b"plain \xF0\x9F\x98\x84 utf8",
            b"bad \xE0\xA0\xF0\x9F\x98\x84 utf8",
            b"\x1b[1\x07;2mstyled\x1b[0m",
            b"\x1b]2;window title\x1b\\text",
            b"\x1bP$qm\x1b\\text",
            b"\x1b_Ga=q;payload\x1b\\text",
            b"\x1b_25a1;s\x1b\\text",
            b"\x1b]2;first\x1b\\\x1b_Gsecond",
            b"\x1b[12\x9D2;title\x1b\\text",
            b"\x1b[12\x18text\x1b[1\x1Atext",
        ];

        for corpus in &corpora {
            for cut in 0..corpus.len() + 1 {
                let mut source: Stream<ContinuationTestHandler> = Stream::init(Options {
                    allocator: Some(testing_allocator()),
                    handler: ContinuationTestHandler::default(),
                    continuation_max_bytes: Some(64 * 1024),
                });
                // defer source.deinit();
                source.next_slice(&corpus[0..cut]);

                let mut continuation = vec![0u8; 64 * 1024];
                let mut continuation_writer = FixedIoWriter::new(&mut continuation);
                source.write_continuation(&mut continuation_writer).unwrap();

                let mut restored: Stream<ContinuationTestHandler> = Stream::init(Options {
                    allocator: Some(testing_allocator()),
                    handler: ContinuationTestHandler::default(),
                    continuation_max_bytes: Some(64 * 1024),
                });
                // defer restored.deinit();
                restored.next_slice(continuation_writer.buffered());
                assert_eq!(0, restored.handler.committed);
                assert_eq!(source.handler.apc_active, restored.handler.apc_active);
                assert_eq!(source.handler.dcs_active, restored.handler.dcs_active);
                if source.handler.apc_active {
                    assert_eq!(
                        &source.handler.apc_buf[0..source.handler.apc_len],
                        &restored.handler.apc_buf[0..restored.handler.apc_len],
                    );
                }

                let mut reexport = vec![0u8; 64 * 1024];
                let mut reexport_writer = FixedIoWriter::new(&mut reexport);
                restored.write_continuation(&mut reexport_writer).unwrap();
                assert_eq!(continuation_writer.buffered(), reexport_writer.buffered());

                source.handler.committed = 0;
                restored.handler.committed = 0;
                source.next_slice(&corpus[cut..]);
                let mut offset = cut;
                // var partition = cut +% corpus.len +% 1;
                let mut partition = cut.wrapping_add(corpus.len()).wrapping_add(1);
                while offset < corpus.len() {
                    // partition = partition *% 1664525 +% 1013904223;
                    partition = partition.wrapping_mul(1664525).wrapping_add(1013904223);
                    // const len = @min(1 + partition % 7, corpus.len - offset);
                    let len = core::cmp::min(1 + partition % 7, corpus.len() - offset);
                    restored.next_slice(&corpus[offset..offset + len]);
                    offset += len;
                }
                assert_eq!(source.handler.committed, restored.handler.committed);
                assert_eq!(source.handler.apc_active, restored.handler.apc_active);
                assert_eq!(source.handler.dcs_active, restored.handler.dcs_active);

                let mut source_final = [0u8; 64];
                let mut source_final_writer = FixedIoWriter::new(&mut source_final);
                source.write_continuation(&mut source_final_writer).unwrap();
                let mut restored_final = [0u8; 64];
                let mut restored_final_writer = FixedIoWriter::new(&mut restored_final);
                restored
                    .write_continuation(&mut restored_final_writer)
                    .unwrap();
                assert_eq!(
                    source_final_writer.buffered(),
                    restored_final_writer.buffered()
                );

                restored.deinit();
                source.deinit();
            }
        }
    }
}
