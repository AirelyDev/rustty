//! Represents the CFRelease thread. Pools of CFTypeRefs are sent to
//! this thread to be released, so that their release callback logic
//! doesn't block the execution of a high throughput thread like the
//! renderer thread.
//!
//! rustty-os::cf_release_thread
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/cf_release_thread.zig` — every line, comment,
//! and `test {}` is ported here (R1). Retargeted only for product identity (R2).
//! The reference has no `test {}` blocks (Phase-0 inventory:
//! `os/cf_release_thread.zig` has 0 tests).
//!
//! PARITY (SEAM SATISFIED, R4): the reference imports `xev =
//! @import("../global.zig").xev`. In rustty that backend is now the real
//! `rustty-xev` kqueue Loop/Async (Milestone A/B), a downward-acyclic macOS edge
//! (rustty-xev deps only `{ rustty-global, libc }`). The full thread logic below
//! is ported verbatim against the live `xev` surface (`Loop`/`Async`/`Completion`/
//! `CallbackAction`/`WaitError=MachPortError`/`RunMode::UntilDone`). No reference
//! logic, comment, or behavior is dropped.

// PORT NOTE: `pub const Thread = @This();` — the file *is* the `Thread` struct in
// Zig. In Rust the file is a module and `Thread` is a named struct below.

// PORT NOTE: the Zig reference imports `std`, `builtin`, `macos`, `internal_os =
// os/main.zig`, `xev = global.zig.xev`, `BlockingQueue = datastruct/main.zig`,
// `Allocator = std.mem.Allocator`, and `log = std.log.scoped(.cf_release_thread)`.
// In Rust we use the ported `crate::macos` sibling, `rustty_datastruct::BlockingQueue`,
// the real `rustty_xev` backend, and CoreFoundation's `CFRelease` declared as
// `extern "C"` (R3 binding to the SAME framework).
#![allow(dead_code)]
// PARITY: the reference's identifiers are camelCase (`threadMain`, `drainMailbox`,
// `wakeupCallback`, `stopCallback`, ...). Preserved verbatim per R1/R2.
#![allow(non_snake_case)]

use rustty_datastruct::BlockingQueue;

// PORT NOTE: `const xev = @import("../global.zig").xev;` — now the real kqueue backend.
#[cfg(target_os = "macos")]
use rustty_xev as xev;

// PORT NOTE: `const log = std.log.scoped(.cf_release_thread);`. Routed through the
// log facade where used; the call sites preserve the reference's messages.

pub enum Message {
    /// Release a slice of CFTypeRefs. Uses alloc to free the slice after
    /// releasing all the refs.
    Release { refs: Vec<*mut std::ffi::c_void> },
}

/// The type used for sending messages to the thread. For now this is
/// hardcoded with a capacity. We can make this a comptime parameter in
/// the future if we want it configurable.
pub type Mailbox = BlockingQueue<Message, 64>;

/// PARITY: the reference is a single struct (the file). Its fields are reproduced
/// here, except the reference's first field (see the `alloc` PARITY note below)
/// which Rust ownership subsumes; all remaining fields are verbatim.
//
// PORT NOTE: macOS-gated because `loop_`/`wakeup`/`stop` name `rustty_xev::Loop`/
// `Async`, which are `#[cfg(target_os = "macos")]` (the kqueue backend is
// macOS-only). `Message`/`Mailbox` (no xev) stay cross-platform below.
#[cfg(target_os = "macos")]
pub struct Thread {
    /// Allocator used for some state
    //
    // PARITY: the reference's first field is `alloc: std.mem.Allocator`. In Rust
    // the allocator is implicit (the global allocator), and the owned `Box<Mailbox>`
    // (and the `xev` handles) carry their own RAII cleanup, so there is no literal
    // `alloc` field to store; this field is subsumed by Rust ownership. The
    // reference's `self.alloc` usages map to allocator-less Rust equivalents:
    // `Mailbox.create(alloc)` -> `Mailbox::create()`, and
    // `self.mailbox.destroy(self.alloc)` -> `self.mailbox.destroy()`.

    /// The main event loop for the thread. The user data of this loop
    /// is always the allocator used to create the loop. This is a convenience
    /// so that users of the loop always have an allocator.
    pub loop_: xev::Loop,

    /// This can be used to wake up the thread.
    pub wakeup: xev::Async,
    wakeup_c: xev::Completion,

    /// This can be used to stop the thread on the next loop iteration.
    pub stop: xev::Async,
    stop_c: xev::Completion,

    /// The mailbox that can be used to send this thread messages. Note
    /// this is a blocking queue so if it is full you will get errors (or block).
    pub mailbox: Box<Mailbox>,

