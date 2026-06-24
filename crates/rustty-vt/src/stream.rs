//! rustty-vt stream
//!
//! PORTS: ghostty src/terminal/stream.zig
//!
//! PORT STATUS: stub
//!
//! SKELETON (vt-9): this sub-batch defines the `Handler` trait, the `Stream<H>`
//! type, `init`/`deinit`, `next`/`next_slice`, and the action router. The
//! per-final dispatch bodies (`csi_dispatch`/`osc_dispatch`/`esc_dispatch`) are
//! stubbed `todo!()` so the crate builds; the Dispatch phase fills them in
//! against this exact `Handler` surface.

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
// PORT NOTE: the reference `Action` is a tagged union whose tag enum `Key` and
// C-ABI union (`Tag`/`Value`/`C`/`CValue`/`cval`) drive the comptime `vt`
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
    fn print_repeat(&mut self, _count: u16) {}

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
// pub fn Stream(comptime H: type) type { return struct { ... }; }
// PORT NOTE: ported as the generic `Stream<H: Handler>` struct.
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

    // pub fn deinit(self: *Self) void {
    pub fn deinit(&mut self) {
        // self.parser.deinit();
        self.parser.deinit();
        // self.handler.deinit();
        self.handler.deinit();
    }

    /// Process a string of characters.
    // pub inline fn nextSlice(self: *Self, input: []const u8) void { ... }
    // PORT NOTE: the reference uses a SIMD fast path when `build_options.simd`
    // and not in `debug`. The SIMD primitives (`simd.vt.utf8DecodeUntilControlSeq`)
    // land in a later sub-batch; until then the scalar fallback (the reference's
    // own `comptime debug or !build_options.simd` arm) is used unconditionally.
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

        // SIMD path: filled by a later sub-batch (nextSliceCapped + simd.vt).
        todo!("vt SIMD nextSlice path (Dispatch/SIMD phase)")
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
                    // Explicitly ignored:
                    0x7F => {}
                    // Defer to the state machine to
                    // handle any other characters:
                    _ => break 'csi_param,
                }
                return;
            }
        }

        // We explicitly inline this call here for performance reasons.
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
            // `vt_raw` returns the skip flag directly.
            if self.handler.vt_raw(&action) {
                continue;
            }

            // switch (action) { ... }
            match action {
                parser::Action::Print(p) => self.print(p),
                parser::Action::Execute(code) => self.execute(code),
                parser::Action::CsiDispatch(csi_action) => self.csi_dispatch(csi_action),
                parser::Action::EscDispatch(esc) => self.esc_dispatch(esc),
                parser::Action::OscDispatch(cmd) => self.osc_dispatch(cmd),
                parser::Action::DcsHook(dcs) => self.handler.dcs_hook(dcs),
                parser::Action::DcsPut(code) => self.handler.dcs_put(code),
                parser::Action::DcsUnhook => self.handler.dcs_unhook(),
                parser::Action::ApcStart => self.handler.apc_start(),
                parser::Action::ApcPut(code) => self.handler.apc_put(code),
                parser::Action::ApcEnd => self.handler.apc_end(),
            }
        }
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
                intermediates: Vec::new(),
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

            // else => log.warn("invalid C0 character, ignoring: 0x{x}", .{c}),
            _ => {}
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
                    1 => input.params[0],
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
                    // log.warn("invalid device attributes command: {f}", .{input});
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
                        // log.warn("unimplemented mode: {}", .{mode_int});
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
                        // log.warn("unimplemented mode: {}", .{mode_int});
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

                        // else => log.warn("unknown CSI m with intermediate: {}", ...),
                        _ => {}
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
                                // else log.warn("ignoring CSI 22/23 t with extra parameters: {f}", ...),
                            }
                            // else => log.warn("ignoring CSI t with unimplemented parameter: {f}", ...),
                            _ => {}
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
                // log.info("OSC 1 (change icon) received and ignored icon={s}", .{icon});
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

                // log.warn("unimplemented ESC action: {f}", .{action});
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

            // else => log.warn("unimplemented ESC action: {f}", .{action}),
            _ => {}
        }
    }
}

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
