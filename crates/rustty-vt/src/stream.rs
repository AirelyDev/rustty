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
    // PORT NOTE (vt-9 SKELETON): the per-final CSI dispatch is filled by the
    // Dispatch phase against the `Handler` surface above. Stubbed so the crate
    // builds.
    fn csi_dispatch(&mut self, _input: parser::CSI) {
        todo!("vt csiDispatch (Dispatch phase)")
    }

    // inline fn oscDispatch(self: *Self, cmd: osc.Command) void { ... }
    fn osc_dispatch(&mut self, _cmd: osc::Command) {
        todo!("vt oscDispatch (Dispatch phase)")
    }

    // inline fn configureCharset(self: *Self, intermediates, set) void { ... }
    // PORT NOTE (vt-9 SKELETON): only called from `esc_dispatch` (itself a
    // `todo!()` stub), so it is unused until the Dispatch phase wires the ESC
    // charset finals. Allowed dead until then.
    #[allow(dead_code)]
    fn configure_charset(&mut self, _intermediates: &[u8], _set: charsets::Charset) {
        todo!("vt configureCharset (Dispatch phase)")
    }

    // inline fn escDispatch(self: *Self, action: Parser.Action.ESC) void { ... }
    fn esc_dispatch(&mut self, _action: parser::ESC) {
        todo!("vt escDispatch (Dispatch phase)")
    }
}
