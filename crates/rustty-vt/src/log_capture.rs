//! TEST-ONLY capture logger for the task-14b `LOG`-row restorations.
//!
//! `rustty-vt` sits below `rustty-vt-c`/`rustty-core` in R4's acyclic layering
//! (DECISION-logging.md §6r IMPORTANT 5, call shape 3), so its restored
//! `log::{warn,info,error,debug}!` sites reach whichever process-wide `log::Log`
//! backend the running BINARY installs (`rustty-vt-c::sys::Logger` or
//! `rustty-core::logging::AppLogger`). Neither backend exists in `cargo test -p
//! rustty-vt`'s own test binary — this crate has no dependency on either crate
//! that owns one — so proving a restored site actually emits requires this
//! crate's OWN minimal `log::Log` implementation, installed for the test binary
//! only (`#[cfg(test)]`, never compiled into the production library).
//!
//! `log::set_logger` succeeds **once per process** (`log`'s own documented
//! contract). `cargo test` runs every `#[test]` fn in this crate in ONE process
//! (parallel by default), so if each test installed its own logger, only the
//! first call would win and every other test's install would silently return
//! `Err` and observe nothing — the exact hazard the pattern-fixing rounds in
//! DECISION-logging.md (§6r CRITICAL 1) found and fixed for the two production
//! backends. Avoided here the same way: install exactly ONE logger for the
//! whole test binary (`std::sync::Once`), with a mutex-guarded buffer that
//! individual tests drain — and serialize any test that reads the buffer via a
//! second, buffer-scoped lock so concurrent `cargo test` threads don't
//! interleave each other's captured records.
use std::sync::{Mutex, MutexGuard, Once, OnceLock};

/// One captured record: `(level, scope/target, formatted message)`.
pub(crate) type CapturedRecord = (log::Level, String, String);

static INSTALL: Once = Once::new();
static BUFFER: OnceLock<Mutex<Vec<CapturedRecord>>> = OnceLock::new();
/// Serializes access to the shared buffer across concurrently-running tests —
/// held for the lifetime of the returned guard, so a test's install-log/
/// trigger/assert/drain sequence is atomic with respect to every other test
/// using this capture logger.
static TEST_SERIAL: Mutex<()> = Mutex::new(());

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        buffer().lock().unwrap().push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }

    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

fn buffer() -> &'static Mutex<Vec<CapturedRecord>> {
    BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

/// Installs the capture logger for the test binary (idempotent — the `Once`
/// guard means only the very first call in the process actually registers it;
/// every later call is a no-op, matching the pattern the two production
/// backends already use).
fn install() {
    INSTALL.call_once(|| {
        log::set_logger(&LOGGER).expect("no other logger installed in the rustty-vt test binary");
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// RAII guard returned by [`start`]: holds the serialization lock and clears
/// the buffer on drop, so a test's captured records never leak into the next
/// test that acquires the guard.
pub(crate) struct Capture {
    _serial: MutexGuard<'static, ()>,
}

impl Capture {
    /// Returns every record captured since this guard was created.
    pub(crate) fn messages(&self) -> Vec<CapturedRecord> {
        buffer().lock().unwrap().clone()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        buffer().lock().unwrap().clear();
    }
}

/// Installs the capture logger (once per process) and takes the serialization
/// lock, returning a guard with a freshly-cleared buffer. Call at the top of
/// every test that exercises a restored `log::` call site.
pub(crate) fn start() -> Capture {
    install();
    let serial = TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Clear any records left by a prior test in case that test panicked before
    // its own `Capture` guard could `Drop::drop` and clear the buffer.
    buffer().lock().unwrap().clear();
    Capture { _serial: serial }
}
