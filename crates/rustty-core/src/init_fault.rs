//! Test-only fault injection and teardown-order probe for `Surface::init`'s
//! post-spawn error path (UD104, task R13-D2). Rust-only; no reference
//! counterpart.
//!
//! Compiled only with the `init-fault-inject` feature, which only
//! rustty-apprt-c's `[dev-dependencies]` enable. With `resolver = "2"` a
//! dev-dependency feature is not activated by `cargo build -p rustty`, `cargo
//! build --workspace`, or the release and xcframework builds; it is unified
//! only into invocations that build test targets. Even there it does nothing
//! unless a test arms it: every hook checks a thread-local that only [`arm`]
//! sets, and `Surface::init` runs on the calling thread, so arming affects
//! only the test that armed it (parallel tests in the same binary are not
//! touched).
//!
//! What it offers:
//! - a fault: fail the io thread's spawn ([`Fault::IoSpawn`]), or fail the
//!   first fallible step after it, where the `set_title` `try` stands
//!   ([`Fault::AfterIoSpawn`]), so both threads are running;
//! - a log of the spawns, stops, joins and frees on the error path;
//! - a guard at every free: if a spawned thread has not been joined when its
//!   state is about to be freed, the probe prints why and ends the process
//!   with `_exit(86)` BEFORE the free, so the violation is reported without
//!   any freed memory being touched. Tests therefore drive it in a child
//!   process.

use std::cell::{Cell, RefCell};
use std::io::Write as _;

/// Where the armed fault fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// The io thread's `Builder::spawn` returns an error (the renderer thread
    /// is already running).
    IoSpawn,
    /// The io thread spawned; the next fallible step (where the `set_title`
    /// `try` sits) returns an error, with both threads running.
    AfterIoSpawn,
}

/// One step of the error path, in the order it happened on the init thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    RendererSpawned,
    IoSpawned,
    Injected(Fault),
    RendererStopped,
    RendererJoined,
    IoStopped,
    IoJoined,
    /// A piece of state the threads use was freed (after the guard passed).
    Freed(&'static str),
}

/// The exit status of a child whose state was about to be freed under a
/// running thread.
pub const VIOLATION_EXIT: i32 = 86;

thread_local! {
    static ARMED: Cell<Option<Fault>> = const { Cell::new(None) };
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static LOG: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

/// Arm `fault` for the next `Surface::init` on this thread and start
/// recording.
pub fn arm(fault: Fault) {
    LOG.with_borrow_mut(Vec::clear);
    ARMED.set(Some(fault));
    RECORDING.set(true);
}

/// Disarm, stop recording, and return what was recorded.
pub fn take_log() -> Vec<Event> {
    ARMED.set(None);
    RECORDING.set(false);
    LOG.take()
}

/// `true` (once) when `at` is the armed fault on this thread.
pub(crate) fn inject(at: Fault) -> bool {
    if ARMED.get() == Some(at) {
        ARMED.set(None);
        record(Event::Injected(at));
        true
    } else {
        false
    }
}

pub(crate) fn record(event: Event) {
    if RECORDING.get() {
        LOG.with_borrow_mut(|log| log.push(event));
    }
}

/// Called right before the error path frees `what`. A spawned thread that has
/// not been joined could still be using it, so report and end the process
/// before the free.
pub(crate) fn before_free(what: &'static str) {
    if !RECORDING.get() {
        return;
    }
    let running = LOG.with_borrow(|log| {
        let has = |e: Event| log.contains(&e);
        [
            (Event::RendererSpawned, Event::RendererJoined, "renderer"),
            (Event::IoSpawned, Event::IoJoined, "io"),
        ]
        .into_iter()
        .find(|&(spawned, joined, _)| has(spawned) && !has(joined))
        .map(|(_, _, name)| name)
    });
    if let Some(name) = running {
        let mut err = std::io::stderr();
        let _ = writeln!(
            err,
            "UD104 probe: `{what}` is about to be freed while the {name} thread is \
             still running (spawned, not joined); exiting before the free"
        );
        let _ = err.flush();
        extern "C" {
            fn _exit(status: core::ffi::c_int) -> !;
        }
        // SAFETY: `_exit` ends the process at once, without running any
        // handler that could touch the state the thread is still using.
        unsafe { _exit(VIOLATION_EXIT) };
    }
    record(Event::Freed(what));
}
