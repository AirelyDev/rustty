//! The big vt-Stream handler: routes terminal.Stream actions into terminal
//! state mutations, termio-mailbox writes, and surface/renderer messages.
//!
//! PORTS: ghostty src/termio/stream_handler.zig (0 `test{}` blocks).
//!
//! PORT STATUS: ported (integration-tier stage 4) EXCEPT the two cross-thread
//! writer slow paths (`surfaceMessageWriter`/`rendererMessageWriter` — the
//! manual renderer-mutex unlock/relock + the opaque core `Timeout` at the
//! AppMailbox seam; the `mailbox::send` deferral precedent); each deferred
//! body is preserved verbatim (DEFERRALS.md, R5: no silent drops).
//!
//! The tmux control-mode arm of `dcsCommand` is LIVE as of the
//! `fix-freetype-tmux` lane: its stated blocker (rustty-vt's dcs
//! `TmuxCommand = ()` stub) is gone — `dcs::Command::Tmux` now carries a real
//! `tmux::ControlNotification` and `tmux::Viewer::next` is ported.

// PARITY: the reference's identifiers are camelCase (`queueRender`,
// `surfaceMessageWriter`, `dcsHook`, ...); preserved verbatim per R1/R2.
#![allow(non_snake_case)]
// PARITY: `setModifyKeyFormat`'s `switch (format)` with a single live arm
// mirrors the reference switch verbatim; rewriting as `if` would distort the
// 1:1 structural mapping (rustty-vt stream.rs precedent).
#![allow(clippy::single_match)]

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const xev = @import("../global.zig").xev;
// const apprt = @import("../apprt.zig");
// const build_config = @import("../build_config.zig");
// const configpkg = @import("../config.zig");
// const internal_os = @import("../os/main.zig");
// const renderer = @import("../renderer.zig");
// const termio = @import("../termio.zig");
// const terminal = @import("../terminal/main.zig");
// const terminfo = @import("../terminfo/main.zig");
// const posix = std.posix;
//
// PORT NOTE: `xev` -> crate::xev_seam (D6); `apprt` -> rustty-apprt-api's
// surface seam (`M: AppMailbox`); `terminal` -> rustty-vt; `terminfo` ->
// rustty-terminfo; `posix` is unreferenced by any live declaration here.
use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api::structs::Clipboard;
use rustty_apprt_api::surface::{
    AppMailbox, DesktopNotification as SurfaceDesktopNotification, Message as SurfaceMessage,
    ReportTitleStyle,
};
use rustty_config::value_types::{ClipboardAccess, OSCColorReportFormat};
use rustty_datastruct::blocking_queue::Timeout;
use rustty_renderer::size::Size;
use rustty_renderer::state::State;
use rustty_renderer::thread as renderer_thread;
use rustty_vt::stream;
use rustty_vt::terminal::Terminal;
use rustty_vt::{ansi, color, csi, device_attributes, device_status, kitty, modes, osc};

use crate::mailbox::Mailbox;
use crate::message::Message;
use crate::termio_impl::DerivedConfig;
use crate::xev_seam as xev;

use core::fmt::Write as _;

// const log = std.log.scoped(.io_handler);
//
// PORT NOTE (DECISION-logging.md §6/§6r4): `LOG_SCOPE` is the `@tagName` of the
// reference's `std.log.scoped(.io_handler)`; restored sites call the `log`
// facade with `target: LOG_SCOPE`. This supersedes the earlier
// "preserved verbatim as comments at each site" note — the Zig comment stays
// (R1) and the live call is added beneath it.
const LOG_SCOPE: &str = "io_handler";

/// PORT NOTE (Zig-format shim, task-14e-1): `{x}` applied to a `[]const u8` in
/// Zig prints the bytes as concatenated lowercase hex with no separator. Rust's
/// `{:x}` has no slice impl, so this wrapper reproduces it for the two
/// `log.debug("... response: {x}", .{final})` sites.
struct HexBytes<'b>(&'b [u8]);

impl core::fmt::Display for HexBytes<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// `pub const Stream = terminal.Stream(StreamHandler);`
//
// PORT NOTE: the reference nests this in StreamHandler; hoisted to a
// module-level alias (Rust cannot nest a generic type alias in a struct's
// namespace). `'a` threads the vt Stream's OSC-parser allocator lifetime
// (rustty-vt stream_terminal precedent).
pub type Stream<'a, M> = stream::Stream<'a, StreamHandler<M>>;

/// True if we have tmux control mode built in.
//
// pub const tmux_enabled = terminal.options.tmux_control_mode;
pub const TMUX_ENABLED: bool = rustty_vt::options::TMUX_CONTROL_MODE;

/// This is used as the handler for the terminal.Stream type. This is
/// stateful and is expected to live for the entire lifetime of the terminal.
/// It is NOT VALID to stop a stream handler, create a new one, and use that
/// unless all of the member fields are copied.
//
// pub const StreamHandler = struct {
//
// PORT NOTE: `M` is the `apprt.surface.Mailbox` seam
// (`rustty_apprt_api::surface::AppMailbox`, implemented by rustty-core later);
// the reference's `*`-pointer fields are raw pointers (Termio/renderer
// state.rs precedent — shared mutable across the self-referential
// Stream/Termio aliasing).
pub struct StreamHandler<M: AppMailbox> {
    // alloc: Allocator,
    pub alloc: Allocator,
    // size: *renderer.Size,
    pub size: *mut Size,
    // terminal: *terminal.Terminal,
    pub terminal: *mut Terminal,

    /// Mailbox for data to the termio thread.
    // termio_mailbox: *termio.Mailbox,
    pub termio_mailbox: *mut Mailbox,

    /// Mailbox for the surface.
    // surface_mailbox: apprt.surface.Mailbox,
    pub surface_mailbox: M,

    /// The shared render state
    // renderer_state: *renderer.State,
    pub renderer_state: *mut State,

    /// The mailbox for notifying the renderer of things.
    // renderer_mailbox: *renderer.Thread.Mailbox,
    pub renderer_mailbox: *mut renderer_thread::Mailbox,

    /// A handle to wake up the renderer. This hints to the renderer that
    /// a repaint should happen.
    // renderer_wakeup: xev.Async,
    pub renderer_wakeup: xev::Async,

    /// The response to use for ENQ requests. The memory is owned by
    /// whoever owns StreamHandler.
    // enquiry_response: []const u8,
    //
    // PORT NOTE: the reference field aliases DerivedConfig's arena; the Rust
    // port owns a copy (`changeConfig` re-clones) — the borrow would be
    // self-referential against Termio's config field (arena-collapse
    // precedent).
    pub enquiry_response: Vec<u8>,

    /// The color reporting format for OSC requests.
    // osc_color_report_format: configpkg.Config.OSCColorReportFormat,
    pub osc_color_report_format: OSCColorReportFormat,

    /// The clipboard write access configuration.
    // clipboard_write: configpkg.ClipboardAccess,
    pub clipboard_write: ClipboardAccess,

    /// Maximum total decoded bytes per Kitty clipboard protocol
    /// (OSC 5522) write transaction; exceeding it aborts with EFBIG.
    // clipboard_write_limit: usize,
    pub clipboard_write_limit: usize,

    //---------------------------------------------------------------
    // Internal state
    /// The APC command handler maintains the APC state. APC is like
    /// CSI or OSC, but it is a private escape sequence that is used
    /// to send commands to the terminal emulator. This is used by
    /// the kitty graphics protocol.
    // apc: terminal.apc.Handler = .{},
    pub apc: rustty_vt::apc::Handler,

    /// The DCS handler maintains DCS state. DCS is like CSI or OSC,
    /// but requires more stateful parsing. This is used by functionality
    /// such as XTGETTCAP.
    // dcs: terminal.dcs.Handler = .{},
    pub dcs: rustty_vt::dcs::Handler,

    /// The tmux control mode viewer state.
    // tmux_viewer: if (tmux_enabled) ?*terminal.tmux.Viewer else void = if (tmux_enabled) null else {},
    //
    // PORT NOTE: the comptime-conditional field type collapses to
    // `Option<Box<Viewer>>` (the `void` case is the always-`None` state; Zig's
    // zero-size `void` field has no Rust analogue in a runtime-conditional
    // struct).
    pub tmux_viewer: Option<Box<rustty_vt::tmux::viewer::Viewer>>,

    /// Session password grants for the Kitty clipboard protocol.
    /// Requests carrying a granted password skip the permission prompt.
    // kitty_clipboard_grants: terminal.kitty.clipboard.Grants = .{},
    pub kitty_clipboard_grants: rustty_vt::kitty::clipboard::Grants,

    /// The in-flight Kitty clipboard protocol (OSC 5522) write
    /// transaction, if any.
    // kitty_clipboard_write: ?*terminal.kitty.clipboard.WriteState = null,
    // PORT NOTE: `?*` (manual create/destroy + errdefer) -> `Option<Box<_>>`
    // (assessment E hazard #2); the abort/finish paths are `take()` + drop.
    pub kitty_clipboard_write: Option<Box<rustty_vt::kitty::clipboard::WriteState>>,

    /// This is set to true when a message was written to the termio
    /// mailbox. This can be used by callers to determine if they need
    /// to wake up the termio thread.
    // termio_messaged: bool = false,
    pub termio_messaged: bool,

    /// This is set to true when we've seen a title escape sequence. We use
    /// this to determine if we need to default the window title.
    // seen_title: bool = false,
    pub seen_title: bool,
}

impl<M: AppMailbox> StreamHandler<M> {
    // pub fn deinit(self: *StreamHandler) void {
    pub fn deinit(&mut self) {
        // self.apc.deinit();
        self.apc.deinit();
        // self.dcs.deinit();
        self.dcs.deinit();
        // self.kittyClipboardWriteAbort();
        self.kittyClipboardWriteAbort();
        // self.kitty_clipboard_grants.deinit(self.alloc);
        // (drop of the replaced Grants frees the entries.)
        self.kitty_clipboard_grants = rustty_vt::kitty::clipboard::Grants::default();
        // if (comptime tmux_enabled) tmux: {
        //     const viewer = self.tmux_viewer orelse break :tmux;
        //     viewer.deinit();
        //     self.alloc.destroy(viewer);
        //     self.tmux_viewer = null;
        // }
        if TMUX_ENABLED {
            if let Some(mut viewer) = self.tmux_viewer.take() {
                viewer.deinit();
                // self.alloc.destroy(viewer); — Box drop
                // self.tmux_viewer = null; — the take() above
            }
        }
    }

    /// This queues a render operation with the renderer thread. The render
    /// isn't guaranteed to happen immediately but it will happen as soon as
    /// practical.
    // pub inline fn queueRender(self: *StreamHandler) !void {
    //     try self.renderer_wakeup.notify();
    // }
    #[inline]
    pub fn queueRender(&mut self) -> Result<(), xev::NotifyError> {
        self.renderer_wakeup.notify()
    }

    /// Change the configuration for this handler.
    // pub fn changeConfig(self: *StreamHandler, config: *termio.DerivedConfig) void {
    pub fn changeConfig(&mut self, config: &mut DerivedConfig) {
        self.osc_color_report_format = config.osc_color_report_format;
        self.clipboard_write = config.clipboard_write;
        // self.clipboard_write_limit = config.clipboard_write_limit;
        self.clipboard_write_limit = config.clipboard_write_limit;
        // self.enquiry_response = config.enquiry_response;
        // (PORT NOTE: owned copy — see the field's PORT NOTE.)
        self.enquiry_response = config.enquiry_response.clone();
        // self.terminal.setDefaultCursorStyle(config.cursor_style);
        // self.terminal.setDefaultCursorBlink(config.cursor_blink);
        //
        // PORT NOTE (oracle c594031d5): the handler no longer owns the cursor
        // defaults; `Terminal` does, and its setters apply them immediately
        // when the cursor still follows the defaults.
        let cursor_style = config.cursor_style;
        let cursor_blink = config.cursor_blink;
        self.term().set_default_cursor_style(cursor_style);
        self.term().set_default_cursor_blink(cursor_blink);

        // The config could have changed any of our colors so update mode 2031
        // self.messageWriter(.{ .color_scheme_report = .{ .force = false } });
        self.messageWriter(Message::ColorSchemeReport { force: false });
    }

    // inline fn surfaceMessageWriter(
    //     self: *StreamHandler,
    //     msg: apprt.surface.Message,
    // ) void {
    //
    // PORT NOTE (core C-T un-defer): the opaque core `Timeout` is now concrete
    // (`Timeout::Instant`/`Forever`) and the slow path unlocks/relocks the
    // caller-held renderer mutex via the guard-less `rustty_zigstd::thread::
    // Mutex` (the C0 model). The reference re-pushes its by-value `msg` copy on
    // a failed instant push; Rust moves, so `push_or_return` hands `msg` back
    // (the `AppMailbox` retain-on-failure primitive). `SurfaceMessage<'static>`
    // because a storing push cannot take a borrow (mailbox `Queue` precedent).
    #[inline]
    fn surfaceMessageWriter(&mut self, msg: SurfaceMessage<'static>) {
        // See messageWriter which has similar logic and explains why
        // we may have to do this.
        // if (self.surface_mailbox.push(msg, .{ .instant = {} }) == 0) {
        if let Err(msg) = self.surface_mailbox.push_or_return(msg, Timeout::Instant) {
            // self.renderer_state.mutex.unlock();
            //
            // SAFETY: `renderer_state` points at the live shared render State
            // for the handler's lifetime; its `mutex` is the guard-less lock
            // held by the caller. The callee unlock/relock inversion is exactly
            // what the guard-less design exists for (zigstd thread::Mutex doc).
            unsafe { (*(*self.renderer_state).mutex).unlock() };
            // defer self.renderer_state.mutex.lock();
            // (guard-less zigstd Mutex — the `defer` relock is the explicit
            // re-lock after the blocking push below, the sole exit.)
            // _ = self.surface_mailbox.push(msg, .{ .forever = {} });
            let _ = self.surface_mailbox.push(msg, Timeout::Forever);
            // SAFETY: see the unlock above — we relock the caller's lock.
            unsafe { (*(*self.renderer_state).mutex).lock() };
        }
    }

    // inline fn messageWriter(self: *StreamHandler, msg: termio.Message) void {
    #[inline]
    fn messageWriter(&mut self, msg: Message<'static>) {
        // self.termio_mailbox.send(msg, self.renderer_state.mutex);
        let mutex = unsafe { (*self.renderer_state).mutex };
        unsafe { &mut *self.termio_mailbox }.send(msg, Some(mutex));
        // self.termio_messaged = true;
        self.termio_messaged = true;
    }

    /// Send a renderer message and unlock the renderer state mutex
    /// if necessary to ensure we don't deadlock.
    ///
    /// This assumes the renderer state mutex is locked.
    //
    // inline fn rendererMessageWriter(
    //     self: *StreamHandler,
    //     msg: renderer.Message,
    // ) void {
    //
    // PORT NOTE (core C-T un-defer): the slow path unlocks/relocks the caller-
    // held renderer mutex via the guard-less C0 `rustty_zigstd::thread::Mutex`,
    // and re-pushes its by-value `msg` on a failed instant push — Rust moves,
    // so `push_or_return` hands `msg` back. `#[allow(dead_code)]` retained: the
    // reference declaration is itself uncalled in this file (every VT action
    // routes through `messageWriter`/`surfaceMessageWriter`), so no caller is
    // fabricated — the faithful body is preserved and compiled.
    #[inline]
    #[allow(dead_code)]
    fn rendererMessageWriter(&mut self, msg: rustty_renderer::message::Message) {
        // See termio.Mailbox.send for more details on how this works.

        // Try instant first. If it works then we can return.
        // if (self.renderer_mailbox.push(msg, .{ .instant = {} }) > 0) {
        //     return;
        // }
        //
        // SAFETY: `renderer_mailbox` points at the live shared renderer thread
        // mailbox (`BlockingQueue<Message, 64>`) for the handler's lifetime.
        let msg = match unsafe { &*self.renderer_mailbox }.push_or_return(msg, Timeout::Instant) {
            Ok(_) => return,
            Err(msg) => msg,
        };

        // Instant would have blocked. Release the renderer mutex,
        // wake up the renderer to allow it to process the message,
        // and then try again.
        // self.renderer_state.mutex.unlock();
        //
        // SAFETY: the guard-less caller-held lock (see surfaceMessageWriter).
        unsafe { (*(*self.renderer_state).mutex).unlock() };
        // defer self.renderer_state.mutex.lock();
        // (guard-less zigstd Mutex — the `defer` relock is the explicit re-lock
        // after the blocking push below, the sole exit.)
        // self.renderer_wakeup.notify() catch |err| {
        if let Err(err) = self.renderer_wakeup.notify() {
            // This is an EXTREMELY unlikely case. We still don't return
            // and attempt to send the message because its most likely
            // that everything is fine, but log in case a freeze happens.
            // log.warn(
            //     "failed to notify renderer, may deadlock err={}",
            //     .{err},
            // );
            log::warn!(target: LOG_SCOPE, "failed to notify renderer, may deadlock err={err:?}");
        }
        // _ = self.renderer_mailbox.push(msg, .{ .forever = {} });
        let _ = unsafe { &*self.renderer_mailbox }.push(msg, Timeout::Forever);
        // SAFETY: see the unlock above — we relock the caller's lock.
        unsafe { (*(*self.renderer_state).mutex).lock() };
    }

    // pub fn vt(
    //     self: *StreamHandler,
    //     comptime action: Stream.Action.Tag,
    //     value: Stream.Action.Value(action),
    // ) void {
    //     self.vtFallible(action, value) catch |err| {
    //         log.warn("error handling VT action action={} err={}", .{ action, err });
    //     };
    // }
    //
    // PORT NOTE (stream_terminal precedent): the reference's `vt`/`vtFallible`
    // comptime dispatcher (a single `switch (action)`) is distributed across
    // the `impl stream::Handler` methods below — the Rust Stream calls the
    // per-action trait methods directly, so each `vtFallible` arm becomes the
    // corresponding trait-method body, funneled through this helper for the
    // `catch |err| log.warn(...)` behavior.
    //
    // PORT NOTE (log text): the reference's `action` is the COMPTIME
    // `Stream.Action.Tag` value and Zig's `{}` on an exhaustive enum prints
    // `<typename>.<tag>`. The port's dispatch is one trait method PER action, so
    // the tag is not a value here at all; each caller passes its own
    // `Action.Tag` FIELD NAME as a literal and the site renders that bare name.
    // The information is the same, the type-name prefix is not reproduced —
    // NEW-FINDINGS N11's class (Rust standing in for Zig's builtin formatter).
    #[inline]
    fn vt(&mut self, action: &'static str, r: Result<(), crate::Error>) {
        if let Err(err) = r {
            // log.warn("error handling VT action action={} err={}", .{ action, err });
            log::warn!(target: LOG_SCOPE, "error handling VT action action={action} err={err:?}");
        }
    }