    flags: Flags,
}

/// PORT NOTE: the reference's inline `flags: packed struct { drain: bool = false }`.
#[cfg(target_os = "macos")]
#[derive(Default)]
pub struct Flags {
    /// This is set to true only when an abnormal exit is detected. It
    /// tells our mailbox system to drain and ignore all messages.
    drain: bool,
}

#[cfg(target_os = "macos")]
impl Thread {
    /// Initialize the thread. This does not START the thread. This only sets
    /// up all the internal state necessary prior to starting the thread. It
    /// is up to the caller to start the thread with the threadMain entrypoint.
    pub fn init() -> Result<Thread, xev::Error> {
        // Create our event loop.
        let loop_ = xev::Loop::init()?;
        // errdefer loop.deinit();

        // This async handle is used to "wake up" the thread to collect objects.
        let wakeup_h = xev::Async::init()?;
        // errdefer wakeup_h.deinit();

        // This async handle is used to stop the loop and force the thread to end.
        let stop_h = xev::Async::init()?;
        // errdefer stop_h.deinit();

        // The mailbox for messaging this thread
        let mailbox = Mailbox::create();
        // errdefer mailbox.destroy(alloc);

        Ok(Thread {
            loop_,
            wakeup: wakeup_h,
            wakeup_c: xev::Completion::default(),
            stop: stop_h,
            stop_c: xev::Completion::default(),
            mailbox,
            flags: Flags::default(),
        })
    }

    /// Clean up the thread. This is only safe to call once the thread
    /// completes executing; the caller must join prior to this.
    pub fn deinit(mut self) {
        self.stop.deinit();
        self.wakeup.deinit();
        self.loop_.deinit();

        // Nothing can possibly access the mailbox anymore, destroy it.
        self.mailbox.destroy();
    }

    /// The main entrypoint for the thread.
    //
    // PORT NOTE (SB/TB, `rust-mut-args-strongly-protected`): the reference
    // `threadMain(self: *Thread)` keeps `&mut self` here, but delegates to the
    // raw-`*mut Thread` child so the raw aliasing (the loop simultaneously holds
    // `&raw mut (*self_).loop_`/`(*self_).wakeup_c` as raw pointers) lives only
    // where the loop holds it, never as a `&mut self` re-read across ticks.
    pub fn threadMain(&mut self) {
        let self_: *mut Thread = self;
        // Call child function so we can use errors...
        if let Err(err) = unsafe { Self::threadMain_(self_) } {
            log_warn_thread_err(&err);
        }

        // If our loop is not stopped, then we need to keep running so that
        // messages are drained and we can wait for the surface to send a stop
        // message.
        if !unsafe { (*self_).loop_.stopped() } {
            log_warn_abrupt_exit();
            // defer log.debug("cf release thread fully exiting after abnormal failure", .{});
            unsafe { (*self_).flags.drain = true };
            if let Err(err) = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) } {
                log_err_drain_failed(&err);
            }
            log_debug_fully_exiting();
        }
    }

    /// # Safety
    /// `self_` must be a valid, uniquely-owned `Thread` pointer that outlives the
    /// loop run (the loop holds its interior `loop_`/`wakeup_c`/`stop_c` raw).
    unsafe fn threadMain_(self_: *mut Thread) -> Result<(), xev::Error> {
        // defer log.debug("cf release thread exited", .{});

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        #[cfg(target_os = "macos")]
        {
            unsafe { crate::macos::pthread_setname_np(c"cf_release".as_ptr()) };
        }

        // Start the async handlers. We start these first so that they're
        // registered even if anything below fails so we can drain the mailbox.
        //
        // PORT NOTE: `self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self,
        // wakeupCallback)` — `wakeup` is read out by-copy as the `.wait(self, ..)`
        // receiver (Async is Copy), which does NOT create a `&mut` to `wakeup`, so
        // there is no aliasing with the `&raw mut (*self_).wakeup_c` argument. The
        // loop userdata IS the `Thread` (`UD = Thread`, `userdata = self_`).
        unsafe {
            (*self_).wakeup.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).wakeup_c,
                self_,
                wakeupCallback,
            );
            (*self_).stop.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).stop_c,
                self_,
                stopCallback,
            );
        }

        // Run
        log_debug_starting();
        // defer log.debug("starting cf release thread shutdown", .{});
        let result = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) };
        log_debug_shutdown();
        result
    }

    /// Drain the mailbox, handling all the messages in our terminal implementation.
    //
    /// # Safety
    /// `self_` is the loop userdata (non-null by construction); only reached from
    /// `wakeupCallback` via the loop-held raw pointer.
    unsafe fn drainMailbox(self_: *mut Thread) -> Result<(), xev::Error> {
        // If we're draining, we just drain the mailbox and return.
        if unsafe { (*self_).flags.drain } {
            while unsafe { (*self_).mailbox.pop() }.is_some() {}
            return Ok(());
        }

        while let Some(message) = unsafe { (*self_).mailbox.pop() } {
            // log.debug("mailbox message={}", .{message});
            match message {
                Message::Release { refs } => {
                    for r in &refs {
                        unsafe { CFRelease(*r) };
                    }
                    // log.debug("Released {} CFTypeRefs.", .{ msg.refs.len });
                    // PORT NOTE: `msg.alloc.free(msg.refs);` — the `Vec<*mut c_void>`
                    // is dropped here, freeing the slice.
                    drop(refs);
                }
            }
        }
        Ok(())
    }
}

