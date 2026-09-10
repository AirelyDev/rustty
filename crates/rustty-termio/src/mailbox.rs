//! The termio mailbox: the location to where write-related messages are sent.
//!
//! PORTS: ghostty src/termio/mailbox.zig (0 `test{}` blocks).
//!
//! PORT STATUS: struct shapes + xev-free logic ported (integration-tier stage
//! 2). `send` is deferred with its verbatim body below: its slow path unlocks
//! and relocks a caller-held `std.Thread.Mutex`, a manual lock/unlock pattern
//! Rust's guard-based `std::sync::Mutex` cannot express. The mutex MODEL is
//! now decided (core C0, CORE_BLUEPRINT §3): the guard-less Zig-shaped
//! `rustty_zigstd::thread::Mutex` — `send`'s parameter carries it; the body
//! un-defers with the core-tier termio pass (C-T; DEFERRALS.md).

// PARITY: the reference's identifiers are camelCase (`initSPSC`); preserved
// verbatim per R1/R2.
#![allow(non_snake_case)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const xev = @import("../global.zig").xev;
// const renderer = @import("../renderer.zig");
// const termio = @import("../termio.zig");
// const BlockingQueue = @import("../datastruct/main.zig").BlockingQueue;
//
// PORT NOTE: `renderer` is imported by the reference but not referenced by any
// declaration in this file, so no `use` is bound for it. `xev` is the D6
// deferred seam (crate::xev_seam).
use rustty_datastruct::blocking_queue::Timeout;
use rustty_datastruct::BlockingQueue;

use crate::xev_seam as xev;
use crate::Message;

// const log = std.log.scoped(.io_writer);
//
// PORT NOTE (DECISION-logging.md §6/§6r4): `LOG_SCOPE` is the `@tagName` of the
// reference's `std.log.scoped(.io_writer)`; restored sites go through the `log`
// facade. This replaces the two EMPTY `log_*` stub helpers this file used to
// carry — an empty helper is a dropped diagnostic, not a facade (G-LOGFILE's
// own `NOTE:` line lists them as such), so the calls are back inline where the
// reference has them.
const LOG_SCOPE: &str = "io_writer";

/// A queue used for storing messages that is periodically drained.
/// Typically used by a multi-threaded application. The capacity is
/// hardcoded to a value that empirically has made sense for Ghostty usage
/// but I'm open to changing it with good arguments.
//
// PORT NOTE: reference `const Queue = BlockingQueue(termio.Message, 64);`
// (file-private; `pub` here because it appears in the `Mailbox` field shape).
// The cross-thread queue stores the `'static` instantiation of `Message` — the
// reference's `write_stable` contract is "the data pointer is stable" across
// the consuming IO thread, which is exactly a `'static` borrow in Rust.
pub type Queue = BlockingQueue<Message<'static>, 64>;

/// The location to where write-related messages are sent.
pub enum Mailbox {
    // /// Write messages to an unbounded list backed by an allocator.
    // /// This is useful for single-threaded applications where you're not
    // /// afraid of running out of memory. You should be careful that you're
    // /// processing this in a timely manner though since some heavy workloads
    // /// will produce a LOT of messages.
    // ///
    // /// At the time of authoring this, the primary use case for this is
    // /// testing more than anything, but it probably will have a use case
    // /// in libghostty eventually.
    // unbounded: std.ArrayList(termio.Message),
    //
    /// Write messages to a SPSC queue for multi-threaded applications.
    //
    // PORT NOTE: reference `spsc: struct { queue: *Queue, wakeup: xev.Async }`.
    // `*Queue` (created via `Queue.create(alloc)`) is the owned `Box<Queue>`.
    Spsc {
        queue: Box<Queue>,
        wakeup: xev::Async,
    },
}

impl Mailbox {
    /// Init the SPSC writer.
    //
    // PORT NOTE: reference `pub fn initSPSC(alloc: Allocator) !Mailbox` — the
    // `alloc` param collapses into `Box` ownership and with it the OOM half of
    // the `!`; the remaining error is `xev.Async.init()`'s (the D6 seam).
    pub fn initSPSC() -> Result<Mailbox, xev::Error> {
        let queue = Queue::create();
        // errdefer queue.destroy(alloc);

        let wakeup = xev::Async::init()?;
        // errdefer wakeup.deinit();

        Ok(Mailbox::Spsc { queue, wakeup })
    }

