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
//! `comptime debug or !build_options.simd` switch): `SIMD` defaults to `false`
//! (`cfg!(feature = "simd")`), so the scalar fallback — the reference's own
//! non-SIMD arm — runs unconditionally by default, and the SIMD arm is
//! exercised under `--features simd` (all 38 stream tests pass in both modes;
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

    /// `apc_end`
    fn apc_end(&mut self) {}

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
}

impl<'a, H: Handler> Stream<'a, H> {
    /// Initialize an allocation-free stream. This will preallocate various
    /// sizes as necessary and anything over that will be dropped. If you
    /// want to support more dynamic behavior use initAlloc instead.
    ///
    /// As a concrete example of something that requires heap allocation,
    /// consider OSC 52 (clipboard operations) which can be arbitrarily
    /// large.
    ///
    /// If you want to limit allocation size, use an allocator with
    /// a size limit with initAlloc.
    ///
    /// This takes ownership of the handler and will call deinit
    /// when the stream is deinitialized.
    // pub fn init(h: Handler) Self {
    pub fn init(h: H) -> Stream<'a, H> {
        // return .{ .handler = h, .parser = .init(), .utf8decoder = .{} };
        Stream {
            handler: h,
            parser: Parser::init(),
            utf8decoder: UTF8Decoder::new(),
        }
    }

    /// Initialize the stream that supports heap allocation as necessary.
    // pub fn initAlloc(alloc: Allocator, h: Handler) Self {
    // PORT NOTE: `alloc` is the OSC parser's allocator (Zig `std.mem.Allocator`
    // -> `ZigAllocatorRef`); threading it enables dynamic OSC buffer growth
    // (e.g. arbitrarily large OSC 52 clipboard payloads).
    pub fn init_alloc(alloc: rustty_lib::allocator::ZigAllocatorRef<'a>, h: H) -> Stream<'a, H> {
        // var self: Self = .init(h);
        let mut stream = Stream::init(h);
        // self.parser.osc_parser.alloc = alloc;
        stream.parser.osc_parser.alloc = Some(alloc);
        // return self;
        stream
    }

    // pub fn deinit(self: *Self) void {
    pub fn deinit(&mut self) {
        // self.parser.deinit();
        self.parser.deinit();
        // self.handler.deinit();
        self.handler.deinit();
    }

    /// Process a string of characters.
    // pub inline fn nextSlice(self: *Self, input: []const u8) void { ... }
    pub fn next_slice(&mut self, input: &[u8]) {
        // Disable SIMD optimizations if build requests it or if our
        // manual debug mode is on.
        // if (comptime debug or !build_options.simd) {
        //     for (input) |c| self.next(c);
        //     return;
        // }
        if DEBUG || !crate::options::SIMD {
            for &c in input {
                self.next(c);
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
            // for (cp_buf[0..res.decoded]) |cp| {
            for &cp in &cp_buf[0..res.decoded] {
                // if (cp <= 0xF) {
                if cp <= 0xF {
                    // self.execute(@intCast(cp));
                    self.execute(cp as u8);
                } else {
                    // self.print(@intCast(cp));
                    self.print(cp);
                }
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
            }

            // self.nextNonUtf8(input[offset]);
            self.next_non_utf8(input[offset]);
            // offset += 1;
            offset += 1;
        }
        // return offset;
        offset
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
    // pub inline fn next(self: *Self, c: u8) void { ... }
    pub fn next(&mut self, c: u8) {
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
                parser::Action::ApcEnd => self.handler.apc_end(),
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
            // .context_signal => { ... }
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

        let mut s: Stream<H> = Stream::init(H);
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
        let mut s: Stream<H> = Stream::init(H::default());
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

    /// The public `next_slice` entry point. Identical to `drive_bulk` when the
    /// `simd` feature is on; identical to `drive_per_byte` when it is off.
    /// Both configurations are asserted, so neither can rot.
    fn drive_slice<H: Handler + Default>(input: &[u8]) -> H {
        let mut s: Stream<H> = Stream::init(H::default());
        s.next_slice(input);
        s.handler
    }

    fn drive_per_byte<H: Handler + Default>(input: &[u8]) -> H {
        let mut s: Stream<H> = Stream::init(H::default());
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
                let mut s: Stream<RecordingHandler> = Stream::init(RecordingHandler::default());
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

        let mut s: Stream<H> = Stream::init(H { c: Some(0) });
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

        let mut s: Stream<H> = Stream::init(H { count: None });
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

        let mut s: Stream<H> = Stream::init(H { c: Some(0) });
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

        let mut s: Stream<H> = Stream::init(H { c: None });
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

        let mut s: Stream<H> = Stream::init(H { amount: 0 });
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

        let mut s: Stream<H> = Stream::init(H {
            mode: modes::Mode::CursorKeys,
        });
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

        let mut s: Stream<H> = Stream::init(H { mode: None });
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

        let mut s: Stream<H> = Stream::init(H { mode: None });
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

        let mut s: Stream<H> = Stream::init(H { called: false });
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

        let mut s: Stream<H> = Stream::init(H { n: 0 });
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

        let mut s: Stream<H> = Stream::init(H { v: None });
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

        let mut s: Stream<H> = Stream::init(H {
            mode: None,
            protected: None,
        });
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

        let mut s: Stream<H> = Stream::init(H {
            mode: None,
            protected: None,
        });
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

        let mut s: Stream<H> = Stream::init(H { style: None });
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

        let mut s: Stream<H> = Stream::init(H { style: None });
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

        let mut s: Stream<H> = Stream::init(H { escape: None });
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
            let mut s: Stream<H> = Stream::init(H { seen: false });
            s.next_slice(b"\x1b]2;abc\x1b\\");
            assert!(s.handler.seen);
        }

        {
            let mut s: Stream<H> = Stream::init(H { seen: false });
            s.next_slice(b"\x1b]2;abc\xc0\x1b\\");
            assert!(!s.handler.seen);
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

        let mut s: Stream<H> = Stream::init(H { called: false });
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

        let mut s: Stream<H> = Stream::init(H { value: None });
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

        let mut s: Stream<H> = Stream::init(H { called: false });
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

        let mut s: Stream<H> = Stream::init(H { called: false });
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

        let mut s: Stream<H> = Stream::init(H {});
        s.next_slice(b"\x1B[1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1C");
    }

    // test "stream: csi param too long" { ... }
    #[test]
    fn stream_csi_param_too_long() {
        struct H {}
        impl Handler for H {}

        let mut s: Stream<H> = Stream::init(H {});
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

        let mut s: Stream<H> = Stream::init(H { style: None });

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

        let mut s: Stream<H> = Stream::init(H { style: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { index: None });

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

        let mut s: Stream<H> = Stream::init(H { action: None });

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

        let mut s: Stream<H> = Stream::init(H { action: None });

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

        let mut s: Stream<H> = Stream::init(H { action: None });

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

        let mut s: Stream<H> = Stream::init(H { called: false });

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

        let mut s: Stream<H> = Stream::init(H { called: false });
        // This is the exact input from the fuzz crash (minus the mode byte):
        // CSI with a huge numeric param that saturates to 65535, followed by 'g'.
        s.next_slice(b"\x1b[388888888888888888888888888888888888g\x1b[0m");
    }
}
