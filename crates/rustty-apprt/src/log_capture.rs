//! TEST-ONLY capture logger for `rustty-apprt`'s `LOG`-row restorations
//! (stage 14d-3).
//!
//! This is a deliberate, near-verbatim SIBLING of
//! `crates/rustty-vt/src/log_capture.rs`,
//! `crates/rustty-renderer/src/log_capture.rs` and
//! `crates/rustty-core/src/log_capture.rs`, down to the admission-control
//! mechanism and its self-tests. The crates cannot share one module — the
//! helper is `#[cfg(test)]` in each of them and so is not part of any published
//! surface — and the alternative (a differently-shaped answer to the same
//! problem per crate) is worse than the duplication. Keep them in step: a fix
//! to one belongs in all four. (The controller intends to converge them into
//! `rustty-util`; this copy follows the existing shape precisely so that
//! convergence is a move, not a merge.)
//!
//! # Why this crate needs one
//!
//! Stage 14d-3 restores `src/apprt/embedded.zig`'s and the whole GTK tree's
//! diagnostics as `log::{error,warn,info,debug}!(target: LOG_SCOPE, ...)`. The
//! production backend for this crate is `rustty-core`'s `AppLogger`, which is a
//! WRITE-ONLY sink (straight to stderr / macOS unified logging, no injectable
//! capture point), so proving a restored site emits — with the right LEVEL,
//! SCOPE and MESSAGE, which is exactly what rows S54-F6/S58-F6/S59-F6 are about
//! — needs a capturing `log::Log` of our own, installed for the test binary
//! only.
//!
//! # Why there is exactly one logger, and why it admits only one thread
//!
//! `log::set_logger` succeeds **once per process** (`log`'s own documented
//! contract). `cargo test` runs every `#[test]` fn in this crate in ONE process
//! (parallel by default), so if each test installed its own logger, only the
//! first call would win and every other test's install would silently return
//! `Err` and observe nothing — the exact hazard the pattern-fixing rounds in
//! DECISION-logging.md (§6r CRITICAL 1) found and fixed for the two production
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
//! target could not have separated them either.
//!
//! **The fix is admission control at the point of capture, not filtering at the
//! point of read.** The buffer records who owns the active capture, and
//! [`CaptureLogger::log`] pushes a record ONLY when the emitting thread is
//! admitted. Records from every other thread are dropped on the floor, so they
//! can neither pollute an assertion nor accumulate in the buffer over the life
//! of the test binary. Because [`start`] holds `TEST_SERIAL` for the lifetime of
//! the guard, at most one capture is ever active.
//!
//! # The spawned-thread hole
//!
//! A [`start`] capture observes only records emitted **on the thread that called
//! it**. [`start_admitting_all_threads`] admits records from every thread for the
//! lifetime of the guard and, being serialized by the same `TEST_SERIAL`, is safe
//! against other capture users; the trade it accepts is that a NON-capture test
//! running concurrently can pollute the buffer, so a test using it must be
//! written to tolerate that. Prefer [`start`] whenever the emission is on the
//! test's own thread — in this crate it always is, because every GTK callback a
//! test drives runs on the calling thread.
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
/// can only ever name one capture at a time.
static TEST_SERIAL: Mutex<()> = Mutex::new(());

/// Who may write into the capture buffer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Admit {
    /// Only the thread that created the guard (the default; total isolation).
    Thread(ThreadId),
    /// Every thread (see [`start_admitting_all_threads`]).
    AllThreads,
}

