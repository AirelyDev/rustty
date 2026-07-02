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
//! the timer/async callbacks) is DEFERRED with faithful signatures + verbatim
//! reference bodies in comments + `unimplemented!()` markers (D6 xev; 0
//! reference tests — R1-safe), NOT fabricated logic. See
//! .port/phase-2/DEFERRALS.md ("rustty-termio stage 2").

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
use rustty_renderer::size::Size;

use crate::termio_impl::Termio;
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

/// PORT: the reference's inline `flags: packed struct { ... }`.
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
        let loop_ = xev::Loop::init()?;
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
    pub fn threadMain<M: AppMailbox>(&mut self, _io: &mut Termio<M>) {
        unimplemented!("termio.Thread.threadMain deferred (D6 xev run loop); verbatim body above")
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
}
