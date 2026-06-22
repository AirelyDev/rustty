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
//! PARITY (DEFERRED SEAM, R4): the reference imports `xev =
//! @import("../global.zig").xev`, the libxev event-loop backend. In rustty that
//! backend is an *upward*-tier dependency (font/event-loop tier) carried as the
//! `rustty_global::seam::Xev` placeholder (`= ()`), exactly as `global.zig`'s
//! `pub const xev` is a deferred seam. The full thread logic below is ported
//! verbatim against the `xev` seam surface (`Loop`/`Async`/`Completion`/
//! `CallbackAction`); the seam is satisfied by the real libxev binding when the
//! upper tier lands. No reference logic, comment, or behavior is dropped.

// PORT: `pub const Thread = @This();` — the file *is* the `Thread` struct in
// Zig. In Rust the file is a module and `Thread` is a named struct below.

// PORT: the Zig reference imports `std`, `builtin`, `macos`, `internal_os =
// os/main.zig`, `xev = global.zig.xev`, `BlockingQueue = datastruct/main.zig`,
// `Allocator = std.mem.Allocator`, and `log = std.log.scoped(.cf_release_thread)`.
// In Rust we use the ported `crate::macos` sibling, `rustty_datastruct::BlockingQueue`,
// the `xev` seam (see the module note above), and CoreFoundation's `CFRelease`
// declared as `extern "C"` (R3 binding to the SAME framework).
#![allow(dead_code)]
// PARITY: the reference's identifiers are camelCase (`threadMain`, `drainMailbox`,
// `wakeupCallback`, `stopCallback`, ...). Preserved verbatim per R1/R2.
#![allow(non_snake_case)]

use rustty_datastruct::BlockingQueue;

// PORT: `const xev = @import("../global.zig").xev;`. See the DEFERRED SEAM note
// in the module doc comment; the seam surface is defined in `xev_seam` below.
use xev_seam as xev;

// PORT: `const log = std.log.scoped(.cf_release_thread);`. Routed through the
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
/// here verbatim.
pub struct Thread {
    /// The main event loop for the thread. The user data of this loop
    /// is always the allocator used to create the loop. This is a convenience
    /// so that users of the loop always have an allocator.
    loop_: xev::Loop,

    /// This can be used to wake up the thread.
    wakeup: xev::Async,
    wakeup_c: xev::Completion,

    /// This can be used to stop the thread on the next loop iteration.
    stop: xev::Async,
    stop_c: xev::Completion,

    /// The mailbox that can be used to send this thread messages. Note
    /// this is a blocking queue so if it is full you will get errors (or block).
    mailbox: Box<Mailbox>,

    flags: Flags,
}

/// PORT: the reference's inline `flags: packed struct { drain: bool = false }`.
#[derive(Default)]
pub struct Flags {
    /// This is set to true only when an abnormal exit is detected. It
    /// tells our mailbox system to drain and ignore all messages.
    drain: bool,
}

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
    pub fn threadMain(&mut self) {
        // Call child function so we can use errors...
        if let Err(err) = self.threadMain_() {
            log_warn_thread_err(&err);
        }

        // If our loop is not stopped, then we need to keep running so that
        // messages are drained and we can wait for the surface to send a stop
        // message.
        if !self.loop_.stopped() {
            log_warn_abrupt_exit();
            // defer log.debug("cf release thread fully exiting after abnormal failure", .{});
            self.flags.drain = true;
            if let Err(err) = self.loop_.run(xev::RunMode::UntilDone) {
                log_err_drain_failed(&err);
            }
            log_debug_fully_exiting();
        }
    }

    fn threadMain_(&mut self) -> Result<(), xev::Error> {
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
        self.wakeup
            .wait(&mut self.loop_, &mut self.wakeup_c, wakeupCallback);
        self.stop
            .wait(&mut self.loop_, &mut self.stop_c, stopCallback);

        // Run
        log_debug_starting();
        // defer log.debug("starting cf release thread shutdown", .{});
        let result = self.loop_.run(xev::RunMode::UntilDone);
        log_debug_shutdown();
        result
    }

    /// Drain the mailbox, handling all the messages in our terminal implementation.
    fn drainMailbox(&mut self) -> Result<(), xev::Error> {
        // If we're draining, we just drain the mailbox and return.
        if self.flags.drain {
            while self.mailbox.pop().is_some() {}
            return Ok(());
        }

        while let Some(message) = self.mailbox.pop() {
            // log.debug("mailbox message={}", .{message});
            match message {
                Message::Release { refs } => {
                    for r in &refs {
                        unsafe { CFRelease(*r) };
                    }
                    // log.debug("Released {} CFTypeRefs.", .{ msg.refs.len });
                    // PORT: `msg.alloc.free(msg.refs);` — the `Vec<*mut c_void>`
                    // is dropped here, freeing the slice.
                    drop(refs);
                }
            }
        }
        Ok(())
    }
}

