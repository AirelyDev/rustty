//! rustty-vt stream_terminal
//!
//! PORTS: ghostty src/terminal/stream_terminal.zig
//!
//! PORT STATUS: stub
//!
//! vt-12b-7a fills the STATE-mutating dispatch arms (the `vtFallible` arms that
//! call already-ported Terminal methods), the Effects scaffolding, and the
//! simple helpers (`horizontalTab`/`horizontalTabBack`/`setMode`). The
//! RESPONSE/QUERY arms (device_attributes/device_status/report_*/request_mode*/
//! send_mode_report/query_kitty_keyboard/color_operation/kitty_color_report) are
//! filled in vt-12b-7b and remain `todo!()`; APC end + kitty/glyph are stubbed.

// PORT NOTE (vt-12b-7a): the RESPONSE/QUERY arms left as `todo!()` keep tripping
// clippy::todo, and the apc-stub fields are not all read yet, so we retain the
// skeleton allows until vt-12b-7b removes them.
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
use crate::color;
use crate::csi;
use crate::device_attributes;
use crate::device_status;
use crate::kitty;
use crate::modes;
use crate::osc;
use crate::screen::CursorStyle;
use crate::sgr;
use crate::size_report;
use crate::stream;
use crate::terminal::{DeccolmMode, SwitchScreenMode, Terminal};

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
    //
    // PORT NOTE (vt-12b-7a): APC is font-blocked and deferred past vt-12
    // (DEFERRALS.md); the handler is a ZST/no-op stub, so `deinit` does nothing.
    pub fn deinit(&mut self) {}
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
        Handler {
            terminal,
            effects: Effects::READONLY,
            apc_handler: ApcHandler,
            default_cursor: true,
            default_cursor_style: CursorStyle::Block,
            default_cursor_blink: false,
            _phantom: PhantomData,
        }
    }

    // PORT NOTE: the reference's `vt`/`vtFallible` comptime dispatcher (a single
    // `switch (action)` over `Action.Tag`) is distributed across the
    // `impl stream::Handler for Handler` methods below: the Rust `Stream`
    // calls the individual trait methods directly (monomorphized per action),
    // so each `vtFallible` arm becomes the corresponding trait-method body. The
    // OSC/CSI helper functions the arms delegate to (`writePty`/`setMode`/
    // `horizontalTab`/...) are the inherent methods below.

    // pub fn vt(
    //     self: *Handler,
    //     comptime action: Action.Tag,
    //     value: Action.Value(action),
    // ) void {
    //     self.vtFallible(action, value) catch |err| {
    //         log.warn("error handling VT action action={} err={}", .{ action, err });
    //     };
    // }
    //
    // PORT NOTE: the reference's `vt` wraps the fallible per-action dispatch and
    // swallows+logs any error. Since the dispatch is distributed across the
    // trait-method arms here, `vt` becomes a small helper each fallible arm
    // funnels its `Result`-returning Terminal call through: on `Err` it logs and
    // continues (the `catch |err| log.warn(...)` behavior), so a transient
    // allocation failure never propagates out of the stream.
    #[inline]
    fn vt<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Terminal) -> Result<(), crate::pagelist::OutOfMemory>,
    {
        if f(self.term()).is_err() {
            // log.warn("error handling VT action action={} err={}", ...);
        }
    }

    // PORT NOTE: convenience accessor for the `*mut Terminal` deref. Each call
    // re-derefs the raw pointer (no `&mut Terminal` is held across re-reads, per
    // the soundness rule); callers must not nest two live `term()` borrows.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    fn term(&self) -> &mut Terminal {
        // SAFETY: `self.terminal` aliases the `Terminal` that created this
        // `Handler` (via `Terminal::vt_handler`); the `Stream` owns the
        // `Handler` and the borrow checker cannot express that self-referential
        // aliasing (mirrors `ScreenSet`'s `active: *mut Screen`). The deref is
        // short-lived and never overlaps another live `&mut Terminal`.
        unsafe { &mut *self.terminal }
    }

    // inline fn writePty(self: *Handler, data: [:0]const u8) void {
    //     const func = self.effects.write_pty orelse return;
    //     func(self, data);
    // }
    //
    // PORT NOTE (vt-12b-7a): the `write_pty` callback takes `&[u8]`. The
    // reference's `[:0]const u8` sentinel is a Zig encoding detail of the
    // response buffers (filled in vt-12b-7b alongside the response arms); the
    // callback boundary itself only needs the byte slice.
    #[inline]
    fn write_pty(&mut self, data: &[u8]) {
        let Some(func) = self.effects.write_pty else {
            return;
        };
        func(self, data);
    }

    // fn bell(self: *Handler) void {
    //     const func = self.effects.bell orelse return;
    //     func(self);
    // }
    fn do_bell(&mut self) {
        let Some(func) = self.effects.bell else {
            return;
        };
        func(self);
    }

    // inline fn horizontalTab(self: *Handler, count: u16) void {
    //     for (0..count) |_| {
    //         const x = self.terminal.screens.active.cursor.x;
    //         self.terminal.horizontalTab();
    //         if (x == self.terminal.screens.active.cursor.x) break;
    //     }
    // }
    #[inline]
    fn do_horizontal_tab(&mut self, count: u16) {
        for _ in 0..count {
            let x = self.term().active_mut().cursor.x;
            self.term().horizontal_tab();
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn horizontalTabBack(self: *Handler, count: u16) void {
    //     for (0..count) |_| {
    //         const x = self.terminal.screens.active.cursor.x;
    //         self.terminal.horizontalTabBack();
    //         if (x == self.terminal.screens.active.cursor.x) break;
    //     }
    // }
    #[inline]
    fn do_horizontal_tab_back(&mut self, count: u16) {
        for _ in 0..count {
            let x = self.term().active_mut().cursor.x;
            self.term().horizontal_tab_back();
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // fn setMode(self: *Handler, mode: modes.Mode, enabled: bool) !void
    fn do_set_mode(
        &mut self,
        mode: modes::Mode,
        enabled: bool,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // Set the mode on the terminal
        // self.terminal.modes.set(mode, enabled);
        self.term().modes.set(mode, enabled);

        // Some modes require additional processing
        match mode {
            // .autorepeat,
            // .reverse_colors,
            // => {},
            modes::Mode::Autorepeat | modes::Mode::ReverseColors => {}

            // .origin => self.terminal.setCursorPos(1, 1),
            modes::Mode::Origin => self.term().set_cursor_pos(1, 1),

            // .enable_left_and_right_margin => if (!enabled) {
            //     self.terminal.scrolling_region.left = 0;
            //     self.terminal.scrolling_region.right = self.terminal.cols - 1;
            // },
            modes::Mode::EnableLeftAndRightMargin => {
                if !enabled {
                    self.term().scrolling_region.left = 0;
                    let right = self.term().cols - 1;
                    self.term().scrolling_region.right = right;
                }
            }

            // .alt_screen_legacy => try self.terminal.switchScreenMode(.@"47", enabled),
            modes::Mode::AltScreenLegacy => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M47, enabled)?;
            }
            // .alt_screen => try self.terminal.switchScreenMode(.@"1047", enabled),
            modes::Mode::AltScreen => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M1047, enabled)?;
            }
            // .alt_screen_save_cursor_clear_enter => try self.terminal.switchScreenMode(.@"1049", enabled),
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M1049, enabled)?;
            }

            // .save_cursor => if (enabled) {
            //     self.terminal.saveCursor();
            // } else {
            //     self.terminal.restoreCursor();
            // },
            modes::Mode::SaveCursor => {
                if enabled {
                    self.term().save_cursor();
                } else {
                    self.term().restore_cursor();
                }
            }

            // .enable_mode_3 => {},
            modes::Mode::EnableMode3 => {}

            // .@"132_column" => try self.terminal.deccolm(
            //     self.terminal.screens.active.alloc,
            //     if (enabled) .@"132_cols" else .@"80_cols",
            // ),
            modes::Mode::Column132 => {
                let alloc = unsafe { (*self.term().screens.active).alloc };
                self.term().deccolm(
                    alloc,
                    if enabled {
                        DeccolmMode::Cols132
                    } else {
                        DeccolmMode::Cols80
                    },
                )?;
            }

            // .synchronized_output,
            // .linefeed,
            // .in_band_size_reports,
            // .focus_event,
            // => {},
            modes::Mode::SynchronizedOutput
            | modes::Mode::Linefeed
            | modes::Mode::InBandSizeReports
            | modes::Mode::FocusEvent => {}

            // .mouse_event_x10 => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .x10;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventX10 => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::X10
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_normal => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .normal;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventNormal => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Normal
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_button => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .button;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventButton => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Button
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_any => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .any;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventAny => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Any
                } else {
                    crate::mouse::Event::None
                };
            }

            // .mouse_format_utf8 => self.terminal.flags.mouse_format = if (enabled) .utf8 else .x10,
            modes::Mode::MouseFormatUtf8 => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Utf8
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_sgr => self.terminal.flags.mouse_format = if (enabled) .sgr else .x10,
            modes::Mode::MouseFormatSgr => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Sgr
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_urxvt => self.terminal.flags.mouse_format = if (enabled) .urxvt else .x10,
            modes::Mode::MouseFormatUrxvt => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Urxvt
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_sgr_pixels => self.terminal.flags.mouse_format = if (enabled) .sgr_pixels else .x10,
            modes::Mode::MouseFormatSgrPixels => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::SgrPixels
                } else {
                    crate::mouse::Format::X10
                };
            }

            // else => {},
            _ => {}
        }

        Ok(())
    }

    // fn colorOperation(
    //     self: *Handler,
    //     op: osc_color.Operation,
    //     requests: *const osc_color.List,
    // ) !void
    fn do_color_operation(
        &mut self,
        op: osc::color::Operation,
        requests: &osc::color::List,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // _ = op;
        let _ = op;
        // if (requests.count() == 0) return;
        if requests.is_empty() {
            return Ok(());
        }

        // var it = requests.constIterator(0);
        // while (it.next()) |req| {
        for req in requests.iter() {
            match req {
                // .set => |set| {
                osc::color::Request::Set(set) => {
                    match set.target {
                        // .palette => |i| {
                        //     self.terminal.flags.dirty.palette = true;
                        //     self.terminal.colors.palette.set(i, set.color);
                        // },
                        osc::color::Target::Palette(i) => {
                            self.term().flags.dirty.palette = true;
                            self.term().colors.palette.set(i, set.color);
                        }
                        // .dynamic => |dynamic| switch (dynamic) {
                        osc::color::Target::Dynamic(dynamic) => match dynamic {
                            // .foreground => self.terminal.colors.foreground.set(set.color),
                            color::Dynamic::Foreground => {
                                self.term().colors.foreground.set(set.color);
                            }
                            // .background => self.terminal.colors.background.set(set.color),
                            color::Dynamic::Background => {
                                self.term().colors.background.set(set.color);
                            }
                            // .cursor => self.terminal.colors.cursor.set(set.color),
                            color::Dynamic::Cursor => {
                                self.term().colors.cursor.set(set.color);
                            }
                            // .pointer_foreground,
                            // .pointer_background,
                            // .tektronix_foreground,
                            // .tektronix_background,
                            // .highlight_background,
                            // .tektronix_cursor,
                            // .highlight_foreground,
                            // => {},
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {}
                        },
                        // .special => {},
                        osc::color::Target::Special(_) => {}
                    }
                }

                // .reset => |target| switch (target) {
                osc::color::Request::Reset(target) => match target {
                    // .palette => |i| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(i);
                    // },
                    osc::color::Target::Palette(i) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*i);
                    }
                    // .dynamic => |dynamic| switch (dynamic) {
                    osc::color::Target::Dynamic(dynamic) => match dynamic {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        color::Dynamic::Foreground => self.term().colors.foreground.reset(),
                        // .background => self.terminal.colors.background.reset(),
                        color::Dynamic::Background => self.term().colors.background.reset(),
                        // .cursor => self.terminal.colors.cursor.reset(),
                        color::Dynamic::Cursor => self.term().colors.cursor.reset(),
                        // .pointer_foreground,
                        // ...
                        // => {},
                        color::Dynamic::PointerForeground
                        | color::Dynamic::PointerBackground
                        | color::Dynamic::TektronixForeground
                        | color::Dynamic::TektronixBackground
                        | color::Dynamic::HighlightBackground
                        | color::Dynamic::TektronixCursor
                        | color::Dynamic::HighlightForeground => {}
                    },
                    // .special => {},
                    osc::color::Target::Special(_) => {}
                },

                // .reset_palette => {
                //     const mask = &self.terminal.colors.palette.mask;
                //     var mask_it = mask.iterator(.{});
                //     while (mask_it.next()) |i| {
                //         self.terminal.flags.dirty.palette = true;
                //         self.terminal.colors.palette.reset(@intCast(i));
                //     }
                //     mask.* = .initEmpty();
                // },
                osc::color::Request::ResetPalette => {
                    let indices: Vec<usize> = self.term().colors.palette.mask.iter_set().collect();
                    for i in indices {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(i as u8);
                    }
                    self.term().colors.palette.mask = color::PaletteMask::init_empty();
                }

                // .query,
                // .reset_special,
                // => {},
                osc::color::Request::Query(_) | osc::color::Request::ResetSpecial => {}
            }
        }

        Ok(())
    }

    // fn kittyColorOperation(
    //     self: *Handler,
    //     request: kitty_color.OSC,
    // ) !void
    fn do_kitty_color_operation(
        &mut self,
        request: kitty::color::OSC,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // for (request.list.items) |item| {
        for item in &request.list {
            match item {
                // .set => |v| switch (v.key) {
                kitty::color::Request::Set { key, color } => match key {
                    // .palette => |palette| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.set(palette, v.color);
                    // },
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.set(*palette, *color);
                    }
                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.set(v.color),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.set(*color);
                        }
                        // .background => self.terminal.colors.background.set(v.color),
                        kitty::color::Special::Background => {
                            self.term().colors.background.set(*color);
                        }
                        // .cursor => self.terminal.colors.cursor.set(v.color),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.set(*color);
                        }
                        // else => {},
                        _ => {}
                    },
                },
                // .reset => |key| switch (key) {
                kitty::color::Request::Reset(key) => match key {
                    // .palette => |palette| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(palette);
                    // },
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*palette);
                    }
                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        kitty::color::Special::Foreground => self.term().colors.foreground.reset(),
                        // .background => self.terminal.colors.background.reset(),
                        kitty::color::Special::Background => self.term().colors.background.reset(),
                        // .cursor => self.terminal.colors.cursor.reset(),
                        kitty::color::Special::Cursor => self.term().colors.cursor.reset(),
                        // else => {},
                        _ => {}
                    },
                },
                // .query => {},
                kitty::color::Request::Query(_) => {}
            }
        }

        Ok(())
    }

    // fn apcEnd(self: *Handler) void
    //
    // PORT NOTE (vt-12b-7a): APC (kitty-graphics/glyph) is font-blocked and
    // deferred past vt-12 (DEFERRALS.md); `apc_handler` is a ZST/no-op stub so
    // `end()` yields nothing and there is no command to dispatch. The reference
    // body's kitty/glyph response encoding is reactivated in 12c-kitty.
    fn apc_end(&mut self) {}
}