/// The shared capture buffer plus the admission rule for writing into it.
///
/// `owner` is `None` whenever no [`Capture`] guard is alive, which makes the
/// logger a no-op for the whole binary outside a capture window.
struct CaptureState {
    owner: Option<Admit>,
    records: Vec<CapturedRecord>,
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let mut state = lock_state();
        // Admission control: only an admitted thread may append. See this
        // module's header for why filtering here rather than in `messages()`
        // is what actually makes the capture isolated.
        match state.owner {
            Some(Admit::AllThreads) => {}
            Some(Admit::Thread(id)) if id == std::thread::current().id() => {}
            _ => return,
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
///
pub(crate) fn install() {
    INSTALL.call_once(|| {
        log::set_logger(&LOGGER)
            .expect("no other logger installed in the rustty-apprt test binary");
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// RAII guard returned by [`start`]: holds the serialization lock, owns the
/// capture, and on drop releases the ownership and clears the buffer — so a
/// test's captured records never leak into the next test that acquires the
/// guard.
pub(crate) struct Capture {
    _serial: MutexGuard<'static, ()>,
}

impl Capture {
    /// Returns every record captured since this guard was created. See the
    /// module header for the spawned-thread caveat.
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
/// restored `log::` call site on its own thread.
pub(crate) fn start() -> Capture {
    begin(Admit::Thread(std::thread::current().id()))
}

/// Like [`start`], but admits records emitted on ANY thread.
///
/// Read the module header's last section before reaching for this: it trades
/// total isolation for cross-thread visibility, and the assertions written
/// against it must account for that rather than being loosened.
pub(crate) fn start_admitting_all_threads() -> Capture {
    begin(Admit::AllThreads)
}

fn begin(admit: Admit) -> Capture {
    install();
    let serial = TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = lock_state();
    state.owner = Some(admit);
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
    /// must not enter a [`start`] capture.
    #[test]
    fn capture_admits_only_the_owning_thread() {
        let cap = start();
        log::warn!(target: "gtk_rustty_surface", "mine");

        std::thread::scope(|s| {
            s.spawn(|| {
                log::warn!(target: "gtk_rustty_surface", "foreign");
            });
        });

        let messages = cap.messages();
        drop(cap);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "gtk_rustty_surface".to_string(),
                "mine".to_string()
            )],
            messages
        );
    }

    /// The escape hatch's contract, stated as a test: under
    /// [`start_admitting_all_threads`] the SAME spawned-thread record that
    /// `capture_admits_only_the_owning_thread` proves is dropped is now
    /// captured, in emission order.
    ///
    /// The projection to `SELFTEST_TARGET` is the documented cost of this
    /// entry point, not a loosened assertion: admitting every thread also
    /// admits records from tests running concurrently in this binary (the
    /// first draft of this test caught five of them from `app.rs`'s own
    /// restored sites). The assertion below is still a FULL-VECTOR
    /// `assert_eq!` over everything this test is responsible for — nothing
    /// it emits can go missing, appear out of order, or arrive with the
    /// wrong level and still pass.
    #[test]
    fn all_threads_capture_admits_a_spawned_thread() {
        const SELFTEST_TARGET: &str = "task14d3_all_threads_selftest";

        let cap = start_admitting_all_threads();
        log::warn!(target: SELFTEST_TARGET, "mine");

        std::thread::scope(|s| {
            s.spawn(|| {
                log::warn!(target: SELFTEST_TARGET, "foreign");
            });
        });

        let messages = cap.messages();
        drop(cap);
        let mine: Vec<CapturedRecord> = messages
            .into_iter()
            .filter(|(_, target, _)| target == SELFTEST_TARGET)
            .collect();
        assert_eq!(
            vec![
                (
                    log::Level::Warn,
                    SELFTEST_TARGET.to_string(),
                    "mine".to_string()
                ),
                (
                    log::Level::Warn,
                    SELFTEST_TARGET.to_string(),
                    "foreign".to_string()
                ),
            ],
            mine
        );
    }

    /// Outside a capture window the logger must drop everything, so foreign
    /// records cannot accumulate in the buffer over the life of the test
    /// binary and cannot be observed by the NEXT capture.
    #[test]
    fn records_emitted_outside_a_capture_are_discarded() {
        log::warn!(target: "gtk_rustty_surface", "before any capture");
        let cap = start();
        let messages = cap.messages();
        drop(cap);
        assert_eq!(Vec::<CapturedRecord>::new(), messages);
    }
}
