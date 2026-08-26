//! Represents the "writer" thread for terminal IO. The reader side is
//! handled by the Termio struct itself and dependent on the underlying
//! implementation (i.e. if its a pty, manual, etc.).
//!
//! The writer thread does handle writing bytes to the pty but also handles
//! different events such as starting synchronized output, changing some
//! modes (like linefeed), etc. The goal is to offload as much from the
//! reader thread as possible since it is the hot path in parsing VT
//! sequences and updating terminal state.
//!
//! This thread state can only be used by one thread at a time.
//!
//! PORTS: ghostty src/termio/Thread.zig (0 `test{}` blocks).
//!
//! PORT STATUS: struct shape + init/deinit ported (integration-tier stage 2).
//! The run loop (`threadMain`/`threadMain_`/`CallbackData`/`drainMailbox` and
//! the timer/async callbacks) landed in xev B4 — RAW `*mut Thread`/`*mut
//! Termio<M>` throughout ([ADV-FIX F2]), the frame-pinned `CallbackData` shuttle
//! carrying the out-param `ThreadData`, and ZST fn-item callbacks recovering
//! state via `*mut CallbackData`. Each ported body sits below its preserved
//! verbatim reference comment (0 reference tests — R1-safe). See
//! .port/phase-2/XEV_MILESTONE_B_BLUEPRINT.md.

// PARITY: the reference's identifiers are camelCase (`threadMain`); preserved
// verbatim per R1/R2.
#![allow(non_snake_case)]

// pub const Thread = @This();
//
// const std = @import("std");
// const ArenaAllocator = std.heap.ArenaAllocator;
// const builtin = @import("builtin");
// const xev = @import("../global.zig").xev;
// const crash = @import("../crash/main.zig");
// const internal_os = @import("../os/main.zig");
// const termio = @import("../termio.zig");
// const renderer = @import("../renderer.zig");
//
// const Allocator = std.mem.Allocator;
// const log = std.log.scoped(.io_thread);
//
// PORT NOTE: `crash`/`internal_os` (the sentry thread-state + Darwin thread
// naming) and `ArenaAllocator` are only used by the deferred run-loop bodies
// below; no `use` is bound for them yet.
use rustty_apprt_api::surface::AppMailbox;
use rustty_apprt_api::surface::Message as SurfaceMessage;
use rustty_datastruct::blocking_queue::Timeout;
use rustty_renderer::size::Size;
use rustty_vt::csi;
use rustty_vt::modes::Mode;

use crate::mailbox::Mailbox;
use crate::message::Message;
use crate::termio_impl::{Termio, ThreadData};
use crate::xev_seam as xev;

/// This stores the information that is coalesced.
//
// PORT NOTE: reference file-private `const Coalesce = struct`; `pub` here
// because it appears in the `Thread` field shape.
#[derive(Default)]
pub struct Coalesce {
    // (reference default: `= null`)
    pub resize: Option<Size>,
}

impl Coalesce {
    /// The number of milliseconds to coalesce certain messages like resize for.
    /// Not all message types are coalesced.
    //
    // PORT NOTE: reference `const min_ms = 25;` (a file-scope-style associated
    // const, consumed by the deferred `handleResize`).
    pub const MIN_MS: u64 = 25;
}

/// The number of milliseconds before we reset the synchronized output flag
/// if the running program hasn't already.
pub const SYNC_RESET_MS: u64 = 1000;

/// The number of milliseconds between each movement during selection scrolling.
pub const SELECTION_SCROLL_MS: u64 = 15;

// PORT NOTE: the reference is a top-level file-struct (`Thread.zig`); the Rust
// port names it `Thread`. The reference's first field `alloc: std.mem.Allocator`
// ("Allocator used for some state") collapses — Rust ownership carries the
// cleanup (cf_release_thread precedent). Field defaults (`= .{}` / `= false`)
// are documented at each field; `Thread::init` reproduces them.
pub struct Thread {
    /// The main event loop for the thread. The user data of this loop
    /// is always the allocator used to create the loop. This is a convenience
    /// so that users of the loop always have an allocator.
    //
    // PORT NOTE: reference field name `loop` is a Rust keyword -> `loop_`
    // (cf_release_thread precedent).
    pub loop_: xev::Loop,

    /// The completion to use for the wakeup async handle that is present
    /// on the termio.Writer.
    // (reference default: `= .{}`)
    pub wakeup_c: xev::Completion,

    /// This can be used to stop the thread on the next loop iteration.
    pub stop: xev::Async,
    // (reference default: `= .{}`)
    pub stop_c: xev::Completion,

    /// This is used for timer-based selection scrolling.
    pub scroll: xev::Timer,
    // (reference default: `= .{}`)
    pub scroll_c: xev::Completion,
    // (reference default: `= false`)
    pub scroll_active: bool,

    /// This is used to coalesce resize events.
    pub coalesce: xev::Timer,
    // (reference default: `= .{}`)
    pub coalesce_c: xev::Completion,
    // (reference default: `= .{}`)
    pub coalesce_cancel_c: xev::Completion,
    // (reference default: `= .{}`)
    pub coalesce_data: Coalesce,

    /// This timer is used to reset synchronized output modes so that
    /// the terminal doesn't freeze with a bad actor.
    pub sync_reset: xev::Timer,
    // (reference default: `= .{}`)
    pub sync_reset_c: xev::Completion,
    // (reference default: `= .{}`)
    pub sync_reset_cancel_c: xev::Completion,

    // (reference default: `= .{}`)
    pub flags: Flags,
}

/// PORT NOTE: the reference's inline `flags: packed struct { ... }`.
#[derive(Default)]
pub struct Flags {
    /// This is set to true only when an abnormal exit is detected. It
    /// tells our mailbox system to drain and ignore all messages.
    pub drain: bool,

    /// True if linefeed mode is enabled. This is duplicated here so that the
    /// write thread doesn't need to grab a lock to check this on every write.
    pub linefeed_mode: bool,

    /// This is true when the inspector is active.
    pub has_inspector: bool,
}