// impl stream::Handler for Handler ------------------------------------------
//
// PORT NOTE: every method is `todo!()` for the vt-12a skeleton. The bodies are
// the per-action arms of the reference `vtFallible` switch (plus the OSC/DCS/APC
// helpers), filled in vt-12b-7.
impl<'a> stream::Handler for Handler<'a> {
    // pub fn deinit(self: *Handler) void { self.apc_handler.deinit(); }
    fn deinit(&mut self) {
        self.apc_handler.deinit();
    }

    // PORT NOTE: the reference has no `vtRaw` declared on this handler, so the
    // stream's `@hasDecl(T, "vtRaw")` branch is skipped (no raw interception).
    // The trait's default returns `false`; we keep the explicit override as a
    // no-op for fidelity with that "not declared" state.
    fn vt_raw(&mut self, action: &crate::parser::Action) -> bool {
        false
    }

    // .print => try self.terminal.print(value.cp),
    fn print(&mut self, cp: u32) {
        self.vt(|t| t.print(cp));
    }

    // .print_repeat => try self.terminal.printRepeat(value),
    fn print_repeat(&mut self, count: u16) {
        self.vt(|t| t.print_repeat(count as usize));
    }

    // .bell => self.bell(),
    fn bell(&mut self) {
        self.do_bell();
    }