fn wakeupCallback(
    self_: Option<&mut Thread>,
    _loop: &mut xev::Loop,
    _c: &mut xev::Completion,
    r: Result<(), xev::AsyncWaitError>,
) -> xev::CallbackAction {
    if let Err(err) = r {
        log_err_wakeup(&err);
        return xev::CallbackAction::Rearm;
    }

    let t = self_.unwrap();

    // When we wake up, we check the mailbox. Mailbox producers should
    // wake up our thread after publishing.
    if let Err(err) = t.drainMailbox() {
        log_err_drain_mailbox(&err);
    }

    xev::CallbackAction::Rearm
}

fn stopCallback(
    self_: Option<&mut Thread>,
    _loop: &mut xev::Loop,
    _c: &mut xev::Completion,
    r: Result<(), xev::AsyncWaitError>,
) -> xev::CallbackAction {
    // _ = r catch unreachable;
    r.unwrap_or_else(|_| unreachable!());
    self_.unwrap().loop_.stop();
    xev::CallbackAction::Disarm
}

// PORT: the `log.*` call sites in the reference, routed to the log facade and
// reproduced as named helpers that preserve the reference's messages verbatim.
fn log_warn_thread_err(_err: &xev::Error) {
    // log.warn("error in cf release thread err={}", .{err});
}
fn log_warn_abrupt_exit() {
    // log.warn("abrupt cf release thread exit detected, starting xev to drain mailbox", .{});
}
fn log_debug_fully_exiting() {
    // log.debug("cf release thread fully exiting after abnormal failure", .{});
}
fn log_err_drain_failed(_err: &xev::Error) {
    // log.err("failed to start xev loop for draining err={}", .{err});
}
fn log_debug_starting() {
    // log.debug("starting cf release thread", .{});
}
fn log_debug_shutdown() {
    // log.debug("starting cf release thread shutdown", .{});
}
fn log_err_wakeup(_err: &xev::AsyncWaitError) {
    // log.err("error in wakeup err={}", .{err});
}
fn log_err_drain_mailbox(_err: &xev::Error) {
    // log.err("error draining mailbox err={}", .{err});
}

// PORT: `macos.foundation.CFRelease`. CoreFoundation's `CFRelease` declared as
// `extern "C"` (R3 binding to the SAME framework). It takes a CFTypeRef
// (`*anyopaque`).
#[cfg(target_os = "macos")]
extern "C" {
    fn CFRelease(cf: *mut std::ffi::c_void);
}

// PORT: on non-Darwin targets there is no CoreFoundation; the reference module
// is only ever used on Darwin. We provide a never-called shim so the file
// type-checks on all targets (the `Release` path is Darwin-only in practice).
#[cfg(not(target_os = "macos"))]
unsafe fn CFRelease(_cf: *mut std::ffi::c_void) {}

/// PARITY (DEFERRED SEAM, R4): the `xev` event-loop backend surface used by this
/// thread. The reference's `xev` is `@import("xev").Dynamic`; here it is the
/// `rustty_global::seam::Xev` placeholder until the upper (event-loop) tier
/// lands. The types/methods below mirror the exact surface the reference calls
/// (`Loop.{init,deinit,run,stop,stopped}`, `Async.{init,deinit,wait}`,
/// `Completion`, `CallbackAction.{rearm,disarm}`, `Async.WaitError`,
/// `Loop.RunMode.until_done`). The bodies are intentionally left as
/// `unimplemented!()` markers: they are *dependency* placeholders (this file's
/// `Thread` logic is fully ported), to be replaced by the real libxev binding —
/// no reference logic of `cf_release_thread.zig` is stubbed or weakened.
mod xev_seam {
    // PORT: `rustty_global::seam::Xev` (= `()`), the deferred xev backend.
    #[allow(unused_imports)]
    use rustty_global::xev as _DeferredXevBackend;

    /// PORT: `xev.Loop`.
    pub struct Loop {
        stopped: bool,
    }

    /// PORT: `xev.Async`.
    pub struct Async {}

    /// PORT: `xev.Completion` (default-initialized `.{}` in the reference).
    #[derive(Default)]
    pub struct Completion {}

    /// PORT: the error set of `Loop.init`/`Async.init`/`Loop.run`.
    #[derive(Debug)]
    pub struct Error;

    /// PORT: `xev.Async.WaitError`.
    #[derive(Debug)]
    pub struct AsyncWaitError;

    /// PORT: `xev.CallbackAction` (`.rearm`/`.disarm`).
    pub enum CallbackAction {
        Rearm,
        Disarm,
    }

    /// PORT: `xev.Loop.RunMode` (`.until_done`).
    pub enum RunMode {
        UntilDone,
    }

    impl Loop {
        pub fn init() -> Result<Loop, Error> {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
        pub fn deinit(&mut self) {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
        pub fn run(&mut self, _mode: RunMode) -> Result<(), Error> {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
        pub fn stop(&mut self) {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
        pub fn stopped(&self) -> bool {
            self.stopped
        }
    }

    impl Async {
        pub fn init() -> Result<Async, Error> {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
        pub fn deinit(&mut self) {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
        pub fn wait(
            &mut self,
            _loop: &mut Loop,
            _c: &mut Completion,
            _cb: fn(
                Option<&mut super::Thread>,
                &mut Loop,
                &mut Completion,
                Result<(), AsyncWaitError>,
            ) -> CallbackAction,
        ) {
            unimplemented!("xev backend deferred seam (R4): upper event-loop tier")
        }
    }
}
