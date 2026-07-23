//! rustty-vt stream_terminal
//!
//! PORTS: ghostty src/terminal/stream_terminal.zig
//!
//! PORT STATUS: stub
//!
//! vt-12b-7a fills the STATE-mutating dispatch arms (the `vtFallible` arms that
//! call already-ported Terminal methods), the Effects scaffolding, and the
//! simple helpers (`horizontalTab`/`horizontalTabBack`/`setMode`). vt-12b-7b
//! fills the RESPONSE/QUERY arms (device_attributes/device_status/report_*/
//! request_mode*/send_mode_report/query_kitty_keyboard/enquiry/xtversion/
//! size_report/window_title/report_pwd) — they encode a VT response and write it
//! via the Effects `write_pty` callback. Stage A.3 wires the real `apc::Handler`:
//! `apcStart`/`apcPut`/`apcEnd` drive it and `apcEnd` dispatches the kitty
//! graphics + Glyph Protocol responses (the apc cluster is unblocked).

// PORT NOTE (vt-12b-7b): the RESPONSE/QUERY arms are now filled, so the
// `clippy::todo` allow is dropped. `dead_code`/`unused_variables` are retained:
// the reference's no-op arms (`.title_push`/`.title_pop`/`.dcs_*`/
// `.show_desktop_notification`/...) carry value params they ignore, and the
// `vt_raw` argument remains unread.
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
/// PORT NOTE (Stage A.3 — apc complete): the apc cluster is unblocked
/// (rustty-font landed); this is now the real `apc::Handler`, not a ZST stub.
/// `ApcHandler` is a type alias so the field type and `apc.Handler` call sites
/// (`start`/`feed`/`end`/`deinit`) map directly onto the ported handler.
pub type ApcHandler = crate::apc::Handler;

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
            // apc_handler: .{} (the reference field default)
            apc_handler: ApcHandler::default(),
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

    // fn reportDeviceAttributes(self: *Handler, req: device_attributes.Req) void {
    //     const func = self.effects.device_attributes orelse return;
    //     const attrs = func(self);
    //
    //     var stack = std.heap.stackFallback(128, self.terminal.gpa());
    //     const alloc = stack.get();
    //
    //     var aw: std.Io.Writer.Allocating = .init(alloc);
    //     defer aw.deinit();
    //
    //     attrs.encode(req, &aw.writer) catch return;
    //
    //     const written = aw.toOwnedSliceSentinel(0) catch return;
    //     defer alloc.free(written);
    //     self.writePty(written);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference accumulates the encoded response into a
    // `std.Io.Writer.Allocating` (a `stackFallback(128, ...)`-backed growable
    // buffer) and then writes the owned sentinel slice. Rust's `String` is the
    // growable byte accumulator here; `device_attributes::Attributes::encode`
    // writes via `core::fmt::Write` (matching the reference `attrs.encode`), and
    // an encode failure (`catch return`) is the `is_err()` early-out. The
    // resulting bytes (no sentinel needed at the `&[u8]` callback boundary) are
    // handed to `writePty`.
    fn report_device_attributes(&mut self, req: device_attributes::Req) {
        let Some(func) = self.effects.device_attributes else {
            return;
        };
        let attrs = func(self);

        let mut aw = String::new();
        if attrs.encode(req, &mut aw).is_err() {
            return;
        }

        self.write_pty(aw.as_bytes());
    }

    // fn deviceStatus(self: *Handler, req: device_status.Request) void {
    //     switch (req) {
    //         .operating_status => self.writePty("\x1B[0n"),
    //
    //         .cursor_position => { ... },
    //
    //         .color_scheme => { ... },
    //     }
    // }
    fn device_status(&mut self, req: device_status::Request) {
        match req {
            // .operating_status => self.writePty("\x1B[0n"),
            device_status::Request::OperatingStatus => self.write_pty(b"\x1B[0n"),

            // .cursor_position => {
            //     const pos: struct { x: usize, y: usize } =
            //         if (self.terminal.modes.get(.origin)) .{
            //             .x = self.terminal.screens.active.cursor.x -| self.terminal.scrolling_region.left,
            //             .y = self.terminal.screens.active.cursor.y -| self.terminal.scrolling_region.top,
            //         } else .{
            //             .x = self.terminal.screens.active.cursor.x,
            //             .y = self.terminal.screens.active.cursor.y,
            //         };
            //
            //     var buf: [64]u8 = undefined;
            //     const resp = std.fmt.bufPrintZ(&buf, "\x1B[{};{}R", .{ pos.y + 1, pos.x + 1 }) catch return;
            //     self.writePty(resp);
            // },
            device_status::Request::CursorPosition => {
                let (x, y): (usize, usize) = if self.term().modes.get(modes::Mode::Origin) {
                    let cx = self.term().active_mut().cursor.x;
                    let cy = self.term().active_mut().cursor.y;
                    let left = self.term().scrolling_region.left;
                    let top = self.term().scrolling_region.top;
                    (
                        cx.saturating_sub(left) as usize,
                        cy.saturating_sub(top) as usize,
                    )
                } else {
                    (
                        self.term().active_mut().cursor.x as usize,
                        self.term().active_mut().cursor.y as usize,
                    )
                };

                // PORT NOTE: the reference formats into a fixed `[64]u8` via
                // `bufPrintZ`; Rust's `String` accumulator is the analog (the
                // 64-byte cap is a Zig stack-buffer detail, not observable).
                let resp = format!("\x1B[{};{}R", y + 1, x + 1);
                self.write_pty(resp.as_bytes());
            }

            // .color_scheme => {
            //     const func = self.effects.color_scheme orelse return;
            //     const scheme = func(self) orelse return;
            //     self.writePty(switch (scheme) {
            //         .dark => "\x1B[?997;1n",
            //         .light => "\x1B[?997;2n",
            //     });
            // },
            device_status::Request::ColorScheme => {
                let Some(func) = self.effects.color_scheme else {
                    return;
                };
                let Some(scheme) = func(self) else {
                    return;
                };
                self.write_pty(match scheme {
                    device_status::ColorScheme::Dark => b"\x1B[?997;1n",
                    device_status::ColorScheme::Light => b"\x1B[?997;2n",
                });
            }
        }
    }

    // fn reportEnquiry(self: *Handler) void {
    //     const func = self.effects.enquiry orelse return;
    //     const response = func(self);
    //     if (response.len == 0) return;
    //     var buf: [256]u8 = undefined;
    //     if (response.len >= buf.len) return;
    //     @memcpy(buf[0..response.len], response);
    //     buf[response.len] = 0;
    //     self.writePty(buf[0..response.len :0]);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference copies the response into a fixed
    // `[256]u8` (NUL-terminating it) and rejects responses `>= 256` bytes. We
    // preserve both guards (empty -> silent; `>= 256` -> silent); the sentinel
    // copy is a Zig encoding detail, so we pass the response slice directly.
    fn report_enquiry(&mut self) {
        let Some(func) = self.effects.enquiry else {
            return;
        };
        let response = func(self);
        if response.is_empty() {
            return;
        }
        if response.len() >= 256 {
            return;
        }
        self.write_pty(response);
    }

    // fn reportXtversion(self: *Handler) void {
    //     const version = if (self.effects.xtversion) |func| func(self) else "";
    //     var buf: [288]u8 = undefined;
    //     const resp = std.fmt.bufPrintZ(
    //         &buf,
    //         "\x1BP>|{s}\x1B\\",
    //         .{if (version.len > 0) version else "libghostty"},
    //     ) catch return;
    //     self.writePty(resp);
    // }
    //
    // PORT NOTE (R2 ghostty->Rustty): the reference's fallback version string is
    // "libghostty"; retargeted to "librustty" per the rename rule. The DCS
    // wrapping (`ESC P > | <version> ESC \`) is unchanged.
    fn report_xtversion(&mut self) {
        let version: &[u8] = if let Some(func) = self.effects.xtversion {
            func(self)
        } else {
            b""
        };
        // PORT NOTE: the reference formats into a fixed `[288]u8`; Rust's `Vec`
        // accumulator is the analog. A non-UTF-8 version slice is written
        // verbatim, so we build the byte sequence directly rather than via a
        // format string (the `{s}` in the reference is a raw byte splice).
        let mut resp: Vec<u8> = Vec::new();
        resp.extend_from_slice(b"\x1BP>|");
        if version.is_empty() {
            resp.extend_from_slice(b"librustty");
        } else {
            resp.extend_from_slice(version);
        }
        resp.extend_from_slice(b"\x1B\\");
        self.write_pty(&resp);
    }

    // fn reportSize(self: *Handler, style: csi.SizeReportStyle) void { ... }
    fn report_size(&mut self, style: csi::SizeReportStyle) {
        // Almost all size reports will fit in 256 bytes so try that
        // on the stack before falling back to a heap allocation.
        // var stack = std.heap.stackFallback(256, self.terminal.gpa());
        // const alloc = stack.get();
        //
        // // Allocating writing to accumulate the response.
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        // defer aw.deinit();
        //
        // PORT NOTE (vt-12b-7b): the reference accumulates into a
        // `stackFallback(256, ...)`-backed `std.Io.Writer.Allocating`; Rust's
        // `String` is the growable accumulator. An encode/print failure
        // (`catch return` / `catch |err| { log.warn(...); return; }`) is the
        // `is_err()` early-out.
        let mut aw = String::new();

        // Build the response.
        match style {
            // .csi_21_t => {
            //     const title = self.terminal.getTitle() orelse "";
            //     aw.writer.print("\x1b]l{s}\x1b\\", .{title}) catch return;
            // },
            csi::SizeReportStyle::Csi21T => {
                // PORT NOTE: the title is a raw byte slice spliced via `{s}`; we
                // build the bytes directly to avoid assuming UTF-8.
                let mut resp: Vec<u8> = Vec::new();
                resp.extend_from_slice(b"\x1b]l");
                if let Some(title) = self.term().get_title() {
                    resp.extend_from_slice(title);
                }
                resp.extend_from_slice(b"\x1b\\");
                self.write_pty(&resp);
                return;
            }

            // .csi_14_t, .csi_16_t, .csi_18_t => {
            //     const get_size = self.effects.size orelse return;
            //     const s = get_size(self) orelse return;
            //     const report_style: size_report.Style = switch (style) {
            //         .csi_14_t => .csi_14_t,
            //         .csi_16_t => .csi_16_t,
            //         .csi_18_t => .csi_18_t,
            //         .csi_21_t => unreachable,
            //     };
            //     size_report.encode(&aw.writer, report_style, s) catch |err| {
            //         log.warn("error encoding size report err={}", .{err});
            //         return;
            //     };
            // },
            csi::SizeReportStyle::Csi14T
            | csi::SizeReportStyle::Csi16T
            | csi::SizeReportStyle::Csi18T => {
                let Some(get_size) = self.effects.size else {
                    return;
                };
                let Some(s) = get_size(self) else {
                    return;
                };
                let report_style: size_report::Style = match style {
                    csi::SizeReportStyle::Csi14T => size_report::Style::Csi14T,
                    csi::SizeReportStyle::Csi16T => size_report::Style::Csi16T,
                    csi::SizeReportStyle::Csi18T => size_report::Style::Csi18T,
                    csi::SizeReportStyle::Csi21T => unreachable!(),
                };
                if size_report::encode(&mut aw, report_style, s).is_err() {
                    // log.warn("error encoding size report err={}", .{err});
                    return;
                }
            }
        }

        // const resp = aw.toOwnedSliceSentinel(0) catch return;
        // defer alloc.free(resp);
        // self.writePty(resp);
        self.write_pty(aw.as_bytes());
    }

    // fn windowTitle(self: *Handler, title_raw: []const u8) void { ... }
    fn do_window_title(&mut self, title_raw: &[u8]) {
        // Prevent DoS attacks by limiting title length.
        // const max_title_len = 1024;
        // const title = if (title_raw.len > max_title_len) title: {
        //     log.warn("title length {d} exceeds max length {d}, truncating", ...);
        //     break :title title_raw[0..max_title_len];
        // } else title_raw;
        let max_title_len = 1024;
        let title = if title_raw.len() > max_title_len {
            // log.warn("title length {d} exceeds max length {d}, truncating", ...);
            &title_raw[0..max_title_len]
        } else {
            title_raw
        };

        // self.terminal.setTitle(title) catch |err| {
        //     log.warn("error setting title err={}", .{err});
        //     return;
        // };
        if self.term().set_title(title).is_err() {
            // log.warn("error setting title err={}", .{err});
            return;
        }

        // const func = self.effects.title_changed orelse return;
        // func(self);
        let Some(func) = self.effects.title_changed else {
            return;
        };
        func(self);
    }

    // fn reportPwd(self: *Handler, url_raw: []const u8) void { ... }
    fn do_report_pwd(&mut self, url_raw: &[u8]) {
        // Prevent DoS attacks by limiting url length. Headroom for
        // Linux PATH_MAX (4096) plus URI scheme/host and percent-encoding.
        // const max_url_len = 4096;
        // const url = if (url_raw.len > max_url_len) url: {
        //     log.warn("pwd url length {d} exceeds max length {d}, truncating", ...);
        //     break :url url_raw[0..max_url_len];
        // } else url_raw;
        let max_url_len = 4096;
        let url = if url_raw.len() > max_url_len {
            // log.warn("pwd url length {d} exceeds max length {d}, truncating", ...);
            &url_raw[0..max_url_len]
        } else {
            url_raw
        };

        // We store the raw payload unparsed. Embedders read it via
        // getPwd() and are responsible for decoding any URI scheme.
        // self.terminal.setPwd(url) catch |err| {
        //     log.warn("error setting pwd err={}", .{err});
        //     return;
        // };
        if self.term().set_pwd(url).is_err() {
            // log.warn("error setting pwd err={}", .{err});
            return;
        }

        // const func = self.effects.pwd_changed orelse return;
        // func(self);
        let Some(func) = self.effects.pwd_changed else {
            return;
        };
        func(self);
    }

    // fn requestMode(self: *Handler, mode: modes.Mode) void {
    //     const report = self.terminal.modes.getReport(.fromMode(mode));
    //     self.sendModeReport(report);
    // }
    fn do_request_mode(&mut self, mode: modes::Mode) {
        let report = self
            .term()
            .modes
            .get_report(modes::ModeTag::from_mode(mode));
        self.send_mode_report(report);
    }

    // fn requestModeUnknown(self: *Handler, mode_raw: u16, ansi: bool) void {
    //     const report = self.terminal.modes.getReport(.{
    //         .value = @truncate(mode_raw),
    //         .ansi = ansi,
    //     });
    //     self.sendModeReport(report);
    // }
    fn do_request_mode_unknown(&mut self, mode_raw: u16, ansi: bool) {
        let report = self.term().modes.get_report(modes::ModeTag {
            // .value = @truncate(mode_raw),
            value: mode_raw & 0x7FFF,
            ansi,
        });
        self.send_mode_report(report);
    }

    // fn sendModeReport(self: *Handler, report: modes.Report) void {
    //     var buf: [modes.Report.max_size + 1]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(&buf);
    //     report.encode(&writer) catch |err| {
    //         log.warn("error encoding mode report err={}", .{err});
    //         return;
    //     };
    //     const len = writer.buffered().len;
    //     buf[len] = 0;
    //     self.writePty(buf[0..len :0]);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference encodes into a fixed
    // `[Report.max_size + 1]u8` (the +1 is the NUL sentinel slot). Rust's
    // `String` accumulator is the analog; the sentinel slot is a Zig encoding
    // detail, so we pass the buffered bytes directly.
    fn send_mode_report(&mut self, report: modes::Report) {
        let mut buf = String::new();
        if report.encode(&mut buf).is_err() {
            // log.warn("error encoding mode report err={}", .{err});
            return;
        }
        self.write_pty(buf.as_bytes());
    }

    // fn queryKittyKeyboard(self: *Handler) void {
    //     // Max response is "\x1b[?31u\x00" (7 bytes): the flags are a u5 (max 31).
    //     var buf: [32]u8 = undefined;
    //     const resp = std.fmt.bufPrintZ(&buf, "\x1b[?{}u", .{
    //         self.terminal.screens.active.kitty_keyboard.current().int(),
    //     }) catch return;
    //     self.writePty(resp);
    // }
    fn query_kitty_keyboard(&mut self) {
        // Max response is "\x1b[?31u\x00" (7 bytes): the flags are a u5 (max 31).
        // PORT NOTE: the reference formats into a fixed `[32]u8`; Rust's `String`
        // is the analog accumulator.
        let flags = self.term().active_mut().kitty_keyboard.current().int();
        let resp = format!("\x1b[?{flags}u");
        self.write_pty(resp.as_bytes());
    }

    // fn apcEnd(self: *Handler) void {
    //     const alloc = self.terminal.gpa();
    //     var cmd = self.apc_handler.end() orelse return;
    //     defer cmd.deinit(alloc);
    //     switch (cmd) { ... };
    // }
    // PORT NOTE (Stage A.3 — apc complete): the apc cluster is unblocked. The
    // reference `end()` takes no allocator; the Rust `apc::Handler::end` threads
    // `alloc` for call-site fidelity (kitty `complete` consumes it in the
    // reference). The `defer cmd.deinit(alloc)` is reproduced by an explicit
    // `cmd.deinit(alloc)` at every exit path (both arms own their data via `Vec`s
    // and free on drop, so it is a no-op fidelity hook). The reference buffers the
    // response then appends a `\0` and writes `final[0..final.len-1 :0]` (a Zig
    // sentinel slice); our `write_pty` takes a bare `&[u8]`, so we write the
    // encoded bytes directly with no sentinel byte.
    fn apc_end(&mut self) {
        let alloc = self.term().gpa();
        let Some(mut cmd) = self.apc_handler.end(alloc) else {
            return;
        };

        match &mut cmd {
            // .kitty => |*kitty_cmd| if (comptime build_options.kitty_graphics) {
            crate::apc::Command::Kitty(kitty_cmd) => {
                if crate::options::KITTY_GRAPHICS {
                    // if (self.terminal.kittyGraphics(alloc, kitty_cmd)) |resp| resp: {
                    if let Some(resp) = self.term().kitty_graphics(alloc, kitty_cmd) {
                        'resp: {
                            // Don't waste time encoding if we can't write responses
                            // anyways.
                            // if (self.effects.write_pty == null) break :resp;
                            if self.effects.write_pty.is_none() {
                                break 'resp;
                            }

                            // Encode and write the response if we have one.
                            // var buf: [1024]u8 = undefined;
                            // var writer: std.Io.Writer = .fixed(&buf);
                            // resp.encode(&writer) catch return;
                            // writer.writeByte(0) catch return;
                            // const final = writer.buffered();
                            // if (final.len > 3) self.writePty(final[0 .. final.len - 1 :0]);
                            // PORT NOTE: the reference encodes into a fixed `[1024]u8`
                            // and only writes when `final.len > 3` (i.e. more than the
                            // bare `\x1b_\x1b\\` framing plus the appended `\0`). With
                            // no sentinel byte the guard is `final.len > 2` on the
                            // encoded bytes — but the kitty `Response::encode` only
                            // emits when there is a real payload, so we keep the
                            // reference's `> 3` semantics by comparing the encoded
                            // length plus the (omitted) sentinel byte.
                            let mut buf: Vec<u8> = Vec::new();
                            // PORT NOTE: the reference `resp.encode(...) catch
                            // return` (and the glyph arm's `formatWire ... catch
                            // return`) bail without the `defer cmd.deinit` having
                            // run yet — but `cmd` is `var ...defer deinit` so it
                            // still deinits on the deferred path. Here `cmd` drops
                            // at scope end (freeing its data), so the bare `return`
                            // reproduces the reference cleanup without re-borrowing
                            // `cmd` while `kitty_cmd`/`glyph_req` is live.
                            if resp.encode(&mut buf).is_err() {
                                return;
                            }
                            let final_ = buf;
                            if final_.len() + 1 > 3 {
                                self.write_pty(&final_);
                            }
                        }
                    }
                } else {
                    unreachable!();
                }
            }

            // .glyph => |*glyph_req| {
            crate::apc::Command::Glyph(glyph_req) => {
                // const resp = self.terminal.glyphProtocol(alloc, glyph_req);
                let resp = self.term().glyph_protocol(alloc, glyph_req);
                // if (resp) |r| resp_block: {
                if let Some(r) = resp {
                    'resp_block: {
                        // Don't waste time encoding if we can't write responses
                        // anyways.
                        // if (self.effects.write_pty == null) break :resp_block;
                        if self.effects.write_pty.is_none() {
                            break 'resp_block;
                        }

                        // Glyph responses are short and bounded by the protocol
                        // fields we emit, so this matches the Kitty response
                        // buffer size above with ample headroom.
                        // var buf: [apc.glyph.Response.max_wire_bytes]u8 = undefined;
                        // var writer: std.Io.Writer = .fixed(&buf);
                        // r.formatWire(&writer) catch return;
                        // writer.writeByte(0) catch return;
                        // const final = writer.buffered();
                        // self.writePty(final[0 .. final.len - 1 :0]);
                        // PORT NOTE: `formatWire` writes to a `std.Io.Writer` over a
                        // fixed `[max_wire_bytes]u8`; the Rust `format_wire` writes to
                        // a `String` (the bounded wire payload is ASCII). We then
                        // write the UTF-8 bytes directly (no sentinel `\0`).
                        let mut buf = String::new();
                        // PORT NOTE: see the kitty-arm note above — bare `return`
                        // (cmd drops/frees) reproduces the reference `catch return`.
                        if r.format_wire(&mut buf).is_err() {
                            return;
                        }
                        self.write_pty(buf.as_bytes());
                    }
                }
            }
        }

        cmd.deinit(alloc);
    }
}