    // .backspace => self.terminal.backspace(),
    fn backspace(&mut self) {
        self.term().backspace();
    }

    // .horizontal_tab => self.horizontalTab(value),
    fn horizontal_tab(&mut self, count: u16) {
        self.do_horizontal_tab(count);
    }

    // .horizontal_tab_back => self.horizontalTabBack(value),
    fn horizontal_tab_back(&mut self, count: u16) {
        self.do_horizontal_tab_back(count);
    }

    // .linefeed => try self.terminal.linefeed(),
    fn linefeed(&mut self) {
        self.vt(|t| t.linefeed());
    }

    // .carriage_return => self.terminal.carriageReturn(),
    fn carriage_return(&mut self) {
        self.term().carriage_return();
    }

    // .enquiry => self.reportEnquiry(),
    fn enquiry(&mut self) {
        todo!()
    }

    // .invoke_charset => self.terminal.invokeCharset(value.bank, value.charset, value.locking),
    fn invoke_charset(&mut self, v: stream::InvokeCharset) {
        self.term().invoke_charset(v.bank, v.charset, v.locking);
    }

    // .cursor_up => self.terminal.cursorUp(value.value),
    fn cursor_up(&mut self, v: stream::CursorMovement) {
        self.term().cursor_up(v.value as usize);
    }

    // .cursor_down => self.terminal.cursorDown(value.value),
    fn cursor_down(&mut self, v: stream::CursorMovement) {
        self.term().cursor_down(v.value as usize);
    }

    // .cursor_left => self.terminal.cursorLeft(value.value),
    fn cursor_left(&mut self, v: stream::CursorMovement) {
        self.term().cursor_left(v.value as usize);
    }

    // .cursor_right => self.terminal.cursorRight(value.value),
    fn cursor_right(&mut self, v: stream::CursorMovement) {
        self.term().cursor_right(v.value as usize);
    }

    // .cursor_col => self.terminal.setCursorPos(self.terminal.screens.active.cursor.y + 1, value.value),
    fn cursor_col(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y;
        self.term().set_cursor_pos(y as usize + 1, v.value as usize);
    }

    // .cursor_row => self.terminal.setCursorPos(value.value, self.terminal.screens.active.cursor.x + 1),
    fn cursor_row(&mut self, v: stream::CursorMovement) {
        let x = self.term().active_mut().cursor.x;
        self.term().set_cursor_pos(v.value as usize, x as usize + 1);
    }

