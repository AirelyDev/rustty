//! rustty-vt stream_terminal
//!
//! PORTS: ghostty src/terminal/stream_terminal.zig
//!
//! PORT STATUS: skeleton (struct-first; method bodies are `todo!()`, ported in
//! vt-12b-7)

// PORT NOTE (vt-12a skeleton): every handler body is `todo!()` and the fields
// are not yet read, so we allow the lints the skeleton trivially trips. These
// allows are removed as vt-12b-7 fills the bodies.
#![allow(clippy::todo)]
#![allow(dead_code)]
#![allow(unused_variables)]

// const std = @import("std");
// const build_options = @import("terminal_options");
// const testing = std.testing;
// const apc = @import("apc.zig");
// const csi = @import("csi.zig");
// const device_attributes = @import("device_attributes.zig");
// const device_status = @import("device_status.zig");
// const stream = @import("stream.zig");
// const Action = stream.Action;
// const Screen = @import("Screen.zig");
// const modes = @import("modes.zig");
// const osc_color = @import("osc/parsers/color.zig");
// const kitty_color = @import("kitty/color.zig");
// const size_report = @import("size_report.zig");
// const Terminal = @import("Terminal.zig");
use crate::ansi;
use crate::csi;
use crate::device_attributes;
use crate::device_status;
use crate::kitty;
use crate::osc;
use crate::screen::CursorStyle;
use crate::sgr;
use crate::size_report;
use crate::stream;
use crate::terminal::Terminal;

use core::marker::PhantomData;

// const log = std.log.scoped(.stream_terminal);

/// This is a Stream implementation that processes actions against
/// a Terminal and updates the Terminal state.
// pub const Stream = stream.Stream(Handler);
// PORT NOTE: the reference `stream.Stream(Handler)` becomes the generic
// `stream::Stream<'a, Handler<'a>>`. The `'a` threads the OSC parser allocator
// lifetime (see `stream::Stream::init_alloc`) through both the stream and the
// handler's effect callbacks.
pub type Stream<'a> = stream::Stream<'a, Handler<'a>>;

/// The APC command handler maintains the APC state.
///
/// PORT NOTE (vt-12a deferral): APC (and the kitty-graphics/Glyph protocols it
/// drives) is font-blocked and remains deferred past vt-12 (see DEFERRALS.md).
/// The reference `apc.Handler` is a ZST/no-op stub here; `start`/`feed`/`end`/
/// `deinit` are no-ops until the protocol is reactivated.
#[derive(Debug, Default)]
pub struct ApcHandler;

impl ApcHandler {
    // pub fn deinit(self: *Handler) void
    pub fn deinit(&mut self) {
        todo!()
    }
}

/// A stream handler that updates terminal state. By default, it is
/// readonly in the sense that it only updates terminal state and ignores
/// all other sequences that require a response or otherwise have side
/// effects (e.g. clipboards).
///
/// You can manually set various effects callbacks in the `effects` field
/// to implement certain effects such as bells, titles, clipboard, etc.
pub struct Handler<'a> {
    /// The terminal state to modify.
    // terminal: *Terminal
    // PORT NOTE: `*mut Terminal` (raw ptr, NOT `&mut`). The `Stream` owns this
    // `Handler`, and the `Handler` aliases the `Terminal` it is created from
    // (`Terminal::vt_handler`). This mirrors `ScreenSet`'s `active: *mut Screen`
    // seam: the borrow checker can't express the self-referential aliasing, so
    // we carry the raw pointer and deref it (`unsafe { &mut *self.terminal }`)
    // in the method bodies (vt-12b-7).
    pub terminal: *mut Terminal,

    /// Callbacks for certain effects that handlers may have. These
    /// may or may not fully replace internal handling of certain effects,
    /// but they allow for the handler to trigger or query external
    /// effects.
    pub effects: Effects<'a>,

    /// The APC command handler maintains the APC state. APC is like
    /// CSI or OSC, but it is a private escape sequence that is used
    /// to send commands to the terminal emulator. This is used by
    /// the kitty graphics protocol.
    pub apc_handler: ApcHandler,

    /// Default cursor style used by DECSCUSR reset (CSI 0 q).
    pub default_cursor: bool,
    pub default_cursor_style: CursorStyle,
    pub default_cursor_blink: bool,

    // PORT NOTE: ties the `'a` effect-callback lifetime to the handler without a
    // stored borrow; the actual aliased `Terminal` is held as a raw pointer.
    _phantom: PhantomData<&'a ()>,
}

