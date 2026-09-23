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
//!   ([`Fault::AfterIoSpawn`]), so both threads are running; or fail
//!   `termio::Thread::init()` ([`Fault::IoThreadInit`]), the last early return
//!   before the threads spawn (UD115's grid release, task R13-H2);
//! - a log of the spawns, stops, joins and frees on the error path;
//! - a [`ThreadProbe`] moved into each spawned thread, which observes the
//!   thread itself rather than the log;
//! - a guard at every free: if a spawned thread is still running, or has not
//!   been joined, when its state is about to be freed, the probe prints why
//!   and ends the process with `_exit(86)` BEFORE the free, so the violation
//!   is reported without any freed memory being touched. Tests therefore
//!   drive it in a child process.

use std::cell::{Cell, RefCell};
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Where the armed fault fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// The io thread's `Builder::spawn` returns an error (the renderer thread
    /// is already running).
    IoSpawn,
    /// The io thread spawned; the next fallible step (where the `set_title`
    /// `try` sits) returns an error, with both threads running.
    AfterIoSpawn,
    /// `termio::Thread::init()` returns an error: the last early return
    /// before the big write into `self` (UD115, task R13-H2). The renderer
    /// thread is initialised but not yet spawned.
    IoThreadInit,
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

/// The threads [`ThreadProbe`] watches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProbedThread {
    Renderer,
    Io,
}

impl ProbedThread {
    fn name(self) -> &'static str {
        match self {
            ProbedThread::Renderer => "renderer",
            ProbedThread::Io => "io",
        }
    }
}

/// How many probed threads of each kind have been handed to a spawn and whose
/// main has not yet returned. Process-wide, because the count is dropped on
/// the spawned thread; only a recording (armed) init thread adds to it.
static RUNNING: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

/// How long a probed thread lingers after its main returns, before it counts
/// itself as exited. A joined thread is waited for, linger included; a thread
/// that was only stopped and not joined is then still counted at the next
/// free, so the probe does not depend on how fast the thread exits.
const LINGER: std::time::Duration = std::time::Duration::from_millis(200);

/// Observes a spawned thread directly, independently of the log that the code
/// under test writes (task R13-H2: a join replaced by `drop(handle)` still
/// logs `RendererJoined`, but the thread is then still running). Created on
/// the init thread just before the spawn and moved into the thread's closure;
/// it counts the thread as running from creation until it is dropped at the
/// end of the closure (after [`LINGER`]). If the spawn fails, the closure and
/// the probe are dropped inside the spawn call, so the count is back to zero
/// before any free. Inert unless this thread is recording.
pub struct ThreadProbe {
    which: Option<ProbedThread>,
}

impl ThreadProbe {
    pub(crate) fn start(which: ProbedThread) -> ThreadProbe {
        if !RECORDING.get() {
            return ThreadProbe { which: None };
        }
        RUNNING[which as usize].fetch_add(1, Ordering::SeqCst);
        ThreadProbe { which: Some(which) }
    }
}

impl Drop for ThreadProbe {
    fn drop(&mut self) {
        if let Some(which) = self.which {
            std::thread::sleep(LINGER);
            RUNNING[which as usize].fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Called right before the error path frees `what`. A spawned thread that is
/// still running (observed by its [`ThreadProbe`]), or that the log shows
/// spawned but not joined, could still be using it, so report and end the
/// process before the free.
pub(crate) fn before_free(what: &'static str) {
    if !RECORDING.get() {
        return;
    }
    let still_running = [ProbedThread::Renderer, ProbedThread::Io]
        .into_iter()
        .find(|&t| RUNNING[t as usize].load(Ordering::SeqCst) != 0)
        .map(|t| (t.name(), "its main has not returned"));
    let unjoined = || {
        LOG.with_borrow(|log| {
            let has = |e: Event| log.contains(&e);
            [
                (Event::RendererSpawned, Event::RendererJoined, "renderer"),
                (Event::IoSpawned, Event::IoJoined, "io"),
            ]
            .into_iter()
            .find(|&(spawned, joined, _)| has(spawned) && !has(joined))
            .map(|(_, _, name)| (name, "spawned, not joined"))
        })
    };
    if let Some((name, why)) = still_running.or_else(unjoined) {
        let mut err = std::io::stderr();
        let _ = writeln!(
            err,
            "UD104 probe: `{what}` is about to be freed while the {name} thread is \
             still running ({why}); exiting before the free"
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