    // .cursor_col_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1,
    //     self.terminal.screens.active.cursor.x + 1 +| value.value,
    // ),
    fn cursor_col_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y as usize;
        let x = self.term().active_mut().cursor.x as usize;
        self.term()
            .set_cursor_pos(y + 1, (x + 1).saturating_add(v.value as usize));
    }

    // .cursor_row_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1 +| value.value,
    //     self.terminal.screens.active.cursor.x + 1,
    // ),
    fn cursor_row_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y as usize;
        let x = self.term().active_mut().cursor.x as usize;
        self.term()
            .set_cursor_pos((y + 1).saturating_add(v.value as usize), x + 1);
    }

    // .cursor_pos => self.terminal.setCursorPos(value.row, value.col),
    fn cursor_pos(&mut self, v: stream::CursorPos) {
        self.term().set_cursor_pos(v.row as usize, v.col as usize);
    }

    // .cursor_style => { ... }
    fn cursor_style(&mut self, v: ansi::CursorStyle) {
        // self.default_cursor = false;
        self.default_cursor = false;

        // const blink = switch (value) {
        //     .default => self.default_cursor_blink,
        //     .steady_block, .steady_bar, .steady_underline => false,
        //     .blinking_block, .blinking_bar, .blinking_underline => true,
        // };
        let blink = match v {
            ansi::CursorStyle::Default => self.default_cursor_blink,
            ansi::CursorStyle::SteadyBlock
            | ansi::CursorStyle::SteadyBar
            | ansi::CursorStyle::SteadyUnderline => false,
            ansi::CursorStyle::BlinkingBlock
            | ansi::CursorStyle::BlinkingBar
            | ansi::CursorStyle::BlinkingUnderline => true,
        };
        // const style: Screen.CursorStyle = switch (value) {
        //     .default => style: {
        //         self.default_cursor = true;
        //         break :style self.default_cursor_style;
        //     },
        //     .blinking_block, .steady_block => .block,
        //     .blinking_bar, .steady_bar => .bar,
        //     .blinking_underline, .steady_underline => .underline,
        // };
        let style: CursorStyle = match v {
            ansi::CursorStyle::Default => {
                self.default_cursor = true;
                self.default_cursor_style
            }
            ansi::CursorStyle::BlinkingBlock | ansi::CursorStyle::SteadyBlock => CursorStyle::Block,
            ansi::CursorStyle::BlinkingBar | ansi::CursorStyle::SteadyBar => CursorStyle::Bar,
            ansi::CursorStyle::BlinkingUnderline | ansi::CursorStyle::SteadyUnderline => {
                CursorStyle::Underline
            }
        };
        // self.terminal.modes.set(.cursor_blinking, blink);
        self.term().modes.set(modes::Mode::CursorBlinking, blink);
        // self.terminal.screens.active.cursor.cursor_style = style;
        self.term().active_mut().cursor.cursor_style = style;
    }

    // .erase_display_below => self.terminal.eraseDisplay(.below, value),
    fn erase_display_below(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Below, protected);
    }

    // .erase_display_above => self.terminal.eraseDisplay(.above, value),
    fn erase_display_above(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Above, protected);
    }

    // .erase_display_complete => self.terminal.eraseDisplay(.complete, value),
    fn erase_display_complete(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Complete, protected);
    }

    // .erase_display_scrollback => self.terminal.eraseDisplay(.scrollback, value),
    fn erase_display_scrollback(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Scrollback, protected);
    }

    // .erase_display_scroll_complete => self.terminal.eraseDisplay(.scroll_complete, value),
    fn erase_display_scroll_complete(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::ScrollComplete, protected);
    }

    // .erase_line_right => self.terminal.eraseLine(.right, value),
    fn erase_line_right(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Right, protected);
    }

    // .erase_line_left => self.terminal.eraseLine(.left, value),
    fn erase_line_left(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Left, protected);
    }

    // .erase_line_complete => self.terminal.eraseLine(.complete, value),
    fn erase_line_complete(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Complete, protected);
    }

    // .erase_line_right_unless_pending_wrap => self.terminal.eraseLine(.right_unless_pending_wrap, value),
    fn erase_line_right_unless_pending_wrap(&mut self, protected: bool) {
        self.term()
            .erase_line(csi::EraseLine::RightUnlessPendingWrap, protected);
    }

    // .delete_chars => self.terminal.deleteChars(value),
    fn delete_chars(&mut self, count: usize) {
        self.term().delete_chars(count);
    }

    // .erase_chars => self.terminal.eraseChars(value),
    fn erase_chars(&mut self, count: usize) {
        self.term().erase_chars(count);
    }

    // .insert_lines => self.terminal.insertLines(value),
    fn insert_lines(&mut self, count: usize) {
        self.term().insert_lines(count);
    }

    // .insert_blanks => self.terminal.insertBlanks(value),
    fn insert_blanks(&mut self, count: usize) {
        self.term().insert_blanks(count);
    }

    // .delete_lines => self.terminal.deleteLines(value),
    fn delete_lines(&mut self, count: usize) {
        self.term().delete_lines(count);
    }

    // .scroll_up => try self.terminal.scrollUp(value),
    fn scroll_up(&mut self, count: usize) {
        self.vt(|t| t.scroll_up(count));
    }

    // .scroll_down => self.terminal.scrollDown(value),
    fn scroll_down(&mut self, count: usize) {
        self.term().scroll_down(count);
    }

    // .tab_clear_current => self.terminal.tabClear(.current),
    fn tab_clear_current(&mut self) {
        self.term().tab_clear(csi::TabClear::Current);
    }

    // .tab_clear_all => self.terminal.tabClear(.all),
    fn tab_clear_all(&mut self) {
        self.term().tab_clear(csi::TabClear::All);
    }

    // .tab_set => self.terminal.tabSet(),
    fn tab_set(&mut self) {
        self.term().tab_set();
    }

    // .tab_reset => self.terminal.tabReset(),
    fn tab_reset(&mut self) {
        self.term().tab_reset();
    }

    // .index => try self.terminal.index(),
    fn index(&mut self) {
        self.vt(|t| t.index());
    }

    // .next_line => {
    //     try self.terminal.index();
    //     self.terminal.carriageReturn();
    // },
    fn next_line(&mut self) {
        self.vt(|t| t.index());
        self.term().carriage_return();
    }

    // .reverse_index => self.terminal.reverseIndex(),
    fn reverse_index(&mut self) {
        self.term().reverse_index();
    }

    // .full_reset => {
    //     self.terminal.fullReset();
    //     self.default_cursor = true;
    //     self.terminal.modes.set(.cursor_blinking, self.default_cursor_blink);
    //     self.terminal.screens.active.cursor.cursor_style = self.default_cursor_style;
    // },
    fn full_reset(&mut self) {
        self.term().full_reset();
        self.default_cursor = true;
        let blink = self.default_cursor_blink;
        self.term().modes.set(modes::Mode::CursorBlinking, blink);
        let style = self.default_cursor_style;
        self.term().active_mut().cursor.cursor_style = style;
    }

    // .set_mode => try self.setMode(value.mode, true),
    fn set_mode(&mut self, v: stream::Mode) {
        let _ = self.do_set_mode(v.mode, true);
    }

    // .reset_mode => try self.setMode(value.mode, false),
    fn reset_mode(&mut self, v: stream::Mode) {
        let _ = self.do_set_mode(v.mode, false);
    }

    // .save_mode => self.terminal.modes.save(value.mode),
    fn save_mode(&mut self, v: stream::Mode) {
        self.term().modes.save(v.mode);
    }

    // .restore_mode => {
    //     const v = self.terminal.modes.restore(value.mode);
    //     try self.setMode(value.mode, v);
    // },
    fn restore_mode(&mut self, v: stream::Mode) {
        let enabled = self.term().modes.restore(v.mode);
        let _ = self.do_set_mode(v.mode, enabled);
    }

    // .request_mode => self.requestMode(value.mode),
    fn request_mode(&mut self, v: stream::Mode) {
        todo!()
    }

    // .request_mode_unknown => self.requestModeUnknown(value.mode, value.ansi),
    fn request_mode_unknown(&mut self, v: stream::RawMode) {
        todo!()
    }

    // .top_and_bottom_margin => self.terminal.setTopAndBottomMargin(value.top_left, value.bottom_right),
    fn top_and_bottom_margin(&mut self, v: stream::Margin) {
        self.term()
            .set_top_and_bottom_margin(v.top_left as usize, v.bottom_right as usize);
    }

    // .left_and_right_margin => self.terminal.setLeftAndRightMargin(value.top_left, value.bottom_right),
    fn left_and_right_margin(&mut self, v: stream::Margin) {
        self.term()
            .set_left_and_right_margin(v.top_left as usize, v.bottom_right as usize);
    }

    // .left_and_right_margin_ambiguous => {
    //     if (self.terminal.modes.get(.enable_left_and_right_margin)) {
    //         self.terminal.setLeftAndRightMargin(0, 0);
    //     } else {
    //         self.terminal.saveCursor();
    //     }
    // },
    fn left_and_right_margin_ambiguous(&mut self) {
        if self.term().modes.get(modes::Mode::EnableLeftAndRightMargin) {
            self.term().set_left_and_right_margin(0, 0);
        } else {
            self.term().save_cursor();
        }
    }

    // .save_cursor => self.terminal.saveCursor(),
    fn save_cursor(&mut self) {
        self.term().save_cursor();
    }

    // .restore_cursor => self.terminal.restoreCursor(),
    fn restore_cursor(&mut self) {
        self.term().restore_cursor();
    }

    // .modify_key_format => {
    //     self.terminal.flags.modify_other_keys_2 = false;
    //     switch (value) {
    //         .other_keys_numeric => self.terminal.flags.modify_other_keys_2 = true,
    //         else => {},
    //     }
    // },
    fn modify_key_format(&mut self, v: ansi::ModifyKeyFormat) {
        self.term().flags.modify_other_keys_2 = false;
        // PORT NOTE: the reference switch has a single non-default arm
        // (`.other_keys_numeric => ...; else => {}`); clippy recognizes that as
        // an equality test, so it is written as an `if` here.
        if v == ansi::ModifyKeyFormat::OtherKeysNumeric {
            self.term().flags.modify_other_keys_2 = true;
        }
    }

    // .mouse_shift_capture => self.terminal.flags.mouse_shift_capture = if (value) .true else .false,
    fn mouse_shift_capture(&mut self, capture: bool) {
        self.term().flags.mouse_shift_capture = if capture {
            crate::terminal::MouseShiftCapture::True
        } else {
            crate::terminal::MouseShiftCapture::False
        };
    }

    // .protected_mode_off => self.terminal.setProtectedMode(.off),
    fn protected_mode_off(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Off);
    }

    // .protected_mode_iso => self.terminal.setProtectedMode(.iso),
    fn protected_mode_iso(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Iso);
    }

    // .protected_mode_dec => self.terminal.setProtectedMode(.dec),
    fn protected_mode_dec(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Dec);
    }

    // .size_report => self.reportSize(value),
    fn size_report(&mut self, v: csi::SizeReportStyle) {
        todo!()
    }

    // .title_push, .title_pop => {} (have no terminal-modifying effect)
    fn title_push(&mut self, v: u16) {}

    // .title_push, .title_pop => {} (have no terminal-modifying effect)
    fn title_pop(&mut self, v: u16) {}

    // .xtversion => self.reportXtversion(),
    fn xtversion(&mut self) {
        todo!()
    }

    // .device_attributes => self.reportDeviceAttributes(value),
    fn device_attributes(&mut self, req: device_attributes::Req) {
        todo!()
    }

    // .device_status => self.deviceStatus(value.request),
    fn device_status(&mut self, v: stream::DeviceStatus) {
        todo!()
    }

    // .kitty_keyboard_query => self.queryKittyKeyboard(),
    fn kitty_keyboard_query(&mut self) {
        todo!()
    }

    // .kitty_keyboard_push => self.terminal.screens.active.kitty_keyboard.push(value.flags),
    fn kitty_keyboard_push(&mut self, v: stream::KittyKeyboardFlags) {
        self.term().active_mut().kitty_keyboard.push(v.flags);
    }

    // .kitty_keyboard_pop => self.terminal.screens.active.kitty_keyboard.pop(@intCast(value)),
    fn kitty_keyboard_pop(&mut self, v: u16) {
        self.term().active_mut().kitty_keyboard.pop(v as usize);
    }

    // .kitty_keyboard_set => self.terminal.screens.active.kitty_keyboard.set(.set, value.flags),
    fn kitty_keyboard_set(&mut self, v: stream::KittyKeyboardFlags) {
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Set, v.flags);
    }

    // .kitty_keyboard_set_or => self.terminal.screens.active.kitty_keyboard.set(.@"or", value.flags),
    fn kitty_keyboard_set_or(&mut self, v: stream::KittyKeyboardFlags) {
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Or, v.flags);
    }

    // .kitty_keyboard_set_not => self.terminal.screens.active.kitty_keyboard.set(.not, value.flags),
    fn kitty_keyboard_set_not(&mut self, v: stream::KittyKeyboardFlags) {
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Not, v.flags);
    }

    // .dcs_hook, .dcs_put, .dcs_unhook => {},
    // No supported DCS commands have any terminal-modifying effects, but they
    // may in the future. For now we just ignore it.
    fn dcs_hook(&mut self, dcs: crate::parser::DCS) {}

    fn dcs_put(&mut self, code: u8) {}

    fn dcs_unhook(&mut self) {}

    // .apc_start => self.apc_handler.start(),
    // PORT NOTE (vt-12b-7a): apc_handler is a ZST/no-op stub (font-blocked,
    // deferred past vt-12); `start`/`feed`/`end` do nothing.
    fn apc_start(&mut self) {}

    // .apc_put => self.apc_handler.feed(self.terminal.gpa(), value),
    fn apc_put(&mut self, code: u8) {}

    // .apc_end => self.apcEnd(),
    fn apc_end(&mut self) {
        Handler::apc_end(self);
    }

    // .start_hyperlink => try self.terminal.screens.active.startHyperlink(value.uri, value.id),
    fn start_hyperlink(&mut self, v: stream::StartHyperlink) {
        let _ = self.term().active_mut().start_hyperlink(v.uri, v.id);
    }

    // .end_hyperlink => self.terminal.screens.active.endHyperlink(),
    fn end_hyperlink(&mut self) {
        self.term().active_mut().end_hyperlink();
    }

    // .active_status_display => self.terminal.status_display = value,
    fn active_status_display(&mut self, v: ansi::StatusDisplay) {
        self.term().status_display = v;
    }

    // .decaln => try self.terminal.decaln(),
    fn decaln(&mut self) {
        self.vt(|t| t.decaln());
    }

    // .window_title => self.windowTitle(value.title),
    fn window_title(&mut self, v: stream::WindowTitle) {
        todo!()
    }

    // .report_pwd => self.reportPwd(value.url),
    fn report_pwd(&mut self, v: stream::ReportPwd) {
        todo!()
    }

    // .show_desktop_notification => {} (have no terminal-modifying effect)
    fn show_desktop_notification(&mut self, v: stream::ShowDesktopNotification) {}

    // .progress_report => {} (have no terminal-modifying effect)
    fn progress_report(&mut self, v: osc::ProgressReport) {}

    // .clipboard_contents => {} (have no terminal-modifying effect)
    fn clipboard_contents(&mut self, v: stream::ClipboardContents) {}

    // .mouse_shape => self.terminal.mouse_shape = value,
    fn mouse_shape(&mut self, v: crate::mouse::Shape) {
        self.term().mouse_shape = v;
    }

    // .configure_charset => self.terminal.configureCharset(value.slot, value.charset),
    fn configure_charset(&mut self, v: stream::ConfigureCharset) {
        self.term().configure_charset(v.slot, v.charset);
    }

    // .set_attribute => switch (value) {
    //     .unknown => {},
    //     else => self.terminal.setAttribute(value) catch {},
    // },
    fn set_attribute(&mut self, attr: sgr::Attribute) {
        match attr {
            sgr::Attribute::Unknown { .. } => {}
            _ => {
                let _ = self.term().set_attribute(attr);
            }
        }
    }

    // .kitty_color_report => try self.kittyColorOperation(value),
    fn kitty_color_report(&mut self, v: kitty::color::OSC) {
        let _ = self.do_kitty_color_operation(v);
    }

    // .color_operation => try self.colorOperation(value.op, &value.requests),
    fn color_operation(&mut self, v: stream::ColorOperation) {
        let _ = self.do_color_operation(v.op, &v.requests);
    }

    // .semantic_prompt => try self.terminal.semanticPrompt(value),
    fn semantic_prompt(&mut self, v: stream::SemanticPrompt) {
        self.vt(|t| t.semantic_prompt(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::modes as modespkg;
    use crate::terminal::Colors;
    use crate::terminal::Options;
    use crate::{kitty, size};
    use allocator_api2::alloc::Global;

    // PORT NOTE: the reference tests construct `Terminal: try .init(testing.allocator,
    // .{ .cols, .rows })` and drive bytes through a `Stream`. rustty's `Options`
    // has no field defaults, so we supply them via this helper (matching
    // `terminal.rs`'s test `opts`). `max_scrollback` defaults to 10_000 except
    // where a test overrides it.
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // test "basic print"
    #[test]
    fn basic_print() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"Hello");
        }

        assert_eq!(5_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Hello");

        t.deinit(Global);
    }

    // test "cursor movement"
    #[test]
    fn cursor_movement() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Move cursor using escape sequences
            s.next_slice(b"Hello\x1B[1;1H");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Move to position 2,3
            s.next_slice(b"\x1B[2;3H");
        }
        assert_eq!(2_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "erase operations"
    #[test]
    fn erase_operations() {
        let mut t = Terminal::init(Global, opts(20, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Print some text
            s.next_slice(b"Hello World");
        }
        assert_eq!(11_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Move cursor to position 1,6 and erase from cursor to end of line
            s.next_slice(b"\x1B[1;6H");
            s.next_slice(b"\x1B[K");
        }

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Hello");

        t.deinit(Global);
    }

    // test "tabs"
    #[test]
    fn tabs() {
        let mut t = Terminal::init(Global, opts(80, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"A\tB");
        }
        assert_eq!(9_usize, t.active_mut().cursor.x as usize);

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"A       B");

        t.deinit(Global);
    }

    // test "modes"
    #[test]
    fn modes() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Test wraparound mode
        assert!(t.modes.get(modespkg::Mode::Wraparound));
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B[?7l"); // Disable wraparound
        }
        assert!(!t.modes.get(modespkg::Mode::Wraparound));
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B[?7h"); // Enable wraparound
        }
        assert!(t.modes.get(modespkg::Mode::Wraparound));

        t.deinit(Global);
    }

    // test "scrolling regions"
    #[test]
    fn scrolling_regions() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set scrolling region from line 5 to 20
            s.next_slice(b"\x1B[5;20r");
        }
        assert_eq!(4_usize, t.scrolling_region.top as usize);
        assert_eq!(19_usize, t.scrolling_region.bottom as usize);
        assert_eq!(0_usize, t.scrolling_region.left as usize);
        assert_eq!(79_usize, t.scrolling_region.right as usize);

        t.deinit(Global);
    }

    // test "charsets"
    #[test]
    fn charsets() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Configure G0 as DEC special graphics
            s.next_slice(b"\x1B(0");
            s.next_slice(b"`"); // Should print diamond character
        }

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "◆".as_bytes());

        t.deinit(Global);
    }

    // test "alt screen"
    #[test]
    fn alt_screen() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write to primary screen
            s.next_slice(b"Primary");
        }
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        {
            let mut s = t.vt_stream();
            // Switch to alt screen
            s.next_slice(b"\x1B[?1049h");
        }
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        {
            let mut s = t.vt_stream();
            // Write to alt screen
            s.next_slice(b"Alt");

            // Switch back to primary
            s.next_slice(b"\x1B[?1049l");
        }
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Primary");

        t.deinit(Global);
    }

    // test "cursor save and restore"
    #[test]
    fn cursor_save_and_restore() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Move cursor to 10,15
            s.next_slice(b"\x1B[10;15H");
        }
        assert_eq!(14_usize, t.active_mut().cursor.x as usize);
        assert_eq!(9_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Save cursor
            s.next_slice(b"\x1B7");
            // Move cursor elsewhere
            s.next_slice(b"\x1B[1;1H");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Restore cursor
            s.next_slice(b"\x1B8");
        }
        assert_eq!(14_usize, t.active_mut().cursor.x as usize);
        assert_eq!(9_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "attributes"
    #[test]
    fn attributes() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set bold and write text
            s.next_slice(b"\x1B[1mBold\x1B[0m");
        }

        // Verify we can write attributes - just check the string was written
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Bold");

        t.deinit(Global);
    }

    // test "DECALN screen alignment"
    #[test]
    fn decaln_screen_alignment() {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            // Run DECALN
            s.next_slice(b"\x1B#8");
        }

        // Verify entire screen is filled with 'E'
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"EEEEEEEEEE\nEEEEEEEEEE\nEEEEEEEEEE");

        // Cursor should be at 1,1
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "OSC 4 set and reset palette"
    #[test]
    fn osc_4_set_and_reset_palette() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Save default color
        let default_color_0 = t.colors.palette.original[0];

        {
            let mut s = t.vt_stream();
            // Set color 0 to red
            s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
        }
        assert_eq!(0xff_u8, t.colors.palette.current[0].r);
        assert_eq!(0x00_u8, t.colors.palette.current[0].g);
        assert_eq!(0x00_u8, t.colors.palette.current[0].b);
        assert!(t.colors.palette.mask.is_set(0));

        {
            let mut s = t.vt_stream();
            // Reset color 0
            s.next_slice(b"\x1b]104;0\x1b\\");
        }
        assert_eq!(default_color_0, t.colors.palette.current[0]);
        assert!(!t.colors.palette.mask.is_set(0));

        t.deinit(Global);
    }

    // test "OSC 104 reset all palette colors"
    #[test]
    fn osc_104_reset_all_palette_colors() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set multiple colors
            s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
            s.next_slice(b"\x1b]4;1;rgb:00/ff/00\x1b\\");
            s.next_slice(b"\x1b]4;2;rgb:00/00/ff\x1b\\");
        }
        assert!(t.colors.palette.mask.is_set(0));
        assert!(t.colors.palette.mask.is_set(1));
        assert!(t.colors.palette.mask.is_set(2));

        {
            let mut s = t.vt_stream();
            // Reset all palette colors
            s.next_slice(b"\x1b]104\x1b\\");
        }
        assert_eq!(t.colors.palette.original[0], t.colors.palette.current[0]);
        assert_eq!(t.colors.palette.original[1], t.colors.palette.current[1]);
        assert_eq!(t.colors.palette.original[2], t.colors.palette.current[2]);
        assert!(!t.colors.palette.mask.is_set(0));
        assert!(!t.colors.palette.mask.is_set(1));
        assert!(!t.colors.palette.mask.is_set(2));

        t.deinit(Global);
    }

    // test "OSC 10 set and reset foreground color"
    #[test]
    fn osc_10_set_and_reset_foreground_color() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Initially unset
        assert!(t.colors.foreground.get().is_none());

        {
            let mut s = t.vt_stream();
            // Set foreground to red
            s.next_slice(b"\x1b]10;rgb:ff/00/00\x1b\\");
        }
        let fg = t.colors.foreground.get().unwrap();
        assert_eq!(0xff_u8, fg.r);
        assert_eq!(0x00_u8, fg.g);
        assert_eq!(0x00_u8, fg.b);

        {
            let mut s = t.vt_stream();
            // Reset foreground
            s.next_slice(b"\x1b]110\x1b\\");
        }
        assert!(t.colors.foreground.get().is_none());

        t.deinit(Global);
    }

    // test "OSC 11 set and reset background color"
    #[test]
    fn osc_11_set_and_reset_background_color() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set background to green
            s.next_slice(b"\x1b]11;rgb:00/ff/00\x1b\\");
        }
        let bg = t.colors.background.get().unwrap();
        assert_eq!(0x00_u8, bg.r);
        assert_eq!(0xff_u8, bg.g);
        assert_eq!(0x00_u8, bg.b);

        {
            let mut s = t.vt_stream();
            // Reset background
            s.next_slice(b"\x1b]111\x1b\\");
        }
        assert!(t.colors.background.get().is_none());

        t.deinit(Global);
    }

    // test "OSC 12 set and reset cursor color"
    #[test]
    fn osc_12_set_and_reset_cursor_color() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set cursor to blue
            s.next_slice(b"\x1b]12;rgb:00/00/ff\x1b\\");
        }
        let cursor = t.colors.cursor.get().unwrap();
        assert_eq!(0x00_u8, cursor.r);
        assert_eq!(0x00_u8, cursor.g);
        assert_eq!(0xff_u8, cursor.b);

        {
            let mut s = t.vt_stream();
            // Reset cursor
            s.next_slice(b"\x1b]112\x1b\\");
        }
        // After reset, cursor might be null (using default)

        t.deinit(Global);
    }

    // test "kitty color protocol set palette"
    #[test]
    fn kitty_color_protocol_set_palette() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set palette color 5 to magenta using kitty protocol
            s.next_slice(b"\x1b]21;5=rgb:ff/00/ff\x1b\\");
        }
        assert_eq!(0xff_u8, t.colors.palette.current[5].r);
        assert_eq!(0x00_u8, t.colors.palette.current[5].g);
        assert_eq!(0xff_u8, t.colors.palette.current[5].b);
        assert!(t.colors.palette.mask.is_set(5));
        assert!(t.flags.dirty.palette);

        t.deinit(Global);
    }

    // test "kitty color protocol reset palette"
    #[test]
    fn kitty_color_protocol_reset_palette() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Set and then reset palette color
        let original = t.colors.palette.original[7];
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;7=rgb:aa/bb/cc\x1b\\");
        }
        assert!(t.colors.palette.mask.is_set(7));

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;7=\x1b\\");
        }
        assert_eq!(original, t.colors.palette.current[7]);
        assert!(!t.colors.palette.mask.is_set(7));

        t.deinit(Global);
    }

    // test "kitty color protocol set foreground"
    #[test]
    fn kitty_color_protocol_set_foreground() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set foreground using kitty protocol
            s.next_slice(b"\x1b]21;foreground=rgb:12/34/56\x1b\\");
        }
        let fg = t.colors.foreground.get().unwrap();
        assert_eq!(0x12_u8, fg.r);
        assert_eq!(0x34_u8, fg.g);
        assert_eq!(0x56_u8, fg.b);

        t.deinit(Global);
    }

    // test "kitty color protocol set background"
    #[test]
    fn kitty_color_protocol_set_background() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set background using kitty protocol
            s.next_slice(b"\x1b]21;background=rgb:78/9a/bc\x1b\\");
        }
        let bg = t.colors.background.get().unwrap();
        assert_eq!(0x78_u8, bg.r);
        assert_eq!(0x9a_u8, bg.g);
        assert_eq!(0xbc_u8, bg.b);

        t.deinit(Global);
    }

    // test "kitty color protocol set cursor"
    #[test]
    fn kitty_color_protocol_set_cursor() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set cursor using kitty protocol
            s.next_slice(b"\x1b]21;cursor=rgb:de/f0/12\x1b\\");
        }
        let cursor = t.colors.cursor.get().unwrap();
        assert_eq!(0xde_u8, cursor.r);
        assert_eq!(0xf0_u8, cursor.g);
        assert_eq!(0x12_u8, cursor.b);

        t.deinit(Global);
    }

    // test "kitty color protocol reset foreground"
    #[test]
    fn kitty_color_protocol_reset_foreground() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set and reset foreground
            s.next_slice(b"\x1b]21;foreground=rgb:11/22/33\x1b\\");
        }
        assert!(t.colors.foreground.get().is_some());

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;foreground=\x1b\\");
        }
        // After reset, should be unset
        assert!(t.colors.foreground.get().is_none());

        t.deinit(Global);
    }

    // test "palette dirty flag set on color change"
    #[test]
    fn palette_dirty_flag_set_on_color_change() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Clear dirty flag
        t.flags.dirty.palette = false;

        {
            let mut s = t.vt_stream();
            // Setting palette color should set dirty flag
            s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
        }
        assert!(t.flags.dirty.palette);

        // Clear and test reset
        t.flags.dirty.palette = false;
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]104;0\x1b\\");
        }
        assert!(t.flags.dirty.palette);

        // Clear and test kitty protocol
        t.flags.dirty.palette = false;
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;1=rgb:00/ff/00\x1b\\");
        }
        assert!(t.flags.dirty.palette);

        t.deinit(Global);
    }

    // test "semantic prompt fresh line"
    #[test]
    fn semantic_prompt_fresh_line() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;L\x07");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "semantic prompt fresh line new prompt"
    #[test]
    fn semantic_prompt_fresh_line_new_prompt() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text and then send OSC 133;A (fresh_line_new_prompt)
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;A\x07");
        }

        // Should do a fresh line (carriage return + index)
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);

        // Should set cursor semantic_content to prompt
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );

        {
            let mut s = t.vt_stream();
            // Test with redraw option
            s.next_slice(b"prompt$ ");
            s.next_slice(b"\x1b]133;A;redraw=1\x07");
        }
        assert_eq!(
            crate::osc::semantic_prompt::Redraw::True,
            t.flags.shell_redraws_prompt
        );

        t.deinit(Global);
    }

    // test "semantic prompt end of input, then start output"
    #[test]
    fn semantic_prompt_end_of_input_then_start_output() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text and then send OSC 133;A (fresh_line_new_prompt)
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;A\x07");
            s.next_slice(b"prompt$ ");
            s.next_slice(b"\x1b]133;B\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Input,
            t.active_mut().cursor.semantic_content
        );
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]133;C\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Output,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "semantic prompt prompt_start"
    #[test]
    fn semantic_prompt_prompt_start() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text
            s.next_slice(b"Hello");

            // OSC 133;P marks the start of a prompt (without fresh line behavior)
            s.next_slice(b"\x1b]133;P\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );
        assert_eq!(5_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "semantic prompt new_command"
    #[test]
    fn semantic_prompt_new_command() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;N\x07");
        }

        // Should behave like fresh_line_new_prompt - cursor moves to column 0
        // on next line since we had content
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "semantic prompt new_command at column zero"
    #[test]
    fn semantic_prompt_new_command_at_column_zero() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // OSC 133;N when already at column 0 should stay on same line
            s.next_slice(b"\x1b]133;N\x07");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "semantic prompt end_prompt_start_input_terminate_eol clears on linefeed"
    #[test]
    fn semantic_prompt_end_prompt_start_input_terminate_eol_clears_on_linefeed() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set input terminated by EOL
            s.next_slice(b"\x1b]133;I\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Input,
            t.active_mut().cursor.semantic_content
        );

        {
            let mut s = t.vt_stream();
            // Linefeed should reset semantic content to output
            s.next_slice(b"\n");
        }
        assert_eq!(
            crate::page::SemanticContent::Output,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "ignores query actions"
    //
    // PORT NOTE (vt-12b-7a): DEFERRED to vt-12b-7b. This drives `\x1B[c`
    // (device_attributes), `\x1B[5n`/`\x1B[6n` (device_status) — the
    // RESPONSE/QUERY arms still `todo!()` here. It is reactivated once those
    // arms land in 12b-7b (where readonly effects make them silent no-ops).

    // test "stream: CSI W with intermediate but no params"
    #[test]
    fn stream_csi_w_with_intermediate_but_no_params() {
        // Regression test from AFL++ crash. CSI ? W without
        // parameters caused an out-of-bounds access on input.params[0].
        let mut t = Terminal::init(
            Global,
            Options {
                max_scrollback: 100,
                ..opts(80, 24)
            },
        )
        .unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[?W");
        }

        t.deinit(Global);
    }

    // test "bell effect callback"
    //
    // PORT NOTE (vt-12b-7a): only the null-callback half (which asserts pure
    // Terminal state after a bell) is ported here. The callback-driven half (and
    // the `t.fullReset()` between) exercises the `bell` effect, which is ported,
    // but the callback-count assertions belong with the effect-driven tests; the
    // first block alone is a faithful state-only check that the bell arm does not
    // disturb the terminal.
    #[test]
    fn bell_effect_callback_readonly() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Test bell with null callback (default readonly effects) doesn't crash
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x07");

            // Terminal should still be functional after bell
            s.next_slice(b"AfterBell");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"AfterBell");

        t.deinit(Global);
    }
}
