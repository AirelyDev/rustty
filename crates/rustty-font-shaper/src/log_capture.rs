//! TEST-ONLY capture logger for `rustty-font-shaper`'s `LOG`-row restorations
//! (task-14c-4).
//!
//! This is a deliberate, near-verbatim SIBLING of
//! `crates/rustty-vt/src/log_capture.rs` (task-14r-a fix-review r1) and
//! `crates/rustty-renderer/src/log_capture.rs` (task-14c-1), down to the
//! admission-control mechanism and its two self-tests. The three crates cannot
//! share one module — the helper is `#[cfg(test)]` in each, so it is not part of
//! any crate's published surface — and the alternative (a third, differently
//! shaped answer to the same problem) is worse than the duplication. Keep them
//! in step: a fix to one belongs in all three.
//!
//! `rustty-font-shaper` sits below `rustty-vt-c`/`rustty-core` in R4's acyclic
//! layering (DECISION-logging.md §6r IMPORTANT 5, call shape 3), so its restored
//! `log::{warn,info,error,debug}!` sites reach whichever process-wide `log::Log`
//! backend the running BINARY installs (`rustty-vt-c::sys::Logger`,
//! `rustty-core::logging::AppLogger`, or — in the wasm artifacts —
//! `rustty-os::wasm::log::WasmLogger`, which `face::web_canvas::
//! install_wasm_log_backend` registers). None of those backends exists in
//! `cargo test -p rustty-font-shaper`'s own test binary, so proving a restored
//! site actually emits requires this crate's OWN minimal `log::Log`
//! implementation, installed for the test binary only (`#[cfg(test)]`, never
//! compiled into the production library).
//!
//! # Why there is exactly one logger, and why it admits only one thread
//!
//! `log::set_logger` succeeds **once per process** (`log`'s own documented
//! contract). `cargo test` runs every `#[test]` fn in this crate in ONE process
//! (parallel by default), so if each test installed its own logger, only the
//! first call would win and every other test's install would silently return
//! `Err` and observe nothing — the exact hazard the pattern-fixing rounds in
//! DECISION-logging.md (§6r CRITICAL 1) found and fixed for the production
//! backends. Avoided here the same way: install exactly ONE logger for the whole
//! test binary (`std::sync::Once`).
//!
//! One global logger, however, is fed by EVERY thread in the test binary, not
//! just the test that wants to observe something. `TEST_SERIAL` serializes the
//! tests that call [`start`] against each other, but it cannot serialize them
//! against every other test in the binary that drives a restored call site
//! without touching this module. `rustty-vt` measured that hazard directly —
//! `cargo test -p rustty-vt --lib stream` failed 6 runs out of 8, with polluting
//! records carrying the SAME `target` as the ones under test, so filtering by
//! target could not have separated them either. This crate has the same shape
//! (task-14c-4 makes ~30 sites live, several of them on paths other tests in
//! this crate drive — `collection.rs` and `shared_grid_set.rs` both have large
//! existing test modules) and adopts the same remedy up front.
//!
//! **The fix is admission control at the point of capture, not filtering at the
//! point of read.** The buffer records the [`ThreadId`] that owns the active
//! capture, and [`CaptureLogger::log`] pushes a record ONLY when it is called on
//! that thread. Records from every other thread are dropped on the floor, so
//! they can neither pollute an assertion nor accumulate in the buffer over the
//! life of the test binary. Because [`start`] holds `TEST_SERIAL` for the
//! lifetime of the guard, at most one thread is ever the owner.
//!
//! # The one hole, stated explicitly
//!
//! A capture observes only records emitted **on the thread that called
//! [`start`]**. If a test under capture spawns a worker thread and the code
//! under test logs from that worker, the capture will see nothing. That is a
//! deliberate trade: it is the same property that makes the isolation total,
//! and no current caller needs the other behaviour. This crate DOES contain
//! genuinely multi-threaded code — `shaper/coretext.rs`'s CF release thread,
//! whose restored `error notifying cf release thread to stop` site fires on the
//! shaper's own thread but whose pool draining happens on the release thread —
//! so the hazard is real, not hypothetical. A test that needs to observe a
//! spawned thread's records must extend this module; it must NOT simply loosen
//! its assertions.
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use std::thread::ThreadId;

/// One captured record: `(level, scope/target, formatted message)`.
pub(crate) type CapturedRecord = (log::Level, String, String);