// impl stream::Handler for Handler ------------------------------------------
//
// PORT NOTE: each method body is the corresponding per-action arm of the
// reference `vtFallible` switch (plus the OSC/DCS/APC helpers): the
// state-mutating arms landed in vt-12b-7a and the response/query arms in
// vt-12b-7b. The APC end + kitty/glyph arms remain stubbed (font-blocked,
// deferred past vt-12; see DEFERRALS.md).
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
    fn print_repeat(&mut self, count: usize) {
        self.vt(|t| t.print_repeat(count));
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
        self.report_enquiry();
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
        self.do_request_mode(v.mode);
    }

    // .request_mode_unknown => self.requestModeUnknown(value.mode, value.ansi),
    fn request_mode_unknown(&mut self, v: stream::RawMode) {
        self.do_request_mode_unknown(v.mode, v.ansi);
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
        self.report_size(v);
    }

    // .title_push, .title_pop => {} (have no terminal-modifying effect)
    fn title_push(&mut self, v: u16) {}

    // .title_push, .title_pop => {} (have no terminal-modifying effect)
    fn title_pop(&mut self, v: u16) {}

    // .xtversion => self.reportXtversion(),
    fn xtversion(&mut self) {
        self.report_xtversion();
    }

    // .device_attributes => self.reportDeviceAttributes(value),
    fn device_attributes(&mut self, req: device_attributes::Req) {
        self.report_device_attributes(req);
    }

    // .device_status => self.deviceStatus(value.request),
    fn device_status(&mut self, v: stream::DeviceStatus) {
        Handler::device_status(self, v.request);
    }

    // .kitty_keyboard_query => self.queryKittyKeyboard(),
    fn kitty_keyboard_query(&mut self) {
        self.query_kitty_keyboard();
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
    fn apc_start(&mut self) {
        self.apc_handler.start();
    }

    // .apc_put => self.apc_handler.feed(self.terminal.gpa(), value),
    fn apc_put(&mut self, code: u8) {
        let gpa = self.term().gpa();
        self.apc_handler.feed(gpa, code);
    }

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
        self.do_window_title(v.title);
    }

    // .report_pwd => self.reportPwd(value.url),
    fn report_pwd(&mut self, v: stream::ReportPwd) {
        self.do_report_pwd(v.url);
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
    // .{ .cols, .rows })` and drive bytes through a `Stream`. Rustty's `Options`
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

    // WS3-A KAT (rustty addition, not a reference test): a kitty-graphics PNG
    // transmit sequence through the vt Stream decodes via the wuffs DEFAULT
    // hook — no embedder installed a decoder — proving the non-lib comptime
    // default (sys.zig:18-24) survives the port. `q=2` (quiet always) keeps the
    // handler from emitting an OK response to the (test) pty.
    #[cfg(feature = "wuffs")]
    #[test]
    fn kitty_graphics_png_transmit_decodes_via_default_hook() {
        // Base64 of kitty/testdata/image-png-none-50x76-2147483647-raw.data
        // (the `t=d` direct-medium payload is base64 per the kitty protocol).
        const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAADIAAABMAQMAAAAV2XZDAAAAA1BMVEUrLzc5wwm5AAAADklEQVR4AWOgIRgFowAAAmAAATnED6oAAAAASUVORK5CYII=";

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // APC G: a=t (transmit), f=100 (PNG), t=d (direct), i=31, q=2.
            let seq = format!("\x1b_Ga=t,f=100,t=d,i=31,q=2;{PNG_B64}\x1b\\");
            s.next_slice(seq.as_bytes());
        }
        // The graphics_exec.rs:172 storage-access idiom.
        let img = unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(31)
            .expect("transmitted image stored (PNG decoded by the default hook)");
        assert_eq!(50, img.width);
        assert_eq!(76, img.height);
        assert_eq!(img.format, kitty::graphics_command::Format::Rgba);
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

    // test "full reset"
    #[test]
    fn full_reset() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Make some changes
            s.next_slice(b"Hello");
            s.next_slice(b"\x1B[10;20H");
            s.next_slice(b"\x1B[5;20r"); // Set scroll region
            s.next_slice(b"\x1B[?7l"); // Disable wraparound
            s.next_slice(b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
        }
        // try testing.expect(t.glyph_glossary.contains(0xE0A0));
        assert!(t.glyph_glossary.contains(0xE0A0));

        {
            let mut s = t.vt_stream();
            // Full reset
            s.next_slice(b"\x1Bc");
        }

        // Verify reset state
        assert_eq!(0usize, t.active_mut().cursor.x as usize);
        assert_eq!(0usize, t.active_mut().cursor.y as usize);
        assert_eq!(0usize, t.scrolling_region.top as usize);
        assert_eq!(23usize, t.scrolling_region.bottom as usize);
        assert!(t.modes.get(modespkg::Mode::Wraparound));
        // try testing.expect(!t.glyph_glossary.contains(0xE0A0));
        assert!(!t.glyph_glossary.contains(0xE0A0));

        t.deinit(Global);
    }

    // test "glyph protocol APC with write_pty callback"
    #[test]
    fn glyph_protocol_apc_with_write_pty_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        // handler.effects.write_pty = &S.writePty;
        s.handler.effects.write_pty = Some(cap_write_pty);

        // s.nextSlice("\x1B_25a1;s\x1B\\");
        s.next_slice(b"\x1B_25a1;s\x1B\\");
        // try testing.expectEqualStrings("\x1B_25a1;s;fmt=glyf\x1B\\", S.last_response.?);
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1B_25a1;s;fmt=glyf\x1B\\"
        );

        // s.nextSlice("\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
        s.next_slice(b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
        // try testing.expectEqualStrings("\x1B_25a1;r;cp=e0a0;status=0\x1B\\", S.last_response.?);
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1B_25a1;r;cp=e0a0;status=0\x1B\\"
        );
        // try testing.expect(t.glyph_glossary.contains(0xE0A0));
        drop(s);
        assert!(t.glyph_glossary.contains(0xE0A0));

        t.deinit(Global);
    }

    // test "ignores query actions"
    //
    // PORT NOTE (vt-12b-7b): reactivated now that the RESPONSE/QUERY arms are
    // filled. With default (readonly) effects, `\x1B[c` (device_attributes) and
    // `\x1B[5n`/`\x1B[6n` (device_status) are silent no-ops, so the terminal
    // remains functional afterward.
    #[test]
    fn ignores_query_actions() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // These should be ignored without error
            s.next_slice(b"\x1B[c"); // Device attributes
            s.next_slice(b"\x1B[5n"); // Device status report
            s.next_slice(b"\x1B[6n"); // Cursor position report

            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

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
    // PORT NOTE (vt-12b-7a): the reference's callback-count half captures the
    // count in a local-struct `var` static (`S.bell_count`); Rust's `bell`
    // effect is a bare `fn` pointer (no closures/captures), so the count is
    // routed through the tests-module `COUNTER` thread_local cell — the same
    // analog the vt-12b-7b effect tests below use for `title_changed`.
    #[test]
    fn bell_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Test bell with null callback (default readonly effects) doesn't crash
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x07");

            // Terminal should still be functional after bell
            s.next_slice(b"AfterBell");
        }
        let str = t.plain_string(Global).unwrap();
        // try testing.expectEqualStrings("AfterBell", str);
        assert_eq!(str.as_slice(), b"AfterBell");

        // t.fullReset();
        t.full_reset();

        // Test bell with a callback
        {
            // const S = struct { var bell_count: usize = 0;
            //     fn bell(_: *Handler) void { bell_count += 1; } };
            fn bell(_: &mut Handler) {
                COUNTER.with(|c| c.set(c.get() + 1));
            }
            // S.bell_count = 0;
            cap_reset();

            // var handler: Handler = .init(&t); handler.effects.bell = &S.bell;
            let mut s = t.vt_stream();
            s.handler.effects.bell = Some(bell);

            s.next_slice(b"\x07");
            // try testing.expectEqual(@as(usize, 1), S.bell_count);
            assert_eq!(1_usize, COUNTER.with(|c| c.get()));

            s.next_slice(b"\x07\x07");
            // try testing.expectEqual(@as(usize, 3), S.bell_count);
            assert_eq!(3_usize, COUNTER.with(|c| c.get()));
        }

        t.deinit(Global);
    }

    // -- vt-12b-7b response/query effect tests --------------------------------
    //
    // PORT NOTE (vt-12b-7b): the reference tests capture the `write_pty` output
    // into a local-struct `var` static (`S.written`/`S.last_response`) inside a
    // `fn writePty(...)` callback. Rust's `write_pty` is a bare `fn` pointer (no
    // closures/captures), so the captured bytes are routed through a
    // `thread_local!` cell. The `CAPTURE` helpers below are the analog of the
    // reference's per-test `S` struct: `cap_write_pty` is the `writePty` callback
    // (it stores the latest response, freeing/replacing the prior one — matching
    // the reference's `if (old) free; dupe` pattern); `cap_take`/`cap_reset` read
    // and clear it. Effect callbacks that return fixed strings
    // (`enquiry`/`xtversion`/`getSize`/`colorScheme`/`da`/`titleChanged`) become
    // free `fn`s. The `title_changed`/`bell` counters likewise live in a cell.

    thread_local! {
        static CAPTURE: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
        static COUNTER: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    fn cap_reset() {
        CAPTURE.with(|c| *c.borrow_mut() = None);
        COUNTER.with(|c| c.set(0));
    }

    fn cap_take() -> Option<Vec<u8>> {
        CAPTURE.with(|c| c.borrow().clone())
    }

    // fn writePty(_: *Handler, data: [:0]const u8) void { ...dupe into S.written... }
    fn cap_write_pty(_: &mut Handler, data: &[u8]) {
        CAPTURE.with(|c| *c.borrow_mut() = Some(data.to_vec()));
    }

    // test "request mode DECRQM with write_pty callback"
    #[test]
    fn request_mode_decrqm_with_write_pty_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Without callback, DECRQM should not crash
        {
            let mut s = t.vt_stream();
            // DECRQM for mode 7 (wraparound) — should be silently ignored
            s.next_slice(b"\x1B[?7$p");
        }

        t.full_reset();

        // With callback, DECRQM should produce a response
        {
            cap_reset();
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(cap_write_pty);

            // Wraparound mode (7) is set by default
            s.next_slice(b"\x1B[?7$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?7;1$y");

            // Disable wraparound and query again
            s.next_slice(b"\x1B[?7l");
            s.next_slice(b"\x1B[?7$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?7;2$y");

            // Query an unknown mode
            s.next_slice(b"\x1B[?9999$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?9999;0$y");
        }

        t.deinit(Global);
    }

    // test "window_title effect is called"
    #[test]
    fn window_title_effect_is_called() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn titleChanged(_: *Handler) void { title_changed_count += 1; }
        fn title_changed(_: &mut Handler) {
            COUNTER.with(|c| c.set(c.get() + 1));
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.title_changed = Some(title_changed);

        {
            // Set window title via OSC 2
            s.next_slice(b"\x1b]2;Hello World\x1b\\");
        }
        assert_eq!(t.get_title().unwrap(), b"Hello World");
        assert_eq!(1_usize, COUNTER.with(|c| c.get()));

        t.deinit(Global);
    }

    // test "window_title effect not called without callback"
    #[test]
    fn window_title_effect_not_called_without_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Should not crash when no callback is set
            s.next_slice(b"\x1b]2;Hello World\x1b\\");
        }

        // Title should still be set on terminal state
        assert_eq!(t.get_title().unwrap(), b"Hello World");

        {
            let mut s = t.vt_stream();
            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // test "window_title effect with empty title"
    #[test]
    fn window_title_effect_with_empty_title() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        fn title_changed(_: &mut Handler) {
            COUNTER.with(|c| c.set(c.get() + 1));
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.title_changed = Some(title_changed);

        {
            // Set empty window title
            s.next_slice(b"\x1b]2;\x1b\\");
        }
        assert!(t.get_title().is_none());
        assert_eq!(1_usize, COUNTER.with(|c| c.get()));

        t.deinit(Global);
    }

    // test "kitty_keyboard_query"
    #[test]
    fn kitty_keyboard_query() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Default kitty keyboard flags should be 0
        s.next_slice(b"\x1b[?u");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?0u");

        // Push kitty keyboard mode with flags and query again
        cap_reset();
        s.next_slice(b"\x1b[>1u"); // push with disambiguate flag
        s.next_slice(b"\x1b[?u");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?1u");

        drop(s);
        t.deinit(Global);
    }

    // test "xtversion default"
    //
    // PORT NOTE (R2 ghostty->Rustty): the reference asserts the fallback version
    // "libghostty"; the retargeted fallback emits "librustty".
    #[test]
    fn xtversion_default() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Without xtversion effect set, should report "librustty"
            s.next_slice(b"\x1b[>0q");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP>|librustty\x1b\\");

        t.deinit(Global);
    }

    // test "xtversion with effect"
    #[test]
    fn xtversion_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn xtversion(_: *Handler) []const u8 { return "ghostty 1.2.3"; }
        fn xtversion(_: &mut Handler) -> &'static [u8] {
            b"ghostty 1.2.3"
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.xtversion = Some(xtversion);

        {
            s.next_slice(b"\x1b[>0q");
        }
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1bP>|ghostty 1.2.3\x1b\\"
        );

        t.deinit(Global);
    }

    // test "xtversion with empty string effect"
    //
    // PORT NOTE (R2 ghostty->Rustty): the empty-string effect falls back to the
    // retargeted "librustty".
    #[test]
    fn xtversion_with_empty_string_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn xtversion(_: *Handler) []const u8 { return ""; }
        fn xtversion(_: &mut Handler) -> &'static [u8] {
            b""
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.xtversion = Some(xtversion);

        {
            // Empty string from effect should fall back to "librustty"
            s.next_slice(b"\x1b[>0q");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP>|librustty\x1b\\");

        t.deinit(Global);
    }

    // fn getSize(_: *Handler) ?size_report.Size {
    //     return .{ .rows = 24, .columns = 80, .cell_width = 9, .cell_height = 18 };
    // }
    fn get_size(_: &mut Handler) -> Option<size_report::Size> {
        Some(size_report::Size {
            rows: 24,
            columns: 80,
            cell_width: 9,
            cell_height: 18,
        })
    }

    // test "size report csi_14_t with effect"
    #[test]
    fn size_report_csi_14_t_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.size = Some(get_size);

        {
            // CSI 14 t - report text area size in pixels
            s.next_slice(b"\x1b[14t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[4;432;720t");

        t.deinit(Global);
    }

    // test "size report csi_16_t with effect"
    #[test]
    fn size_report_csi_16_t_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.size = Some(get_size);

        {
            // CSI 16 t - report cell size in pixels
            s.next_slice(b"\x1b[16t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[6;18;9t");

        t.deinit(Global);
    }

    // test "size report csi_18_t with effect"
    #[test]
    fn size_report_csi_18_t_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.size = Some(get_size);

        {
            // CSI 18 t - report text area size in characters
            s.next_slice(b"\x1b[18t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[8;24;80t");

        t.deinit(Global);
    }

    // test "size report no effect callback"
    #[test]
    fn size_report_no_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Without size effect, size reports should be silently ignored
            s.next_slice(b"\x1b[14t");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "size report csi_21_t title"
    #[test]
    fn size_report_csi_21_t_title() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Set a title first
            s.next_slice(b"\x1b]2;My Title\x1b\\");

            // CSI 21 t - report title (no size effect needed)
            s.next_slice(b"\x1b[21t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b]lMy Title\x1b\\");

        t.deinit(Global);
    }

    // test "enquiry no effect"
    #[test]
    fn enquiry_no_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // ENQ without enquiry effect should not write anything
            s.next_slice(b"\x05");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "enquiry with effect"
    #[test]
    fn enquiry_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn enquiry(_: *Handler) []const u8 { return "ghostty"; }
        fn enquiry(_: &mut Handler) -> &'static [u8] {
            b"ghostty"
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.enquiry = Some(enquiry);

        {
            s.next_slice(b"\x05");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"ghostty");

        t.deinit(Global);
    }

    // test "enquiry with empty response"
    #[test]
    fn enquiry_with_empty_response() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn enquiry(_: *Handler) []const u8 { return ""; }
        fn enquiry(_: &mut Handler) -> &'static [u8] {
            b""
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.enquiry = Some(enquiry);

        {
            // Empty enquiry response should not write anything
            s.next_slice(b"\x05");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "device status: operating status"
    #[test]
    fn device_status_operating_status() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // CSI 5 n — operating status report
            s.next_slice(b"\x1B[5n");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[0n");

        t.deinit(Global);
    }

    // test "device status: cursor position"
    #[test]
    fn device_status_cursor_position() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Default position is 0,0 — reported as 1,1
        s.next_slice(b"\x1B[6n");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[1;1R");

        // Move cursor to row 5, col 10
        s.next_slice(b"\x1B[5;10H");
        s.next_slice(b"\x1B[6n");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[5;10R");

        drop(s);
        t.deinit(Global);
    }

    // test "device status: cursor position with origin mode"
    #[test]
    fn device_status_cursor_position_with_origin_mode() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Set scroll region rows 5-20
            s.next_slice(b"\x1B[5;20r");
            // Enable origin mode
            s.next_slice(b"\x1B[?6h");
            // Move to row 3, col 5 within the region
            s.next_slice(b"\x1B[3;5H");
            // Query cursor position
            s.next_slice(b"\x1B[6n");
        }
        // Should report position relative to the scroll region
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[3;5R");

        t.deinit(Global);
    }

    // fn colorScheme(_: *Handler) ?device_status.ColorScheme { return .dark; }
    fn color_scheme_dark(_: &mut Handler) -> Option<device_status::ColorScheme> {
        Some(device_status::ColorScheme::Dark)
    }

    // fn colorScheme(_: *Handler) ?device_status.ColorScheme { return .light; }
    fn color_scheme_light(_: &mut Handler) -> Option<device_status::ColorScheme> {
        Some(device_status::ColorScheme::Light)
    }

    // test "device status: color scheme dark"
    #[test]
    fn device_status_color_scheme_dark() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.color_scheme = Some(color_scheme_dark);

        {
            // CSI ? 996 n — color scheme query
            s.next_slice(b"\x1B[?996n");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?997;1n");

        t.deinit(Global);
    }

    // test "device status: color scheme light"
    #[test]
    fn device_status_color_scheme_light() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.color_scheme = Some(color_scheme_light);

        {
            // CSI ? 996 n — color scheme query
            s.next_slice(b"\x1B[?996n");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?997;2n");

        t.deinit(Global);
    }

    // test "device status: color scheme without callback"
    #[test]
    fn device_status_color_scheme_without_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Without color_scheme effect, query should be silently ignored
            s.next_slice(b"\x1B[?996n");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "device status: readonly ignores all"
    #[test]
    fn device_status_readonly_ignores_all() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // All device status queries should be silently ignored without effects
            s.next_slice(b"\x1B[5n");
            s.next_slice(b"\x1B[6n");
            s.next_slice(b"\x1B[?996n");

            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // fn da(_: *Handler) device_attributes.Attributes { return .{}; }
    fn da_default(_: &mut Handler) -> device_attributes::Attributes {
        device_attributes::Attributes::default()
    }

    // test "device attributes: primary DA"
    #[test]
    fn device_attributes_primary_da() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_default);

        {
            s.next_slice(b"\x1B[c");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?62;22c");

        t.deinit(Global);
    }

    // test "device attributes: secondary DA"
    #[test]
    fn device_attributes_secondary_da() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_default);

        {
            s.next_slice(b"\x1B[>c");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[>1;0;0c");

        t.deinit(Global);
    }

    // test "device attributes: tertiary DA"
    #[test]
    fn device_attributes_tertiary_da() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_default);

        {
            s.next_slice(b"\x1B[=c");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP!|00000000\x1b\\");

        t.deinit(Global);
    }

    // test "device attributes: readonly ignores"
    #[test]
    fn device_attributes_readonly_ignores() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // All DA queries should be silently ignored without effects
            s.next_slice(b"\x1B[c");
            s.next_slice(b"\x1B[>c");
            s.next_slice(b"\x1B[=c");

            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // fn da(_: *Handler) device_attributes.Attributes {
    //     return .{
    //         .primary = .{
    //             .conformance_level = .vt420,
    //             .features = &.{ .ansi_color, .clipboard },
    //         },
    //         .secondary = .{
    //             .device_type = .vt420,
    //             .firmware_version = 100,
    //         },
    //     };
    // }
    fn da_custom(_: &mut Handler) -> device_attributes::Attributes {
        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: device_attributes::ConformanceLevel::VT420,
                features: &[
                    device_attributes::Feature::AnsiColor,
                    device_attributes::Feature::Clipboard,
                ],
            },
            secondary: device_attributes::Secondary {
                device_type: device_attributes::DeviceType::VT420,
                firmware_version: 100,
                ..device_attributes::Secondary::default()
            },
            ..device_attributes::Attributes::default()
        }
    }

    // test "device attributes: custom response"
    #[test]
    fn device_attributes_custom_response() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_custom);

        s.next_slice(b"\x1B[c");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?64;22;52c");

        s.next_slice(b"\x1B[>c");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[>41;100;0c");

        drop(s);
        t.deinit(Global);
    }

    // test "kitty graphics APC response"
    #[test]
    fn kitty_graphics_apc_response() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }

        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        // handler.effects.write_pty = &S.writePty;
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Send a kitty graphics transmit command with image id 1
        s.next_slice(b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;////////\x1b\\");

        // Should have written a response back
        // try testing.expectEqualStrings("\x1b_Gi=1;OK\x1b\\", S.written.?);
        drop(s);
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b_Gi=1;OK\x1b\\");

        t.deinit(Global);
    }

    // test "kitty graphics via APC"
    #[test]
    fn kitty_graphics_via_apc() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }

        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Send a kitty graphics transmit command via APC:
            // ESC _ G <payload> ESC \
            // a=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;//////// (1x2 RGB direct)
            s.next_slice(b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;////////\x1b\\");
        }

        // const storage = &t.screens.active.kitty_images;
        // const img = storage.imageById(1).?;
        // try testing.expectEqual(.rgb, img.format);
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_by_id(1).unwrap();
        assert_eq!(kitty::graphics_command::Format::Rgb, img.format);

        t.deinit(Global);
    }
}
