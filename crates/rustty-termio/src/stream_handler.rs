//! The big vt-Stream handler: routes terminal.Stream actions into terminal
//! state mutations, termio-mailbox writes, and surface/renderer messages.
//!
//! PORTS: ghostty src/termio/stream_handler.zig (2 `test{}` blocks — both
//! ported: `kitty clipboard read: targets-only never consumes a one-time grant`
//! and `kitty clipboard write: oversized text replies EFBIG`).
//
// CORRECTION (task-18, an unclaimed instance of the S139-F1 stale-count class —
// see `NEW-FINDINGS.md` N33): this said "(0 `test{}` blocks)". That was a claim
// about the REFERENCE, and it is false at pin `20abdb50a` — a zero is the most
// dangerous count to leave stale, because nothing downstream looks for tests to
// port from a file the header says has none.
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
pub type Stream<M> = stream::Stream<StreamHandler<M>>;

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
        // (N444: `Mailbox::send` is `&self`; no `&mut Mailbox` while the io
        // writer thread drains it.)
        unsafe { &*self.termio_mailbox }.send(msg, Some(mutex));
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
}

mod dcs_apc;
mod handler_impl;
mod kitty_clipboard;
mod modes_reports;
mod osc_handlers;

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

/// PORT NOTE (Zig-std shim): the error set of `std.Uri.getHost`, plus
/// `UriHostTooLong`, which Zig 0.16.0 does not have: its `getHost` treats the
/// buffer overflow as `unreachable` (reference defect UD351, R13; see
/// `reportPwd`).
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
pub(crate) mod tests;
