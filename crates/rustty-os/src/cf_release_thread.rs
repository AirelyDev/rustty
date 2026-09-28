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

// PORT NOTE: `const log = std.log.scoped(.cf_release_thread);`. The reference's
// scope tag, carried through the `log` facade as `target: LOG_SCOPE`
// (DECISION-logging.md §6r4).
//
// macOS-gated for the same reason the `Thread` logic below is: every call site
// is inside it.
#[cfg(target_os = "macos")]
const LOG_SCOPE: &str = "cf_release_thread";

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
        let loop_ = xev::Loop::init(xev::Options::default())?;
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
            // log.warn("error in cf release thread err={}", .{err});
            //
            // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
            // `{err:?}` renders the Rust error's own Debug. Workspace-dominant
            // rendering; N11's scope, not this site's.
            log::warn!(target: LOG_SCOPE, "error in cf release thread err={err:?}");
        }

        // If our loop is not stopped, then we need to keep running so that
        // messages are drained and we can wait for the surface to send a stop
        // message.
        if !unsafe { (*self_).loop_.stopped() } {
            // log.warn("abrupt cf release thread exit detected, starting xev to drain mailbox", .{});
            log::warn!(
                target: LOG_SCOPE,
                "abrupt cf release thread exit detected, starting xev to drain mailbox"
            );
            // defer log.debug("cf release thread fully exiting after abnormal failure", .{});
            //   (fires at the END of this block — written inline below.)
            unsafe { (*self_).flags.drain = true };
            if let Err(err) = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) } {
                // log.err("failed to start xev loop for draining err={}", .{err});
                log::error!(target: LOG_SCOPE, "failed to start xev loop for draining err={err:?}");
            }
            // log.debug("cf release thread fully exiting after abnormal failure", .{});
            log::debug!(
                target: LOG_SCOPE,
                "cf release thread fully exiting after abnormal failure"
            );
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
        // log.debug("starting cf release thread", .{});
        log::debug!(target: LOG_SCOPE, "starting cf release thread");
        // defer log.debug("starting cf release thread shutdown", .{});
        let result = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) };
        // log.debug("starting cf release thread shutdown", .{});
        log::debug!(target: LOG_SCOPE, "starting cf release thread shutdown");
        // defer log.debug("cf release thread exited", .{});
        //
        // PORT NOTE: registered FIRST in the reference, so Zig's LIFO order
        // fires it LAST — after the shutdown line above, on both the success
        // and the error return.
        log::debug!(target: LOG_SCOPE, "cf release thread exited");
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
        // log.err("error in wakeup err={}", .{err});
        log::error!(target: LOG_SCOPE, "error in wakeup err={err:?}");
        return xev::CallbackAction::Rearm;
    }

    // const t = self_.?;  -> the reference unwraps the optional; our UD is
    // non-null by construction (the loop userdata IS the Thread).

    // When we wake up, we check the mailbox. Mailbox producers should
    // wake up our thread after publishing.
    if let Err(err) = unsafe { Thread::drainMailbox(self_) } {
        // log.err("error draining mailbox err={}", .{err});
        log::error!(target: LOG_SCOPE, "error draining mailbox err={err:?}");
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

// PORT NOTE (task-14e-2, S111-F4): the eight EMPTY `log_*` helpers that used to
// stand here are gone. A named, empty-bodied helper reads as an emitter and is
// exactly as silent as no call at all — `check_log_reconcile.py` counts such a
// call site as a DROPPED diagnostic, which is the correct reading. Each of the
// reference's NINE calls is now emitted inline where the reference emits it
// (the ninth, `defer log.debug("cf release thread exited")`, had no helper at
// all — only a comment), with the reference call preserved above it.

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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    //! task-14e-2 / S111-F4 — `os/cf_release_thread.zig`'s nine diagnostics.
    use super::*;

    /// Runs the REAL xev loop on THIS thread (the same shape as
    /// `tests/cf_release_live.rs`, which is an integration test and so cannot
    /// reach the crate-private capture logger), and pins the three debug lines
    /// a CLEAN run emits — in the reference's order — plus the absence of the
    /// six failure lines.
    ///
    /// The ORDER is the point. `threadMain_` registers
    /// `defer log.debug("cf release thread exited")` FIRST and
    /// `defer log.debug("starting cf release thread shutdown")` LAST, so Zig's
    /// LIFO fires "shutdown" and then "exited". Writing both defers as one
    /// mechanism, or in source order, reverses them.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-os, parity tail C-1): calls the foreign function `kqueue`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn a_clean_run_emits_the_three_debug_lines_in_the_reference_order() {
        let cap = crate::log_capture::start();
        let mut t = Thread::init().expect("Thread::init on real xev");
        let stop = t.stop;

        let notifier = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            stop.notify().expect("stop.notify");
        });

        // Runs on THIS thread, so its records are admitted by the capture.
        t.threadMain();
        notifier.join().expect("notifier join");

        let messages = cap.messages();
        drop(cap);
        t.deinit();

        assert_eq!(
            vec![
                (
                    log::Level::Debug,
                    "cf_release_thread".to_string(),
                    "starting cf release thread".to_string(),
                ),
                (
                    log::Level::Debug,
                    "cf_release_thread".to_string(),
                    "starting cf release thread shutdown".to_string(),
                ),
                (
                    log::Level::Debug,
                    "cf_release_thread".to_string(),
                    "cf release thread exited".to_string(),
                ),
            ],
            messages
        );
    }

    /// task-14e-2 / S111-F4 — the six diagnostics a CLEAN run cannot reach.
    ///
    /// STRUCTURAL, and reported as such. Each hangs off a real `xev` failure —
    /// `Loop::run` returning an error, an `Async::wait` completion reporting
    /// `MachPortError`, `drainMailbox` failing — none of which a test can
    /// provoke on a live kqueue loop without an injection seam this file does
    /// not have. Level, scope and message text are pinned from the source; the
    /// macro tokens are ASSEMBLED FROM FRAGMENTS so no live-looking facade call
    /// enters this file.
    #[test]
    fn the_six_failure_diagnostics_are_live_at_the_reference_level_and_scope() {
        const SRC: &str = include_str!("cf_release_thread.rs");
        let src = crate::log_capture::collapse_ws(SRC);

        assert_eq!(
            1,
            src.matches("const LOG_SCOPE: &str = \"cf_release_thread\";")
                .count(),
            "the reference's scope tag is not declared exactly once"
        );

        for (level, message) in [
            // log.warn("error in cf release thread err={}", .{err});
            ("warn", "error in cf release thread err={err:?}"),
            // log.warn("abrupt cf release thread exit detected, starting xev to drain mailbox", .{});
            (
                "warn",
                "abrupt cf release thread exit detected, starting xev to drain mailbox",
            ),
            // log.err("failed to start xev loop for draining err={}", .{err});
            ("error", "failed to start xev loop for draining err={err:?}"),
            // log.debug("cf release thread fully exiting after abnormal failure", .{});
            (
                "debug",
                "cf release thread fully exiting after abnormal failure",
            ),
            // log.err("error in wakeup err={}", .{err});
            ("error", "error in wakeup err={err:?}"),
            // log.err("error draining mailbox err={}", .{err});
            ("error", "error draining mailbox err={err:?}"),
        ] {
            let wanted = crate::log_capture::emission_fragment(level, message);
            assert_eq!(
                1,
                src.matches(&wanted).count(),
                "expected exactly one live emission `{wanted}`"
            );
        }
    }

    /// Guards the absence assertion in the clean-run test above: prove the
    /// capture would have SEEN a `cf_release_thread`-scoped record emitted on
    /// this thread, so the empty tail of that vector means "not emitted" and
    /// not "not observable".
    ///
    /// The probe emission lives in `log_capture` rather than here on purpose.
    /// `check_log_reconcile.py` counts every `log::` macro in a MAPPED port
    /// file as an emission, TEST call sites included — a probe written inline
    /// credited `src/os/cf_release_thread.zig`'s arm with a tenth emission
    /// against nine reference sites, which would have masked a future dropped
    /// site behind a phantom. `log_capture.rs` is in no portmap arm.
    #[test]
    fn the_capture_observes_this_scope_on_this_thread() {
        let cap = crate::log_capture::start();
        crate::log_capture::emit_probe(LOG_SCOPE, "probe");
        let messages = cap.messages();
        drop(cap);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "cf_release_thread".to_string(),
                "probe".to_string(),
            )],
            messages
        );
    }
}