    // PORT NOTE: convenience accessor for the `*mut Terminal` deref
    // (stream_terminal precedent). Each call re-derefs the raw pointer; no
    // `&mut Terminal` is held across re-reads (the soundness rule of memory
    // rust-mut-args-strongly-protected); callers must not nest two live
    // `term()` borrows.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    fn term(&self) -> &mut Terminal {
        // SAFETY: `self.terminal` aliases the Termio-owned Terminal exactly as
        // the reference's `terminal: *terminal.Terminal`; the deref is
        // short-lived and never overlaps another live `&mut Terminal`.
        unsafe { &mut *self.terminal }
    }

    // pub inline fn dcsHook(self: *StreamHandler, dcs: terminal.DCS) !void {
    #[inline]
    pub fn dcsHook(&mut self, dcs: rustty_vt::parser::DCS) -> Result<(), crate::Error> {
        // var cmd = self.dcs.hook(self.alloc, dcs) orelse return;
        // (PORT NOTE: the trait hands the parser's owned DCS; the dcs Handler takes
        // the borrowed view — the same coercion Zig's `terminal.DCS` does.)
        let mut cmd = match self.dcs.hook(
            self.alloc,
            rustty_vt::dcs::DCS {
                intermediates: &dcs.intermediates,
                params: &dcs.params,
                final_: dcs.final_,
            },
        ) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.dcsCommand(&mut cmd);
        cmd.deinit();
        // try self.dcsCommand(&cmd);
        result
    }

    // pub inline fn dcsPut(self: *StreamHandler, byte: u8) !void {
    #[inline]
    pub fn dcsPut(&mut self, byte: u8) -> Result<(), crate::Error> {
        // var cmd = self.dcs.put(byte) orelse return;
        let mut cmd = match self.dcs.put(byte) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.dcsCommand(&mut cmd);
        cmd.deinit();
        // try self.dcsCommand(&cmd);
        result
    }

    // pub inline fn dcsUnhook(self: *StreamHandler) !void {
    #[inline]
    pub fn dcsUnhook(&mut self) -> Result<(), crate::Error> {
        // var cmd = self.dcs.unhook() orelse return;
        let mut cmd = match self.dcs.unhook() {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.dcsCommand(&mut cmd);
        cmd.deinit();
        // try self.dcsCommand(&cmd);
        result
    }

    // fn dcsCommand(self: *StreamHandler, cmd: *terminal.dcs.Command) !void {
    fn dcsCommand(&mut self, cmd: &mut rustty_vt::dcs::Command) -> Result<(), crate::Error> {
        // log.warn("DCS command: {}", .{cmd});
        match cmd {
            // .tmux => |tmux| tmux: {
            //
            // PORT NOTE (Rust-only glue, three seams — the body below is
            // otherwise 1:1 with the reference):
            //
            //   1. `tmux: { ... break :tmux; }` is a Rust labeled block
            //      (`'tmux: { ... break 'tmux; }`) — the direct analogue.
            //   2. The reference binds `|tmux|` as a *copy* of the union
            //      payload, whose `[]const u8` members alias parser-owned
            //      memory; the Rust `ControlNotification` owns its `Vec<u8>`
            //      payloads, so the value is MOVED out of `*cmd` with a
            //      `mem::replace` tombstone. That is safe because
            //      `dcs::Command::deinit`'s `Tmux` arm is a no-op (dcs.rs) —
            //      the caller's `cmd.deinit()` never reads the payload again —
            //      and because the move happens only on the fallthrough arm,
            //      after `enter`/`exit` have been dispatched and broken out.
            //   3. `viewer.next()` returns a slice borrowed from the viewer
            //      while the loop body needs `&mut self` (`messageWriter`), so
            //      the `Box<Viewer>` is `take()`n out of `self.tmux_viewer` for
            //      the duration of the loop and restored after it. Nothing the
            //      loop calls reads `tmux_viewer`.
            //
            // The reference's `viewer.* = try .init(global.io(), self.alloc)`
            // passes the Zig 0.16 `std.Io` instance; rustty's
            // `tmux::Viewer::init` takes only the allocator (the vt port's
            // signature — no `std.Io` analogue), so only `alloc` is threaded.
            rustty_vt::dcs::Command::Tmux(tmux) => 'tmux: {
                // If tmux control mode is disabled at the build level,
                // then this whole block shouldn't be analyzed.
                // if (comptime !tmux_enabled) break :tmux;
                if !TMUX_ENABLED {
                    break 'tmux;
                }
                // log.info("tmux control mode event cmd={f}", .{tmux});
                log::info!(target: LOG_SCOPE, "tmux control mode event cmd={tmux}");

                // switch (tmux) {
                match tmux {
                    // .enter => {
                    rustty_vt::tmux::ControlNotification::Enter => {
                        // Setup our viewer state
                        // assert(self.tmux_viewer == null);
                        assert!(self.tmux_viewer.is_none());
                        // const viewer = try self.alloc.create(terminal.tmux.Viewer);
                        // errdefer self.alloc.destroy(viewer);
                        // viewer.* = try .init(global.io(), self.alloc);
                        // errdefer viewer.deinit();
                        let viewer =
                            rustty_vt::tmux::Viewer::init(self.alloc).map_err(|_| crate::Error)?;
                        // self.tmux_viewer = viewer;
                        self.tmux_viewer = Some(Box::new(viewer));
                        // break :tmux;
                        break 'tmux;
                    }

                    // .exit => {
                    rustty_vt::tmux::ControlNotification::Exit => {
                        // Free our viewer state if we have one
                        // if (self.tmux_viewer) |viewer| {
                        //     viewer.deinit();
                        //     self.alloc.destroy(viewer);
                        //     self.tmux_viewer = null;
                        // }
                        if let Some(mut viewer) = self.tmux_viewer.take() {
                            viewer.deinit();
                            // self.alloc.destroy(viewer); — Box drop
                            // self.tmux_viewer = null; — the take() above
                        }

                        // And always break since we assert below
                        // that we're not handling an exit command.
                        // break :tmux;
                        break 'tmux;
                    }

                    // else => {},
                    _ => {}
                }

                // assert(tmux != .enter);
                // assert(tmux != .exit);
                //
                // Both are structurally guaranteed: the two arms above always
                // `break 'tmux`, so only the `else` fallthrough reaches here.
                debug_assert!(!matches!(
                    tmux,
                    rustty_vt::tmux::ControlNotification::Enter
                        | rustty_vt::tmux::ControlNotification::Exit
                ));

                // const viewer = self.tmux_viewer orelse {
                let Some(mut viewer) = self.tmux_viewer.take() else {
                    // This can only really happen if we failed to
                    // initialize the viewer on enter.
                    // log.info(
                    //     "received tmux control mode command without viewer: {f}",
                    //     .{tmux},
                    // );
                    log::info!(
                        target: LOG_SCOPE,
                        "received tmux control mode command without viewer: {tmux}"
                    );
                    //
                    // break :tmux;
                    break 'tmux;
                };

                // (seam 2 — see the PORT NOTE above)
                let notification =
                    core::mem::replace(tmux, rustty_vt::tmux::ControlNotification::Exit);

                // for (viewer.next(.{ .tmux = tmux })) |action| {
                for action in viewer.next(rustty_vt::tmux::viewer::Input::Tmux(notification)) {
                    // log.info("tmux viewer action={f}", .{action});
                    log::info!(target: LOG_SCOPE, "tmux viewer action={action}");
                    // switch (action) {
                    match action {
                        // .exit => {
                        rustty_vt::tmux::viewer::Action::Exit => {
                            // We ignore this because we will fully exit when
                            // our DCS connection ends. We may want to handle
                            // this in the future to notify our GUI we're
                            // disconnected though.
                        }

                        // .command => |command| {
                        rustty_vt::tmux::viewer::Action::Command(command) => {
                            // assert(command.len > 0);
                            assert!(!command.is_empty());
                            // assert(command[command.len - 1] == '\n');
                            assert!(command[command.len() - 1] == b'\n');
                            // self.messageWriter(try termio.Message.writeReq(
                            //     self.alloc,
                            //     command,
                            // ));
                            self.messageWriter(Message::writeReq(command));
                        }

                        // .windows => {
                        rustty_vt::tmux::viewer::Action::Windows(_) => {
                            // TODO
                        }
                    }
                }
                self.tmux_viewer = Some(viewer);
            }

            // .xtgettcap => |*gettcap| {
            rustty_vt::dcs::Command::Xtgettcap(gettcap) => {
                // const map = comptime terminfo.ghostty.xtgettcapMap();
                let map = xtgettcapMap();
                // while (gettcap.next()) |key| {
                while let Some(key) = gettcap.next() {
                    // const response = map.get(key) orelse continue;
                    let response: &'static [u8] = match map.get(key) {
                        Some(response) => response,
                        None => continue,
                    };
                    // self.messageWriter(.{ .write_stable = response });
                    self.messageWriter(Message::WriteStable(response));
                }
            }

            // .decrqss => |decrqss| {
            //     var response: [terminal.dcs.Command.DECRQSS.max_response_bytes]u8 = undefined;
            //     const encoded = try decrqss.encode(self.terminal, &response);
            //     const msg = try termio.Message.writeReq(self.alloc, encoded);
            //     self.messageWriter(msg);
            // },
            //
            // PORT NOTE (oracle 39ae85f04, area E11): the 60-line inline
            // DECRQSS builder that used to live here — the Sgr/Decscusr/
            // Decstbm/Decslrm arms with hand-rolled prefix-offset arithmetic —
            // is DELETED. `dcs::Decrqss::encode` is now the single owner so the
            // full termio path and the libghostty-vt terminal stream emit
            // byte-identical replies.
            rustty_vt::dcs::Command::Decrqss(decrqss) => {
                let mut response = [0u8; rustty_vt::dcs::Decrqss::MAX_RESPONSE_BYTES];
                let encoded_len = {
                    let t: *mut Terminal = self.terminal;
                    // SAFETY: `self.terminal` is the live Termio-owned terminal
                    // (see `term()`); the deref is short-lived.
                    let encoded = decrqss
                        .encode(unsafe { &mut *t }, &mut response)
                        .map_err(|_| crate::Error)?;
                    encoded.len()
                };
                let msg = Message::writeReq(&response[0..encoded_len]);
                self.messageWriter(msg);
            }
        }
        Ok(())
    }

    // pub fn apcEnd(self: *StreamHandler) !void {
    pub fn apcEnd(&mut self) -> Result<(), crate::Error> {
        // var result = self.apc.end() orelse return;
        let mut result = match self.apc.end(self.alloc) {
            Some(result) => result,
            None => return Ok(()),
        };
        // defer result.deinit(self.alloc);
        let r = self.apcCommand(&mut result);
        result.deinit(self.alloc);
        r
    }

    // (PORT NOTE, Rust-only glue: the `switch (result)` body of apcEnd, factored
    // so the `defer result.deinit(self.alloc)` runs on every path.)
    fn apcCommand(&mut self, cmd: &mut rustty_vt::apc::Command) -> Result<(), crate::Error> {
        // log.warn("APC command: {}", .{result});
        match cmd {
            // .unknown => return,
            rustty_vt::apc::Command::Unknown(_) => return Ok(()),
            // .kitty => |*kitty_cmd| {
            rustty_vt::apc::Command::Kitty(kitty_cmd) => {
                // if (self.terminal.kittyGraphics(self.alloc, kitty_cmd)) |resp| {
                if let Some(resp) = self.term().kitty_graphics(self.alloc, kitty_cmd) {
                    // var buf: [1024]u8 = undefined;
                    // var writer: std.Io.Writer = .fixed(&buf);
                    let mut buf = [0u8; 1024];
                    let mut cursor = std::io::Cursor::new(&mut buf[..]);
                    // try resp.encode(&writer);
                    resp.encode(&mut cursor).map_err(|_| crate::Error)?;
                    // const final = writer.buffered();
                    let len = cursor.position() as usize;
                    let final_ = &buf[..len];
                    // if (final.len > 2) {
                    if final_.len() > 2 {
                        // log.debug("kitty graphics response: {x}", .{final});
                        log::debug!(
                            target: LOG_SCOPE,
                            "kitty graphics response: {}",
                            HexBytes(final_)
                        );
                        // self.messageWriter(try termio.Message.writeReq(self.alloc, final));
                        let msg = Message::writeReq(final_);
                        self.messageWriter(msg);
                    }
                }
            }

            // .glyph => |*glyph_req| {
            rustty_vt::apc::Command::Glyph(glyph_req) => {
                // const resp = self.terminal.glyphProtocol(self.alloc, glyph_req);
                let resp = self.term().glyph_protocol(self.alloc, glyph_req);
                // switch (glyph_req.*) {
                //     .register, .clear => try self.queueRender(),
                //     .support, .query => {},
                // }
                match glyph_req {
                    rustty_vt::apc::glyph::Request::Register(_)
                    | rustty_vt::apc::glyph::Request::Clear(_) => {
                        self.queueRender().map_err(|_| crate::Error)?;
                    }
                    rustty_vt::apc::glyph::Request::Support
                    | rustty_vt::apc::glyph::Request::Query(_) => {}
                }

                // if (resp) |r| {
                if let Some(r) = resp {
                    // var buf: [terminal.apc.glyph.Response.max_wire_bytes]u8 = undefined;
                    // var writer: std.Io.Writer = .fixed(&buf);
                    let mut buf = String::new();
                    // try r.formatWire(&writer);
                    r.format_wire(&mut buf).map_err(|_| crate::Error)?;
                    // const final = writer.buffered();
                    let final_ = buf.as_bytes();
                    // log.debug("glyph protocol response: {x}", .{final});
                    log::debug!(
                        target: LOG_SCOPE,
                        "glyph protocol response: {}",
                        HexBytes(final_)
                    );
                    // self.messageWriter(try termio.Message.writeReq(self.alloc, final));
                    let msg = Message::writeReq(final_);
                    self.messageWriter(msg);
                }
            }
        }
        Ok(())
    }

    // inline fn bell(self: *StreamHandler) void {
    #[inline]
    fn bell(&mut self) {
        // self.surfaceMessageWriter(.ring_bell);
        self.surfaceMessageWriter(SurfaceMessage::RingBell);
    }

    // inline fn horizontalTab(self: *StreamHandler, count: u16) void {
    #[inline]
    fn horizontalTab(&mut self, count: u16) {
        // for (0..count) |_| {
        for _ in 0..count {
            // const x = self.terminal.screens.active.cursor.x;
            let x = self.term().active_mut().cursor.x;
            // self.terminal.horizontalTab();
            self.term().horizontal_tab();
            // if (x == self.terminal.screens.active.cursor.x) break;
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn horizontalTabBack(self: *StreamHandler, count: u16) void {
    #[inline]
    fn horizontalTabBack(&mut self, count: u16) {
        // for (0..count) |_| {
        for _ in 0..count {
            // const x = self.terminal.screens.active.cursor.x;
            let x = self.term().active_mut().cursor.x;
            // self.terminal.horizontalTabBack();
            self.term().horizontal_tab_back();
            // if (x == self.terminal.screens.active.cursor.x) break;
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn linefeed(self: *StreamHandler) !void {
    #[inline]
    fn linefeed(&mut self) -> Result<(), crate::Error> {
        // Small optimization: call index instead of linefeed because they're
        // identical and this avoids one layer of function call overhead.
        // try self.terminal.index();
        self.term().index().map_err(|_| crate::Error)
    }

    // pub inline fn reverseIndex(self: *StreamHandler) !void {
    #[inline]
    pub fn reverseIndex(&mut self) -> Result<(), crate::Error> {
        // self.terminal.reverseIndex();
        self.term().reverse_index();
        Ok(())
    }

    // pub inline fn index(self: *StreamHandler) !void {
    #[inline]
    pub fn index(&mut self) -> Result<(), crate::Error> {
        // try self.terminal.index();
        self.term().index().map_err(|_| crate::Error)
    }

    // pub inline fn nextLine(self: *StreamHandler) !void {
    #[inline]
    pub fn nextLine(&mut self) -> Result<(), crate::Error> {
        // try self.terminal.index();
        self.term().index().map_err(|_| crate::Error)?;
        // self.terminal.carriageReturn();
        self.term().carriage_return();
        Ok(())
    }

    // pub fn setModifyKeyFormat(self: *StreamHandler, format: terminal.ModifyKeyFormat) !void {
    pub fn setModifyKeyFormat(
        &mut self,
        format: ansi::ModifyKeyFormat,
    ) -> Result<(), crate::Error> {
        // self.terminal.flags.modify_other_keys_2 = false;
        self.term().flags.modify_other_keys_2 = false;
        // switch (format) {
        //     .other_keys_numeric => self.terminal.flags.modify_other_keys_2 = true,
        //     else => {},
        // }
        match format {
            ansi::ModifyKeyFormat::OtherKeysNumeric => {
                self.term().flags.modify_other_keys_2 = true;
            }
            _ => {}
        }
        Ok(())
    }

    // fn requestMode(self: *StreamHandler, mode: terminal.Mode) !void {
    fn requestMode(&mut self, mode: modes::Mode) -> Result<(), crate::Error> {
        // self.sendModeReport(self.terminal.modes.getReport(.fromMode(mode)));
        let report = self
            .term()
            .modes
            .get_report(modes::ModeTag::from_mode(mode));
        self.sendModeReport(report);
        Ok(())
    }

    // fn requestModeUnknown(self: *StreamHandler, mode_raw: u16, ansi: bool) !void {
    fn requestModeUnknown(&mut self, mode_raw: u16, ansi_: bool) -> Result<(), crate::Error> {
        // self.sendModeReport(self.terminal.modes.getReport(.{ .value = @truncate(mode_raw), .ansi = ansi }));
        let report = self.term().modes.get_report(modes::ModeTag {
            value: mode_raw & 0x7FFF, // @truncate to the u15 tag value
            ansi: ansi_,
        });
        self.sendModeReport(report);
        Ok(())
    }

    // fn sendModeReport(self: *StreamHandler, report: terminal.modes.Report) void {
    fn sendModeReport(&mut self, report: modes::Report) {
        // var data: termio.Message.WriteReq.Small.Array = undefined;
        // var writer: std.Io.Writer = .fixed(&data);
        let mut data = [0u8; 38];
        let mut writer = FixedFmt {
            buf: &mut data,
            pos: 0,
        };
        // report.encode(&writer) catch |err| {
        //     log.err("error encoding mode report err={}", .{err});
        //     return;
        // };
        if let Err(err) = report.encode(&mut writer) {
            // log.err("error encoding mode report err={}", .{err});
            log::error!(target: LOG_SCOPE, "error encoding mode report err={err:?}");
            return;
        }
        let len = writer.pos;
        // self.messageWriter(.{ .write_small = .{
        //     .data = data,
        //     .len = @intCast(writer.buffered().len),
        // } });
        self.messageWriter(Message::WriteSmall(
            rustty_datastruct::message_data::Small { data, len },
        ));
    }

    // pub fn setMode(self: *StreamHandler, mode: terminal.Mode, enabled: bool) !void {
    pub fn setMode(&mut self, mode: modes::Mode, enabled: bool) -> Result<(), crate::Error> {
        // Note: this function doesn't need to grab the render state or
        // terminal locks because it is only called from process() which
        // grabs the lock.

        // If we are setting cursor blinking, we ignore it if we have
        // a default cursor blink setting set. This is a really weird
        // behavior so this comment will go deep into trying to explain it.
        //
        // There are two ways to set cursor blinks: DECSCUSR (CSI _ q)
        // and DEC mode 12. DECSCUSR is the modern approach and has a
        // way to revert to the "default" (as defined by the terminal)
        // cursor style and blink by doing "CSI 0 q". DEC mode 12 controls
        // blinking and is either on or off and has no way to set a
        // default. DEC mode 12 is also the more antiquated approach.
        //
        // The problem is that if the user specifies a desired default
        // cursor blink with `cursor-style-blink`, the moment a running
        // program uses DEC mode 12, the cursor blink can never be reset
        // to the default without an explicit DECSCUSR. But if a program
        // is using mode 12, it is by definition not using DECSCUSR.
        // This makes for somewhat annoying interactions where a poorly
        // (or legacy) behaved program will stop blinking, and it simply
        // never restarts.
        //
        // To get around this, we have a special case where if the user
        // specifies some explicit default cursor blink desire, we ignore
        // DEC mode 12. We allow DECSCUSR to still set the cursor blink
        // because programs using DECSCUSR usually are well behaved and
        // reset the cursor blink to the default when they exit.
        //
        // To be extra safe, users can also add a manual `CSI 0 q` to
        // their shell config when they render prompts to ensure the
        // cursor is exactly as they request.
        // if (mode == .cursor_blinking and
        //     self.terminal.cursor.default_blink != null)
        // {
        //     return;
        // }
        //
        // PORT NOTE (oracle c594031d5): this reads the field that MOVED onto
        // `Terminal`. Forgetting to re-point it here silently breaks the
        // "shell set an explicit blink, ignore mode 12 changes" rule; guarded
        // by `stream_handler_mode_12_ignored_when_default_blink_is_explicit`.
        if mode == modes::Mode::CursorBlinking && self.term().cursor.default_blink.is_some() {
            return Ok(());
        }

        // We first always set the raw mode on our mode state.
        // self.terminal.modes.set(mode, enabled);
        self.term().modes.set(mode, enabled);

        // And then some modes require additional processing.
        // switch (mode) {
        match mode {
            // Just noting here that autorepeat has no effect on
            // the terminal. xterm ignores this mode and so do we.
            // We know about just so that we don't log that it is
            // an unknown mode.
            // .autorepeat => {},
            modes::Mode::Autorepeat => {}

            // Schedule a render since we changed colors
            // .reverse_colors => self.terminal.flags.dirty.reverse_colors = true,
            modes::Mode::ReverseColors => {
                self.term().flags.dirty.reverse_colors = true;
            }

            // Origin resets cursor pos. This is called whether or not
            // we're enabling or disabling origin mode and whether or
            // not the value changed.
            // .origin => self.terminal.setCursorPos(1, 1),
            modes::Mode::Origin => self.term().set_cursor_pos(1, 1),

            // .enable_left_and_right_margin => if (!enabled) {
            modes::Mode::EnableLeftAndRightMargin => {
                if !enabled {
                    // When we disable left/right margin mode we need to
                    // reset the left/right margins.
                    // self.terminal.scrolling_region.left = 0;
                    self.term().scrolling_region.left = 0;
                    // self.terminal.scrolling_region.right = self.terminal.cols - 1;
                    let right = self.term().cols - 1;
                    self.term().scrolling_region.right = right;
                }
            }

            // .alt_screen_legacy => {
            //     try self.terminal.switchScreenMode(.@"47", enabled);
            // },
            modes::Mode::AltScreenLegacy => {
                self.term()
                    .switch_screen_mode(rustty_vt::terminal::SwitchScreenMode::M47, enabled)
                    .map_err(|_| crate::Error)?;
            }

            // .alt_screen => {
            //     try self.terminal.switchScreenMode(.@"1047", enabled);
            // },
            modes::Mode::AltScreen => {
                self.term()
                    .switch_screen_mode(rustty_vt::terminal::SwitchScreenMode::M1047, enabled)
                    .map_err(|_| crate::Error)?;
            }

            // .alt_screen_save_cursor_clear_enter => {
            //     try self.terminal.switchScreenMode(.@"1049", enabled);
            // },
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.term()
                    .switch_screen_mode(rustty_vt::terminal::SwitchScreenMode::M1049, enabled)
                    .map_err(|_| crate::Error)?;
            }

            // Mode 1048 is xterm's conditional save cursor depending
            // on if alt screen is enabled or not (at the terminal emulator
            // level). Alt screen is always enabled for us so this just
            // does a save/restore cursor.
            // .save_cursor => {
            modes::Mode::SaveCursor => {
                if enabled {
                    self.term().save_cursor();
                } else {
                    self.term().restore_cursor();
                }
            }

            // Force resize back to the window size
            // .enable_mode_3 => {
            modes::Mode::EnableMode3 => {
                // const grid_size = self.size.grid();
                let grid_size = unsafe { *self.size }.grid();
                // self.terminal.resize(
                //     self.alloc,
                //     .{
                //         .cols = grid_size.columns,
                //         .rows = grid_size.rows,
                //     },
                // ) catch |err| {
                //     log.err("error updating terminal size: {}", .{err});
                // };
                //
                // PORT NOTE (oracle 89b103dd5): no cell size here — this path
                // only restores the grid dimensions, so the terminal's existing
                // pixel geometry is left alone.
                let alloc = self.alloc;
                if let Err(err) = self.term().resize(
                    alloc,
                    rustty_vt::terminal::Resize {
                        cols: grid_size.columns,
                        rows: grid_size.rows,
                        cell_size_px: None,
                    },
                ) {
                    // log.err("error updating terminal size: {}", .{err});
                    log::error!(target: LOG_SCOPE, "error updating terminal size: {err:?}");
                }
            }

            // .@"132_column" => try self.terminal.deccolm(
            //     self.alloc,
            //     if (enabled) .@"132_cols" else .@"80_cols",
            // ),
            modes::Mode::Column132 => {
                let alloc = self.alloc;
                self.term()
                    .deccolm(
                        alloc,
                        if enabled {
                            rustty_vt::terminal::DeccolmMode::Cols132
                        } else {
                            rustty_vt::terminal::DeccolmMode::Cols80
                        },
                    )
                    .map_err(|_| crate::Error)?;
            }

            // We need to start a timer to prevent the emulator being hung
            // forever.
            // .synchronized_output => {
            //     if (enabled) self.messageWriter(.{ .start_synchronized_output = {} });
            // },
            modes::Mode::SynchronizedOutput => {
                if enabled {
                    self.messageWriter(Message::StartSynchronizedOutput);
                }
            }

            // .linefeed => {
            //     self.messageWriter(.{ .linefeed_mode = enabled });
            // },
            modes::Mode::Linefeed => {
                self.messageWriter(Message::LinefeedMode(enabled));
            }

            // .in_band_size_reports => if (enabled) self.messageWriter(.{
            //     .size_report = .mode_2048,
            // }),
            modes::Mode::InBandSizeReports => {
                if enabled {
                    self.messageWriter(Message::SizeReport(
                        rustty_vt::size_report::Style::Mode2048,
                    ));
                }
            }

            // .report_visibility => if (enabled) self.messageWriter(.{
            //     .visibility_report = .{
            //         .visible = self.terminal.flags.visible,
            //         .force = true,
            //     },
            // }),
            modes::Mode::ReportVisibility => {
                if enabled {
                    let visible = self.term().flags.visible;
                    self.messageWriter(Message::VisibilityReport {
                        visible,
                        force: true,
                    });
                }
            }

            // .focus_event => if (enabled) self.messageWriter(.{
            //     .focused = self.terminal.flags.focused,
            // }),
            modes::Mode::FocusEvent => {
                if enabled {
                    let focused = self.term().flags.focused;
                    self.messageWriter(Message::Focused(focused));
                }
            }

            // .mouse_event_x10 => {
            modes::Mode::MouseEventX10 => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::X10;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }
            // .mouse_event_normal => {
            modes::Mode::MouseEventNormal => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::Normal;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }
            // .mouse_event_button => {
            modes::Mode::MouseEventButton => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::Button;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }
            // .mouse_event_any => {
            modes::Mode::MouseEventAny => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::Any;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }

            // .mouse_format_utf8 => self.terminal.flags.mouse_format = if (enabled) .utf8 else .x10,
            modes::Mode::MouseFormatUtf8 => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::Utf8
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }
            // .mouse_format_sgr => self.terminal.flags.mouse_format = if (enabled) .sgr else .x10,
            modes::Mode::MouseFormatSgr => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::Sgr
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }
            // .mouse_format_urxvt => self.terminal.flags.mouse_format = if (enabled) .urxvt else .x10,
            modes::Mode::MouseFormatUrxvt => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::Urxvt
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }
            // .mouse_format_sgr_pixels => self.terminal.flags.mouse_format = if (enabled) .sgr_pixels else .x10,
            modes::Mode::MouseFormatSgrPixels => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::SgrPixels
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }

            // else => {},
            _ => {}
        }

        Ok(())
    }

    // inline fn startHyperlink(self: *StreamHandler, uri: []const u8, id: ?[]const u8) !void {
    #[inline]
    fn startHyperlink(&mut self, uri: &[u8], id: Option<&[u8]>) -> Result<(), crate::Error> {
        // try self.terminal.screens.active.startHyperlink(uri, id);
        self.term()
            .active_mut()
            .start_hyperlink(uri, id)
            .map_err(|_| crate::Error)
    }

    // pub inline fn endHyperlink(self: *StreamHandler) !void {
    #[inline]
    pub fn endHyperlink(&mut self) -> Result<(), crate::Error> {
        // self.terminal.screens.active.endHyperlink();
        self.term().active_mut().end_hyperlink();
        Ok(())
    }

    // pub fn deviceAttributes(
    //     self: *StreamHandler,
    //     req: terminal.DeviceAttributeReq,
    // ) !void {
    pub fn deviceAttributes(&mut self, req: device_attributes::Req) -> Result<(), crate::Error> {
        // For the below, we quack as a VT220. We don't quack as
        // a 420 because we don't support DCS sequences.
        match req {
            // .primary => self.messageWriter(.{
            //     // 62 = Level 2 conformance
            //     // 22 = Color text
            //     // 52 = Clipboard access
            //     .write_stable = if (self.clipboard_write != .deny)
            //         "\x1B[?62;22;52c"
            //     else
            //         "\x1B[?62;22c",
            // }),
            device_attributes::Req::Primary => self.messageWriter(Message::WriteStable(
                // 62 = Level 2 conformance
                // 22 = Color text
                // 52 = Clipboard access
                if self.clipboard_write != ClipboardAccess::Deny {
                    b"\x1B[?62;22;52c"
                } else {
                    b"\x1B[?62;22c"
                },
            )),

            // .secondary => self.messageWriter(.{
            //     .write_stable = "\x1B[>1;10;0c",
            // }),
            device_attributes::Req::Secondary => {
                self.messageWriter(Message::WriteStable(b"\x1B[>1;10;0c"));
            }

            // else => log.warn("unimplemented device attributes req: {}", .{req}),
            _ => {
                // log.warn("unimplemented device attributes req: {}", .{req});
                log::warn!(target: LOG_SCOPE, "unimplemented device attributes req: {req:?}");
            }
        }
        Ok(())
    }

    // pub fn deviceStatusReport(
    //     self: *StreamHandler,
    //     req: terminal.device_status.Request,
    // ) !void {
    pub fn deviceStatusReport(&mut self, req: device_status::Request) -> Result<(), crate::Error> {
        match req {
            // .operating_status => self.messageWriter(.{ .write_stable = "\x1B[0n" }),
            device_status::Request::OperatingStatus => {
                self.messageWriter(Message::WriteStable(b"\x1B[0n"));
            }

            // .cursor_position => {
            device_status::Request::CursorPosition => {
                // const pos: struct {
                //     x: usize,
                //     y: usize,
                // } = if (self.terminal.modes.get(.origin)) .{
                //     .x = self.terminal.screens.active.cursor.x -| self.terminal.scrolling_region.left,
                //     .y = self.terminal.screens.active.cursor.y -| self.terminal.scrolling_region.top,
                // } else .{
                //     .x = self.terminal.screens.active.cursor.x,
                //     .y = self.terminal.screens.active.cursor.y,
                // };
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

                // Response always is at least 4 chars, so this leaves the
                // remainder for the row/column as base-10 numbers. This
                // will support a very large terminal.
                // var msg: termio.Message = .{ .write_small = .{} };
                // const resp = try std.fmt.bufPrint(&msg.write_small.data, "\x1B[{};{}R", .{
                //     pos.y + 1,
                //     pos.x + 1,
                // });
                // msg.write_small.len = @intCast(resp.len);
                let mut data = [0u8; 38];
                let len = write_fixed(&mut data[..], format_args!("\x1B[{};{}R", y + 1, x + 1))?;
                let msg = Message::WriteSmall(rustty_datastruct::message_data::Small { data, len });

                // self.messageWriter(msg);
                self.messageWriter(msg);
            }

            // .color_scheme => self.messageWriter(.{ .color_scheme_report = .{ .force = true } }),
            device_status::Request::ColorScheme => {
                self.messageWriter(Message::ColorSchemeReport { force: true });
            }

            // .visibility => self.messageWriter(.{ .visibility_report = .{
            //     .visible = self.terminal.flags.visible,
            //     .force = true,
            // } }),
            device_status::Request::Visibility => {
                let visible = self.term().flags.visible;
                self.messageWriter(Message::VisibilityReport {
                    visible,
                    force: true,
                });
            }
        }
        Ok(())
    }

    // pub fn setCursorStyle(...) !void { ... }
    //
    // PORT NOTE (oracle c594031d5): DELETED. The whole style/blink resolution
    // moved onto `Terminal::set_cursor_style`; keeping a copy here would
    // double-apply DECSCUSR and re-introduce a second owner of the defaults.

    // pub inline fn decaln(self: *StreamHandler) !void {
    #[inline]
    pub fn decaln(&mut self) -> Result<(), crate::Error> {
        // try self.terminal.decaln();
        self.term().decaln().map_err(|_| crate::Error)
    }

    // pub inline fn saveCursor(self: *StreamHandler) !void {
    #[inline]
    pub fn saveCursor(&mut self) -> Result<(), crate::Error> {
        // self.terminal.saveCursor();
        self.term().save_cursor();
        Ok(())
    }

    // pub inline fn restoreCursor(self: *StreamHandler) !void {
    #[inline]
    pub fn restoreCursor(&mut self) -> Result<(), crate::Error> {
        // self.terminal.restoreCursor();
        self.term().restore_cursor();
        Ok(())
    }

    // pub fn enquiry(self: *StreamHandler) !void {
    pub fn enquiry(&mut self) -> Result<(), crate::Error> {
        // log.debug("sending enquiry response={s}", .{self.enquiry_response});
        log::debug!(
            target: LOG_SCOPE,
            "sending enquiry response={}",
            String::from_utf8_lossy(&self.enquiry_response)
        );
        // self.messageWriter(try termio.Message.writeReq(self.alloc, self.enquiry_response));
        let msg = Message::writeReq(&self.enquiry_response);
        self.messageWriter(msg);
        Ok(())
    }

    // fn configureCharset(
    //     self: *StreamHandler,
    //     slot: terminal.CharsetSlot,
    //     set: terminal.Charset,
    // ) void {
    fn configureCharset(
        &mut self,
        slot: rustty_vt::charsets::Slots,
        set: rustty_vt::charsets::Charset,
    ) {
        // self.terminal.configureCharset(slot, set);
        self.term().configure_charset(slot, set);
    }

    // pub fn fullReset(
    //     self: *StreamHandler,
    // ) !void {
    pub fn fullReset(&mut self) -> Result<(), crate::Error> {
        // self.terminal.fullReset();
        self.term().full_reset();
        // try self.setMouseShape(.text);
        self.setMouseShape(rustty_vt::mouse::Shape::Text)?;

        // Full reset clears Kitty clipboard session grants.
        // self.kitty_clipboard_grants.deinit(self.alloc);
        // self.kitty_clipboard_grants = .{};
        self.kitty_clipboard_grants = rustty_vt::kitty::clipboard::Grants::default();

        // Reset resets our palette so we report it for mode 2031.
        // self.messageWriter(.{ .color_scheme_report = .{ .force = false } });
        self.messageWriter(Message::ColorSchemeReport { force: false });

        // Clear the progress bar
        // self.progressReport(.{ .state = .remove });
        self.progressReport(osc::ProgressReport {
            state: osc::ProgressReportState::Remove,
            progress: None,
        });
        Ok(())
    }

    /// Record a Kitty clipboard protocol session grant so future
    /// requests with this password skip the permission prompt.
    // pub fn kittyClipboardGrant(
    //     self: *StreamHandler,
    //     pw: []const u8,
    //     dir: terminal.kitty.clipboard.Grants.Direction,
    // ) error{OutOfMemory}!void {
    //     try self.kitty_clipboard_grants.grant(self.alloc, pw, dir, false);
    // }
    // (the `error{OutOfMemory}` collapses — `grant` is infallible under the
    // global allocator.)
    pub fn kittyClipboardGrant(&mut self, pw: &[u8], dir: rustty_vt::kitty::clipboard::Direction) {
        self.kitty_clipboard_grants.grant(pw, dir, false);
    }

    /// Handle one Kitty clipboard protocol (OSC 5522) packet.
    // fn kittyClipboard(
    //     self: *StreamHandler,
    //     v: terminal.osc.Command.KittyClipboardProtocol,
    // ) error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboard(&mut self, v: &osc::KittyClipboardProtocol) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;
        use rustty_vt::osc::parsers::kitty_clipboard_protocol::Operation;

        // Decode and validate the metadata. Malformed structure drops
        // the packet without a response. Invalid decoded text on a write
        // data or alias packet aborts an in-flight transaction.
        // var arena: std.heap.ArenaAllocator = .init(self.alloc);
        // defer arena.deinit();
        // (the arena scopes the Metadata allocations; the Rust Metadata owns
        // its Vecs and frees on drop.)
        let meta = match kitty_clipboard::Metadata::parse(&v.metadata) {
            Ok(Some(meta)) => meta,
            Ok(None) => return Ok(()),
            Err(kitty_clipboard::ParseError::OutOfMemory) => return Ok(()),
            Err(kitty_clipboard::ParseError::InvalidValue) => {
                // const state = self.kitty_clipboard_write orelse return;
                // switch (kitty_clipboard.Metadata.operation(v.metadata) orelse return) {
                //     .wdata, .walias => try self.kittyClipboardWriteFinish(
                //         state, .EINVAL, v.terminator),
                //     .read, .write => {},
                // }
                if self.kitty_clipboard_write.is_none() {
                    return Ok(());
                }
                match kitty_clipboard::Metadata::operation(&v.metadata) {
                    Some(Operation::Wdata) | Some(Operation::Walias) => {
                        if let Some(state) = self.kitty_clipboard_write.take() {
                            self.kittyClipboardWriteFinish(
                                &state,
                                kitty_clipboard::Status::Einval,
                                v.terminator,
                            )?;
                        }
                    }
                    Some(Operation::Read) | Some(Operation::Write) | None => {}
                }
                return Ok(());
            }
        };

        let payload: &[u8] = v.payload.as_deref().unwrap_or(b"");
        match meta.op {
            Operation::Read => self.kittyClipboardRead(&meta, payload, v.terminator),
            Operation::Write => self.kittyClipboardWriteBegin(&meta, v.terminator),
            Operation::Wdata => self.kittyClipboardWriteData(&meta, payload, v.terminator),
            Operation::Walias => self.kittyClipboardWriteAlias(&meta, payload, v.terminator),
        }
    }

    // fn kittyClipboardRead(self, meta, payload, terminator) !void {
    fn kittyClipboardRead(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // Everything about the request, including the request struct
        // itself, lives in a single arena that crosses to the surface
        // thread, which owns it from the moment the message is sent.
        //
        // PORT NOTE (assessment E hazard #1): the arena-owned request maps to
        // the OWNED `apprt::KittyRead` boxed into the surface message.

        // The payload is the requested MIME list. A read request with
        // an undecodable payload is dropped without any response,
        // matching kitty.
        let decoded = match kitty_clipboard::Payload::init(payload) {
            Ok(decoded) => decoded,
            Err(_) => return Ok(()),
        };
        if !decoded.is_valid_utf8() {
            return Ok(());
        }

        // The targets type ('.') asks for the listing of available
        // types rather than data. Requested types beyond the cap are
        // dropped and simply never served, which is how the protocol
        // reports an unavailable type anyway.
        // var mimes_buf: [kitty_clipboard.max_read_mimes][]const u8 = undefined;
        let mut mimes: Vec<Vec<u8>> = Vec::new();
        let mut list = false;
        let mut it = decoded.mime_iterator();
        while let Some(mime) = it.next() {
            if mime == kitty_clipboard::targets_mime {
                list = true;
                continue;
            }
            if mimes.len() == kitty_clipboard::max_read_mimes {
                continue;
            }
            mimes.push(mime.to_vec());
        }

        // Per the spec a password without a name is no password. A
        // stored session grant for it lets the surface skip its
        // permission prompt.
        let pw: &[u8] = if !meta.name.is_empty() { &meta.pw } else { b"" };
        let granted = self.kittyClipboardReadGranted(pw, mimes.len());

        // const req = try alloc.create(apprt.ClipboardRequest.KittyRead);
        // ...dupes... req.* = .{
        //     // The arena must be copied in last so it tracks every
        //     // allocation above.
        //     .arena = arena,
        //     ... };
        // (no arena in the port: the Box and its owned Vecs carry every
        // allocation, so there is no copy-order hazard.)
        let req = Box::new(rustty_apprt_api::KittyRead {
            location: match meta.loc {
                rustty_vt::clipboard::Location::Primary => rustty_apprt_api::Clipboard::Primary,
                _ => rustty_apprt_api::Clipboard::Standard,
            },
            mimes,
            list,
            id: meta.id.clone(),
            pw: pw.to_vec(),
            name: meta.name.clone(),
            granted,
            terminator,
        });

        // self.surfaceMessageWriter(.{ .kitty_clipboard_read = req });
        self.surfaceMessageWriter(SurfaceMessage::KittyClipboardRead(req));
        Ok(())
    }

    /// Whether a session grant covers a read request, consuming
    /// one-time grants. A prompt-exempt request never consults the
    /// grants: consuming a one-time paste password on a listing would
    /// burn the grant before the follow-up data read.
    // fn kittyClipboardReadGranted(self, pw: []const u8, mimes_len: usize) bool {
    fn kittyClipboardReadGranted(&mut self, pw: &[u8], mimes_len: usize) -> bool {
        if rustty_vt::kitty::clipboard::read_prompt_exempt(mimes_len) {
            return false;
        }
        self.kitty_clipboard_grants
            .use_(pw, rustty_vt::kitty::clipboard::Direction::Read)
    }

    /// Begin a Kitty clipboard write transaction (type=write).
    // fn kittyClipboardWriteBegin(self, meta, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteBegin(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // A new write silently replaces any in-flight transaction.
        self.kittyClipboardWriteAbort();

        // A write denied by policy can never succeed, so fail the
        // transaction up front instead of spooling data we'd only
        // throw away. Later wdata packets are ignored.
        if self.clipboard_write == ClipboardAccess::Deny {
            // log.info("application attempted to write clipboard, but 'clipboard-write' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
            );
            self.kittyClipboardWriteStatus(kitty_clipboard::Status::Eperm, &meta.id, terminator)?;
            return Ok(());
        }

        // const state = try self.alloc.create(terminal.kitty.clipboard.WriteState);
        // errdefer self.alloc.destroy(state);
        // state.* = try .init(self.alloc, meta, .{ .max_size = self.clipboard_write_limit });
        let state = Box::new(kitty_clipboard::WriteState::init(
            meta,
            kitty_clipboard::WriteOptions {
                max_size: self.clipboard_write_limit,
            },
        ));
        self.kitty_clipboard_write = Some(state);
        Ok(())
    }

    /// Accumulate one wdata chunk, or commit the transaction when the
    /// chunk carries no MIME type.
    // fn kittyClipboardWriteData(self, meta, payload, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteData(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // Data without a transaction is silently ignored, matching
        // kitty.
        // const state = self.kitty_clipboard_write orelse return;
        // (the Box is taken out while operated on and put back on success —
        // see the vt stream_terminal precedent.)
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return Ok(());
        };

        // A wdata packet without a MIME type commits the transaction.
        if meta.mime.is_empty() {
            return self.kittyClipboardWriteCommit(state, terminator);
        }

        match state.data(meta, payload) {
            Ok(()) => {
                self.kitty_clipboard_write = Some(state);
                Ok(())
            }

            // Failing to spool matches kitty's EIO for a failed buffer write.
            Err(kitty_clipboard::DataError::OutOfMemory) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Eio, terminator)
            }

            // Data over the write limit aborts the transaction and is
            // reported to the client.
            Err(kitty_clipboard::DataError::TooLarge) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Efbig, terminator)
            }

            // An invalid base64 payload stream aborts the transaction.
            Err(kitty_clipboard::DataError::Invalid) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Einval, terminator)
            }
        }
    }

    /// Register aliases from a walias packet.
    // fn kittyClipboardWriteAlias(self, meta, payload, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteAlias(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // Aliases without a transaction are silently ignored. Once a
        // transaction exists, a missing target MIME type is invalid and
        // aborts the transaction.
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return Ok(());
        };
        if meta.mime.is_empty() {
            return self.kittyClipboardWriteFinish(
                &state,
                kitty_clipboard::Status::Einval,
                terminator,
            );
        }

        match state.alias(meta, payload) {
            Ok(()) => {
                self.kitty_clipboard_write = Some(state);
                Ok(())
            }

            Err(kitty_clipboard::AliasError::OutOfMemory) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Eio, terminator)
            }

            // An undecodable alias payload aborts the transaction.
            Err(kitty_clipboard::AliasError::Invalid) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Einval, terminator)
            }
        }
    }

    /// Commit the transaction: resolve the final contents and forward
    /// them to the surface thread, which owns policy, any permission
    /// prompt, the clipboard write itself, and the final reply.
    // fn kittyClipboardWriteCommit(self, state, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteCommit(
        &mut self,
        mut state: Box<rustty_vt::kitty::clipboard::WriteState>,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        match self.kittyClipboardWriteSend(&mut state, terminator) {
            Ok(()) => {}
            // The last MIME type's payload stream was not correctly
            // padded, which aborts the transaction.
            // (error.OutOfMemory: the reference EIO-finishes then re-raises;
            // infallible here, carried for parity.)
            Err(kitty_clipboard::CommitError::OutOfMemory) => {
                return self.kittyClipboardWriteFinish(
                    &state,
                    kitty_clipboard::Status::Eio,
                    terminator,
                );
            }
            Err(kitty_clipboard::CommitError::Invalid) => {
                return self.kittyClipboardWriteFinish(
                    &state,
                    kitty_clipboard::Status::Einval,
                    terminator,
                );
            }
        }

        // The transaction is complete; the surface owns the reply.
        // self.kittyClipboardWriteAbort();
        // (the taken-out Box drops here.)
        drop(state);
        Ok(())
    }

    // fn kittyClipboardWriteSend(self, state, terminator)
    //     error{ OutOfMemory, Invalid }!void {
    fn kittyClipboardWriteSend(
        &mut self,
        state: &mut rustty_vt::kitty::clipboard::WriteState,
        terminator: osc::Terminator,
    ) -> Result<(), rustty_vt::kitty::clipboard::CommitError> {
        // const committed = try state.commit(self.alloc);
        // defer committed.deinit(self.alloc);
        //
        // PORT NOTE: `commit` borrows the state, so the grant lookup happens
        // before and the request is built from owned copies while `committed`
        // is live (`granted` is computed from `pw`, which needs `committed`
        // first — so it is read inside the borrow via a split: name/pw copies
        // are taken from the borrowed committed view).
        let (pw_owned, location, contents, id, name): (
            Vec<u8>,
            rustty_apprt_api::Clipboard,
            Vec<rustty_apprt_api::ClipboardContentOwned>,
            Vec<u8>,
            Vec<u8>,
        ) = {
            let committed = state.commit()?;
            let pw: &[u8] = if !committed.name.is_empty() {
                committed.pw
            } else {
                b""
            };
            (
                pw.to_vec(),
                match committed.loc {
                    rustty_vt::clipboard::Location::Primary => rustty_apprt_api::Clipboard::Primary,
                    _ => rustty_apprt_api::Clipboard::Standard,
                },
                committed
                    .contents
                    .iter()
                    .map(|c| rustty_apprt_api::ClipboardContentOwned {
                        mime: c.mime.to_vec(),
                        data: c.data.to_vec(),
                    })
                    .collect(),
                committed.id.to_vec(),
                committed.name.to_vec(),
            )
        };

        // Per the spec a password without a name is no password. A
        // stored session grant for it lets the surface skip its
        // permission prompt.
        // const granted = self.kitty_clipboard_grants.use(self.alloc, pw, .write);
        let granted = self
            .kitty_clipboard_grants
            .use_(&pw_owned, rustty_vt::kitty::clipboard::Direction::Write);

        // Everything about the request, including the request struct itself,
        // lives in a single arena that crosses to the surface thread ...
        // req.* = .{
        //     // The arena must be copied in last so it tracks every
        //     // allocation above.
        //     .arena = arena,
        //     ... };
        // (PORT NOTE: owned Box — see apprt::KittyRead; no copy-order hazard.)
        let req = Box::new(rustty_apprt_api::KittyWrite {
            location,
            contents,
            id,
            pw: pw_owned,
            name,
            granted,
            terminator,
        });

        // self.surfaceMessageWriter(.{ .kitty_clipboard_write = req });
        self.surfaceMessageWriter(SurfaceMessage::KittyClipboardWrite(req));
        Ok(())
    }

    /// Answer the write transaction with its final status and drop it.
    /// The id echoed is the one from the transaction's opening write
    /// packet, matching kitty.
    // fn kittyClipboardWriteFinish(self, state: *const WriteState, status,
    //     terminator) error{ OutOfMemory, WriteFailed }!void {
    //     defer self.kittyClipboardWriteAbort();
    //     try self.kittyClipboardWriteStatus(status, state.id, terminator);
    // }
    // (callers hand the taken-out Box by reference; its drop at the caller is
    // the abort — any STORED transaction was already taken.)
    fn kittyClipboardWriteFinish(
        &mut self,
        state: &rustty_vt::kitty::clipboard::WriteState,
        status: rustty_vt::kitty::clipboard::Status,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        self.kittyClipboardWriteAbort();
        self.kittyClipboardWriteStatus(status, &state.id, terminator)
    }

    /// Reply to a write transaction with a single status packet.
    // fn kittyClipboardWriteStatus(self, status, id, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteStatus(
        &mut self,
        status: rustty_vt::kitty::clipboard::Status,
        id: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        // var stream: std.Io.Writer.Allocating = .init(self.alloc);
        // defer stream.deinit();
        let mut stream = rustty_vt::snapshot::io::AllocatingWriter::new();
        (rustty_vt::kitty::clipboard::Response {
            id,
            terminator,
            ..rustty_vt::kitty::clipboard::Response::new(
                rustty_vt::osc::parsers::kitty_clipboard_protocol::Operation::Write,
                status,
            )
        })
        .encode(&mut stream)
        .map_err(|_| crate::Error)?;
        // self.messageWriter(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try stream.toOwnedSlice(),
        // } });
        self.messageWriter(Message::WriteAlloc(
            rustty_datastruct::message_data::Alloc {
                data: stream.written().to_vec().into_boxed_slice(),
            },
        ));
        Ok(())
    }

    /// Drop any in-flight write transaction without responding.
    // fn kittyClipboardWriteAbort(self: *StreamHandler) void {
    //     if (self.kitty_clipboard_write) |state| {
    //         state.deinit(self.alloc);
    //         self.alloc.destroy(state);
    //         self.kitty_clipboard_write = null;
    //     }
    // }
    fn kittyClipboardWriteAbort(&mut self) {
        // (dropping the Box is the deinit + destroy.)
        self.kitty_clipboard_write = None;
    }

    // pub fn queryKittyKeyboard(self: *StreamHandler) !void {
    pub fn queryKittyKeyboard(&mut self) -> Result<(), crate::Error> {
        // log.debug("querying kitty keyboard mode", .{});
        log::debug!(target: LOG_SCOPE, "querying kitty keyboard mode");
        // var data: termio.Message.WriteReq.Small.Array = undefined;
        // const resp = try std.fmt.bufPrint(&data, "\x1b[?{}u", .{
        //     self.terminal.screens.active.kitty_keyboard.current().int(),
        // });
        let mut data = [0u8; 38];
        let current = self.term().active_mut().kitty_keyboard.current().int();
        let len = write_fixed(&mut data[..], format_args!("\x1b[?{current}u"))?;

        // self.messageWriter(.{
        //     .write_small = .{
        //         .data = data,
        //         .len = @intCast(resp.len),
        //     },
        // });
        self.messageWriter(Message::WriteSmall(
            rustty_datastruct::message_data::Small { data, len },
        ));
        Ok(())
    }

    // pub fn reportXtversion(
    //     self: *StreamHandler,
    // ) !void {
    pub fn reportXtversion(&mut self) -> Result<(), crate::Error> {
        // log.debug("reporting XTVERSION: ghostty {s}", .{build_config.version_string});
        //
        // (R2 identity: `ghostty` in the reference's literal is the product name.)
        log::debug!(
            target: LOG_SCOPE,
            "reporting XTVERSION: {} {}",
            rustty_identity::XTVERSION_NAME,
            rustty_build_config::VERSION_STRING
        );
        // var buf: [288]u8 = undefined;
        // const resp = try std.fmt.bufPrint(
        //     &buf,
        //     "\x1BP>|{s} {s}\x1B\\",
        //     .{
        //         "ghostty",
        //         build_config.version_string,
        //     },
        // );
        //
        // (R2 identity: the XTVERSION product name.)
        let mut buf = [0u8; 288];
        let len = write_fixed(
            &mut buf[..],
            format_args!(
                "\x1BP>|{} {}\x1B\\",
                rustty_identity::XTVERSION_NAME,
                rustty_build_config::VERSION_STRING,
            ),
        )?;
        // const msg = try termio.Message.writeReq(self.alloc, resp);
        let msg = Message::writeReq(&buf[..len]);
        // self.messageWriter(msg);
        self.messageWriter(msg);
        Ok(())
    }

    //-------------------------------------------------------------------------
    // OSC

    // fn windowTitle(self: *StreamHandler, title: []const u8) !void {
    fn windowTitle(&mut self, title: &[u8]) -> Result<(), crate::Error> {
        // var buf: [256]u8 = undefined;
        let mut buf = [0u8; 256];
        // if (title.len >= buf.len) {
        //     log.warn("change title requested larger than our buffer size, ignoring", .{});
        //     return;
        // }
        if title.len() >= buf.len() {
            // log.warn("change title requested larger than our buffer size, ignoring", .{});
            log::warn!(
                target: LOG_SCOPE,
                "change title requested larger than our buffer size, ignoring"
            );
            return Ok(());
        }

        // Set the title on the terminal state. We ignore any errors since
        // we can continue to operate just fine without it.
        // self.terminal.setTitle(title) catch |err| {
        //     log.warn("error setting title in terminal state: {}", .{err});
        // };
        if let Err(err) = self.term().set_title(title) {
            // log.warn("error setting title in terminal state: {}", .{err});
            log::warn!(target: LOG_SCOPE, "error setting title in terminal state: {err:?}");
        }

        // @memcpy(buf[0..title.len], title);
        buf[..title.len()].copy_from_slice(title);
        // buf[title.len] = 0;
        buf[title.len()] = 0;

        // Special handling for the empty title. We treat the empty title
        // as resetting to as if we never saw a title. Other terminals
        // behave this way too (e.g. iTerm2).
        // if (title.len == 0) {
        if title.is_empty() {
            // If we have a pwd then we set the title as the pwd else
            // we just set it to blank.
            // if (self.terminal.getPwd()) |pwd| pwd: {
            //     if (pwd.len >= buf.len) break :pwd;
            //     @memcpy(buf[0..pwd.len], pwd);
            //     buf[pwd.len] = 0;
            // }
            'pwd: {
                let term = self.term();
                if let Some(pwd) = term.get_pwd() {
                    if pwd.len() >= buf.len() {
                        break 'pwd;
                    }
                    let pwd_len = pwd.len();
                    buf[..pwd_len].copy_from_slice(pwd);
                    buf[pwd_len] = 0;
                }
            }

            // self.surfaceMessageWriter(.{ .set_title = buf });
            self.surfaceMessageWriter(SurfaceMessage::SetTitle(buf));
            // self.seen_title = false;
            self.seen_title = false;
            return Ok(());
        }

        // self.seen_title = true;
        self.seen_title = true;
        // self.surfaceMessageWriter(.{ .set_title = buf });
        self.surfaceMessageWriter(SurfaceMessage::SetTitle(buf));
        Ok(())
    }

    // inline fn setMouseShape(
    //     self: *StreamHandler,
    //     shape: terminal.MouseShape,
    // ) !void {
    #[inline]
    fn setMouseShape(&mut self, shape: rustty_vt::mouse::Shape) -> Result<(), crate::Error> {
        // Avoid changing the shape if it is already set to avoid excess
        // cross-thread messaging.
        // if (self.terminal.mouse_shape == shape) return;
        if self.term().mouse_shape == shape {
            return Ok(());
        }

        // self.terminal.mouse_shape = shape;
        self.term().mouse_shape = shape;
        // self.surfaceMessageWriter(.{ .set_mouse_shape = shape });
        self.surfaceMessageWriter(SurfaceMessage::SetMouseShape(shape));
        Ok(())
    }

    // fn clipboardContents(self: *StreamHandler, kind: u8, data: []const u8) !void {
    fn clipboardContents(&mut self, kind: u8, data: &[u8]) -> Result<(), crate::Error> {
        // Note: we ignore the "kind" field and always use the standard clipboard.
        // iTerm also appears to do this but other terminals seem to only allow
        // certain. Let's investigate more.

        // const clipboard_type: apprt.Clipboard = switch (kind) {
        //     'c' => .standard,
        //     's' => .selection,
        //     'p' => .primary,
        //     else => .standard,
        // };
        let clipboard_type: Clipboard = match kind {
            b'c' => Clipboard::Standard,
            b's' => Clipboard::Selection,
            b'p' => Clipboard::Primary,
            _ => Clipboard::Standard,
        };

        // Get clipboard contents
        // if (data.len == 1 and data[0] == '?') {
        if data.len() == 1 && data[0] == b'?' {
            // self.surfaceMessageWriter(.{ .clipboard_read = clipboard_type });
            self.surfaceMessageWriter(SurfaceMessage::ClipboardRead(clipboard_type));
            return Ok(());
        }

        // Write clipboard contents
        // self.surfaceMessageWriter(.{
        //     .clipboard_write = .{
        //         .req = try apprt.surface.Message.WriteReq.init(
        //             self.alloc,
        //             data,
        //         ),
        //         .clipboard_type = clipboard_type,
        //     },
        // });
        self.surfaceMessageWriter(SurfaceMessage::ClipboardWrite {
            req: rustty_apprt_api::surface::WriteReq::init(data),
            clipboard_type,
        });
        Ok(())
    }

    // fn semanticPrompt(
    //     self: *StreamHandler,
    //     cmd: Stream.Action.SemanticPrompt,
    // ) !void {
    fn semanticPrompt(&mut self, cmd: stream::SemanticPrompt) -> Result<(), crate::Error> {
        use rustty_vt::osc::semantic_prompt::option;
        use rustty_vt::osc::semantic_prompt::Action;

        // switch (cmd.action) {
        match cmd.action {
            // .end_input_start_output => {
            //     self.surfaceMessageWriter(.start_command);
            // },
            Action::EndInputStartOutput => {
                self.surfaceMessageWriter(SurfaceMessage::StartCommand);
            }

            // .end_command => {
            Action::EndCommand => {
                // The specification seems to not specify the type but
                // other terminals accept 32-bits, but exit codes are really
                // bytes, so we just do our best here.
                // const code: u8 = code: {
                //     const raw: i32 = cmd.readOption(.exit_code) orelse 0;
                //     break :code std.math.cast(u8, raw) orelse 1;
                // };
                let code: u8 = {
                    let raw: i32 = cmd.read_option(option::EXIT_CODE).unwrap_or(0);
                    u8::try_from(raw).unwrap_or(1)
                };

                // self.surfaceMessageWriter(.{ .stop_command = code });
                self.surfaceMessageWriter(SurfaceMessage::StopCommand(Some(code)));
            }

            // Handled by Terminal, no special handling by us
            // .end_prompt_start_input,
            // .end_prompt_start_input_terminate_eol,
            // .fresh_line,
            // .fresh_line_new_prompt,
            // .new_command,
            // .prompt_start,
            // => {},
            Action::EndPromptStartInput
            | Action::EndPromptStartInputTerminateEol
            | Action::FreshLine
            | Action::FreshLineNewPrompt
            | Action::NewCommand
            | Action::PromptStart => {}
        }

        // We do this last so failures are still processed correctly
        // above.
        // try self.terminal.semanticPrompt(cmd);
        self.term().semantic_prompt(cmd).map_err(|_| crate::Error)
    }

    // fn reportPwd(self: *StreamHandler, url: []const u8) !void {
    fn reportPwd(&mut self, url: &[u8]) -> Result<(), crate::Error> {
        // Special handling for the empty URL. We treat the empty URL
        // as resetting the pwd as if we never saw a pwd. I can't find any
        // other terminal that does this but it seems like a reasonable
        // behavior that enables some useful features. For example, the macOS
        // proxy icon can be hidden when a program reports it doesn't know
        // the pwd rather than showing a stale pwd.
        // if (url.len == 0) {
        if url.is_empty() {
            // Blank value can never fail because no allocs happen.
            // self.terminal.setPwd("") catch unreachable;
            self.term().set_pwd(b"").unwrap();

            // If we haven't seen a title, we're using the pwd as our title.
            // Set it to blank which will reset our title behavior.
            // if (!self.seen_title) {
            if !self.seen_title {
                // try self.windowTitle("");
                self.windowTitle(b"")?;
                // assert(!self.seen_title);
                debug_assert!(!self.seen_title);
            }

            // Report the change.
            // self.surfaceMessageWriter(.{ .pwd_change = .{ .stable = "" } });
            self.surfaceMessageWriter(SurfaceMessage::PwdChange(
                rustty_datastruct::MessageData::Stable(b""),
            ));
            return Ok(());
        }

        // if (builtin.os.tag == .windows) {
        //     log.warn("reportPwd unimplemented on windows", .{});
        //     return;
        // }
        if cfg!(windows) {
            // log.warn("reportPwd unimplemented on windows", .{});
            log::warn!(target: LOG_SCOPE, "reportPwd unimplemented on windows");
            return Ok(());
        }

        // Attempt to parse this file-style URI using options appropriate
        // for this OSC 7 context (e.g. kitty-shell-cwd expects the full,
        // unencoded path).
        // const uri: std.Uri = internal_os.uri.parse(url, .{
        //     .mac_address = comptime builtin.os.tag != .macos,
        //     .raw_path = std.mem.startsWith(u8, url, "kitty-shell-cwd://"),
        // }) catch |e| {
        //     log.warn("invalid url in OSC 7: {}", .{e});
        //     return;
        // };
        let uri = match rustty_os::uri::parse(
            url,
            rustty_os::uri::ParseOptions {
                mac_address: cfg!(not(target_os = "macos")),
                raw_path: url.starts_with(b"kitty-shell-cwd://"),
            },
        ) {
            Ok(uri) => uri,
            Err(e) => {
                // log.warn("invalid url in OSC 7: {}", .{e});
                log::warn!(target: LOG_SCOPE, "invalid url in OSC 7: {e:?}");
                return Ok(());
            }
        };

        // if (!std.mem.eql(u8, "file", uri.scheme) and
        //     !std.mem.eql(u8, "kitty-shell-cwd", uri.scheme))
        // {
        //     log.warn("OSC 7 scheme must be file or kitty-shell-cwd, got: {s}", .{uri.scheme});
        //     return;
        // }
        if uri.scheme != b"file" && uri.scheme != b"kitty-shell-cwd" {
            // log.warn("OSC 7 scheme must be file or kitty-shell-cwd, got: {s}", .{uri.scheme});
            log::warn!(
                target: LOG_SCOPE,
                "OSC 7 scheme must be file or kitty-shell-cwd, got: {}",
                String::from_utf8_lossy(uri.scheme)
            );
            return Ok(());
        }

        // var host_buffer: [std.Uri.host_name_max]u8 = undefined;
        // const host = uri.getHost(&host_buffer) catch |err| switch (err) {
        //     error.UriMissingHost => {
        //         log.warn("OSC 7 uri must contain a hostname: {}", .{err});
        //         return;
        //     },
        //     error.UriHostTooLong => {
        //         log.warn("failed to get full hostname for OSC 7 validation: {}", .{err});
        //         return;
        //     },
        // };
        let mut host_buffer = [0u8; URI_HOST_NAME_MAX];
        let host = match uri_get_host(&uri, &mut host_buffer) {
            Ok(host) => host,
            Err(err @ GetHostError::UriMissingHost) => {
                // log.warn("OSC 7 uri must contain a hostname: {}", .{err});
                log::warn!(target: LOG_SCOPE, "OSC 7 uri must contain a hostname: {err:?}");
                return Ok(());
            }
            // PORT NOTE (task-14e-1, 2026-09-10): this arm and its
            // `failed to get full hostname for OSC 7 validation` message are
            // NOT in the oracle at pin 20abdb50a — `uri.getHost`'s error set
            // there is `error{UriMissingHost}` only. The Zig comment is a stale
            // pre-resync artifact, so NO live emission is added here; adding one
            // would be an emission the reference does not make. The arm itself
            // is left alone (removing it is a behavioural change, out of this
            // task's scope) and is filed in the report as a finding.
            Err(GetHostError::UriHostTooLong) => {
                // (no reference counterpart at the pin — see the note above.)
                return Ok(());
            }
        };

        // OSC 7 is a little sketchy because anyone can send any value from
        // any host (such an SSH session). The best practice terminals follow
        // is to valid the hostname to be local.
        // const host_valid = internal_os.hostname.isLocal(host) catch |err| switch (err) {
        //     error.PermissionDenied,
        //     error.Unexpected,
        //     => {
        //         log.warn("failed to get hostname for OSC 7 validation: {}", .{err});
        //         return;
        //     },
        // };
        let host_valid = match rustty_os::hostname::isLocal(host) {
            Ok(v) => v,
            Err(
                err @ (rustty_os::hostname::LocalHostnameValidationError::PermissionDenied
                | rustty_os::hostname::LocalHostnameValidationError::Unexpected),
            ) => {
                // log.warn("failed to get hostname for OSC 7 validation: {}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to get hostname for OSC 7 validation: {err:?}"
                );
                return Ok(());
            }
        };
        // if (!host_valid) {
        //     log.warn("OSC 7 host ({s}) must be local", .{host});
        //     return;
        // }
        if !host_valid {
            // log.warn("OSC 7 host ({s}) must be local", .{host});
            log::warn!(
                target: LOG_SCOPE,
                "OSC 7 host ({}) must be local",
                String::from_utf8_lossy(host)
            );
            return Ok(());
        }

        // We need the raw path, which might require unescaping. We try to
        // avoid making any heap allocations by using the stack first.
        // var arena_alloc: std.heap.ArenaAllocator = .init(self.alloc);
        // var stack_alloc = std.heap.stackFallback(1024, arena_alloc.allocator());
        // defer arena_alloc.deinit();
        // const path = try uri.path.toRawMaybeAlloc(stack_alloc.get());
        //
        // (PORT NOTE: the stackFallback/arena collapse into the owned Vec.)
        let path: Vec<u8> = {
            let mut w = VecWriter(Vec::new());
            uri.path.formatRaw(&mut w).map_err(|_| crate::Error)?;
            w.0
        };

        // log.debug("terminal pwd: {s}", .{path});
        log::debug!(
            target: LOG_SCOPE,
            "terminal pwd: {}",
            String::from_utf8_lossy(&path)
        );
        // try self.terminal.setPwd(path);
        self.term().set_pwd(&path).map_err(|_| crate::Error)?;

        // Report it to the surface. If creating our write request fails
        // then we just ignore it.
        // if (apprt.surface.Message.WriteReq.init(self.alloc, path)) |req| {
        //     self.surfaceMessageWriter(.{ .pwd_change = req });
        // } else |err| {
        //     log.warn("error notifying surface of pwd change err={}", .{err});
        // }
        //
        // (PORT NOTE: `WriteReq.init` is infallible in Rust's allocation model —
        // the error branch collapses, preserved verbatim above.)
        //
        // PORT NOTE (G-LOGFILE / task-14e-1, 2026-09-10): this is
        // `stream_handler.zig:1547`, the ONE site of this file's 41 that stays
        // UNRESTORED. There is no failure arm to hang it on and emitting it
        // unconditionally would put a diagnostic on a SUCCESS path — the bug this
        // project already shipped once. `LOG-PORTMAP.tsv`'s
        // `src/termio/stream_handler.zig` allowance is held at 1 for exactly this
        // site, with the reason in its `reason` column; see `ADJUDICATION.md`.
        let req = rustty_apprt_api::surface::WriteReq::init(&path);
        self.surfaceMessageWriter(SurfaceMessage::PwdChange(req));

        // If we haven't seen a title, use our pwd as the title.
        // if (!self.seen_title) {
        if !self.seen_title {
            // try self.windowTitle(path);
            self.windowTitle(&path)?;
            // self.seen_title = false;
            self.seen_title = false;
        }
        Ok(())
    }

    // fn colorOperation(
    //     self: *StreamHandler,
    //     op: terminal.osc.color.Operation,
    //     requests: *const terminal.osc.color.List,
    //     terminator: terminal.osc.Terminator,
    // ) !void {
    fn colorOperation(
        &mut self,
        op: osc::color::Operation,
        requests: &osc::color::List,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        // We'll need op one day if we ever implement reporting special colors.
        // _ = op;
        let _ = op;

        // return early if there is nothing to do
        // if (requests.count() == 0) return;
        if requests.is_empty() {
            return Ok(());
        }

        // var buffer: [1024]u8 = undefined;
        // var fba: std.heap.FixedBufferAllocator = .init(&buffer);
        // const alloc = fba.allocator();
        //
        // var response: std.ArrayListUnmanaged(u8) = .empty;
        // const writer = response.writer(alloc);
        //
        // (PORT NOTE: the fixed-buffer allocator + unmanaged list collapse into the
        // owned response Vec/String accumulator.)
        let mut response: Vec<u8> = Vec::new();

        // var it = requests.constIterator(0);
        // while (it.next()) |req| {
        for req in requests.iter() {
            match req {
                // .set => |set| {
                osc::color::Request::Set(set) => {
                    match &set.target {
                        // .palette => |i| {
                        //     self.terminal.flags.dirty.palette = true;
                        //     self.terminal.colors.palette.set(i, set.color);
                        // },
                        osc::color::Target::Palette(i) => {
                            self.term().flags.dirty.palette = true;
                            self.term().colors.palette.set(*i, set.color);
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
                            // => log.info("setting dynamic color {s} not implemented", .{
                            //     @tagName(dynamic),
                            // }),
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {
                                // log.info("setting dynamic color {s} not implemented", ...);
                                log::info!(
                                    target: LOG_SCOPE,
                                    "setting dynamic color {} not implemented",
                                    dynamic.tag_name()
                                );
                            }
                        },
                        // .special => log.info("setting special colors not implemented", .{}),
                        osc::color::Target::Special(_) => {
                            // log.info("setting special colors not implemented", .{});
                            log::info!(
                                target: LOG_SCOPE,
                                "setting special colors not implemented"
                            );
                        }
                    }

                    // Notify the surface of the color change
                    // self.surfaceMessageWriter(.{ .color_change = .{
                    //     .target = set.target,
                    //     .color = set.color,
                    // } });
                    self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                        osc::color::ColoredTarget {
                            target: set.target,
                            color: set.color,
                        },
                    ));
                }

                // .reset => |target| switch (target) {
                osc::color::Request::Reset(target) => match target {
                    // .palette => |i| {
                    osc::color::Target::Palette(i) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*i);

                        // self.surfaceMessageWriter(.{
                        //     .color_change = .{
                        //         .target = target,
                        //         .color = self.terminal.colors.palette.current[i],
                        //     },
                        // });
                        let current = self.term().colors.palette.current[*i as usize];
                        self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                            osc::color::ColoredTarget {
                                target: *target,
                                color: current,
                            },
                        ));
                    }
                    // .dynamic => |dynamic| switch (dynamic) {
                    osc::color::Target::Dynamic(dynamic) => match dynamic {
                        // .foreground => {
                        color::Dynamic::Foreground => {
                            self.term().colors.foreground.reset();

                            // if (self.terminal.colors.foreground.default) |c| {
                            if let Some(c) = self.term().colors.foreground.default {
                                self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                                    osc::color::ColoredTarget {
                                        target: *target,
                                        color: c,
                                    },
                                ));
                            }
                        }
                        // .background => {
                        color::Dynamic::Background => {
                            self.term().colors.background.reset();

                            // if (self.terminal.colors.background.default) |c| {
                            if let Some(c) = self.term().colors.background.default {
                                self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                                    osc::color::ColoredTarget {
                                        target: *target,
                                        color: c,
                                    },
                                ));
                            }
                        }
                        // .cursor => {
                        color::Dynamic::Cursor => {
                            self.term().colors.cursor.reset();

                            // if (self.terminal.colors.cursor.default) |c| {
                            if let Some(c) = self.term().colors.cursor.default {
                                self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                                    osc::color::ColoredTarget {
                                        target: *target,
                                        color: c,
                                    },
                                ));
                            }
                        }
                        // .pointer_foreground,
                        // .pointer_background,
                        // .tektronix_foreground,
                        // .tektronix_background,
                        // .highlight_background,
                        // .tektronix_cursor,
                        // .highlight_foreground,
                        // => log.warn("resetting dynamic color {s} not implemented", .{
                        //     @tagName(dynamic),
                        // }),
                        color::Dynamic::PointerForeground
                        | color::Dynamic::PointerBackground
                        | color::Dynamic::TektronixForeground
                        | color::Dynamic::TektronixBackground
                        | color::Dynamic::HighlightBackground
                        | color::Dynamic::TektronixCursor
                        | color::Dynamic::HighlightForeground => {
                            // log.warn("resetting dynamic color {s} not implemented", ...);
                            log::warn!(
                                target: LOG_SCOPE,
                                "resetting dynamic color {} not implemented",
                                dynamic.tag_name()
                            );
                        }
                    },
                    // .special => log.info("resetting special colors not implemented", .{}),
                    osc::color::Target::Special(_) => {
                        // log.info("resetting special colors not implemented", .{});
                        log::info!(target: LOG_SCOPE, "resetting special colors not implemented");
                    }
                },

                // .reset_palette => {
                osc::color::Request::ResetPalette => {
                    // const mask = &self.terminal.colors.palette.mask;
                    // var mask_it = mask.iterator(.{});
                    // while (mask_it.next()) |i| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(@intCast(i));
                    //     self.surfaceMessageWriter(.{
                    //         .color_change = .{
                    //             .target = .{ .palette = @intCast(i) },
                    //             .color = self.terminal.colors.palette.current[i],
                    //         },
                    //     });
                    // }
                    // mask.* = .initEmpty();
                    let indices: Vec<usize> = self.term().colors.palette.mask.iter_set().collect();
                    for i in indices {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(i as u8);
                        let current = self.term().colors.palette.current[i];
                        self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                            osc::color::ColoredTarget {
                                target: osc::color::Target::Palette(i as u8),
                                color: current,
                            },
                        ));
                    }
                    self.term().colors.palette.mask = color::PaletteMask::init_empty();
                }

                // .reset_special => log.warn(
                //     "resetting all special colors not implemented",
                //     .{},
                // ),
                osc::color::Request::ResetSpecial => {
                    // log.warn("resetting all special colors not implemented", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "resetting all special colors not implemented"
                    );
                }

                // .query => |kind| report: {
                osc::color::Request::Query(kind) => 'report: {
                    // if (self.osc_color_report_format == .none) break :report;
                    if self.osc_color_report_format == OSCColorReportFormat::None {
                        break 'report;
                    }

                    // const color = switch (kind) {
                    let color_val: color::RGB = match kind {
                        // .palette => |i| self.terminal.colors.palette.current[i],
                        osc::color::Target::Palette(i) => {
                            self.term().colors.palette.current[*i as usize]
                        }
                        // .dynamic => |dynamic| switch (dynamic) {
                        osc::color::Target::Dynamic(dynamic) => match dynamic {
                            // .foreground => self.terminal.colors.foreground.get().?,
                            color::Dynamic::Foreground => {
                                self.term().colors.foreground.get().unwrap()
                            }
                            // .background => self.terminal.colors.background.get().?,
                            color::Dynamic::Background => {
                                self.term().colors.background.get().unwrap()
                            }
                            // .cursor => self.terminal.colors.cursor.get() orelse
                            //     self.terminal.colors.foreground.get().?,
                            color::Dynamic::Cursor => {
                                let cursor = self.term().colors.cursor.get();
                                match cursor {
                                    Some(c) => c,
                                    None => self.term().colors.foreground.get().unwrap(),
                                }
                            }
                            // .pointer_foreground,
                            // .pointer_background,
                            // .tektronix_foreground,
                            // .tektronix_background,
                            // .highlight_background,
                            // .tektronix_cursor,
                            // .highlight_foreground,
                            // => {
                            //     log.info(
                            //         "reporting dynamic color {s} not implemented",
                            //         .{@tagName(dynamic)},
                            //     );
                            //     break :report;
                            // },
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {
                                // log.info("reporting dynamic color {s} not implemented", ...);
                                log::info!(
                                    target: LOG_SCOPE,
                                    "reporting dynamic color {} not implemented",
                                    dynamic.tag_name()
                                );
                                break 'report;
                            }
                        },
                        // .special => {
                        //     log.info("reporting special colors not implemented", .{});
                        //     break :report;
                        // },
                        osc::color::Target::Special(_) => {
                            // log.info("reporting special colors not implemented", .{});
                            log::info!(target: LOG_SCOPE, "reporting special colors not implemented");
                            break 'report;
                        }
                    };

                    // switch (self.osc_color_report_format) {
                    match self.osc_color_report_format {
                        // .@"16-bit" => switch (kind) {
                        OSCColorReportFormat::Bit16 => match kind {
                            // .palette => |i| try writer.print(
                            //     "\x1b]4;{d};rgb:{x:0>4}/{x:0>4}/{x:0>4}",
                            //     .{
                            //         i,
                            //         @as(u16, color.r) * 257,
                            //         @as(u16, color.g) * 257,
                            //         @as(u16, color.b) * 257,
                            //     },
                            // ),
                            osc::color::Target::Palette(i) => write!(
                                VecFmt(&mut response),
                                "\x1b]4;{};rgb:{:0>4x}/{:0>4x}/{:0>4x}",
                                i,
                                u16::from(color_val.r) * 257,
                                u16::from(color_val.g) * 257,
                                u16::from(color_val.b) * 257,
                            )
                            .map_err(|_| crate::Error)?,
                            // .dynamic => |dynamic| try writer.print(
                            //     "\x1b]{d};rgb:{x:0>4}/{x:0>4}/{x:0>4}",
                            //     .{
                            //         @intFromEnum(dynamic),
                            //         @as(u16, color.r) * 257,
                            //         @as(u16, color.g) * 257,
                            //         @as(u16, color.b) * 257,
                            //     },
                            // ),
                            osc::color::Target::Dynamic(dynamic) => write!(
                                VecFmt(&mut response),
                                "\x1b]{};rgb:{:0>4x}/{:0>4x}/{:0>4x}",
                                *dynamic as u16,
                                u16::from(color_val.r) * 257,
                                u16::from(color_val.g) * 257,
                                u16::from(color_val.b) * 257,
                            )
                            .map_err(|_| crate::Error)?,
                            // .special => unreachable,
                            osc::color::Target::Special(_) => unreachable!(),
                        },

                        // .@"8-bit" => switch (kind) {
                        OSCColorReportFormat::Bit8 => match kind {
                            // .palette => |i| try writer.print(
                            //     "\x1b]4;{d};rgb:{x:0>2}/{x:0>2}/{x:0>2}",
                            //     .{
                            //         i,
                            //         @as(u16, color.r),
                            //         @as(u16, color.g),
                            //         @as(u16, color.b),
                            //     },
                            // ),
                            osc::color::Target::Palette(i) => write!(
                                VecFmt(&mut response),
                                "\x1b]4;{};rgb:{:0>2x}/{:0>2x}/{:0>2x}",
                                i,
                                u16::from(color_val.r),
                                u16::from(color_val.g),
                                u16::from(color_val.b),
                            )
                            .map_err(|_| crate::Error)?,
                            // .dynamic => |dynamic| try writer.print(
                            //     "\x1b]{d};rgb:{x:0>2}/{x:0>2}/{x:0>2}",
                            //     .{
                            //         @intFromEnum(dynamic),
                            //         @as(u16, color.r),
                            //         @as(u16, color.g),
                            //         @as(u16, color.b),
                            //     },
                            // ),
                            osc::color::Target::Dynamic(dynamic) => write!(
                                VecFmt(&mut response),
                                "\x1b]{};rgb:{:0>2x}/{:0>2x}/{:0>2x}",
                                *dynamic as u16,
                                u16::from(color_val.r),
                                u16::from(color_val.g),
                                u16::from(color_val.b),
                            )
                            .map_err(|_| crate::Error)?,
                            // .special => unreachable,
                            osc::color::Target::Special(_) => unreachable!(),
                        },

                        // .none => unreachable,
                        OSCColorReportFormat::None => unreachable!(),
                    }

                    // try writer.writeAll(terminator.string());
                    response.extend_from_slice(terminator.string());
                }
            }
        }

        // if (response.items.len > 0) {
        if !response.is_empty() {
            // If any of the operations were reports, finalize the report
            // string and send it to the terminal.
            // const msg = try termio.Message.writeReq(self.alloc, response.items);
            let msg = Message::writeReq(&response);
            // self.messageWriter(msg);
            self.messageWriter(msg);
        }
        Ok(())
    }

    // fn showDesktopNotification(
    //     self: *StreamHandler,
    //     title: []const u8,
    //     body: []const u8,
    // ) !void {
    fn showDesktopNotification(&mut self, title: &[u8], body: &[u8]) -> Result<(), crate::Error> {
        // self.surfaceMessageWriter(.{
        //     .desktop_notification = .init(title, body),
        // });
        self.surfaceMessageWriter(SurfaceMessage::DesktopNotification(
            SurfaceDesktopNotification::init(title, body),
        ));
        Ok(())
    }

    /// Send a report to the pty.
    // pub fn sendSizeReport(self: *StreamHandler, style: terminal.SizeReportStyle) void {
    pub fn sendSizeReport(&mut self, style: csi::SizeReportStyle) {
        match style {
            // .csi_14_t => self.messageWriter(.{ .size_report = .csi_14_t }),
            csi::SizeReportStyle::Csi14T => {
                self.messageWriter(Message::SizeReport(rustty_vt::size_report::Style::Csi14T));
            }
            // .csi_16_t => self.messageWriter(.{ .size_report = .csi_16_t }),
            csi::SizeReportStyle::Csi16T => {
                self.messageWriter(Message::SizeReport(rustty_vt::size_report::Style::Csi16T));
            }
            // .csi_18_t => self.messageWriter(.{ .size_report = .csi_18_t }),
            csi::SizeReportStyle::Csi18T => {
                self.messageWriter(Message::SizeReport(rustty_vt::size_report::Style::Csi18T));
            }
            // .csi_21_t => self.surfaceMessageWriter(.{ .report_title = .csi_21_t }),
            csi::SizeReportStyle::Csi21T => {
                self.surfaceMessageWriter(SurfaceMessage::ReportTitle(ReportTitleStyle::Csi21t));
            }
        }
    }

    // fn kittyColorReport(
    //     self: *StreamHandler,
    //     request: terminal.kitty.color.OSC,
    // ) !void {
    fn kittyColorReport(&mut self, request: kitty::color::OSC) -> Result<(), crate::Error> {
        // var stream: std.Io.Writer.Allocating = .init(self.alloc);
        // defer stream.deinit();
        // const writer = &stream.writer;
        let mut stream: String = String::new();

        // for (request.list.items) |item| {
        for item in &request.list {
            match item {
                // .query => |key| {
                kitty::color::Request::Query(key) => {
                    // If the writer buffer is empty, we need to write our prefix
                    // if (stream.written().len == 0) try writer.writeAll("\x1b]21");
                    if stream.is_empty() {
                        stream.push_str("\x1b]21");
                    }

                    // const color: terminal.color.RGB = switch (key) {
                    let color_val: Option<color::RGB> = match key {
                        // .palette => |palette| self.terminal.colors.palette.current[palette],
                        kitty::color::Kind::Palette(palette) => {
                            Some(self.term().colors.palette.current[*palette as usize])
                        }
                        // .special => |special| switch (special) {
                        kitty::color::Kind::Special(special) => match special {
                            // .foreground => self.terminal.colors.foreground.get(),
                            kitty::color::Special::Foreground => {
                                self.term().colors.foreground.get()
                            }
                            // .background => self.terminal.colors.background.get(),
                            kitty::color::Special::Background => {
                                self.term().colors.background.get()
                            }
                            // .cursor => self.terminal.colors.cursor.get(),
                            kitty::color::Special::Cursor => self.term().colors.cursor.get(),
                            // else => {
                            //     log.warn("ignoring unsupported kitty color protocol key: {f}", .{key});
                            //     continue;
                            // },
                            _ => {
                                // log.warn("ignoring unsupported kitty color protocol key: {f}", .{key});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "ignoring unsupported kitty color protocol key: {key}"
                                );
                                continue;
                            }
                        },
                    };

                    // } orelse {
                    //     try writer.print(";{f}=", .{key});
                    //     continue;
                    // };
                    let color_val = match color_val {
                        Some(c) => c,
                        None => {
                            write!(stream, ";{key}=").map_err(|_| crate::Error)?;
                            continue;
                        }
                    };

                    // try writer.print(
                    //     ";{f}=rgb:{x:0>2}/{x:0>2}/{x:0>2}",
                    //     .{ key, color.r, color.g, color.b },
                    // );
                    write!(
                        stream,
                        ";{}=rgb:{:0>2x}/{:0>2x}/{:0>2x}",
                        key, color_val.r, color_val.g, color_val.b
                    )
                    .map_err(|_| crate::Error)?;
                }
                // .set => |v| switch (v.key) {
                kitty::color::Request::Set {
                    key,
                    color: v_color,
                } => match key {
                    // .palette => |palette| {
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.set(*palette, *v_color);
                    }

                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.set(v.color),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.set(*v_color);
                        }
                        // .background => self.terminal.colors.background.set(v.color),
                        kitty::color::Special::Background => {
                            self.term().colors.background.set(*v_color);
                        }
                        // .cursor => self.terminal.colors.cursor.set(v.color),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.set(*v_color);
                        }
                        // else => {
                        //     log.warn(
                        //         "ignoring unsupported kitty color protocol key: {f}",
                        //         .{v.key},
                        //     );
                        //     continue;
                        // },
                        _ => {
                            // log.warn("ignoring unsupported kitty color protocol key: {f}", .{v.key});
                            log::warn!(
                                target: LOG_SCOPE,
                                "ignoring unsupported kitty color protocol key: {key}"
                            );
                            continue;
                        }
                    },
                },
                // .reset => |key| switch (key) {
                kitty::color::Request::Reset(key) => match key {
                    // .palette => |palette| {
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*palette);
                    }

                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.reset();
                        }
                        // .background => self.terminal.colors.background.reset(),
                        kitty::color::Special::Background => {
                            self.term().colors.background.reset();
                        }
                        // .cursor => self.terminal.colors.cursor.reset(),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.reset();
                        }
                        // else => {
                        //     log.warn(
                        //         "ignoring unsupported kitty color protocol key: {f}",
                        //         .{key},
                        //     );
                        //     continue;
                        // },
                        _ => {
                            // log.warn("ignoring unsupported kitty color protocol key: {f}", .{key});
                            log::warn!(
                                target: LOG_SCOPE,
                                "ignoring unsupported kitty color protocol key: {key}"
                            );
                            continue;
                        }
                    },
                },
            }
        }

        // If we had any writes to our buffer, we queue them now
        // if (stream.written().len > 0) {
        if !stream.is_empty() {
            // try writer.writeAll(request.terminator.string());
            stream.push_str(
                std::str::from_utf8(request.terminator.string()).map_err(|_| crate::Error)?,
            );
            // self.messageWriter(.{
            //     .write_alloc = .{
            //         .alloc = self.alloc,
            //         .data = try stream.toOwnedSlice(),
            //     },
            // });
            self.messageWriter(Message::WriteAlloc(
                rustty_datastruct::message_data::Alloc {
                    data: stream.into_bytes().into_boxed_slice(),
                },
            ));
        }

        // Note: we don't have to do a queueRender here because every
        // processed stream will queue a render once it is done processing
        // the read() syscall.
        Ok(())
    }

    /// Display a GUI progress report.
    // fn progressReport(self: *StreamHandler, report: terminal.osc.Command.ProgressReport) void {
    fn progressReport(&mut self, report: osc::ProgressReport) {
        // self.surfaceMessageWriter(.{ .progress_report = report });
        self.surfaceMessageWriter(SurfaceMessage::ProgressReport(report));
    }
}