    // PORT NOTE: reference `pub fn deinit(self: *Mailbox, alloc: Allocator)` —
    // consuming `self` (the reference invalidates the mailbox); the `alloc`
    // param collapses with the `Box` ownership.
    pub fn deinit(self) {
        match self {
            Mailbox::Spsc { queue, mut wakeup } => {
                // v.queue.destroy(alloc);
                queue.destroy();
                // v.wakeup.deinit();
                wakeup.deinit();
            }
        }
    }

    /// Sends the given message without notifying there are messages.
    ///
    /// If the optional mutex is given, it must already be LOCKED. If the
    /// send would block, we'll unlock this mutex, resend the message, and
    /// lock it again. This handles an edge case where queues are full.
    /// This may not apply to all writer types.
    //
    // pub fn send(
    //     self: *Mailbox,
    //     msg: termio.Message,
    //     mutex: ?*std.Thread.Mutex,
    // ) void {
    //
    // PORT NOTE (core C-T un-defer): the mutex MODEL landed in C0 — the
    // reference `?*std.Thread.Mutex` is the guard-less `rustty_zigstd::thread::
    // Mutex` (raw pointer, so the callee can `unlock()`/`lock()` a lock owned
    // by the caller's frame). The reference passes `msg` by COPY and re-pushes
    // it after a failed instant push; Rust moves, so the failed instant push
    // hands `msg` BACK via `push_or_return` (the retain-on-failure primitive).
    pub fn send(
        &mut self,
        msg: Message<'static>,
        mutex: Option<*mut rustty_zigstd::thread::Mutex>,
    ) {
        // switch (self.*) {
        match self {
            // .spsc => |*mb| send: {
            Mailbox::Spsc { queue, wakeup } => {
                // Try to write to the queue with an instant timeout. This is the
                // fast path because we can queue without a lock.
                // if (mb.queue.push(msg, .{ .instant = {} }) > 0) break :send;
                let msg = match queue.push_or_return(msg, Timeout::Instant) {
                    // A nonzero size means the message was queued: `break :send`.
                    Ok(_) => return,
                    // The queue was full; the message is handed back for the
                    // slow-path retry below.
                    Err(msg) => msg,
                };

                // If we enter this conditional, the queue is full. We wake up
                // the writer thread so that it can process messages to clear up
                // space. However, the writer thread may require the renderer
                // lock so we need to unlock.
                // mb.wakeup.notify() catch |err| {
                //     log.warn("failed to wake up writer, data will be dropped err={}", .{err});
                //     return;
                // };
                if let Err(err) = wakeup.notify() {
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to wake up writer, data will be dropped err={err:?}"
                    );
                    return;
                }

                // Unlock the renderer state so the writer thread can acquire it.
                // Then try to queue our message before continuing. This is a very
                // slow path because we are having a lot of contention for data.
                // But this only gets triggered in certain pathological cases.
                //
                // Note that writes themselves don't require a lock, but there
                // are other messages in the writer queue (resize, focus) that
                // could acquire the lock. This is why we have to release our lock
                // here.
                // if (mutex) |m| m.unlock();
                if let Some(m) = mutex {
                    // SAFETY: `mutex`, when set, is `renderer_state.mutex`, the
                    // shared guard-less lock held by the caller for its frame;
                    // the callee-unlock/relock inversion is exactly what the
                    // guard-less design exists for (zigstd thread::Mutex doc).
                    unsafe { (*m).unlock() };
                }
                // defer if (mutex) |m| m.lock();
                // (guard-less zigstd Mutex — the `defer` relock is the explicit
                // re-lock after the blocking push below, the sole exit.)
                // _ = mb.queue.push(msg, .{ .forever = {} });
                let _ = queue.push(msg, Timeout::Forever);
                if let Some(m) = mutex {
                    // SAFETY: see the unlock above — we relock the caller's lock.
                    unsafe { (*m).lock() };
                }
            }
        }
    }

    /// Notify that there are new messages. This may be a noop depending
    /// on the writer type.
    pub fn notify(&mut self) {
        match self {
            // .spsc => |*v| v.wakeup.notify() catch |err| {
            //     log.warn("failed to notify writer, data will be dropped err={}", .{err});
            // },
            Mailbox::Spsc { wakeup, .. } => {
                if let Err(err) = wakeup.notify() {
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to notify writer, data will be dropped err={err:?}"
                    );
                }
            }
        }
    }
}