impl Thread {
    /// Initialize the thread. This does not START the thread. This only sets
    /// up all the internal state necessary prior to starting the thread. It
    /// is up to the caller to start the thread with the threadMain entrypoint.
    //
    // PORT NOTE: reference `pub fn init(alloc: Allocator) !Thread` — the
    // `alloc` param collapses with the alloc field (cf_release_thread
    // precedent); the errors are the xev seam's.
    pub fn init() -> Result<Thread, xev::Error> {
        // Create our event loop.
        // var loop = try xev.Loop.init(.{});
        let loop_ = xev::Loop::init(xev::Options::default())?;
        // errdefer loop.deinit();

        // This async handle is used to stop the loop and force the thread to end.
        let stop_h = xev::Async::init()?;
        // errdefer stop_h.deinit();

        // This timer is used for selection scrolling.
        let scroll_h = xev::Timer::init()?;
        // errdefer scroll_h.deinit();

        // This timer is used to coalesce resize events.
        let coalesce_h = xev::Timer::init()?;
        // errdefer coalesce_h.deinit();

        // This timer is used to reset synchronized output modes.
        let sync_reset_h = xev::Timer::init()?;
        // errdefer sync_reset_h.deinit();

        Ok(Thread {
            loop_,
            wakeup_c: xev::Completion::default(),
            stop: stop_h,
            stop_c: xev::Completion::default(),
            scroll: scroll_h,
            scroll_c: xev::Completion::default(),
            scroll_active: false,
            coalesce: coalesce_h,
            coalesce_c: xev::Completion::default(),
            coalesce_cancel_c: xev::Completion::default(),
            coalesce_data: Coalesce::default(),
            sync_reset: sync_reset_h,
            sync_reset_c: xev::Completion::default(),
            sync_reset_cancel_c: xev::Completion::default(),
            flags: Flags::default(),
        })
    }

    /// Clean up the thread. This is only safe to call once the thread
    /// completes executing; the caller must join prior to this.
    //
    // PORT NOTE: consuming `self` (cf_release `Thread::deinit` precedent).
    pub fn deinit(mut self) {
        self.scroll.deinit();
        self.coalesce.deinit();
        self.sync_reset.deinit();
        self.stop.deinit();
        self.loop_.deinit();
    }