// PORT NOTE (stream_terminal precedent): the `vtFallible` switch, distributed
// across the vt `Handler` trait methods — each arm's body below is the
// corresponding `switch (action)` arm, with the reference's `@branchHint`s
// preserved as comments (Rust has no stable branch-hint intrinsic at this
// altitude). The reference's switch preamble:
//
// The branch hints here are based on real world data
// which indicates that the most common actions are:
//
// 1. print
// 2. set_attribute
// 3. carriage_return
// 4. line_feed
// 5. cursor_pos
//
// Together, these 5 actions make up nearly 98% of
// all actions encountered in real world scenarios.
//
// ref: https://github.com/qwerasd205/asciinema-stats
impl<M: AppMailbox> stream::Handler for StreamHandler<M> {
    fn deinit(&mut self) {
        StreamHandler::deinit(self);
    }

    // .print => {
    //     @branchHint(.likely);
    //     try self.terminal.print(value.cp);
    // },
    fn print(&mut self, cp: u32) {
        let r = self.term().print(cp).map_err(|_| crate::Error);
        self.vt("print", r);
    }

    // .print_slice => { @branchHint(.likely); try self.terminal.printSlice(value.cps); },
    //
    // VERIFICATION NOTE (P3-W7 T6, area-E `E10`): the plan's proposed control
    // for this arm was "assert a multi-codepoint run through `print_slice`
    // produces the SAME SCREEN as the same bytes fed one at a time". At THIS
    // level that assertion is **vacuous by construction** and must not be
    // written: `Handler::print_slice`'s trait default is *defined* as fanning
    // the run back out to `print` (see `rustty_vt::stream`), so deleting this
    // override leaves the resulting screen byte-identical — the test would pass
    // either way. The arm's presence is a throughput property, not an
    // observable-behaviour one, which is why upstream ships it with no test.
    //
    // The behavioural equivalence IS pinned, one layer down where it is
    // falsifiable:
    //   * `rustty_vt::terminal::tests::terminal_print_slice_differential_fuzz_vs_print`
    //     — randomized differential, `print` vs `print_slice`, asserted after
    //     EVERY op;
    //   * `rustty_vt::stream::tests::stream_print_slice_matches_per_byte` —
    //     the ground-state dispatch path, driven through `next_slice_capped`
    //     so it is not silently skipped in a non-SIMD build.
    fn print_slice(&mut self, cps: &[u32]) {
        let r = self.term().print_slice(cps).map_err(|_| crate::Error);
        self.vt("print_slice", r);
    }