static INSTALL: Once = Once::new();
static STATE: OnceLock<Mutex<CaptureState>> = OnceLock::new();
/// Serializes access to the shared buffer across concurrently-running tests —
/// held for the lifetime of the returned guard, so a test's install-log/
/// trigger/assert/drain sequence is atomic with respect to every other test
/// using this capture logger. It is also what guarantees `CaptureState::owner`
/// can only ever name one thread at a time.
static TEST_SERIAL: Mutex<()> = Mutex::new(());

/// The shared capture buffer plus the thread allowed to write into it.
///
/// `owner` is `None` whenever no [`Capture`] guard is alive, which makes the
/// logger a no-op for the whole binary outside a capture window.
struct CaptureState {
    owner: Option<ThreadId>,
    records: Vec<CapturedRecord>,
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let mut state = lock_state();
        // Admission control: only the thread that owns the live capture may
        // append. See this module's header for why filtering here rather than
        // in `messages()` is what actually makes the capture isolated.
        if state.owner != Some(std::thread::current().id()) {
            return;
        }
        state.records.push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }

    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

/// Locks the shared state, tolerating poisoning.
///
/// A test asserting on captured records panics while holding NO lock (the guard
/// is released before `assert_eq!` runs), so poisoning should not happen — but
/// if some future caller does poison it, one failing test must not cascade into
/// every other test in the binary. Same tolerance `start` already applies to
/// `TEST_SERIAL`.
fn lock_state() -> MutexGuard<'static, CaptureState> {
    STATE
        .get_or_init(|| {
            Mutex::new(CaptureState {
                owner: None,
                records: Vec::new(),
            })
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Installs the capture logger for the test binary (idempotent — the `Once`
/// guard means only the very first call in the process actually registers it;
/// every later call is a no-op, matching the pattern the two production
/// backends already use).
fn install() {
    INSTALL.call_once(|| {
        log::set_logger(&LOGGER)
            .expect("no other logger installed in the rustty-renderer test binary");
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// RAII guard returned by [`start`]: holds the serialization lock, owns the
/// capture for the calling thread, and on drop releases the ownership and
/// clears the buffer — so a test's captured records never leak into the next
/// test that acquires the guard.
pub(crate) struct Capture {
    _serial: MutexGuard<'static, ()>,
}

impl Capture {
    /// Returns every record captured on THIS thread since this guard was
    /// created. See the module header for the spawned-thread caveat.
    pub(crate) fn messages(&self) -> Vec<CapturedRecord> {
        lock_state().records.clone()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let mut state = lock_state();
        state.owner = None;
        state.records.clear();
    }
}

/// Installs the capture logger (once per process), takes the serialization
/// lock, and claims the capture for the calling thread, returning a guard with
/// a freshly-cleared buffer. Call at the top of every test that exercises a
/// restored `log::` call site.
pub(crate) fn start() -> Capture {
    install();
    let serial = TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = lock_state();
    state.owner = Some(std::thread::current().id());
    // Clear any records left by a prior test in case that test panicked before
    // its own `Capture` guard could `Drop::drop` and clear the buffer.
    state.records.clear();
    drop(state);
    Capture { _serial: serial }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression this module's admission control exists for: a record
    /// emitted on ANOTHER thread, with the SAME target as the one under test,
    /// must not enter the capture. Without it this test's assertion would see
    /// two records instead of one — which is exactly how the three `stream.rs`
    /// task-14r-a tests failed intermittently under
    /// `cargo test -p rustty-vt --lib stream` before the sibling module was
    /// fixed.
    #[test]
    fn capture_admits_only_the_owning_thread() {
        let cap = start();
        log::warn!(target: "font_face", "mine");

        std::thread::scope(|s| {
            s.spawn(|| {
                log::warn!(target: "font_face", "foreign");
            });
        });

        let messages = cap.messages();
        drop(cap);
        assert_eq!(
            vec![(log::Level::Warn, "font_face".to_string(), "mine".to_string())],
            messages
        );
    }

    /// Outside a capture window the logger must drop everything, so foreign
    /// records cannot accumulate in the buffer over the life of the test
    /// binary and cannot be observed by the NEXT capture.
    #[test]
    fn records_emitted_outside_a_capture_are_discarded() {
        log::warn!(target: "font_face", "before any capture");
        let cap = start();
        let messages = cap.messages();
        drop(cap);
        assert_eq!(Vec::<CapturedRecord>::new(), messages);
    }
}