    /// The main entrypoint for the thread.
    //
    // DEFERRED (D6 xev run loop, 0 tests — R1-safe): the whole run loop — the
    // `threadMain`/`threadMain_` pair, the `CallbackData` shuttle, the mailbox
    // drain and the timer/async callbacks — executes on the live xev loop and
    // is preserved verbatim below; the implementation lands with the D6 libxev
    // port (rustty-xev). NOT fabricated logic. See DEFERRALS.md.
    //
    // pub fn threadMain(self: *Thread, io: *termio.Termio) void {
    //     // Call child function so we can use errors...
    //     self.threadMain_(io) catch |err| {
    //         log.warn("error in io thread err={}", .{err});
    //
    //         // Use an arena to simplify memory management below
    //         var arena = ArenaAllocator.init(self.alloc);
    //         defer arena.deinit();
    //         const alloc = arena.allocator();
    //
    //         // If there is an error, we replace our terminal screen with
    //         // the error message. It might be better in the future to send
    //         // the error to the surface thread and let the apprt deal with it
    //         // in some way but this works for now. Without this, the user would
    //         // just see a blank terminal window.
    //         io.renderer_state.mutex.lock();
    //         defer io.renderer_state.mutex.unlock();
    //         const t = io.renderer_state.terminal;
    //
    //         // Hide the cursor
    //         t.modes.set(.cursor_visible, false);
    //
    //         // This is weird but just ensures that no matter what our underlying
    //         // implementation we have the errors below. For example, Windows doesn't
    //         // have "OpenptyFailed".
    //         const Err = @TypeOf(err) || error{
    //             OpenptyFailed,
    //             InputNotFound,
    //             InputFailed,
    //         };
    //
    //         switch (@as(Err, @errorCast(err))) {
    //             error.OpenptyFailed => {
    //                 const str =
    //                     \\Your system cannot allocate any more pty devices.
    //                     \\
    //                     \\Ghostty requires a pty device to launch a new terminal.
    //                     \\This error is usually due to having too many terminal
    //                     \\windows open or having another program that is using too
    //                     \\many pty devices.
    //                     \\
    //                     \\Please free up some pty devices and try again.
    //                 ;
    //
    //                 t.eraseDisplay(.complete, false);
    //                 t.printString(str) catch {};
    //             },
    //
    //             error.InputNotFound,
    //             error.InputFailed,
    //             => {
    //                 const str =
    //                     \\A configured `input` path was not found, was not readable,
    //                     \\was too large, or the underlying pty failed to accept
    //                     \\the write.
    //                     \\
    //                     \\Ghostty can't continue since it can't guarantee that
    //                     \\initial terminal state will be as desired. Please review
    //                     \\the value of `input` in your configuration file and
    //                     \\ensure that all the path values exist and are readable.
    //                 ;
    //
    //                 t.eraseDisplay(.complete, false);
    //                 t.printString(str) catch {};
    //             },
    //
    //             else => {
    //                 const str = std.fmt.allocPrint(
    //                     alloc,
    //                     \\error starting IO thread: {}
    //                     \\
    //                     \\The underlying shell or command was unable to be started.
    //                     \\This error is usually due to exhausting a system resource.
    //                     \\If this looks like a bug, please report it.
    //                     \\
    //                     \\This terminal is non-functional. Please close it and try again.
    //                 ,
    //                     .{err},
    //                 ) catch
    //                     \\Out of memory. This terminal is non-functional. Please close it and try again.
    //                 ;
    //
    //                 t.eraseDisplay(.complete, false);
    //                 t.printString(str) catch {};
    //             },
    //         }
    //     };
    //
    //     // If our loop is not stopped, then we need to keep running so that
    //     // messages are drained and we can wait for the surface to send a stop
    //     // message.
    //     if (!self.loop.stopped()) {
    //         log.warn("abrupt io thread exit detected, starting xev to drain mailbox", .{});
    //         defer log.debug("io thread fully exiting after abnormal failure", .{});
    //         self.flags.drain = true;
    //         self.loop.run(.until_done) catch |err| {
    //             log.err("failed to start xev loop for draining err={}", .{err});
    //         };
    //     }
    // }
    //
    // PORTED (xev B4, [ADV-FIX F2], 0 reference tests — R1-safe). RAW `self_: *mut
    // Thread` + `io: *mut Termio<M>` (NOT `&mut`): `threadMain` blocks in
    // `loop.run(.until_done)` until shell exit, during which the ReadThread aliases
    // the same Termio via `processOutput` and the parent reads `self.io_thread`
    // (`stop.notify()`) before the join — a `&mut` arg would be a protected `Unique`
    // retag for the whole call, aliasing both (`rust-mut-args-strongly-protected`).
    /// # Safety
    /// `self_`/`io` must be valid, live pointers for the whole call; the io thread
    /// is their sole writer between the surface's spawn and the join.
    pub unsafe fn threadMain<M: AppMailbox + Copy + 'static>(
        self_: *mut Thread,
        io: *mut Termio<M>,
    ) {
        // Call child function so we can use errors...
        // self.threadMain_(io) catch |err| {
        if let Err(err) = unsafe { Self::threadMain_(self_, io) } {
            // log.warn("error in io thread err={}", .{err});
            let _ = &err;

            // Use an arena to simplify memory management below
            // var arena = ArenaAllocator.init(self.alloc);
            // defer arena.deinit();
            // const alloc = arena.allocator();
            //
            // (PORT NOTE: the arena collapses to Rust ownership — the error message is an
            // owned `String` freed on drop; no `self.alloc` field exists, see the
            // `Thread::init` PORT NOTE.)

            // If there is an error, we replace our terminal screen with
            // the error message. It might be better in the future to send
            // the error to the surface thread and let the apprt deal with it
            // in some way but this works for now. Without this, the user would
            // just see a blank terminal window.
            // io.renderer_state.mutex.lock();
            // defer io.renderer_state.mutex.unlock();
            //
            // SAFETY: `io.renderer_state` is the live shared render State; its
            // guard-less `mutex` is the C0 zigstd lock (explicit lock/unlock).
            unsafe { (*(*(*io).renderer_state).mutex).lock() };
            // const t = io.renderer_state.terminal;
            let t = unsafe { (*(*io).renderer_state).terminal };

            // Hide the cursor
            // t.modes.set(.cursor_visible, false);
            unsafe { (*t).modes.set(Mode::CursorVisible, false) };

            // This is weird but just ensures that no matter what our underlying
            // implementation we have the errors below. For example, Windows doesn't
            // have "OpenptyFailed".
            // const Err = @TypeOf(err) || error{ OpenptyFailed, InputNotFound, InputFailed };
            //
            // switch (@as(Err, @errorCast(err))) {
            //     error.OpenptyFailed => { ...pty-exhaustion str...; t.eraseDisplay(.complete, false); t.printString(str) catch {}; },
            //     error.InputNotFound, error.InputFailed => { ...input-path str...; ... },
            //     else => { const str = std.fmt.allocPrint(...) catch "Out of memory..."; ... },
            // }
            //
            // PORT NOTE: `crate::Error` is the OPAQUE termio seam error (lib.rs:122);
            // it carries NO discriminant, so the reference's error switch cannot
            // dispatch `error.OpenptyFailed`/`InputNotFound`/`InputFailed`. Every
            // error collapses to the general `else` arm below. The two special-cased
            // arms (pty-exhaustion + input-path messages) are preserved verbatim in
            // the deferred comment above; they become reachable when `Error` gains
            // its real error set at the growth point (lib.rs:115).
            let str = format!(
                "error starting IO thread: {err}\n\
                 \n\
                 The underlying shell or command was unable to be started.\n\
                 This error is usually due to exhausting a system resource.\n\
                 If this looks like a bug, please report it.\n\
                 \n\
                 This terminal is non-functional. Please close it and try again."
            );
            // (PORT NOTE: Rust `format!` doesn't fail on OOM the way `allocPrint` does, so
            // the reference `catch "Out of memory..."` fallback is unreachable; the
            // fallback string is preserved in the comment above.)

            // t.eraseDisplay(.complete, false);
            unsafe { (*t).erase_display(csi::EraseDisplay::Complete, false) };
            // t.printString(str) catch {};
            let _ = unsafe { (*t).print_string(str.as_bytes()) };

            // defer io.renderer_state.mutex.unlock();
            unsafe { (*(*(*io).renderer_state).mutex).unlock() };
        }

        // If our loop is not stopped, then we need to keep running so that
        // messages are drained and we can wait for the surface to send a stop
        // message.
        // if (!self.loop.stopped()) {
        if !unsafe { (*self_).loop_.stopped() } {
            // log.warn("abrupt io thread exit detected, starting xev to drain mailbox", .{});
            // defer log.debug("io thread fully exiting after abnormal failure", .{});
            // self.flags.drain = true;
            unsafe { (*self_).flags.drain = true };
            // self.loop.run(.until_done) catch |err| {
            //     log.err("failed to start xev loop for draining err={}", .{err});
            // };
            let _ = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) };
        }
    }

    // DEFERRED (with threadMain above; D6). Preserved verbatim:
    //
    // fn threadMain_(self: *Thread, io: *termio.Termio) !void {
    //     defer log.debug("IO thread exited", .{});
    //
    //     // Right now, on Darwin, `std.Thread.setName` can only name the current
    //     // thread, and we have no way to get the current thread from within it,
    //     // so instead we use this code to name the thread instead.
    //     if (builtin.os.tag.isDarwin()) {
    //         internal_os.macos.pthread_setname_np(&"io".*);
    //     }
    //
    //     // Setup our crash metadata
    //     crash.sentry.thread_state = .{
    //         .type = .io,
    //         .surface = io.surface_mailbox.surface,
    //     };
    //     defer crash.sentry.thread_state = null;
    //
    //     // Get the mailbox. This must be an SPSC mailbox for threading.
    //     const mailbox = switch (io.mailbox) {
    //         .spsc => |*v| v,
    //         // else => return error.TermioUnsupportedMailbox,
    //     };
    //
    //     // This is the data sent to xev callbacks. We want a pointer to both
    //     // ourselves and the thread data so we can thread that through (pun intended).
    //     var cb: CallbackData = .{ .self = self, .io = io };
    //
    //     // Run our thread start/end callbacks. This allows the implementation
    //     // to hook into the event loop as needed. The thread data is created
    //     // on the stack here so that it has a stable pointer throughout the
    //     // lifetime of the thread.
    //     try io.threadEnter(self, &cb.data);
    //     defer cb.data.deinit();
    //     defer io.threadExit(&cb.data);
    //
    //     // Start the async handlers.
    //     mailbox.wakeup.wait(&self.loop, &self.wakeup_c, CallbackData, &cb, wakeupCallback);
    //     self.stop.wait(&self.loop, &self.stop_c, CallbackData, &cb, stopCallback);
    //
    //     // Run
    //     log.debug("starting IO thread", .{});
    //     defer log.debug("starting IO thread shutdown", .{});
    //     try self.loop.run(.until_done);
    // }
    //
    // /// This is the data passed to xev callbacks on the thread.
    // const CallbackData = struct {
    //     self: *Thread,
    //     io: *termio.Termio,
    //     data: termio.Termio.ThreadData = undefined,
    // };
    //
    // /// Drain the mailbox, handling all the messages in our terminal implementation.
    // fn drainMailbox(
    //     self: *Thread,
    //     cb: *CallbackData,
    // ) !void {
    //     // We assert when starting the thread that this is the state
    //     const mailbox = cb.io.mailbox.spsc.queue;
    //     const io = cb.io;
    //     const data = &cb.data;
    //
    //     // If we're draining, we just drain the mailbox and return.
    //     if (self.flags.drain) {
    //         while (mailbox.pop()) |_| {}
    //         return;
    //     }
    //
    //     // This holds the mailbox lock for the duration of the drain. The
    //     // expectation is that all our message handlers will be non-blocking
    //     // ENOUGH to not mess up throughput on producers.
    //     var redraw: bool = false;
    //     while (mailbox.pop()) |message| {
    //         // If we have a message we always redraw
    //         redraw = true;
    //
    //         log.debug("mailbox message={s}", .{@tagName(message)});
    //         switch (message) {
    //             .color_scheme_report => |v| try io.colorSchemeReport(data, v.force),
    //             .crash => @panic("crash request, crashing intentionally"),
    //             .change_config => |config| {
    //                 defer config.alloc.destroy(config.ptr);
    //                 try io.changeConfig(data, config.ptr);
    //             },
    //             .inspector => |v| self.flags.has_inspector = v,
    //             .resize => |v| self.handleResize(cb, v),
    //             .size_report => |v| try io.sizeReport(data, v),
    //             .clear_screen => |v| try io.clearScreen(data, v.history),
    //             .scroll_viewport => |v| io.scrollViewport(v),
    //             .selection_scroll => |v| {
    //                 if (v) {
    //                     self.startScrollTimer(cb);
    //                 } else {
    //                     self.stopScrollTimer();
    //                 }
    //             },
    //             .jump_to_prompt => |v| try io.jumpToPrompt(v),
    //             .start_synchronized_output => self.startSynchronizedOutput(cb),
    //             .linefeed_mode => |v| self.flags.linefeed_mode = v,
    //             .focused => |v| try io.focusGained(data, v),
    //             .write_small => |v| try io.queueWrite(
    //                 data,
    //                 v.data[0..v.len],
    //                 self.flags.linefeed_mode,
    //             ),
    //             .write_stable => |v| try io.queueWrite(
    //                 data,
    //                 v,
    //                 self.flags.linefeed_mode,
    //             ),
    //             .write_alloc => |v| {
    //                 defer v.alloc.free(v.data);
    //                 try io.queueWrite(
    //                     data,
    //                     v.data,
    //                     self.flags.linefeed_mode,
    //                 );
    //             },
    //         }
    //     }
    //
    //     // Trigger a redraw after we've drained so we don't waste cyces
    //     // messaging a redraw.
    //     if (redraw) {
    //         try io.renderer_wakeup.notify();
    //     }
    // }
    //
    // fn startSynchronizedOutput(self: *Thread, cb: *CallbackData) void {
    //     self.sync_reset.reset(
    //         &self.loop,
    //         &self.sync_reset_c,
    //         &self.sync_reset_cancel_c,
    //         sync_reset_ms,
    //         CallbackData,
    //         cb,
    //         syncResetCallback,
    //     );
    // }
    //
    // fn handleResize(self: *Thread, cb: *CallbackData, resize: renderer.Size) void {
    //     self.coalesce_data.resize = resize;
    //
    //     // If the timer is already active we just return. In the future we want
    //     // to reset the timer up to a maximum wait time but for now this ensures
    //     // relatively smooth resizing.
    //     if (self.coalesce_c.state() == .active) return;
    //
    //     self.coalesce.reset(
    //         &self.loop,
    //         &self.coalesce_c,
    //         &self.coalesce_cancel_c,
    //         Coalesce.min_ms,
    //         CallbackData,
    //         cb,
    //         coalesceCallback,
    //     );
    // }
    //
    // fn syncResetCallback(
    //     cb_: ?*CallbackData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction {
    //     _ = r catch |err| switch (err) {
    //         error.Canceled => {},
    //         else => {
    //             log.warn("error during sync reset callback err={}", .{err});
    //             return .disarm;
    //         },
    //     };
    //
    //     const cb = cb_ orelse return .disarm;
    //     cb.io.resetSynchronizedOutput();
    //     return .disarm;
    // }
    //
    // fn coalesceCallback(
    //     cb_: ?*CallbackData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction {
    //     _ = r catch |err| switch (err) {
    //         error.Canceled => {},
    //         else => {
    //             log.warn("error during coalesce callback err={}", .{err});
    //             return .disarm;
    //         },
    //     };
    //
    //     const cb = cb_ orelse return .disarm;
    //
    //     if (cb.self.coalesce_data.resize) |v| {
    //         cb.self.coalesce_data.resize = null;
    //         cb.io.resize(&cb.data, v) catch |err| {
    //             log.warn("error during resize err={}", .{err});
    //         };
    //     }
    //
    //     return .disarm;
    // }
    //
    // fn wakeupCallback(
    //     cb_: ?*CallbackData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Async.WaitError!void,
    // ) xev.CallbackAction {
    //     _ = r catch |err| {
    //         log.err("error in wakeup err={}", .{err});
    //         return .rearm;
    //     };
    //
    //     // When we wake up, we check the mailbox. Mailbox producers should
    //     // wake up our thread after publishing.
    //     const cb = cb_ orelse return .rearm;
    //     cb.self.drainMailbox(cb) catch |err|
    //         log.err("error draining mailbox err={}", .{err});
    //
    //     return .rearm;
    // }
    //
    // fn stopCallback(
    //     cb_: ?*CallbackData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Async.WaitError!void,
    // ) xev.CallbackAction {
    //     _ = r catch unreachable;
    //     cb_.?.self.loop.stop();
    //     return .disarm;
    // }
    //
    // fn startScrollTimer(self: *Thread, cb: *CallbackData) void {
    //     self.scroll_active = true;
    //
    //     switch (self.scroll_c.state()) {
    //         // If it is already active, e.g. startScrollTimer is called multiple
    //         // times, then we just return. We can't simply check `scroll_active`
    //         // because its possible that `stopScrollTimer` was called but there
    //         // was no loop tick between then and now to halt out completion.
    //         .active => return,
    //
    //         // If the completion is not active then we need to start it.
    //         .dead => self.scroll.run(
    //             &self.loop,
    //             &self.scroll_c,
    //             selection_scroll_ms,
    //             CallbackData,
    //             cb,
    //             selectionScrollCallback,
    //         ),
    //     }
    // }
    //
    // fn stopScrollTimer(self: *Thread) void {
    //     // This will stop the scrolling on the next iteration.
    //     self.scroll_active = false;
    // }
    //
    // fn selectionScrollCallback(
    //     cb_: ?*CallbackData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction {
    //     _ = r catch |err| switch (err) {
    //         error.Canceled => {},
    //         else => {
    //             log.warn("error during selection scroll callback err={}", .{err});
    //             return .disarm;
    //         },
    //     };
    //
    //     const cb = cb_ orelse return .disarm;
    //     const self = cb.self;
    //
    //     // Send the tick to the main surface
    //     _ = cb.io.surface_mailbox.push(
    //         .{ .selection_scroll_tick = self.scroll_active },
    //         .{ .instant = {} },
    //     );
    //
    //     if (self.scroll_active) self.scroll.run(
    //         &self.loop,
    //         &self.scroll_c,
    //         selection_scroll_ms,
    //         CallbackData,
    //         cb,
    //         selectionScrollCallback,
    //     );
    //
    //     return .disarm;
    // }
    //
    // PORTED (xev B4, [ADV-FIX F2], 0 reference tests — R1-safe): the run loop, its
    // frame-pinned `CallbackData` shuttle, the mailbox drain, and the timer/async
    // callbacks — all below the preserved verbatim reference above. RAW pointers
    // throughout (`self_: *mut Thread`, `cb: *mut CallbackData<M>`) matching the
    // reference's raw `*Thread`/`*CallbackData`: the loop stores completion pointers
    // into these and re-enters them across ticks, so no long-lived `&mut` may cross
    // a tick (`rust-mut-args-strongly-protected`). Message handlers form only
    // transient, field-scoped borrows; the shared terminal state each touches is
    // guarded by the renderer mutex inside the target method (the reference's
    // two-thread + shared-mutex model). All callbacks are ZST fn-items recovering
    // state via the `*mut CallbackData` userdata (the faithful shape of Zig's
    // comptime `struct{fn}`).

    // fn threadMain_(self: *Thread, io: *termio.Termio) !void {
    unsafe fn threadMain_<M: AppMailbox + Copy + 'static>(
        self_: *mut Thread,
        io: *mut Termio<M>,
    ) -> Result<(), crate::Error> {
        // defer log.debug("IO thread exited", .{});

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"io".*);
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            rustty_os::macos::pthread_setname_np(c"io".as_ptr());
        }

        // Setup our crash metadata
        // crash.sentry.thread_state = .{
        //     .type = .io,
        //     .surface = io.surface_mailbox.surface,
        // };
        // defer crash.sentry.thread_state = null;
        //
        // DEFERRED (cross-crate crash-sentry seam, not part of B4): the io thread's
        // crash metadata sets `thread_state.surface = io.surface_mailbox.surface`,
        // but the `AppMailbox` trait does not yet expose a `.surface` accessor (it
        // lands with rustty-core's `Surface: ThreadStateSurface` impl — the SAME
        // deferral as `ReadThread::threadMainPosix`, exec.rs). Preserved verbatim
        // above; wired when core lands.

        // Get the mailbox. This must be an SPSC mailbox for threading.
        // const mailbox = switch (io.mailbox) {
        //     .spsc => |*v| v,
        //     // else => return error.TermioUnsupportedMailbox,
        // };
        //
        // (PORT NOTE: extract the `wakeup` Async — `Copy` — for the `wait` below; the
        // queue itself is reached via `cb.io` in `drainMailbox`. Single-variant
        // match, irrefutable.)
        let mailbox_wakeup = match unsafe { &(*io).mailbox } {
            Mailbox::Spsc { wakeup, .. } => *wakeup,
        };

        // This is the data sent to xev callbacks. We want a pointer to both
        // ourselves and the thread data so we can thread that through (pun intended).
        // var cb: CallbackData = .{ .self = self, .io = io };
        //
        // Run our thread start/end callbacks. This allows the implementation
        // to hook into the event loop as needed. The thread data is created
        // on the stack here so that it has a stable pointer throughout the
        // lifetime of the thread.
        let mut cb: CallbackData<M> = CallbackData {
            self_,
            io,
            data: core::mem::MaybeUninit::uninit(),
        };
        let cb_ptr: *mut CallbackData<M> = &raw mut cb;

        // try io.threadEnter(self, &cb.data);
        //
        // SAFETY: `io`/`self_` are live; `(*cb_ptr).data` is the frame-pinned
        // out-param storage `threadEnter` initializes. On error we return BEFORE
        // registering the deinit/threadExit "defers" (faithful to Zig's `try`
        // preceding those `defer`s).
        unsafe { Termio::threadEnter(io, self_, (*cb_ptr).data.as_mut_ptr()) }?;
        // defer cb.data.deinit();
        // defer io.threadExit(&cb.data);
        // (PORT NOTE: both `defer`s run — in reverse registration order, threadExit then
        // data.deinit — AFTER `loop.run` returns; formed as explicit calls at the
        // single exit below, where the loop no longer aliases `cb.data`.)

        // Start the async handlers.
        // mailbox.wakeup.wait(&self.loop, &self.wakeup_c, CallbackData, &cb, wakeupCallback);
        // self.stop.wait(&self.loop, &self.stop_c, CallbackData, &cb, stopCallback);
        //
        // SAFETY: `loop_`/`wakeup_c`/`stop_c` are stable fields of the frame-pinned
        // Thread; `cb_ptr` is the frame-pinned userdata pointer; the callbacks are
        // ZST fn-items. `mailbox_wakeup`/`stop` are `Copy` Asyncs.
        unsafe {
            mailbox_wakeup.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).wakeup_c,
                cb_ptr,
                Self::wakeupCallback::<M>,
            );
            (*self_).stop.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).stop_c,
                cb_ptr,
                Self::stopCallback::<M>,
            );
        }

        // Run
        // log.debug("starting IO thread", .{});
        // defer log.debug("starting IO thread shutdown", .{});
        // try self.loop.run(.until_done);
        let run_result = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) };

        // defer io.threadExit(&cb.data);  (runs first)
        // defer cb.data.deinit();          (runs second)
        //
        // SAFETY: `loop.run` has returned, so the loop no longer holds pointers into
        // `cb.data`; it was fully initialized by `threadEnter`. `threadExit` writes
        // the read-thread quit pipe + joins; `deinit` then closes the pipe + deinits
        // the process/stream/timer. Finally we `drop_in_place` the ThreadData: the
        // reference's `cb.data` is a stack value whose `deinit` frees the write
        // pools directly, but the port's `ThreadData::deinit` collapses those to
        // Rust `Drop` (see its PORT NOTE), and `MaybeUninit` does NOT auto-drop — so
        // the explicit drop (matching the B2 KAT's `td.deinit()` + scope-drop) frees
        // the pools + detaches the joined read-thread placeholder.
        unsafe {
            let data_ptr = (*cb_ptr).data.as_mut_ptr();
            (*io).threadExit(&mut *data_ptr);
            (*data_ptr).deinit();
            core::ptr::drop_in_place(data_ptr);
        }

        // try self.loop.run(.until_done);
        run_result.map_err(|_| crate::Error)
    }

    // /// Drain the mailbox, handling all the messages in our terminal implementation.
    // fn drainMailbox(self: *Thread, cb: *CallbackData) !void {
    unsafe fn drainMailbox<M: AppMailbox + Copy + 'static>(
        self_: *mut Thread,
        cb: *mut CallbackData<M>,
    ) -> Result<(), crate::Error> {
        // We assert when starting the thread that this is the state
        // const mailbox = cb.io.mailbox.spsc.queue;
        // const io = cb.io;
        // const data = &cb.data;
        let io = unsafe { (*cb).io };
        let data = unsafe { (*cb).data.as_mut_ptr() };

        // If we're draining, we just drain the mailbox and return.
        // if (self.flags.drain) {
        //     while (mailbox.pop()) |_| {}
        //     return;
        // }
        if unsafe { (*self_).flags.drain } {
            while match unsafe { &(*io).mailbox } {
                Mailbox::Spsc { queue, .. } => queue.pop(),
            }
            .is_some()
            {}
            return Ok(());
        }

        // This holds the mailbox lock for the duration of the drain. The
        // expectation is that all our message handlers will be non-blocking
        // ENOUGH to not mess up throughput on producers.
        // var redraw: bool = false;
        let mut redraw = false;
        // while (mailbox.pop()) |message| {
        while let Some(message) = match unsafe { &(*io).mailbox } {
            Mailbox::Spsc { queue, .. } => queue.pop(),
        } {
            // If we have a message we always redraw
            redraw = true;

            // log.debug("mailbox message={s}", .{@tagName(message)});
            // switch (message) {
            //
            // (PORT NOTE: each `io.method(data, ...)` forms a transient `&mut *io` +
            // `&mut *data` — disjoint objects; the reference threads raw `io`/`data`.
            // The borrow never outlives the handler call, so it does not cross a loop
            // tick; shared terminal state is renderer-mutex-guarded inside each.)
            match message {
                // .color_scheme_report => |v| try io.colorSchemeReport(data, v.force),
                Message::ColorSchemeReport { force } => {
                    unsafe { (*io).colorSchemeReport(&mut *data, force) }?
                }
                // .visibility_report => |v| try io.visibilityReport(data, v.visible, v.force),
                Message::VisibilityReport { visible, force } => {
                    unsafe { (*io).visibilityReport(&mut *data, visible, force) }?
                }
                // .crash => @panic("crash request, crashing intentionally"),
                Message::Crash => panic!("crash request, crashing intentionally"),
                // .change_config => |config| {
                //     defer config.alloc.destroy(config.ptr);
                //     try io.changeConfig(data, config.ptr);
                // },
                Message::ChangeConfig { ptr } => {
                    // (PORT NOTE: `config.alloc.destroy(config.ptr)` collapses into the
                    // `Box<DerivedConfig>` drop — `*ptr` moves the value into
                    // `changeConfig` (by value here) and frees the box.)
                    unsafe { (*io).changeConfig(&mut *data, *ptr) }?
                }
                // .inspector => |v| self.flags.has_inspector = v,
                Message::Inspector(v) => unsafe { (*self_).flags.has_inspector = v },
                // .resize => |v| self.handleResize(cb, v),
                Message::Resize(v) => Self::handleResize(self_, cb, v),
                // .size_report => |v| try io.sizeReport(data, v),
                Message::SizeReport(v) => unsafe { (*io).sizeReport(&mut *data, v) }?,
                // .clear_screen => |v| try io.clearScreen(data, v.history),
                Message::ClearScreen { history } => {
                    unsafe { (*io).clearScreen(&mut *data, history) }?
                }
                // .scroll_viewport => |v| io.scrollViewport(v),
                Message::ScrollViewport(v) => unsafe { (*io).scrollViewport(v) },
                // .selection_scroll => |v| {
                //     if (v) { self.startScrollTimer(cb); } else { self.stopScrollTimer(); }
                // },
                Message::SelectionScroll(v) => {
                    if v {
                        Self::startScrollTimer(self_, cb);
                    } else {
                        Self::stopScrollTimer(self_);
                    }
                }
                // .jump_to_prompt => |v| try io.jumpToPrompt(v),
                Message::JumpToPrompt(v) => unsafe { (*io).jumpToPrompt(v) }?,
                // .kitty_clipboard_grant_read => |v| {
                //     defer v.alloc.free(v.pw);
                //     try io.kittyClipboardGrant(v.pw, .read);
                // },
                // (the `defer free` is the owned `pw` Vec dropping at arm end.)
                Message::KittyClipboardGrantRead(v) => unsafe {
                    (*io).kittyClipboardGrant(&v.pw, rustty_vt::kitty::clipboard::Direction::Read)
                },
                // .kitty_clipboard_grant_write => |v| { ... .write ... },
                Message::KittyClipboardGrantWrite(v) => unsafe {
                    (*io).kittyClipboardGrant(&v.pw, rustty_vt::kitty::clipboard::Direction::Write)
                },
                // .start_synchronized_output => self.startSynchronizedOutput(cb),
                Message::StartSynchronizedOutput => Self::startSynchronizedOutput(self_, cb),
                // .linefeed_mode => |v| self.flags.linefeed_mode = v,
                Message::LinefeedMode(v) => unsafe { (*self_).flags.linefeed_mode = v },
                // .focused => |v| try io.focusGained(data, v),
                Message::Focused(v) => unsafe { (*io).focusGained(&mut *data, v) }?,
                // .write_small => |v| try io.queueWrite(data, v.data[0..v.len], self.flags.linefeed_mode),
                Message::WriteSmall(v) => {
                    let linefeed = unsafe { (*self_).flags.linefeed_mode };
                    unsafe { (*io).queueWrite(&mut *data, &v.data[..v.len], linefeed) }?
                }
                // .write_stable => |v| try io.queueWrite(data, v, self.flags.linefeed_mode),
                Message::WriteStable(v) => {
                    let linefeed = unsafe { (*self_).flags.linefeed_mode };
                    unsafe { (*io).queueWrite(&mut *data, v, linefeed) }?
                }
                // .write_alloc => |v| {
                //     defer v.alloc.free(v.data);
                //     try io.queueWrite(data, v.data, self.flags.linefeed_mode);
                // },
                Message::WriteAlloc(v) => {
                    // (PORT NOTE: `v.alloc.free(v.data)` collapses into the `Box<[u8]>`
                    // drop — `v` drops at the end of this arm.)
                    let linefeed = unsafe { (*self_).flags.linefeed_mode };
                    unsafe { (*io).queueWrite(&mut *data, &v.data, linefeed) }?
                }
            }
        }

        // Trigger a redraw after we've drained so we don't waste cyces
        // messaging a redraw.
        // if (redraw) {
        //     try io.renderer_wakeup.notify();
        // }
        if redraw {
            unsafe { (*io).renderer_wakeup.notify() }.map_err(|_| crate::Error)?;
        }

        Ok(())
    }

    // fn startSynchronizedOutput(self: *Thread, cb: *CallbackData) void {
    fn startSynchronizedOutput<M: AppMailbox + Copy + 'static>(
        self_: *mut Thread,
        cb: *mut CallbackData<M>,
    ) {
        // self.sync_reset.reset(
        //     &self.loop, &self.sync_reset_c, &self.sync_reset_cancel_c,
        //     sync_reset_ms, CallbackData, cb, syncResetCallback,
        // );
        //
        // SAFETY: `sync_reset` is a `Copy` Timer; the loop/completions are stable
        // frame-pinned Thread fields; `cb` is the frame-pinned userdata.
        unsafe {
            (*self_).sync_reset.reset(
                &raw mut (*self_).loop_,
                &raw mut (*self_).sync_reset_c,
                &raw mut (*self_).sync_reset_cancel_c,
                SYNC_RESET_MS,
                cb,
                Self::syncResetCallback::<M>,
            );
        }
    }

    // fn handleResize(self: *Thread, cb: *CallbackData, resize: renderer.Size) void {
    fn handleResize<M: AppMailbox + Copy + 'static>(
        self_: *mut Thread,
        cb: *mut CallbackData<M>,
        resize: Size,
    ) {
        // self.coalesce_data.resize = resize;
        unsafe { (*self_).coalesce_data.resize = Some(resize) };

        // If the timer is already active we just return. In the future we want
        // to reset the timer up to a maximum wait time but for now this ensures
        // relatively smooth resizing.
        // if (self.coalesce_c.state() == .active) return;
        if unsafe { (*self_).coalesce_c.state_of() } == xev::CompletionState::Active {
            return;
        }

        // self.coalesce.reset(
        //     &self.loop, &self.coalesce_c, &self.coalesce_cancel_c,
        //     Coalesce.min_ms, CallbackData, cb, coalesceCallback,
        // );
        unsafe {
            (*self_).coalesce.reset(
                &raw mut (*self_).loop_,
                &raw mut (*self_).coalesce_c,
                &raw mut (*self_).coalesce_cancel_c,
                Coalesce::MIN_MS,
                cb,
                Self::coalesceCallback::<M>,
            );
        }
    }

    // fn syncResetCallback(cb_: ?*CallbackData, _: *xev.Loop, _: *xev.Completion, r: xev.Timer.RunError!void) xev.CallbackAction {
    fn syncResetCallback<M: AppMailbox + Copy + 'static>(
        cb_: *mut CallbackData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     error.Canceled => {},
        //     else => { log.warn("error during sync reset callback err={}", .{err}); return .disarm; },
        // };
        match r {
            Ok(()) => {}
            Err(xev::TimerRunError::Canceled) => {}
            Err(_err) => {
                // log.warn("error during sync reset callback err={}", .{err});
                return xev::CallbackAction::Disarm;
            }
        }

        // const cb = cb_ orelse return .disarm;
        if cb_.is_null() {
            return xev::CallbackAction::Disarm;
        }
        let cb = cb_;
        // cb.io.resetSynchronizedOutput();
        unsafe { (*(*cb).io).resetSynchronizedOutput() };
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn coalesceCallback(cb_: ?*CallbackData, _: *xev.Loop, _: *xev.Completion, r: xev.Timer.RunError!void) xev.CallbackAction {
    fn coalesceCallback<M: AppMailbox + Copy + 'static>(
        cb_: *mut CallbackData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     error.Canceled => {},
        //     else => { log.warn("error during coalesce callback err={}", .{err}); return .disarm; },
        // };
        match r {
            Ok(()) => {}
            Err(xev::TimerRunError::Canceled) => {}
            Err(_err) => {
                // log.warn("error during coalesce callback err={}", .{err});
                return xev::CallbackAction::Disarm;
            }
        }

        // const cb = cb_ orelse return .disarm;
        if cb_.is_null() {
            return xev::CallbackAction::Disarm;
        }
        let cb = cb_;

        // if (cb.self.coalesce_data.resize) |v| {
        //     cb.self.coalesce_data.resize = null;
        //     cb.io.resize(&cb.data, v) catch |err| { log.warn("error during resize err={}", .{err}); };
        // }
        let self_ = unsafe { (*cb).self_ };
        // (PORT NOTE: `.take()` reads the resize AND clears it — the reference's `if
        // (...) |v| { ...resize = null; ... }`.)
        if let Some(v) = unsafe { (*self_).coalesce_data.resize.take() } {
            let io = unsafe { (*cb).io };
            let data = unsafe { (*cb).data.as_mut_ptr() };
            if unsafe { (*io).resize(&mut *data, v) }.is_err() {
                // log.warn("error during resize err={}", .{err});
            }
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn wakeupCallback(cb_: ?*CallbackData, _: *xev.Loop, _: *xev.Completion, r: xev.Async.WaitError!void) xev.CallbackAction {
    fn wakeupCallback<M: AppMailbox + Copy + 'static>(
        cb_: *mut CallbackData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| { log.err("error in wakeup err={}", .{err}); return .rearm; };
        if r.is_err() {
            // log.err("error in wakeup err={}", .{err});
            return xev::CallbackAction::Rearm;
        }

        // When we wake up, we check the mailbox. Mailbox producers should
        // wake up our thread after publishing.
        // const cb = cb_ orelse return .rearm;
        if cb_.is_null() {
            return xev::CallbackAction::Rearm;
        }
        let cb = cb_;
        // cb.self.drainMailbox(cb) catch |err| log.err("error draining mailbox err={}", .{err});
        let self_ = unsafe { (*cb).self_ };
        if unsafe { Self::drainMailbox(self_, cb) }.is_err() {
            // log.err("error draining mailbox err={}", .{err});
        }

        // return .rearm;
        xev::CallbackAction::Rearm
    }

    // fn stopCallback(cb_: ?*CallbackData, _: *xev.Loop, _: *xev.Completion, r: xev.Async.WaitError!void) xev.CallbackAction {
    fn stopCallback<M: AppMailbox + Copy + 'static>(
        cb_: *mut CallbackData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch unreachable;
        r.expect("stop wait: unreachable error");
        // cb_.?.self.loop.stop();
        let self_ = unsafe { (*cb_).self_ };
        unsafe { (*self_).loop_.stop() };
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn startScrollTimer(self: *Thread, cb: *CallbackData) void {
    fn startScrollTimer<M: AppMailbox + Copy + 'static>(
        self_: *mut Thread,
        cb: *mut CallbackData<M>,
    ) {
        // self.scroll_active = true;
        unsafe { (*self_).scroll_active = true };

        // switch (self.scroll_c.state()) {
        match unsafe { (*self_).scroll_c.state_of() } {
            // If it is already active, e.g. startScrollTimer is called multiple
            // times, then we just return. We can't simply check `scroll_active`
            // because its possible that `stopScrollTimer` was called but there
            // was no loop tick between then and now to halt out completion.
            // .active => return,
            xev::CompletionState::Active => {}

            // If the completion is not active then we need to start it.
            // .dead => self.scroll.run(&self.loop, &self.scroll_c, selection_scroll_ms, CallbackData, cb, selectionScrollCallback),
            xev::CompletionState::Dead => unsafe {
                (*self_).scroll.run(
                    &raw mut (*self_).loop_,
                    &raw mut (*self_).scroll_c,
                    SELECTION_SCROLL_MS,
                    cb,
                    Self::selectionScrollCallback::<M>,
                );
            },
        }
    }

    // fn stopScrollTimer(self: *Thread) void {
    fn stopScrollTimer(self_: *mut Thread) {
        // This will stop the scrolling on the next iteration.
        // self.scroll_active = false;
        unsafe { (*self_).scroll_active = false };
    }

    // fn selectionScrollCallback(cb_: ?*CallbackData, _: *xev.Loop, _: *xev.Completion, r: xev.Timer.RunError!void) xev.CallbackAction {
    fn selectionScrollCallback<M: AppMailbox + Copy + 'static>(
        cb_: *mut CallbackData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     error.Canceled => {},
        //     else => { log.warn("error during selection scroll callback err={}", .{err}); return .disarm; },
        // };
        match r {
            Ok(()) => {}
            Err(xev::TimerRunError::Canceled) => {}
            Err(_err) => {
                // log.warn("error during selection scroll callback err={}", .{err});
                return xev::CallbackAction::Disarm;
            }
        }

        // const cb = cb_ orelse return .disarm;
        if cb_.is_null() {
            return xev::CallbackAction::Disarm;
        }
        let cb = cb_;
        // const self = cb.self;
        let self_ = unsafe { (*cb).self_ };

        // Send the tick to the main surface
        // _ = cb.io.surface_mailbox.push(.{ .selection_scroll_tick = self.scroll_active }, .{ .instant = {} });
        let scroll_active = unsafe { (*self_).scroll_active };
        let _ = unsafe {
            (*(*cb).io).surface_mailbox.push(
                SurfaceMessage::SelectionScrollTick(scroll_active),
                Timeout::Instant,
            )
        };

        // if (self.scroll_active) self.scroll.run(&self.loop, &self.scroll_c, selection_scroll_ms, CallbackData, cb, selectionScrollCallback);
        if scroll_active {
            unsafe {
                (*self_).scroll.run(
                    &raw mut (*self_).loop_,
                    &raw mut (*self_).scroll_c,
                    SELECTION_SCROLL_MS,
                    cb,
                    Self::selectionScrollCallback::<M>,
                );
            }
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }
}

/// This is the data passed to xev callbacks on the thread.
//
// PORT NOTE: reference file-private `const CallbackData = struct { self: *Thread,
// io: *termio.Termio, data: termio.Termio.ThreadData = undefined }`. `self` is a
// Rust keyword -> `self_`. `data = undefined` -> `MaybeUninit<ThreadData<M>>`:
// `Termio::threadEnter` fills it via the out-param `ptr::write` (the faithful
// mapping of Zig's `undefined`-out-param protocol). Created on the stack in
// `threadMain_` so it has a stable pointer throughout the thread's lifetime.
// `M` threads the `apprt.surface.Mailbox` seam through `ThreadData<M>`.
pub(crate) struct CallbackData<M: AppMailbox> {
    pub(crate) self_: *mut Thread,
    pub(crate) io: *mut Termio<M>,
    pub(crate) data: core::mem::MaybeUninit<ThreadData<M>>,
}