    // .print_repeat => try self.terminal.printRepeat(value),
    fn print_repeat(&mut self, count: usize) {
        let r = self.term().print_repeat(count).map_err(|_| crate::Error);
        self.vt("print_repeat", r);
    }

    // .bell => self.bell(),
    fn bell(&mut self) {
        StreamHandler::bell(self);
    }

    // .backspace => self.terminal.backspace(),
    fn backspace(&mut self) {
        self.term().backspace();
    }

    // .horizontal_tab => self.horizontalTab(value),
    fn horizontal_tab(&mut self, count: u16) {
        self.horizontalTab(count);
    }

    // .horizontal_tab_back => self.horizontalTabBack(value),
    fn horizontal_tab_back(&mut self, count: u16) {
        self.horizontalTabBack(count);
    }

    // .linefeed => {
    //     @branchHint(.likely);
    //     try self.linefeed();
    // },
    fn linefeed(&mut self) {
        let r = StreamHandler::linefeed(self);
        self.vt("linefeed", r);
    }

    // .carriage_return => {
    //     @branchHint(.likely);
    //     self.terminal.carriageReturn();
    // },
    fn carriage_return(&mut self) {
        self.term().carriage_return();
    }

    // .enquiry => try self.enquiry(),
    fn enquiry(&mut self) {
        let r = StreamHandler::enquiry(self);
        self.vt("enquiry", r);
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

    // .cursor_pos => {
    //     @branchHint(.likely);
    //     self.terminal.setCursorPos(value.row, value.col);
    // },
    fn cursor_pos(&mut self, v: stream::CursorPos) {
        self.term().set_cursor_pos(v.row as usize, v.col as usize);
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
        let y = self.term().active_mut().cursor.y;
        let x = self.term().active_mut().cursor.x;
        self.term()
            .set_cursor_pos(y as usize + 1, (x + 1).saturating_add(v.value) as usize);
    }

    // .cursor_row_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1 +| value.value,
    //     self.terminal.screens.active.cursor.x + 1,
    // ),
    fn cursor_row_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y;
        let x = self.term().active_mut().cursor.x;
        self.term()
            .set_cursor_pos((y + 1).saturating_add(v.value) as usize, x as usize + 1);
    }