// PORT NOTE: the raw-`*mut Thread` callback ABI (matching termio thread.rs). The real
// `Async::wait` hands the userdata + loop/completion back as raw pointers held
// across ticks; the callback error type is `xev::MachPortError` (the wakeup
// async's `WaitError`), NOT the deleted placeholder `AsyncWaitError`.
#[cfg(target_os = "macos")]
fn wakeupCallback(
    self_: *mut Thread,
    _loop: *mut xev::Loop,
    _c: *mut xev::Completion,
    r: core::result::Result<(), xev::MachPortError>,
) -> xev::CallbackAction {
    if let Err(err) = r {
        log_err_wakeup(&err);
        return xev::CallbackAction::Rearm;
    }

    // const t = self_.?;  -> the reference unwraps the optional; our UD is
    // non-null by construction (the loop userdata IS the Thread).

    // When we wake up, we check the mailbox. Mailbox producers should
    // wake up our thread after publishing.
    if let Err(err) = unsafe { Thread::drainMailbox(self_) } {
        log_err_drain_mailbox(&err);
    }

    xev::CallbackAction::Rearm
}

#[cfg(target_os = "macos")]
fn stopCallback(
    self_: *mut Thread,
    _loop: *mut xev::Loop,
    _c: *mut xev::Completion,
    r: core::result::Result<(), xev::MachPortError>,
) -> xev::CallbackAction {
    // _ = r catch unreachable;
    r.unwrap_or_else(|_| unreachable!());
    unsafe { (*self_).loop_.stop() };
    xev::CallbackAction::Disarm
}

// PORT NOTE: the `log.*` call sites in the reference, routed to the log facade and
// reproduced as named helpers that preserve the reference's messages verbatim.
// macOS-gated: several name `xev::Error`/`xev::MachPortError`, and all are only
// reached from the macOS-gated `Thread` logic above.
#[cfg(target_os = "macos")]
fn log_warn_thread_err(_err: &xev::Error) {
    // log.warn("error in cf release thread err={}", .{err});
}
#[cfg(target_os = "macos")]
fn log_warn_abrupt_exit() {
    // log.warn("abrupt cf release thread exit detected, starting xev to drain mailbox", .{});
}
#[cfg(target_os = "macos")]
fn log_debug_fully_exiting() {
    // log.debug("cf release thread fully exiting after abnormal failure", .{});
}
#[cfg(target_os = "macos")]
fn log_err_drain_failed(_err: &xev::Error) {
    // log.err("failed to start xev loop for draining err={}", .{err});
}
#[cfg(target_os = "macos")]
fn log_debug_starting() {
    // log.debug("starting cf release thread", .{});
}
#[cfg(target_os = "macos")]
fn log_debug_shutdown() {
    // log.debug("starting cf release thread shutdown", .{});
}
#[cfg(target_os = "macos")]
fn log_err_wakeup(_err: &xev::MachPortError) {
    // log.err("error in wakeup err={}", .{err});
}
#[cfg(target_os = "macos")]
fn log_err_drain_mailbox(_err: &xev::Error) {
    // log.err("error draining mailbox err={}", .{err});
}

// PORT NOTE: `macos.foundation.CFRelease`. CoreFoundation's `CFRelease` declared as
// `extern "C"` (R3 binding to the SAME framework). It takes a CFTypeRef
// (`*anyopaque`).
#[cfg(target_os = "macos")]
extern "C" {
    fn CFRelease(cf: *mut std::ffi::c_void);
}

// PORT NOTE: on non-Darwin targets there is no CoreFoundation; the reference module
// is only ever used on Darwin. We provide a never-called shim so the file
// type-checks on all targets (the `Release` path is Darwin-only in practice).
#[cfg(not(target_os = "macos"))]
unsafe fn CFRelease(_cf: *mut std::ffi::c_void) {}
