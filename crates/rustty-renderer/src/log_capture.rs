//! TEST-ONLY capture logger for the task-14c `LOG`-row restorations.
//!
//! `rustty-renderer` sits below `rustty-vt-c`/`rustty-core` in R4's acyclic
//! layering (DECISION-logging.md §6r IMPORTANT 5, call shape 3), so its restored
//! `log::{warn,info,error,debug}!` sites reach whichever process-wide `log::Log`
//! backend the running BINARY installs (`rustty-vt-c::sys::Logger` or
//! `rustty-core::logging::AppLogger`). Neither backend exists in
//! `cargo test -p rustty-renderer`'s own test binary — this crate depends on
//! neither crate that owns one — so proving a restored site actually emits
//! requires this crate's OWN minimal `log::Log` implementation, installed for the
//! test binary only (`#[cfg(test)]`, never compiled into the production library).
//!
//! This is the same mechanism `rustty-vt`'s `log_capture` uses (one process-wide
//! logger behind a `Once`, a mutex-guarded buffer, and a serialization lock so
//! two capturing tests cannot interleave), with ONE deliberate difference,
//! recorded here so a reviewer does not read it as an accidental divergence:
//!
//! **Records are tagged with the emitting thread and [`Capture::messages`]
//! returns only this thread's.** `log::set_logger` is process-wide, so the buffer
//! sees records from EVERY thread in the test binary, including the
//! non-capturing tests `cargo test` runs concurrently. The serialization lock
//! only orders tests that call [`start`]; it cannot stop a foreign test from
//! pushing a record into an open capture window. Filtering by thread makes the
//! capture observe exactly the emissions the capturing test caused, which is what
//! lets its assertions stay strict — full record vector, level, target, and exact
//! message text — instead of being loosened to tolerate foreign noise.
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use std::thread::ThreadId;

/// One captured record: `(level, scope/target, formatted message)`.
pub(crate) type CapturedRecord = (log::Level, String, String);

/// A captured record plus the thread that emitted it (see the module docs).
type TaggedRecord = (ThreadId, CapturedRecord);

static INSTALL: Once = Once::new();
static BUFFER: OnceLock<Mutex<Vec<TaggedRecord>>> = OnceLock::new();
/// Serializes access to the shared buffer across concurrently-running tests —
/// held for the lifetime of the returned guard, so a test's trigger/assert/drain
/// sequence is atomic with respect to every other test using this capture logger.
static TEST_SERIAL: Mutex<()> = Mutex::new(());

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        buffer().lock().unwrap_or_else(|p| p.into_inner()).push((
            std::thread::current().id(),
            (
                record.level(),
                record.target().to_string(),
                record.args().to_string(),
            ),
        ));
    }

    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

fn buffer() -> &'static Mutex<Vec<TaggedRecord>> {
    BUFFER.get_or_init(|| Mutex::new(Vec::new()))
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

/// RAII guard returned by [`start`]: holds the serialization lock and drops this
/// thread's records on drop, so a test's captured records never leak into the
/// next test that acquires the guard.
pub(crate) struct Capture {
    _serial: MutexGuard<'static, ()>,
}

impl Capture {
    /// Every record emitted **on this thread** since this guard was created, in
    /// order. Foreign threads' records are excluded — see the module docs.
    pub(crate) fn messages(&self) -> Vec<CapturedRecord> {
        let me = std::thread::current().id();
        buffer()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(tid, _)| *tid == me)
            .map(|(_, rec)| rec.clone())
            .collect()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let me = std::thread::current().id();
        buffer()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(tid, _)| *tid != me);
    }
}

/// Installs the capture logger (once per process) and takes the serialization
/// lock, returning a guard with this thread's records cleared. Call at the top of
/// every test that exercises a restored `log::` call site.
pub(crate) fn start() -> Capture {
    install();
    let serial = TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Clear any records left by a prior test on this thread in case that test
    // panicked before its own `Capture` guard could `Drop::drop`.
    let me = std::thread::current().id();
    buffer()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .retain(|(tid, _)| *tid != me);
    Capture { _serial: serial }
}