    // .cursor_style => self.terminal.setCursorStyle(value),
    fn cursor_style(&mut self, v: ansi::CursorStyle) {
        self.term().set_cursor_style(v);
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

    // .erase_display_complete => {
    //     self.terminal.scrollViewport(.{ .bottom = {} });
    //     self.terminal.eraseDisplay(.complete, value);
    // },
    fn erase_display_complete(&mut self, protected: bool) {
        self.term()
            .scroll_viewport(rustty_vt::terminal::ScrollViewport::Bottom);
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
        let r = self.term().scroll_up(count).map_err(|_| crate::Error);
        self.vt("scroll_up", r);
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

    // .index => try self.index(),
    fn index(&mut self) {
        let r = StreamHandler::index(self);
        self.vt("index", r);
    }

    // .next_line => try self.nextLine(),
    fn next_line(&mut self) {
        let r = self.nextLine();
        self.vt("next_line", r);
    }

    // .reverse_index => try self.reverseIndex(),
    fn reverse_index(&mut self) {
        let r = self.reverseIndex();
        self.vt("reverse_index", r);
    }

    // .full_reset => try self.fullReset(),
    fn full_reset(&mut self) {
        let r = self.fullReset();
        self.vt("full_reset", r);
    }

    // .set_mode => try self.setMode(value.mode, true),
    fn set_mode(&mut self, v: stream::Mode) {
        let r = self.setMode(v.mode, true);
        self.vt("set_mode", r);
    }

    // .reset_mode => try self.setMode(value.mode, false),
    fn reset_mode(&mut self, v: stream::Mode) {
        let r = self.setMode(v.mode, false);
        self.vt("reset_mode", r);
    }

    // .save_mode => self.terminal.modes.save(value.mode),
    fn save_mode(&mut self, v: stream::Mode) {
        self.term().modes.save(v.mode);
    }

    // .restore_mode => {
    //     // For restore mode we have to restore but if we set it, we
    //     // always have to call setMode because setting some modes have
    //     // side effects and we want to make sure we process those.
    //     const v = self.terminal.modes.restore(value.mode);
    //     try self.setMode(value.mode, v);
    // },
    fn restore_mode(&mut self, v: stream::Mode) {
        // For restore mode we have to restore but if we set it, we
        // always have to call setMode because setting some modes have
        // side effects and we want to make sure we process those.
        let restored = self.term().modes.restore(v.mode);
        let r = self.setMode(v.mode, restored);
        self.vt("restore_mode", r);
    }

    // .request_mode => try self.requestMode(value.mode),
    fn request_mode(&mut self, v: stream::Mode) {
        let r = self.requestMode(v.mode);
        self.vt("request_mode", r);
    }

    // .request_mode_unknown => try self.requestModeUnknown(value.mode, value.ansi),
    fn request_mode_unknown(&mut self, v: stream::RawMode) {
        let r = self.requestModeUnknown(v.mode, v.ansi);
        self.vt("request_mode_unknown", r);
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

    // .save_cursor => try self.saveCursor(),
    fn save_cursor(&mut self) {
        let r = self.saveCursor();
        self.vt("save_cursor", r);
    }

    // .restore_cursor => try self.restoreCursor(),
    fn restore_cursor(&mut self) {
        let r = self.restoreCursor();
        self.vt("restore_cursor", r);
    }

    // .modify_key_format => try self.setModifyKeyFormat(value),
    fn modify_key_format(&mut self, v: ansi::ModifyKeyFormat) {
        let r = self.setModifyKeyFormat(v);
        self.vt("modify_key_format", r);
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

    // .mouse_shift_capture => self.terminal.flags.mouse_shift_capture = if (value) .true else .false,
    fn mouse_shift_capture(&mut self, capture: bool) {
        self.term().flags.mouse_shift_capture = if capture {
            rustty_vt::terminal::MouseShiftCapture::True
        } else {
            rustty_vt::terminal::MouseShiftCapture::False
        };
    }

    // .size_report => self.sendSizeReport(value),
    fn size_report(&mut self, v: csi::SizeReportStyle) {
        self.sendSizeReport(v);
    }

    // .xtversion => try self.reportXtversion(),
    fn xtversion(&mut self) {
        let r = self.reportXtversion();
        self.vt("xtversion", r);
    }

    // .device_attributes => try self.deviceAttributes(value),
    fn device_attributes(&mut self, req: device_attributes::Req) {
        let r = self.deviceAttributes(req);
        self.vt("device_attributes", r);
    }

    // .device_status => try self.deviceStatusReport(value.request),
    fn device_status(&mut self, v: stream::DeviceStatus) {
        let r = self.deviceStatusReport(v.request);
        self.vt("device_status", r);
    }

    // .kitty_keyboard_query => try self.queryKittyKeyboard(),
    fn kitty_keyboard_query(&mut self) {
        let r = self.queryKittyKeyboard();
        self.vt("kitty_keyboard_query", r);
    }

    // .kitty_keyboard_push => {
    //     log.debug("pushing kitty keyboard mode: {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.push(value.flags);
    // },
    fn kitty_keyboard_push(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("pushing kitty keyboard mode: {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "pushing kitty keyboard mode:{:?}", v.flags);
        self.term().active_mut().kitty_keyboard.push(v.flags);
    }

    // .kitty_keyboard_pop => {
    //     log.debug("popping kitty keyboard mode n={}", .{value});
    //     self.terminal.screens.active.kitty_keyboard.pop(@intCast(value));
    // },
    fn kitty_keyboard_pop(&mut self, v: u16) {
        // log.debug("popping kitty keyboard mode n={}", .{value});
        log::debug!(target: LOG_SCOPE, "popping kitty keyboard mode n={v}");
        self.term().active_mut().kitty_keyboard.pop(v as usize);
    }

    // .kitty_keyboard_set => {
    //     log.debug("setting kitty keyboard mode: set {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.set(.set, value.flags);
    // },
    fn kitty_keyboard_set(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("setting kitty keyboard mode: set {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "setting kitty keyboard mode: set{:?}", v.flags);
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Set, v.flags);
    }

    // .kitty_keyboard_set_or => {
    //     log.debug("setting kitty keyboard mode: or {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.set(.@"or", value.flags);
    // },
    fn kitty_keyboard_set_or(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("setting kitty keyboard mode: or {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "setting kitty keyboard mode: or{:?}", v.flags);
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Or, v.flags);
    }

    // .kitty_keyboard_set_not => {
    //     log.debug("setting kitty keyboard mode: not {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.set(.not, value.flags);
    // },
    fn kitty_keyboard_set_not(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("setting kitty keyboard mode: not {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "setting kitty keyboard mode: not{:?}", v.flags);
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Not, v.flags);
    }

    // .kitty_color_report => try self.kittyColorReport(value),
    fn kitty_color_report(&mut self, v: kitty::color::OSC) {
        let r = self.kittyColorReport(v);
        self.vt("kitty_color_report", r);
    }

    // .color_operation => try self.colorOperation(value.op, &value.requests, value.terminator),
    fn color_operation(&mut self, v: stream::ColorOperation) {
        let r = self.colorOperation(v.op, &v.requests, v.terminator);
        self.vt("color_operation", r);
    }

    // .end_hyperlink => try self.endHyperlink(),
    fn end_hyperlink(&mut self) {
        let r = self.endHyperlink();
        self.vt("end_hyperlink", r);
    }

    // .active_status_display => self.terminal.status_display = value,
    fn active_status_display(&mut self, v: ansi::StatusDisplay) {
        self.term().status_display = v;
    }

    // .decaln => try self.decaln(),
    fn decaln(&mut self) {
        let r = StreamHandler::decaln(self);
        self.vt("decaln", r);
    }

    // .window_title => try self.windowTitle(value.title),
    fn window_title(&mut self, v: stream::WindowTitle) {
        let r = self.windowTitle(v.title);
        self.vt("window_title", r);
    }

    // .report_pwd => try self.reportPwd(value.url),
    fn report_pwd(&mut self, v: stream::ReportPwd) {
        let r = self.reportPwd(v.url);
        self.vt("report_pwd", r);
    }

    // .show_desktop_notification => try self.showDesktopNotification(value.title, value.body),
    fn show_desktop_notification(&mut self, v: stream::ShowDesktopNotification) {
        let r = self.showDesktopNotification(v.title, v.body);
        self.vt("show_desktop_notification", r);
    }

    // .progress_report => self.progressReport(value),
    fn progress_report(&mut self, v: osc::ProgressReport) {
        self.progressReport(v);
    }

    // .start_hyperlink => try self.startHyperlink(value.uri, value.id),
    fn start_hyperlink(&mut self, v: stream::StartHyperlink) {
        let r = self.startHyperlink(v.uri, v.id);
        self.vt("start_hyperlink", r);
    }

    // .clipboard_contents => try self.clipboardContents(value.kind, value.data),
    fn clipboard_contents(&mut self, v: stream::ClipboardContents) {
        let r = self.clipboardContents(v.kind, v.data);
        self.vt("clipboard_contents", r);
    }

    // .semantic_prompt => try self.semanticPrompt(value),
    fn semantic_prompt(&mut self, v: stream::SemanticPrompt) {
        let r = self.semanticPrompt(v);
        self.vt("semantic_prompt", r);
    }

    // .mouse_shape => try self.setMouseShape(value),
    fn mouse_shape(&mut self, v: rustty_vt::mouse::Shape) {
        let r = self.setMouseShape(v);
        self.vt("mouse_shape", r);
    }

    // .configure_charset => self.configureCharset(value.slot, value.charset),
    fn configure_charset(&mut self, v: stream::ConfigureCharset) {
        self.configureCharset(v.slot, v.charset);
    }

    // .set_attribute => {
    //     @branchHint(.likely);
    //     switch (value) {
    //         .unknown => |unk| {
    //             // We optimize for the happy path scenario here, since
    //             // unknown/invalid SGRs aren't that common in the wild.
    //             @branchHint(.unlikely);
    //             log.warn("unimplemented or unknown SGR attribute: {any}", .{unk});
    //         },
    //         else => {
    //             @branchHint(.likely);
    //             self.terminal.setAttribute(value) catch |err| {
    //                 @branchHint(.cold);
    //                 log.warn("error setting attribute {}: {}", .{ value, err });
    //             };
    //         },
    //     }
    // },
    fn set_attribute(&mut self, attr: rustty_vt::sgr::Attribute) {
        match attr {
            rustty_vt::sgr::Attribute::Unknown(unk) => {
                // We optimize for the happy path scenario here, since
                // unknown/invalid SGRs aren't that common in the wild.
                // log.warn("unimplemented or unknown SGR attribute: {any}", .{unk});
                log::warn!(
                    target: LOG_SCOPE,
                    "unimplemented or unknown SGR attribute: {unk:?}"
                );
            }
            _ => {
                if let Err(err) = self.term().set_attribute(attr) {
                    // log.warn("error setting attribute {}: {}", .{ value, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error setting attribute {attr:?}: {err:?}"
                    );
                }
            }
        }
    }

    // .dcs_hook => try self.dcsHook(value),
    fn dcs_hook(&mut self, dcs: rustty_vt::parser::DCS) {
        let r = self.dcsHook(dcs);
        self.vt("dcs_hook", r);
    }

    // .dcs_put => try self.dcsPut(value),
    fn dcs_put(&mut self, byte: u8) {
        let r = self.dcsPut(byte);
        self.vt("dcs_put", r);
    }

    // .dcs_unhook => try self.dcsUnhook(),
    fn dcs_unhook(&mut self) {
        let r = self.dcsUnhook();
        self.vt("dcs_unhook", r);
    }

    // .apc_start => self.apc.start(),
    fn apc_start(&mut self) {
        self.apc.start();
    }

    // .apc_end => try self.apcEnd(),
    fn apc_end(&mut self, _v: rustty_vt::stream::ApcEnd) {
        let r = self.apcEnd();
        self.vt("apc_end", r);
    }

    // .apc_put => self.apc.feed(self.alloc, value),
    fn apc_put(&mut self, byte: u8) {
        let alloc = self.alloc;
        self.apc.feed(alloc, byte);
    }

    // .apc_put_slice => self.apc.feedSlice(self.alloc, value.bytes),
    fn apc_put_slice(&mut self, bytes: &[u8]) {
        let alloc = self.alloc;
        self.apc.feed_slice(alloc, bytes);
    }

    // // Unimplemented
    // .title_push,
    // .title_pop,
    // => {},
    fn title_push(&mut self, _v: u16) {}
    fn title_pop(&mut self, _v: u16) {}

    // .kitty_clipboard => try self.kittyClipboard(value),
    fn kitty_clipboard(&mut self, v: osc::KittyClipboardProtocol) {
        let r = StreamHandler::kittyClipboard(self, &v);
        self.vt("kitty_clipboard", r);
    }

    // Unimplemented
    // .title_push,
    // .title_pop,
    // .kitty_dnd,
    // => {},
    // (af8d28a94, assessment E row P5-E11: the dnd Action is added next to the
    // title_push/title_pop no-ops.)
    fn kitty_dnd(&mut self, _v: osc::KittyDndProtocol) {}
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims (Rust-only glue).
// ---------------------------------------------------------------------------

/// PORT NOTE: `comptime terminfo.ghostty.xtgettcapMap()` — the comptime-static
/// XTGETTCAP response map, realized as a process-lifetime `OnceLock` (the
/// entries' `'static` lifetime matches the reference's comptime data, which
/// the `write_stable` message contract requires).
fn xtgettcapMap() -> &'static std::collections::HashMap<Vec<u8>, Vec<u8>> {
    use std::sync::OnceLock;
    static MAP: OnceLock<std::collections::HashMap<Vec<u8>, Vec<u8>>> = OnceLock::new();
    MAP.get_or_init(|| rustty_terminfo::rustty().xtgettcap_map())
}

/// PORT NOTE (Zig-std shim): `std.Uri.host_name_max` (the fixed host buffer size).
const URI_HOST_NAME_MAX: usize = 255;

/// PORT NOTE (Zig-std shim): the error set of `std.Uri.getHost`.
#[derive(Debug)]
enum GetHostError {
    UriMissingHost,
    UriHostTooLong,
}

/// PORT NOTE (Zig-std shim): `std.Uri.getHost` — the percent-decoded host component
/// written into the caller's buffer.
fn uri_get_host<'b>(
    uri: &rustty_os::uri::Uri<'b>,
    buffer: &'b mut [u8],
) -> Result<&'b [u8], GetHostError> {
    let host = uri.host.as_ref().ok_or(GetHostError::UriMissingHost)?;
    host.toRaw(buffer).map_err(|_| GetHostError::UriHostTooLong)
}

/// PORT NOTE (Zig-std shim): a fixed-buffer `std.Io.Writer` — formats into the
/// buffer and reports the written length; overflow is `NoSpaceLeft` (mapped to
/// the crate's opaque error at the call sites).
fn write_fixed(buf: &mut [u8], args: core::fmt::Arguments<'_>) -> Result<usize, crate::Error> {
    let mut w = FixedFmt { buf, pos: 0 };
    w.write_fmt(args).map_err(|_| crate::Error)?;
    Ok(w.pos)
}

/// The state of [`write_fixed`] (also used directly where the reference keeps
/// a running fixed writer).
struct FixedFmt<'b> {
    buf: &'b mut [u8],
    pos: usize,
}

impl core::fmt::Write for FixedFmt<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if self.pos + bytes.len() > self.buf.len() {
            // (NoSpaceLeft)
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

/// PORT NOTE (Zig-std shim): a `core::fmt::Write` adapter over a byte `Vec` (the
/// reference's fixed-buffer-allocator-backed ArrayList writer).
struct VecFmt<'b>(&'b mut Vec<u8>);

impl core::fmt::Write for VecFmt<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// PORT NOTE (Zig-std shim): the rustty-os `string_encoding::Writer` adapter over a
/// byte `Vec` (for `uri.path.toRawMaybeAlloc`'s decode target).
struct VecWriter(Vec<u8>);

impl rustty_os::string_encoding::Writer for VecWriter {
    type Error = core::convert::Infallible;

    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }

