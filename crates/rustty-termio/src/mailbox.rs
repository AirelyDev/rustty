//! The termio mailbox: the location to where write-related messages are sent.
//!
//! PORTS: ghostty src/termio/mailbox.zig (0 `test{}` blocks).
//!
//! PORT STATUS: struct shapes + xev-free logic ported (integration-tier stage
//! 2). `send` is deferred with its verbatim body below: its slow path unlocks
//! and relocks a caller-held `std.Thread.Mutex`, a manual lock/unlock pattern
//! Rust's guard-based `std::sync::Mutex` cannot express — the mutex modeling is
//! decided once, when the threads become live (D6 / core tier; DEFERRALS.md).

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
use rustty_datastruct::BlockingQueue;

use crate::xev_seam as xev;
use crate::Message;

// const log = std.log.scoped(.io_writer);
//
// PORT: routed through named log helpers below that preserve the reference's
// messages verbatim (rustty-os cf_release_thread precedent).

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
    // DEFERRED (D6 / core-tier mutex modeling, 0 tests — R1-safe): the body
    // unlocks and relocks the caller-held renderer-state mutex
    // (`m.unlock()` ... `defer m.lock()`), Zig `std.Thread.Mutex`'s manual
    // lock/unlock API that guard-based `std::sync::Mutex` cannot express from
    // this stack frame. The reference body is preserved verbatim; the
    // implementation lands with the live-thread wiring (see DEFERRALS.md):
    //
    // pub fn send(
    //     self: *Mailbox,
    //     msg: termio.Message,
    //     mutex: ?*std.Thread.Mutex,
    // ) void {
    //     switch (self.*) {
    //         .spsc => |*mb| send: {
    //             // Try to write to the queue with an instant timeout. This is the
    //             // fast path because we can queue without a lock.
    //             if (mb.queue.push(msg, .{ .instant = {} }) > 0) break :send;
    //
    //             // If we enter this conditional, the queue is full. We wake up
    //             // the writer thread so that it can process messages to clear up
    //             // space. However, the writer thread may require the renderer
    //             // lock so we need to unlock.
    //             mb.wakeup.notify() catch |err| {
    //                 log.warn("failed to wake up writer, data will be dropped err={}", .{err});
    //                 return;
    //             };
    //
    //             // Unlock the renderer state so the writer thread can acquire it.
    //             // Then try to queue our message before continuing. This is a very
    //             // slow path because we are having a lot of contention for data.
    //             // But this only gets triggered in certain pathological cases.
    //             //
    //             // Note that writes themselves don't require a lock, but there
    //             // are other messages in the writer queue (resize, focus) that
    //             // could acquire the lock. This is why we have to release our lock
    //             // here.
    //             if (mutex) |m| m.unlock();
    //             defer if (mutex) |m| m.lock();
    //             _ = mb.queue.push(msg, .{ .forever = {} });
    //         },
    //     }
    // }
    pub fn send(&mut self, msg: Message<'static>, mutex: Option<*mut std::sync::Mutex<()>>) {
        let _ = (msg, mutex);
        unimplemented!(
            "termio.Mailbox.send deferred (D6 / core-tier mutex modeling); verbatim body above"
        )
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
                    log_warn_notify_failed(&err);
                }
            }
        }
    }
}

// PORT: the `log.warn` call site in `notify`, preserved verbatim (cf_release
// log-helper precedent).
fn log_warn_notify_failed(_err: &xev::NotifyError) {
    // log.warn("failed to notify writer, data will be dropped err={}", .{err});
}