/// Callbacks for certain effects that handlers may have.
// pub const Effects = struct { ... }
pub struct Effects<'a> {
    /// Called when the terminal needs to write data back to the pty,
    /// e.g. in response to a DECRQM query. The data is only valid
    /// during the lifetime of the call so callers must copy it
    /// if it needs to be stored or used after the call returns.
    // write_pty: ?*const fn (*Handler, [:0]const u8) void
    pub write_pty: Option<fn(&mut Handler<'a>, &[u8])>,

    /// Called when the bell is rung (BEL).
    // bell: ?*const fn (*Handler) void
    pub bell: Option<fn(&mut Handler<'a>)>,

    /// Called in response to a color scheme DSR query (CSI ? 996 n).
    /// Returns the current color scheme. Return null to silently
    /// ignore the query.
    // color_scheme: ?*const fn (*Handler) ?device_status.ColorScheme
    pub color_scheme: Option<fn(&mut Handler<'a>) -> Option<device_status::ColorScheme>>,

    /// Called in response to a device attributes query (CSI c,
    /// CSI > c, CSI = c). Returns the response to encode and
    /// write back to the pty.
    // device_attributes: ?*const fn (*Handler) device_attributes.Attributes
    pub device_attributes: Option<fn(&mut Handler<'a>) -> device_attributes::Attributes>,

    /// Called in response to ENQ (0x05). Returns the raw response
    /// bytes to write back to the pty. The returned memory must be
    /// valid for the lifetime of the call.
    // enquiry: ?*const fn (*Handler) []const u8
    pub enquiry: Option<fn(&mut Handler<'a>) -> &'a [u8]>,

    /// Called in response to XTWINOPS size queries (CSI 14/16/18 t).
    /// Returns the current terminal geometry used for encoding.
    /// Return null to silently ignore the query.
    // size: ?*const fn (*Handler) ?size_report.Size
    pub size: Option<fn(&mut Handler<'a>) -> Option<size_report::Size>>,

    /// Called when the terminal title changes via escape sequences
    /// (e.g. OSC 0/2). The new title can be queried via
    /// handler.terminal.getTitle().
    // title_changed: ?*const fn (*Handler) void
    pub title_changed: Option<fn(&mut Handler<'a>)>,

    /// Called when the terminal pwd changes via escape sequences
    /// (e.g. OSC 7). The new pwd can be queried via
    /// handler.terminal.getPwd().
    // pwd_changed: ?*const fn (*Handler) void
    pub pwd_changed: Option<fn(&mut Handler<'a>)>,

    /// Called in response to an XTVERSION query. Returns the version
    /// string to report (e.g. "ghostty 1.2.3"). The returned memory
    /// must be valid for the lifetime of the call. The maximum length
    /// is 256 bytes; longer strings will be silently ignored.
    // xtversion: ?*const fn (*Handler) []const u8
    pub xtversion: Option<fn(&mut Handler<'a>) -> &'a [u8]>,
}

impl<'a> Effects<'a> {
    /// No effects means that the stream effectively becomes readonly
    /// that only affects pure terminal state and ignores all side
    /// effects beyond that.
    // pub const readonly: Effects = .{ ... all null ... };
    pub const READONLY: Effects<'a> = Effects {
        bell: None,
        color_scheme: None,
        device_attributes: None,
        enquiry: None,
        size: None,
        title_changed: None,
        pwd_changed: None,
        write_pty: None,
        xtversion: None,
    };
}

impl<'a> Default for Effects<'a> {
    // effects: Effects = .readonly
    fn default() -> Effects<'a> {
        Effects::READONLY
    }
}

impl<'a> Handler<'a> {
    // pub fn init(terminal: *Terminal) Handler {
    //     return .{ .terminal = terminal };
    // }
    pub fn init(terminal: *mut Terminal) -> Handler<'a> {
        todo!()
    }

    // inline fn vtFallible(
    //     self: *Handler,
    //     comptime action: Action.Tag,
    //     value: Action.Value(action),
    // ) !void
    //
    // PORT NOTE: the reference's `vt`/`vtFallible` comptime dispatcher (a single
    // `switch (action)` over `Action.Tag`) is distributed across the
    // `impl stream::Handler for Handler` methods below: the Rust `Stream`
    // calls the individual trait methods directly (monomorphized per action),
    // so each `vtFallible` arm becomes the corresponding trait-method body in
    // vt-12b-7. We keep `vt_fallible` here as a single seam that those bodies
    // can delegate to where the reference dispatched centrally.
    fn vt_fallible(&mut self) {
        todo!()
    }
}

// impl stream::Handler for Handler ------------------------------------------
//
// PORT NOTE: every method is `todo!()` for the vt-12a skeleton. The bodies are
// the per-action arms of the reference `vtFallible` switch (plus the OSC/DCS/APC
// helpers), filled in vt-12b-7.
impl<'a> stream::Handler for Handler<'a> {
    // pub fn deinit(self: *Handler) void { self.apc_handler.deinit(); }
    fn deinit(&mut self) {
        todo!()
    }

    fn vt_raw(&mut self, action: &crate::parser::Action) -> bool {
        todo!()
    }

    fn print(&mut self, cp: u32) {
        todo!()
    }

    fn print_repeat(&mut self, count: u16) {
        todo!()
    }

    fn bell(&mut self) {
        todo!()
    }

    fn backspace(&mut self) {
        todo!()
    }

    fn horizontal_tab(&mut self, count: u16) {
        todo!()
    }

    fn horizontal_tab_back(&mut self, count: u16) {
        todo!()
    }

    fn linefeed(&mut self) {
        todo!()
    }

    fn carriage_return(&mut self) {
        todo!()
    }

    fn enquiry(&mut self) {
        todo!()
    }

    fn invoke_charset(&mut self, v: stream::InvokeCharset) {
        todo!()
    }

    fn cursor_up(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_down(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_left(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_right(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_col(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_row(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_col_relative(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_row_relative(&mut self, v: stream::CursorMovement) {
        todo!()
    }

    fn cursor_pos(&mut self, v: stream::CursorPos) {
        todo!()
    }

    fn cursor_style(&mut self, v: ansi::CursorStyle) {
        todo!()
    }

    fn erase_display_below(&mut self, protected: bool) {
        todo!()
    }

    fn erase_display_above(&mut self, protected: bool) {
        todo!()
    }

    fn erase_display_complete(&mut self, protected: bool) {
        todo!()
    }

    fn erase_display_scrollback(&mut self, protected: bool) {
        todo!()
    }

    fn erase_display_scroll_complete(&mut self, protected: bool) {
        todo!()
    }

    fn erase_line_right(&mut self, protected: bool) {
        todo!()
    }

    fn erase_line_left(&mut self, protected: bool) {
        todo!()
    }

    fn erase_line_complete(&mut self, protected: bool) {
        todo!()
    }

    fn erase_line_right_unless_pending_wrap(&mut self, protected: bool) {
        todo!()
    }

    fn delete_chars(&mut self, count: usize) {
        todo!()
    }

    fn erase_chars(&mut self, count: usize) {
        todo!()
    }

    fn insert_lines(&mut self, count: usize) {
        todo!()
    }

    fn insert_blanks(&mut self, count: usize) {
        todo!()
    }

    fn delete_lines(&mut self, count: usize) {
        todo!()
    }

    fn scroll_up(&mut self, count: usize) {
        todo!()
    }

    fn scroll_down(&mut self, count: usize) {
        todo!()
    }

    fn tab_clear_current(&mut self) {
        todo!()
    }

    fn tab_clear_all(&mut self) {
        todo!()
    }

    fn tab_set(&mut self) {
        todo!()
    }

    fn tab_reset(&mut self) {
        todo!()
    }

    fn index(&mut self) {
        todo!()
    }

    fn next_line(&mut self) {
        todo!()
    }

    fn reverse_index(&mut self) {
        todo!()
    }

    fn full_reset(&mut self) {
        todo!()
    }

    fn set_mode(&mut self, v: stream::Mode) {
        todo!()
    }

    fn reset_mode(&mut self, v: stream::Mode) {
        todo!()
    }

    fn save_mode(&mut self, v: stream::Mode) {
        todo!()
    }

    fn restore_mode(&mut self, v: stream::Mode) {
        todo!()
    }

    fn request_mode(&mut self, v: stream::Mode) {
        todo!()
    }

    fn request_mode_unknown(&mut self, v: stream::RawMode) {
        todo!()
    }

    fn top_and_bottom_margin(&mut self, v: stream::Margin) {
        todo!()
    }

    fn left_and_right_margin(&mut self, v: stream::Margin) {
        todo!()
    }

    fn left_and_right_margin_ambiguous(&mut self) {
        todo!()
    }

    fn save_cursor(&mut self) {
        todo!()
    }

    fn restore_cursor(&mut self) {
        todo!()
    }

    fn modify_key_format(&mut self, v: ansi::ModifyKeyFormat) {
        todo!()
    }

    fn mouse_shift_capture(&mut self, capture: bool) {
        todo!()
    }

    fn protected_mode_off(&mut self) {
        todo!()
    }

    fn protected_mode_iso(&mut self) {
        todo!()
    }

    fn protected_mode_dec(&mut self) {
        todo!()
    }

    fn size_report(&mut self, v: csi::SizeReportStyle) {
        todo!()
    }

    fn title_push(&mut self, v: u16) {
        todo!()
    }

    fn title_pop(&mut self, v: u16) {
        todo!()
    }

    fn xtversion(&mut self) {
        todo!()
    }

    fn device_attributes(&mut self, req: device_attributes::Req) {
        todo!()
    }

    fn device_status(&mut self, v: stream::DeviceStatus) {
        todo!()
    }

    fn kitty_keyboard_query(&mut self) {
        todo!()
    }

    fn kitty_keyboard_push(&mut self, v: stream::KittyKeyboardFlags) {
        todo!()
    }

    fn kitty_keyboard_pop(&mut self, v: u16) {
        todo!()
    }

    fn kitty_keyboard_set(&mut self, v: stream::KittyKeyboardFlags) {
        todo!()
    }

    fn kitty_keyboard_set_or(&mut self, v: stream::KittyKeyboardFlags) {
        todo!()
    }

    fn kitty_keyboard_set_not(&mut self, v: stream::KittyKeyboardFlags) {
        todo!()
    }

    fn dcs_hook(&mut self, dcs: crate::parser::DCS) {
        todo!()
    }

    fn dcs_put(&mut self, code: u8) {
        todo!()
    }

    fn dcs_unhook(&mut self) {
        todo!()
    }

    fn apc_start(&mut self) {
        todo!()
    }

    fn apc_put(&mut self, code: u8) {
        todo!()
    }

    fn apc_end(&mut self) {
        todo!()
    }

    fn start_hyperlink(&mut self, v: stream::StartHyperlink) {
        todo!()
    }

    fn end_hyperlink(&mut self) {
        todo!()
    }

    fn active_status_display(&mut self, v: ansi::StatusDisplay) {
        todo!()
    }

    fn decaln(&mut self) {
        todo!()
    }

    fn window_title(&mut self, v: stream::WindowTitle) {
        todo!()
    }

    fn report_pwd(&mut self, v: stream::ReportPwd) {
        todo!()
    }

    fn show_desktop_notification(&mut self, v: stream::ShowDesktopNotification) {
        todo!()
    }

    fn progress_report(&mut self, v: osc::ProgressReport) {
        todo!()
    }

    fn clipboard_contents(&mut self, v: stream::ClipboardContents) {
        todo!()
    }

    fn mouse_shape(&mut self, v: crate::mouse::Shape) {
        todo!()
    }

    fn configure_charset(&mut self, v: stream::ConfigureCharset) {
        todo!()
    }

    fn set_attribute(&mut self, attr: sgr::Attribute) {
        todo!()
    }

    fn kitty_color_report(&mut self, v: kitty::color::OSC) {
        todo!()
    }

    fn color_operation(&mut self, v: stream::ColorOperation) {
        todo!()
    }

    fn semantic_prompt(&mut self, v: stream::SemanticPrompt) {
        todo!()
    }
}