    fn writeByte(&mut self, byte: u8) -> Result<(), Self::Error> {
        self.0.push(byte);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // A no-op `AppMailbox` double: the mode-12 guard returns before any mailbox
    // is touched, so accepting every push is enough.
    struct NullMailbox;

    impl AppMailbox for NullMailbox {
        type Surface = ();

        #[allow(clippy::result_large_err)]
        fn push_or_return(
            &self,
            _msg: rustty_apprt_api::surface::Message<'static>,
            _timeout: Timeout,
        ) -> Result<
            rustty_datastruct::blocking_queue::Size,
            rustty_apprt_api::surface::Message<'static>,
        > {
            Ok(1)
        }
    }

    fn terminal_opts(default_cursor_blink: Option<bool>) -> rustty_vt::terminal::Options {
        rustty_vt::terminal::Options {
            cols: 10,
            rows: 3,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            default_modes: rustty_vt::modes::ModePacked::default(),
            default_cursor_style: rustty_vt::screen::CursorStyle::Block,
            default_cursor_blink,
            colors: rustty_vt::terminal::Colors {
                background: color::DynamicRGB::UNSET,
                foreground: color::DynamicRGB::UNSET,
                cursor: color::DynamicRGB::UNSET,
                palette: color::DynamicPalette::default_palette(),
            },
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
        }
    }

    // A minimal StreamHandler over a caller-owned Terminal. Every field the
    // mode-12 guard does not read is null / empty: the guard returns before the
    // renderer state, mailboxes or size are touched.
    fn handler_for(
        terminal: *mut Terminal,
        termio_mailbox: *mut Mailbox,
        renderer_state: *mut State,
    ) -> StreamHandler<NullMailbox> {
        StreamHandler::<NullMailbox> {
            alloc: Global,
            size: std::ptr::null_mut(),
            terminal,
            termio_mailbox,
            surface_mailbox: NullMailbox,
            renderer_state,
            renderer_mailbox: std::ptr::null_mut(),
            renderer_wakeup: xev::Async::init().expect("Async::init"),
            enquiry_response: Vec::new(),
            osc_color_report_format: OSCColorReportFormat::Bit16,
            clipboard_write: ClipboardAccess::Deny,
            clipboard_write_limit: rustty_vt::kitty::clipboard::max_write_size,
            kitty_clipboard_grants: rustty_vt::kitty::clipboard::Grants::default(),
            kitty_clipboard_write: None,
            apc: rustty_vt::apc::Handler::default(),
            dcs: rustty_vt::dcs::Handler::default(),
            tmux_viewer: None,
            termio_messaged: false,
            seen_title: false,
        }
    }

    // NEGATIVE CONTROL (P3-W4 T4 Step 4): `c594031d5` MOVES the field that this
    // rule reads (`default_cursor_blink`) from the handler onto `Terminal`. A
    // mechanical port that re-points it at the wrong thing — or hard-codes it —
    // silently breaks the "the user configured an explicit blink, so ignore DEC
    // mode 12" rule, and no existing KAT covers it.
    #[test]
    fn stream_handler_mode_12_ignored_when_default_blink_is_explicit() {
        // (a) An EXPLICIT configured blink: DEC mode 12 must be ignored.
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        assert!(!term.modes.get(modes::Mode::CursorBlinking));
        let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
        h.setMode(modes::Mode::CursorBlinking, true).unwrap();
        assert!(
            !term.modes.get(modes::Mode::CursorBlinking),
            "DEC mode 12 must be ignored while an explicit default blink is set"
        );
        h.renderer_wakeup.deinit();
        term.deinit(Global);

        // (b) NO configured blink (null = emulator default): mode 12 applies.
        let mut term = Terminal::init(Global, terminal_opts(None)).unwrap();
        // A null default resolves to the emulator default, which is blinking.
        assert!(term.modes.get(modes::Mode::CursorBlinking));
        let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
        h.setMode(modes::Mode::CursorBlinking, false).unwrap();
        assert!(
            !term.modes.get(modes::Mode::CursorBlinking),
            "DEC mode 12 must apply when no explicit default blink is configured"
        );
        h.renderer_wakeup.deinit();
        term.deinit(Global);
    }

    // ---- task-14e-1: the restored `io_handler` diagnostics -----------------

    /// `stream_handler.zig:186` — the `vt`/`vtFallible` funnel. This is the
    /// highest-risk shape in the file: the port collapses the reference's
    /// COMPTIME `switch (action)` into one trait method per action, so the
    /// `action=` half of the message is not a value at the site and had been
    /// DROPPED entirely. Each of the 38 callers now passes its own reference
    /// `Action.Tag` field name; this pins the rendered form for two of them.
    #[test]
    fn vt_funnel_emits_the_action_name_and_the_error() {
        let cap = crate::log_capture::start();
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());

        h.vt("print", Err(crate::Error));
        h.vt("kitty_keyboard_query", Err(crate::Error));
        // The SUCCESS arm must stay silent — the reference only logs in `catch`.
        h.vt("print", Ok(()));

        let messages = cap.messages();
        drop(cap);
        h.renderer_wakeup.deinit();
        term.deinit(Global);

        assert_eq!(
            vec![
                (
                    log::Level::Warn,
                    "io_handler".to_string(),
                    "error handling VT action action=print err=Error".to_string(),
                ),
                (
                    log::Level::Warn,
                    "io_handler".to_string(),
                    "error handling VT action action=kitty_keyboard_query err=Error".to_string(),
                ),
            ],
            messages
        );
    }

    /// `stream_handler.zig:946` — the over-long window title warn. Drives a real
    /// call path rather than the funnel, and pins the exact record vector.
    #[test]
    fn window_title_warns_when_the_title_exceeds_the_buffer() {
        let cap = crate::log_capture::start();
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());

        // The reference guards on `title.len >= buf.len` with `buf: [256]u8`.
        let too_long = vec![b'x'; 256];
        h.windowTitle(&too_long).unwrap();

        let messages = cap.messages();
        drop(cap);
        h.renderer_wakeup.deinit();
        term.deinit(Global);

        assert_eq!(
            vec![(
                log::Level::Warn,
                "io_handler".to_string(),
                "change title requested larger than our buffer size, ignoring".to_string(),
            )],
            messages
        );
    }

    /// The ABSENCE arm of the test above, with the positive assertion
    /// PROCEDURE.md requires: a title that FITS must produce NO record, and the
    /// test must prove it actually reached the code the absence is about rather
    /// than bailing out somewhere earlier.
    ///
    /// The positive proof is the terminal's title itself. `windowTitle` sets it
    /// on the way past the guard, so a set title is evidence the guard was
    /// evaluated and NOT taken — which an assertion on the empty record vector
    /// alone cannot distinguish from "the function returned before the guard".
    ///
    /// NEGATIVE CONTROL RUN (task-14e-1): moving the `log::warn!` out of the
    /// `if title.len() >= buf.len()` block — the exact "diagnostic on a success
    /// path" bug this project shipped once — fails THIS test with
    /// `left: [] right: [(Warn, "io_handler", "change title requested ...")]`,
    /// and leaves `window_title_warns_when_the_title_exceeds_the_buffer`
    /// PASSING. So the control fails here, and it fails for the stated reason.
    #[test]
    fn window_title_is_silent_when_the_title_fits() {
        let cap = crate::log_capture::start();
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());

        h.windowTitle(b"a short title").unwrap();

        let messages = cap.messages();
        // POSITIVE: we got past the length guard and through `set_title`.
        let title_after = term.get_title().map(<[u8]>::to_vec);
        drop(cap);
        h.renderer_wakeup.deinit();
        term.deinit(Global);

        assert_eq!(
            Some(b"a short title".to_vec()),
            title_after,
            "the title was not set, so this test never reached the guard whose \
             silence it is asserting"
        );
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    // Pops one message from the SPSC termio mailbox (the queue the handler's
    // `messageWriter` pushes into).
    fn mailbox_pop(mailbox: *mut Mailbox) -> Option<Message<'static>> {
        match unsafe { &*mailbox } {
            Mailbox::Spsc { queue, .. } => queue.pop(),
        }
    }

    // NEGATIVE CONTROL (P3-W4 T7 Step 5), termio half. `6c8c07981` adds TWO
    // independently droppable termio arms: the mode-2033 ENABLE arm and the
    // CSI ? 998 n DSR arm. Both must push a FORCED `VisibilityReport` carrying
    // the terminal's current `flags.visible`. Dropping either is invisible to
    // every existing termio KAT.
    #[test]
    fn stream_handler_visibility_report_arms_push_forced_messages() {
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        term.flags.visible = false;

        let mailbox: *mut Mailbox = Box::into_raw(Box::new(
            crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
        ));
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        let state: *mut State = Box::into_raw(Box::new(State {
            mutex,
            terminal: &raw mut term,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: rustty_renderer::state::Mouse::default(),
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        }));
        let mut h = handler_for(&raw mut term, mailbox, state);

        // (a) Enabling mode 2033 pushes a forced report with the current state.
        h.setMode(modes::Mode::ReportVisibility, true).unwrap();
        match mailbox_pop(mailbox) {
            Some(Message::VisibilityReport { visible, force }) => {
                assert!(!visible, "the report must carry terminal.flags.visible");
                assert!(force, "a mode-2033 enable is a FORCED report");
            }
            other => panic!("expected a VisibilityReport, got {:?}", other.is_some()),
        }

        // (b) DISABLING pushes nothing.
        h.setMode(modes::Mode::ReportVisibility, false).unwrap();
        assert!(
            mailbox_pop(mailbox).is_none(),
            "disabling mode 2033 must not push a report"
        );

        // (c) An explicit CSI ? 998 n query pushes a forced report too.
        term.flags.visible = true;
        h.deviceStatusReport(rustty_vt::device_status::Request::Visibility)
            .unwrap();
        match mailbox_pop(mailbox) {
            Some(Message::VisibilityReport { visible, force }) => {
                assert!(visible);
                assert!(force, "an explicit query is answered unconditionally");
            }
            other => panic!("expected a VisibilityReport, got {:?}", other.is_some()),
        }

        h.renderer_wakeup.deinit();
        unsafe {
            drop(Box::from_raw(state));
            drop(Box::from_raw(mutex));
            drop(Box::from_raw(mailbox));
        }
        term.deinit(Global);
    }

    // The `changeConfig` half of the same move: the handler no longer owns the
    // defaults, so a config change must reach `Terminal`'s setters (and must
    // apply immediately only while the cursor still follows its defaults).
    #[test]
    fn stream_handler_change_config_routes_cursor_defaults_to_the_terminal() {
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        // `changeConfig` ends with a `messageWriter` (the mode-2031 color-scheme
        // report), so this handler needs a real termio mailbox.
        let mailbox: *mut Mailbox = Box::into_raw(Box::new(
            crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
        ));
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        let state: *mut State = Box::into_raw(Box::new(State {
            mutex,
            terminal: &raw mut term,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: rustty_renderer::state::Mouse::default(),
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        }));
        let mut h = handler_for(&raw mut term, mailbox, state);

        let mut cfg = DerivedConfig::init(&rustty_config::Config::default());
        cfg.cursor_style = rustty_vt::screen::CursorStyle::Underline;
        cfg.cursor_blink = Some(true);
        h.changeConfig(&mut cfg);

        assert_eq!(
            rustty_vt::screen::CursorStyle::Underline,
            term.cursor.default_style
        );
        assert_eq!(Some(true), term.cursor.default_blink);
        assert!(term.cursor.is_default);
        assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
            (*term.screens.active).cursor.cursor_style
        });
        assert!(term.modes.get(modes::Mode::CursorBlinking));

        h.renderer_wakeup.deinit();
        unsafe {
            drop(Box::from_raw(state));
            drop(Box::from_raw(mutex));
            drop(Box::from_raw(mailbox));
        }
        term.deinit(Global);
    }

    // The bytes of a `write_small`/`write_alloc` termio message.
    fn write_bytes(msg: &Message<'static>) -> Vec<u8> {
        match msg {
            Message::WriteSmall(v) => v.data[0..v.len].to_vec(),
            Message::WriteStable(v) => v.to_vec(),
            Message::WriteAlloc(v) => v.data.to_vec(),
            _ => panic!("expected a write message"),
        }
    }

    // `dcsCommand`'s tmux control-mode arm (`fix-freetype-tmux` lane). This arm
    // was `Command::Tmux(_) => {}` — a silent no-op — while `TMUX_ENABLED` is
    // TRUE in the shipped binary, so entering tmux control mode built no viewer,
    // sent tmux no commands, and never tore the viewer down.
    //
    // Drives the reference's three prongs end to end (`stream_handler.zig:384`):
    //   `.enter` -> allocate the viewer
    //   fallthrough -> `viewer.next(.{ .tmux = tmux })`, and every `.command`
    //                  action goes out the termio mailbox via `messageWriter`
    //   `.exit`  -> tear the viewer down
    //
    // Runs in BOTH build configurations, asserting the reference's behaviour in
    // each: with `tmux_control_mode` OFF the whole block is `break :tmux` — a
    // no-op — and with it ON (the shipped binary; `cargo test -p rustty-termio
    // --features oniguruma` here) the three prongs above must all fire.
    #[test]
    fn stream_handler_dcs_tmux_arm_drives_the_viewer() {
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        let mailbox: *mut Mailbox = Box::into_raw(Box::new(
            crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
        ));
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        let state: *mut State = Box::into_raw(Box::new(State {
            mutex,
            terminal: &raw mut term,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: rustty_renderer::state::Mouse::default(),
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        }));
        let mut h = handler_for(&raw mut term, mailbox, state);

        let feed = |h: &mut StreamHandler<NullMailbox>, n: rustty_vt::tmux::ControlNotification| {
            let mut cmd: rustty_vt::dcs::Command = rustty_vt::dcs::Command::Tmux(n);
            h.dcsCommand(&mut cmd).expect("dcsCommand");
        };

        // (a) `.enter` allocates the viewer.
        assert!(h.tmux_viewer.is_none());
        feed(&mut h, rustty_vt::tmux::ControlNotification::Enter);

        if !TMUX_ENABLED {
            // `if (comptime !tmux_enabled) break :tmux;` — the entire block is
            // inert in a build without the option, so nothing was allocated.
            assert!(
                h.tmux_viewer.is_none(),
                "with tmux_control_mode off the arm must be a no-op"
            );
            assert!(mailbox_pop(mailbox).is_none());

            h.deinit();
            h.renderer_wakeup.deinit();
            unsafe {
                drop(Box::from_raw(state));
                drop(Box::from_raw(mutex));
                drop(Box::from_raw(mailbox));
            }
            term.deinit(Global);
            return;
        }

        assert!(
            h.tmux_viewer.is_some(),
            "the .enter prong must build the tmux viewer"
        );
        assert!(
            mailbox_pop(mailbox).is_none(),
            ".enter emits no tmux command"
        );

        // (b) The fallthrough prong routes the notification into the viewer.
        // A block end moves the viewer startup_block -> startup_session and
        // yields no action.
        feed(
            &mut h,
            rustty_vt::tmux::ControlNotification::BlockEnd(Vec::new()),
        );
        assert!(
            mailbox_pop(mailbox).is_none(),
            "the startup block end yields no action"
        );

        // ...and the session-changed notification makes the viewer enter its
        // command queue, whose first command MUST reach the termio mailbox.
        feed(
            &mut h,
            rustty_vt::tmux::ControlNotification::SessionChanged {
                id: 1,
                name: b"rustty".to_vec(),
            },
        );
        let msg = mailbox_pop(mailbox)
            .expect("the viewer's first queued command must be written to tmux");
        let bytes = write_bytes(&msg);
        assert!(!bytes.is_empty());
        assert_eq!(
            b'\n',
            bytes[bytes.len() - 1],
            "the reference asserts the command carries its trailing newline"
        );
        assert!(
            h.termio_messaged,
            "messageWriter must mark the termio thread as needing a wakeup"
        );

        // (c) `.exit` tears the viewer down.
        feed(&mut h, rustty_vt::tmux::ControlNotification::Exit);
        assert!(
            h.tmux_viewer.is_none(),
            "the .exit prong must free the viewer"
        );

        h.deinit();
        h.renderer_wakeup.deinit();
        unsafe {
            drop(Box::from_raw(state));
            drop(Box::from_raw(mutex));
            drop(Box::from_raw(mailbox));
        }
        term.deinit(Global);
    }

    // test "kitty clipboard read: targets-only never consumes a one-time grant"
    #[test]
    fn kitty_clipboard_read_targets_only_never_consumes_a_one_time_grant() {
        // var handler: StreamHandler = undefined;
        // handler.alloc = testing.allocator;
        // handler.kitty_clipboard_grants = .{};
        //
        // PORT NOTE: Rust cannot mirror `undefined`; the minimal constructed
        // handler double (`handler_for`, the module's established seam) stands
        // in — only `kitty_clipboard_grants` is read by the code under test.
        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();
        let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
        h.kitty_clipboard_grants
            .grant(b"otp", rustty_vt::kitty::clipboard::Direction::Read, true);

        // A listing request must not burn the one-time paste password...
        assert!(!h.kittyClipboardReadGranted(b"otp", 0));
        // ...so the follow-up data read is still granted, exactly once.
        assert!(h.kittyClipboardReadGranted(b"otp", 1));
        assert!(!h.kittyClipboardReadGranted(b"otp", 1));

        h.renderer_wakeup.deinit();
        term.deinit(Global);
    }

    // test "kitty clipboard write: oversized text replies EFBIG"
    #[test]
    fn kitty_clipboard_write_oversized_text_replies_efbig() {
        use rustty_vt::kitty::clipboard as kitty_clipboard;
        use rustty_vt::osc::parsers::kitty_clipboard_protocol::Operation;

        let mut term = Terminal::init(Global, terminal_opts(Some(false))).unwrap();

        // var mailbox = try termio.Mailbox.initSPSC(testing.allocator);
        let mailbox: *mut Mailbox = Box::into_raw(Box::new(
            crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
        ));
        // var mutex: std.Io.Mutex = .init; mutex.lockUncancelable(global.io());
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        // var renderer_state: renderer.State = .{ .mutex = &mutex, .terminal = undefined };
        let state: *mut State = Box::into_raw(Box::new(State {
            mutex,
            terminal: &raw mut term,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: rustty_renderer::state::Mouse::default(),
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        }));

        // var handler: StreamHandler = undefined; (minimal constructed double)
        let mut h = handler_for(&raw mut term, mailbox, state);
        h.clipboard_write = ClipboardAccess::Allow;
        h.clipboard_write_limit = 4;
        h.kitty_clipboard_write = None;

        // const begin: Metadata = .{ .op = .write, .id = "macos" };
        let mut begin = kitty_clipboard::Metadata::new(Operation::Write);
        begin.id = b"macos".to_vec();
        h.kittyClipboardWriteBegin(&begin, rustty_vt::osc::Terminator::St)
            .unwrap();

        // const state = handler.kitty_clipboard_write.?;
        // try state.data(..., "SGVsbA==") // "Hell"
        // try testing.expectError(error.TooLarge, state.data(..., "bw==")) // "o"
        let mut wdata = kitty_clipboard::Metadata::new(Operation::Wdata);
        wdata.mime = b"text/plain".to_vec();
        {
            let write_state = h.kitty_clipboard_write.as_mut().unwrap();
            write_state.data(&wdata, b"SGVsbA==").unwrap();
            assert_eq!(
                Err(kitty_clipboard::DataError::TooLarge),
                write_state.data(&wdata, b"bw==")
            );
        }
        // try handler.kittyClipboardWriteFinish(state, .EFBIG, .st);
        let write_state = h.kitty_clipboard_write.take().unwrap();
        h.kittyClipboardWriteFinish(
            &write_state,
            kitty_clipboard::Status::Efbig,
            rustty_vt::osc::Terminator::St,
        )
        .unwrap();
        drop(write_state);
        assert!(h.kitty_clipboard_write.is_none());

        // const response = mailbox.spsc.queue.pop(global.io());
        let msg = mailbox_pop(mailbox).expect("expected the EFBIG status reply");
        match msg {
            Message::WriteAlloc(v) => assert_eq!(
                b"\x1B]5522;type=write:status=EFBIG:id=macos\x1B\\".as_slice(),
                &*v.data,
            ),
            _ => panic!("expected a write_alloc message"),
        }

        // Teardown leaves no transaction that could be committed and
        // forwarded to the macOS clipboard path.
        assert!(mailbox_pop(mailbox).is_none());

        h.deinit();
        h.renderer_wakeup.deinit();
        unsafe {
            drop(Box::from_raw(state));
            drop(Box::from_raw(mutex));
            drop(Box::from_raw(mailbox));
        }
        term.deinit(Global);
    }
}
